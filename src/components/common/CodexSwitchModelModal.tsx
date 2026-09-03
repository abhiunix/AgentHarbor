import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  open: boolean;
  onClose: () => void;
}

interface CodexReasoningEffort {
  reasoningEffort: string;
  description?: string;
}

interface CodexModel {
  id: string;
  model: string;
  displayName: string;
  description?: string;
  defaultReasoningEffort: string;
  supportedReasoningEfforts: CodexReasoningEffort[];
  inputModalities: string[];
  isDefault: boolean;
}

interface CodexModelList {
  models: CodexModel[];
  appServerAvailable: boolean;
  warning?: string;
  configuredModel?: string;
  configuredReasoningEffort?: string;
}

interface CodexModelUpdateResult {
  configuredModel?: string;
  configuredReasoningEffort?: string;
  warning?: string;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function matchesConfiguredModel(
  model: CodexModel,
  configuredModel?: string,
): boolean {
  const configured = configuredModel?.trim();
  return (
    Boolean(configured) &&
    (model.id === configured || model.model === configured)
  );
}

function selectInitialModel(catalog: CodexModelList): CodexModel | undefined {
  return (
    catalog.models.find((model) =>
      matchesConfiguredModel(model, catalog.configuredModel),
    ) ?? catalog.models.find((model) => model.isDefault)
  );
}

function initialReasoningEffort(
  model: CodexModel | undefined,
  configuredReasoningEffort?: string,
): string {
  if (!model) return "";
  const configured = configuredReasoningEffort?.trim();
  if (configured) return configured;
  const supportedDefault = model.supportedReasoningEfforts.find(
    (effort) => effort.reasoningEffort === model.defaultReasoningEffort,
  );
  return (
    supportedDefault?.reasoningEffort ??
    model.supportedReasoningEfforts[0]?.reasoningEffort ??
    model.defaultReasoningEffort ??
    ""
  );
}

export function CodexSwitchModelModal({ open, onClose }: Props) {
  const [catalog, setCatalog] = useState<CodexModelList | null>(null);
  const [selectedModelId, setSelectedModelId] = useState("");
  const [selectedReasoningEffort, setSelectedReasoningEffort] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [saveWarning, setSaveWarning] = useState<string | null>(null);
  const [savedSelection, setSavedSelection] = useState({
    modelId: "",
    reasoningEffort: "",
  });
  const requestGeneration = useRef(0);
  const dialogRef = useRef<HTMLDivElement | null>(null);
  const closeButtonRef = useRef<HTMLButtonElement | null>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) {
      requestGeneration.current += 1;
      return;
    }
    const generation = ++requestGeneration.current;
    setLoading(true);
    setSaving(false);
    setCatalog(null);
    setSelectedModelId("");
    setSelectedReasoningEffort("");
    setError(null);
    setSuccess(null);
    setSaveWarning(null);
    setSavedSelection({ modelId: "", reasoningEffort: "" });

    invoke<CodexModelList>("list_codex_models")
      .then((result) => {
        if (generation !== requestGeneration.current) return;
        setCatalog(result);
        const initialModel = selectInitialModel(result);
        const usesConfiguredModel = Boolean(
          initialModel &&
          matchesConfiguredModel(initialModel, result.configuredModel),
        );
        const modelId = initialModel?.id ?? "";
        const reasoningEffort = initialReasoningEffort(
          initialModel,
          usesConfiguredModel ? result.configuredReasoningEffort : undefined,
        );
        setSelectedModelId(modelId);
        setSelectedReasoningEffort(reasoningEffort);
        setSavedSelection({ modelId, reasoningEffort });
      })
      .catch((loadError) => {
        if (generation === requestGeneration.current) {
          setError(errorMessage(loadError));
        }
      })
      .finally(() => {
        if (generation === requestGeneration.current) setLoading(false);
      });

