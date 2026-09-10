import { useCallback, useEffect, useMemo, useState } from "react";
import {
  getCodexProjectsOverview,
  getCodexProjectSessions,
  getCodexSessionTimeline,
  type CodexAgentNode,
  type CodexProjectSummary,
  type CodexTimelineEvent,
} from "../lib/tauri";

const TIMELINE_PAGE = 100;

/** Icon + label + accent per timeline event kind. */
const KIND_META: Record<string, { icon: string; label: string; tone: string }> = {
  user: { icon: "👤", label: "Prompt", tone: "text-accent-blue" },
  assistant: { icon: "🤖", label: "Response", tone: "text-emerald-400" },
  reasoning: { icon: "💭", label: "Reasoning", tone: "text-purple-400" },
  tool_call: { icon: "⚙️", label: "Tool", tone: "text-amber-400" },
  tool_output: { icon: "📄", label: "Output", tone: "text-text-muted" },
  spawn_agent: { icon: "🌱", label: "Spawn agent", tone: "text-emerald-400" },
  send_message: { icon: "✉️", label: "Message", tone: "text-accent-blue" },
  wait_agent: { icon: "⏳", label: "Wait", tone: "text-text-muted" },
  task_started: { icon: "▶️", label: "Task started", tone: "text-text-muted" },
  task_complete: { icon: "✅", label: "Task complete", tone: "text-emerald-400" },
};

/** Stable colour per agent name, so the same agent looks the same everywhere. */
const AVATAR_TONES = [
  "bg-accent-blue/20 text-accent-blue",
  "bg-emerald-500/20 text-emerald-400",
  "bg-purple-500/20 text-purple-400",
  "bg-amber-500/20 text-amber-400",
  "bg-pink-500/20 text-pink-400",
  "bg-cyan-500/20 text-cyan-400",
];
function avatarTone(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i += 1) hash = (hash * 31 + name.charCodeAt(i)) >>> 0;
  return AVATAR_TONES[hash % AVATAR_TONES.length];
}

function formatTokens(n: number): string {
  if (n >= 1e9) return `${(n / 1e9).toFixed(2)}B`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)}M`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(1)}K`;
  return String(n);
}

function formatSize(bytes: number): string {
  if (bytes >= 1024 ** 3) return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
  if (bytes >= 1024 ** 2) return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}

