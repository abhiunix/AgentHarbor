import { useMemo, useState } from "react";
import type { AgentModel, ImportableAgent, ImportStatus } from "../../lib/types";
import { getModelLabel } from "../../lib/types";
import { importAgentsFromDir } from "../../lib/tauri";

interface ImportAgentsModalProps {
  path: string;
  candidates: ImportableAgent[];
  onClose: () => void;
  onImported: (importedCount: number) => void;
}

const modelBgColors: Record<AgentModel, string> = {
  haiku: "bg-accent-cyan/20 text-accent-cyan",
  sonnet: "bg-accent-purple/20 text-accent-purple",
  opus: "bg-accent-orange/20 text-accent-orange",
};

const statusStyles: Record<ImportStatus, string> = {
  new: "bg-accent-green/15 text-accent-green",
  "duplicate-id": "bg-accent-yellow/15 text-accent-yellow",
  "content-match": "bg-text-muted/20 text-text-muted",
};

const statusLabels: Record<ImportStatus, string> = {
  new: "New",
  "duplicate-id": "Duplicate ID",
  "content-match": "Content match",
};

const toolLabels: Record<string, string> = {
  "claude-code": "Claude Code",
  cursor: "Cursor",
  gemini: "Gemini CLI",
  codex: "Codex",
};

export function ImportAgentsModal({
  path,
  candidates,
  onClose,
  onImported,
}: ImportAgentsModalProps) {
  const [selected, setSelected] = useState<Set<string>>(
    () => new Set(candidates.filter((c) => c.status === "new").map((c) => c.source_path))
  );
  const [renameOnConflict, setRenameOnConflict] = useState(false);
  const [importing, setImporting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const includeCodex = useMemo(
    () => candidates.some((c) => c.source_tool === "codex"),
    [candidates]
  );

  const groups = useMemo(() => {
    const map = new Map<string, ImportableAgent[]>();
    for (const c of candidates) {
      const list = map.get(c.source_tool) ?? [];
      list.push(c);
      map.set(c.source_tool, list);
    }
    return Array.from(map.entries());
  }, [candidates]);

  const toggle = (sourcePath: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(sourcePath)) {
        next.delete(sourcePath);
      } else {
        next.add(sourcePath);
      }
      return next;
    });
  };

  const toggleGroup = (items: ImportableAgent[], allSelected: boolean) => {
    setSelected((prev) => {
      const next = new Set(prev);
      for (const item of items) {
        if (allSelected) {
          next.delete(item.source_path);
        } else {
          next.add(item.source_path);
        }
      }
      return next;
    });
  };

  const handleImport = async () => {
    setImporting(true);
    setError(null);
    try {
      const result = await importAgentsFromDir(
        path,
        Array.from(selected),
        includeCodex,
        renameOnConflict
      );
      onImported(result.imported);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setImporting(false);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/60 z-50 flex items-center justify-center p-4">
      <div className="bg-app-sidebar border border-border rounded-lg w-full max-w-2xl max-h-[90vh] flex flex-col">
        <div className="flex items-center justify-between p-4 border-b border-border">
          <div>
            <h2 className="text-lg font-semibold text-text-primary">Import Agents</h2>
            <p className="text-xs text-text-muted mt-0.5 font-mono truncate max-w-md">{path}</p>
          </div>
          <button
            onClick={onClose}
            className="w-8 h-8 flex items-center justify-center rounded-md hover:bg-app-card-hover text-text-muted hover:text-text-primary transition-colors"
          >
            ✕
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-4 space-y-5">
          {candidates.length === 0 ? (
            <div className="text-center py-10">
              <p className="text-text-muted">No importable agents found in this folder.</p>
            </div>
          ) : (
            groups.map(([tool, items]) => {
              const allSelected = items.every((i) => selected.has(i.source_path));
              return (
                <div key={tool}>
                  <div className="flex items-center justify-between mb-2">
                    <h3 className="text-sm font-semibold text-text-primary">
                      {toolLabels[tool] ?? tool}{" "}
                      <span className="text-text-muted font-normal">({items.length})</span>
                    </h3>
                    <button
                      type="button"
                      onClick={() => toggleGroup(items, allSelected)}
                      className="text-xs text-accent-blue hover:underline"
                    >
                      {allSelected ? "Deselect all" : "Select all"}
                    </button>
                  </div>

                  <div className="space-y-1.5">
                    {items.map((item) => {
                      const checked = selected.has(item.source_path);
                      return (
                        <label
                          key={item.source_path}
                          className="flex items-start gap-3 p-2.5 rounded-md bg-app-card border border-border hover:bg-app-card-hover cursor-pointer"
                        >
                          <input
                            type="checkbox"
                            checked={checked}
                            onChange={() => toggle(item.source_path)}
                            className="mt-1 accent-accent-blue"
                          />
                          <div className="min-w-0 flex-1">
                            <div className="flex items-center gap-2 flex-wrap">
                              <span className="text-sm text-text-primary font-medium truncate">
                                {item.agent.name}
                              </span>
                              <span
                                className={`text-[10px] font-semibold px-1.5 py-0.5 rounded ${modelBgColors[item.agent.model]}`}
                              >
                                {getModelLabel(item.agent.model).toUpperCase()}
                              </span>
                              {item.model_defaulted && (
                                <span
                                  className="text-[10px] px-1.5 py-0.5 rounded bg-accent-orange/10 text-accent-orange"
                                  title="The source model wasn't an exact match and was defaulted to Sonnet."
                                >
                                  defaulted
                                </span>
                              )}
                              <span
                                className={`text-[10px] px-1.5 py-0.5 rounded ${statusStyles[item.status]}`}
                              >
                                {statusLabels[item.status]}
                              </span>
                            </div>
                            <p className="text-[11px] font-mono text-text-muted truncate mt-0.5">
                              {item.source_path}
                            </p>
                          </div>
                        </label>
                      );
                    })}
                  </div>
                </div>
              );
            })
          )}
        </div>

        <div className="p-4 border-t border-border space-y-3">
          {error && <p className="text-xs text-accent-red">{error}</p>}
          <div className="flex items-center justify-between gap-4">
            <label className="flex items-center gap-2 text-xs text-text-secondary cursor-pointer">
              <input
                type="checkbox"
                checked={renameOnConflict}
                onChange={(e) => setRenameOnConflict(e.target.checked)}
                className="accent-accent-blue"
              />
              Rename on ID conflict instead of skipping
            </label>
            <div className="flex items-center gap-2">
              <button
                onClick={onClose}
                className="h-9 px-4 rounded-md bg-app-card border border-border text-sm text-text-primary hover:bg-app-card-hover transition-colors"
              >
                Cancel
              </button>
              <button
                onClick={handleImport}
                disabled={selected.size === 0 || importing}
                className="h-9 px-4 rounded-md bg-accent-blue text-white text-sm font-medium hover:bg-accent-blue/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {importing ? "Importing…" : `Import ${selected.size} agent${selected.size === 1 ? "" : "s"}`}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
