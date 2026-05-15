import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { save as saveDialog } from "@tauri-apps/plugin-dialog";
import { useRegistryStore } from "../stores/registryStore";
import { useBenchmarkStore } from "../stores/benchmarkStore";
import {
  analyzeBenchmarkStrategyRepo,
  exportBenchmarkRun,
  hasProviderToken,
  importBenchmarkDataset,
  saveProviderToken,
  type BenchmarkCase,
  type BenchmarkDataset,
  type BenchmarkJudgeConfig,
  type BenchmarkModality,
  type BenchmarkModel,
  type BenchmarkRunRequest,
  type BenchmarkRunItem,
  type BenchmarkStrategyAnalysis,
  type BenchmarkTarget,
  type BenchmarkVariant,
  type ManualReview,
} from "../lib/tauri";
import type { UniversalCapability } from "../lib/types";

type TargetDraft = {
  providerId: string;
  modelId: string;
};

const BENCHMARK_TOKEN_KEY = "api-key";

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

function sum(values: Array<number | null | undefined>): number {
  return values.reduce<number>((total, value) => total + (value ?? 0), 0);
}

function average(values: Array<number | null | undefined>): number | null {
  const normalized = values.filter((value): value is number => value != null);
  if (normalized.length === 0) {
    return null;
  }
  return normalized.reduce((total, value) => total + value, 0) / normalized.length;
}

function totalTokens(item: BenchmarkRunItem): number {
  return item.token_counts.input_tokens
    + item.token_counts.output_tokens
    + item.token_counts.cache_read_tokens
    + item.token_counts.cache_write_tokens;
}

function formatDelta(value: number | null): string {
  if (value === null || Number.isNaN(value)) {
    return "—";
  }
  const rounded = Math.round(value * 100) / 100;
  return `${rounded > 0 ? "+" : ""}${rounded}`;
}

function formatModelCount(count: number): string {
  return count === 1 ? "1 model" : `${count} models`;
}

function metricWidth(value: number, max: number): string {
  if (!max || max <= 0) {
    return "0%";
  }
  return `${Math.max(10, Math.min(100, (value / max) * 100))}%`;
}

