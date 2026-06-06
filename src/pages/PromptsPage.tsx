import { useState, useEffect, useRef, useCallback } from "react";
import {
  getPromptHistory,
  searchPromptHistory,
  getPromptStats,
  buildResumeCommand,
  startClaudeSession,
} from "../lib/tauri";
import type { PromptEntry, PromptStats } from "../lib/tauri";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";
import { DebugPath } from "../components/common/DebugPath";

function formatTimestamp(ts: string): string {
  try {
    const d = new Date(ts);
    if (isNaN(d.getTime())) return ts;
    const now = new Date();
    const diffMs = now.getTime() - d.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    if (diffMins < 1) return "just now";
    if (diffMins < 60) return `${diffMins}m ago`;
    const diffHours = Math.floor(diffMins / 60);
    if (diffHours < 24) return `${diffHours}h ago`;
    const diffDays = Math.floor(diffHours / 24);
    if (diffDays < 7) return `${diffDays}d ago`;
    return d.toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
      year: d.getFullYear() !== now.getFullYear() ? "numeric" : undefined,
    });
  } catch {
    return ts;
  }
}

function truncate(text: string, max: number): string {
  if (text.length <= max) return text;
  return text.slice(0, max).trimEnd() + "…";
}

const PAGE_SIZE = 100;

