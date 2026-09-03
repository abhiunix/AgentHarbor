import { useCallback, useEffect, useRef, useState } from "react";
import { DebugPath } from "../components/common/DebugPath";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";
import {
  getCodexPromptHistory,
  type CodexPromptEntry,
  type CodexPromptHistoryPage as CodexPromptHistoryResult,
} from "../lib/tauri";

const PAGE_SIZE = 100;
const SEARCH_DEBOUNCE_MS = 350;
const PREVIEW_LENGTH = 220;

function formatTimestamp(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value || "Unknown time";

  const minutes = Math.floor((Date.now() - date.getTime()) / 60_000);
  if (minutes < 1) return "just now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 7) return `${days}d ago`;
  return date.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year:
      date.getFullYear() === new Date().getFullYear() ? undefined : "numeric",
  });
}

function preview(text: string): string {
  if (text.length <= PREVIEW_LENGTH) return text;
  return `${text.slice(0, PREVIEW_LENGTH).trimEnd()}...`;
}

function shellQuote(value: string): string {
  return `'${value.replace(/'/g, `'"'"'`)}'`;
}

function resumeCommand(entry: CodexPromptEntry): string {
  const command = `codex resume ${shellQuote(entry.sessionId)}`;
  return entry.project ? `${command} -C ${shellQuote(entry.project)}` : command;
}

function entryKey(entry: CodexPromptEntry, index: number): string {
  return `${entry.sessionId}:${entry.timestampMs}:${index}`;
}

