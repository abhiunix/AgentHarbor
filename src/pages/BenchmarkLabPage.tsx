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

function supportedModels(models: BenchmarkModel[], providerId: string, modality: BenchmarkModality): BenchmarkModel[] {
  return models.filter((model) => model.provider_id === providerId && model.modality === modality);
}

function defaultTargetForModality(modality: BenchmarkModality): TargetDraft {
  return modality === "image"
    ? { providerId: "mock", modelId: "mock-image" }
    : { providerId: "mock", modelId: "mock-fast" };
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
    loadRun,
    runSuite,
    saveDataset,
    saveManualReview,
    clearError,
  } = useBenchmarkStore();
  const capabilities = useRegistryStore((state) => state.capabilities);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [activeTab, setActiveTab] = useState<"runner" | "references">("runner");
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
