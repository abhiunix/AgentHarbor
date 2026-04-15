import { useState } from "react";
import { ProjectSelector } from "./ProjectSelector";
import { AgentDeployReview } from "./AgentDeployReview";
import { DeployPreview } from "./DeployPreview";
import { useProjectStore, getProjectName } from "../../stores/projectStore";
import { previewDeploy, executeDeploy, recordDeployment, DiffEntry, DeployResultResponse, openProjectInFinder, openProjectInCursor, openProjectInVscode, type ClaudeSettingsTarget } from "../../lib/tauri";
import { PROJECTS_RELOAD_EVENT } from "../projects/ProjectList";
import type { AgentDefinition } from "../../lib/types";
import { fileManagerName } from "../../lib/platform";

export type AgentDeployStep = "project" | "adapter" | "review" | "preview" | "success";

const ADAPTERS = [
  { id: "claude-code", name: "Claude Code", icon: "🟡" },
  { id: "cursor", name: "Cursor", icon: "🟣" },
  { id: "windsurf", name: "Windsurf", icon: "🔵" },
];

interface AgentDeployWizardProps {
  isOpen: boolean;
  onClose: () => void;
  agent: AgentDefinition;
}

export function AgentDeployWizard({
  isOpen,
  onClose,
  agent,
}: AgentDeployWizardProps) {
  const [step, setStep] = useState<AgentDeployStep>("project");
  const [selectedAdapter, setSelectedAdapter] = useState("claude-code");
  const [claudeSettingsTargets, setClaudeSettingsTargets] = useState<Set<ClaudeSettingsTarget>>(new Set(["local"]));
  const [diffEntries, setDiffEntries] = useState<DiffEntry[]>([]);
  const [strategies, setStrategies] = useState<Record<string, string>>({});
  const [deployResult, setDeployResult] = useState<DeployResultResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const { selectedProject, clearSelection } = useProjectStore();

  if (!isOpen) return null;

  const handleClose = () => {
    setStep("project");
    setSelectedAdapter("claude-code");
    setDiffEntries([]);
    setStrategies({});
    setDeployResult(null);
    setError(null);
    clearSelection();
    onClose();
  };

  const handleProjectSelected = () => {
    setStep("adapter");
  };

  const handleAdapterSelected = () => {
    setStep("review");
  };

  const handleReviewComplete = async () => {
    setLoading(true);
    setError(null);

    try {
      const capabilityIds = agent.required_capabilities || [];
      const allDiffs: DiffEntry[] = [];
      if (selectedAdapter === "claude-code") {
        for (const target of Array.from(claudeSettingsTargets)) {
          const diffs = await previewDeploy(selectedProject!, selectedAdapter, capabilityIds, [agent.id], target);
          allDiffs.push(...diffs);
        }
      } else {
        const diffs = await previewDeploy(selectedProject!, selectedAdapter, capabilityIds, [agent.id]);
        allDiffs.push(...diffs);
      }
      setDiffEntries(allDiffs);
      const initialStrategies: Record<string, string> = {};
      allDiffs.forEach((d) => { initialStrategies[d.file_path] = "merge"; });
      setStrategies(initialStrategies);
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
      const capabilityIds = agent.required_capabilities || [];
      const projectPath = selectedProject!;
      let lastResult: DeployResultResponse | null = null;
      let recorded = false;
      if (selectedAdapter === "claude-code") {
        for (const target of Array.from(claudeSettingsTargets)) {
          lastResult = await executeDeploy(projectPath, selectedAdapter, capabilityIds, [agent.id], strategies, target);
          if (lastResult.success) {
            try {
              await recordDeployment(projectPath, selectedAdapter, capabilityIds, [agent.id]);
              recorded = true;
            } catch (e) {
              console.error("recordDeployment failed:", e);
            }
          }
        }
      } else {
        lastResult = await executeDeploy(projectPath, selectedAdapter, capabilityIds, [agent.id], strategies);
        if (lastResult.success) {
          try {
            await recordDeployment(projectPath, selectedAdapter, capabilityIds, [agent.id]);
            recorded = true;
          } catch (e) {
            console.error("recordDeployment failed:", e);
          }
        }
      }
      setDeployResult(lastResult!);
      setStep("success");
      if (recorded) {
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

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-app-sidebar border border-border rounded-xl w-full max-w-3xl max-h-[85vh] flex flex-col shadow-2xl">
        <div className="flex items-center justify-between px-6 py-4 border-b border-border">
          <div className="flex items-center gap-3">
            <h2 className="text-lg font-semibold text-text-primary">Deploy Agent</h2>
            <AgentStepIndicator currentStep={step} />
          </div>
          <button
            onClick={handleClose}
            className="w-8 h-8 flex items-center justify-center rounded-full hover:bg-white/10 text-text-muted hover:text-text-primary transition-colors"
          >
            ✕
          </button>
        </div>

        <div className="flex-1 overflow-y-auto">
          {error && (
            <div className="mx-6 mt-4 p-3 bg-accent-red/10 border border-accent-red/30 rounded-lg text-accent-red text-sm">
              {error}
            </div>
          )}

          {step === "project" && (
            <div className="p-6">
              <p className="text-text-muted mb-4">Select a project folder to deploy the agent to.</p>
              <ProjectSelector onProjectSelected={handleProjectSelected} />
            </div>
          )}

          {step === "adapter" && (
            <div className="p-6">
              <p className="text-text-muted mb-4">Select which IDE/agent to deploy to.</p>
              <div className="space-y-2 mb-6">
                {ADAPTERS.map((adapter) => (
                  <button
                    key={adapter.id}
                    onClick={() => setSelectedAdapter(adapter.id)}
                    className={`w-full flex items-center gap-3 px-4 py-3 rounded-lg border transition-colors ${
                      selectedAdapter === adapter.id
                        ? "border-accent-purple bg-accent-purple/10 text-text-primary"
                        : "border-border bg-app-card hover:bg-app-card-hover text-text-secondary"
                    }`}
                  >
                    <span className="text-xl">{adapter.icon}</span>
                    <span className="font-medium">{adapter.name}</span>
                  </button>
                ))}
              </div>
              {selectedAdapter === "claude-code" && (
                <div className="flex items-center gap-4 mb-6">
                  <p className="text-xs text-text-muted uppercase whitespace-nowrap">Claude Code settings:</p>
                  {[
                    { value: "local" as const, label: "Project settings (local)", sub: ".claude/settings.local.json" },
                    { value: "project" as const, label: "Project settings", sub: ".claude/settings.json" },
                  ].map((opt) => (
                    <label
                      key={opt.value}
                      className="flex items-center gap-1.5 cursor-pointer"
                    >
                      <input
                        type="checkbox"
                        checked={claudeSettingsTargets.has(opt.value)}
                        onChange={() => {
                          const next = new Set(claudeSettingsTargets);
                          if (next.has(opt.value)) {
                            if (next.size > 1) next.delete(opt.value);
                          } else {
                            next.add(opt.value);
                          }
                          setClaudeSettingsTargets(next);
                        }}
                        className="w-3.5 h-3.5 rounded accent-accent-purple"
                      />
                      <span className={`text-sm ${claudeSettingsTargets.has(opt.value) ? "text-text-primary" : "text-text-secondary"}`}>
                        {opt.label}
                      </span>
                      <span className="text-[10px] text-text-muted font-mono">{opt.sub}</span>
                    </label>
                  ))}
                </div>
              )}
              <div className="flex justify-between">
                <button
                  onClick={() => setStep("project")}
                  className="px-4 py-2 text-sm text-text-muted hover:text-text-primary transition-colors"
                >
                  Back
                </button>
                <button
                  onClick={handleAdapterSelected}
                  className="px-6 py-2 bg-accent-purple text-white rounded-lg font-medium hover:bg-accent-purple/80 transition-colors"
                >
                  Continue
                </button>
              </div>
            </div>
          )}

          {step === "review" && (
            <AgentDeployReview
              agent={agent}
              onContinue={handleReviewComplete}
              onBack={() => setStep("adapter")}
              loading={loading}
            />
          )}

          {step === "preview" && (
            <DeployPreview
              diffs={diffEntries}
              strategies={strategies}
              onStrategyChange={handleStrategyChange}
              onDeploy={handleDeploy}
              onBack={() => setStep("review")}
              loading={loading}
            />
          )}

          {step === "success" && deployResult && (
            <AgentDeploySuccess
              result={deployResult}
              agent={agent}
              projectName={getProjectName(selectedProject || "")}
              projectPath={selectedProject || ""}
              onClose={handleClose}
            />
          )}
        </div>
      </div>
    </div>
  );
}

function AgentStepIndicator({ currentStep }: { currentStep: AgentDeployStep }) {
  const steps: { key: AgentDeployStep; label: string }[] = [
    { key: "project", label: "1" },
    { key: "adapter", label: "2" },
    { key: "review", label: "3" },
    { key: "preview", label: "4" },
    { key: "success", label: "5" },
  ];

  const currentIndex = steps.findIndex((s) => s.key === currentStep);

  return (
    <div className="flex items-center gap-1">
      {steps.map((step, i) => (
        <div
          key={step.key}
          className={`w-6 h-6 rounded-full flex items-center justify-center text-xs font-medium ${
            i === currentIndex
              ? "bg-accent-purple text-white"
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

function AgentDeploySuccess({
  result,
  agent,
  projectName,
  projectPath,
  onClose,
}: {
  result: DeployResultResponse;
  agent: AgentDefinition;
  projectName: string;
  projectPath: string;
  onClose: () => void;
}) {
  const colorMap: Record<string, string> = {
    red: "#ef4444",
    blue: "#3b82f6",
    green: "#22c55e",
    yellow: "#eab308",
    purple: "#9333ea",
    orange: "#f97316",
    pink: "#ec4899",
    cyan: "#06b6d4",
  };

  const handleOpenInFinder = async () => {
    try {
      await openProjectInFinder(projectPath);
    } catch (error) {
      console.error(`Failed to open in ${fileManagerName}:`, error);
    }
  };

  const handleOpenInCursor = async () => {
    try {
      await openProjectInCursor(projectPath);
    } catch (error) {
      console.error("Failed to open in Cursor:", error);
    }
  };

  const handleOpenInVscode = async () => {
    try {
      await openProjectInVscode(projectPath);
    } catch (error) {
      console.error("Failed to open in VS Code:", error);
    }
  };

  if (!result.success) {
    return (
      <div className="p-8 text-center">
        <div className="w-16 h-16 mx-auto mb-4 rounded-full bg-accent-red/20 flex items-center justify-center">
          <span className="text-3xl">✕</span>
        </div>
        <h3 className="text-xl font-semibold text-text-primary mb-2">Deploy Failed</h3>
        <p className="text-text-muted mb-4">Could not deploy the agent.</p>
        <div className="bg-accent-red/10 border border-accent-red/30 rounded-lg p-4 text-left max-h-40 overflow-y-auto">
          {result.errors.map((error, i) => (
            <p key={i} className="text-sm text-accent-red font-mono">
              {error}
            </p>
          ))}
        </div>
        <button
          onClick={onClose}
          className="mt-6 px-6 py-2 bg-app-card border border-border rounded-lg text-text-primary hover:bg-app-card-hover transition-colors"
        >
          Close
        </button>
      </div>
    );
  }

  const requiredCapsCount = agent.required_capabilities?.length || 0;

  return (
    <div className="p-8 text-center">
      <div
        className="w-16 h-16 mx-auto mb-4 rounded-full flex items-center justify-center animate-pulse"
        style={{ backgroundColor: `${colorMap[agent.color] || "#3b82f6"}20` }}
      >
        <span className="text-3xl">🤖</span>
      </div>

      <h3 className="text-xl font-semibold text-text-primary mb-2">
        @{agent.name} is now available!
      </h3>
      <p className="text-text-muted mb-6">
        Agent deployed to <span className="font-semibold text-text-primary">{projectName}</span>
      </p>

      <div className="bg-app-card border border-border rounded-lg p-4 mb-4 text-left">
        <p className="text-xs text-text-muted uppercase mb-2">Files Created</p>
        <div className="space-y-1 max-h-32 overflow-y-auto">
          {result.files_written.map((file, i) => (
            <p key={i} className="text-sm font-mono text-text-primary flex items-center gap-2">
              <span
                className="w-2 h-2 rounded-full"
                style={{
                  backgroundColor: file.includes("agents/")
                    ? "#ec4899"
                    : "#22c55e",
                }}
              />
              {file}
            </p>
          ))}
        </div>
      </div>

      {requiredCapsCount > 0 && (
        <p className="text-xs text-text-muted mb-4">
          + {requiredCapsCount} capabilit{requiredCapsCount === 1 ? "y" : "ies"} configured
        </p>
      )}

      <div className="bg-accent-blue/10 border border-accent-blue/30 rounded-lg p-3 mb-6 text-sm text-text-muted">
        💡 Works with Claude Code & Cursor (shared <code className="font-mono">.claude/</code> directory)
      </div>

      <div className="flex items-center justify-center gap-4 flex-wrap">
        <span className="text-sm text-text-muted">Open in</span>
        <div className="flex items-center gap-1">
          <button
            type="button"
            onClick={handleOpenInFinder}
            title={`Open in ${fileManagerName}`}
            className="w-9 h-9 flex items-center justify-center rounded-lg bg-white/5 border border-border text-text-muted hover:bg-white/10 hover:text-text-primary transition-colors"
            aria-label={`Open in ${fileManagerName}`}
          >
            <span className="text-lg" role="img" aria-hidden>📁</span>
          </button>
          <button
            type="button"
            onClick={handleOpenInCursor}
            title="Open in Cursor"
            className="w-9 h-9 flex items-center justify-center rounded-lg bg-white/5 border border-border text-text-muted hover:bg-white/10 hover:text-text-primary transition-colors"
            aria-label="Open in Cursor"
          >
            <span className="text-sm font-semibold text-[#9333ea]" role="img" aria-hidden>C</span>
          </button>
          <button
            type="button"
            onClick={handleOpenInVscode}
            title="Open in VS Code"
            className="w-9 h-9 flex items-center justify-center rounded-lg bg-white/5 border border-border text-text-muted hover:bg-white/10 hover:text-text-primary transition-colors"
            aria-label="Open in VS Code"
          >
            <span className="text-sm font-semibold text-[#007acc]" role="img" aria-hidden>V</span>
          </button>
        </div>
        <button
          onClick={onClose}
          className="px-6 py-2 bg-accent-purple text-white rounded-lg font-medium hover:bg-accent-purple/80 transition-colors"
        >
          Done
        </button>
      </div>
    </div>
  );
}