function formatWhen(value: string | null): string {
  if (!value) return "";
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return value;
  return d.toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

function countAgents(node: CodexAgentNode): number {
  return node.children.reduce((sum, c) => sum + 1 + countAgents(c), 0);
}
function totalTokens(node: CodexAgentNode): number {
  return node.children.reduce((sum, c) => sum + totalTokens(c), node.total_tokens);
}
function flatten(node: CodexAgentNode): CodexAgentNode[] {
  return node.children.reduce<CodexAgentNode[]>((acc, c) => acc.concat(flatten(c)), [node]);
}

/** One timeline event: icon, kind, two-line preview, expand. */
function TimelineRow({ event }: { event: CodexTimelineEvent }) {
  const [expanded, setExpanded] = useState(false);
  const meta = KIND_META[event.kind] ?? { icon: "•", label: event.kind, tone: "text-text-muted" };
  const lines = event.preview.split("\n");
  const collapsed = lines.slice(0, 2).join("\n");
  const hasMore = event.truncated || lines.length > 2;

  return (
    <div className="px-3 py-2 border-b border-border/50 hover:bg-white/[0.02]">
      <div className="flex items-center gap-2">
        <span className="text-[11px]">{meta.icon}</span>
        <span className={`text-[10px] font-medium uppercase tracking-wider ${meta.tone}`}>
          {meta.label}
        </span>
        {event.name && (
          <span className="rounded bg-app-bg px-1.5 py-0.5 text-[9px] font-mono text-text-secondary">
            {event.name}
          </span>
        )}
        {event.timestamp && (
          <span className="ml-auto text-[9px] text-text-muted">{formatWhen(event.timestamp)}</span>
        )}
      </div>
      {event.preview && (
        <pre className="mt-1 whitespace-pre-wrap break-words font-mono text-[11px] leading-relaxed text-text-secondary">
          {expanded ? event.preview : collapsed}
        </pre>
      )}
      {hasMore && (
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="mt-1 text-[10px] text-accent-blue hover:underline"
        >
          {expanded ? "Show less" : "Show more"}
        </button>
      )}
    </div>
  );
}

/** Inline, lazily-loaded timeline for one agent inside the tree. */
function InlineTimeline({ node }: { node: CodexAgentNode }) {
  const [events, setEvents] = useState<CodexTimelineEvent[]>([]);
  const [total, setTotal] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);

  const load = useCallback(
    async (offset: number) => {
      setLoading(true);
      try {
        const page = await getCodexSessionTimeline(node.rollout_path, offset, TIMELINE_PAGE);
        setEvents((prev) => (offset === 0 ? page.events : [...prev, ...page.events]));
        setTotal(page.total);
        setHasMore(page.has_more);
      } finally {
        setLoading(false);
      }
    },
    [node.rollout_path],
  );

  useEffect(() => {
    void load(0);
  }, [load]);

  return (
    <div className="border-t border-border/50 bg-app-bg/40">
      {loading && events.length === 0 ? (
        <div className="px-3 py-2 text-[10px] text-text-muted">Loading timeline…</div>
      ) : (
        <>
          {events.map((e) => (
            <TimelineRow key={`${e.ordinal}-${e.timestamp ?? ""}`} event={e} />
          ))}
          {hasMore && (
            <div className="p-2 text-center">
              <button
                type="button"
                disabled={loading}
                onClick={() => void load(events.length)}
                className="rounded border border-border bg-app-card px-2 py-1 text-[10px] text-text-secondary hover:text-text-primary disabled:opacity-50"
              >
                {loading ? "Loading…" : `Load more (${events.length} of ${total})`}
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}

/** A session or subagent row; expands to reveal children and its own timeline. */
function AgentRow({
  node,
  depth,
  onFocus,
  focused,
}: {
  node: CodexAgentNode;
  depth: number;
  onFocus: (n: CodexAgentNode) => void;
  focused: string | null;
}) {
  const [open, setOpen] = useState(false);
  const [showTimeline, setShowTimeline] = useState(false);
  const name = node.nickname ?? "Main session";
  const agents = countAgents(node);
  const isFocused = focused === node.rollout_path;

  return (
    <div>
      <div
        className={`flex items-start gap-2 border-b border-border/50 px-3 py-2.5 transition-colors ${
          isFocused ? "bg-accent-blue/10" : "hover:bg-white/[0.02]"
        }`}
        style={{ paddingLeft: `${12 + depth * 18}px` }}
      >
        <button
          type="button"
          onClick={() => {
            if (node.children.length > 0) setOpen((v) => !v);
            else setShowTimeline((v) => !v);
          }}
          className="mt-0.5 w-3 shrink-0 text-[10px] text-text-muted"
          aria-label="Toggle"
        >
          {node.children.length > 0 ? (open ? "▼" : "▶") : showTimeline ? "▾" : "▸"}
        </button>

        <span
          className={`mt-0.5 flex h-5 w-5 shrink-0 items-center justify-center rounded-full text-[10px] font-semibold ${avatarTone(name)}`}
        >
          {name.charAt(0).toUpperCase()}
        </span>

        <div className="min-w-0 flex-1">
          <div className="flex items-baseline gap-2">
            <span className="truncate text-[13px] font-medium text-text-primary">{name}</span>
            {node.agent_path && (
              <span className="truncate font-mono text-[10px] text-text-muted">{node.agent_path}</span>
            )}
          </div>
          <div className="mt-1 flex flex-wrap items-center gap-1.5">
            {agents > 0 && (
              <span className="rounded bg-emerald-500/15 px-1.5 py-0.5 text-[9px] text-emerald-400">
                {agents} subagent{agents !== 1 ? "s" : ""}
              </span>
            )}
            <span className="rounded bg-app-bg px-1.5 py-0.5 text-[9px] text-text-secondary">
              {node.offsets_len} events
            </span>
            <span className="rounded bg-app-bg px-1.5 py-0.5 text-[9px] text-text-secondary">
              {formatTokens(totalTokens(node))} tok
            </span>
            {node.tools.slice(0, 2).map(([tool, count]) => (
              <span key={tool} className="rounded bg-app-bg px-1.5 py-0.5 font-mono text-[9px] text-amber-400/80">
                {tool}×{count}
              </span>
            ))}
          </div>
        </div>

        <button
          type="button"
          onClick={() => {
            setShowTimeline((v) => !v);
            onFocus(node);
          }}
          className="mt-0.5 shrink-0 rounded border border-border px-1.5 py-0.5 text-[9px] text-text-secondary hover:text-text-primary"
        >
          {showTimeline ? "Hide" : "Timeline"}
        </button>
      </div>

      {showTimeline && <InlineTimeline node={node} />}

      {open &&
        node.children.map((child) => (
          <AgentRow
            key={child.rollout_path}
            node={child}
            depth={depth + 1}
            onFocus={onFocus}
            focused={focused}
          />
        ))}
    </div>
  );
}

export function CodexProjectsPage() {
  const [projects, setProjects] = useState<CodexProjectSummary[]>([]);
  const [activeProject, setActiveProject] = useState<string | null>(null);
  const [sessions, setSessions] = useState<CodexAgentNode[]>([]);
  const [focused, setFocused] = useState<CodexAgentNode | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");

  useEffect(() => {
    getCodexProjectsOverview()
      .then((o) => {
        setProjects(o.projects);
        setActiveProject((cur) => cur ?? o.projects[0]?.project_path ?? null);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    if (!activeProject) return;
    setFocused(null);
    getCodexProjectSessions(activeProject).then(setSessions).catch((e) => setError(String(e)));
  }, [activeProject]);

  const project = projects.find((p) => p.project_path === activeProject) ?? null;

  /** Aggregates for the default right-hand summary. */
  const summary = useMemo(() => {
    const all = sessions.flatMap(flatten);
    const agents = all.filter((n) => n.nickname);
    const tools = new Map<string, number>();
    for (const node of all) {
      for (const [name, count] of node.tools) tools.set(name, (tools.get(name) ?? 0) + count);
    }
    const topAgents = [...agents].sort((a, b) => b.total_tokens - a.total_tokens).slice(0, 6);
    const topTools = [...tools.entries()].sort((a, b) => b[1] - a[1]).slice(0, 6);
    const events = all.reduce((s, n) => s + n.offsets_len, 0);
    const bytes = all.reduce((s, n) => s + n.file_size_bytes, 0);
    return { topAgents, topTools, events, bytes, agentCount: agents.length };
  }, [sessions]);

  const visibleProjects = projects.filter((p) =>
    filter.trim() === "" ? true : p.project_name.toLowerCase().includes(filter.trim().toLowerCase()),
  );

  if (loading) {
    return <div className="p-6 text-sm text-text-muted">Reading Codex sessions…</div>;
  }

  return (
    <div className="flex h-[calc(100vh-7rem)] flex-col p-6">
      <div className="mb-4 shrink-0">
        <h1 className="text-2xl font-bold text-text-primary">Codex Projects</h1>
        <p className="text-sm text-text-secondary">
          Sessions, subagents, and per-event timelines from ~/.codex/sessions
        </p>
      </div>

      {error && (
        <div className="mb-3 shrink-0 rounded border border-red-500/30 bg-red-500/5 px-3 py-2 text-xs text-red-300">
          {error}
        </div>
      )}

      <div className="grid min-h-0 flex-1 grid-cols-[minmax(0,200px)_minmax(0,1.15fr)_minmax(0,1fr)] gap-3">
        {/* Projects rail */}
        <div className="flex min-h-0 flex-col overflow-hidden rounded-lg border border-border bg-app-card">
          <div className="shrink-0 border-b border-border px-3 py-2">
            <div className="mb-2 text-[10px] uppercase tracking-wider text-text-muted">
              Projects ({projects.length})
            </div>
            <input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="Filter…"
              className="w-full rounded border border-border bg-app-bg px-2 py-1 text-[11px] text-text-primary placeholder-text-muted focus:border-accent-blue focus:outline-none"
            />
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">
            {visibleProjects.map((p) => {
              const active = p.project_path === activeProject;
              return (
                <button
                  key={p.project_path}
                  type="button"
                  onClick={() => setActiveProject(p.project_path)}
                  title={p.project_path}
                  className={`w-full border-b border-border/50 px-3 py-2 text-left transition-colors ${
                    active ? "border-l-2 border-l-accent-blue bg-accent-blue/10" : "hover:bg-white/[0.02]"
                  }`}
                >
                  <div className="truncate text-[12px] font-medium text-text-primary">{p.project_name}</div>
                  <div className="mt-0.5 flex gap-2 text-[9px] text-text-muted">
                    <span>{p.sessions}s</span>
                    <span>{p.subagents}a</span>
                    <span>{formatTokens(p.total_tokens)}</span>
                  </div>
                </button>
              );
            })}
          </div>
        </div>

        {/* Sessions & subagents */}
        <div className="flex min-h-0 flex-col overflow-hidden rounded-lg border border-border bg-app-card">
          <div className="shrink-0 border-b border-border px-3 py-2 text-[10px] uppercase tracking-wider text-text-muted">
            Sessions &amp; subagents
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto">
            {sessions.length === 0 ? (
              <div className="p-4 text-xs text-text-muted">No sessions for this project.</div>
            ) : (
              sessions.map((s) => (
                <AgentRow
                  key={s.rollout_path}
                  node={s}
                  depth={0}
                  onFocus={setFocused}
                  focused={focused?.rollout_path ?? null}
                />
              ))
            )}
          </div>
        </div>

        {/* Detail: focused agent, or project summary by default */}
        <div className="flex min-h-0 flex-col overflow-hidden rounded-lg border border-border bg-app-card">
          <div className="shrink-0 border-b border-border px-3 py-2 text-[10px] uppercase tracking-wider text-text-muted">
            {focused ? focused.nickname ?? "Main session" : "Project summary"}
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto p-3">
            {focused ? (
              <div className="space-y-3">
                {focused.agent_path && (
                  <div className="font-mono text-[10px] text-text-muted">{focused.agent_path}</div>
                )}
                <div className="grid grid-cols-2 gap-2">
                  {[
                    ["Input", formatTokens(focused.input_tokens)],
                    ["Cached", formatTokens(focused.cached_input_tokens)],
                    ["Output", formatTokens(focused.output_tokens)],
                    ["Events", String(focused.offsets_len)],
                    ["Tool calls", String(focused.tool_calls)],
                    ["Messages", String(focused.messages)],
                    ["Size", formatSize(focused.file_size_bytes)],
                    ["Model", focused.model ?? "—"],
                  ].map(([label, value]) => (
                    <div key={label} className="rounded border border-border bg-app-bg px-2 py-1.5">
                      <div className="text-[9px] uppercase tracking-wider text-text-muted">{label}</div>
                      <div className="truncate text-[12px] text-text-primary">{value}</div>
                    </div>
                  ))}
                </div>
                {focused.tools.length > 0 && (
                  <div>
                    <div className="mb-1 text-[10px] uppercase tracking-wider text-text-muted">Tools</div>
                    <div className="flex flex-wrap gap-1.5">
                      {focused.tools.map(([tool, count]) => (
                        <span
                          key={tool}
                          className="rounded bg-app-bg px-1.5 py-0.5 font-mono text-[10px] text-amber-400/90"
                        >
                          {tool} ×{count}
                        </span>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            ) : project ? (
              <div className="space-y-4">
                <div>
                  <div className="text-sm font-medium text-text-primary">{project.project_name}</div>
                  <div className="mt-0.5 truncate font-mono text-[10px] text-text-muted">
                    {project.project_path}
                  </div>
                </div>
                <div className="grid grid-cols-2 gap-2">
                  {[
                    ["Sessions", String(project.sessions)],
                    ["Subagents", String(summary.agentCount)],
                    ["Tokens", formatTokens(project.total_tokens)],
                    ["Events", String(summary.events)],
                    ["On disk", formatSize(summary.bytes)],
                    ["Last active", formatWhen(project.last_active) || "—"],
                  ].map(([label, value]) => (
                    <div key={label} className="rounded border border-border bg-app-bg px-2 py-1.5">
                      <div className="text-[9px] uppercase tracking-wider text-text-muted">{label}</div>
                      <div className="truncate text-[12px] text-text-primary">{value}</div>
                    </div>
                  ))}
                </div>

                {summary.topAgents.length > 0 && (
                  <div>
                    <div className="mb-1.5 text-[10px] uppercase tracking-wider text-text-muted">
                      Busiest subagents
                    </div>
                    <div className="space-y-1">
                      {summary.topAgents.map((a) => {
                        const max = summary.topAgents[0].total_tokens || 1;
                        const pct = Math.max(4, Math.round((a.total_tokens / max) * 100));
                        return (
                          <div key={a.rollout_path} className="flex items-center gap-2">
                            <span
                              className={`flex h-4 w-4 shrink-0 items-center justify-center rounded-full text-[9px] font-semibold ${avatarTone(a.nickname ?? "?")}`}
                            >
                              {(a.nickname ?? "?").charAt(0).toUpperCase()}
                            </span>
                            <span className="w-20 shrink-0 truncate text-[11px] text-text-primary">
                              {a.nickname}
                            </span>
                            <div className="h-1.5 flex-1 overflow-hidden rounded bg-app-bg">
                              <div className="h-full rounded bg-accent-blue/60" style={{ width: `${pct}%` }} />
                            </div>
                            <span className="w-12 shrink-0 text-right text-[10px] text-text-muted">
                              {formatTokens(a.total_tokens)}
                            </span>
                          </div>
                        );
                      })}
                    </div>
                  </div>
                )}

                {summary.topTools.length > 0 && (
                  <div>
                    <div className="mb-1.5 text-[10px] uppercase tracking-wider text-text-muted">Tools used</div>
                    <div className="space-y-1">
                      {summary.topTools.map(([tool, count]) => (
                        <div key={tool} className="flex items-center justify-between text-[11px]">
                          <span className="font-mono text-amber-400/90">{tool}</span>
                          <span className="text-text-secondary">{count}</span>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            ) : (
              <div className="text-xs text-text-muted">Select a project.</div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
