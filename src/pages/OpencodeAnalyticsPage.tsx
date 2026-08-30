/**
 * OpenCode Analytics Page
 * Shows connection status and local session/token/cost analytics read from
 * ~/.local/share/opencode/opencode.db. Costs are real (OpenCode computes and
 * stores them itself) — no "estimated" language here, unlike Codex.
 */
import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  PieChart,
  Pie,
  Cell,
  BarChart,
  Bar,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
} from "recharts";

// ── Types ───────────────────────────────────────────────────────────────────

interface ProviderStatus {
  provider_id: string;
  provider_name: string;
  connected: boolean;
  connection_method: string;
  account_email: string | null;
  plan_name: string | null;
  org_name: string | null;
  error: string | null;
}

interface OpencodeSession {
  id: string;
  title: string | null;
  model: string | null;
  tokens_used: number;
  cost: number;
  directory: string | null;
  agent: string | null;
  created_at: string | null;
  updated_at: string | null;
}

interface AuthProviderInfo {
  provider_id: string;
  type: string;
}

interface ProviderAnalytics {
  provider_id: string;
  provider_name: string;
  status: ProviderStatus;
  extra: Record<string, unknown>;
  fetched_at: string;
}

// ── Accent color ────────────────────────────────────────────────────────────

const ACCENT = "#f5a623";
const CHART_COLORS = [
  "#f5a623", "#6366f1", "#10a37f", "#ef4444", "#8b5cf6",
  "#ec4899", "#14b8a6", "#f97316", "#06b6d4", "#84cc16",
];

// ── Helpers ─────────────────────────────────────────────────────────────────

function timeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const m = Math.floor(diff / 60000);
  if (m < 1) return "just now";
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}