export function CodexPromptHistoryPage() {
  const [prompts, setPrompts] = useState<CodexPromptEntry[]>([]);
  const [result, setResult] = useState<CodexPromptHistoryResult | null>(null);
  const [query, setQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [projectScope, setProjectScope] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [copyStatus, setCopyStatus] = useState<string | null>(null);
  const requestIdRef = useRef(0);
  const copyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const timer = setTimeout(
      () => setDebouncedQuery(query.trim()),
      SEARCH_DEBOUNCE_MS,
    );
    return () => clearTimeout(timer);
  }, [query]);

  useEffect(
    () => () => {
      if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
    },
    [],
  );

  const loadPage = useCallback(
    async (offset: number, append: boolean) => {
      const requestId = ++requestIdRef.current;
      if (append) {
        setLoadingMore(true);
      } else {
        setLoading(true);
        setLoadingMore(false);
        setPrompts([]);
        setResult(null);
      }
      setError(null);

      try {
        const next = await getCodexPromptHistory(
          PAGE_SIZE,
          offset,
          projectScope,
          debouncedQuery,
        );
        if (requestId !== requestIdRef.current) return;
        setPrompts((current) =>
          append ? [...current, ...next.entries] : next.entries,
        );
        setResult(next);
      } catch (caught) {
        if (requestId !== requestIdRef.current) return;
        setError(caught instanceof Error ? caught.message : String(caught));
        if (!append) {
          setPrompts([]);
          setResult(null);
        }
      } finally {
        if (requestId === requestIdRef.current) {
          setLoading(false);
          setLoadingMore(false);
        }
      }
    },
    [debouncedQuery, projectScope],
  );

  useEffect(() => {
    setExpanded(new Set());
    void loadPage(0, false);
    return () => {
      requestIdRef.current += 1;
    };
  }, [loadPage]);

  const copy = async (text: string, status: string) => {
    setActionError(null);
    try {
      await navigator.clipboard.writeText(text);
      setCopyStatus(status);
      if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
      copyTimerRef.current = setTimeout(() => setCopyStatus(null), 1_800);
    } catch (caught) {
      setActionError(
        caught instanceof Error ? caught.message : "Clipboard access failed",
      );
    }
  };

  const toggleExpanded = (key: string) => {
    setExpanded((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const searching = query.trim() !== debouncedQuery;

  return (
    <div className="p-6 h-full flex flex-col overflow-hidden">
      <div className="flex items-start justify-between gap-4 mb-5 shrink-0">
        <div>
          <h1 className="text-2xl font-semibold text-text-primary">
            Prompt History
          </h1>
          <DebugPath path={result?.sourcePath ?? "Codex history"} />
          <p className="text-sm text-text-secondary mt-1">
            Browse prompts recorded by Codex
            {result && (
              <span className="text-text-muted">
                {" "}
                &middot; {result.total.toLocaleString()} matching
              </span>
            )}
          </p>
        </div>
        <button
          type="button"
          onClick={() => void loadPage(0, false)}
          disabled={loading}
          className="px-3 py-2 text-sm bg-app-card border border-border rounded-lg text-text-secondary hover:text-text-primary hover:bg-app-card-hover disabled:opacity-50"
        >
          {loading ? "Refreshing..." : "Refresh"}
        </button>
      </div>

      <div className="flex items-center gap-3 mb-4 shrink-0">
        <div className="relative flex-1 min-w-0">
          <label htmlFor="codex-prompt-search" className="sr-only">
            Search Codex prompts
          </label>
          <svg
            aria-hidden="true"
            className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-muted"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
            />
          </svg>
          <input
            id="codex-prompt-search"
            type="search"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="Search prompts..."
            className="w-full bg-app-card border border-border rounded-lg pl-10 pr-4 py-2 text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:ring-1 focus:ring-accent-blue focus:border-accent-blue"
          />
        </div>
        <ProjectScopeSelector value={projectScope} onChange={setProjectScope} />
      </div>

      {(error || actionError) && (
        <div
          role="alert"
          className="bg-red-500/10 border border-red-500/30 rounded-lg px-4 py-3 mb-4 text-red-400 text-sm shrink-0"
        >
          {actionError ?? error}
        </div>
      )}

      {result?.truncated && (
        <div
          role="status"
          className="bg-amber-500/10 border border-amber-500/30 rounded-lg px-4 py-3 mb-4 text-amber-300 text-sm shrink-0"
        >
          Codex history is larger than the safe scan limit. These results may be
          incomplete.
        </div>
      )}

      <p className="sr-only" aria-live="polite">
        {copyStatus}
      </p>

      {loading || searching ? (
        <div
          role="status"
          aria-live="polite"
          className="flex-1 flex items-center justify-center text-text-muted text-sm"
        >
          {searching ? "Waiting to search..." : "Loading prompts..."}
        </div>
      ) : error && !result ? (
        <div className="flex-1 flex items-center justify-center text-text-muted text-sm">
          Prompt history could not be loaded. Use Refresh to try again.
        </div>
      ) : prompts.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center text-center px-6">
          <p className="text-text-secondary text-sm">
            {debouncedQuery
              ? "No prompts match this search."
              : projectScope
                ? "No prompt history was found for this project."
                : "No Codex prompt history was found."}
          </p>
          <p className="text-text-muted text-xs mt-1">
            New prompts appear after Codex records them in local history.
          </p>
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto min-h-0 space-y-2 pr-1">
          {prompts.map((entry, index) => {
            const key = entryKey(entry, index);
            const isExpanded = expanded.has(key);
            const canExpand = entry.display.length > PREVIEW_LENGTH;
            const projectLabel = entry.projectName || entry.project;

            return (
              <article
                key={key}
                className="bg-app-card border border-border rounded-lg px-4 py-3 hover:border-accent-blue/40 transition-colors"
              >
                <div className="flex items-center gap-2 flex-wrap mb-2">
                  <time
                    className="text-xs text-text-muted"
                    dateTime={entry.timestamp}
                  >
                    {formatTimestamp(entry.timestamp)}
                  </time>
                  {projectLabel && (
                    <span
                      className="text-[11px] bg-cyan-500/15 text-cyan-300 px-2 py-0.5 rounded-full max-w-[280px] truncate"
                      title={entry.project ?? projectLabel}
                    >
                      {projectLabel}
                    </span>
                  )}
                  <span className="ml-auto text-[11px] text-text-muted font-mono">
                    {entry.sessionId.slice(0, 8)}
                  </span>
                </div>

                <p className="text-sm text-text-primary whitespace-pre-wrap break-words leading-relaxed">
                  {isExpanded ? entry.display : preview(entry.display)}
                </p>

                <div className="flex items-center gap-3 mt-2">
                  {canExpand && (
                    <button
                      type="button"
                      onClick={() => toggleExpanded(key)}
                      aria-expanded={isExpanded}
                      className="text-xs text-accent-blue hover:underline"
                    >
                      {isExpanded ? "Show less" : "Show more"}
                    </button>
                  )}
                  <div className="ml-auto flex items-center gap-2">
                    <button
                      type="button"
                      onClick={() =>
                        void copy(entry.display, `Prompt copied for ${key}`)
                      }
                      className="px-2.5 py-1 text-xs text-text-secondary bg-app-bg border border-border rounded hover:text-text-primary hover:bg-app-card-hover"
                    >
                      {copyStatus === `Prompt copied for ${key}`
                        ? "Copied"
                        : "Copy prompt"}
                    </button>
                    <button
                      type="button"
                      onClick={() =>
                        void copy(
                          resumeCommand(entry),
                          `Resume command copied for ${key}`,
                        )
                      }
                      className="px-2.5 py-1 text-xs text-accent-blue bg-accent-blue/10 border border-accent-blue/30 rounded hover:bg-accent-blue/20"
                      title="Copy a shell-safe Codex resume command"
                    >
                      {copyStatus === `Resume command copied for ${key}`
                        ? "Copied command"
                        : "Copy resume command"}
                    </button>
                  </div>
                </div>
              </article>
            );
          })}

          {result?.hasMore && (
            <div className="py-4 text-center">
              <button
                type="button"
                onClick={() => void loadPage(prompts.length, true)}
                disabled={loadingMore}
                className="px-4 py-2 text-sm bg-app-card border border-border rounded-lg text-text-secondary hover:text-text-primary hover:bg-app-card-hover disabled:opacity-50"
              >
                {loadingMore ? "Loading more..." : "Load more"}
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
