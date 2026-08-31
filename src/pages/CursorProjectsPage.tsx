/**
 * Cursor Projects — per-project analytics assembled from Cursor's local
 * state.vscdb + workspaceStorage + ~/.cursor/projects (no network calls).
 * Modeled on KimiAnalyticsV2Page (Section/StatCard/ProjectRow) and
 * CursorAnalyticsV2Page (CommitTable layout).
 */
import { useEffect, useState, useCallback } from "react";
import {
  getCursorProjectsOverview,
  getCursorProjectDetail,
  startCursorInProject,
  type CursorProjectsOverview,
  type CursorProjectStat,
  type CursorProjectDetail,
} from "../lib/tauri";
import { DebugPath } from "../components/common/DebugPath";

// ── Helpers ───────────────────────────────────────────────────────────────

function formatNum(n: number | null | undefined): string {
  if (n == null || n === 0) return "0";
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

function timeAgo(iso: string | null): string {
  if (!iso) return "never";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "—";
  const secs = Math.floor((Date.now() - then) / 1000);
  if (secs < 60) return "just now";
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.floor(hrs / 24);
  return `${days}d ago`;
}

function formatDate(iso: string | null | undefined): string {
  if (!iso) return "—";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
}

// ── Shared small components (mirrors KimiAnalyticsV2Page / CursorAnalyticsV2Page) ──

function StatCard({ label, value, sub, color }: { label: string; value: string; sub?: string; color?: string }) {
  return (
    <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] px-4 py-3">
      <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">{label}</div>
      <div className={`text-xl font-semibold ${color || "text-text-primary"}`}>{value}</div>
      {sub && <div className="text-[11px] text-text-muted mt-0.5">{sub}</div>}
    </div>
  );
}

function Section({ title, children, defaultOpen = true, info }: { title: string; children: React.ReactNode; defaultOpen?: boolean; info?: string }) {
  const storageKey = `cursor-projects-${title}`;
  const [open, setOpen] = useState(() => {
    try { const s = localStorage.getItem(storageKey); return s !== "0"; } catch { return defaultOpen; }
  });
  const toggle = () => { const next = !open; setOpen(next); try { localStorage.setItem(storageKey, next ? "1" : "0"); } catch { /* noop */ } };
  return (
    <div className="mb-6">
      <div className="flex items-center gap-2 mb-3">
        <button onClick={toggle} className="flex items-center gap-2 text-left group">
          <svg className={`w-3 h-3 text-text-muted transition-transform ${open ? "rotate-90" : ""}`} fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
            <path strokeLinecap="round" strokeLinejoin="round" d="M9 5l7 7-7 7" />
          </svg>
          <h3 className="text-xs font-semibold uppercase tracking-wider text-text-muted group-hover:text-text-secondary">{title}</h3>
        </button>
        {info && <span className="text-[10px] text-text-muted">{info}</span>}
      </div>
      {open && children}
    </div>
  );
}

function AiPctChip({ pct }: { pct: number }) {
  const color = pct >= 66 ? "bg-indigo-500/20 text-indigo-400" : pct >= 33 ? "bg-amber-500/20 text-amber-400" : "bg-emerald-500/20 text-emerald-400";
  return <span className={`text-[9px] px-1.5 py-0.5 rounded font-medium ${color}`}>{pct.toFixed(0)}% AI</span>;
}

// ── Detail panel (lazy-loaded on row expand) ────────────────────────────────

type DetailTab = "sessions" | "commits" | "models" | "generations" | "mcps" | "plans";

