import { useState } from "react";
import type { DeployResultResponse } from "../../lib/tauri";
import { invoke } from "@tauri-apps/api/core";
import { restoreProjectBackup, openInApp } from "../../lib/tauri";
import { fileManagerName } from "../../lib/platform";
import cursorIcon from "../../assets/cursor_logo.png";
import vscodeIcon from "../../assets/vs_code_logo.png";

interface DeploySuccessProps {
  result: DeployResultResponse;
  projectName: string;
  projectPath: string;
  capabilityCount: number;
  agentCount: number;
  onClose: () => void;
  backupId?: string;
}

export function DeploySuccess({
  result,
  projectName,
  projectPath,
  capabilityCount,
  agentCount,
  onClose,
  backupId,
}: DeploySuccessProps) {
  const [undoing, setUndoing] = useState(false);
  const [undoSuccess, setUndoSuccess] = useState(false);
  const [undoError, setUndoError] = useState<string | null>(null);

  const handleOpenInFinder = async () => {
    try {
      await invoke("plugin:opener|reveal_item_in_dir", { path: projectPath });
    } catch (error) {
      console.error(`Failed to open in ${fileManagerName}:`, error);
    }
  };

  const handleUndo = async () => {
    if (!backupId) return;
    
    setUndoing(true);
    setUndoError(null);
    
    try {
      await restoreProjectBackup(backupId);
      setUndoSuccess(true);
    } catch (error) {
      setUndoError(error instanceof Error ? error.message : "Failed to undo");
    } finally {
      setUndoing(false);
    }
  };

  if (!result.success) {
    return (
      <div className="p-8 text-center">
        <div className="w-16 h-16 mx-auto mb-4 rounded-full bg-accent-red/20 flex items-center justify-center">
          <span className="text-3xl">✕</span>
        </div>
        <h3 className="text-xl font-semibold text-text-primary mb-2">Deploy Failed</h3>
        <p className="text-text-muted mb-4">Some files could not be written.</p>
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

  if (undoSuccess) {
    return (
      <div className="p-8 text-center">
        <div className="w-16 h-16 mx-auto mb-4 rounded-full bg-accent-orange/20 flex items-center justify-center">
          <span className="text-3xl text-accent-orange">↩</span>
        </div>
        <h3 className="text-xl font-semibold text-text-primary mb-2">Deployment Undone</h3>
        <p className="text-text-muted mb-6">
          Files have been restored to their previous state.
        </p>
        <button
          onClick={onClose}
          className="px-6 py-2 bg-accent-blue text-white rounded-lg font-medium hover:bg-accent-blue/80 transition-colors"
        >
          Done
        </button>
      </div>
    );
  }

  const itemText =
    capabilityCount > 0 && agentCount > 0
      ? `${capabilityCount} capabilities and ${agentCount} agents`
      : capabilityCount > 0
      ? `${capabilityCount} capabilit${capabilityCount === 1 ? "y" : "ies"}`
      : `${agentCount} agent${agentCount === 1 ? "" : "s"}`;

  return (
    <div className="p-8 text-center">
      <div className="w-16 h-16 mx-auto mb-4 rounded-full bg-accent-green/20 flex items-center justify-center animate-pulse">
        <span className="text-3xl text-accent-green">✓</span>
      </div>

      <h3 className="text-xl font-semibold text-text-primary mb-2">Deploy Successful!</h3>
      <p className="text-text-muted mb-6">
        {itemText} deployed to <span className="font-semibold text-text-primary">{projectName}</span>
      </p>

      <div className="bg-app-card border border-border rounded-lg p-4 mb-6 text-left">
        <p className="text-xs text-text-muted uppercase mb-2">Files Written</p>
        <div className="space-y-1 max-h-40 overflow-y-auto">
          {result.files_written.map((file, i) => (
            <p key={i} className="text-sm font-mono text-text-primary flex items-center gap-2">
              <span className="w-2 h-2 rounded-full bg-accent-green" />
              {file}
            </p>
          ))}
        </div>
      </div>

      {undoError && (
        <div className="mb-4 p-3 bg-accent-red/10 border border-accent-red/30 rounded-lg text-accent-red text-sm">
          {undoError}
        </div>
      )}

      <div className="flex items-center justify-center gap-2 flex-wrap">
        <button
          onClick={handleOpenInFinder}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-border text-xs text-text-secondary hover:text-text-primary hover:border-text-muted transition-colors"
        >
          <span>📁</span>
          <span>{fileManagerName}</span>
        </button>
        <button
          onClick={() => openInApp(projectPath, "Cursor").catch(() => {})}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-border text-xs text-text-secondary hover:text-text-primary hover:border-text-muted transition-colors"
        >
          <img src={cursorIcon} alt="Cursor" className="w-3.5 h-3.5 object-contain" />
          <span>Cursor</span>
        </button>
        <button
          onClick={() => openInApp(projectPath, "Visual Studio Code").catch(() => {})}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg border border-border text-xs text-text-secondary hover:text-text-primary hover:border-text-muted transition-colors"
        >
          <img src={vscodeIcon} alt="VS Code" className="w-3.5 h-3.5 object-contain" />
          <span>VS Code</span>
        </button>
        {backupId && (
          <button
            onClick={handleUndo}
            disabled={undoing}
            className="px-4 py-2 text-sm text-accent-orange hover:text-accent-orange/80 transition-colors flex items-center gap-2 disabled:opacity-50"
          >
            {undoing ? "Undoing..." : "↩ Undo Deploy"}
          </button>
        )}
        <button
          onClick={onClose}
          className="px-6 py-2 bg-accent-blue text-white rounded-lg font-medium hover:bg-accent-blue/80 transition-colors"
        >
          Done
        </button>
      </div>
    </div>
  );
}