function renderResponseBlocks(text: string) {
  return text.split("\n").map((line, index) => {
    const trimmed = line.trim();
    if (!trimmed) {
      return <div key={`blank-${index}`} className="h-3" />;
    }
    if (trimmed.startsWith("#")) {
      return (
        <p key={`heading-${index}`} className="text-sm font-semibold text-text-primary">
          {trimmed.replace(/^#+\s*/, "")}
        </p>
      );
    }
    if (trimmed.startsWith("- ") || trimmed.startsWith("* ")) {
      return (
        <div key={`bullet-${index}`} className="flex gap-2">
          <span className="mt-1.5 h-1.5 w-1.5 rounded-full bg-accent-blue shrink-0" />
          <p className="text-sm leading-6 text-text-secondary">{trimmed.slice(2)}</p>
        </div>
      );
    }
    return (
      <p key={`line-${index}`} className="text-sm leading-6 text-text-secondary">
        {line}
      </p>
    );
  });
}

function normalizeReferenceTemplate(referenceId: string): { modality: BenchmarkModality; text: string; datasetHint?: string } {
  switch (referenceId) {
    case "swe-bench":
      return {
        modality: "text",
        datasetHint: "seeded/coding-quick-check",
        text: "Given a bug report for a production service, explain the minimal patch, the test plan, and the rollout guardrails in compact bullet points.",
      };
    case "browsecomp":
      return {
        modality: "text",
        text: "Research a difficult technical question using only the most relevant evidence, then return a concise cited answer and a short evidence table.",
      };
    case "gdpval":
      return {
        modality: "text",
        text: "Solve a practical business workflow task with a short answer, a structured output section, and no unnecessary explanation.",
      };
    case "t2i-compbench-plus-plus":
      return {
        modality: "image",
        datasetHint: "seeded/image-prompt-fidelity",
        text: "Create a highly compositional editorial image that follows every visual constraint exactly.",
      };
    default:
      return {
        modality: "text",
        text: "Answer the task with high quality while minimizing prompt overhead and response verbosity.",
      };
  }
}

function supportedModels(models: BenchmarkModel[], providerId: string, modality: BenchmarkModality): BenchmarkModel[] {
  return models.filter((model) => model.provider_id === providerId && model.modality === modality);
}

function defaultTargetForModality(modality: BenchmarkModality): TargetDraft {
  return modality === "image"
    ? { providerId: "mock", modelId: "mock-image" }
    : { providerId: "mock", modelId: "mock-fast" };
}

function targetSelectionKey(providerId: string, modelId: string): string {
  return `${providerId}::${modelId}`;
}

export function BenchmarkLabPage() {
  const {
    providers,
    models,
    datasets,
    references,
    runs,
    currentRun,
    loading,
    running,
    error,
    bootstrap,
    refreshModels,
    loadRun,
    runSuite,
    saveDataset,
    saveManualReview,
    clearError,
  } = useBenchmarkStore();
  const capabilities = useRegistryStore((state) => state.capabilities);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [activeTab, setActiveTab] = useState<"runner" | "references">("runner");
  const [runnerTab, setRunnerTab] = useState<"setup" | "compare" | "responses" | "gallery">("setup");
  const [runName, setRunName] = useState("Benchmark Run");
  const [modality, setModality] = useState<BenchmarkModality>("text");
  const [datasetId, setDatasetId] = useState<string>("");
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
  const [strategyRepoUrl, setStrategyRepoUrl] = useState("");
  const [strategyLoading, setStrategyLoading] = useState(false);
  const [strategyAnalysis, setStrategyAnalysis] = useState<BenchmarkStrategyAnalysis | null>(null);
  const [strategyError, setStrategyError] = useState<string | null>(null);
  const [selectedReferenceId, setSelectedReferenceId] = useState<string>("swe-bench");
  const [selectedReferenceTargets, setSelectedReferenceTargets] = useState<string[]>([]);
  const [collapsedPanels, setCollapsedPanels] = useState<Record<string, boolean>>({
    runSetup: false,
    credentials: false,
    strategy: false,
    variants: false,
    targets: false,
    scoring: false,
    history: false,
    results: false,
  });

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
    if (casesText.trim() || datasets.length === 0) {
      return;
    }
    const seededDataset = datasets.find((dataset) => dataset.modality === modality);
    if (!seededDataset) {
      return;
    }
    setDatasetId(seededDataset.id);
    setCasesText(seededDataset.cases.map((item) => item.input).join("\n\n"));
  }, [casesText, datasets, modality]);

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
    const capabilityContext = selectedCapabilities
      .map((capability) => `## ${capability.name}\n${summarizeCapability(capability)}`)
      .join("\n\n");
    const strategyContext = strategyAnalysis?.extracted_context?.trim() ?? "";
    return [capabilityContext, strategyContext].filter(Boolean).join("\n\n");
  }, [selectedCapabilities, strategyAnalysis]);

  const caseList = useMemo(() => buildCasesFromText(casesText, modality), [casesText, modality]);

  const availableJudgeModels = useMemo(
    () => models.filter((model) => model.supports_judge && model.modality === "text" && model.provider_id === judgeProvider),
    [judgeProvider, models],
  );

  const providerModelCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const model of models) {
      counts[model.provider_id] = (counts[model.provider_id] ?? 0) + 1;
    }
    return counts;
  }, [models]);

  const selectedReference = useMemo(
    () => references.find((reference) => reference.id === selectedReferenceId) ?? references[0] ?? null,
    [references, selectedReferenceId],
  );

  const selectedReferenceTemplate = useMemo(
    () => normalizeReferenceTemplate(selectedReference?.id ?? "swe-bench"),
    [selectedReference],
  );

  const launchableReferenceModels = useMemo(() => {
    const modalityFilter = selectedReferenceTemplate.modality;
    return models.filter((model) => {
      if (model.modality !== modalityFilter) {
        return false;
      }
      if (model.provider_id === "mock") {
        return true;
      }
      return providerKeyState[model.provider_id] === true;
    });
  }, [models, providerKeyState, selectedReferenceTemplate.modality]);

  const comparisonRows = useMemo(() => {
    if (!currentRun || currentRun.variants.length < 2) {
      return [];
    }

    const rows: Array<{
      key: string;
      providerId: string;
      modelId: string;
      baselineVariant: string;
      challengerVariant: string;
      baselineTokens: number;
      challengerTokens: number;
      baselineCost: number | null;
      challengerCost: number | null;
      baselineLatency: number | null;
      challengerLatency: number | null;
      baselineJudge: number | null;
      challengerJudge: number | null;
    }> = [];

    const grouped = new Map<string, BenchmarkRunItem[]>();
    for (const item of currentRun.items) {
      const key = `${item.provider_id}::${item.model_id}`;
      const list = grouped.get(key) ?? [];
      list.push(item);
      grouped.set(key, list);
    }

    for (const [key, items] of grouped.entries()) {
      const byVariant = new Map<string, BenchmarkRunItem[]>();
      for (const item of items) {
        const list = byVariant.get(item.variant_name) ?? [];
        list.push(item);
        byVariant.set(item.variant_name, list);
      }
      const variantNames = Array.from(byVariant.keys());
      if (variantNames.length < 2) {
        continue;
      }
      const baseline = byVariant.get(variantNames[0]) ?? [];
      const challenger = byVariant.get(variantNames[1]) ?? [];
      if (baseline.length === 0 || challenger.length === 0) {
        continue;
      }

      rows.push({
        key,
        providerId: baseline[0].provider_id,
        modelId: baseline[0].model_id,
        baselineVariant: variantNames[0],
        challengerVariant: variantNames[1],
        baselineTokens: sum(baseline.map((item) => totalTokens(item))),
        challengerTokens: sum(challenger.map((item) => totalTokens(item))),
        baselineCost: average(baseline.map((item) => item.estimated_cost_usd)),
        challengerCost: average(challenger.map((item) => item.estimated_cost_usd)),
        baselineLatency: average(baseline.map((item) => item.latency_ms)),
        challengerLatency: average(challenger.map((item) => item.latency_ms)),
        baselineJudge: average(baseline.map((item) => item.judge_score?.score)),
        challengerJudge: average(challenger.map((item) => item.judge_score?.score)),
      });
    }

    return rows;
  }, [currentRun]);

  const runMetricRows = useMemo(() => {
    if (!currentRun) {
      return [];
    }
    return currentRun.items.map((item) => ({
      key: item.item_id,
      label: `${item.provider_id} / ${item.model_id} / ${item.variant_name}`,
      providerId: item.provider_id,
      modelId: item.model_id,
      variantName: item.variant_name,
      tokens: totalTokens(item),
      cost: item.estimated_cost_usd ?? 0,
      latency: item.latency_ms ?? 0,
      judge: item.judge_score?.score ?? 0,
      hasJudge: item.judge_score?.score != null,
    }));
  }, [currentRun]);

  const metricMaxima = useMemo(() => ({
    tokens: Math.max(1, ...runMetricRows.map((row) => row.tokens)),
    cost: Math.max(0.00001, ...runMetricRows.map((row) => row.cost)),
    latency: Math.max(1, ...runMetricRows.map((row) => row.latency)),
    judge: Math.max(1, ...runMetricRows.map((row) => row.judge)),
  }), [runMetricRows]);

  const galleryItems = useMemo(
    () =>
      currentRun?.items.filter((item) => item.artifact_refs.length > 0).map((item) => ({
        item,
        artifacts: item.artifact_refs,
      })) ?? [],
    [currentRun],
  );

  const imageComparisonGroups = useMemo(() => {
    const groups = new Map<string, Array<{ item: BenchmarkRunItem; artifacts: BenchmarkRunItem["artifact_refs"] }>>();
    for (const entry of galleryItems) {
      const key = entry.item.case_name;
      const list = groups.get(key) ?? [];
      list.push(entry);
      groups.set(key, list);
    }
    return Array.from(groups.entries()).map(([caseName, items]) => ({ caseName, items }));
  }, [galleryItems]);

  useEffect(() => {
    if (availableJudgeModels.length > 0 && !availableJudgeModels.some((model) => model.id === judgeModel)) {
      setJudgeModel(availableJudgeModels[0].id);
    }
  }, [availableJudgeModels, judgeModel]);

  useEffect(() => {
    if (!selectedReference && references.length > 0) {
      setSelectedReferenceId(references[0].id);
    }
  }, [references, selectedReference]);

  useEffect(() => {
    const launchKeys = launchableReferenceModels.map((model) => targetSelectionKey(model.provider_id, model.id));
    if (launchKeys.length === 0) {
      setSelectedReferenceTargets([]);
      return;
    }
    setSelectedReferenceTargets((current) => {
      const retained = current.filter((key) => launchKeys.includes(key));
      if (retained.length > 0) {
        return retained;
      }
      return launchKeys.slice(0, Math.min(3, launchKeys.length));
    });
  }, [launchableReferenceModels]);

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

  const togglePanel = (panelId: string) => {
    setCollapsedPanels((current) => ({ ...current, [panelId]: !current[panelId] }));
  };

  const handleSaveProviderKey = async (providerId: string) => {
    const value = providerKeyInputs[providerId]?.trim();
    if (!value) {
      return;
    }
    await saveProviderToken(`benchmark-${providerId}`, BENCHMARK_TOKEN_KEY, value);
    setProviderKeyState((current) => ({ ...current, [providerId]: true }));
    setProviderKeyInputs((current) => ({ ...current, [providerId]: "" }));
    if (providerId === "openai" || providerId === "anthropic") {
      await refreshModels(providerId);
    }
  };

  const handleAnalyzeStrategyRepo = async () => {
    if (!strategyRepoUrl.trim()) {
      return;
    }
    setStrategyLoading(true);
    setStrategyError(null);
    try {
      const analysis = await analyzeBenchmarkStrategyRepo(strategyRepoUrl.trim());
      setStrategyAnalysis(analysis);
      setEnableChallenger(true);
      setChallengerName(`${analysis.repository_full_name} strategy`);
      if (!challengerPrefix.trim()) {
        setChallengerPrefix(
          "Apply the repository-derived strategy context below when composing the answer. Preserve correctness and avoid mentioning the strategy unless the task requires it.",
        );
      }
    } catch (analysisError) {
      setStrategyAnalysis(null);
      setStrategyError(analysisError instanceof Error ? analysisError.message : String(analysisError));
    } finally {
      setStrategyLoading(false);
    }
  };

  const handleUseReferenceBenchmark = (referenceId: string) => {
    const template = normalizeReferenceTemplate(referenceId);
    setActiveTab("runner");
    setRunnerTab("setup");
    setModality(template.modality);
    setCasesText(template.text);
    if (template.datasetHint) {
      setDatasetId(template.datasetHint);
    } else {
      setDatasetId("");
    }
    setRunName(`Benchmark from ${referenceId}`);
  };

  const toggleReferenceTarget = (selectionKey: string) => {
    setSelectedReferenceTargets((current) =>
      current.includes(selectionKey)
        ? current.filter((key) => key !== selectionKey)
        : [...current, selectionKey],
    );
  };

  const buildRunRequest = (
    nextCases: BenchmarkCase[],
    nextModality: BenchmarkModality,
    nextTargets: TargetDraft[],
    nextRunName?: string,
    nextDatasetName?: string | null,
  ): BenchmarkRunRequest | null => {
    const normalizedTargets: BenchmarkTarget[] = nextTargets
      .filter((target) => target.providerId && target.modelId)
      .map((target) => ({
        provider_id: target.providerId,
        model_id: target.modelId,
        modality: nextModality,
        temperature: nextModality === "text" ? 0.2 : null,
        max_output_tokens: nextModality === "text" ? 1400 : null,
        image_size: nextModality === "image" ? "1:1" : null,
        image_quality: nextModality === "image" ? "medium" : null,
      }));

    if (nextCases.length === 0 || normalizedTargets.length === 0) {
      return null;
    }

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

    if (enableChallenger && nextModality === "text") {
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
      judgeEnabled && nextModality === "text"
        ? {
            enabled: true,
            provider_id: judgeProvider,
            model_id: judgeModel,
            rubric: judgeRubric,
          }
        : null;

    return {
      name: nextRunName ?? runName,
      modality: nextModality,
      dataset_name: nextDatasetName === undefined ? datasetId || null : nextDatasetName,
      cases: nextCases,
      variants,
      targets: normalizedTargets,
      judge,
    };
  };

  const handleLaunchReferenceBenchmark = async () => {
    if (!selectedReference) {
      return;
    }

    const template = normalizeReferenceTemplate(selectedReference.id);
    const nextCases = buildCasesFromText(template.text, template.modality);
    const nextTargets: TargetDraft[] = selectedReferenceTargets
      .map((selectionKey) => {
        const [providerId, modelId] = selectionKey.split("::");
        return { providerId, modelId };
      })
      .filter((target) => target.providerId && target.modelId);

    const request = buildRunRequest(
      nextCases,
      template.modality,
      nextTargets,
      `${selectedReference.name} benchmark`,
      template.datasetHint ?? null,
    );

    if (!request) {
      return;
    }

    setRunName(request.name);
    setModality(template.modality);
    setCasesText(template.text);
    setDatasetId(template.datasetHint ?? "");
    setTargets(nextTargets);
    setActiveTab("runner");
    setRunnerTab(template.modality === "image" ? "gallery" : "compare");
    await runSuite(request);
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
    const request = buildRunRequest(caseList, modality, targets);
    if (!request) {
      return;
    }
    await runSuite(request);
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
          <div className="flex items-center gap-2">
            <button
              onClick={() => setActiveTab("runner")}
              data-testid="benchmark-tab-runner"
              className={`px-3 py-1.5 rounded-lg text-xs font-medium ${activeTab === "runner" ? "bg-accent-blue text-white" : "bg-[#1a1b23] text-text-secondary"}`}
            >
              Runner
            </button>
            <button
              onClick={() => setActiveTab("references")}
              data-testid="benchmark-tab-references"
              className={`px-3 py-1.5 rounded-lg text-xs font-medium ${activeTab === "references" ? "bg-accent-blue text-white" : "bg-[#1a1b23] text-text-secondary"}`}
            >
              Reference Benchmarks
            </button>
          </div>
        </div>

        {error && (
          <div className="mb-4 rounded-lg border border-red-500/30 bg-red-500/10 px-4 py-3 text-sm text-red-300">
            {error}
          </div>
        )}

        {activeTab === "references" ? (
          <div className="space-y-5">
            <section className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
              <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h2 className="text-base font-semibold text-text-primary">Quick Launch</h2>
                  <p className="mt-1 max-w-2xl text-sm text-text-secondary">
                    Choose a benchmark family, select multiple live models, and launch a comparison run directly from this page.
                  </p>
                </div>
                <button
                  onClick={() => void handleLaunchReferenceBenchmark()}
                  disabled={selectedReferenceTargets.length === 0 || running}
                  className="rounded-lg bg-accent-blue px-4 py-2 text-sm font-medium text-white disabled:opacity-50"
                >
                  {running ? "Launching..." : "Launch Benchmark"}
                </button>
              </div>

              <div className="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(320px,0.9fr)_minmax(0,1.1fr)]">
                <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-4">
                  <label className="mb-3 block space-y-1">
                    <span className="text-xs text-text-muted">Benchmark family</span>
                    <select
                      value={selectedReferenceId}
                      onChange={(event) => setSelectedReferenceId(event.target.value)}
                      className="w-full rounded-lg border border-[#2a2b36] bg-[#13141a] px-3 py-2 text-sm text-text-primary"
                    >
                      {references.map((reference) => (
                        <option key={reference.id} value={reference.id}>
                          {reference.name}
                        </option>
                      ))}
                    </select>
                  </label>

                  {selectedReference && (
                    <div className="space-y-3">
                      <div>
                        <div className="mb-2 flex items-center gap-2">
                          <span className="rounded-full bg-white/5 px-2 py-1 text-[10px] uppercase tracking-wide text-text-muted">
                            {selectedReference.category}
                          </span>
                          <span className="rounded-full bg-accent-blue/10 px-2 py-1 text-[10px] uppercase tracking-wide text-accent-blue">
                            {selectedReferenceTemplate.modality}
                          </span>
                        </div>
                        <p className="text-sm text-text-secondary">{selectedReference.summary}</p>
                        <p className="mt-2 text-xs text-text-muted">{selectedReference.notes}</p>
                      </div>

                      <div className="rounded-lg border border-dashed border-[#2a2b36] bg-[#13141a] p-3">
                        <p className="mb-2 text-[11px] uppercase tracking-wide text-text-muted">Seed prompt</p>
                        <p className="text-sm leading-6 text-text-secondary">{selectedReferenceTemplate.text}</p>
                      </div>
                    </div>
                  )}
                </div>

                <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-4">
                  <div className="mb-3 flex items-center justify-between gap-3">
                    <div>
                      <p className="text-sm font-medium text-text-primary">Model Selection</p>
                      <p className="text-[11px] text-text-muted">
                        Saved-provider keys unlock live models here. Mock remains available for dry runs.
                      </p>
                    </div>
                    <span className="rounded-full bg-white/5 px-2 py-1 text-[10px] uppercase tracking-wide text-text-muted">
                      {selectedReferenceTargets.length} selected
                    </span>
                  </div>

                  {launchableReferenceModels.length > 0 ? (
                    <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                      {launchableReferenceModels.map((model) => {
                        const selectionKey = targetSelectionKey(model.provider_id, model.id);
                        const selected = selectedReferenceTargets.includes(selectionKey);
                        return (
                          <label
                            key={selectionKey}
                            className={`rounded-lg border p-3 transition-colors ${selected ? "border-accent-blue bg-accent-blue/10" : "border-[#2a2b36] bg-[#13141a]"}`}
                          >
                            <div className="flex items-start gap-3">
                              <input
                                type="checkbox"
                                checked={selected}
                                onChange={() => toggleReferenceTarget(selectionKey)}
                                className="mt-1"
                              />
                              <div className="min-w-0">
                                <p className="truncate text-sm font-medium text-text-primary">
                                  {model.display_name}
                                </p>
                                <p className="text-[11px] text-text-muted">
                                  {model.provider_id} • {model.context_window ? `${model.context_window.toLocaleString()} ctx` : "context n/a"}
                                </p>
                              </div>
                            </div>
                          </label>
                        );
                      })}
                    </div>
                  ) : (
                    <p className="text-sm text-text-muted">
                      No launchable models yet for this modality. Save a provider key and refresh models first.
                    </p>
                  )}
                </div>
              </div>
            </section>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              {references.map((reference) => (
                <div key={reference.id} className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                  <div className="flex items-center justify-between gap-3 mb-3">
                    <h2 className="text-base font-semibold text-text-primary">{reference.name}</h2>
                    <span className="rounded-full bg-white/5 px-2 py-1 text-[10px] uppercase tracking-wide text-text-muted">
                      {reference.category}
                    </span>
                  </div>
                  <p className="text-sm text-text-secondary mb-3">{reference.summary}</p>
                  <p className="text-xs text-text-muted mb-4">{reference.notes}</p>
                  <div className="flex flex-wrap items-center gap-2">
                    <button
                      onClick={() => handleUseReferenceBenchmark(reference.id)}
                      className="rounded-lg bg-accent-blue px-3 py-2 text-xs font-medium text-white"
                    >
                      Load In Runner
                    </button>
                    <button
                      onClick={() => setSelectedReferenceId(reference.id)}
                      className="rounded-lg border border-[#2a2b36] px-3 py-2 text-xs text-text-primary"
                    >
                      Select For Launch
                    </button>
                    <a href={reference.source_url} target="_blank" rel="noreferrer" className="text-xs text-accent-blue hover:underline">
                      Open source
                    </a>
                  </div>
                </div>
              ))}
            </div>
          </div>
        ) : (
          <div className="space-y-4">
            <div className="flex flex-wrap items-center gap-2">
              {[
                ["setup", "Setup"],
                ["compare", "Compare"],
                ["responses", "Responses"],
                ["gallery", "Gallery"],
              ].map(([id, label]) => (
                <button
                  key={id}
                  onClick={() => setRunnerTab(id as "setup" | "compare" | "responses" | "gallery")}
                  className={`rounded-full px-3 py-1.5 text-xs font-medium transition-colors ${runnerTab === id ? "bg-accent-blue text-white" : "bg-[#1a1b23] text-text-secondary hover:bg-[#20222b]"}`}
                >
                  {label}
                </button>
              ))}
            </div>

          <div className="grid grid-cols-1 xl:grid-cols-[minmax(0,1.08fr)_minmax(320px,0.92fr)] gap-6">
            <div className="space-y-5">
              <section className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <div className="flex items-center justify-between mb-4">
                  <div className="flex items-center gap-3">
                    <button
                      onClick={() => togglePanel("runSetup")}
                      className="rounded-full border border-[#2a2b36] px-2 py-1 text-[10px] uppercase tracking-wide text-text-muted"
                    >
                      {collapsedPanels.runSetup ? "Expand" : "Collapse"}
                    </button>
                    <h2 className="text-sm font-semibold text-text-primary">Run Setup</h2>
                  </div>
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

                {!collapsedPanels.runSetup && (
                  <Fragment>
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
                      {datasets
                        .filter((dataset) => dataset.modality === modality)
                        .map((dataset) => (
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
                  </Fragment>
                )}
              </section>

              <section className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <div className="mb-4 flex items-center justify-between gap-3">
                  <div className="flex items-center gap-3">
                    <button
                      onClick={() => togglePanel("credentials")}
                      className="rounded-full border border-[#2a2b36] px-2 py-1 text-[10px] uppercase tracking-wide text-text-muted"
                    >
                      {collapsedPanels.credentials ? "Expand" : "Collapse"}
                    </button>
                    <h2 className="text-sm font-semibold text-text-primary">Credentials</h2>
                  </div>
                </div>
                {!collapsedPanels.credentials && (
                  <Fragment>
                <p className="mb-3 text-xs text-text-muted">
                  Live benchmarking uses these benchmark-specific provider keys. Save a key, then switch your targets away from <span className="text-text-primary">Mock</span>.
                </p>
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
                          <div className="flex items-center gap-2">
                            <span className="text-[11px] text-text-muted">{formatModelCount(providerModelCounts[provider.id] ?? 0)}</span>
                            <span className={`h-2.5 w-2.5 rounded-full ${providerKeyState[provider.id] ? "bg-emerald-400" : "bg-amber-400"}`} />
                          </div>
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
                          {(provider.id === "openai" || provider.id === "anthropic") && (
                            <button
                              onClick={() => void refreshModels(provider.id)}
                              disabled={!providerKeyState[provider.id]}
                              className="rounded-lg border border-[#2a2b36] px-3 py-2 text-xs text-text-primary disabled:opacity-50"
                            >
                              Refresh Models
                            </button>
                          )}
                        </div>
                      </div>
                    ))}
                </div>
                  </Fragment>
                )}
              </section>

              <section className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <div className="flex items-center justify-between gap-3 mb-4">
                  <div className="flex items-center gap-3">
                    <button
                      onClick={() => togglePanel("strategy")}
                      className="rounded-full border border-[#2a2b36] px-2 py-1 text-[10px] uppercase tracking-wide text-text-muted"
                    >
                      {collapsedPanels.strategy ? "Expand" : "Collapse"}
                    </button>
                    <div>
                      <h2 className="text-sm font-semibold text-text-primary">GitHub Strategy Comparator</h2>
                      <p className="text-xs text-text-muted mt-1">
                        Paste a GitHub repository that claims context reduction, token savings, prompt compression, or retrieval optimization. Benchmark Lab will compare your baseline against a strategy-augmented variant built from that repo’s docs.
                      </p>
                    </div>
                  </div>
                </div>

                {!collapsedPanels.strategy && (
                  <Fragment>
                    <div className="flex items-center gap-2 mb-3">
                      <input
                        value={strategyRepoUrl}
                        onChange={(event) => setStrategyRepoUrl(event.target.value)}
                        placeholder="https://github.com/owner/repo"
                        className="flex-1 rounded-lg border border-[#2a2b36] bg-[#0e0f13] px-3 py-2 text-sm text-text-primary"
                      />
                      <button
                        onClick={() => void handleAnalyzeStrategyRepo()}
                        disabled={strategyLoading || !strategyRepoUrl.trim()}
                        className="rounded-lg bg-accent-blue px-3 py-2 text-xs font-medium text-white disabled:opacity-50"
                      >
                        {strategyLoading ? "Analyzing..." : "Fetch Strategy"}
                      </button>
                    </div>

                    {strategyError && (
                      <div className="mb-3 rounded-lg border border-red-500/25 bg-red-500/10 px-3 py-2 text-xs text-red-300">
                        {strategyError}
                      </div>
                    )}

                    {strategyAnalysis && (
                      <div className="space-y-4">
                        <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-4">
                          <div className="flex items-center justify-between gap-3 mb-2">
                            <div>
                              <p className="text-sm font-medium text-text-primary">{strategyAnalysis.repository_full_name}</p>
                              <p className="text-[11px] text-text-muted">Fetched from {strategyAnalysis.default_branch} on {new Date(strategyAnalysis.fetched_at).toLocaleString()}</p>
                            </div>
                            <span className="rounded-full bg-accent-blue/15 px-2 py-1 text-[10px] uppercase tracking-wide text-accent-blue">
                              Challenger ready
                            </span>
                          </div>
                          <p className="text-sm text-text-secondary">{strategyAnalysis.summary}</p>
                        </div>

                        {strategyAnalysis.claims.length > 0 && (
                          <div className="grid grid-cols-1 xl:grid-cols-2 gap-3">
                            {strategyAnalysis.claims.map((claim) => (
                              <div key={`${claim.source_path}-${claim.evidence}`} className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-3">
                                <div className="flex items-center justify-between gap-3 mb-2">
                                  <p className="text-xs font-medium text-text-primary">{claim.headline}</p>
                                  {claim.metric && (
                                    <span className="rounded-full bg-emerald-500/10 px-2 py-1 text-[10px] font-medium text-emerald-300">
                                      {claim.metric}
                                    </span>
                                  )}
                                </div>
                                <p className="text-xs text-text-secondary">{claim.evidence}</p>
                                {claim.source_url && (
                                  <a href={claim.source_url} target="_blank" rel="noreferrer" className="mt-2 inline-block text-[11px] text-accent-blue hover:underline">
                                    {claim.source_path ?? "Open source file"}
                                  </a>
                                )}
                              </div>
                            ))}
                          </div>
                        )}

                        {strategyAnalysis.documents.length > 0 && (
                          <div className="rounded-lg border border-dashed border-[#2a2b36] bg-[#0e0f13] p-4">
                            <p className="text-xs font-medium text-text-primary mb-2">Strategy context that will be injected into the challenger</p>
                            <div className="space-y-3 max-h-64 overflow-y-auto">
                              {strategyAnalysis.documents.map((document) => (
                                <div key={document.path}>
                                  <p className="text-[11px] uppercase tracking-wide text-text-muted mb-1">{document.path}</p>
                                  <pre className="whitespace-pre-wrap rounded-lg border border-[#2a2b36] bg-[#13141a] p-3 text-[11px] text-text-secondary">
                                    {document.excerpt}
                                  </pre>
                                </div>
                              ))}
                            </div>
                          </div>
                        )}
                      </div>
                    )}
                  </Fragment>
                )}
              </section>

              <section className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <div className="mb-4 flex items-center justify-between gap-3">
                  <div className="flex items-center gap-3">
                    <button
                      onClick={() => togglePanel("variants")}
                      className="rounded-full border border-[#2a2b36] px-2 py-1 text-[10px] uppercase tracking-wide text-text-muted"
                    >
                      {collapsedPanels.variants ? "Expand" : "Collapse"}
                    </button>
                    <h2 className="text-sm font-semibold text-text-primary">Variants</h2>
                  </div>
                </div>
                {!collapsedPanels.variants && (
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
                      <p className="text-xs text-text-muted mb-2">Prompt-bearing capabilities for challenger. These stack with any GitHub strategy context fetched above.</p>
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
                )}
              </section>

              <section className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <div className="flex items-center justify-between mb-4">
                  <div className="flex items-center gap-3">
                    <button
                      onClick={() => togglePanel("targets")}
                      className="rounded-full border border-[#2a2b36] px-2 py-1 text-[10px] uppercase tracking-wide text-text-muted"
                    >
                      {collapsedPanels.targets ? "Expand" : "Collapse"}
                    </button>
                    <h2 className="text-sm font-semibold text-text-primary">Targets</h2>
                  </div>
                  <button onClick={addTarget} className="rounded-lg bg-[#1a1b23] px-3 py-1.5 text-xs text-text-primary hover:bg-[#20222b]">
                    Add Target
                  </button>
                </div>
                {!collapsedPanels.targets && (
                <Fragment>
                  <p className="mb-3 text-xs text-text-muted">
                    For live runs, choose a provider with a saved key and select one of the fetched models. Leaving the target on <span className="text-text-primary">Mock</span> keeps the run synthetic.
                  </p>
                  <div className="space-y-3">
                  {targets.map((target, index) => {
                    const availableProviders = providers.filter((provider) =>
                      provider.supported_modalities.includes(modality),
                    );
                    const modelChoices = supportedModels(models, target.providerId, modality);
                    return (
                      <div key={`${target.providerId}-${index}`} className="grid grid-cols-1 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] gap-3">
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
                </Fragment>
                )}
              </section>

              <section className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <div className="mb-4 flex items-center justify-between gap-3">
                  <div className="flex items-center gap-3">
                    <button
                      onClick={() => togglePanel("scoring")}
                      className="rounded-full border border-[#2a2b36] px-2 py-1 text-[10px] uppercase tracking-wide text-text-muted"
                    >
                      {collapsedPanels.scoring ? "Expand" : "Collapse"}
                    </button>
                    <h2 className="text-sm font-semibold text-text-primary">Scoring</h2>
                  </div>
                </div>
                {!collapsedPanels.scoring && (
                <Fragment>
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
                </Fragment>
                )}
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
                  <div className="flex items-center gap-3">
                    <button
                      onClick={() => togglePanel("history")}
                      className="rounded-full border border-[#2a2b36] px-2 py-1 text-[10px] uppercase tracking-wide text-text-muted"
                    >
                      {collapsedPanels.history ? "Expand" : "Collapse"}
                    </button>
                    <h2 className="text-sm font-semibold text-text-primary">Run History</h2>
                  </div>
                  <span className="text-xs text-text-muted">{runs.length} runs</span>
                </div>
                {!collapsedPanels.history && (
                <div className="space-y-2 max-h-[280px] overflow-y-auto">
                  {runs.map((run) => (
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
                  ))}
                </div>
                )}
              </section>

              <section className="rounded-xl border border-[#2a2b36] bg-[#13141a] p-5">
                <div className="flex items-center justify-between mb-4">
                  <div className="flex items-center gap-3">
                    <button
                      onClick={() => togglePanel("results")}
                      className="rounded-full border border-[#2a2b36] px-2 py-1 text-[10px] uppercase tracking-wide text-text-muted"
                    >
                      {collapsedPanels.results ? "Expand" : "Collapse"}
                    </button>
                    <h2 className="text-sm font-semibold text-text-primary">Results</h2>
                  </div>
                  {currentRun && (
                    <span className="text-xs text-text-muted">
                      {currentRun.status} • {currentRun.items.length} items
                    </span>
                  )}
                </div>

                {collapsedPanels.results ? null : !currentRun ? (
                  <p className="text-sm text-text-muted">Run a benchmark or load a previous run to inspect outputs.</p>
                ) : (
                  <div className="space-y-4 max-h-[920px] overflow-y-auto" data-testid="benchmark-results">
                    {runnerTab === "setup" && (
                      <div className="space-y-4">
                        <div className="grid grid-cols-1 gap-3 md:grid-cols-2 xl:grid-cols-4">
                          <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-4">
                            <p className="text-[11px] uppercase tracking-wide text-text-muted">Run status</p>
                            <p className="mt-2 text-lg font-semibold text-text-primary">{currentRun.status}</p>
                            <p className="mt-1 text-xs text-text-muted">{currentRun.items.length} result items</p>
                          </div>
                          <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-4">
                            <p className="text-[11px] uppercase tracking-wide text-text-muted">Total tokens</p>
                            <p className="mt-2 text-lg font-semibold text-text-primary">
                              {sum(currentRun.items.map((item) => totalTokens(item))).toLocaleString()}
                            </p>
                            <p className="mt-1 text-xs text-text-muted">Across all providers and variants</p>
                          </div>
                          <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-4">
                            <p className="text-[11px] uppercase tracking-wide text-text-muted">Average cost</p>
                            <p className="mt-2 text-lg font-semibold text-text-primary">
                              ${average(currentRun.items.map((item) => item.estimated_cost_usd))?.toFixed(5) ?? "0.00000"}
                            </p>
                            <p className="mt-1 text-xs text-text-muted">Per result item</p>
                          </div>
                          <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-4">
                            <p className="text-[11px] uppercase tracking-wide text-text-muted">Average latency</p>
                            <p className="mt-2 text-lg font-semibold text-text-primary">
                              {Math.round(average(currentRun.items.map((item) => item.latency_ms)) ?? 0)} ms
                            </p>
                            <p className="mt-1 text-xs text-text-muted">Per result item</p>
                          </div>
                        </div>

                        <div className="rounded-lg border border-dashed border-[#2a2b36] bg-[#0e0f13] p-4">
                          <p className="text-sm font-medium text-text-primary">Next views</p>
                          <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-3">
                            <button
                              onClick={() => setRunnerTab("compare")}
                              className="rounded-lg border border-[#2a2b36] bg-[#13141a] px-4 py-3 text-left"
                            >
                              <p className="text-sm font-medium text-text-primary">Compare</p>
                              <p className="mt-1 text-xs text-text-muted">Bar charts and baseline-vs-challenger deltas.</p>
                            </button>
                            <button
                              onClick={() => setRunnerTab("responses")}
                              className="rounded-lg border border-[#2a2b36] bg-[#13141a] px-4 py-3 text-left"
                            >
                              <p className="text-sm font-medium text-text-primary">Responses</p>
                              <p className="mt-1 text-xs text-text-muted">Rendered model answers, judge output, and notes.</p>
                            </button>
                            <button
                              onClick={() => setRunnerTab("gallery")}
                              className="rounded-lg border border-[#2a2b36] bg-[#13141a] px-4 py-3 text-left"
                            >
                              <p className="text-sm font-medium text-text-primary">Gallery</p>
                              <p className="mt-1 text-xs text-text-muted">Side-by-side image generations and artifacts.</p>
                            </button>
                          </div>
                        </div>
                      </div>
                    )}

                    {runnerTab === "compare" && (
                      <div className="space-y-4">
                        {runMetricRows.length > 0 && (
                          <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-4">
                            <div className="mb-3">
                              <p className="text-sm font-medium text-text-primary">Model comparison</p>
                              <p className="text-[11px] text-text-muted">Visual comparison across tokens, cost, latency, and judge score.</p>
                            </div>
                            <div className="space-y-4">
                              {runMetricRows.map((row) => (
                                <div key={row.key} className="rounded-lg border border-[#2a2b36] bg-[#13141a] p-4">
                                  <div className="mb-3 flex items-center justify-between gap-3">
                                    <div>
                                      <p className="text-sm font-medium text-text-primary">{row.providerId} / {row.modelId}</p>
                                      <p className="text-[11px] text-text-muted">{row.variantName}</p>
                                    </div>
                                  </div>
                                  <div className="grid grid-cols-1 gap-3">
                                    <div>
                                      <div className="mb-1 flex items-center justify-between text-[11px] text-text-secondary">
                                        <span>Tokens</span>
                                        <span>{row.tokens}</span>
                                      </div>
                                      <div className="h-2 rounded-full bg-[#0e0f13]">
                                        <div className="h-2 rounded-full bg-accent-blue" style={{ width: metricWidth(row.tokens, metricMaxima.tokens) }} />
                                      </div>
                                    </div>
                                    <div>
                                      <div className="mb-1 flex items-center justify-between text-[11px] text-text-secondary">
                                        <span>Cost</span>
                                        <span>${row.cost.toFixed(5)}</span>
                                      </div>
                                      <div className="h-2 rounded-full bg-[#0e0f13]">
                                        <div className="h-2 rounded-full bg-emerald-400" style={{ width: metricWidth(row.cost, metricMaxima.cost) }} />
                                      </div>
                                    </div>
                                    <div>
                                      <div className="mb-1 flex items-center justify-between text-[11px] text-text-secondary">
                                        <span>Latency</span>
                                        <span>{Math.round(row.latency)} ms</span>
                                      </div>
                                      <div className="h-2 rounded-full bg-[#0e0f13]">
                                        <div className="h-2 rounded-full bg-amber-400" style={{ width: metricWidth(row.latency, metricMaxima.latency) }} />
                                      </div>
                                    </div>
                                    {row.hasJudge && (
                                      <div>
                                        <div className="mb-1 flex items-center justify-between text-[11px] text-text-secondary">
                                          <span>Judge</span>
                                          <span>{row.judge.toFixed(1)}/100</span>
                                        </div>
                                        <div className="h-2 rounded-full bg-[#0e0f13]">
                                          <div className="h-2 rounded-full bg-fuchsia-400" style={{ width: metricWidth(row.judge, metricMaxima.judge) }} />
                                        </div>
                                      </div>
                                    )}
                                  </div>
                                </div>
                              ))}
                            </div>
                          </div>
                        )}
                    {comparisonRows.length > 0 && (
                      <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-4">
                        <div className="flex items-center justify-between gap-3 mb-3">
                          <div>
                            <p className="text-sm font-medium text-text-primary">Side-by-side comparison</p>
                            <p className="text-[11px] text-text-muted">
                              Baseline vs strategy-augmented challenger across the current run.
                            </p>
                          </div>
                        </div>
                        <div className="grid grid-cols-1 gap-3">
                          {comparisonRows.map((row) => (
                            <div key={row.key} className="rounded-lg border border-[#2a2b36] bg-[#13141a] p-4">
                              <div className="flex items-center justify-between gap-3 mb-3">
                                <div>
                                  <p className="text-sm font-medium text-text-primary">{row.providerId} / {row.modelId}</p>
                                  <p className="text-[11px] text-text-muted">{row.baselineVariant} vs {row.challengerVariant}</p>
                                </div>
                              </div>
                              <div className="grid grid-cols-2 gap-3 text-xs">
                                <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-3">
                                  <p className="text-[11px] uppercase tracking-wide text-text-muted mb-2">{row.baselineVariant}</p>
                                  <div className="space-y-1 text-text-secondary">
                                    <div>Total tokens: <span className="text-text-primary">{row.baselineTokens}</span></div>
                                    <div>Avg cost: <span className="text-text-primary">{row.baselineCost != null ? `$${row.baselineCost.toFixed(5)}` : "—"}</span></div>
                                    <div>Avg latency: <span className="text-text-primary">{row.baselineLatency != null ? `${Math.round(row.baselineLatency)} ms` : "—"}</span></div>
                                    <div>Avg judge: <span className="text-text-primary">{row.baselineJudge != null ? `${row.baselineJudge.toFixed(1)}/100` : "—"}</span></div>
                                  </div>
                                </div>
                                <div className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-3">
                                  <p className="text-[11px] uppercase tracking-wide text-text-muted mb-2">{row.challengerVariant}</p>
                                  <div className="space-y-1 text-text-secondary">
                                    <div>Total tokens: <span className="text-text-primary">{row.challengerTokens}</span></div>
                                    <div>Avg cost: <span className="text-text-primary">{row.challengerCost != null ? `$${row.challengerCost.toFixed(5)}` : "—"}</span></div>
                                    <div>Avg latency: <span className="text-text-primary">{row.challengerLatency != null ? `${Math.round(row.challengerLatency)} ms` : "—"}</span></div>
                                    <div>Avg judge: <span className="text-text-primary">{row.challengerJudge != null ? `${row.challengerJudge.toFixed(1)}/100` : "—"}</span></div>
                                  </div>
                                </div>
                              </div>
                              <div className="mt-3 grid grid-cols-1 sm:grid-cols-2 gap-3 text-[11px]">
                                <div className={`${row.challengerTokens <= row.baselineTokens ? "text-emerald-300" : "text-amber-300"}`}>
                                  Token delta: {formatDelta(row.challengerTokens - row.baselineTokens)}
                                </div>
                                <div className={`${(row.challengerCost ?? 0) <= (row.baselineCost ?? 0) ? "text-emerald-300" : "text-amber-300"}`}>
                                  Cost delta: {formatDelta((row.challengerCost ?? 0) - (row.baselineCost ?? 0))}
                                </div>
                                <div className={`${(row.challengerLatency ?? 0) <= (row.baselineLatency ?? 0) ? "text-emerald-300" : "text-amber-300"}`}>
                                  Latency delta: {formatDelta((row.challengerLatency ?? 0) - (row.baselineLatency ?? 0))} ms
                                </div>
                                <div className={`${(row.challengerJudge ?? 0) >= (row.baselineJudge ?? 0) ? "text-emerald-300" : "text-amber-300"}`}>
                                  Judge delta: {formatDelta((row.challengerJudge ?? 0) - (row.baselineJudge ?? 0))}
                                </div>
                              </div>
                            </div>
                          ))}
                        </div>
                      </div>
                    )}
                      </div>
                    )}

                    {runnerTab === "gallery" && (
                      imageComparisonGroups.length > 0 ? (
                        <div className="space-y-4">
                          {imageComparisonGroups.map((group) => (
                            <div key={group.caseName} className="rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-4">
                              <div className="mb-4">
                                <p className="text-sm font-medium text-text-primary">{group.caseName}</p>
                                <p className="text-[11px] text-text-muted">Rendered side-by-side for visual comparison across models and variants.</p>
                              </div>
                              <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
                                {group.items.map(({ item, artifacts }) => (
                                  <div key={item.item_id} className="rounded-lg border border-[#2a2b36] bg-[#13141a] p-3">
                                    <div className="mb-3 flex items-center justify-between gap-3">
                                      <div className="min-w-0">
                                        <p className="truncate text-sm font-medium text-text-primary">{item.provider_id} / {item.model_id}</p>
                                        <p className="text-[11px] text-text-muted">{item.variant_name}</p>
                                      </div>
                                      <span className="rounded-full bg-white/5 px-2 py-1 text-[10px] uppercase tracking-wide text-text-muted">
                                        {item.status}
                                      </span>
                                    </div>
                                    <div className="space-y-3">
                                      {artifacts.map((artifact) => (
                                        <div key={artifact.path} className="overflow-hidden rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-3">
                                          {artifact.preview_data_url ? (
                                            <img src={artifact.preview_data_url} alt={artifact.label} className="max-h-[420px] w-full rounded-lg object-contain" />
                                          ) : null}
                                          <div className="mt-3 grid grid-cols-1 gap-2 text-[11px] text-text-secondary sm:grid-cols-3">
                                            <div>Latency: {formatMetric(item.latency_ms, " ms")}</div>
                                            <div>Cost: {item.estimated_cost_usd != null ? `$${item.estimated_cost_usd.toFixed(5)}` : "—"}</div>
                                            <div>Context: {item.context_used_percent != null ? `${item.context_used_percent}%` : "—"}</div>
                                          </div>
                                        </div>
                                      ))}
                                    </div>
                                  </div>
                                ))}
                              </div>
                            </div>
                          ))}
                        </div>
                      ) : (
                        <p className="text-sm text-text-muted">No generated image artifacts in the current run yet.</p>
                      )
                    )}

                    {runnerTab === "responses" && currentRun.items.map((item) => (

                      <div key={item.item_id} className="min-w-0 overflow-hidden rounded-lg border border-[#2a2b36] bg-[#0e0f13] p-4">
                        <div className="flex items-start justify-between gap-3 mb-3">
                          <div className="min-w-0">
                            <p className="truncate text-sm font-medium text-text-primary">{item.case_name}</p>
                            <p className="text-[11px] text-text-muted">
                              {item.provider_id} / {item.model_id} / {item.variant_name}
                            </p>
                          </div>
                          <span className={`rounded-full px-2 py-1 text-[10px] uppercase tracking-wide ${item.status === "completed" ? "bg-emerald-500/15 text-emerald-300" : "bg-red-500/15 text-red-300"}`}>
                            {item.status}
                          </span>
                        </div>

                        <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 mb-3 text-[11px] text-text-secondary">
                          <div>Latency: {formatMetric(item.latency_ms, " ms")}</div>
                          <div>Estimated cost: {item.estimated_cost_usd != null ? `$${item.estimated_cost_usd.toFixed(5)}` : "—"}</div>
                          <div>Input tokens: {item.token_counts.input_tokens}</div>
                          <div>Output tokens: {item.token_counts.output_tokens}</div>
                          <div>Context window: {formatMetric(item.context_window)}</div>
                          <div>Context used: {item.context_used_percent != null ? `${item.context_used_percent}%` : "—"}</div>
                        </div>

                        {item.output_text && (
                          <div className="mb-3 grid grid-cols-1 gap-3">
                            <div className="rounded-lg border border-[#2a2b36] bg-[#13141a] p-3">
                              <p className="mb-2 text-xs font-medium text-text-primary">Rendered response</p>
                              <div className="space-y-2">
                                {renderResponseBlocks(item.output_text)}
                              </div>
                            </div>
                            <pre className="max-h-48 overflow-y-auto whitespace-pre-wrap break-words rounded-lg border border-[#2a2b36] bg-[#13141a] p-3 text-xs text-text-secondary">
                              {item.output_text}
                            </pre>
                          </div>
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

                        <div className="grid grid-cols-1 gap-2 xl:grid-cols-[120px_160px_minmax(0,1fr)]">
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
          </div>
        )}

        {loading && !running ? (
          <div className="mt-4 text-sm text-text-muted">Loading benchmark data…</div>
        ) : null}
      </div>
    </div>
  );
}
