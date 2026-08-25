import { useState, useEffect, useRef } from "react";
import {
  listKimiTranscriptSessions,
  readKimiTranscript,
  searchKimiTranscripts,
} from "../lib/tauri";
import type { KimiTranscriptSession, KimiTranscriptMessage } from "../lib/tauri";
import { DebugPath } from "../components/common/DebugPath";

const SEARCH_DEBOUNCE_MS = 500;

function formatDate(isoString: string | null): string {
  if (!isoString) return "";
  const d = new Date(isoString);
  if (isNaN(d.getTime())) return "";
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

interface ProjectGroup {
  projectPath: string;
  displayPath: string;
  sessions: KimiTranscriptSession[];
  latestActivity: string | null;
}

function groupByProject(sessions: KimiTranscriptSession[]): ProjectGroup[] {
  const map = new Map<string, KimiTranscriptSession[]>();
  for (const s of sessions) {
    const key = s.project_path;
    if (!map.has(key)) map.set(key, []);
    map.get(key)!.push(s);
  }

  const groups: ProjectGroup[] = [];
  for (const [projectPath, items] of map) {
    const sorted = [...items].sort((a, b) => {
      const at = a.last_activity ? new Date(a.last_activity).getTime() : 0;
      const bt = b.last_activity ? new Date(b.last_activity).getTime() : 0;
      return bt - at;
    });
    groups.push({
      projectPath,
      displayPath: shortenPath(projectPath),
      sessions: sorted,
      latestActivity: sorted[0]?.last_activity ?? null,
    });
  }

  groups.sort((a, b) => {
    const at = a.latestActivity ? new Date(a.latestActivity).getTime() : 0;
    const bt = b.latestActivity ? new Date(b.latestActivity).getTime() : 0;
    return bt - at;
  });
  return groups;
}

function RoleLabel({ role }: { role: string }) {
  if (role === "user") return <>You</>;
  if (role === "system") return <>System</>;
  return <>Assistant</>;
}

export function KimiTranscriptsPage() {
  const [allSessions, setAllSessions] = useState<KimiTranscriptSession[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [search, setSearch] = useState("");
  const [searching, setSearching] = useState(false);
  const [searchResults, setSearchResults] = useState<KimiTranscriptSession[] | null>(
    null
  );
  const searchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const [expandedProjects, setExpandedProjects] = useState<Set<string>>(new Set());
  const [selectedSession, setSelectedSession] = useState<KimiTranscriptSession | null>(
    null
  );
  const [messages, setMessages] = useState<KimiTranscriptMessage[]>([]);
  const [messagesLoading, setMessagesLoading] = useState(false);
  const [messagesError, setMessagesError] = useState<string | null>(null);
  const [systemExpanded, setSystemExpanded] = useState(false);

  useEffect(() => {
    setLoading(true);
    setError(null);
    listKimiTranscriptSessions()
      .then((data) => {
        setAllSessions(data);
        if (data.length > 0) {
          const firstProject = data[0]?.project_path;
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
      searchKimiTranscripts(query)
        .then((results) => {
          setSearchResults(results);
          const projectKeys = new Set(results.map((r) => r.project_path));
          setExpandedProjects(projectKeys);
        })
        .catch(() => setSearchResults([]))
        .finally(() => setSearching(false));
    }, SEARCH_DEBOUNCE_MS);

    return () => {
      if (searchTimer.current) clearTimeout(searchTimer.current);
    };
  }, [search]);

  const handleSelectSession = (session: KimiTranscriptSession) => {
    setSelectedSession(session);
    setMessages([]);
    setMessagesError(null);
    setSystemExpanded(false);
    setMessagesLoading(true);
    readKimiTranscript(session.session_id, session.project_path)
      .then(setMessages)
      .catch((e) => setMessagesError(String(e)))
      .finally(() => setMessagesLoading(false));
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
  const groups = groupByProject(baseSessions);

  const systemMessage = messages.find((m) => m.role === "system");
  const visibleMessages = messages.filter((m) => m.role !== "system");

  return (
    <div className="flex h-full overflow-hidden">
      {/* Left panel — project tree */}
      <div className="w-1/3 border-r border-border flex flex-col min-w-0">
        <div className="p-4 border-b border-border space-y-3 shrink-0">
          <h1 className="text-lg font-semibold text-text-primary">Transcripts</h1>
          <DebugPath path="~/.kimi/sessions/" />

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
              placeholder="Search messages..."
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

          {searching && <div className="text-xs text-text-muted">Searching...</div>}
          {searchResults !== null && !searching && (
            <div className="text-xs text-text-muted">
              {searchResults.length} session{searchResults.length !== 1 ? "s" : ""} matched
            </div>
          )}
        </div>

        <div className="flex-1 overflow-y-auto">
          {loading && (
            <div className="p-4 text-text-secondary text-sm">Loading transcripts...</div>
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
                    <div
                      className="text-sm font-medium text-text-primary truncate"
                      title={group.projectPath}
                    >
                      {group.displayPath}
                    </div>
                    <div className="flex items-center gap-1.5 mt-1">
                      <span className="text-[10px] text-text-muted">
                        {group.sessions.length} session
                        {group.sessions.length !== 1 ? "s" : ""}
                      </span>
                      {group.latestActivity && (
                        <span className="text-[10px] text-text-muted">
                          · {formatDate(group.latestActivity)}
                        </span>
                      )}
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
                        <div className="text-xs text-text-primary truncate" title={session.title}>
                          {session.title}
                        </div>
                        <div className="flex items-center gap-2 text-[10px] text-text-muted mt-0.5">
                          <span>
                            {session.message_count} msg
                            {session.message_count !== 1 ? "s" : ""}
                          </span>
                          <span>·</span>
                          <span>{formatDate(session.last_activity) || "—"}</span>
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
              <div className="text-4xl mb-3 opacity-30">🌙</div>
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
                title={selectedSession.title}
              >
                {selectedSession.title}
              </span>
              <span className="text-[10px] font-medium px-1.5 py-0.5 rounded bg-cyan-500/20 text-cyan-400">
                Kimi
              </span>
              <span className="text-xs text-text-muted font-mono">
                {selectedSession.session_id.slice(0, 8)}
              </span>
              <span className="text-xs text-text-muted ml-auto">
                {formatDate(selectedSession.last_activity)}
              </span>
            </div>

            <div className="flex-1 overflow-y-auto p-4 space-y-3">
              {messagesLoading && (
                <div className="text-text-secondary text-sm text-center py-8">
                  Loading messages...
                </div>
              )}
              {messagesError && (
                <div className="text-red-400 text-sm text-center py-8">{messagesError}</div>
              )}

              {!messagesLoading && systemMessage && (
                <div className="rounded-lg border border-border bg-app-card/40">
                  <button
                    onClick={() => setSystemExpanded((v) => !v)}
                    className="w-full text-left px-3 py-2 text-[10px] font-medium text-text-muted hover:text-text-secondary flex items-center gap-1.5"
                  >
                    <span>{systemExpanded ? "▼" : "▶"}</span>
                    System prompt
                  </button>
                  {systemExpanded && (
                    <div className="px-3 pb-3 text-xs text-text-secondary whitespace-pre-wrap break-words font-mono">
                      {systemMessage.content}
                    </div>
                  )}
                </div>
              )}

              {!messagesLoading &&
                visibleMessages.map((msg, i) => {
                  const isUser = msg.role === "user";
                  return (
                    <div key={i} className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
                      <div
                        className={`max-w-[80%] rounded-lg px-3 py-2 text-sm whitespace-pre-wrap break-words ${
                          isUser
                            ? "bg-indigo-500/20 text-text-primary"
                            : "bg-app-card text-text-primary"
                        }`}
                      >
                        <div className="flex items-center gap-2 text-[10px] text-text-muted mb-1 font-medium">
                          <RoleLabel role={msg.role} />
                          {msg.timestamp && (
                            <span className="font-normal">{formatDate(msg.timestamp)}</span>
                          )}
                        </div>
                        {msg.content}
                      </div>
                    </div>
                  );
                })}

              {!messagesLoading && !messagesError && visibleMessages.length === 0 && (
                <div className="text-text-muted text-sm text-center py-8">
                  No messages in this session.
                </div>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  );
}