function ProjectDetailPanel({ path }: { path: string }) {
  const [detail, setDetail] = useState<CursorProjectDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<DetailTab>("sessions");

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    getCursorProjectDetail(path)
      .then((d) => { if (!cancelled) { setDetail(d); setError(null); } })
      .catch((e) => { if (!cancelled) setError(String(e)); })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [path]);

  if (loading) return <div className="p-4 text-xs text-text-muted">Loading project detail…</div>;
  if (error) return <div className="p-4 text-xs text-accent-red">{error}</div>;
  if (!detail) return null;

  const tabs: { id: DetailTab; label: string }[] = [
    { id: "sessions", label: `Sessions (${detail.sessions.length})` },
    { id: "commits", label: `Commits (${detail.commits.length})` },
    { id: "models", label: "Models & Context" },
    { id: "generations", label: `Generations (${detail.generations.length})` },
    { id: "mcps", label: `MCPs (${detail.mcps.length})` },
    { id: "plans", label: `Plans (${detail.plans.length})` },
  ];

  return (
    <div className="p-4 bg-[#15161d] border-t border-[#2a2b36]">
      <div className="flex gap-1 border-b border-[#2a2b36] mb-3">
        {tabs.map((t) => (
          <button
            key={t.id}
            onClick={() => setTab(t.id)}
            className={`px-3 py-1.5 text-[11px] font-medium border-b-2 transition-colors ${
              tab === t.id ? "border-indigo-500 text-text-primary" : "border-transparent text-text-muted hover:text-text-secondary"
            }`}
          >
            {t.label}
          </button>
        ))}
      </div>

      {tab === "sessions" && (
        detail.sessions.length === 0 ? <p className="text-xs text-text-muted">No sessions.</p> : (
          <div className="overflow-x-auto">
            <table className="w-full text-xs">
              <thead>
                <tr className="text-left text-text-muted border-b border-[#2a2b36]">
                  <th className="px-2 py-1.5 font-medium">Name</th>
                  <th className="px-2 py-1.5 font-medium">Model</th>
                  <th className="px-2 py-1.5 font-medium text-right">Context</th>
                  <th className="px-2 py-1.5 font-medium text-right">±Lines</th>
                  <th className="px-2 py-1.5 font-medium text-right">Tokens</th>
                  <th className="px-2 py-1.5 font-medium">Source</th>
                  <th className="px-2 py-1.5 font-medium">Updated</th>
                </tr>
              </thead>
              <tbody>
                {detail.sessions.map((s) => (
                  <tr key={s.composer_id} className="border-b border-[#1e1f2a] hover:bg-[#1e1f2a]">
                    <td className="px-2 py-1.5 text-text-primary truncate max-w-[220px]">
                      {s.name ?? s.composer_id.slice(0, 8)}
                      {s.is_subagent && <span className="ml-1 text-[9px] px-1 rounded bg-purple-500/20 text-purple-400">subagent</span>}
                      {s.is_archived && <span className="ml-1 text-[9px] px-1 rounded border border-[#2a2b36] text-text-muted">archived</span>}
                    </td>
                    <td className="px-2 py-1.5 text-text-secondary">{s.model ?? "—"}</td>
                    <td className={`px-2 py-1.5 text-right font-mono ${s.context_usage_percent != null && s.context_usage_percent > 80 ? "text-red-400" : "text-text-secondary"}`}>
                      {s.context_usage_percent != null ? `${s.context_usage_percent.toFixed(0)}%` : "—"}
                    </td>
                    <td className="px-2 py-1.5 text-right font-mono">
                      <span className="text-emerald-400">+{s.lines_added}</span>{" "}
                      <span className="text-red-400">-{s.lines_removed}</span>
                    </td>
                    <td className="px-2 py-1.5 text-right font-mono text-text-muted">{formatNum(s.input_tokens + s.output_tokens)}</td>
                    <td className="px-2 py-1.5 text-[10px] text-text-muted">{s.resolution_source}</td>
                    <td className="px-2 py-1.5 text-[10px] text-text-muted whitespace-nowrap">{timeAgo(s.last_updated_at)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      )}

      {tab === "commits" && (
        detail.commits.length === 0 ? <p className="text-xs text-text-muted">No commits attributed to this project yet.</p> : (
          <div className="overflow-x-auto max-h-[350px] overflow-y-auto">
            <table className="w-full text-xs">
              <thead className="sticky top-0 bg-[#15161d]">
                <tr className="text-left text-text-muted border-b border-[#2a2b36]">
                  <th className="px-2 py-1.5 font-medium">Commit</th>
                  <th className="px-2 py-1.5 font-medium">Branch</th>
                  <th className="px-2 py-1.5 font-medium">Message</th>
                  <th className="px-2 py-1.5 font-medium text-right">AI %</th>
                  <th className="px-2 py-1.5 font-medium text-right">+Lines</th>
                  <th className="px-2 py-1.5 font-medium text-right">-Lines</th>
                  <th className="px-2 py-1.5 font-medium text-right">Tab Lines</th>
                  <th className="px-2 py-1.5 font-medium">Date</th>
                </tr>
              </thead>
              <tbody>
                {detail.commits.map((c) => (
                  <tr key={`${c.commit_hash}-${c.branch_name}`} className="border-b border-[#1e1f2a] hover:bg-[#1e1f2a]">
                    <td className="px-2 py-1.5 font-mono text-text-secondary">{c.commit_hash.slice(0, 8)}</td>
                    <td className="px-2 py-1.5 text-text-secondary">{c.branch_name}</td>
                    <td className="px-2 py-1.5 text-text-primary truncate max-w-[260px]">{c.commit_message ?? "—"}</td>
                    <td className="px-2 py-1.5 text-right font-mono">
                      <span className={c.ai_percentage > 50 ? "text-indigo-400" : "text-emerald-400"}>{c.ai_percentage.toFixed(1)}%</span>
                    </td>
                    <td className="px-2 py-1.5 text-right font-mono text-emerald-400">+{c.lines_added ?? 0}</td>
                    <td className="px-2 py-1.5 text-right font-mono text-red-400">-{c.lines_deleted ?? 0}</td>
                    <td className="px-2 py-1.5 text-right font-mono text-text-muted">+{c.tab_lines_added ?? 0}/-{c.tab_lines_deleted ?? 0}</td>
                    <td className="px-2 py-1.5 text-[10px] text-text-muted whitespace-nowrap">{formatDate(c.commit_date)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )
      )}

      {tab === "models" && (
        <div className="space-y-3">
          {Object.keys(detail.model_mix).length === 0 ? <p className="text-xs text-text-muted">No model data.</p> : (
            <div className="flex flex-wrap gap-2">
              {Object.entries(detail.model_mix).sort((a, b) => b[1] - a[1]).map(([model, count]) => (
                <span key={model} className="text-[10px] px-2 py-1 rounded border border-[#2a2b36] text-text-secondary">
                  {model} <span className="text-text-muted">×{count}</span>
                </span>
              ))}
            </div>
          )}
          <div>
            <p className="text-[10px] text-text-muted uppercase tracking-wider mb-1.5">Context pressure (sessions &gt; 80% used)</p>
            {detail.sessions.filter((s) => (s.context_usage_percent ?? 0) > 80).length === 0 ? (
              <p className="text-xs text-text-muted">No sessions near their context limit.</p>
            ) : (
              <ul className="space-y-1">
                {detail.sessions.filter((s) => (s.context_usage_percent ?? 0) > 80).map((s) => (
                  <li key={s.composer_id} className="text-xs flex justify-between">
                    <span className="text-text-secondary truncate max-w-[300px]">{s.name ?? s.composer_id.slice(0, 8)}</span>
                    <span className="text-red-400 font-mono">{s.context_usage_percent?.toFixed(0)}%</span>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </div>
      )}

      {tab === "generations" && (
        detail.generations.length === 0 ? <p className="text-xs text-text-muted">No generations recorded.</p> : (
          <ul className="space-y-1.5 max-h-[350px] overflow-y-auto">
            {detail.generations.map((g) => (
              <li key={g.generation_uuid} className="text-xs flex gap-2 items-start">
                <span className="text-[10px] text-text-muted whitespace-nowrap font-mono">{new Date(g.unix_ms).toLocaleString()}</span>
                <span className="text-text-secondary truncate">{g.text_description ?? g.kind}</span>
              </li>
            ))}
          </ul>
        )
      )}

      {tab === "mcps" && (
        detail.mcps.length === 0 ? <p className="text-xs text-text-muted">No MCP servers registered for this project.</p> : (
          <div className="space-y-1.5">
            {detail.mcps.map((m) => (
              <div key={m.server_identifier} className="flex items-center justify-between text-xs bg-[#1a1b23] border border-[#2a2b36] rounded px-3 py-2">
                <div>
                  <span className="text-text-primary">{m.server_name ?? m.server_identifier}</span>
                  <span className="ml-2 text-[10px] text-text-muted">{m.server_identifier}</span>
                </div>
                {m.status_summary && <span className="text-[10px] text-text-muted truncate max-w-[300px]">{m.status_summary}</span>}
              </div>
            ))}
          </div>
        )
      )}

      {tab === "plans" && (
        detail.plans.length === 0 ? <p className="text-xs text-text-muted">No plans found for this project.</p> : (
          <div className="space-y-2">
            {detail.plans.map((p) => {
              const pct = p.total_todos > 0 ? (p.completed_todos / p.total_todos) * 100 : 0;
              return (
                <div key={p.file_path} className="bg-[#1a1b23] border border-[#2a2b36] rounded px-3 py-2">
                  <div className="flex justify-between text-xs mb-1">
                    <span className="text-text-primary">{p.name}</span>
                    <span className="text-text-muted">{p.completed_todos}/{p.total_todos} todos</span>
                  </div>
                  <div className="h-1.5 bg-[#0e0f13] rounded-full overflow-hidden">
                    <div className="h-full bg-emerald-500 rounded-full" style={{ width: `${pct}%` }} />
                  </div>
                </div>
              );
            })}
          </div>
        )
      )}
    </div>
  );
}

// ── Project row ──────────────────────────────────────────────────────────

function ProjectRow({ p, expanded, onToggle }: { p: CursorProjectStat; expanded: boolean; onToggle: () => void }) {
  const [copied, setCopied] = useState(false);
  const copyDir = async (e: React.MouseEvent) => {
    e.stopPropagation();
    try {
      await navigator.clipboard.writeText(p.path);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch { /* clipboard unavailable */ }
  };
  const startSession = (e: React.MouseEvent) => {
    e.stopPropagation();
    startCursorInProject(p.path).catch(() => {});
  };

  return (
    <>
      <tr onClick={onToggle} className="border-b border-[#1e1f2a] hover:bg-[#22232e] cursor-pointer">
        <td className="px-3 py-2 w-[240px] max-w-[240px]">
          <div className="flex items-center gap-1.5 min-w-0">
            <svg className={`w-3 h-3 text-text-muted shrink-0 transition-transform ${expanded ? "rotate-90" : ""}`} fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M9 5l7 7-7 7" />
            </svg>
            <div className="flex flex-col gap-0.5 min-w-0">
              <span className="block truncate text-text-primary" title={p.path}>{p.name}</span>
              <span className="text-[10px] text-text-muted">{timeAgo(p.last_activity)}</span>
            </div>
          </div>
        </td>
        <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(p.sessions)}</td>
        <td className="px-3 py-2 text-right text-text-muted font-mono">
          {formatNum(p.input_tokens)}/{formatNum(p.output_tokens)}
        </td>
        <td className="px-3 py-2 text-right font-mono">
          <span className="text-emerald-400">+{formatNum(p.lines_added)}</span>{" "}
          <span className="text-red-400">-{formatNum(p.lines_removed)}</span>
        </td>
        <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(p.files_changed)}</td>
        <td className="px-3 py-2 text-right">
          <div className="flex items-center justify-end gap-1.5">
            <span className="text-text-secondary font-mono">{formatNum(p.commit_count)}</span>
            {p.commit_count > 0 && <AiPctChip pct={p.ai_line_pct} />}
          </div>
        </td>
        <td className="px-3 py-2">
          <div className="flex items-center gap-1" onClick={(e) => e.stopPropagation()}>
            <button
              onClick={startSession}
              title="Open this project in Cursor"
              className="text-[10px] px-1.5 py-0.5 rounded bg-blue-500/15 text-blue-400 hover:bg-blue-500/25 whitespace-nowrap"
            >
              Start session
            </button>
            <button
              onClick={copyDir}
              title="Copy the project directory path"
              className="text-[10px] px-1.5 py-0.5 rounded border border-[#2a2b36] text-text-secondary hover:text-text-primary whitespace-nowrap w-[92px]"
            >
              {copied ? "Copied" : "Copy directory"}
            </button>
          </div>
        </td>
      </tr>
      {expanded && (
        <tr>
          <td colSpan={7} className="p-0">
            <ProjectDetailPanel path={p.path} />
          </td>
        </tr>
      )}
    </>
  );
}

// ── Page ─────────────────────────────────────────────────────────────────

export function CursorProjectsPage() {
  const [overview, setOverview] = useState<CursorProjectsOverview | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expandedPath, setExpandedPath] = useState<string | null>(null);

  const load = useCallback((forceRefresh = false) => {
    setLoading(true);
    getCursorProjectsOverview(forceRefresh)
      .then((o) => { setOverview(o); setError(null); })
      .catch((e) => setError(String(e)))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => { load(false); }, [load]);

  // While a commit-history scan is still resolving in the background, poll
  // once more after a few seconds so the AI% chips / commit counts update
  // without the user having to hit Refresh.
  useEffect(() => {
    if (!overview?.commit_resolution_pending) return;
    const id = setTimeout(() => load(false), 4000);
    return () => clearTimeout(id);
  }, [overview?.commit_resolution_pending, load]);

  return (
    <div className="p-6 space-y-6 overflow-y-auto h-full">
      <div className="flex items-center justify-between flex-wrap gap-3">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Cursor Projects</h1>
          <p className="text-sm text-text-secondary mt-1">
            Every project Cursor has been used in, with per-project sessions, tokens, lines, and commits.
          </p>
          <DebugPath path="~/Library/Application Support/Cursor/User/globalStorage/state.vscdb" />
        </div>
        <button
          onClick={() => load(true)}
          disabled={loading}
          className="px-3 py-1.5 text-sm bg-app-card border border-border rounded-lg hover:bg-app-card-hover disabled:opacity-50"
        >
          {loading ? "Loading..." : "Refresh"}
        </button>
      </div>

      {error && <p className="text-sm text-accent-red">{error}</p>}

      {overview && (
        <>
          <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
            <StatCard label="Projects" value={formatNum(overview.projects.length)} />
            <StatCard label="Sessions" value={formatNum(overview.totals.sessions)} />
            <StatCard label="Tokens" value={formatNum(overview.totals.input_tokens + overview.totals.output_tokens)} sub={`${formatNum(overview.totals.input_tokens)} in · ${formatNum(overview.totals.output_tokens)} out`} />
            <StatCard label="Lines" value={`+${formatNum(overview.totals.lines_added)}/-${formatNum(overview.totals.lines_removed)}`} />
            <StatCard label="Files changed" value={formatNum(overview.totals.files_changed)} />
            <StatCard label="Commits" value={formatNum(overview.totals.commit_count)} />
            <StatCard label="Unresolved sessions" value={formatNum(overview.unresolved_sessions)} sub="older chats predate project tracking" />
            <StatCard label="Unattributed commits" value={formatNum(overview.unattributed_commits)} sub="no local git repo matched" />
          </div>

          {overview.commit_resolution_pending && (
            <div className="flex items-center gap-2 text-xs text-text-muted bg-[#1a1b23] border border-[#2a2b36] rounded-lg px-3 py-2">
              <svg className="w-3 h-3 animate-spin" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M12 3a9 9 0 1 0 9 9" />
              </svg>
              Resolving commit history against local git repos…
            </div>
          )}

          <Section title="Projects" info="Commits are matched to a project by scanning locally-cloned git repos; sessions are resolved from Cursor's own workspace and chat-storage records.">
            {overview.projects.length === 0 ? (
              <p className="text-sm text-text-muted">No Cursor projects found yet.</p>
            ) : (
              <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-x-auto">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="text-left text-text-muted border-b border-[#2a2b36]">
                      <th className="px-3 py-2 font-medium">Project</th>
                      <th className="px-3 py-2 font-medium text-right">Sessions</th>
                      <th className="px-3 py-2 font-medium text-right">Tokens in/out</th>
                      <th className="px-3 py-2 font-medium text-right">±Lines</th>
                      <th className="px-3 py-2 font-medium text-right">Files</th>
                      <th className="px-3 py-2 font-medium text-right">Commits</th>
                      <th className="px-3 py-2 font-medium">Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {overview.projects.map((p) => (
                      <ProjectRow
                        key={p.path}
                        p={p}
                        expanded={expandedPath === p.path}
                        onToggle={() => setExpandedPath(expandedPath === p.path ? null : p.path)}
                      />
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </Section>
        </>
      )}
    </div>
  );
}
