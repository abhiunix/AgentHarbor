import { useState, useEffect } from "react";
import { ProjectSelector } from "./ProjectSelector";
import { AdapterSelector } from "./AdapterSelector";
import { useProjectStore, getProjectName } from "../../stores/projectStore";
import { previewDeploy, executeDeploy, recordDeployment, DiffEntry, DeployResultResponse, type ClaudeSettingsTarget } from "../../lib/tauri";
import { PROJECTS_RELOAD_EVENT } from "../projects/ProjectList";
import { basename } from "../../lib/platform";

export type DeployStep = "project" | "select" | "preview" | "success";

interface DeployWizardProps {
  isOpen: boolean;
  onClose: () => void;
  initialCapabilityIds?: string[];
  initialAgentIds?: string[];
  initialProjectPath?: string;
}

interface MultiAdapterDiff {
  adapterId: string;
  adapterName: string;
  diffs: DiffEntry[];
}

interface MultiAdapterResult {
  adapterId: string;
  adapterName: string;
  result: DeployResultResponse;
}

const ADAPTER_NAMES: Record<string, string> = {
  "claude-code": "Claude Code",
  "cursor": "Cursor",
  "windsurf": "Windsurf",
};

export function DeployWizard({
  isOpen,
  onClose,
  initialCapabilityIds = [],
  initialAgentIds = [],
  initialProjectPath,
}: DeployWizardProps) {
  const [step, setStep] = useState<DeployStep>(initialProjectPath ? "select" : "project");
  const [isGlobalDeploy, setIsGlobalDeploy] = useState(false);
  const [selectedCapabilityIds, setSelectedCapabilityIds] = useState<string[]>(initialCapabilityIds);
  const [selectedAgentIds, setSelectedAgentIds] = useState<string[]>(initialAgentIds);
  const [selectedAdapterIds, setSelectedAdapterIds] = useState<string[]>(["claude-code"]);
  const [claudeSettingsTargets, setClaudeSettingsTargets] = useState<Set<ClaudeSettingsTarget>>(new Set(["local"]));
  const [multiAdapterDiffs, setMultiAdapterDiffs] = useState<MultiAdapterDiff[]>([]);
  const [strategies, setStrategies] = useState<Record<string, string>>({});
  const [multiResults, setMultiResults] = useState<MultiAdapterResult[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const { selectedProject, clearSelection, setSelectedProject } = useProjectStore();

  useEffect(() => {
    if (initialProjectPath && !selectedProject) {
      setSelectedProject(initialProjectPath);
    }
  }, [initialProjectPath, selectedProject, setSelectedProject]);

  if (!isOpen) return null;

  const handleClose = () => {
    setStep("project");
    setIsGlobalDeploy(false);
    setSelectedCapabilityIds(initialCapabilityIds);
    setSelectedAgentIds(initialAgentIds);
    setMultiAdapterDiffs([]);
    setStrategies({});
    setMultiResults([]);
    setError(null);
    clearSelection();
    onClose();
  };

  const handleProjectSelected = () => {
    setStep("select");
  };

  const handleGlobalDeploy = () => {
    setIsGlobalDeploy(true);
    setStep("select");
  };

  const handleSelectionComplete = async (capIds: string[], agentIds: string[]) => {
    setSelectedCapabilityIds(capIds);
    setSelectedAgentIds(agentIds);
    setLoading(true);
    setError(null);

    try {
      const allDiffs: MultiAdapterDiff[] = [];
      const allStrategies: Record<string, string> = {};

      for (const adapterId of selectedAdapterIds) {
        const agentsForAdapter = isGlobalDeploy || adapterId === "windsurf" ? [] : agentIds;

        if (adapterId === "claude-code" && !isGlobalDeploy) {
          // Deploy to each selected Claude settings target
          const targets = Array.from(claudeSettingsTargets);
          for (const target of targets) {
            const diffs = await previewDeploy(
              selectedProject || ".",
              adapterId,
              capIds,
              agentsForAdapter,
              target,
              false
            );
            const label = target === "local" ? "Claude Code (local)" : "Claude Code (shared)";
            allDiffs.push({ adapterId: `${adapterId}:${target}`, adapterName: label, diffs });
            diffs.forEach((d) => {
              allStrategies[`${adapterId}:${target}:${d.file_path}`] = "merge";
            });
          }
        } else {
          const claudeTarget = adapterId === "claude-code" && isGlobalDeploy ? "user" as ClaudeSettingsTarget : undefined;
          const diffs = await previewDeploy(
            selectedProject || ".",
            adapterId,
            capIds,
            agentsForAdapter,
            claudeTarget,
            isGlobalDeploy
          );
          allDiffs.push({ adapterId, adapterName: ADAPTER_NAMES[adapterId] || adapterId, diffs });
          diffs.forEach((d) => {
            allStrategies[`${adapterId}:${d.file_path}`] = "merge";
          });
        }
      }

      setMultiAdapterDiffs(allDiffs);
      setStrategies(allStrategies);
      setStep("preview");
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to preview");
    } finally {
      setLoading(false);
    }
  };

  const handleDeploy = async () => {
    setLoading(true);
    setError(null);

    try {
      const results: MultiAdapterResult[] = [];
      const projectPathForRecord = selectedProject || ".";
      let recordedAnyProjectDeploy = false;

      for (const adapterId of selectedAdapterIds) {
        const agentsForAdapter = isGlobalDeploy || adapterId === "windsurf" ? [] : selectedAgentIds;

        if (adapterId === "claude-code" && !isGlobalDeploy) {
          const targets = Array.from(claudeSettingsTargets);
          for (const target of targets) {
            const prefix = `${adapterId}:${target}:`;
            const adapterStrategies: Record<string, string> = {};
            Object.entries(strategies).forEach(([key, value]) => {
              if (key.startsWith(prefix)) {
                adapterStrategies[key.substring(prefix.length)] = value;
              }
            });
            const result = await executeDeploy(
              projectPathForRecord,
              adapterId,
              selectedCapabilityIds,
              agentsForAdapter,
              adapterStrategies,
              target,
              false
            );
            const label = target === "local" ? "Claude Code (local)" : "Claude Code (shared)";
            results.push({ adapterId: `${adapterId}:${target}`, adapterName: label, result });
            if (result.success) {
              try {
                await recordDeployment(
                  projectPathForRecord,
                  adapterId,
                  selectedCapabilityIds,
                  agentsForAdapter
                );
                recordedAnyProjectDeploy = true;
              } catch (e) {
                console.error("recordDeployment failed:", e);
              }
            }
          }
        } else {
          const adapterStrategies: Record<string, string> = {};
          Object.entries(strategies).forEach(([key, value]) => {
            if (key.startsWith(`${adapterId}:`)) {
              adapterStrategies[key.substring(adapterId.length + 1)] = value;
            }
          });
          const claudeTarget = adapterId === "claude-code" && isGlobalDeploy ? "user" as ClaudeSettingsTarget : undefined;
          const result = await executeDeploy(
            projectPathForRecord,
            adapterId,
            selectedCapabilityIds,
            agentsForAdapter,
            adapterStrategies,
            claudeTarget,
            isGlobalDeploy
          );
          results.push({ adapterId, adapterName: ADAPTER_NAMES[adapterId] || adapterId, result });
          if (result.success && !isGlobalDeploy) {
            try {
              await recordDeployment(
                projectPathForRecord,
                adapterId,
                selectedCapabilityIds,
                agentsForAdapter
              );
              recordedAnyProjectDeploy = true;
            } catch (e) {
              console.error("recordDeployment failed:", e);
            }
          }
        }
      }

      setMultiResults(results);
      setStep("success");
      if (recordedAnyProjectDeploy) {
        window.dispatchEvent(new CustomEvent(PROJECTS_RELOAD_EVENT));
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : "Deploy failed");
    } finally {
      setLoading(false);
    }
  };

  const handleStrategyChange = (filePath: string, strategy: string) => {
    setStrategies((prev) => ({ ...prev, [filePath]: strategy }));
  };

  const combinedResult: DeployResultResponse | null = multiResults.length > 0
    ? {
        success: multiResults.every((r) => r.result.success),
        files_written: multiResults.flatMap((r) => r.result.files_written),
        errors: multiResults.flatMap((r) => r.result.errors),
        warnings: multiResults.flatMap((r) => r.result.warnings || []),
      }
    : null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div data-testid="deploy-wizard" className="bg-app-sidebar border border-border rounded-xl w-full max-w-3xl max-h-[85vh] flex flex-col shadow-2xl">
        <div className="flex items-center justify-between px-6 py-4 border-b border-border">
          <div className="flex items-center gap-3">
            <h2 className="text-lg font-semibold text-text-primary">
            {isGlobalDeploy ? "Deploy to Global Config" : "Deploy to Project"}
          </h2>
            <StepIndicator currentStep={step} />
          </div>
          <button
            onClick={handleClose}
            className="w-8 h-8 flex items-center justify-center rounded-full hover:bg-white/10 text-text-muted hover:text-text-primary transition-colors"
          >
            ✕
          </button>
        </div>

        <div className={`flex-1 ${step === "select" ? "flex flex-col overflow-hidden" : "overflow-y-auto"}`}>
          {error && (
            <div className="mx-6 mt-4 p-3 bg-accent-red/10 border border-accent-red/30 rounded-lg text-accent-red text-sm flex-shrink-0">
              {error}
            </div>
          )}

          {step === "project" && (
            <div className="p-6">
              <p className="text-text-muted mb-4">Select a project folder to deploy capabilities to.</p>
              <ProjectSelector
                onProjectSelected={handleProjectSelected}
                onGlobalDeploy={handleGlobalDeploy}
              />
            </div>
          )}

          {step === "select" && (
            <AdapterSelector
              selectedAdapterIds={selectedAdapterIds}
              onAdapterChange={setSelectedAdapterIds}
              claudeSettingsTargets={claudeSettingsTargets}
              onClaudeSettingsTargetsChange={setClaudeSettingsTargets}
              initialCapabilityIds={selectedCapabilityIds}
              initialAgentIds={selectedAgentIds}
              onComplete={handleSelectionComplete}
              onBack={isGlobalDeploy ? undefined : () => setStep("project")}
              loading={loading}
              isGlobalDeploy={isGlobalDeploy}
            />
          )}

          {step === "preview" && (
            <MultiAdapterPreview
              multiAdapterDiffs={multiAdapterDiffs}
              strategies={strategies}
              onStrategyChange={handleStrategyChange}
              onDeploy={handleDeploy}
              onBack={() => setStep("select")}
              loading={loading}
            />
          )}

          {step === "success" && combinedResult && (
            <MultiAdapterSuccess
              results={multiResults}
              projectName={isGlobalDeploy ? "Global Config" : getProjectName(selectedProject || "")}
              capabilityCount={selectedCapabilityIds.length}
              agentCount={isGlobalDeploy ? 0 : selectedAgentIds.length}
              onClose={handleClose}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function StepIndicator({ currentStep }: { currentStep: DeployStep }) {
  const steps: { key: DeployStep; label: string }[] = [
    { key: "project", label: "1" },
    { key: "select", label: "2" },
    { key: "preview", label: "3" },
    { key: "success", label: "4" },
  ];

  const currentIndex = steps.findIndex((s) => s.key === currentStep);

  return (
    <div className="flex items-center gap-1">
      {steps.map((step, i) => (
        <div
          key={step.key}
          className={`w-6 h-6 rounded-full flex items-center justify-center text-xs font-medium ${
            i === currentIndex
              ? "bg-accent-blue text-white"
              : i < currentIndex
              ? "bg-accent-green text-white"
              : "bg-white/10 text-text-muted"
          }`}
        >
          {i < currentIndex ? "✓" : step.label}
        </div>
      ))}
    </div>
  );
}

interface MultiAdapterPreviewProps {
  multiAdapterDiffs: MultiAdapterDiff[];
  strategies: Record<string, string>;
  onStrategyChange: (key: string, strategy: string) => void;
  onDeploy: () => void;
  onBack: () => void;
  loading: boolean;
}

function MultiAdapterPreview({
  multiAdapterDiffs,
  strategies,
  onStrategyChange,
  onDeploy,
  onBack,
  loading,
}: MultiAdapterPreviewProps) {
  const [selectedFile, setSelectedFile] = useState<string | null>(null);

  const totalFiles = multiAdapterDiffs.reduce((sum, mad) => sum + mad.diffs.length, 0);

  const adapterColorMap: Record<string, string> = {
    "claude-code": "#9333ea",
    "cursor": "#3b82f6",
    "windsurf": "#22c55e",
    "gemini": "#4285f4",
  };
  // Support composite adapterIds like "claude-code:local"
  const getAdapterColor = (id: string) => {
    if (adapterColorMap[id]) return adapterColorMap[id];
    const base = id.split(":")[0];
    return adapterColorMap[base] || "#666";
  };

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 flex min-h-0">
        <div className="w-1/3 border-r border-border overflow-y-auto">
          <div className="p-4">
            <p className="text-xs text-text-muted uppercase mb-3">
              Files to Deploy ({totalFiles})
            </p>
            {multiAdapterDiffs.map((mad) => (
              <div key={mad.adapterId} className="mb-4">
                <div className="flex items-center gap-2 mb-2">
                  <div
                    className="w-2 h-2 rounded-full"
                    style={{ backgroundColor: mad.diffs.length === 0 ? "#555" : getAdapterColor(mad.adapterId) }}
                  />
                  <span className={`text-xs font-medium ${mad.diffs.length === 0 ? "text-text-muted" : "text-text-primary"}`}>{mad.adapterName}</span>
                  {mad.diffs.length === 0 ? (
                    <span className="text-xs text-text-muted italic">does not support</span>
                  ) : (
                    <span className="text-xs text-text-muted">({mad.diffs.length} files)</span>
                  )}
                </div>
                <div className="space-y-1 pl-4">
                  {mad.diffs.map((diff) => {
                    const key = `${mad.adapterId}:${diff.file_path}`;
                    const isSharedAgent = diff.file_path.includes("/agents/") && 
                      multiAdapterDiffs.filter((m) => m.diffs.some((d) => d.file_path === diff.file_path)).length > 1;
                    
                    return (
                      <button
                        key={key}
                        onClick={() => setSelectedFile(key)}
                        className={`w-full text-left px-2 py-1.5 rounded text-xs transition-colors ${
                          selectedFile === key
                            ? "bg-accent-blue/20 text-accent-blue"
                            : "hover:bg-white/5 text-text-muted"
                        }`}
                      >
                        <div className="flex items-center gap-2">
                          <span className={`text-[10px] uppercase px-1 rounded ${
                            diff.change_type === "add" ? "bg-accent-green/20 text-accent-green" :
                            diff.change_type === "modify" ? "bg-accent-orange/20 text-accent-orange" :
                            "bg-accent-red/20 text-accent-red"
                          }`}>
                            {diff.change_type === "add" ? "+" : diff.change_type === "modify" ? "~" : "-"}
                          </span>
                          <span className="truncate flex-1">{basename(diff.file_path)}</span>
                          {isSharedAgent && (
                            <span className="text-[9px] px-1 rounded bg-purple-500/20 text-purple-400">shared</span>
                          )}
                        </div>
                      </button>
                    );
                  })}
                </div>
              </div>
            ))}
          </div>
        </div>

        <div className="flex-1 overflow-y-auto p-4">
          {selectedFile ? (
            <DiffDetail
              fileKey={selectedFile}
              multiAdapterDiffs={multiAdapterDiffs}
              strategy={strategies[selectedFile] || "merge"}
              onStrategyChange={(s) => onStrategyChange(selectedFile, s)}
            />
          ) : (
            <div className="flex items-center justify-center h-full text-text-muted">
              Select a file to see diff
            </div>
          )}
        </div>
      </div>

      <div className="flex items-center justify-between px-6 py-4 border-t border-border">
        <button
          onClick={onBack}
          data-testid="wizard-back"
          className="px-4 py-2 text-sm text-text-muted hover:text-text-primary transition-colors"
        >
          ← Back
        </button>
        <button
          onClick={onDeploy}
          disabled={loading || totalFiles === 0}
          data-testid="wizard-deploy"
          className="px-6 py-2 bg-accent-blue text-white rounded-lg font-medium hover:bg-accent-blue/80 transition-colors disabled:opacity-50"
        >
          {loading ? "Deploying..." : `Deploy to ${multiAdapterDiffs.length} Adapter${multiAdapterDiffs.length !== 1 ? "s" : ""}`}
        </button>
      </div>
    </div>
  );
}

function DiffDetail({
  fileKey,
  multiAdapterDiffs,
  strategy,
  onStrategyChange,
}: {
  fileKey: string;
  multiAdapterDiffs: MultiAdapterDiff[];
  strategy: string;
  onStrategyChange: (s: string) => void;
}) {
  // Find the adapter by matching known adapterIds (which may contain colons, e.g. "claude-code:local")
  let adapterDiff: MultiAdapterDiff | undefined;
  let diff: DiffEntry | undefined;
  for (const mad of multiAdapterDiffs) {
    if (fileKey.startsWith(mad.adapterId + ":")) {
      const filePath = fileKey.substring(mad.adapterId.length + 1);
      const found = mad.diffs.find((d) => d.file_path === filePath);
      if (found) {
        adapterDiff = mad;
        diff = found;
        break;
      }
    }
  }

  if (!diff) {
    return <div className="text-text-muted">File not found</div>;
  }

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <div>
          <p className="text-sm font-medium text-text-primary">{diff.file_path}</p>
          <p className="text-xs text-text-muted">{adapterDiff?.adapterName}</p>
        </div>
        <select
          value={strategy}
          onChange={(e) => onStrategyChange(e.target.value)}
          className="px-3 py-1.5 bg-app-bg border border-border rounded text-sm text-text-primary"
        >
          <option value="merge">Merge</option>
          <option value="overwrite">Overwrite</option>
          <option value="skip">Skip</option>
        </select>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <div>
          <p className="text-xs text-text-muted uppercase mb-2">Current</p>
          <pre className="p-3 bg-app-bg rounded text-xs overflow-auto max-h-64 text-text-secondary">
            {diff.current_content || "(new file)"}
          </pre>
        </div>
        <div>
          <p className="text-xs text-text-muted uppercase mb-2">Proposed</p>
          <pre className="p-3 bg-app-bg rounded text-xs overflow-auto max-h-64 text-accent-green">
            {diff.proposed_content}
          </pre>
        </div>
      </div>
    </div>
  );
}

interface MultiAdapterSuccessProps {
  results: MultiAdapterResult[];
  projectName: string;
  capabilityCount: number;
  agentCount: number;
  onClose: () => void;
}

function MultiAdapterSuccess({
  results,
  projectName,
  capabilityCount,
  agentCount,
  onClose,
}: MultiAdapterSuccessProps) {
  const allSuccess = results.every((r) => r.result.success);

  const adapterColors: Record<string, string> = {
    "claude-code": "#9333ea",
    "cursor": "#3b82f6",
    "windsurf": "#22c55e",
    "gemini": "#4285f4",
  };
  const getColor = (id: string) => {
    if (adapterColors[id]) return adapterColors[id];
    return adapterColors[id.split(":")[0]] || "#666";
  };

  return (
    <div className="p-8 text-center">
      <div className={`w-16 h-16 rounded-full mx-auto mb-4 flex items-center justify-center ${
        allSuccess ? "bg-accent-green/20" : "bg-accent-orange/20"
      }`}>
        <span className={`text-3xl ${allSuccess ? "text-accent-green" : "text-accent-orange"}`}>
          {allSuccess ? "✓" : "!"}
        </span>
      </div>

      <h3 className="text-xl font-semibold text-text-primary mb-2">
        {allSuccess ? "Deployment Complete" : "Deployment Partial"}
      </h3>

      <p className="text-text-muted mb-6">
        {capabilityCount} capabilit{capabilityCount !== 1 ? "ies" : "y"} and {agentCount} agent{agentCount !== 1 ? "s" : ""} deployed to {projectName}
      </p>

      <div className="space-y-4 mb-6">
        {results.map((r) => (
          <div
            key={r.adapterId}
            className="p-4 bg-app-card border border-border rounded-lg text-left"
          >
            <div className="flex items-center gap-2 mb-2">
              <div
                className="w-3 h-3 rounded-full"
                style={{ backgroundColor: getColor(r.adapterId) }}
              />
              <span className="font-medium text-text-primary">{r.adapterName}</span>
              <span className={`text-xs px-2 py-0.5 rounded ${
                r.result.success
                  ? "bg-accent-green/20 text-accent-green"
                  : "bg-accent-red/20 text-accent-red"
              }`}>
                {r.result.success ? "Success" : "Failed"}
              </span>
            </div>
            <div className="text-xs text-text-muted">
              {r.result.files_written.length} file{r.result.files_written.length !== 1 ? "s" : ""} written
            </div>
            {r.result.errors.length > 0 && (
              <div className="mt-2 text-xs text-accent-red">
                {r.result.errors.join(", ")}
              </div>
            )}
          </div>
        ))}
      </div>

      <button
        onClick={onClose}
        data-testid="wizard-done"
        className="px-6 py-2 bg-accent-blue text-white rounded-lg font-medium hover:bg-accent-blue/80 transition-colors"
      >
        Done
      </button>
    </div>
  );
}