    return () => {
      if (generation === requestGeneration.current) {
        requestGeneration.current += 1;
      }
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    previousFocusRef.current =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    const focusFrame = window.requestAnimationFrame(() => {
      closeButtonRef.current?.focus();
    });
    return () => {
      window.cancelAnimationFrame(focusFrame);
      previousFocusRef.current?.focus();
      previousFocusRef.current = null;
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        if (!saving) onClose();
        return;
      }
      if (event.key !== "Tab") return;

      const dialog = dialogRef.current;
      if (!dialog) return;
      const focusable = Array.from(
        dialog.querySelectorAll<HTMLElement>(
          'button:not([disabled]), select:not([disabled]), input:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])',
        ),
      ).filter((element) => !element.hasAttribute("aria-hidden"));
      if (focusable.length === 0) {
        event.preventDefault();
        dialog.focus();
        return;
      }

      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      const activeElement = document.activeElement;
      if (!activeElement || !dialog.contains(activeElement)) {
        event.preventDefault();
        (event.shiftKey ? last : first).focus();
        return;
      }
      if (
        event.shiftKey &&
        (activeElement === first || activeElement === dialog)
      ) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [onClose, open, saving]);

  const selectedModel = useMemo(
    () => catalog?.models.find((model) => model.id === selectedModelId),
    [catalog, selectedModelId],
  );

  const selectedMatchesConfigured = Boolean(
    selectedModel &&
    matchesConfiguredModel(selectedModel, catalog?.configuredModel),
  );

  const reasoningOptions = useMemo(() => {
    if (!selectedModel) return [];
    const options = [...selectedModel.supportedReasoningEfforts];
    const configuredEffort = selectedMatchesConfigured
      ? catalog?.configuredReasoningEffort?.trim()
      : undefined;
    if (
      configuredEffort &&
      !options.some((option) => option.reasoningEffort === configuredEffort)
    ) {
      options.unshift({
        reasoningEffort: configuredEffort,
        description: "Current configured value",
      });
    }
    if (
      selectedModel.defaultReasoningEffort &&
      !options.some(
        (option) =>
          option.reasoningEffort === selectedModel.defaultReasoningEffort,
      )
    ) {
      options.unshift({
        reasoningEffort: selectedModel.defaultReasoningEffort,
        description: "Model default reported by Codex",
      });
    }
    return options;
  }, [
    catalog?.configuredReasoningEffort,
    selectedMatchesConfigured,
    selectedModel,
  ]);

  const selectedEffort = reasoningOptions.find(
    (option) => option.reasoningEffort === selectedReasoningEffort,
  );

  const dirty =
    selectedModelId !== savedSelection.modelId ||
    selectedReasoningEffort !== savedSelection.reasoningEffort;

  function requestClose() {
    if (!saving) onClose();
  }

  function handleModelChange(modelId: string) {
    const model = catalog?.models.find((candidate) => candidate.id === modelId);
    setSelectedModelId(modelId);
    setSelectedReasoningEffort(
      initialReasoningEffort(
        model,
        model && matchesConfiguredModel(model, catalog?.configuredModel)
          ? catalog?.configuredReasoningEffort
          : undefined,
      ),
    );
    setError(null);
    setSuccess(null);
    setSaveWarning(null);
  }

  async function handleSave() {
    if (!selectedModelId || !selectedReasoningEffort || !dirty) return;
    const generation = requestGeneration.current;
    setSaving(true);
    setError(null);
    setSuccess(null);
    setSaveWarning(null);
    try {
      const result = await invoke<CodexModelUpdateResult>(
        "update_codex_model_settings",
        {
          model: selectedModelId,
          reasoningEffort: selectedReasoningEffort,
        },
      );
      if (generation !== requestGeneration.current) return;
      setCatalog((current) =>
        current
          ? {
              ...current,
              configuredModel: result.configuredModel,
              configuredReasoningEffort: result.configuredReasoningEffort,
              models: current.models.map((model) => ({
                ...model,
                isDefault: matchesConfiguredModel(
                  model,
                  result.configuredModel,
                ),
              })),
            }
          : current,
      );
      setSavedSelection({
        modelId: selectedModelId,
        reasoningEffort: selectedReasoningEffort,
      });
      setSuccess(
        result.warning
          ? "Saved to the Codex configuration file."
          : "Saved. The model and reasoning effort apply to new Codex sessions.",
      );
      setSaveWarning(result.warning ?? null);
    } catch (saveError) {
      if (generation === requestGeneration.current) {
        setError(errorMessage(saveError));
      }
    } finally {
      if (generation === requestGeneration.current) setSaving(false);
    }
  }

