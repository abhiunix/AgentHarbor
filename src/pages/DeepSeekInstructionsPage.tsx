import { useState, useEffect, useCallback } from "react";
import {
  listDeepSeekInstructionFiles,
  readDeepSeekInstruction,
  writeDeepSeekInstruction,
} from "../lib/tauri";
import type { DeepSeekInstructionFile } from "../lib/tauri";
import { DebugPath } from "../components/common/DebugPath";

export function DeepSeekInstructionsPage() {
  const [files, setFiles] = useState<DeepSeekInstructionFile[]>([]);
  const [selected, setSelected] = useState<DeepSeekInstructionFile | null>(null);
  const [content, setContent] = useState("");
  const [savedContent, setSavedContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [loadingContent, setLoadingContent] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadFiles = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await listDeepSeekInstructionFiles();
      setFiles(result);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadFiles();
  }, [loadFiles]);

  const openFile = useCallback(async (file: DeepSeekInstructionFile) => {
    setSelected(file);
    setLoadingContent(true);
    setError(null);
    try {
      const text = await readDeepSeekInstruction(file.abs_path);
      setContent(text);
      setSavedContent(text);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoadingContent(false);
    }
  }, []);

  const isDirty = selected != null && content !== savedContent;

  const handleSave = async () => {
    if (!selected) return;
    setSaving(true);
    setError(null);
    try {
      await writeDeepSeekInstruction(selected.abs_path, content);
      setSavedContent(content);
      await loadFiles();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const tokenEstimate = Math.ceil(content.length / 4);

  return (
    <div className="h-full flex flex-col">
      <div className="p-6 border-b border-border flex items-center justify-between flex-wrap gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-text-primary mb-1">Instructions</h1>
          <p className="text-sm text-text-secondary">
            DeepSeek Harness has no global instructions file — AGENTS.md is per-workspace.
          </p>
          {selected && <DebugPath path={selected.abs_path} className="text-sm" />}
        </div>
        {selected && (
          <div className="flex items-center gap-2">
            {!loadingContent && content.length > 0 && (
              <span className="text-xs text-text-muted bg-app-card border border-border px-2 py-0.5 rounded font-mono">
                ~{tokenEstimate.toLocaleString()} tokens
              </span>
            )}
            {isDirty && (
              <span className="text-xs text-amber-400 font-medium">Unsaved changes</span>
            )}
            <button
              onClick={() => openFile(selected)}
              disabled={loadingContent}
              className="text-sm text-accent-blue hover:underline disabled:opacity-50"
            >
              Refresh
            </button>
            <button
              onClick={handleSave}
              disabled={loadingContent || saving || !isDirty}
              className="px-4 py-2 rounded-lg bg-accent-blue text-white text-sm font-medium hover:bg-accent-blue/90 disabled:opacity-50"
            >
              {saving ? "Saving..." : "Save"}
            </button>
          </div>
        )}
      </div>

      <div className="flex-1 overflow-hidden flex">
        <div className="w-72 flex-shrink-0 border-r border-border overflow-y-auto p-4 space-y-2">
          {loading ? (
            <p className="text-sm text-text-muted">Loading workspaces...</p>
          ) : files.length === 0 ? (
            <p className="text-sm text-text-muted">No DeepSeek workspaces found.</p>
          ) : (
            files.map((file) => {
              const isSelected = selected?.abs_path === file.abs_path;
              return (
                <button
                  key={file.abs_path}
                  onClick={() => openFile(file)}
                  className={`w-full text-left px-3 py-2 rounded-lg border transition-colors ${
                    isSelected
                      ? "border-accent-blue bg-accent-blue/10"
                      : "border-border bg-app-card hover:bg-app-card-hover"
                  }`}
                >
                  <p className="text-sm font-medium text-text-primary truncate">
                    {file.project_name}
                  </p>
                  <p className="text-xs text-text-secondary truncate mt-0.5">
                    {file.abs_path}
                  </p>
                  <p className="text-xs mt-1">
                    {file.exists ? (
                      <span className="text-accent-blue">
                        Present {file.size_bytes != null ? formatSize(file.size_bytes) : ""}
                      </span>
                    ) : (
                      <span className="text-text-muted">Not created</span>
                    )}
                  </p>
                </button>
              );
            })
          )}
        </div>

        <div className="flex-1 overflow-hidden p-6 flex flex-col gap-4">
          {error && <p className="text-sm text-accent-red">{error}</p>}
          {!selected ? (
            <div className="h-64 flex items-center justify-center text-text-muted text-sm">
              Select a workspace to view or create its AGENTS.md.
            </div>
          ) : loadingContent ? (
            <div className="h-64 flex items-center justify-center text-text-muted">Loading...</div>
          ) : (
            <>
              {!selected.exists && (
                <p className="text-xs text-text-muted">
                  This workspace has no AGENTS.md yet. Start typing and Save to create it.
                </p>
              )}
              <textarea
                value={content}
                onChange={(e) => setContent(e.target.value)}
                className="w-full flex-1 min-h-[200px] px-4 py-3 bg-app-card border border-border rounded-lg font-mono text-sm text-text-primary focus:outline-none focus:border-accent-blue resize-none"
                placeholder="# AGENTS.md&#10;&#10;Add workspace-specific instructions for DeepSeek Harness."
              />
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