export function PromptsPage() {
  const [prompts, setPrompts] = useState<PromptEntry[]>([]);
  const [stats, setStats] = useState<PromptStats | null>(null);
  const [query, setQuery] = useState("");
  const [projectScope, setProjectScope] = useState<string | null>(null);
  const [offset, setOffset] = useState(0);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expandedIds, setExpandedIds] = useState<Set<number>>(new Set());
  const [copiedIdx, setCopiedIdx] = useState<number | null>(null);
  const [copiedCmdIdx, setCopiedCmdIdx] = useState<number | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const fetchPrompts = useCallback(
    async (searchQuery: string, reset: boolean) => {
      try {
        if (reset) {
          setLoading(true);
          setOffset(0);
        } else {
          setLoadingMore(true);
        }
        setError(null);

        let results: PromptEntry[];
        if (searchQuery.trim()) {
          results = await searchPromptHistory(searchQuery.trim());
        } else {
          const currentOffset = reset ? 0 : offset;
          results = await getPromptHistory(PAGE_SIZE, currentOffset);
        }

        if (reset) {
          setPrompts(results);
        } else {
          setPrompts((prev) => [...prev, ...results]);
        }
      } catch (err) {
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setLoading(false);
        setLoadingMore(false);
      }
    },
    [offset]
  );

  useEffect(() => {
    fetchPrompts("", true);
    getPromptStats()
      .then(setStats)
      .catch(() => {});
  }, []);

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      fetchPrompts(query, true);
    }, 300);
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [query]);

  const handleLoadMore = async () => {
    const newOffset = offset + PAGE_SIZE;
    setOffset(newOffset);
    setLoadingMore(true);
    try {
      const results = await getPromptHistory(PAGE_SIZE, newOffset);
      setPrompts((prev) => [...prev, ...results]);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoadingMore(false);
    }
  };

  const toggleExpand = (idx: number) => {
    setExpandedIds((prev) => {
      const next = new Set(prev);
      if (next.has(idx)) next.delete(idx);
      else next.add(idx);
      return next;
    });
  };

  const copyToClipboard = async (text: string, idx: number) => {
    try {
      await navigator.clipboard.writeText(text);
      setCopiedIdx(idx);
      setTimeout(() => setCopiedIdx(null), 1500);
    } catch {
      /* clipboard not available */
    }
  };

  const copyResumeCommand = async (entry: PromptEntry, idx: number) => {
    if (!entry.session_id) return;
    try {
      const cmd = await buildResumeCommand(entry.session_id, entry.project);
      await navigator.clipboard.writeText(cmd);
      setCopiedCmdIdx(idx);
      setTimeout(() => setCopiedCmdIdx(null), 1500);
    } catch {
      /* clipboard not available */
    }
  };

  const startSession = async (entry: PromptEntry) => {
    if (!entry.session_id) return;
    try {
      await startClaudeSession(entry.session_id, entry.project);
    } catch (e) {
      setError(typeof e === "string" ? e : "Failed to start session");
    }
  };

  const filtered =
    projectScope != null
      ? prompts.filter((p) => p.project === projectScope)
      : prompts;

  return (
    <div className="p-6 h-full flex flex-col">
      <div className="mb-6">
        <h1 className="text-2xl font-bold text-text-primary mb-1">
          Prompt History
        </h1>
        <DebugPath path="~/.claude/history.jsonl" className="text-sm" />
        <p className="text-text-secondary text-sm">
          Browse and search your Claude Code prompts
          {stats && (
            <span className="text-text-muted ml-2">
              &middot; {stats.total.toLocaleString()} total
            </span>
          )}
        </p>
      </div>

      <div className="flex items-center gap-3 mb-4">
        <div className="relative flex-1">
          <svg
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
            type="text"
            placeholder="Search prompts..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            className="w-full bg-app-card border border-border rounded-lg pl-10 pr-4 py-2 text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:ring-1 focus:ring-blue-500/50 focus:border-blue-500/50"
          />
        </div>

        <ProjectScopeSelector value={projectScope} onChange={setProjectScope} />
      </div>

      {error && (
        <div className="bg-red-500/10 border border-red-500/30 rounded-lg px-4 py-3 mb-4 text-red-400 text-sm">
          {error}
        </div>
      )}

      {loading ? (
        <div className="flex-1 flex items-center justify-center text-text-muted text-sm">
          Loading prompts...
        </div>
      ) : filtered.length === 0 ? (
        <div className="flex-1 flex items-center justify-center text-text-muted text-sm">
          {query ? "No prompts match your search." : "No prompt history found."}
        </div>
      ) : (
        <div className="flex-1 overflow-y-auto space-y-2 min-h-0">
          {filtered.map((entry, idx) => {
            const isExpanded = expandedIds.has(idx);
            const displayText = isExpanded
              ? entry.display
              : truncate(entry.display, 100);
            const needsTruncation = entry.display.length > 100;

            return (
              <div
                key={`${entry.timestamp_ms}-${idx}`}
                className="bg-app-card border border-border rounded-lg px-4 py-3 group hover:border-blue-500/30 transition-colors"
              >
                <div className="flex items-start gap-3">
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2 mb-1.5">
                      <span className="text-text-muted text-xs whitespace-nowrap">
                        {formatTimestamp(entry.timestamp)}
                      </span>
                      {(entry.project_name || entry.project) && (
                        <span className="text-xs bg-blue-500/15 text-blue-400 px-2 py-0.5 rounded-full whitespace-nowrap">
                          {entry.project_name || entry.project}
                        </span>
                      )}
                      {entry.session_id && (
                        <div className="flex items-center gap-1.5">
                          <button
                            onClick={() => startSession(entry)}
                            className="text-xs font-medium bg-blue-500/15 text-blue-400 hover:bg-blue-500/25 px-2 py-0.5 rounded-full whitespace-nowrap flex items-center gap-1"
                            title="Open a terminal and resume this session"
                          >
                            <svg
                              className="w-3 h-3"
                              fill="none"
                              stroke="currentColor"
                              viewBox="0 0 24 24"
                            >
                              <path
                                strokeLinecap="round"
                                strokeLinejoin="round"
                                strokeWidth={2}
                                d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z"
                              />
                              <path
                                strokeLinecap="round"
                                strokeLinejoin="round"
                                strokeWidth={2}
                                d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
                              />
                            </svg>
                            Start this session
                          </button>
                          <button
                            onClick={() => copyResumeCommand(entry, idx)}
                            className="text-xs font-medium bg-app-card-hover text-text-secondary hover:text-text-primary border border-border px-2 py-0.5 rounded-full whitespace-nowrap flex items-center gap-1"
                            title="Copy the claude --resume command"
                          >
                            {copiedCmdIdx === idx ? (
                              <>
                                <svg
                                  className="w-3 h-3 text-green-400"
                                  fill="none"
                                  stroke="currentColor"
                                  viewBox="0 0 24 24"
                                >
                                  <path
                                    strokeLinecap="round"
                                    strokeLinejoin="round"
                                    strokeWidth={2}
                                    d="M5 13l4 4L19 7"
                                  />
                                </svg>
                                Copied
                              </>
                            ) : (
                              <>
                                <svg
                                  className="w-3 h-3"
                                  fill="none"
                                  stroke="currentColor"
                                  viewBox="0 0 24 24"
                                >
                                  <path
                                    strokeLinecap="round"
                                    strokeLinejoin="round"
                                    strokeWidth={2}
                                    d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"
                                  />
                                </svg>
                                Copy resume command
                              </>
                            )}
                          </button>
                          <button
                            onClick={() => copyToClipboard(entry.display, idx)}
                            className="text-xs font-medium bg-app-card-hover text-text-secondary hover:text-text-primary border border-border px-2 py-0.5 rounded-full whitespace-nowrap flex items-center gap-1"
                            title="Copy prompt"
                          >
                            {copiedIdx === idx ? (
                              <>
                                <svg
                                  className="w-3 h-3 text-green-400"
                                  fill="none"
                                  stroke="currentColor"
                                  viewBox="0 0 24 24"
                                >
                                  <path
                                    strokeLinecap="round"
                                    strokeLinejoin="round"
                                    strokeWidth={2}
                                    d="M5 13l4 4L19 7"
                                  />
                                </svg>
                                Copied
                              </>
                            ) : (
                              <>
                                <svg
                                  className="w-3 h-3"
                                  fill="none"
                                  stroke="currentColor"
                                  viewBox="0 0 24 24"
                                >
                                  <path
                                    strokeLinecap="round"
                                    strokeLinejoin="round"
                                    strokeWidth={2}
                                    d="M8 16H6a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v2m-6 12h8a2 2 0 002-2v-8a2 2 0 00-2-2h-8a2 2 0 00-2 2v8a2 2 0 002 2z"
                                  />
                                </svg>
                                Copy prompt
                              </>
                            )}
                          </button>
                        </div>
                      )}
                    </div>
                    <p
                      className={`text-sm text-text-primary whitespace-pre-wrap break-words ${needsTruncation ? "cursor-pointer" : ""}`}
                      onClick={() => needsTruncation && toggleExpand(idx)}
                    >
                      {displayText}
                    </p>
                    {needsTruncation && !isExpanded && (
                      <button
                        onClick={() => toggleExpand(idx)}
                        className="text-xs text-text-muted hover:text-text-secondary mt-1"
                      >
                        show more
                      </button>
                    )}
                  </div>
                </div>
              </div>
            );
          })}

          {!query.trim() && filtered.length >= PAGE_SIZE && (
            <div className="py-4 flex justify-center">
              <button
                onClick={handleLoadMore}
                disabled={loadingMore}
                className="bg-app-card border border-border rounded-lg px-6 py-2 text-sm text-text-secondary hover:text-text-primary hover:border-blue-500/30 transition-colors disabled:opacity-50"
              >
                {loadingMore ? "Loading..." : "Load More"}
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
