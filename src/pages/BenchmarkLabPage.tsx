import { useEffect, useMemo, useRef, useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useRegistryStore } from "../stores/registryStore";
import { useBenchmarkStore } from "../stores/benchmarkStore";
import {
  exportBenchmarkRun,
  hasProviderToken,
  importBenchmarkDataset,
  saveProviderToken,
  type BenchmarkCase,
  type BenchmarkDataset,
  type BenchmarkJudgeConfig,
  type BenchmarkModality,
  type BenchmarkModel,
  type BenchmarkRunItem,
  type BenchmarkTarget,
  type BenchmarkVariant,
  type ManualReview,
} from "../lib/tauri";
import type { UniversalCapability } from "../lib/types";

type TargetDraft = {
  providerId: string;
  modelId: string;
};

type AnalysisPoint = {
  key: string;
  label: string;
  providerId: string;
  modelId: string;
  variantName: string;
  runId: string;
  runName: string;
  createdAt: string;
  avgScore: number | null;
  avgLatencyMs: number | null;
  avgCostUsd: number | null;
  avgContextUsedPercent: number | null;
  itemCount: number;
};

type StabilityStatus = "stable" | "warn" | "critical" | "unknown";

type MetricStatus = {
  status: StabilityStatus;
  delta: number | null;
  baseline: number | null;
  latest: number | null;
};

const BENCHMARK_TOKEN_KEY = "api-key";

type MethodologyEntry = {
  id: string;
  name: string;
  category: "Deterministic" | "Model judge" | "Human review";
  description: string;
  rubric: string;
  example: string;
};

const METHODOLOGY_ENTRIES: MethodologyEntry[] = [
  {
    id: "json_keys",
    name: "JSON keys present",
    category: "Deterministic",
    description: "Verifies the model output is JSON and contains every key listed in the assertion.",
    rubric: "Pass when JSON.parse succeeds and every required key is present at the top level.",
    example: "Required keys: [\"score\", \"rationale\"] → output { score: 95, rationale: \"…\" } passes.",
  },
  {
    id: "regex_match",
    name: "Regex match",
    category: "Deterministic",
    description: "Tests whether the response matches a configured regular expression.",
    rubric: "Pass when the regex finds at least one match in the output text.",
    example: "Pattern /^\\d{3}-\\d{4}$/ → output \"212-5510\" passes.",
  },
  {
    id: "contains_substring",
    name: "Contains substring",
    category: "Deterministic",
    description: "Checks for required tokens that must appear verbatim in the response.",
    rubric: "Pass when every required substring is found (case-sensitive unless overridden).",
    example: "Required: [\"BEGIN\", \"END\"] → output \"BEGIN…END\" passes.",
  },
  {
    id: "llm_judge",
    name: "Model-as-judge rubric",
    category: "Model judge",
    description: "Runs a configurable rubric against a separate scoring model (default: mock or gpt-4o-mini).",
    rubric: "Judge must return strict JSON { score: 0–100, rationale: string }. Non-JSON returns are surfaced as errors.",
    example: "Rubric: \"Score for correctness, usefulness, and instruction following.\" → judge returns { score: 84, rationale: \"…\" }.",
  },
  {
    id: "manual_review",
    name: "Manual review",
    category: "Human review",
    description: "Stores a 1-5 star rating, a preferred-output flag, and free-form notes alongside each item.",
    rubric: "Aggregated into stability scoring as rating × 20 (so 5 stars maps to 100).",
    example: "Reviewer rates 4 stars → contributes 80 to the quality score for that case.",
  },
];

