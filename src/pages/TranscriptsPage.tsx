import { useState, useEffect, useCallback, useRef } from "react";
import { useParams } from "react-router-dom";
import {
  listTranscriptSessions,
  readTranscript,
  searchTranscripts,
  searchTranscriptMatches,
  replaceInTranscript,
  updateTranscriptMessage,
  openTranscriptInTextEditor,
  deleteTranscriptBackup,
} from "../lib/tauri";
import type {
  TranscriptSession,
  TranscriptMessage,
  MatchLocation,
} from "../lib/tauri";
import { DebugPath } from "../components/common/DebugPath";

const PAGE_SIZE = 50;
const SEARCH_DEBOUNCE_MS = 500;

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDate(isoString: string): string {
  const d = new Date(isoString);
  const now = new Date();
  const diffMs = now.getTime() - d.getTime();
  const diffMins = Math.floor(diffMs / 60000);
  if (diffMins < 1) return "just now";
  if (diffMins < 60) return `${diffMins}m ago`;
  const diffHours = Math.floor(diffMins / 60);
  if (diffHours < 24) return `${diffHours}h ago`;
  const diffDays = Math.floor(diffHours / 24);
  if (diffDays < 7) return `${diffDays}d ago`;
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

function SourceBadge({ source }: { source: string }) {
  const isClaud = source === "claude";
  const bg = isClaud
    ? "bg-blue-500/20 text-blue-400"
    : "bg-purple-500/20 text-purple-400";
  const label = isClaud ? "Claude" : "Cursor";
  return (
    <span className={`text-[10px] font-medium px-1.5 py-0.5 rounded ${bg}`}>
      {label}
    </span>
  );
}

function shortenPath(path: string): string {
  const home = "/Users/";
  if (path.startsWith(home)) {
    const afterUsers = path.slice(home.length);
    const slashIdx = afterUsers.indexOf("/");
    if (slashIdx !== -1) {
      return "~" + afterUsers.slice(slashIdx);
    }
    return "~";
  }
  return path;
}

// Case-sensitive, non-overlapping offsets of `term` within `text`.
function findOffsets(text: string, term: string): number[] {
  if (!term) return [];
  const out: number[] = [];
  let from = 0;
  let idx = text.indexOf(term, from);
  while (idx !== -1) {
    out.push(idx);
    from = idx + term.length;
    idx = text.indexOf(term, from);
  }
  return out;
}

interface ProjectGroup {
  projectPath: string;
  displayPath: string;
  sessions: TranscriptSession[];
  sources: Set<string>;
  latestDate: string;
}

function groupByProject(sessions: TranscriptSession[]): ProjectGroup[] {
  const map = new Map<string, TranscriptSession[]>();
  for (const s of sessions) {
    const key = s.project_name;
    if (!map.has(key)) map.set(key, []);
    map.get(key)!.push(s);
  }

  const groups: ProjectGroup[] = [];
  for (const [projectPath, items] of map) {
    const sorted = [...items].sort(
      (a, b) =>
        new Date(b.modified_at).getTime() - new Date(a.modified_at).getTime()
    );
    const sources = new Set(sorted.map((s) => s.source));
    groups.push({
      projectPath,
      displayPath: shortenPath(projectPath),
      sessions: sorted,
      sources,
      latestDate: sorted[0]?.modified_at ?? "",
    });
  }

  groups.sort(
    (a, b) =>
      new Date(b.latestDate).getTime() - new Date(a.latestDate).getTime()
  );
  return groups;
}

export function TranscriptsPage() {
  const { adapterId } = useParams<{ adapterId?: string }>();
  const defaultSource: "all" | "claude" | "cursor" =
    adapterId === "claude-code" ? "claude" : adapterId === "cursor" ? "cursor" : "all";

  const [allSessions, setAllSessions] = useState<TranscriptSession[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [sourceFilter, setSourceFilter] = useState<"all" | "claude" | "cursor">(
    defaultSource
  );
  const [search, setSearch] = useState("");
  const [searching, setSearching] = useState(false);
  const [searchResults, setSearchResults] = useState<
    TranscriptSession[] | null
  >(null);
  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const [expandedProjects, setExpandedProjects] = useState<Set<string>>(
    new Set()
  );
  const [selectedSession, setSelectedSession] =
    useState<TranscriptSession | null>(null);
  const [messages, setMessages] = useState<TranscriptMessage[]>([]);
  const [messagesLoading, setMessagesLoading] = useState(false);
  const [messagesError, setMessagesError] = useState<string | null>(null);
  const [offset, setOffset] = useState(0);
  const [hasMore, setHasMore] = useState(false);

  // Secret-scrubbing state (Claude + Cursor transcripts; both are local JSONL).
  const isScrubbable =
    selectedSession?.source === "claude" || selectedSession?.source === "cursor";
  const EDIT_SIZE_LIMIT = 2 * 1024 * 1024; // 2 MB — above this, block in-app edit
  const [editMode, setEditMode] = useState(false);
  // In-flight per-message edits keyed by JSONL line_index.
  const [editDrafts, setEditDrafts] = useState<Record<number, string>>({});
  const [savingLine, setSavingLine] = useState<number | null>(null);
  const [editError, setEditError] = useState<string | null>(null);
  const [oversizeWarning, setOversizeWarning] = useState(false);

  const [findBarOpen, setFindBarOpen] = useState(false);
  const [showReplace, setShowReplace] = useState(false);
  const [findText, setFindText] = useState("");
  const [replaceText, setReplaceText] = useState("");
  const [matches, setMatches] = useState<MatchLocation[]>([]);
  const [activeMatchIndex, setActiveMatchIndex] = useState(0);
  const [replaceBusy, setReplaceBusy] = useState(false);
  const [backupNotice, setBackupNotice] = useState<{
    count: number;
    backupPath: string | null;
  } | null>(null);
  const matchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const conversationRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    setLoading(true);
    setError(null);
    listTranscriptSessions()
      .then((data) => {
        setAllSessions(data);
        if (data.length > 0) {
          const firstProject = data[0]?.project_name;
          if (firstProject) {
            setExpandedProjects(new Set([firstProject]));
          }
        }
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    if (searchTimer.current) clearTimeout(searchTimer.current);

    const query = search.trim();
    if (!query) {
      setSearchResults(null);
      setSearching(false);
      return;
    }

    setSearching(true);
    searchTimer.current = setTimeout(() => {
      searchTranscripts(query)
        .then((results) => {
          setSearchResults(results);
          const projectKeys = new Set(results.map((r) => r.project_name));
          setExpandedProjects(projectKeys);
        })
        .catch(() => setSearchResults([]))
        .finally(() => setSearching(false));
    }, SEARCH_DEBOUNCE_MS);

    return () => {
      if (searchTimer.current) clearTimeout(searchTimer.current);
    };
  }, [search]);

  const loadMessages = useCallback(
    async (
      session: TranscriptSession,
      startOffset: number,
      append: boolean
    ) => {
      setMessagesLoading(true);
      setMessagesError(null);
      try {
        const result = await readTranscript(
          session.file_path,
          PAGE_SIZE,
          startOffset
        );
        if (append) {
          setMessages((prev) => [...prev, ...result]);
        } else {
          setMessages(result);
        }
        setOffset(startOffset + result.length);
        setHasMore(result.length === PAGE_SIZE);
      } catch (e) {
        setMessagesError(String(e));
      } finally {
        setMessagesLoading(false);
      }
    },
    []
  );

  const handleSelectSession = (session: TranscriptSession) => {
    setSelectedSession(session);
    setMessages([]);
    setOffset(0);
    setHasMore(false);
    setEditMode(false);
    setEditDrafts({});
    setSavingLine(null);
    setEditError(null);
    setOversizeWarning(false);
    setFindBarOpen(false);
    setShowReplace(false);
    setFindText("");
    setReplaceText("");
    setMatches([]);
    setActiveMatchIndex(0);
    setBackupNotice(null);
    loadMessages(session, 0, false);
  };

  // Recompute matches against the raw file when the in-transcript find term changes.
  useEffect(() => {
    if (matchTimer.current) clearTimeout(matchTimer.current);
    if (!selectedSession || !findText) {
      setMatches([]);
      setActiveMatchIndex(0);
      return;
    }
    const path = selectedSession.file_path;
    const term = findText;
    matchTimer.current = setTimeout(() => {
      searchTranscriptMatches(path, term)
        .then((m) => {
          setMatches(m);
          setActiveMatchIndex(0);
        })
        .catch(() => setMatches([]));
    }, 250);
    return () => {
      if (matchTimer.current) clearTimeout(matchTimer.current);
    };
  }, [findText, selectedSession]);

  // Total highlightable matches actually present in the rendered messages.
  const visibleMatchCount = findText
    ? messages.reduce(
        (acc, m) => acc + findOffsets(m.content, findText).length,
        0
      )
    : 0;

  // When searching, auto-load remaining pages so matches further down the
  // transcript become rendered and highlightable (backend counts the whole
  // file; the UI only paginates). Stop once every page is loaded.
  useEffect(() => {
    if (!findText || !selectedSession || editMode) return;
    if (messagesLoading) return;
    if (!hasMore) return;
    // Keep loading while there are more pages and the rendered messages don't
    // yet account for all backend matches.
    if (visibleMatchCount >= matches.length && matches.length > 0) return;
    loadMessages(selectedSession, offset, true);
  }, [
    findText,
    selectedSession,
    editMode,
    messagesLoading,
    hasMore,
    visibleMatchCount,
    matches.length,
    offset,
    loadMessages,
  ]);

  // Scroll the active highlighted match into view.
  useEffect(() => {
    if (!findText || visibleMatchCount === 0) return;
    const el = conversationRef.current?.querySelector<HTMLElement>(
      `mark[data-active="true"]`
    );
    el?.scrollIntoView({ block: "center", behavior: "smooth" });
  }, [activeMatchIndex, visibleMatchCount, findText, messages]);

  const gotoMatch = (dir: 1 | -1) => {
    if (visibleMatchCount === 0) return;
    setActiveMatchIndex(
      (i) => (i + dir + visibleMatchCount) % visibleMatchCount
    );
  };

  const enterEditMode = () => {
    if (!selectedSession) return;
    setEditError(null);
    // Large files crash the renderer when fully loaded for editing — gate them.
    if (selectedSession.file_size_bytes > EDIT_SIZE_LIMIT) {
      setOversizeWarning(true);
      return;
    }
    setEditDrafts({});
    setEditMode(true);
    setFindBarOpen(false);
  };

  const openInExternalEditor = async () => {
    if (!selectedSession) return;
    try {
      await openTranscriptInTextEditor(selectedSession.file_path);
      setOversizeWarning(false);
    } catch (e) {
      setEditError(String(e));
    }
  };

  const saveMessageEdit = async (lineIndex: number) => {
    if (!selectedSession) return;
    const newText = editDrafts[lineIndex];
    if (newText === undefined) return;
    setSavingLine(lineIndex);
    setEditError(null);
    try {
      const backup = await updateTranscriptMessage(
        selectedSession.file_path,
        lineIndex,
        newText,
        true
      );
      setBackupNotice({ count: -1, backupPath: backup });
      // Reflect the edit in the loaded messages without a full reload.
      setMessages((prev) =>
        prev.map((m) =>
          m.line_index === lineIndex ? { ...m, content: newText } : m
        )
      );
      setEditDrafts((prev) => {
        const next = { ...prev };
        delete next[lineIndex];
        return next;
      });
    } catch (e) {
      setEditError(String(e));
    } finally {
      setSavingLine(null);
    }
  };

  const replaceAll = async () => {
    if (!selectedSession || !findText) return;
    // Clear any prior banner so the confirm reads against a clean slate (the
    // banner from a previous replace must not linger behind this dialog).
    setBackupNotice(null);
    const ok = window.confirm(
      `Replace all ${matches.length} occurrence(s) of this string with "${replaceText}"?\n\nThis rewrites the transcript file on disk. A backup will be created.`
    );
    if (!ok) return;
    setReplaceBusy(true);
    try {
      const result = await replaceInTranscript(
        selectedSession.file_path,
        findText,
        replaceText,
        true
      );
      setFindText("");
      setReplaceText("");
      setMatches([]);
      // Reload conversation to reflect scrubbed content, THEN show the banner
      // so it appears after the replacement has actually been applied.
      setMessages([]);
      setOffset(0);
      await loadMessages(selectedSession, 0, false);
      setBackupNotice({ count: result.count, backupPath: result.backup_path });
    } catch (e) {
      setEditError(String(e));
    } finally {
      setReplaceBusy(false);
    }
  };

  const deleteBackup = async () => {
    if (!backupNotice?.backupPath) return;
    try {
      await deleteTranscriptBackup(backupNotice.backupPath);
      setBackupNotice(null);
    } catch (e) {
      setEditError(String(e));
    }
  };

  const handleLoadMore = () => {
    if (selectedSession) {
      loadMessages(selectedSession, offset, true);
    }
  };

  const toggleProject = (projectPath: string) => {
    setExpandedProjects((prev) => {
      const next = new Set(prev);
      if (next.has(projectPath)) {
        next.delete(projectPath);
      } else {
        next.add(projectPath);
      }
      return next;
    });
  };

  const baseSessions = searchResults ?? allSessions;
  const filteredSessions = baseSessions.filter((s) => {
    if (sourceFilter !== "all" && s.source !== sourceFilter) return false;
    return true;
  });

  const groups = groupByProject(filteredSessions);

  return (
    <div className="flex h-full overflow-hidden">
      {/* Left panel — project tree */}
      <div className="w-1/3 border-r border-border flex flex-col min-w-0">
        <div className="p-4 border-b border-border space-y-3 shrink-0">
          <h1 className="text-lg font-semibold text-text-primary">
            Transcripts
          </h1>
          <DebugPath path={
            adapterId === "claude-code"
              ? "~/.claude/projects/"
              : adapterId === "cursor"
              ? "~/.cursor/projects/"
              : "~/.claude/projects/ · ~/.cursor/projects/"
          } />

          <div className="relative">
            <svg
              className="absolute left-2.5 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-text-secondary pointer-events-none"
              fill="none"
              viewBox="0 0 24 24"
              stroke="currentColor"
              strokeWidth={2}
            >
              <circle cx="11" cy="11" r="8" />
              <path d="m21 21-4.35-4.35" strokeLinecap="round" />
            </svg>
            <input
              type="text"
              placeholder="Search user prompts..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              className="w-full bg-app-card border border-border rounded pl-8 pr-8 py-1.5 text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:border-indigo-500"
            />
            {search && (
              <button
                onClick={() => setSearch("")}
                className="absolute right-2.5 top-1/2 -translate-y-1/2 text-text-secondary hover:text-text-primary text-xs"
              >
                &times;
              </button>
            )}
          </div>

          {defaultSource === "all" && (
            <div className="flex gap-1">
              {(["all", "claude", "cursor"] as const).map((f) => (
                <button
                  key={f}
                  onClick={() => setSourceFilter(f)}
                  className={`px-2.5 py-1 text-xs rounded transition-colors ${
                    sourceFilter === f
                      ? "bg-indigo-500/20 text-indigo-400"
                      : "text-text-secondary hover:text-text-primary hover:bg-app-card"
                  }`}
                >
                  {f === "all" ? "All" : f === "claude" ? "Claude" : "Cursor"}
                </button>
              ))}
            </div>
          )}

          {searching && (
            <div className="text-xs text-text-muted">Searching...</div>
          )}
          {searchResults !== null && !searching && (
            <div className="text-xs text-text-muted">
              {searchResults.length} transcript
              {searchResults.length !== 1 ? "s" : ""} matched
            </div>
          )}
        </div>

        <div className="flex-1 overflow-y-auto">
          {loading && (
            <div className="p-4 text-text-secondary text-sm">
              Loading transcripts...
            </div>
          )}
          {error && <div className="p-4 text-red-400 text-sm">{error}</div>}
          {!loading && !error && groups.length === 0 && (
            <div className="p-4 text-text-muted text-sm">
              {search.trim() ? "No matching transcripts." : "No transcripts found."}
            </div>
          )}

          {groups.map((group) => {
            const isExpanded = expandedProjects.has(group.projectPath);
            return (
              <div key={group.projectPath}>
                <button
                  onClick={() => toggleProject(group.projectPath)}
                  className="w-full text-left px-3 py-2.5 flex items-start gap-2 hover:bg-app-card transition-colors border-b border-border/50"
                >
                  <span className="text-text-secondary text-xs mt-0.5 flex-shrink-0 w-3">
                    {isExpanded ? "▼" : "▶"}
                  </span>
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium text-text-primary truncate" title={group.projectPath}>
                      {group.displayPath}
                    </div>
                    <div className="flex items-center gap-1.5 mt-1">
                      {Array.from(group.sources).map((src) => (
                        <SourceBadge key={src} source={src} />
                      ))}
                      <span className="text-[10px] text-text-muted">
                        {group.sessions.length} session
                        {group.sessions.length !== 1 ? "s" : ""}
                      </span>
                      <span className="text-[10px] text-text-muted">
                        · {formatDate(group.latestDate)}
                      </span>
                    </div>
                  </div>
                </button>

                {isExpanded && (
                  <div className="bg-app-bg/50">
                    {group.sessions.map((session) => (
                      <button
                        key={session.session_id}
                        onClick={() => handleSelectSession(session)}
                        className={`w-full text-left pl-8 pr-3 py-2 border-b border-border/30 transition-colors ${
                          selectedSession?.session_id === session.session_id
                            ? "bg-indigo-500/10"
                            : "hover:bg-app-card/60"
                        }`}
                      >
                        <div className="flex items-center gap-2">
                          <span className="text-xs font-mono text-text-secondary truncate">
                            {session.session_id.slice(0, 8)}
                          </span>
                          <SourceBadge source={session.source} />
                        </div>
                        <div className="flex items-center gap-2 text-[10px] text-text-muted mt-0.5">
                          <span>{formatFileSize(session.file_size_bytes)}</span>
                          <span>·</span>
                          <span>{formatDate(session.modified_at)}</span>
                        </div>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>

      {/* Right panel — conversation */}
      <div className="w-2/3 flex flex-col min-w-0">
        {!selectedSession ? (
          <div className="flex-1 flex items-center justify-center">
            <div className="text-center">
              <div className="text-4xl mb-3 opacity-30">💬</div>
              <p className="text-text-secondary text-sm">
                Select a session to view the conversation
              </p>
            </div>
          </div>
        ) : (
          <>
            <div className="px-4 py-3 border-b border-border shrink-0 flex items-center gap-2">
              <span
                className="text-sm font-medium text-text-primary truncate"
                title={selectedSession.project_name}
              >
                {shortenPath(selectedSession.project_name)}
              </span>
              <SourceBadge source={selectedSession.source} />
              <span className="text-xs text-text-muted font-mono">
                {selectedSession.session_id.slice(0, 8)}
              </span>
              <span className="text-xs text-text-muted ml-auto">
                {formatDate(selectedSession.modified_at)}
              </span>
              {isScrubbable && !editMode && (
                <div className="flex items-center gap-1">
                  <button
                    onClick={() => {
                      setFindBarOpen((v) => !v);
                      setShowReplace(false);
                    }}
                    className={`px-2 py-1 text-xs rounded transition-colors ${
                      findBarOpen
                        ? "bg-indigo-500/20 text-indigo-400"
                        : "text-text-secondary hover:text-text-primary hover:bg-app-card"
                    }`}
                  >
                    Find
                  </button>
                  <button
                    onClick={() => {
                      setShowReplace(true);
                      setFindBarOpen(true);
                    }}
                    className="px-2 py-1 text-xs rounded text-text-secondary hover:text-text-primary hover:bg-app-card transition-colors"
                  >
                    Scrub Secret
                  </button>
                  <button
                    onClick={enterEditMode}
                    className="px-2 py-1 text-xs rounded text-text-secondary hover:text-text-primary hover:bg-app-card transition-colors"
                  >
                    Edit
                  </button>
                </div>
              )}
              {isScrubbable && editMode && (
                <div className="flex items-center gap-2">
                  <span className="text-[10px] text-indigo-400">
                    Editing — click a message to edit its text
                  </span>
                  <button
                    onClick={() => {
                      setEditMode(false);
                      setEditDrafts({});
                      setEditError(null);
                    }}
                    className="px-2.5 py-1 text-xs rounded bg-indigo-500/20 text-indigo-400 hover:bg-indigo-500/30 transition-colors"
                  >
                    Done
                  </button>
                </div>
              )}
            </div>

            {isScrubbable && findBarOpen && !editMode && (
              <div className="px-4 py-2.5 border-b border-border shrink-0 space-y-2 bg-app-card/40">
                <div className="flex items-center gap-2">
                  <input
                    type="text"
                    autoFocus
                    placeholder="Find in transcript (case-sensitive)…"
                    value={findText}
                    onChange={(e) => setFindText(e.target.value)}
                    className="flex-1 bg-app-card border border-border rounded px-2.5 py-1.5 text-xs font-mono text-text-primary placeholder:text-text-muted focus:outline-none focus:border-indigo-500"
                  />
                  <span className="text-xs text-text-muted whitespace-nowrap min-w-[88px] text-right">
                    {findText
                      ? hasMore && messagesLoading
                        ? "loading…"
                        : `${matches.length} match${matches.length !== 1 ? "es" : ""}`
                      : ""}
                  </span>
                  <button
                    onClick={() => gotoMatch(-1)}
                    disabled={visibleMatchCount === 0}
                    className="px-1.5 py-1 text-xs rounded border border-border text-text-secondary hover:text-text-primary disabled:opacity-30"
                  >
                    ↑
                  </button>
                  <button
                    onClick={() => gotoMatch(1)}
                    disabled={visibleMatchCount === 0}
                    className="px-1.5 py-1 text-xs rounded border border-border text-text-secondary hover:text-text-primary disabled:opacity-30"
                  >
                    ↓
                  </button>
                </div>
                {showReplace && (
                  <div className="flex items-center gap-2">
                    <input
                      type="text"
                      placeholder="Replace with (e.g. DUMMY_SECRET)…"
                      value={replaceText}
                      onChange={(e) => setReplaceText(e.target.value)}
                      className="flex-1 bg-app-card border border-border rounded px-2.5 py-1.5 text-xs font-mono text-text-primary placeholder:text-text-muted focus:outline-none focus:border-indigo-500"
                    />
                    <button
                      onClick={replaceAll}
                      disabled={replaceBusy || !findText || matches.length === 0}
                      className="px-3 py-1.5 text-xs rounded bg-red-500/20 text-red-300 hover:bg-red-500/30 transition-colors disabled:opacity-40 whitespace-nowrap"
                    >
                      {replaceBusy ? "Replacing…" : "Replace All"}
                    </button>
                  </div>
                )}
              </div>
            )}

            {backupNotice && (
              <div className="px-4 py-2.5 border-b border-border shrink-0 bg-amber-500/10 text-xs text-amber-200 flex items-start gap-3">
                <span className="flex-1">
                  {backupNotice.count >= 0
                    ? `Replaced ${backupNotice.count} occurrence${backupNotice.count !== 1 ? "s" : ""}. `
                    : "Transcript saved. "}
                  {backupNotice.backupPath ? (
                    <>
                      A backup was written to{" "}
                      <span className="font-mono">{backupNotice.backupPath.split("/").pop()}</span> — it
                      still contains the original string and is auto-deleted after 7 days.
                    </>
                  ) : (
                    "No backup was created."
                  )}
                </span>
                {backupNotice.backupPath && (
                  <button
                    onClick={deleteBackup}
                    className="px-2 py-1 rounded bg-red-500/20 text-red-300 hover:bg-red-500/30 whitespace-nowrap"
                  >
                    Delete backup
                  </button>
                )}
                <button
                  onClick={() => setBackupNotice(null)}
                  className="text-amber-200/70 hover:text-amber-200"
                >
                  ✕
                </button>
              </div>
            )}

            {oversizeWarning && (
              <div className="px-4 py-3 border-b border-border shrink-0 bg-amber-500/10 text-xs text-amber-200 flex items-start gap-3">
                <span className="text-base leading-none">⚠️</span>
                <div className="flex-1">
                  <div className="font-medium mb-1">
                    This transcript is too large to edit in-app
                  </div>
                  <div className="text-amber-200/80">
                    {formatFileSize(selectedSession.file_size_bytes)} exceeds the 2 MB limit —
                    loading it for editing can crash the app. Open it in your default text editor
                    instead. You can still use Find / Scrub Secret here.
                  </div>
                </div>
                <button
                  onClick={openInExternalEditor}
                  className="px-2.5 py-1 rounded bg-indigo-500/20 text-indigo-400 hover:bg-indigo-500/30 whitespace-nowrap"
                >
                  Edit in Text Editor
                </button>
                <button
                  onClick={() => setOversizeWarning(false)}
                  className="px-2 py-1 rounded text-amber-200/70 hover:text-amber-200"
                >
                  Cancel
                </button>
              </div>
            )}

            <div
              ref={conversationRef}
              className="flex-1 overflow-y-auto p-4 space-y-3"
            >
              {messagesLoading && messages.length === 0 && (
                <div className="text-text-secondary text-sm text-center py-8">
                  Loading messages...
                </div>
              )}
              {messagesError && (
                <div className="text-red-400 text-sm text-center py-8">
                  {messagesError}
                </div>
              )}
              {editError && (
                <div className="text-red-400 text-xs text-center">{editError}</div>
              )}

              {(() => {
                // Build a flat list of visible-match positions so prev/next
                // and the active emphasis line up with what's rendered.
                let flatCursor = 0;
                return messages.map((msg, i) => {
                  const isUser = msg.role === "user" || msg.role === "human";
                  const offsets = !editMode && findText
                    ? findOffsets(msg.content, findText)
                    : [];
                  const activeLocalOffset = (() => {
                    if (editMode || !findText || offsets.length === 0) return null;
                    const start = flatCursor;
                    const end = flatCursor + offsets.length;
                    flatCursor = end;
                    if (activeMatchIndex >= start && activeMatchIndex < end) {
                      return offsets[activeMatchIndex - start];
                    }
                    return null;
                  })();
                  const draft = editDrafts[msg.line_index];
                  const isEditing = draft !== undefined;
                  return (
                    <div
                      key={i}
                      className={`flex ${isUser ? "justify-end" : "justify-start"}`}
                    >
                      <div
                        className={`max-w-[80%] rounded-lg px-3 py-2 text-sm whitespace-pre-wrap break-words ${
                          isUser
                            ? "bg-indigo-500/20 text-text-primary"
                            : "bg-app-card text-text-primary"
                        } ${
                          editMode
                            ? "ring-1 ring-indigo-500/30 cursor-text"
                            : ""
                        }`}
                      >
                        <div className="text-[10px] text-text-muted mb-1 font-medium">
                          {isUser ? "You" : "Assistant"}
                        </div>
                        {editMode ? (
                          isEditing ? (
                            <div className="space-y-2">
                              <textarea
                                autoFocus
                                value={draft}
                                onChange={(e) =>
                                  setEditDrafts((prev) => ({
                                    ...prev,
                                    [msg.line_index]: e.target.value,
                                  }))
                                }
                                rows={Math.min(20, draft.split("\n").length + 1)}
                                className="w-full resize-y bg-app-bg border border-indigo-500/40 rounded p-2 text-sm text-text-primary focus:outline-none focus:border-indigo-500"
                              />
                              <div className="flex items-center gap-2">
                                <button
                                  onClick={() => saveMessageEdit(msg.line_index)}
                                  disabled={savingLine === msg.line_index}
                                  className="px-2.5 py-1 text-xs rounded bg-indigo-500/20 text-indigo-400 hover:bg-indigo-500/30 disabled:opacity-40"
                                >
                                  {savingLine === msg.line_index ? "Saving…" : "Save"}
                                </button>
                                <button
                                  onClick={() =>
                                    setEditDrafts((prev) => {
                                      const next = { ...prev };
                                      delete next[msg.line_index];
                                      return next;
                                    })
                                  }
                                  className="px-2 py-1 text-xs rounded text-text-secondary hover:text-text-primary"
                                >
                                  Cancel
                                </button>
                              </div>
                            </div>
                          ) : (
                            <button
                              onClick={() =>
                                setEditDrafts((prev) => ({
                                  ...prev,
                                  [msg.line_index]: msg.content,
                                }))
                              }
                              className="text-left w-full"
                              title="Click to edit"
                            >
                              {msg.content}
                            </button>
                          )
                        ) : findText && offsets.length > 0 ? (
                          offsets.map((off, k) => {
                            const isActive = activeLocalOffset === off;
                            const prev = k === 0 ? 0 : offsets[k - 1] + findText.length;
                            return (
                              <span key={k}>
                                {msg.content.slice(prev, off)}
                                <mark
                                  data-active={isActive ? "true" : undefined}
                                  className={
                                    isActive
                                      ? "bg-amber-400 text-black rounded-sm"
                                      : "bg-amber-500/30 text-amber-200 rounded-sm"
                                  }
                                >
                                  {msg.content.slice(off, off + findText.length)}
                                </mark>
                                {k === offsets.length - 1
                                  ? msg.content.slice(off + findText.length)
                                  : null}
                              </span>
                            );
                          })
                        ) : (
                          msg.content
                        )}
                      </div>
                    </div>
                  );
                });
              })()}

              {hasMore && (
                <div className="flex justify-center pt-2">
                  <button
                    onClick={handleLoadMore}
                    disabled={messagesLoading}
                    className="px-4 py-1.5 text-xs rounded bg-app-card border border-border text-text-secondary hover:text-text-primary hover:border-indigo-500 transition-colors disabled:opacity-50"
                  >
                    {messagesLoading ? "Loading..." : "Load More"}
                  </button>
                </div>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