function formatTokens(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function formatUsd(n: number): string {
  if (n >= 1000) return `$${(n / 1000).toFixed(1)}K`;
  if (n >= 0.01) return `$${n.toFixed(2)}`;
  if (n > 0) return `$${n.toFixed(4)}`;
  return "$0.00";
}

function projectName(dir: string | null): string {
  if (!dir) return "Unknown";
  const parts = dir.split("/");
  return parts[parts.length - 1] || dir;
}

function formatDate(iso: string | null): string {
  if (!iso) return "--";
  try {
    const dt = new Date(iso);
    return dt.toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return iso;
  }
}

// ── Reusable UI components ──────────────────────────────────────────────────

function StatCard({ label, value, sub }: { label: string; value: string | number; sub?: string }) {
  return (
    <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg p-4">
      <p className="text-[10px] text-text-muted uppercase tracking-wider mb-1">{label}</p>
      <p className="text-xl font-semibold text-text-primary">{value}</p>
      {sub && <p className="text-[10px] text-text-muted mt-0.5">{sub}</p>}
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="mb-6">
      <h2 className="text-xs font-semibold text-text-secondary uppercase tracking-wider mb-3">{title}</h2>
      {children}
    </div>
  );
}

function Badge({ children, color }: { children: React.ReactNode; color?: string }) {
  return (
    <span
      className="inline-flex items-center px-2 py-0.5 rounded text-[10px] font-medium"
      style={{
        backgroundColor: (color ?? ACCENT) + "20",
        color: color ?? ACCENT,
      }}
    >
      {children}
    </span>
  );
}

// ── Skeleton ────────────────────────────────────────────────────────────────

function OpencodeDashboardSkeleton() {
  return (
    <div className="px-6 py-6 animate-pulse">
      <div className="h-5 w-48 bg-[#1a1b23] rounded mb-2" />
      <div className="h-3 w-72 bg-[#1a1b23] rounded mb-6" />
      <div className="grid grid-cols-3 gap-4 mb-6">
        {[1, 2, 3].map((i) => (
          <div key={i} className="h-20 bg-[#1a1b23] rounded-lg" />
        ))}
      </div>
      <div className="h-64 bg-[#1a1b23] rounded-lg" />
    </div>
  );
}

// ── Not Connected ───────────────────────────────────────────────────────────

function NotConnected({ error }: { error?: string | null }) {
  return (
    <div className="px-6 py-6">
      <h1 className="text-lg font-semibold text-text-primary mb-2">OpenCode Analytics</h1>
      <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg p-6 text-center max-w-md mx-auto mt-8">
        <div className="w-12 h-12 rounded-full mx-auto mb-4 flex items-center justify-center text-2xl" style={{ backgroundColor: ACCENT + "20" }}>
          <span style={{ color: ACCENT }}>&#x2318;</span>
        </div>
        <h2 className="text-sm font-semibold text-text-primary mb-2">OpenCode Not Connected</h2>
        <p className="text-xs text-text-muted mb-4">
          Install OpenCode and run a session to see your analytics. AgentHarbor reads local
          session data from <code className="font-mono text-[10px] bg-[#22232e] px-1 py-0.5 rounded">~/.local/share/opencode/opencode.db</code>.
        </p>
        {error && (
          <p className="text-[10px] text-red-400 bg-red-500/10 border border-red-500/20 rounded px-3 py-2 mt-2">
            {error}
          </p>
        )}
      </div>
    </div>
  );
}

// ── Chart tooltip ───────────────────────────────────────────────────────────

function ChartTooltip({ active, payload }: { active?: boolean; payload?: Array<{ name: string; value: number }> }) {
  if (!active || !payload?.length) return null;
  return (
    <div className="bg-[#1a1b23] border border-[#2a2b36] rounded px-3 py-2 text-xs shadow-lg">
      <p className="text-text-primary font-medium">{payload[0].name}</p>
      <p className="text-text-muted">{formatTokens(payload[0].value)} tokens</p>
    </div>
  );
}

// ── Usage window row (today / this week / all time) ─────────────────────────

function WindowRow({
  label,
  sessions,
  tokens,
  cost,
}: {
  label: string;
  sessions: number;
  tokens: number;
  cost: number;
}) {
  return (
    <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg p-4">
      <p className="text-[10px] text-text-muted uppercase tracking-wider mb-2">{label}</p>
      <div className="grid grid-cols-3 gap-2">
        <div>
          <p className="text-sm font-semibold text-text-primary">{sessions.toLocaleString()}</p>
          <p className="text-[9px] text-text-muted">sessions</p>
        </div>
        <div>
          <p className="text-sm font-semibold text-text-primary">{formatTokens(tokens)}</p>
          <p className="text-[9px] text-text-muted">tokens</p>
        </div>
        <div>
          <p className="text-sm font-semibold text-emerald-400">{formatUsd(cost)}</p>
          <p className="text-[9px] text-text-muted">cost</p>
        </div>
      </div>
    </div>
  );
}

// ── Main Component ──────────────────────────────────────────────────────────

export function OpencodeAnalyticsPage() {
  const [analytics, setAnalytics] = useState<ProviderAnalytics | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [lastRefreshed, setLastRefreshed] = useState<string | null>(null);
  const [sessionPage, setSessionPage] = useState(0);
  const SESSIONS_PER_PAGE = 15;

  const [, setTick] = useState(0);
  useEffect(() => {
    const interval = setInterval(() => setTick((t) => t + 1), 30000);
    return () => clearInterval(interval);
  }, []);

  const loadData = useCallback(async () => {
    setLoading(true);
    setLoadError(null);
    try {
      const data = await invoke<ProviderAnalytics>("get_provider_analytics", {
        providerId: "opencode",
      });
      setAnalytics(data);
      setLastRefreshed(data.fetched_at);
    } catch (err) {
      console.error("[OpenCode] Analytics load error:", err);
      setLoadError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  useEffect(() => {
    const interval = setInterval(() => loadData(), 5 * 60 * 1000);
    return () => clearInterval(interval);
  }, [loadData]);

  // ── Early returns (after all hooks) ──────────────────────────────────

  if (loading && !analytics) {
    return <OpencodeDashboardSkeleton />;
  }

  if (loadError && !analytics) {
    return (
      <div className="p-6">
        <div className="bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-xs text-red-400">
          <p className="font-medium mb-1">Failed to load OpenCode analytics</p>
          <p className="text-red-400/70">{loadError}</p>
          <button
            onClick={() => loadData()}
            className="mt-3 px-3 py-1.5 bg-red-500/20 rounded text-red-300 hover:bg-red-500/30"
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  if (!analytics?.status.connected) {
    return <NotConnected error={analytics?.status.error} />;
  }

  // ── Derived data ─────────────────────────────────────────────────────

  const { status, extra } = analytics;

  const totalSessions = (extra.total_sessions as number) ?? 0;
  const totalTokensUsed = (extra.total_tokens_used as number) ?? 0;
  const totalCost = (extra.estimated_total_cost as number) ?? 0;
  const sessions = (extra.sessions as OpencodeSession[] | undefined) ?? [];
  const tokensByModel = (extra.tokens_by_model as Record<string, number> | undefined) ?? {};
  const costByModel = (extra.cost_by_model as Record<string, number> | undefined) ?? {};
  const sessionsByProject = (extra.sessions_by_project as Record<string, number> | undefined) ?? {};
  const activeNow = extra.active_now === true;
  const authProviders = (extra.auth_providers as AuthProviderInfo[] | undefined) ?? [];
  const hasLocalData = totalSessions > 0;

  const startTodaySessions = (extra.start_today_sessions as number | undefined) ?? 0;
  const startTodayTokens = (extra.start_today_tokens as number | undefined) ?? 0;
  const startTodayCost = (extra.start_today_cost as number | undefined) ?? 0;
  const thisWeekSessions = (extra.this_week_sessions as number | undefined) ?? 0;
  const thisWeekTokens = (extra.this_week_tokens as number | undefined) ?? 0;
  const thisWeekCost = (extra.this_week_cost as number | undefined) ?? 0;

  const paginatedSessions = sessions.slice(
    sessionPage * SESSIONS_PER_PAGE,
    (sessionPage + 1) * SESSIONS_PER_PAGE
  );
  const totalSessionPages = Math.ceil(sessions.length / SESSIONS_PER_PAGE);

  return (
    <div className="h-full overflow-y-auto relative">
      {loading && (
        <div className="sticky top-0 z-50 w-full">
          <div className="h-0.5 bg-[#0e0f13] w-full overflow-hidden">
            <div
              className="h-full animate-[opencode-loading-bar_1.5s_ease-in-out_infinite] w-1/3 rounded-full"
              style={{ backgroundColor: ACCENT }}
            />
          </div>
          <style>{`@keyframes opencode-loading-bar { 0% { transform: translateX(-100%); } 100% { transform: translateX(400%); } }`}</style>
        </div>
      )}

      <div className="px-6 py-6">
        {/* Header */}
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-lg font-semibold text-text-primary flex items-center gap-2">
              OpenCode Analytics
              {loading && (
                <span
                  className="inline-block w-2 h-2 rounded-full animate-pulse"
                  style={{ backgroundColor: ACCENT }}
                  title="Refreshing..."
                />
              )}
              {activeNow && (
                <span className="inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded whitespace-nowrap bg-emerald-500/20 text-emerald-400">
                  <span className="inline-block w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse" />
                  Active now
                </span>
              )}
            </h1>
            <p className="text-xs text-text-muted">
              <span style={{ color: ACCENT }}>Local data</span>
              {status.connection_method === "local-file" && " · read from opencode.db"}
            </p>
          </div>
          <div className="flex items-center gap-2">
            {lastRefreshed && (
              <span className="text-[10px] text-text-muted">Updated {timeAgo(lastRefreshed)}</span>
            )}
            <button
              onClick={() => loadData()}
              className={`p-1.5 rounded transition-colors ${
                loading
                  ? "bg-opacity-10"
                  : "text-text-muted hover:text-text-primary hover:bg-[#1a1b23]"
              }`}
              style={loading ? { color: ACCENT, backgroundColor: ACCENT + "15" } : undefined}
              title="Refresh"
            >
              <svg
                className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`}
                fill="none"
                viewBox="0 0 24 24"
                stroke="currentColor"
                strokeWidth={2}
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"
                />
              </svg>
            </button>
          </div>
        </div>

        {/* Auth providers */}
        {authProviders.length > 0 && (
          <Section title="Connected Providers">
            <div className="flex flex-wrap gap-2">
              {authProviders.map((p) => (
                <Badge key={p.provider_id} color="#6366f1">
                  {p.provider_id} ({p.type})
                </Badge>
              ))}
            </div>
          </Section>
        )}

        {/* Session Stats */}
        {hasLocalData && (
          <Section title="Session Stats">
            <div className="grid grid-cols-4 gap-4">
              <StatCard label="Total Sessions" value={totalSessions.toLocaleString()} sub="All time" />
              <StatCard label="Total Tokens" value={formatTokens(totalTokensUsed)} sub={`${totalTokensUsed.toLocaleString()} exact`} />
              <StatCard label="Total Cost" value={formatUsd(totalCost)} sub="Real cost, from OpenCode" />
              <StatCard label="Projects" value={Object.keys(sessionsByProject).length} />
            </div>
          </Section>
        )}

        {/* Usage Windows */}
        {hasLocalData && (
          <Section title="Usage">
            <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
              <WindowRow label="Today" sessions={startTodaySessions} tokens={startTodayTokens} cost={startTodayCost} />
              <WindowRow label="This Week" sessions={thisWeekSessions} tokens={thisWeekTokens} cost={thisWeekCost} />
              <WindowRow label="All Time" sessions={totalSessions} tokens={totalTokensUsed} cost={totalCost} />
            </div>
          </Section>
        )}

        {/* Cost by Model */}
        {Object.keys(costByModel).length > 0 && (
          <Section title="Cost by Model">
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-hidden">
              <table className="w-full text-xs">
                <thead>
                  <tr className="border-b border-[#2a2b36] text-text-muted">
                    <th className="text-left px-3 py-2">Model</th>
                    <th className="text-right px-3 py-2">Tokens</th>
                    <th className="text-right px-3 py-2">Cost</th>
                  </tr>
                </thead>
                <tbody>
                  {Object.entries(costByModel)
                    .sort((a, b) => b[1] - a[1])
                    .map(([model, cost]) => (
                      <tr key={model} className="border-b border-[#1e1f2a] hover:bg-[#22232e]">
                        <td className="px-3 py-2 text-text-primary font-medium">{model}</td>
                        <td className="px-3 py-2 text-right text-text-muted font-mono">{formatTokens(tokensByModel[model] ?? 0)}</td>
                        <td className="px-3 py-2 text-right text-emerald-400 font-mono font-semibold">{formatUsd(cost)}</td>
                      </tr>
                    ))}
                  <tr className="bg-[#22232e]">
                    <td className="px-3 py-2 text-text-primary font-semibold">Total</td>
                    <td className="px-3 py-2 text-right text-text-primary font-mono font-semibold">{formatTokens(totalTokensUsed)}</td>
                    <td className="px-3 py-2 text-right text-emerald-400 font-mono font-semibold">{formatUsd(totalCost)}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </Section>
        )}

        {/* Charts — Tokens by Model & Sessions by Project */}
        {hasLocalData && (Object.keys(tokensByModel).length > 0 || Object.keys(sessionsByProject).length > 0) && (
          <Section title="Usage Breakdown">
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
              {Object.keys(tokensByModel).length > 0 && (
                <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg p-4">
                  <p className="text-[10px] text-text-muted uppercase tracking-wider mb-3">Tokens by Model</p>
                  <div className="h-48">
                    <ResponsiveContainer width="100%" height="100%">
                      <PieChart>
                        <Pie
                          data={Object.entries(tokensByModel)
                            .sort((a, b) => b[1] - a[1])
                            .map(([name, value]) => ({ name, value }))}
                          dataKey="value"
                          nameKey="name"
                          cx="50%"
                          cy="50%"
                          outerRadius={65}
                          innerRadius={35}
                          paddingAngle={2}
                        >
                          {Object.entries(tokensByModel)
                            .sort((a, b) => b[1] - a[1])
                            .map((_, i) => (
                              <Cell key={i} fill={CHART_COLORS[i % CHART_COLORS.length]} />
                            ))}
                        </Pie>
                        <Tooltip content={<ChartTooltip />} />
                      </PieChart>
                    </ResponsiveContainer>
                  </div>
                  <div className="flex flex-wrap gap-2 mt-2">
                    {Object.entries(tokensByModel)
                      .sort((a, b) => b[1] - a[1])
                      .slice(0, 6)
                      .map(([model, tokens], i) => (
                        <span key={model} className="inline-flex items-center gap-1 text-[10px] text-text-muted">
                          <span
                            className="inline-block w-2 h-2 rounded-full"
                            style={{ backgroundColor: CHART_COLORS[i % CHART_COLORS.length] }}
                          />
                          {model}: {formatTokens(tokens)}
                        </span>
                      ))}
                  </div>
                </div>
              )}

              {Object.keys(sessionsByProject).length > 0 && (
                <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg p-4">
                  <p className="text-[10px] text-text-muted uppercase tracking-wider mb-3">Sessions by Project</p>
                  <div className="h-48">
                    <ResponsiveContainer width="100%" height="100%">
                      <BarChart
                        data={Object.entries(sessionsByProject)
                          .sort((a, b) => b[1] - a[1])
                          .slice(0, 8)
                          .map(([name, value]) => ({ name, value }))}
                        layout="vertical"
                        margin={{ left: 0, right: 10, top: 0, bottom: 0 }}
                      >
                        <XAxis type="number" hide />
                        <YAxis
                          type="category"
                          dataKey="name"
                          width={100}
                          tick={{ fontSize: 10, fill: "#9394a1" }}
                          tickLine={false}
                          axisLine={false}
                        />
                        <Tooltip
                          content={({ active, payload }) => {
                            if (!active || !payload?.length) return null;
                            return (
                              <div className="bg-[#1a1b23] border border-[#2a2b36] rounded px-3 py-2 text-xs shadow-lg">
                                <p className="text-text-primary font-medium">{payload[0].payload.name}</p>
                                <p className="text-text-muted">{payload[0].value} sessions</p>
                              </div>
                            );
                          }}
                        />
                        <Bar dataKey="value" fill={ACCENT} radius={[0, 4, 4, 0]} barSize={14} />
                      </BarChart>
                    </ResponsiveContainer>
                  </div>
                </div>
              )}
            </div>
          </Section>
        )}

        {/* Recent Sessions Table */}
        {sessions.length > 0 && (
          <Section title="Recent Sessions">
            <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg overflow-hidden">
              <table className="w-full text-xs">
                <thead>
                  <tr className="border-b border-[#2a2b36]">
                    <th className="text-left px-4 py-2.5 text-text-muted font-medium uppercase tracking-wider text-[10px]">Title</th>
                    <th className="text-left px-4 py-2.5 text-text-muted font-medium uppercase tracking-wider text-[10px]">Model</th>
                    <th className="text-right px-4 py-2.5 text-text-muted font-medium uppercase tracking-wider text-[10px]">Tokens</th>
                    <th className="text-right px-4 py-2.5 text-text-muted font-medium uppercase tracking-wider text-[10px]">Cost</th>
                    <th className="text-left px-4 py-2.5 text-text-muted font-medium uppercase tracking-wider text-[10px]">Project</th>
                    <th className="text-right px-4 py-2.5 text-text-muted font-medium uppercase tracking-wider text-[10px]">Updated</th>
                  </tr>
                </thead>
                <tbody>
                  {paginatedSessions.map((session) => (
                    <tr key={session.id} className="border-b border-[#2a2b36] last:border-b-0 hover:bg-[#22232e] transition-colors">
                      <td className="px-4 py-2.5 text-text-primary max-w-[200px] truncate">
                        {session.title || <span className="text-text-muted italic">Untitled</span>}
                      </td>
                      <td className="px-4 py-2.5">
                        {session.model ? <Badge>{session.model}</Badge> : <span className="text-text-muted">--</span>}
                      </td>
                      <td className="px-4 py-2.5 text-right font-mono text-text-secondary">{formatTokens(session.tokens_used)}</td>
                      <td className="px-4 py-2.5 text-right font-mono text-emerald-400">{formatUsd(session.cost)}</td>
                      <td className="px-4 py-2.5 text-text-muted max-w-[120px] truncate">{projectName(session.directory)}</td>
                      <td className="px-4 py-2.5 text-right text-text-muted whitespace-nowrap">
                        {formatDate(session.updated_at ?? session.created_at)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {totalSessionPages > 1 && (
                <div className="flex items-center justify-between px-4 py-2.5 border-t border-[#2a2b36]">
                  <span className="text-[10px] text-text-muted">
                    Showing {sessionPage * SESSIONS_PER_PAGE + 1}--
                    {Math.min((sessionPage + 1) * SESSIONS_PER_PAGE, sessions.length)} of {sessions.length}
                  </span>
                  <div className="flex items-center gap-1">
                    <button
                      onClick={() => setSessionPage((p) => Math.max(0, p - 1))}
                      disabled={sessionPage === 0}
                      className="px-2 py-1 text-[10px] rounded text-text-muted hover:text-text-primary hover:bg-[#22232e] disabled:opacity-30 disabled:cursor-not-allowed"
                    >
                      Prev
                    </button>
                    <span className="text-[10px] text-text-muted px-1">
                      {sessionPage + 1} / {totalSessionPages}
                    </span>
                    <button
                      onClick={() => setSessionPage((p) => Math.min(totalSessionPages - 1, p + 1))}
                      disabled={sessionPage >= totalSessionPages - 1}
                      className="px-2 py-1 text-[10px] rounded text-text-muted hover:text-text-primary hover:bg-[#22232e] disabled:opacity-30 disabled:cursor-not-allowed"
                    >
                      Next
                    </button>
                  </div>
                </div>
              )}
            </div>
          </Section>
        )}

        {/* Extra info badges — filter out keys we already display */}
        {(() => {
          const displayedKeys = new Set([
            "total_sessions", "total_tokens_used", "estimated_total_cost", "sessions",
            "tokens_by_model", "cost_by_model", "sessions_by_project", "active_now",
            "auth_providers", "start_today_sessions", "start_today_tokens", "start_today_cost",
            "this_week_sessions", "this_week_tokens", "this_week_cost",
          ]);
          const remainingExtra = Object.entries(extra).filter(([key]) => !displayedKeys.has(key));
          if (remainingExtra.length === 0) return null;
          return (
            <Section title="Details">
              <div className="flex flex-wrap gap-2">
                {remainingExtra.map(([key, value]) => (
                  <Badge key={key}>
                    {key.replace(/_/g, " ")}: {String(value)}
                  </Badge>
                ))}
              </div>
            </Section>
          );
        })()}
      </div>
    </div>
  );
}
