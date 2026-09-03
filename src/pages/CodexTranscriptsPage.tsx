import { useCallback, useEffect, useRef, useState } from "react";
import { DebugPath } from "../components/common/DebugPath";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";
import {
  listCodexTranscriptSessions,
  readCodexTranscript,
  type CodexTranscriptPage,
  type CodexTranscriptSession,
  type CodexTranscriptSessionPage,
} from "../lib/tauri";

const SESSION_PAGE_SIZE = 60;
const MESSAGE_PAGE_SIZE = 100;
const SEARCH_DEBOUNCE_MS = 450;

function formatRelativeTime(value: string): string {
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

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function shortPath(path: string): string {
  const match = path.match(/^\/Users\/[^/]+(\/.*)?$/);
  return match ? `~${match[1] ?? ""}` : path;
}

function mergeSessions(
  current: CodexTranscriptSession[],
  incoming: CodexTranscriptSession[],
): CodexTranscriptSession[] {
  const seen = new Set(current.map((session) => session.sessionId));
  return [
    ...current,
    ...incoming.filter((session) => !seen.has(session.sessionId)),
  ];
}

export function CodexTranscriptsPage() {
  const [sessions, setSessions] = useState<CodexTranscriptSession[]>([]);
  const [sessionResult, setSessionResult] =
    useState<CodexTranscriptSessionPage | null>(null);
  const [query, setQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [projectScope, setProjectScope] = useState<string | null>(null);
  const [loadingSessions, setLoadingSessions] = useState(true);
  const [loadingMoreSessions, setLoadingMoreSessions] = useState(false);
  const [sessionError, setSessionError] = useState<string | null>(null);

  const [selectedSession, setSelectedSession] =
    useState<CodexTranscriptSession | null>(null);
  const [transcript, setTranscript] = useState<CodexTranscriptPage | null>(
    null,
  );
  const [loadingTranscript, setLoadingTranscript] = useState(false);
  const [loadingMoreMessages, setLoadingMoreMessages] = useState(false);
  const [transcriptError, setTranscriptError] = useState<string | null>(null);

  const sessionRequestIdRef = useRef(0);
  const transcriptRequestIdRef = useRef(0);

  useEffect(() => {
    const timer = setTimeout(
      () => setDebouncedQuery(query.trim()),
      SEARCH_DEBOUNCE_MS,
    );
    return () => clearTimeout(timer);
  }, [query]);

  const loadSessions = useCallback(
    async (offset: number, append: boolean) => {
      const requestId = ++sessionRequestIdRef.current;
      if (append) {
        setLoadingMoreSessions(true);
      } else {
        setLoadingSessions(true);
        setLoadingMoreSessions(false);
        setSessions([]);
        setSessionResult(null);
      }
      setSessionError(null);

      try {
        const next = await listCodexTranscriptSessions(
          SESSION_PAGE_SIZE,
          offset,
          projectScope,
          debouncedQuery,
        );
        if (requestId !== sessionRequestIdRef.current) return;
        setSessions((current) =>
          append ? mergeSessions(current, next.sessions) : next.sessions,
        );
        setSessionResult(next);
      } catch (caught) {
        if (requestId !== sessionRequestIdRef.current) return;
        setSessionError(
          caught instanceof Error ? caught.message : String(caught),
        );
        if (!append) {
          setSessions([]);
          setSessionResult(null);
        }
      } finally {
        if (requestId === sessionRequestIdRef.current) {
          setLoadingSessions(false);
          setLoadingMoreSessions(false);
        }
      }
    },
    [debouncedQuery, projectScope],
  );

  useEffect(() => {
    transcriptRequestIdRef.current += 1;
    setSelectedSession(null);
    setTranscript(null);
    setTranscriptError(null);
    setLoadingTranscript(false);
    setLoadingMoreMessages(false);
    void loadSessions(0, false);
    return () => {
      sessionRequestIdRef.current += 1;
    };
  }, [loadSessions]);

  useEffect(
    () => () => {
      transcriptRequestIdRef.current += 1;
    },
    [],
  );

  const loadTranscript = async (
    session: CodexTranscriptSession,
    offset: number,
    append: boolean,
  ) => {
    const requestId = ++transcriptRequestIdRef.current;
    if (append) {
      setLoadingMoreMessages(true);
    } else {
      setLoadingTranscript(true);
      setLoadingMoreMessages(false);
    }
    setTranscriptError(null);

    try {
      const next = await readCodexTranscript(
        session.sessionId,
        MESSAGE_PAGE_SIZE,
        offset,
      );
      if (requestId !== transcriptRequestIdRef.current) return;
      setTranscript((current) =>
        append && current
          ? {
              ...next,
              messages: [...current.messages, ...next.messages],
              hasMore: next.hasMore && next.messages.length > 0,
              truncated: current.truncated || next.truncated,
            }
          : {
              ...next,
              hasMore: next.hasMore && next.messages.length > 0,
            },
      );
    } catch (caught) {
      if (requestId !== transcriptRequestIdRef.current) return;
      setTranscriptError(
        caught instanceof Error ? caught.message : String(caught),
      );
      if (!append) setTranscript(null);
    } finally {
      if (requestId === transcriptRequestIdRef.current) {
        setLoadingTranscript(false);
        setLoadingMoreMessages(false);
      }
    }
  };

  const selectSession = (session: CodexTranscriptSession) => {
    setSelectedSession(session);
    setTranscript(null);
    void loadTranscript(session, 0, false);
  };

  const searching = query.trim() !== debouncedQuery;

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <header className="px-6 py-5 border-b border-border shrink-0">
        <div className="flex items-start justify-between gap-4 mb-4">
          <div>
            <h1 className="text-2xl font-semibold text-text-primary">
              Transcripts
            </h1>
            <DebugPath path={sessionResult?.sourcePath ?? "Codex sessions"} />
            <p className="text-sm text-text-secondary mt-1">
              Read Codex conversations by project
              {sessionResult && (
                <span className="text-text-muted">
                  {" "}
                  &middot; {sessionResult.total.toLocaleString()} matching
                </span>
              )}
            </p>
          </div>
          <button
            type="button"
            onClick={() => void loadSessions(0, false)}
            disabled={loadingSessions}
            className="px-3 py-2 text-sm bg-app-card border border-border rounded-lg text-text-secondary hover:text-text-primary hover:bg-app-card-hover disabled:opacity-50"
          >
            {loadingSessions ? "Refreshing..." : "Refresh"}
          </button>
        </div>

        <div className="flex items-center gap-3">
          <div className="relative flex-1 min-w-0">
            <label htmlFor="codex-transcript-search" className="sr-only">
              Search Codex transcripts
            </label>
            <svg
              aria-hidden="true"
              className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-muted"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <circle cx="11" cy="11" r="7" strokeWidth={2} />
              <path d="m20 20-4-4" strokeWidth={2} strokeLinecap="round" />
            </svg>
            <input
              id="codex-transcript-search"
              type="search"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="Search titles, projects, and user messages..."
              className="w-full bg-app-card border border-border rounded-lg pl-10 pr-4 py-2 text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:ring-1 focus:ring-accent-blue focus:border-accent-blue"
            />
          </div>
          <ProjectScopeSelector
            value={projectScope}
            onChange={setProjectScope}
          />
        </div>
      </header>

      <div className="px-6 py-2.5 border-b border-border bg-blue-500/5 text-xs text-text-secondary shrink-0">
        Only user and assistant messages are shown. Reasoning, tool activity,
        and internal records stay hidden.
      </div>

      <div className="flex-1 flex min-h-0 overflow-hidden">
        <aside
          className="w-[340px] min-w-[280px] max-w-[42%] border-r border-border flex flex-col overflow-hidden"
          aria-label="Codex transcript sessions"
        >
          {sessionError && (
            <div
              role="alert"
              className="m-3 p-3 text-sm text-red-400 bg-red-500/10 border border-red-500/30 rounded-lg"
            >
              {sessionError}
            </div>
          )}

          {sessionResult?.truncated && (
            <div
              role="status"
              className="m-3 p-3 text-xs text-amber-300 bg-amber-500/10 border border-amber-500/30 rounded-lg"
            >
              The safe scan limit was reached. Some sessions or search matches
              may be missing.
            </div>
          )}

          {loadingSessions || searching ? (
            <div
              role="status"
              aria-live="polite"
              className="flex-1 flex items-center justify-center text-sm text-text-muted p-4"
            >
              {searching ? "Waiting to search..." : "Loading sessions..."}
            </div>
          ) : sessionError && !sessionResult ? (
            <div className="flex-1 flex items-center justify-center text-center text-sm text-text-muted p-6">
              Transcript sessions could not be loaded. Use Refresh to try again.
            </div>
          ) : sessions.length === 0 ? (
            <div className="flex-1 flex items-center justify-center text-center text-sm text-text-muted p-6">
              {debouncedQuery
                ? "No transcripts match this search."
                : projectScope
                  ? "No transcripts were found for this project."
                  : "No Codex transcripts were found."}
            </div>
          ) : (
            <div className="flex-1 overflow-y-auto min-h-0">
              {sessions.map((session) => (
                <button
                  key={session.sessionId}
                  type="button"
                  onClick={() => selectSession(session)}
                  aria-pressed={
                    selectedSession?.sessionId === session.sessionId
                  }
                  className={`w-full text-left px-4 py-3 border-b border-border/70 transition-colors ${
                    selectedSession?.sessionId === session.sessionId
                      ? "bg-accent-blue/10 border-l-2 border-l-accent-blue"
                      : "hover:bg-app-card-hover"
                  }`}
                >
                  <div className="flex items-start gap-2">
                    <p className="flex-1 min-w-0 text-sm font-medium text-text-primary line-clamp-2">
                      {session.title}
                    </p>
                    <time
                      className="text-[10px] text-text-muted whitespace-nowrap"
                      dateTime={session.updatedAt}
                    >
                      {formatRelativeTime(session.updatedAt)}
                    </time>
                  </div>
                  <p
                    className="text-xs text-text-secondary truncate mt-1"
                    title={session.projectPath}
                  >
                    {session.projectName || shortPath(session.projectPath)}
                  </p>
                  <div className="flex items-center gap-2 mt-1.5 text-[10px] text-text-muted">
                    <span>{formatSize(session.fileSizeBytes)}</span>
                    <span className="font-mono">
                      {session.sessionId.slice(0, 8)}
                    </span>
                    {session.archived && (
                      <span className="px-1.5 py-0.5 rounded bg-purple-500/15 text-purple-300">
                        Archived
                      </span>
                    )}
                  </div>
                </button>
              ))}

              {sessionResult?.hasMore && (
                <div className="p-3 text-center">
                  <button
                    type="button"
                    onClick={() => void loadSessions(sessions.length, true)}
                    disabled={loadingMoreSessions}
                    className="px-3 py-1.5 text-xs bg-app-card border border-border rounded text-text-secondary hover:text-text-primary disabled:opacity-50"
                  >
                    {loadingMoreSessions ? "Loading..." : "Load more sessions"}
                  </button>
                </div>
              )}
            </div>
          )}
        </aside>

        <main className="flex-1 flex flex-col min-w-0 overflow-hidden">
          {!selectedSession ? (
            <div className="flex-1 flex items-center justify-center text-sm text-text-muted p-6 text-center">
              Select a session to read its conversation.
            </div>
          ) : (
            <>
              <div className="px-5 py-3 border-b border-border shrink-0">
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0">
                    <h2 className="text-sm font-semibold text-text-primary truncate">
                      {selectedSession.title}
                    </h2>
                    <p
                      className="text-xs text-text-muted truncate mt-0.5"
                      title={selectedSession.projectPath}
                    >
                      {shortPath(selectedSession.projectPath)}
                    </p>
                  </div>
                  <span className="text-[10px] text-text-muted font-mono shrink-0">
                    {selectedSession.sessionId}
                  </span>
                </div>
              </div>

              {transcriptError && (
                <div
                  role="alert"
                  className="m-4 p-3 text-sm text-red-400 bg-red-500/10 border border-red-500/30 rounded-lg"
                >
                  {transcriptError}
                </div>
              )}

              {loadingTranscript ? (
                <div
                  role="status"
                  aria-live="polite"
                  className="flex-1 flex items-center justify-center text-sm text-text-muted"
                >
                  Loading conversation...
                </div>
              ) : transcript ? (
                <div className="flex-1 overflow-y-auto min-h-0 px-5 py-4 space-y-4">
                  {transcript.truncated && (
                    <div
                      role="status"
                      className="p-3 text-xs text-amber-300 bg-amber-500/10 border border-amber-500/30 rounded-lg"
                    >
                      This conversation reached a safe read limit, or a visible
                      message was shortened. The transcript may be incomplete.
                    </div>
                  )}

                  {transcript.messages.length === 0 ? (
                    <p className="text-center text-sm text-text-muted py-10">
                      No user or assistant messages were found in this session.
                    </p>
                  ) : (
                    transcript.messages.map((message) => (
                      <article
                        key={`${selectedSession.sessionId}:${message.ordinal}`}
                        className={`rounded-lg border p-4 ${
                          message.role === "user"
                            ? "bg-blue-500/5 border-blue-500/20"
                            : "bg-app-card border-border"
                        }`}
                      >
                        <div className="flex items-center justify-between gap-3 mb-2">
                          <span
                            className={`text-xs font-semibold ${
                              message.role === "user"
                                ? "text-blue-300"
                                : "text-emerald-300"
                            }`}
                          >
                            {message.role === "user" ? "You" : "Codex"}
                          </span>
                          {message.timestamp && (
                            <time
                              dateTime={message.timestamp}
                              className="text-[10px] text-text-muted"
                            >
                              {formatRelativeTime(message.timestamp)}
                            </time>
                          )}
                        </div>
                        <p className="text-sm text-text-primary whitespace-pre-wrap break-words leading-relaxed">
                          {message.content}
                        </p>
                      </article>
                    ))
                  )}

                  {transcript.hasMore && (
                    <div className="text-center pt-2 pb-4">
                      <button
                        type="button"
                        onClick={() =>
                          void loadTranscript(
                            selectedSession,
                            transcript.messages.length,
                            true,
                          )
                        }
                        disabled={loadingMoreMessages}
                        className="px-4 py-2 text-sm bg-app-card border border-border rounded-lg text-text-secondary hover:text-text-primary hover:bg-app-card-hover disabled:opacity-50"
                      >
                        {loadingMoreMessages
                          ? "Loading more..."
                          : "Load more messages"}
                      </button>
                    </div>
                  )}
                </div>
              ) : null}
            </>
          )}
        </main>
      </div>
    </div>
  );
}