  if (!open) return null;

  const noReasoningOptions =
    Boolean(selectedModel) && reasoningOptions.length === 0;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm p-4"
      onClick={(event) => {
        if (event.target === event.currentTarget) requestClose();
      }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="codex-switch-model-title"
        aria-busy={loading || saving}
        tabIndex={-1}
        className="bg-app-card border border-border rounded-xl w-full max-w-2xl shadow-2xl overflow-hidden"
        onClick={(event) => event.stopPropagation()}
      >
        <div className="px-6 py-4 border-b border-border flex items-center justify-between gap-4">
          <div>
            <h2
              id="codex-switch-model-title"
              className="text-lg font-semibold text-text-primary"
            >
              Switch Codex Model
            </h2>
            <p className="text-xs text-text-muted mt-1">
              Updates the global defaults used by new Codex sessions.
            </p>
          </div>
          <button
            ref={closeButtonRef}
            type="button"
            onClick={requestClose}
            disabled={saving}
            aria-label="Close model switcher"
            className="rounded p-1 text-text-muted hover:text-text-primary focus:outline-none focus:ring-2 focus:ring-[#10a37f]/50 disabled:opacity-50"
          >
            &#x2715;
          </button>
        </div>

        <div className="p-6 space-y-5 max-h-[70vh] overflow-y-auto">
          {loading && (
            <div className="h-40 flex items-center justify-center text-sm text-text-muted">
              Loading Codex models...
            </div>
          )}

          {error && (
            <div
              role="alert"
              aria-live="assertive"
              className="px-3 py-2 rounded-md border border-red-500/40 bg-red-500/10 text-sm text-red-400"
            >
              {error}
            </div>
          )}

          {success && (
            <div
              role="status"
              aria-live="polite"
              className="px-3 py-2 rounded-md border border-emerald-500/40 bg-emerald-500/10 text-sm text-emerald-400"
            >
              {success}
            </div>
          )}

          {saveWarning && (
            <div
              role="status"
              aria-live="polite"
              className="px-3 py-2 rounded-md border border-amber-500/40 bg-amber-500/10 text-sm text-amber-300"
            >
              {saveWarning}
            </div>
          )}

          {!loading && catalog && (
            <>
              {!catalog.appServerAvailable && (
                <div className="px-3 py-2 rounded-md border border-amber-500/40 bg-amber-500/10 text-sm text-amber-300 leading-relaxed">
                  Codex App Server is unavailable. The fallback model catalog
                  may be incomplete or older than your installed Codex version.
                </div>
              )}

              {catalog.warning && (
                <div className="px-3 py-2 rounded-md border border-amber-500/40 bg-amber-500/10 text-sm text-amber-300">
                  {catalog.warning}
                </div>
              )}

              {catalog.models.length === 0 ? (
                <div className="py-10 text-center">
                  <p className="text-sm text-text-secondary">
                    No Codex models were reported.
                  </p>
                  <p className="text-xs text-text-muted mt-1">
                    Refresh after Codex App Server or the local model cache
                    becomes available.
                  </p>
                </div>
              ) : (
                <>
                  {!selectedModel && (
                    <div className="px-3 py-2 rounded-md border border-border bg-app-bg text-sm text-text-secondary">
                      No current Codex model was reported. Choose a model
                      explicitly to change the global default.
                    </div>
                  )}
                  <div>
                    <label
                      htmlFor="codex-model"
                      className="block text-xs font-medium text-text-secondary mb-1.5"
                    >
                      Model
                    </label>
                    <select
                      id="codex-model"
                      value={selectedModelId}
                      disabled={saving}
                      onChange={(event) =>
                        handleModelChange(event.target.value)
                      }
                      className="w-full px-3 py-2.5 bg-app-bg border border-border rounded-lg text-sm text-text-primary focus:outline-none focus:border-[#10a37f] disabled:opacity-50"
                    >
                      {!selectedModelId && (
                        <option value="" disabled>
                          Select a model
                        </option>
                      )}
                      {catalog.models.map((model) => (
                        <option key={model.id} value={model.id}>
                          {model.displayName} ({model.id})
                        </option>
                      ))}
                    </select>
                  </div>

                  {selectedModel && (
                    <div className="bg-app-bg border border-border rounded-lg p-4">
                      <div className="flex items-start justify-between gap-3">
                        <div>
                          <p className="text-sm font-medium text-text-primary">
                            {selectedModel.displayName}
                          </p>
                          <p className="text-xs font-mono text-text-muted mt-0.5">
                            {selectedModel.id}
                          </p>
                        </div>
                        {selectedModel.isDefault && (
                          <span className="text-[10px] font-semibold uppercase tracking-wide px-2 py-1 rounded bg-[#10a37f]/20 text-[#5fd3b1]">
                            Current default
                          </span>
                        )}
                      </div>
                      {selectedModel.description && (
                        <p className="text-xs text-text-secondary mt-3 leading-relaxed">
                          {selectedModel.description}
                        </p>
                      )}
                      {selectedModel.inputModalities.length > 0 && (
                        <div className="flex flex-wrap gap-1.5 mt-3">
                          {selectedModel.inputModalities.map((modality) => (
                            <span
                              key={modality}
                              className="text-[10px] px-2 py-0.5 rounded bg-[#2a2b36] text-text-secondary"
                            >
                              {modality} input
                            </span>
                          ))}
                        </div>
                      )}
                    </div>
                  )}

                  <div>
                    <label
                      htmlFor="codex-reasoning-effort"
                      className="block text-xs font-medium text-text-secondary mb-1.5"
                    >
                      Reasoning effort
                    </label>
                    <select
                      id="codex-reasoning-effort"
                      value={selectedReasoningEffort}
                      disabled={saving || !selectedModel || noReasoningOptions}
                      onChange={(event) => {
                        setSelectedReasoningEffort(event.target.value);
                        setError(null);
                        setSuccess(null);
                        setSaveWarning(null);
                      }}
                      className="w-full px-3 py-2.5 bg-app-bg border border-border rounded-lg text-sm text-text-primary focus:outline-none focus:border-[#10a37f] disabled:opacity-50"
                    >
                      {!selectedReasoningEffort && (
                        <option value="" disabled>
                          Select a reasoning effort
                        </option>
                      )}
                      {reasoningOptions.map((effort) => (
                        <option
                          key={effort.reasoningEffort}
                          value={effort.reasoningEffort}
                        >
                          {effort.reasoningEffort}
                          {effort.reasoningEffort ===
                          selectedModel?.defaultReasoningEffort
                            ? " (model default)"
                            : ""}
                        </option>
                      ))}
                    </select>
                    {selectedEffort?.description && (
                      <p className="text-xs text-text-muted mt-1.5 leading-relaxed">
                        {selectedEffort.description}
                      </p>
                    )}
                    {noReasoningOptions && (
                      <p className="text-xs text-amber-400 mt-1.5">
                        This model did not report a supported reasoning effort,
                        so it cannot be saved safely.
                      </p>
                    )}
                  </div>

                  <div className="px-3 py-2.5 rounded-md border border-border bg-app-bg text-xs text-text-muted leading-relaxed">
                    Saving changes the default model and reasoning effort in
                    Codex configuration. It does not change sessions that are
                    already running.
                  </div>
                </>
              )}
            </>
          )}
        </div>

        <div className="px-6 py-4 border-t border-border flex items-center justify-end gap-2">
          {dirty && (
            <span className="mr-auto text-xs font-medium text-amber-400">
              Unsaved changes
            </span>
          )}
          <button
            type="button"
            onClick={requestClose}
            disabled={saving}
            className="px-4 py-2 text-sm text-text-secondary hover:text-text-primary disabled:opacity-50"
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={
              loading ||
              saving ||
              !selectedModelId ||
              !selectedReasoningEffort ||
              !dirty ||
              (catalog?.models.length ?? 0) === 0
            }
            className="px-4 py-2 bg-[#10a37f] text-white text-sm rounded-md font-medium hover:bg-[#0d8c6d] disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {saving ? "Saving..." : "Save"}
          </button>
        </div>
      </div>
    </div>
  );
}
