import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";
import { DebugPath } from "../components/common/DebugPath";

interface CodexInstructionSource {
  path: string;
  kind: string;
  exists: boolean;
  loaded: boolean;
  truncated: boolean;
}

interface CodexInstructionsResult {
  scope: string;
  path: string;
  content: string;
  exists: boolean;
  revision: string;
  instructionSources: CodexInstructionSource[];
}

function fileName(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  return normalized.slice(normalized.lastIndexOf("/") + 1);
}

function parentPath(path: string): string {
  const normalized = path.replace(/\\/g, "/");
  const separator = normalized.lastIndexOf("/");
  return separator > 0 ? normalized.slice(0, separator) : normalized;
}

function formatKind(kind: string): string {
  if (!kind.trim()) return "Instruction file";
  return kind
    .replace(/[_-]+/g, " ")
    .replace(/\b\w/g, (character) => character.toUpperCase());
}

export function CodexInstructionsPage() {
  const [projectScope, setProjectScope] = useState<string | null>(null);
  const [result, setResult] = useState<CodexInstructionsResult | null>(null);
  const [content, setContent] = useState("");
  const [savedContent, setSavedContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const requestId = useRef(0);
  const scopeVersion = useRef(0);

  const isGlobal = projectScope == null;
  const fallbackPath = isGlobal
    ? "Codex home/AGENTS.md"
    : `${projectScope}/AGENTS.md`;
  const targetPath = result?.path || fallbackPath;

  const load = useCallback(async () => {
    const currentRequest = ++requestId.current;
    setLoading(true);
    setError(null);
    setSuccess(null);
    try {
      const response = await invoke<CodexInstructionsResult>(
        "read_codex_instructions",
        { projectPath: projectScope },
      );
      if (currentRequest !== requestId.current) return;
      setResult(response);
      setContent(response.content);
      setSavedContent(response.content);
    } catch (loadError) {
      if (currentRequest !== requestId.current) return;
      setError(
        loadError instanceof Error ? loadError.message : String(loadError),
      );
    } finally {
      if (currentRequest === requestId.current) setLoading(false);
    }
  }, [projectScope]);

  useEffect(() => {
    void load();
  }, [load]);

  const isDirty = content !== savedContent;
  const tokenEstimate = Math.ceil(content.length / 4);

  const highestLoadedSource = useMemo(() => {
    const loaded =
      result?.instructionSources.filter((source) => source.loaded) ?? [];
    return loaded.length > 0 ? loaded[loaded.length - 1].path : null;
  }, [result]);

  const targetHasOverride = useMemo(() => {
    if (!result) return false;
    const targetDirectory = parentPath(targetPath);
    return result.instructionSources.some(
      (source) =>
        source.exists &&
        fileName(source.path) === "AGENTS.override.md" &&
        parentPath(source.path) === targetDirectory,
    );
  }, [result, targetPath]);

  const handleProjectScopeChange = (nextScope: string | null) => {
    if (nextScope === projectScope || saving) return;
    if (
      isDirty &&
      !window.confirm(
        "Discard unsaved Codex instruction changes and switch scope?",
      )
    ) {
      return;
    }
    scopeVersion.current += 1;
    requestId.current += 1;
    setProjectScope(nextScope);
    setResult(null);
    setContent("");
    setSavedContent("");
    setError(null);
    setSuccess(null);
    setLoading(true);
  };

  const handleRefresh = () => {
    if (
      isDirty &&
      !window.confirm(
        "Discard unsaved Codex instruction changes and refresh from disk?",
      )
    ) {
      return;
    }
    void load();
  };

  const handleSave = async () => {
    const saveScopeVersion = scopeVersion.current;
    setSaving(true);
    setError(null);
    setSuccess(null);
    try {
      const response = await invoke<CodexInstructionsResult>(
        "write_codex_instructions",
        {
          projectPath: projectScope,
          content,
          expectedRevision: result?.revision ?? null,
        },
      );
      if (saveScopeVersion !== scopeVersion.current) return;
      setResult(response);
      setContent(response.content);
      setSavedContent(response.content);
      setSuccess("Codex instructions saved.");
    } catch (saveError) {
      if (saveScopeVersion === scopeVersion.current) {
        setError(
          saveError instanceof Error ? saveError.message : String(saveError),
        );
      }
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="h-full flex flex-col">
      <div className="p-6 border-b border-border flex items-center justify-between flex-wrap gap-3 shrink-0">
        <div className="min-w-0">
          <h1 className="text-2xl font-semibold text-text-primary mb-1">
            Instructions
          </h1>
          <div className="flex items-center gap-2 flex-wrap">
            <DebugPath path={targetPath} className="text-sm" />
            {!loading && (
              <span
                className={`text-[10px] font-medium px-2 py-0.5 rounded border ${
                  result?.exists
                    ? "border-accent-green/30 bg-accent-green/10 text-accent-green"
                    : "border-border bg-app-card text-text-muted"
                }`}
              >
                {result?.exists ? "Existing file" : "Created on save"}
              </span>
            )}
            {!loading && content.length > 0 && (
              <span className="text-xs text-text-muted bg-app-card border border-border px-2 py-0.5 rounded font-mono">
                ~{tokenEstimate.toLocaleString()} tokens
              </span>
            )}
          </div>
        </div>

        <div className="flex items-center gap-2 flex-wrap justify-end">
          <ProjectScopeSelector
            value={projectScope}
            onChange={handleProjectScopeChange}
          />
          {isDirty && (
            <span className="text-xs text-amber-400 font-medium">
              Unsaved changes
            </span>
          )}
          <button
            type="button"
            onClick={handleRefresh}
            disabled={loading || saving}
            className="text-sm text-accent-blue hover:underline disabled:opacity-50"
          >
            Refresh
          </button>
          <button
            type="button"
            onClick={() => void handleSave()}
            disabled={loading || saving || !isDirty || !result}
            className="px-4 py-2 rounded-lg bg-accent-blue text-white text-sm font-medium hover:bg-accent-blue/90 disabled:opacity-50"
          >
            {saving ? "Saving..." : "Save"}
          </button>
        </div>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto p-6 flex flex-col gap-4">
        {error && (
          <div
            role="alert"
            aria-live="assertive"
            className="flex items-start justify-between gap-4 rounded-lg border border-accent-red/30 bg-accent-red/10 px-4 py-3"
          >
            <div>
              <p className="text-sm font-medium text-accent-red">
                Codex instructions error
              </p>
              <p className="text-xs text-accent-red/80 mt-1 break-words">
                {error}
              </p>
            </div>
            <button
              type="button"
              onClick={handleRefresh}
              disabled={loading}
              className="text-xs text-accent-red hover:underline disabled:opacity-50 shrink-0"
            >
              Try again
            </button>
          </div>
        )}

        {success && (
          <div
            role="status"
            aria-live="polite"
            className="rounded-lg border border-accent-green/30 bg-accent-green/10 px-4 py-3 text-sm text-accent-green"
          >
            {success}
          </div>
        )}

        {loading ? (
          <div className="flex-1 min-h-64 flex items-center justify-center text-text-muted">
            Loading Codex instructions...
          </div>
        ) : (
          <>
            <section className="bg-app-card border border-border rounded-lg p-4 shrink-0">
              <div className="flex items-start justify-between gap-4 mb-3">
                <div>
                  <h2 className="text-sm font-semibold text-text-primary">
                    Instruction source chain
                  </h2>
                  <p className="text-xs text-text-muted mt-1 leading-relaxed">
                    Codex loads global instructions first, then project
                    instructions from the project root toward the selected
                    directory. Files later in this list have higher precedence
                    when instructions conflict.
                  </p>
                </div>
                <span className="text-[10px] uppercase tracking-wider text-text-muted border border-border rounded px-2 py-1 shrink-0">
                  {result?.scope || (isGlobal ? "Global" : "Project")}
                </span>
              </div>

              <div className="rounded-md border border-accent-purple/30 bg-accent-purple/10 px-3 py-2.5 mb-3">
                <p className="text-xs text-text-secondary leading-relaxed">
                  <code className="font-mono text-accent-purple">
                    AGENTS.override.md
                  </code>{" "}
                  takes precedence over{" "}
                  <code className="font-mono text-text-primary">AGENTS.md</code>{" "}
                  in the same directory. When the override file exists, Codex
                  uses it instead of AGENTS.md for that directory.
                </p>
              </div>

              {targetHasOverride && (
                <div className="rounded-md border border-amber-500/30 bg-amber-500/10 px-3 py-2.5 mb-3">
                  <p className="text-xs text-amber-300 leading-relaxed">
                    This directory has an AGENTS.override.md file. Changes saved
                    to this AGENTS.md will not affect Codex until the override
                    file is removed or renamed.
                  </p>
                </div>
              )}

              {result?.instructionSources.length ? (
                <ol className="space-y-2">
                  {result.instructionSources.map((source, index) => {
                    const isHighest =
                      source.loaded && source.path === highestLoadedSource;
                    const isOverride =
                      fileName(source.path) === "AGENTS.override.md";

                    return (
                      <li
                        key={`${source.path}-${source.kind}-${index}`}
                        className="flex items-center gap-3 rounded-md border border-border bg-app-bg px-3 py-2"
                      >
                        <span className="w-5 h-5 rounded-full bg-app-card border border-border text-[10px] font-mono text-text-muted flex items-center justify-center shrink-0">
                          {index + 1}
                        </span>
                        <div className="flex-1 min-w-0">
                          <div className="flex items-center gap-2 flex-wrap">
                            <span
                              className="text-xs text-text-primary font-mono truncate max-w-full"
                              title={source.path}
                            >
                              {source.path}
                            </span>
                            {isOverride && (
                              <span className="text-[9px] font-semibold uppercase tracking-wider text-accent-purple bg-accent-purple/10 rounded px-1.5 py-0.5">
                                Override
                              </span>
                            )}
                            {isHighest && (
                              <span className="text-[9px] font-semibold uppercase tracking-wider text-accent-blue bg-accent-blue/10 rounded px-1.5 py-0.5">
                                Highest precedence
                              </span>
                            )}
                            {source.truncated && (
                              <span className="text-[9px] font-semibold uppercase tracking-wider text-amber-300 bg-amber-500/10 rounded px-1.5 py-0.5">
                                Truncated
                              </span>
                            )}
                          </div>
                          <p className="text-[10px] text-text-muted mt-0.5">
                            {formatKind(source.kind)}
                          </p>
                        </div>
                        <span
                          className={`text-[10px] font-medium shrink-0 ${
                            source.loaded
                              ? "text-accent-green"
                              : source.exists
                                ? "text-amber-300"
                                : "text-text-muted"
                          }`}
                        >
                          {source.loaded
                            ? "Loaded"
                            : source.exists
                              ? "Not loaded"
                              : "Missing"}
                        </span>
                      </li>
                    );
                  })}
                </ol>
              ) : (
                <p className="rounded-md border border-border bg-app-bg px-3 py-3 text-xs text-text-muted">
                  No instruction sources were found. Save this file to start a
                  new instruction chain for this scope.
                </p>
              )}
            </section>

            <section className="flex-1 min-h-[280px] flex flex-col">
              <div className="flex items-center justify-between gap-3 mb-2">
                <div>
                  <h2
                    id="codex-instructions-editor-label"
                    className="text-sm font-semibold text-text-primary"
                  >
                    Edit AGENTS.md
                  </h2>
                  <p className="text-xs text-text-muted mt-0.5">
                    Add instructions that Codex should follow in this scope.
                  </p>
                </div>
                <span
                  className="text-[10px] text-text-muted font-mono truncate max-w-sm"
                  title={targetPath}
                >
                  {targetPath}
                </span>
              </div>
              <textarea
                id="codex-instructions-editor"
                aria-labelledby="codex-instructions-editor-label"
                value={content}
                onChange={(event) => {
                  setContent(event.target.value);
                  setSuccess(null);
                }}
                className="w-full flex-1 min-h-[240px] px-4 py-3 bg-app-card border border-border rounded-lg font-mono text-sm text-text-primary focus:outline-none focus:border-accent-blue resize-none"
                placeholder={
                  isGlobal
                    ? "# AGENTS.md\n\nAdd instructions that Codex should follow across all projects."
                    : "# AGENTS.md\n\nAdd instructions that Codex should follow for this project."
                }
                spellCheck={false}
              />
            </section>
          </>
        )}
      </div>
    </div>
  );
}