function slug(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

function summarizeCapability(capability: UniversalCapability): string {
  if (capability.type === "rule") {
    return capability.content;
  }
  if (capability.type === "skill") {
    return capability.files.map((file) => `# ${file.path}\n${file.content}`).join("\n\n");
  }
  if (capability.type === "custom") {
    return capability.description;
  }
  return capability.description;
}

function buildCasesFromText(value: string, modality: BenchmarkModality): BenchmarkCase[] {
  return value
    .split(/\n\s*\n/)
    .map((chunk) => chunk.trim())
    .filter(Boolean)
    .map((chunk, index) => ({
      id: `case-${index + 1}`,
      name: `Case ${index + 1}`,
      modality,
      input: chunk,
      reference_output: null,
      assertions: [],
      tags: [],
    }));
}

function formatMetric(value: number | null | undefined, suffix = ""): string {
  if (value === null || value === undefined) {
    return "—";
  }
  return `${value}${suffix}`;
}

function supportedModels(models: BenchmarkModel[], providerId: string, modality: BenchmarkModality): BenchmarkModel[] {
  return models.filter((model) => model.provider_id === providerId && model.modality === modality);
}

function defaultTargetForModality(modality: BenchmarkModality): TargetDraft {
  return modality === "image"
    ? { providerId: "mock", modelId: "mock-image" }
    : { providerId: "mock", modelId: "mock-fast" };
}

function average(values: number[]): number | null {
  if (values.length === 0) {
    return null;
  }
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

function standardDeviation(values: number[]): number {
  if (values.length <= 1) {
    return 0;
  }
  const mean = average(values) ?? 0;
  const variance = values.reduce((sum, value) => sum + ((value - mean) ** 2), 0) / values.length;
  return Math.sqrt(variance);
}

function scoreForItem(item: BenchmarkRunItem): number | null {
  if (typeof item.judge_score?.score === "number") {
    return item.judge_score.score;
  }
  if (item.deterministic_scores.length > 0) {
    const passed = item.deterministic_scores.filter((score) => score.passed).length;
    return (passed / item.deterministic_scores.length) * 100;
  }
  if (typeof item.manual_review.rating === "number") {
    return item.manual_review.rating * 20;
  }
  return null;
}

function formatDelta(value: number | null | undefined, suffix = ""): string {
  if (value === null || value === undefined) {
    return "—";
  }
  const rounded = Math.abs(value) >= 10 ? value.toFixed(1) : value.toFixed(2);
  return `${value > 0 ? "+" : ""}${rounded}${suffix}`;
}

function classifyMetric(
  latest: number | null,
  baselineValues: number[],
  direction: "higher_better" | "lower_better",
): MetricStatus {
  if (latest === null || baselineValues.length === 0) {
    return {
      status: "unknown",
      delta: null,
      baseline: baselineValues.length > 0 ? average(baselineValues) : null,
      latest,
    };
  }

  const baseline = average(baselineValues);
  if (baseline === null) {
    return { status: "unknown", delta: null, baseline: null, latest };
  }

  const stddev = standardDeviation(baselineValues);
  const delta = latest - baseline;
  const badDelta = direction === "higher_better" ? -delta : delta;
  const warnThreshold = Math.max(direction === "higher_better" ? 5 : 0.05, stddev * 1.5);
  const criticalThreshold = Math.max(direction === "higher_better" ? 10 : 0.15, stddev * 2.5);

  if (badDelta >= criticalThreshold) {
    return { status: "critical", delta, baseline, latest };
  }
  if (badDelta >= warnThreshold) {
    return { status: "warn", delta, baseline, latest };
  }
  return { status: "stable", delta, baseline, latest };
}

function statusTone(status: StabilityStatus): string {
  switch (status) {
    case "critical":
      return "bg-red-500/15 text-red-300 border-red-500/30";
    case "warn":
      return "bg-amber-500/15 text-amber-300 border-amber-500/30";
    case "stable":
      return "bg-emerald-500/15 text-emerald-300 border-emerald-500/30";
    default:
      return "bg-white/5 text-text-muted border-[#2a2b36]";
  }
}

function statusRank(status: StabilityStatus): number {
  switch (status) {
    case "critical":
      return 3;
    case "warn":
      return 2;
    case "stable":
      return 1;
    default:
      return 0;
  }
}

export function BenchmarkLabPage() {
  const {
    providers,
    models,
    datasets,
    references,
    runs,
    runDetails,
    currentRun,
    loading,
    running,
    error,
    bootstrap,
    hydrateRunDetails,
    loadRun,
    runSuite,
    saveDataset,
    saveManualReview,
    clearError,
  } = useBenchmarkStore();
  const capabilities = useRegistryStore((state) => state.capabilities);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [activeTab, setActiveTab] = useState<
    "runner" | "stability" | "regressions" | "compare" | "methodology" | "references"
  >("stability");
  const [compareSelection, setCompareSelection] = useState<string[]>([]);
  const [comparePreset, setComparePreset] = useState<
    "head_to_head" | "latest_vs_previous" | "all_variants" | null
  >(null);
  const [runName, setRunName] = useState("Benchmark Run");
  const [modality, setModality] = useState<BenchmarkModality>("text");
  const [datasetId, setDatasetId] = useState<string>("");
  const [selectedDomain, setSelectedDomain] = useState("all");
  const [casesText, setCasesText] = useState("");
  const [baselineSystem, setBaselineSystem] = useState("You are a precise benchmark assistant.");
  const [baselinePrefix, setBaselinePrefix] = useState("");
  const [baselineSuffix, setBaselineSuffix] = useState("");
  const [enableChallenger, setEnableChallenger] = useState(false);
  const [challengerName, setChallengerName] = useState("Capability Variant");
  const [challengerSystem, setChallengerSystem] = useState("You are a precise benchmark assistant.");
  const [challengerPrefix, setChallengerPrefix] = useState("");
  const [challengerSuffix, setChallengerSuffix] = useState("");
  const [selectedCapabilityIds, setSelectedCapabilityIds] = useState<string[]>([]);
  const [targets, setTargets] = useState<TargetDraft[]>([defaultTargetForModality("text")]);
  const [judgeEnabled, setJudgeEnabled] = useState(false);
  const [judgeProvider, setJudgeProvider] = useState("mock");
  const [judgeModel, setJudgeModel] = useState("mock-fast");
  const [judgeRubric, setJudgeRubric] = useState("Score the response from 0 to 100 for correctness, usefulness, and instruction following. Return strict JSON with keys score and rationale.");
  const [providerKeyInputs, setProviderKeyInputs] = useState<Record<string, string>>({});
  const [providerKeyState, setProviderKeyState] = useState<Record<string, boolean>>({});

  useEffect(() => {
    bootstrap();
  }, [bootstrap]);

  useEffect(() => {
    const refreshKeyState = async () => {
      const nextState: Record<string, boolean> = {};
      for (const provider of providers) {
        if (provider.auth_type === "api-key") {
          nextState[provider.id] = await hasProviderToken(`benchmark-${provider.id}`, BENCHMARK_TOKEN_KEY);
        }
      }
      setProviderKeyState(nextState);
    };
    if (providers.length > 0) {
      void refreshKeyState();
    }
  }, [providers]);

  useEffect(() => {
    setTargets([defaultTargetForModality(modality)]);
    if (modality === "image") {
      setEnableChallenger(false);
      setJudgeEnabled(false);
    }
  }, [modality]);

  useEffect(() => {
    const tabsNeedingHistory: typeof activeTab[] = ["stability", "regressions", "compare"];
    if (tabsNeedingHistory.includes(activeTab) && runs.length > 0) {
      void hydrateRunDetails();
    }
  }, [activeTab, hydrateRunDetails, runs]);

  const promptCapabilities = useMemo(
    () =>
      capabilities.filter((capability) =>
        capability.type === "rule" || capability.type === "skill" || capability.type === "custom"
      ),
    [capabilities],
  );

  const selectedCapabilities = useMemo(
    () => promptCapabilities.filter((capability) => selectedCapabilityIds.includes(capability.id)),
    [promptCapabilities, selectedCapabilityIds],
  );

  const challengerContext = useMemo(() => {
    return selectedCapabilities
      .map((capability) => `## ${capability.name}\n${summarizeCapability(capability)}`)
      .join("\n\n");
  }, [selectedCapabilities]);

  const caseList = useMemo(() => buildCasesFromText(casesText, modality), [casesText, modality]);

  const domainOptions = useMemo(() => {
    const values = new Set<string>();
    for (const dataset of datasets) {
      for (const tag of dataset.tags) {
        if (!["seeded", "user", "text", "image"].includes(tag)) {
          values.add(tag);
        }
      }
    }
    for (const reference of references) {
      values.add(reference.category.toLowerCase());
    }
    return ["all", ...Array.from(values).sort()];
  }, [datasets, references]);

  const filteredDatasets = useMemo(() => {
    return datasets.filter((dataset) => {
      if (dataset.modality !== modality) {
        return false;
      }
      if (selectedDomain === "all") {
        return true;
      }
      return dataset.tags.includes(selectedDomain);
    });
  }, [datasets, modality, selectedDomain]);

  const filteredReferences = useMemo(() => {
    if (selectedDomain === "all") {
      return references;
    }
    return references.filter((reference) => reference.category.toLowerCase() === selectedDomain);
  }, [references, selectedDomain]);

  const historicalRuns = useMemo(
    () =>
      Object.values(runDetails)
        .slice()
        .sort((left, right) => Date.parse(left.created_at) - Date.parse(right.created_at)),
    [runDetails],
  );

  const analysisSeries = useMemo(() => {
    const grouped = new Map<string, AnalysisPoint[]>();

    for (const run of historicalRuns) {
      const bucket = new Map<string, BenchmarkRunItem[]>();
      for (const item of run.items) {
        if (item.status !== "completed") {
          continue;
        }
        const key = `${item.provider_id}::${item.model_id}::${item.variant_name}`;
        const items = bucket.get(key) ?? [];
        items.push(item);
        bucket.set(key, items);
      }

      for (const [key, items] of bucket.entries()) {
        const scoreValues = items.map(scoreForItem).filter((value): value is number => value !== null);
        const latencyValues = items
          .map((item) => item.latency_ms)
          .filter((value): value is number => typeof value === "number");
        const costValues = items
          .map((item) => item.estimated_cost_usd)
          .filter((value): value is number => typeof value === "number");
        const contextValues = items
          .map((item) => item.context_used_percent)
          .filter((value): value is number => typeof value === "number");
        const sample = items[0];
        const point: AnalysisPoint = {
          key,
          label: `${sample.provider_id} / ${sample.model_id} / ${sample.variant_name}`,
          providerId: sample.provider_id,
          modelId: sample.model_id,
          variantName: sample.variant_name,
          runId: run.id,
          runName: run.name,
          createdAt: run.created_at,
          avgScore: average(scoreValues),
          avgLatencyMs: average(latencyValues),
          avgCostUsd: average(costValues),
          avgContextUsedPercent: average(contextValues),
          itemCount: items.length,
        };
        const series = grouped.get(key) ?? [];
        series.push(point);
        grouped.set(key, series);
      }
    }

    return Array.from(grouped.values())
      .filter((series) => series.length > 0)
      .sort((left, right) => left[0].label.localeCompare(right[0].label));
  }, [historicalRuns]);

  const stabilityCards = useMemo(() => {
    return analysisSeries
      .map((series) => {
        const latest = series[series.length - 1];
        const previous = series.slice(0, -1);
        const scoreStatus = classifyMetric(
          latest.avgScore,
          previous.map((point) => point.avgScore).filter((value): value is number => value !== null),
          "higher_better",
        );
        const latencyStatus = classifyMetric(
          latest.avgLatencyMs,
          previous.map((point) => point.avgLatencyMs).filter((value): value is number => value !== null),
          "lower_better",
        );
        const costStatus = classifyMetric(
          latest.avgCostUsd,
          previous.map((point) => point.avgCostUsd).filter((value): value is number => value !== null),
          "lower_better",
        );
        const contextStatus = classifyMetric(
          latest.avgContextUsedPercent,
          previous.map((point) => point.avgContextUsedPercent).filter((value): value is number => value !== null),
          "lower_better",
        );
        const overallStatus = [scoreStatus.status, latencyStatus.status, costStatus.status, contextStatus.status]
          .sort((left, right) => statusRank(right) - statusRank(left))[0] ?? "unknown";

        return {
          key: latest.key,
          label: latest.label,
          latest,
          previousCount: previous.length,
          scoreStatus,
          latencyStatus,
          costStatus,
          contextStatus,
          overallStatus,
          recentScores: series.slice(-6).map((point) => point.avgScore),
        };
      })
      .sort((left, right) => statusRank(right.overallStatus) - statusRank(left.overallStatus) || left.label.localeCompare(right.label));
  }, [analysisSeries]);

  const regressionCards = useMemo(() => {
    return stabilityCards
      .flatMap((card) => {
        const metricEntries = [
          { metric: "Quality score", details: card.scoreStatus, suffix: "" },
          { metric: "Latency", details: card.latencyStatus, suffix: " ms" },
          { metric: "Cost", details: card.costStatus, suffix: " USD" },
          { metric: "Context used", details: card.contextStatus, suffix: "%" },
        ];
        return metricEntries
          .filter((entry) => entry.details.status === "warn" || entry.details.status === "critical")
          .map((entry) => ({
            key: `${card.key}-${entry.metric}`,
            label: card.label,
            metric: entry.metric,
            suffix: entry.suffix,
            ...entry.details,
          }));
      })
      .sort((left, right) => {
        const statusDifference = statusRank(right.status) - statusRank(left.status);
        if (statusDifference !== 0) {
          return statusDifference;
        }
        return Math.abs(right.delta ?? 0) - Math.abs(left.delta ?? 0);
      });
  }, [stabilityCards]);

  const freshness = useMemo(() => {
    if (runs.length === 0) {
      return { state: "empty" as const, lastRun: null as string | null, ageHours: null as number | null };
    }
    const latest = runs.reduce((acc, run) => (Date.parse(run.created_at) > Date.parse(acc.created_at) ? run : acc));
    const ageMs = Date.now() - Date.parse(latest.created_at);
    const ageHours = ageMs / 3_600_000;
    const state = ageHours > 168 ? "stale" : ageHours > 24 ? "warming" : "fresh";
    return { state, lastRun: latest.created_at, ageHours };
  }, [runs]);

  const summaryCards = useMemo(() => {
    if (stabilityCards.length === 0) {
      return { bestScore: null, biggestRegression: null, mostExpensive: null };
    }
    const bestScore = stabilityCards
      .filter((card) => card.latest.avgScore != null)
      .sort((left, right) => (right.latest.avgScore ?? 0) - (left.latest.avgScore ?? 0))[0] ?? null;

    const biggestRegression =
      regressionCards.length > 0
        ? regressionCards.reduce((acc, item) => (Math.abs(item.delta ?? 0) > Math.abs(acc.delta ?? 0) ? item : acc))
        : null;

    const mostExpensive = stabilityCards
      .filter((card) => card.latest.avgCostUsd != null)
      .sort((left, right) => (right.latest.avgCostUsd ?? 0) - (left.latest.avgCostUsd ?? 0))[0] ?? null;

    return { bestScore, biggestRegression, mostExpensive };
  }, [regressionCards, stabilityCards]);

  const compareSeries = useMemo(() => {
    return analysisSeries.map((series) => {
      const latest = series[series.length - 1];
      const previous = series.length > 1 ? series[series.length - 2] : null;
      const baseline = series[0];
      const scoreDelta = latest.avgScore != null && previous?.avgScore != null ? latest.avgScore - previous.avgScore : null;
      const peak = series.reduce<AnalysisPoint | null>((acc, point) => {
        if (point.avgScore == null) return acc;
        if (!acc || (acc.avgScore ?? -Infinity) < point.avgScore) return point;
        return acc;
      }, null);
      return { key: latest.key, label: latest.label, latest, previous, baseline, scoreDelta, peak };
    });
  }, [analysisSeries]);

  const compareSelected = useMemo(
    () => compareSeries.filter((entry) => compareSelection.includes(entry.key)),
    [compareSeries, compareSelection],
  );

  const caseWinners = useMemo(() => {
    if (compareSelected.length === 0) {
      return [];
    }
    const byCase = new Map<string, { caseName: string; scores: Map<string, number | null> }>();
    for (const entry of compareSelected) {
      const run = runDetails[entry.latest.runId];
      if (!run) continue;
      for (const item of run.items) {
        if (
          item.provider_id !== entry.latest.providerId ||
          item.model_id !== entry.latest.modelId ||
          item.variant_name !== entry.latest.variantName
        ) {
          continue;
        }
        const bucket = byCase.get(item.case_id) ?? { caseName: item.case_name, scores: new Map() };
        bucket.scores.set(entry.key, scoreForItem(item));
        byCase.set(item.case_id, bucket);
      }
    }
    return Array.from(byCase.entries()).map(([caseId, bucket]) => {
      let winnerKey: string | null = null;
      let winnerScore = -Infinity;
      for (const [key, score] of bucket.scores) {
        if (score != null && score > winnerScore) {
          winnerScore = score;
          winnerKey = key;
        }
      }
      return { caseId, caseName: bucket.caseName, scores: bucket.scores, winnerKey };
    });
  }, [compareSelected, runDetails]);

  const compareHeadline = useMemo(() => {
    if (compareSelected.length < 2) {
      return { leading: null, biggestMove: null, peakInRange: null };
    }
    const leading = compareSelected
      .filter((entry) => entry.latest.avgScore != null)
      .sort((left, right) => (right.latest.avgScore ?? 0) - (left.latest.avgScore ?? 0))[0] ?? null;
    const biggestMove = compareSelected
      .filter((entry) => entry.scoreDelta != null)
      .sort((left, right) => Math.abs(right.scoreDelta ?? 0) - Math.abs(left.scoreDelta ?? 0))[0] ?? null;
    const peakInRange = compareSelected
      .filter((entry) => entry.peak?.avgScore != null)
      .sort((left, right) => (right.peak?.avgScore ?? 0) - (left.peak?.avgScore ?? 0))[0] ?? null;
    return { leading, biggestMove, peakInRange };
  }, [compareSelected]);

  useEffect(() => {
    if (comparePreset == null) {
      return;
    }
    const allKeys = compareSeries.map((entry) => entry.key);
    if (allKeys.length === 0) {
      return;
    }
    if (comparePreset === "head_to_head") {
      setCompareSelection(allKeys.slice(0, 2));
    } else if (comparePreset === "latest_vs_previous") {
      const withHistory = compareSeries.filter((entry) => entry.previous != null);
      setCompareSelection(withHistory.length > 0 ? [withHistory[0].key] : allKeys.slice(0, 1));
    } else if (comparePreset === "all_variants") {
      setCompareSelection(allKeys);
    }
  }, [comparePreset, compareSeries]);

  const availableJudgeModels = useMemo(
    () => models.filter((model) => model.supports_judge && model.modality === "text" && model.provider_id === judgeProvider),
    [judgeProvider, models],
  );

  useEffect(() => {
    if (availableJudgeModels.length > 0 && !availableJudgeModels.some((model) => model.id === judgeModel)) {
      setJudgeModel(availableJudgeModels[0].id);
    }
  }, [availableJudgeModels, judgeModel]);

  const syncDataset = (selectedId: string) => {
    if (!selectedId) {
      setDatasetId("");
      return;
    }
    const dataset = datasets.find((entry) => entry.id === selectedId);
    if (!dataset) {
      return;
    }
    setModality(dataset.modality);
    setCasesText(dataset.cases.map((item) => item.input).join("\n\n"));
    setDatasetId(dataset.id);
  };

  const handleTargetChange = (index: number, field: keyof TargetDraft, value: string) => {
    setTargets((current) =>
      current.map((target, targetIndex) => {
        if (targetIndex !== index) {
          return target;
        }
        if (field === "providerId") {
          const providerModels = supportedModels(models, value, modality);
          return {
            providerId: value,
            modelId: providerModels[0]?.id ?? "",
          };
        }
        return { ...target, [field]: value };
      }),
    );
  };

  const addTarget = () => {
    setTargets((current) => [...current, defaultTargetForModality(modality)]);
  };

  const removeTarget = (index: number) => {
    setTargets((current) => current.filter((_, targetIndex) => targetIndex !== index));
  };

  const toggleCapability = (capabilityId: string) => {
    setSelectedCapabilityIds((current) =>
      current.includes(capabilityId)
        ? current.filter((id) => id !== capabilityId)
        : [...current, capabilityId],
    );
  };

  const handleSaveProviderKey = async (providerId: string) => {
    const value = providerKeyInputs[providerId]?.trim();
    if (!value) {
      return;
    }
    await saveProviderToken(`benchmark-${providerId}`, BENCHMARK_TOKEN_KEY, value);
    setProviderKeyState((current) => ({ ...current, [providerId]: true }));
    setProviderKeyInputs((current) => ({ ...current, [providerId]: "" }));
  };

  const handleSaveDataset = async () => {
    if (caseList.length === 0) {
      return;
    }
    const now = new Date().toISOString();
    const payload: BenchmarkDataset = {
      id: datasetId || "",
      name: runName.trim() || "Saved Dataset",
      modality,
      description: `Saved from Benchmark Lab on ${new Date().toLocaleString()}`,
      cases: caseList,
      tags: ["user"],
      created_at: now,
      updated_at: now,
    };
    const saved = await saveDataset(payload);
    if (saved) {
      setDatasetId(saved.id);
    }
  };

  const handleImportDataset = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    if (!file) {
      return;
    }
    const text = await file.text();
    const imported = await importBenchmarkDataset(text);
    setDatasetId(imported.id);
    setModality(imported.modality);
    setCasesText(imported.cases.map((item) => item.input).join("\n\n"));
    await bootstrap();
  };

  const handleRun = async () => {
    clearError();
    const normalizedTargets: BenchmarkTarget[] = targets
      .filter((target) => target.providerId && target.modelId)
      .map((target) => ({
        provider_id: target.providerId,
        model_id: target.modelId,
        modality,
        temperature: modality === "text" ? 0.2 : null,
        max_output_tokens: modality === "text" ? 1400 : null,
        image_size: modality === "image" ? "1:1" : null,
        image_quality: modality === "image" ? "medium" : null,
      }));

    const variants: BenchmarkVariant[] = [
      {
        id: "baseline",
        name: "Baseline",
        system_prompt: baselineSystem || null,
        prompt_prefix: baselinePrefix || null,
        prompt_suffix: baselineSuffix || null,
        capability_context: null,
        capability_labels: [],
      },
    ];

    if (enableChallenger && modality === "text") {
      variants.push({
        id: "challenger",
        name: challengerName || "Capability Variant",
        system_prompt: challengerSystem || null,
        prompt_prefix: challengerPrefix || null,
        prompt_suffix: challengerSuffix || null,
        capability_context: challengerContext || null,
        capability_labels: selectedCapabilities.map((capability) => capability.name),
      });
    }

    const judge: BenchmarkJudgeConfig | null =
      judgeEnabled && modality === "text"
        ? {
            enabled: true,
            provider_id: judgeProvider,
            model_id: judgeModel,
            rubric: judgeRubric,
          }
        : null;

    await runSuite({
      name: runName,
      modality,
      dataset_name: datasetId || null,
      cases: caseList,
      variants,
      targets: normalizedTargets,
      judge,
    });
  };

  const handleExportRun = async () => {
    if (!currentRun) {
      return;
    }
    const path = await saveDialog({
      defaultPath: `${slug(currentRun.name || "benchmark-run")}.zip`,
      filters: [{ name: "ZIP", extensions: ["zip"] }],
    });
    if (path) {
      await exportBenchmarkRun(currentRun.id, path);
    }
  };

  const handleManualReviewChange = (item: BenchmarkRunItem, updates: Partial<ManualReview>) => {
    const nextReview: ManualReview = {
      rating: updates.rating ?? item.manual_review.rating ?? null,
      preferred: updates.preferred ?? item.manual_review.preferred ?? null,
      notes: updates.notes ?? item.manual_review.notes ?? null,
    };
    void saveManualReview(currentRun!.id, item.item_id, nextReview);
  };

  return (
    <div className="h-full overflow-y-auto" data-testid="benchmark-page">
      <div className="max-w-7xl mx-auto px-6 py-6">
        <div className="flex items-start justify-between gap-4 mb-6">
          <div>
            <h1 className="text-xl font-semibold text-text-primary">Benchmark Lab</h1>
            <p className="text-sm text-text-muted mt-1">
              Compare prompts, capability variants, providers, and models with local run history and exportable artifacts.
            </p>
          </div>
          <div className="flex items-center gap-2 flex-wrap justify-end">
            {(
              [
                ["stability", "Stability"],
                ["regressions", "Regressions"],
                ["compare", "Compare"],
                ["runner", "Runner"],
                ["methodology", "Methodology"],
                ["references", "References"],
              ] as const
            ).map(([id, label]) => (
              <button
                key={id}
                onClick={() => setActiveTab(id)}
                data-testid={`benchmark-tab-${id}`}
                className={`px-3 py-1.5 rounded-lg text-xs font-medium ${activeTab === id ? "bg-accent-blue text-white" : "bg-[#1a1b23] text-text-secondary"}`}
              >
                {label}
              </button>
            ))}
          </div>
        </div>

        {error && (
          <div className="mb-4 rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-300">
            {error}
          </div>
        )}

        <div
          data-testid="benchmark-status-banner"
          className={`mb-5 flex flex-wrap items-center justify-between gap-3 rounded-xl border px-4 py-3 ${
            running
              ? "border-accent-blue/40 bg-accent-blue/10"
              : freshness.state === "stale"
                ? "border-amber-500/30 bg-amber-500/10"
                : freshness.state === "empty"
                  ? "border-[#2a2b36] bg-[#13141a]"
                  : "border-emerald-500/30 bg-emerald-500/5"
          }`}
        >
          <div className="flex items-center gap-3 text-xs">
            <span
              className={`inline-flex h-2.5 w-2.5 rounded-full ${
                running
                  ? "animate-pulse bg-accent-blue"
                  : freshness.state === "stale"
                    ? "bg-amber-400"
                    : freshness.state === "empty"
                      ? "bg-text-muted"
                      : "bg-emerald-400"
              }`}
            />
            <span className="font-medium text-text-primary">
              {running
                ? "Benchmark run in flight…"
                : freshness.state === "empty"
                  ? "No benchmark runs yet"
                  : freshness.state === "stale"
                    ? `Signals are stale — last refresh ${Math.floor((freshness.ageHours ?? 0) / 24)} days ago`
                    : `Last run ${new Date(freshness.lastRun ?? "").toLocaleString()}`}
            </span>
            <span className="text-text-muted">
              {runs.length} runs • {Object.keys(runDetails).length} hydrated
            </span>
          </div>
          <div className="flex items-center gap-2 text-[11px] text-text-muted">
            <span>{stabilityCards.length} model/variant series</span>
            <span className="text-[#2a2b36]">•</span>
            <span className={regressionCards.length > 0 ? "text-amber-300" : "text-text-muted"}>
              {regressionCards.length} active regressions
            </span>
          </div>
        </div>

        <div className="mb-5 flex flex-wrap items-center gap-2">
          <span className="text-xs text-text-muted">Domain</span>
          {domainOptions.map((domain) => (
            <button
              key={domain}
              onClick={() => setSelectedDomain(domain)}
              className={`rounded-full border px-3 py-1 text-xs ${selectedDomain === domain ? "border-accent-blue bg-accent-blue/15 text-accent-blue" : "border-[#2a2b36] bg-[#13141a] text-text-secondary"}`}
            >
              {domain === "all" ? "All" : domain}
            </button>
          ))}
        </div>

        {activeTab === "stability" ? (
          <div className="space-y-4" data-testid="benchmark-stability">
            {stabilityCards.length === 0 ? (
              <div className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-6 text-sm">
                <p className="text-text-primary font-medium">No benchmark history yet.</p>
                <p className="text-text-muted mt-1">
                  Run at least two benchmarks against the same model/variant to unlock stability bands and drift signals.
                </p>
                <p className="text-text-muted mt-3 text-xs">
                  AgentHarbor stores every prompt, parameter, and raw response locally — open the Runner tab to start.
                </p>
              </div>
            ) : (
              <>
                <div
                  data-testid="benchmark-summary-cards"
                  className="grid grid-cols-1 md:grid-cols-3 gap-3 mb-2"
                >
                  <div className="rounded-xl border border-emerald-500/20 bg-[#13141a] p-4">
                    <p className="text-[10px] uppercase tracking-wider text-text-muted">Leading</p>
                    {summaryCards.bestScore ? (
                      <>
                        <p className="mt-1 text-base font-semibold text-text-primary">{summaryCards.bestScore.label}</p>
                        <p className="text-[11px] text-emerald-300 mt-1">
                          Score {formatMetric(summaryCards.bestScore.latest.avgScore)}
                        </p>
                      </>
                    ) : (
                      <p className="mt-1 text-sm text-text-muted">Not enough scored runs yet</p>
                    )}
                  </div>
                  <div className="rounded-xl border border-amber-500/20 bg-[#13141a] p-4">
                    <p className="text-[10px] uppercase tracking-wider text-text-muted">Biggest move</p>
                    {summaryCards.biggestRegression ? (
                      <>
                        <p className="mt-1 text-base font-semibold text-text-primary">
                          {summaryCards.biggestRegression.label}
                        </p>
                        <p className="text-[11px] text-amber-300 mt-1">
                          {summaryCards.biggestRegression.metric} {formatDelta(summaryCards.biggestRegression.delta, summaryCards.biggestRegression.suffix)}
                        </p>
                      </>
                    ) : (
                      <p className="mt-1 text-sm text-text-muted">No regressions in range</p>
                    )}
                  </div>
                  <div className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-4">
                    <p className="text-[10px] uppercase tracking-wider text-text-muted">Most expensive</p>
                    {summaryCards.mostExpensive ? (
                      <>
                        <p className="mt-1 text-base font-semibold text-text-primary">{summaryCards.mostExpensive.label}</p>
                        <p className="text-[11px] text-text-secondary mt-1">
                          ${(summaryCards.mostExpensive.latest.avgCostUsd ?? 0).toFixed(5)} avg / item
                        </p>
                      </>
                    ) : (
                      <p className="mt-1 text-sm text-text-muted">No cost data yet</p>
                    )}
                  </div>
                </div>
              </>
            )}
            {stabilityCards.length > 0 && (
              <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">
                {stabilityCards.map((card) => (
                  <div key={card.key} className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                    <div className="flex items-start justify-between gap-3 mb-4">
                      <div>
                        <h2 className="text-base font-semibold text-text-primary">{card.label}</h2>
                        <p className="text-xs text-text-muted mt-1">
                          Latest run: {card.latest.runName} • {new Date(card.latest.createdAt).toLocaleString()} • {card.previousCount} historical comparison runs
                        </p>
                      </div>
                      <span className={`rounded-full border px-2.5 py-1 text-[10px] uppercase tracking-wide ${statusTone(card.overallStatus)}`}>
                        {card.overallStatus}
                      </span>
                    </div>

                    <div className="grid grid-cols-2 gap-3 text-sm mb-4">
                      <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-3">
                        <p className="text-[11px] text-text-muted mb-1">Quality score</p>
                        <p className="text-text-primary font-medium">{formatMetric(card.latest.avgScore)}</p>
                        <p className={`text-[11px] mt-1 ${card.scoreStatus.status === "critical" ? "text-red-300" : card.scoreStatus.status === "warn" ? "text-amber-300" : "text-text-muted"}`}>
                          {formatDelta(card.scoreStatus.delta)}
                        </p>
                      </div>
                      <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-3">
                        <p className="text-[11px] text-text-muted mb-1">Latency</p>
                        <p className="text-text-primary font-medium">{formatMetric(card.latest.avgLatencyMs, " ms")}</p>
                        <p className={`text-[11px] mt-1 ${card.latencyStatus.status === "critical" ? "text-red-300" : card.latencyStatus.status === "warn" ? "text-amber-300" : "text-text-muted"}`}>
                          {formatDelta(card.latencyStatus.delta, " ms")}
                        </p>
                      </div>
                      <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-3">
                        <p className="text-[11px] text-text-muted mb-1">Estimated cost</p>
                        <p className="text-text-primary font-medium">{card.latest.avgCostUsd != null ? `$${card.latest.avgCostUsd.toFixed(5)}` : "—"}</p>
                        <p className={`text-[11px] mt-1 ${card.costStatus.status === "critical" ? "text-red-300" : card.costStatus.status === "warn" ? "text-amber-300" : "text-text-muted"}`}>
                          {formatDelta(card.costStatus.delta)}
                        </p>
                      </div>
                      <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-3">
                        <p className="text-[11px] text-text-muted mb-1">Context used</p>
                        <p className="text-text-primary font-medium">{card.latest.avgContextUsedPercent != null ? `${card.latest.avgContextUsedPercent.toFixed(2)}%` : "—"}</p>
                        <p className={`text-[11px] mt-1 ${card.contextStatus.status === "critical" ? "text-red-300" : card.contextStatus.status === "warn" ? "text-amber-300" : "text-text-muted"}`}>
                          {formatDelta(card.contextStatus.delta, "%")}
                        </p>
                      </div>
                    </div>

                    <div>
                      <p className="text-[11px] text-text-muted mb-2">Recent quality history</p>
                      <div className="flex items-end gap-1 h-14">
                        {card.recentScores.map((score, index) => (
                          <div
                            key={`${card.key}-score-${index}`}
                            className="flex-1 rounded-t bg-accent-blue/60"
                            style={{ height: `${Math.max(10, Math.min(100, score ?? 10))}%` }}
                            title={score != null ? score.toFixed(1) : "No score"}
                          />
                        ))}
                      </div>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        ) : activeTab === "regressions" ? (
          <div className="space-y-4" data-testid="benchmark-regressions">
            {regressionCards.length === 0 ? (
              <div className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-6 text-sm text-text-muted">
                No active regressions detected in the local run history for the current benchmark series.
              </div>
            ) : (
              regressionCards.map((card) => (
                <div key={card.key} className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                  <div className="flex items-start justify-between gap-3">
                    <div>
                      <h2 className="text-base font-semibold text-text-primary">{card.label}</h2>
                      <p className="text-sm text-text-secondary mt-1">{card.metric}</p>
                    </div>
                    <span className={`rounded-full border px-2.5 py-1 text-[10px] uppercase tracking-wide ${statusTone(card.status)}`}>
                      {card.status}
                    </span>
                  </div>
                  <div className="mt-4 grid grid-cols-3 gap-3 text-sm">
                    <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-3">
                      <p className="text-[11px] text-text-muted mb-1">Latest</p>
                      <p className="text-text-primary">{formatMetric(card.latest, card.suffix)}</p>
                    </div>
                    <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-3">
                      <p className="text-[11px] text-text-muted mb-1">Baseline</p>
                      <p className="text-text-primary">{formatMetric(card.baseline, card.suffix)}</p>
                    </div>
                    <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-3">
                      <p className="text-[11px] text-text-muted mb-1">Delta</p>
                      <p className={card.status === "critical" ? "text-red-300" : "text-amber-300"}>{formatDelta(card.delta, card.suffix)}</p>
                    </div>
                  </div>
                </div>
              ))
            )}
          </div>
        ) : activeTab === "compare" ? (
          <div className="space-y-4" data-testid="benchmark-compare">
            <div className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
              <div className="flex items-center justify-between mb-3">
                <h2 className="text-sm font-semibold text-text-primary">Pick a preset to compare in one click</h2>
                <span className="text-[11px] text-text-muted">{compareSelected.length} selected of {compareSeries.length}</span>
              </div>
              <div className="flex flex-wrap gap-2 mb-4">
                {(
                  [
                    ["head_to_head", "Head to head"],
                    ["latest_vs_previous", "Latest vs previous"],
                    ["all_variants", "Every variant"],
                  ] as const
                ).map(([id, label]) => (
                  <button
                    key={id}
                    data-testid={`compare-preset-${id}`}
                    onClick={() => setComparePreset(id)}
                    disabled={compareSeries.length === 0}
                    className={`rounded-lg border px-3 py-1.5 text-xs ${
                      comparePreset === id
                        ? "border-accent-blue bg-accent-blue/15 text-accent-blue"
                        : "border-[#2a2b36] bg-[#0e0f13] text-text-secondary hover:bg-[#1a1b23]"
                    } disabled:opacity-40`}
                  >
                    {label}
                  </button>
                ))}
                <button
                  onClick={() => {
                    setComparePreset(null);
                    setCompareSelection([]);
                  }}
                  className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] px-3 py-1.5 text-xs text-text-secondary hover:bg-[#1a1b23]"
                >
                  Clear
                </button>
              </div>

              {compareSeries.length === 0 ? (
                <p className="text-sm text-text-muted">
                  Compare needs at least one model/variant history. Run something in the Runner tab first.
                </p>
              ) : (
                <div className="grid grid-cols-1 md:grid-cols-2 gap-2 max-h-60 overflow-y-auto">
                  {compareSeries.map((entry) => {
                    const checked = compareSelection.includes(entry.key);
                    return (
                      <label
                        key={entry.key}
                        className={`flex items-start gap-2 rounded-lg border px-3 py-2 text-xs cursor-pointer ${
                          checked ? "border-accent-blue bg-accent-blue/10" : "border-[#2a2b36] bg-[#0e0f13]"
                        }`}
                      >
                        <input
                          type="checkbox"
                          checked={checked}
                          onChange={(event) => {
                            setComparePreset(null);
                            setCompareSelection((current) =>
                              event.target.checked ? [...current, entry.key] : current.filter((id) => id !== entry.key),
                            );
                          }}
                          className="mt-0.5"
                        />
                        <span className="flex-1">
                          <span className="block text-text-primary font-medium">{entry.label}</span>
                          <span className="block text-text-muted">
                            Score {formatMetric(entry.latest.avgScore)} • ${(entry.latest.avgCostUsd ?? 0).toFixed(5)}/item
                          </span>
                        </span>
                      </label>
                    );
                  })}
                </div>
              )}
            </div>

            {compareSelected.length >= 1 && (
              <div className="grid grid-cols-1 md:grid-cols-3 gap-3">
                <div className="rounded-xl border border-emerald-500/20 bg-[#13141a] p-4">
                  <p className="text-[10px] uppercase tracking-wider text-text-muted">Leading</p>
                  {compareHeadline.leading ? (
                    <>
                      <p className="mt-1 text-base font-semibold text-text-primary">{compareHeadline.leading.label}</p>
                      <p className="text-[11px] text-emerald-300 mt-1">Score {formatMetric(compareHeadline.leading.latest.avgScore)}</p>
                    </>
                  ) : (
                    <p className="mt-1 text-sm text-text-muted">Pick two or more variants</p>
                  )}
                </div>
                <div className="rounded-xl border border-amber-500/20 bg-[#13141a] p-4">
                  <p className="text-[10px] uppercase tracking-wider text-text-muted">Biggest move</p>
                  {compareHeadline.biggestMove ? (
                    <>
                      <p className="mt-1 text-base font-semibold text-text-primary">{compareHeadline.biggestMove.label}</p>
                      <p className="text-[11px] text-amber-300 mt-1">Δ {formatDelta(compareHeadline.biggestMove.scoreDelta)}</p>
                    </>
                  ) : (
                    <p className="mt-1 text-sm text-text-muted">Needs two runs per variant</p>
                  )}
                </div>
                <div className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-4">
                  <p className="text-[10px] uppercase tracking-wider text-text-muted">Peak in range</p>
                  {compareHeadline.peakInRange?.peak ? (
                    <>
                      <p className="mt-1 text-base font-semibold text-text-primary">{compareHeadline.peakInRange.label}</p>
                      <p className="text-[11px] text-text-secondary mt-1">
                        Best run {formatMetric(compareHeadline.peakInRange.peak.avgScore)} on {new Date(compareHeadline.peakInRange.peak.createdAt).toLocaleDateString()}
                      </p>
                    </>
                  ) : (
                    <p className="mt-1 text-sm text-text-muted">No scored history yet</p>
                  )}
                </div>
              </div>
            )}

            {compareSelected.length >= 2 && caseWinners.length > 0 && (
              <div className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <div className="flex items-center justify-between mb-3">
                  <h2 className="text-sm font-semibold text-text-primary">Per-case breakdown</h2>
                  <span className="text-[11px] text-text-muted">★ marks the case winner</span>
                </div>
                <div className="overflow-x-auto">
                  <table className="min-w-full text-xs">
                    <thead>
                      <tr className="text-left text-text-muted">
                        <th className="py-2 pr-3 font-medium">Case</th>
                        {compareSelected.map((entry) => (
                          <th key={entry.key} className="py-2 px-3 font-medium text-text-secondary">
                            {entry.label}
                          </th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {caseWinners.map((row) => (
                        <tr key={row.caseId} className="border-t border-[#2a2b36]">
                          <td className="py-2 pr-3 text-text-primary">{row.caseName}</td>
                          {compareSelected.map((entry) => {
                            const score = row.scores.get(entry.key);
                            const isWinner = row.winnerKey === entry.key;
                            return (
                              <td key={entry.key} className={`py-2 px-3 ${isWinner ? "text-emerald-300 font-medium" : "text-text-secondary"}`}>
                                {isWinner && <span className="mr-1">★</span>}
                                {score != null ? score.toFixed(1) : "—"}
                              </td>
                            );
                          })}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            )}
          </div>
        ) : activeTab === "methodology" ? (
          <div className="space-y-4" data-testid="benchmark-methodology">
            <div className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
              <h2 className="text-base font-semibold text-text-primary">How we score every run</h2>
              <p className="mt-2 text-sm text-text-secondary">
                AgentHarbor stores the prompt, parameters, and the raw response for every benchmark item — so any score below comes from data
                you can inspect in the Runner tab.
              </p>
            </div>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              {METHODOLOGY_ENTRIES.map((entry) => (
                <div key={entry.id} className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                  <div className="flex items-center justify-between gap-3 mb-2">
                    <h3 className="text-sm font-semibold text-text-primary">{entry.name}</h3>
                    <span className="rounded-full bg-white/5 px-2 py-1 text-[10px] uppercase tracking-wide text-text-muted">
                      {entry.category}
                    </span>
                  </div>
                  <p className="text-xs text-text-secondary">{entry.description}</p>
                  <div className="mt-3 rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-3">
                    <p className="text-[10px] uppercase tracking-wider text-text-muted mb-1">Scoring rubric</p>
                    <p className="text-xs text-text-secondary">{entry.rubric}</p>
                  </div>
                  <div className="mt-2 rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-3">
                    <p className="text-[10px] uppercase tracking-wider text-text-muted mb-1">Example</p>
                    <p className="text-xs text-text-secondary font-mono">{entry.example}</p>
                  </div>
                </div>
              ))}
            </div>
          </div>
        ) : activeTab === "references" ? (
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {filteredReferences.map((reference) => (
              <div key={reference.id} className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <div className="flex items-center justify-between gap-3 mb-3">
                  <h2 className="text-base font-semibold text-text-primary">{reference.name}</h2>
                  <span className="rounded-full bg-white/5 px-2 py-1 text-[10px] uppercase tracking-wide text-text-muted">
                    {reference.category}
                  </span>
                </div>
                <p className="text-sm text-text-secondary mb-3">{reference.summary}</p>
                <p className="text-xs text-text-muted mb-4">{reference.notes}</p>
                <a href={reference.source_url} target="_blank" rel="noreferrer" className="text-xs text-accent-blue hover:underline">
                  Open source
                </a>
              </div>
            ))}
          </div>
        ) : (
          <div className="grid grid-cols-[minmax(0,1.3fr)_minmax(340px,0.7fr)] gap-6">
            <div className="space-y-5">
              <section className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <div className="flex items-center justify-between mb-4">
                  <h2 className="text-sm font-semibold text-text-primary">Run Setup</h2>
                  <div className="flex items-center gap-2">
                    <button onClick={handleSaveDataset} className="rounded-lg bg-[#1a1b23] px-3 py-1.5 text-xs text-text-primary hover:bg-[#20222b]">
                      Save Dataset
                    </button>
                    <button onClick={() => fileInputRef.current?.click()} className="rounded-lg bg-[#1a1b23] px-3 py-1.5 text-xs text-text-primary hover:bg-[#20222b]">
                      Import Dataset
                    </button>
                  </div>
                </div>

                <input
                  ref={fileInputRef}
                  type="file"
                  accept=".json"
                  className="hidden"
                  data-testid="benchmark-import-input"
                  onChange={handleImportDataset}
                />

                <div className="grid grid-cols-2 gap-4 mb-4">
                  <label className="space-y-1">
                    <span className="text-xs text-text-muted">Run name</span>
                    <input
                      value={runName}
                      onChange={(event) => setRunName(event.target.value)}
                      className="w-full rounded-lg border border-[#2a2b36] bg-[#0e0f13] px-3 py-2 text-sm text-text-primary"
                    />
                  </label>
                  <label className="space-y-1">
                    <span className="text-xs text-text-muted">Modality</span>
                    <select
                      value={modality}
                      onChange={(event) => setModality(event.target.value as BenchmarkModality)}
                      className="w-full rounded-lg border border-[#2a2b36] bg-[#0e0f13] px-3 py-2 text-sm text-text-primary"
                    >
                      <option value="text">Text / Code</option>
                      <option value="image">Image</option>
                    </select>
                  </label>
                  <label className="space-y-1 col-span-2">
                    <span className="text-xs text-text-muted">Load saved dataset</span>
                    <select
                      value={datasetId}
                      onChange={(event) => syncDataset(event.target.value)}
                      className="w-full rounded-lg border border-[#2a2b36] bg-[#0e0f13] px-3 py-2 text-sm text-text-primary"
                    >
                      <option value="">None</option>
                      {filteredDatasets.map((dataset) => (
                          <option key={dataset.id} value={dataset.id}>
                            {dataset.name}
                          </option>
                        ))}
                    </select>
                  </label>
                </div>

                <label className="space-y-1 block">
                  <span className="text-xs text-text-muted">Cases</span>
                  <textarea
                    value={casesText}
                    onChange={(event) => setCasesText(event.target.value)}
                    data-testid="benchmark-cases"
                    className="min-h-[180px] w-full rounded-xl border border-[#2a2b36] bg-[#0e0f13] px-3 py-3 text-sm text-text-primary"
                    placeholder="One case per blank line."
                  />
                </label>
                <p className="mt-2 text-[11px] text-text-muted">
                  Parsed cases: {caseList.length}
                </p>
              </section>

              <section className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <h2 className="text-sm font-semibold text-text-primary mb-4">Credentials</h2>
                <div className="space-y-3">
                  {providers
                    .filter((provider) => provider.auth_type === "api-key")
                    .map((provider) => (
                      <div key={provider.id} className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-3">
                        <div className="flex items-center justify-between mb-2">
                          <div>
                            <p className="text-sm text-text-primary">{provider.name}</p>
                            <p className="text-[11px] text-text-muted">
                              {providerKeyState[provider.id] ? "Key saved" : "Key not saved"}
                            </p>
                          </div>
                          <span className={`h-2.5 w-2.5 rounded-full ${providerKeyState[provider.id] ? "bg-emerald-400" : "bg-amber-400"}`} />
                        </div>
                        <div className="flex items-center gap-2">
                          <input
                            type="password"
                            value={providerKeyInputs[provider.id] ?? ""}
                            onChange={(event) =>
                              setProviderKeyInputs((current) => ({ ...current, [provider.id]: event.target.value }))
                            }
                            placeholder={provider.key_placeholder}
                            className="flex-1 rounded-lg border border-[#2a2b36] bg-[#13141a] px-3 py-2 text-xs text-text-primary"
                          />
                          <button
                            onClick={() => void handleSaveProviderKey(provider.id)}
                            className="rounded-lg bg-accent-blue px-3 py-2 text-xs font-medium text-white"
                          >
                            Save
                          </button>
                        </div>
                      </div>
                    ))}
                </div>
                <p className="mt-3 text-[11px] text-text-muted">
                  Live benchmark keys are stored separately from general analytics credentials under benchmark-scoped provider ids.
                </p>
              </section>

              <section className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <h2 className="text-sm font-semibold text-text-primary mb-4">Variants</h2>
                <div className="grid grid-cols-1 xl:grid-cols-2 gap-4">
                  <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-4 space-y-3">
                    <p className="text-sm font-medium text-text-primary">Baseline</p>
                    <input
                      value={baselineSystem}
                      onChange={(event) => setBaselineSystem(event.target.value)}
                      className="w-full rounded-lg border border-[#2a2b36] bg-[#13141a] px-3 py-2 text-sm text-text-primary"
                      placeholder="System prompt"
                    />
                    <textarea
                      value={baselinePrefix}
                      onChange={(event) => setBaselinePrefix(event.target.value)}
                      className="min-h-[90px] w-full rounded-lg border border-[#2a2b36] bg-[#13141a] px-3 py-2 text-sm text-text-primary"
                      placeholder="Optional prompt prefix"
                    />
                    <textarea
                      value={baselineSuffix}
                      onChange={(event) => setBaselineSuffix(event.target.value)}
                      className="min-h-[70px] w-full rounded-lg border border-[#2a2b36] bg-[#13141a] px-3 py-2 text-sm text-text-primary"
                      placeholder="Optional prompt suffix"
                    />
                  </div>

                  <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-4 space-y-3">
                    <label className="flex items-center gap-2 text-sm font-medium text-text-primary">
                      <input
                        type="checkbox"
                        checked={enableChallenger}
                        disabled={modality === "image"}
                        onChange={(event) => setEnableChallenger(event.target.checked)}
                      />
                      Enable challenger variant
                    </label>
                    <input
                      value={challengerName}
                      onChange={(event) => setChallengerName(event.target.value)}
                      className="w-full rounded-lg border border-[#2a2b36] bg-[#13141a] px-3 py-2 text-sm text-text-primary disabled:opacity-50"
                      placeholder="Variant name"
                      disabled={!enableChallenger || modality === "image"}
                    />
                    <input
                      value={challengerSystem}
                      onChange={(event) => setChallengerSystem(event.target.value)}
                      className="w-full rounded-lg border border-[#2a2b36] bg-[#13141a] px-3 py-2 text-sm text-text-primary disabled:opacity-50"
                      placeholder="System prompt"
                      disabled={!enableChallenger || modality === "image"}
                    />
                    <textarea
                      value={challengerPrefix}
                      onChange={(event) => setChallengerPrefix(event.target.value)}
                      className="min-h-[70px] w-full rounded-lg border border-[#2a2b36] bg-[#13141a] px-3 py-2 text-sm text-text-primary disabled:opacity-50"
                      placeholder="Optional prompt prefix"
                      disabled={!enableChallenger || modality === "image"}
                    />
                    <textarea
                      value={challengerSuffix}
                      onChange={(event) => setChallengerSuffix(event.target.value)}
                      className="min-h-[70px] w-full rounded-lg border border-[#2a2b36] bg-[#13141a] px-3 py-2 text-sm text-text-primary disabled:opacity-50"
                      placeholder="Optional prompt suffix"
                      disabled={!enableChallenger || modality === "image"}
                    />
                    <div className="rounded-lg border border-dashed border-[#2a2b36] p-3">
                      <p className="text-xs text-text-muted mb-2">Prompt-bearing capabilities for challenger</p>
                      <div className="max-h-40 overflow-y-auto space-y-2">
                        {promptCapabilities.map((capability) => (
                          <label key={capability.id} className="flex items-start gap-2 text-xs text-text-secondary">
                            <input
                              type="checkbox"
                              checked={selectedCapabilityIds.includes(capability.id)}
                              disabled={!enableChallenger || modality === "image"}
                              onChange={() => toggleCapability(capability.id)}
                            />
                            <span>
                              <span className="block text-text-primary">{capability.name}</span>
                              <span className="block text-text-muted">{capability.type}</span>
                            </span>
                          </label>
                        ))}
                      </div>
                    </div>
                  </div>
                </div>
              </section>

              <section className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <div className="flex items-center justify-between mb-4">
                  <h2 className="text-sm font-semibold text-text-primary">Targets</h2>
                  <button onClick={addTarget} className="rounded-lg bg-[#1a1b23] px-3 py-1.5 text-xs text-text-primary hover:bg-[#20222b]">
                    Add Target
                  </button>
                </div>
                <div className="space-y-3">
                  {targets.map((target, index) => {
                    const availableProviders = providers.filter((provider) =>
                      provider.supported_modalities.includes(modality),
                    );
                    const modelChoices = supportedModels(models, target.providerId, modality);
                    return (
                      <div key={`${target.providerId}-${index}`} className="grid grid-cols-[1fr_1fr_auto] gap-3">
                        <select
                          value={target.providerId}
                          onChange={(event) => handleTargetChange(index, "providerId", event.target.value)}
                          className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] px-3 py-2 text-sm text-text-primary"
                        >
                          {availableProviders.map((provider) => (
                            <option key={provider.id} value={provider.id}>
                              {provider.name}
                            </option>
                          ))}
                        </select>
                        <select
                          value={target.modelId}
                          onChange={(event) => handleTargetChange(index, "modelId", event.target.value)}
                          className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] px-3 py-2 text-sm text-text-primary"
                        >
                          {modelChoices.map((model) => (
                            <option key={model.id} value={model.id}>
                              {model.display_name}
                            </option>
                          ))}
                        </select>
                        <button
                          onClick={() => removeTarget(index)}
                          disabled={targets.length === 1}
                          className="rounded-lg border border-[#2a2b36] px-3 py-2 text-xs text-text-secondary disabled:opacity-50"
                        >
                          Remove
                        </button>
                      </div>
                    );
                  })}
                </div>
              </section>

              <section className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <h2 className="text-sm font-semibold text-text-primary mb-4">Scoring</h2>
                <label className="flex items-center gap-2 text-sm text-text-primary mb-3">
                  <input
                    type="checkbox"
                    checked={judgeEnabled}
                    disabled={modality === "image"}
                    onChange={(event) => setJudgeEnabled(event.target.checked)}
                  />
                  Enable model-as-judge
                </label>
                <div className="grid grid-cols-2 gap-4 mb-3">
                  <select
                    value={judgeProvider}
                    onChange={(event) => setJudgeProvider(event.target.value)}
                    disabled={!judgeEnabled || modality === "image"}
                    className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] px-3 py-2 text-sm text-text-primary disabled:opacity-50"
                  >
                    {providers
                      .filter((provider) => provider.supported_modalities.includes("text"))
                      .map((provider) => (
                        <option key={provider.id} value={provider.id}>
                          {provider.name}
                        </option>
                      ))}
                  </select>
                  <select
                    value={judgeModel}
                    onChange={(event) => setJudgeModel(event.target.value)}
                    disabled={!judgeEnabled || modality === "image"}
                    className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] px-3 py-2 text-sm text-text-primary disabled:opacity-50"
                  >
                    {availableJudgeModels.map((model) => (
                      <option key={model.id} value={model.id}>
                        {model.display_name}
                      </option>
                    ))}
                  </select>
                </div>
                <textarea
                  value={judgeRubric}
                  onChange={(event) => setJudgeRubric(event.target.value)}
                  disabled={!judgeEnabled || modality === "image"}
                  className="min-h-[90px] w-full rounded-lg border border-[#2a2b36] bg-[#0e0f13] px-3 py-2 text-sm text-text-primary disabled:opacity-50"
                />
              </section>

              <div className="flex items-center gap-3">
                <button
                  onClick={() => void handleRun()}
                  disabled={running || caseList.length === 0}
                  data-testid="benchmark-run"
                  className="rounded-lg bg-accent-blue px-4 py-2 text-sm font-medium text-white disabled:opacity-50"
                >
                  {running ? "Running..." : "Run Benchmark"}
                </button>
                <button
                  onClick={handleExportRun}
                  disabled={!currentRun}
                  className="rounded-lg bg-[#1a1b23] px-4 py-2 text-sm text-text-primary disabled:opacity-50"
                >
                  Export Current Run
                </button>
              </div>
            </div>

            <div className="space-y-5">
              <section className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <div className="flex items-center justify-between mb-4">
                  <h2 className="text-sm font-semibold text-text-primary">Run History</h2>
                  <span className="text-xs text-text-muted">{runs.length} runs</span>
                </div>
                <div className="space-y-2 max-h-[280px] overflow-y-auto">
                  {runs.length === 0 ? (
                    <p className="rounded-lg border border-dashed border-[#2a2b36] bg-[#0e0f13] p-3 text-[11px] text-text-muted">
                      No runs yet. Press <span className="text-text-primary">Run Benchmark</span> to capture the first prompt, params, and raw responses locally.
                    </p>
                  ) : (
                    runs.map((run) => (
                      <button
                        key={run.id}
                        onClick={() => void loadRun(run.id)}
                        className={`w-full rounded-lg border px-3 py-3 text-left ${currentRun?.id === run.id ? "border-accent-blue bg-accent-blue/10" : "border-[#2a2b36] bg-[#0e0f13]"}`}
                      >
                        <div className="flex items-center justify-between gap-3">
                          <span className="text-sm font-medium text-text-primary">{run.name}</span>
                          <span className="text-[10px] uppercase tracking-wide text-text-muted">{run.status}</span>
                        </div>
                        <p className="mt-1 text-[11px] text-text-muted">
                          {run.modality} • {run.item_count} items • {new Date(run.created_at).toLocaleString()}
                        </p>
                      </button>
                    ))
                  )}
                </div>
              </section>

              <section className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <div className="flex items-center justify-between mb-4">
                  <h2 className="text-sm font-semibold text-text-primary">Results</h2>
                  {currentRun && (
                    <span className="text-xs text-text-muted">
                      {currentRun.status} • {currentRun.items.length} items
                    </span>
                  )}
                </div>

                {!currentRun ? (
                  <p className="text-sm text-text-muted">Run a benchmark or load a previous run to inspect outputs.</p>
                ) : (
                  <div className="space-y-4 max-h-[920px] overflow-y-auto" data-testid="benchmark-results">
                    <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-4">
                      <div className="flex items-center justify-between gap-3 mb-3">
                        <p className="text-sm font-medium text-text-primary">Methodology</p>
                        <span className="text-[11px] text-text-muted">
                          {currentRun.cases.length} cases • {currentRun.targets.length} targets • {currentRun.variants.length} variants
                        </span>
                      </div>
                      <div className="grid grid-cols-2 gap-3 text-[11px] text-text-secondary mb-3">
                        <div>Run: {currentRun.name}</div>
                        <div>Status: {currentRun.status}</div>
                        <div>Modality: {currentRun.modality}</div>
                        <div>Dataset: {currentRun.dataset_name || "ad hoc"}</div>
                        <div>Created: {new Date(currentRun.created_at).toLocaleString()}</div>
                        <div>Judge: {currentRun.judge_config?.enabled ? `${currentRun.judge_config.provider_id} / ${currentRun.judge_config.model_id}` : "disabled"}</div>
                      </div>
                      <div className="space-y-2">
                        {currentRun.targets.map((target, index) => (
                          <div key={`${target.provider_id}-${target.model_id}-${index}`} className="rounded-lg border border-[#2a2b36] bg-[#13141a] p-3 text-[11px] text-text-secondary">
                            <p className="text-text-primary font-medium mb-1">{target.provider_id} / {target.model_id}</p>
                            <p>
                              temperature: {formatMetric(target.temperature)} • max output: {formatMetric(target.max_output_tokens)} • image size: {target.image_size || "—"} • image quality: {target.image_quality || "—"}
                            </p>
                          </div>
                        ))}
                        {currentRun.variants.map((variant) => (
                          <details key={variant.id} className="rounded-lg border border-[#2a2b36] bg-[#13141a] p-3">
                            <summary className="cursor-pointer text-[11px] font-medium text-text-primary">
                              Variant: {variant.name}
                            </summary>
                            <div className="mt-2 space-y-2 text-[11px] text-text-secondary">
                              <p>System prompt: {variant.system_prompt || "—"}</p>
                              <p>Prompt prefix: {variant.prompt_prefix || "—"}</p>
                              <p>Prompt suffix: {variant.prompt_suffix || "—"}</p>
                              <p>Capabilities: {variant.capability_labels.length > 0 ? variant.capability_labels.join(", ") : "none"}</p>
                              {variant.capability_context ? (
                                <pre className="max-h-32 overflow-y-auto whitespace-pre-wrap rounded border border-[#2a2b36] bg-[#0e0f13] p-2 text-text-muted">
                                  {variant.capability_context}
                                </pre>
                              ) : null}
                            </div>
                          </details>
                        ))}
                      </div>
                    </div>

                    {currentRun.items.map((item) => (
                      <div key={item.item_id} className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-4">
                        <div className="flex items-start justify-between gap-3 mb-3">
                          <div>
                            <p className="text-sm font-medium text-text-primary">{item.case_name}</p>
                            <p className="text-[11px] text-text-muted">
                              {item.provider_id} / {item.model_id} / {item.variant_name}
                            </p>
                          </div>
                          <span className={`rounded-full px-2 py-1 text-[10px] uppercase tracking-wide ${item.status === "completed" ? "bg-emerald-500/15 text-emerald-300" : "bg-red-500/15 text-red-300"}`}>
                            {item.status}
                          </span>
                        </div>

                        <div className="grid grid-cols-2 gap-2 mb-3 text-[11px] text-text-secondary">
                          <div>Latency: {formatMetric(item.latency_ms, " ms")}</div>
                          <div>Estimated cost: {item.estimated_cost_usd != null ? `$${item.estimated_cost_usd.toFixed(5)}` : "—"}</div>
                          <div>Input tokens: {item.token_counts.input_tokens}</div>
                          <div>Output tokens: {item.token_counts.output_tokens}</div>
                          <div>Context window: {formatMetric(item.context_window)}</div>
                          <div>Context used: {item.context_used_percent != null ? `${item.context_used_percent}%` : "—"}</div>
                        </div>

                        {item.output_text && (
                          <pre className="mb-3 max-h-48 overflow-y-auto rounded-lg border border-[#2a2b36] bg-[#13141a] p-3 text-xs text-text-secondary whitespace-pre-wrap">
                            {item.output_text}
                          </pre>
                        )}

                        {item.artifact_refs.length > 0 && (
                          <div className="mb-3 space-y-2">
                            {item.artifact_refs.map((artifact) => (
                              <div key={artifact.path} className="rounded-lg border border-[#2a2b36] bg-[#13141a] p-3">
                                {artifact.preview_data_url ? (
                                  <img src={artifact.preview_data_url} alt={artifact.label} className="mb-2 max-h-64 w-full rounded-lg object-contain" />
                                ) : null}
                                <p className="text-[11px] text-text-muted break-all">{artifact.path}</p>
                              </div>
                            ))}
                          </div>
                        )}

                        {item.deterministic_scores.length > 0 && (
                          <div className="mb-3 rounded-lg border border-[#2a2b36] bg-[#13141a] p-3">
                            <p className="mb-2 text-xs font-medium text-text-primary">Deterministic checks</p>
                            <div className="space-y-1">
                              {item.deterministic_scores.map((score) => (
                                <div key={`${item.item_id}-${score.kind}`} className="flex items-center justify-between text-[11px]">
                                  <span className="text-text-secondary">{score.kind}</span>
                                  <span className={score.passed ? "text-emerald-300" : "text-red-300"}>
                                    {score.passed ? "Pass" : "Fail"}
                                  </span>
                                </div>
                              ))}
                            </div>
                          </div>
                        )}

                        {item.judge_score && (
                          <div className="mb-3 rounded-lg border border-[#2a2b36] bg-[#13141a] p-3">
                            <p className="mb-1 text-xs font-medium text-text-primary">Judge score</p>
                            <p className="text-[11px] text-text-secondary">
                              {item.judge_score.score != null ? `${item.judge_score.score}/100` : item.judge_score.error || "Unavailable"}
                            </p>
                            {item.judge_score.rationale && (
                              <p className="mt-1 text-[11px] text-text-muted">{item.judge_score.rationale}</p>
                            )}
                          </div>
                        )}

                        {item.error && (
                          <div className="mb-3 rounded-lg border border-red-500/25 bg-red-500/10 px-3 py-2 text-[11px] text-red-300">
                            {item.error}
                          </div>
                        )}

                        <div className="grid grid-cols-[120px_120px_1fr] gap-2">
                          <select
                            value={item.manual_review.rating ?? ""}
                            onChange={(event) =>
                              handleManualReviewChange(item, {
                                rating: event.target.value ? Number(event.target.value) : null,
                              })
                            }
                            className="rounded-lg border border-[#2a2b36] bg-[#13141a] px-3 py-2 text-xs text-text-primary"
                          >
                            <option value="">Rating</option>
                            {[1, 2, 3, 4, 5].map((rating) => (
                              <option key={rating} value={rating}>
                                {rating}/5
                              </option>
                            ))}
                          </select>
                          <label className="flex items-center gap-2 rounded-lg border border-[#2a2b36] bg-[#13141a] px-3 py-2 text-xs text-text-primary">
                            <input
                              type="checkbox"
                              checked={Boolean(item.manual_review.preferred)}
                              onChange={(event) =>
                                handleManualReviewChange(item, {
                                  preferred: event.target.checked,
                                })
                              }
                            />
                            Preferred
                          </label>
                          <input
                            value={item.manual_review.notes ?? ""}
                            onChange={(event) =>
                              handleManualReviewChange(item, {
                                notes: event.target.value,
                              })
                            }
                            placeholder="Manual notes"
                            className="rounded-lg border border-[#2a2b36] bg-[#13141a] px-3 py-2 text-xs text-text-primary"
                          />
                        </div>
                      </div>
                    ))}
                  </div>
                )}
              </section>
            </div>
          </div>
        )}

        {loading && !running ? (
          <div className="mt-4 text-sm text-text-muted">Loading benchmark data…</div>
        ) : null}
      </div>
    </div>
  );
}
