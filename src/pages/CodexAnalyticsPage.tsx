/**
 * Codex (OpenAI) Analytics Page
 * Shows connection status, account info, rate limits, credit usage,
 * and local session/usage analytics from ~/.codex/ files.
 */
import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
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

interface RateLimitWindow {
  provider_id: string;
  label: string;
  used_percent: number;
  remaining_percent: number;
  resets_at: string | null;
  resets_in_seconds: number | null;
  window_seconds: number | null;
}

interface CreditUsage {
  provider_id: string;
  used: number;
  limit: number | null;
  remaining: number;
  currency: string;
  billing_cycle_end: string | null;
  plan_name: string | null;
}

interface CodexSession {
  id: string;
  title: string | null;
  model: string | null;
  tokens_used: number;
  source: string | null;
  cwd: string | null;
  git_branch: string | null;
  reasoning_effort: string | null;
  created_at: string | null;
  updated_at: string | null;
}

interface OrgInfo {
  id: string | null;
  title: string | null;
  role: string | null;
}

interface ProviderAnalytics {
  provider_id: string;
  provider_name: string;
  status: ProviderStatus;
  rate_limits: RateLimitWindow[];
  credit_usage: CreditUsage | null;
  token_counts: unknown;
  extra: Record<string, unknown>;
  fetched_at: string;
}

// ── Accent color ────────────────────────────────────────────────────────────

const ACCENT = "#10a37f";
const CHART_COLORS = [
  "#10a37f", "#6366f1", "#f59e0b", "#ef4444", "#8b5cf6",
  "#ec4899", "#14b8a6", "#f97316", "#06b6d4", "#84cc16",
];

// ── Helpers ─────────────────────────────────────────────────────────────────

function formatDuration(seconds: number | null | undefined): string {
  if (!seconds) return "--";
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (h > 0 && m > 0) return `${h}h ${m}m`;
  if (h > 0) return `${h}h`;
  return `${m}m`;
}

function formatResetTime(resets_at: string | null | undefined): string {
  if (!resets_at) return "";
  try {
    const dt = new Date(resets_at);
    return dt.toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return resets_at;
  }
}

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
  if (n >= 1) return `$${n.toFixed(2)}`;
  if (n >= 0.01) return `$${n.toFixed(2)}`;
  if (n > 0) return `$${n.toFixed(4)}`;
  return "$0.00";
}

function projectName(cwd: string | null): string {
  if (!cwd) return "Unknown";
  const parts = cwd.split("/");
  return parts[parts.length - 1] || cwd;
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

function ProgressBar({ percent, label, resetInfo }: { percent: number; label: string; resetInfo?: string }) {
  const clamped = Math.min(Math.max(percent, 0), 100);
  const barColor = clamped > 90 ? "#ef4444" : clamped > 70 ? "#f59e0b" : ACCENT;

  return (
    <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg p-4">
      <div className="flex items-center justify-between mb-2">
        <span className="text-xs text-text-primary font-medium">{label}</span>
        <span className="text-xs font-semibold" style={{ color: barColor }}>
          {clamped.toFixed(1)}%
        </span>
      </div>
      <div className="w-full h-2 bg-[#2a2b36] rounded-full overflow-hidden">
        <div
          className="h-full rounded-full transition-all duration-500"
          style={{ width: `${clamped}%`, backgroundColor: barColor }}
        />
      </div>
      {resetInfo && (
        <p className="text-[10px] text-text-muted mt-1.5">Resets {resetInfo}</p>
      )}
    </div>
  );
}

// ── Skeleton ────────────────────────────────────────────────────────────────

function CodexDashboardSkeleton() {
  return (
    <div className="px-6 py-6 animate-pulse">
      <div className="h-5 w-48 bg-[#1a1b23] rounded mb-2" />
      <div className="h-3 w-72 bg-[#1a1b23] rounded mb-6" />
      <div className="grid grid-cols-4 gap-4 mb-6">
        {[1, 2, 3, 4].map((i) => (
          <div key={i} className="h-20 bg-[#1a1b23] rounded-lg" />
        ))}
      </div>
      <div className="grid grid-cols-2 gap-4 mb-6">
        {[1, 2].map((i) => (
          <div key={i} className="h-48 bg-[#1a1b23] rounded-lg" />
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
      <h1 className="text-lg font-semibold text-text-primary mb-2">Codex Analytics</h1>
      <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg p-6 text-center max-w-md mx-auto mt-8">
        <div className="w-12 h-12 rounded-full mx-auto mb-4 flex items-center justify-center text-2xl" style={{ backgroundColor: ACCENT + "20" }}>
          <span style={{ color: ACCENT }}>&#x229B;</span>
        </div>
        <h2 className="text-sm font-semibold text-text-primary mb-2">Codex Not Connected</h2>
        <p className="text-xs text-text-muted mb-4">
          Install OpenAI Codex and sign in to see your analytics. AgentHarbor auto-detects
          credentials from <code className="font-mono text-[10px] bg-[#22232e] px-1 py-0.5 rounded">~/.codex/auth.json</code>.
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

// ── Main Component ──────────────────────────────────────────────────────────

export function CodexAnalyticsPage() {
  const [analytics, setAnalytics] = useState<ProviderAnalytics | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [lastRefreshed, setLastRefreshed] = useState<string | null>(null);
  const [sessionPage, setSessionPage] = useState(0);
  const SESSIONS_PER_PAGE = 15;

  // Tick for "X ago" display refresh
  const [, setTick] = useState(0);
  useEffect(() => {
    const interval = setInterval(() => setTick((t) => t + 1), 30000);
    return () => clearInterval(interval);
  }, []);

  const loadData = useCallback(async (_forceRefresh = false) => {
    setLoading(true);
    setLoadError(null);
    try {
      const data = await invoke<ProviderAnalytics>("get_provider_analytics", {
        providerId: "codex",
      });
      setAnalytics(data);
      setLastRefreshed(data.fetched_at);
    } catch (err) {
      console.error("[Codex] Analytics load error:", err);
      setLoadError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData(false);
  }, [loadData]);

  // Auto-refresh every 5 minutes
  useEffect(() => {
    const interval = setInterval(() => loadData(true), 5 * 60 * 1000);
    return () => clearInterval(interval);
  }, [loadData]);

  // ── Early returns (after all hooks) ──────────────────────────────────

  if (loading && !analytics) {
    return <CodexDashboardSkeleton />;
  }

  if (loadError && !analytics) {
    return (
      <div className="p-6">
        <div className="bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-xs text-red-400">
          <p className="font-medium mb-1">Failed to load Codex analytics</p>
          <p className="text-red-400/70">{loadError}</p>
          <button
            onClick={() => loadData(true)}
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

  const { status, rate_limits, credit_usage, extra } = analytics;
  const planType = (extra.plan_type as string) ?? status.plan_name ?? "Unknown";
  const unlimitedCredits = extra.unlimited_credits === true;

  // Local data from extra
  const totalSessions = (extra.total_sessions as number) ?? 0;
  const totalTokensUsed = (extra.total_tokens_used as number) ?? 0;
  const sessions = (extra.sessions as CodexSession[] | undefined) ?? [];
  const tokensByModel = (extra.tokens_by_model as Record<string, number> | undefined) ?? {};
  const sessionsByProject = (extra.sessions_by_project as Record<string, number> | undefined) ?? {};
  const availableModels = (extra.available_models as string[] | undefined) ?? [];
  const configModel = (extra.config_model as string | undefined) ?? null;
  const configReasoning = (extra.config_reasoning_effort as string | undefined) ?? null;
  const accountName = (extra.account_name as string | undefined) ?? null;
  const organizations = (extra.organizations as OrgInfo[] | undefined) ?? [];
  const estimatedTotalCost = (extra.estimated_total_cost as number) ?? 0;
  const costByModel = (extra.cost_by_model as Record<string, number> | undefined) ?? {};
  const hasLocalData = totalSessions > 0;

  // Paginated sessions
  const paginatedSessions = sessions.slice(
    sessionPage * SESSIONS_PER_PAGE,
    (sessionPage + 1) * SESSIONS_PER_PAGE
  );
  const totalSessionPages = Math.ceil(sessions.length / SESSIONS_PER_PAGE);

  // Connection method display
  const connectionLabel =
    status.connection_method === "oauth-auto"
      ? "Auto-detected"
      : status.connection_method === "local-file"
      ? "Local data"
      : "Connected";

  return (
    <div className="h-full overflow-y-auto relative">
      {/* Top loading bar */}
      {loading && (
        <div className="sticky top-0 z-50 w-full">
          <div className="h-0.5 bg-[#0e0f13] w-full overflow-hidden">
            <div
              className="h-full animate-[codex-loading-bar_1.5s_ease-in-out_infinite] w-1/3 rounded-full"
              style={{ backgroundColor: ACCENT }}
            />
          </div>
          <style>{`@keyframes codex-loading-bar { 0% { transform: translateX(-100%); } 100% { transform: translateX(400%); } }`}</style>
        </div>
      )}

      <div className="px-6 py-6">
        {/* Header */}
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-lg font-semibold text-text-primary flex items-center gap-2">
              Codex Analytics
              {loading && (
                <span
                  className="inline-block w-2 h-2 rounded-full animate-pulse"
                  style={{ backgroundColor: ACCENT }}
                  title="Refreshing..."
                />
              )}
            </h1>
            <p className="text-xs text-text-muted">
              {accountName ?? status.account_email}
              {planType && (
                <span>
                  {" "}&middot;{" "}
                  <span className="capitalize">{planType}</span>
                </span>
              )}
              <span style={{ color: ACCENT }}>
                {" "}&middot;{" "}
                {connectionLabel}
              </span>
            </p>
          </div>
          <div className="flex items-center gap-2">
            {lastRefreshed && (
              <span className="text-[10px] text-text-muted">Updated {timeAgo(lastRefreshed)}</span>
            )}
            <button
              onClick={() => loadData(true)}
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

        {/* Account Overview */}
        <Section title="Account">
          <div className="grid grid-cols-3 gap-4">
            <StatCard
              label="Email"
              value={status.account_email ?? "N/A"}
              sub={accountName ?? undefined}
            />
            <StatCard
              label="Plan"
              value={planType}
              sub={unlimitedCredits ? "Unlimited credits" : undefined}
            />
            <StatCard
              label="Status"
              value={status.connected ? "Connected" : "Disconnected"}
              sub={status.connection_method === "oauth-auto" ? "Auto-detected from auth.json" : status.connection_method}
            />
          </div>
          {organizations.length > 0 && (
            <div className="flex flex-wrap gap-2 mt-3">
              {organizations.map((org, i) => (
                <Badge key={i} color="#6366f1">
                  {org.title ?? org.id ?? "Org"}{org.role ? ` (${org.role})` : ""}
                </Badge>
              ))}
            </div>
          )}
        </Section>

        {/* Session Stats — only show if we have local data */}
        {hasLocalData && (
          <Section title="Session Stats">
            <div className="grid grid-cols-4 gap-4">
              <StatCard
                label="Total Sessions"
                value={totalSessions.toLocaleString()}
                sub="All time"
              />
              <StatCard
                label="Total Tokens"
                value={formatTokens(totalTokensUsed)}
                sub={`${totalTokensUsed.toLocaleString()} exact`}
              />
              <StatCard
                label="Unique Models"
                value={Object.keys(tokensByModel).length}
                sub={configModel ? `Current: ${configModel}` : undefined}
              />
              <StatCard
                label="Projects"
                value={Object.keys(sessionsByProject).length}
              />
            </div>
          </Section>
        )}

        {/* Cost Analysis */}
        {hasLocalData && estimatedTotalCost > 0 && (
          <Section title="Cost Analysis">
            {/* ROI insight card */}
            <div className="bg-gradient-to-r from-[#1a1b23] to-[#1e1f2a] rounded-lg border border-[#2a2b36] p-4 mb-3">
              <div className="flex items-start justify-between mb-3">
                <div>
                  <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">API-Equivalent Value</div>
                  <div className="text-2xl font-bold text-emerald-400">{formatUsd(estimatedTotalCost)}</div>
                  <div className="text-[10px] text-text-muted mt-0.5">Estimated cost at OpenAI API pay-per-token rates (all time)</div>
                </div>
                <div className="text-right">
                  <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">Per Message</div>
                  <div className="text-lg font-semibold text-text-primary">
                    {totalSessions > 0 ? formatUsd(estimatedTotalCost / totalSessions) : "$0.00"}
                  </div>
                  <div className="text-[10px] text-text-muted mt-0.5">avg cost per session</div>
                </div>
              </div>
              <div className="bg-[#0e0f13] rounded-lg p-3 text-[10px] text-text-muted leading-relaxed">
                <span className="text-amber-400 font-medium">How to read this:</span> Your Team subscription includes all this usage in your flat monthly fee.
                The &quot;{formatUsd(estimatedTotalCost)}&quot; represents what equivalent API usage would cost without a subscription &mdash; it shows the
                <span className="text-emerald-400 font-medium"> compute value</span> you&apos;re getting from your plan, not what you&apos;re being charged.
              </div>
            </div>

            {/* Per-model cost breakdown */}
            {Object.keys(costByModel).length > 0 && (
              <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-hidden">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="border-b border-[#2a2b36] text-text-muted">
                      <th className="text-left px-3 py-2">Model</th>
                      <th className="text-right px-3 py-2">Tokens</th>
                      <th className="text-right px-3 py-2">Est. Cost</th>
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
                      <td className="px-3 py-2 text-right text-emerald-400 font-mono font-semibold">{formatUsd(estimatedTotalCost)}</td>
                    </tr>
                  </tbody>
                </table>

                {/* Pricing reference */}
                <div className="border-t border-[#2a2b36] px-3 py-2">
                  <div className="text-[9px] text-text-muted mb-1">
                    <Badge>API REFERENCE RATES</Badge>{" "}
                    <a href="https://openai.com/api/pricing/" target="_blank" rel="noopener noreferrer" className="text-[#10a37f] hover:underline">
                      OpenAI Pricing
                    </a>{" "}
                    &mdash; estimated combined (avg input+output) per Mtok:
                  </div>
                  <div className="flex flex-wrap gap-x-4 gap-y-1 text-[9px] text-text-muted font-mono">
                    <span><span className="text-emerald-400">GPT-5.4:</span> ~$10/Mtok</span>
                    <span><span className="text-emerald-400">GPT-5.4-mini:</span> ~$2.5/Mtok</span>
                    <span><span className="text-blue-400">GPT-5.3-codex:</span> ~$10/Mtok</span>
                    <span><span className="text-blue-400">GPT-5.1-codex:</span> ~$7.5/Mtok</span>
                    <span><span className="text-purple-400">GPT-5:</span> ~$5/Mtok</span>
                  </div>
                </div>
              </div>
            )}
          </Section>
        )}

        {/* Model Configuration */}
        {(configModel || configReasoning) && (
          <Section title="Configuration">
            <div className="grid grid-cols-3 gap-4">
              {configModel && (
                <StatCard
                  label="Model"
                  value={configModel}
                  sub="From config.toml"
                />
              )}
              {configReasoning && (
                <StatCard
                  label="Reasoning Effort"
                  value={configReasoning}
                  sub="From config.toml"
                />
              )}
              {availableModels.length > 0 && (
                <StatCard
                  label="Available Models"
                  value={availableModels.length}
                  sub="From models cache"
                />
              )}
            </div>
          </Section>
        )}

        {/* Charts — Tokens by Model & Sessions by Project */}
        {hasLocalData && (Object.keys(tokensByModel).length > 0 || Object.keys(sessionsByProject).length > 0) && (
          <Section title="Usage Breakdown">
            <div className="grid grid-cols-1 lg:grid-cols-2 gap-4">
              {/* Tokens by Model — Pie Chart */}
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
                        <span
                          key={model}
                          className="inline-flex items-center gap-1 text-[10px] text-text-muted"
                        >
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

              {/* Sessions by Project — Bar Chart */}
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

        {/* Rate Limits */}
        {rate_limits.length > 0 && (
          <Section title="Rate Limits">
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
              {rate_limits.map((rl) => {
                const resetInfo = rl.resets_at
                  ? formatResetTime(rl.resets_at)
                  : rl.resets_in_seconds
                  ? `in ${formatDuration(rl.resets_in_seconds)}`
                  : undefined;

                return (
                  <ProgressBar
                    key={rl.label}
                    label={rl.label}
                    percent={rl.used_percent}
                    resetInfo={resetInfo}
                  />
                );
              })}
            </div>
          </Section>
        )}

        {/* Credits */}
        {credit_usage && (
          <Section title="Credits">
            <div className="grid grid-cols-3 gap-4">
              <StatCard
                label="Balance"
                value={
                  unlimitedCredits
                    ? "Unlimited"
                    : `${credit_usage.remaining.toFixed(2)} ${credit_usage.currency}`
                }
              />
              <StatCard
                label="Used"
                value={`${credit_usage.used.toFixed(2)} ${credit_usage.currency}`}
              />
              <StatCard
                label="Plan"
                value={credit_usage.plan_name ?? planType}
                sub={credit_usage.billing_cycle_end ? `Resets ${formatResetTime(credit_usage.billing_cycle_end)}` : undefined}
              />
            </div>
            <div className="flex justify-end mt-2">
              <a
                href="https://openai.com/api/pricing/"
                target="_blank"
                rel="noopener noreferrer"
                className="text-xs text-text-muted hover:text-blue-400 transition-colors inline-flex items-center gap-1"
                onClick={async (e) => {
                  e.preventDefault();
                  await openUrl("https://openai.com/api/pricing/");
                }}
              >
                View pricing
                <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                </svg>
              </a>
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
                    <th className="text-left px-4 py-2.5 text-text-muted font-medium uppercase tracking-wider text-[10px]">Project</th>
                    <th className="text-left px-4 py-2.5 text-text-muted font-medium uppercase tracking-wider text-[10px]">Branch</th>
                    <th className="text-right px-4 py-2.5 text-text-muted font-medium uppercase tracking-wider text-[10px]">Date</th>
                  </tr>
                </thead>
                <tbody>
                  {paginatedSessions.map((session) => (
                    <tr
                      key={session.id}
                      className="border-b border-[#2a2b36] last:border-b-0 hover:bg-[#22232e] transition-colors"
                    >
                      <td className="px-4 py-2.5 text-text-primary max-w-[200px] truncate">
                        {session.title || <span className="text-text-muted italic">Untitled</span>}
                      </td>
                      <td className="px-4 py-2.5">
                        {session.model ? (
                          <Badge>{session.model}</Badge>
                        ) : (
                          <span className="text-text-muted">--</span>
                        )}
                      </td>
                      <td className="px-4 py-2.5 text-right font-mono text-text-secondary">
                        {formatTokens(session.tokens_used)}
                      </td>
                      <td className="px-4 py-2.5 text-text-muted max-w-[120px] truncate">
                        {projectName(session.cwd)}
                      </td>
                      <td className="px-4 py-2.5 text-text-muted max-w-[100px] truncate">
                        {session.git_branch || "--"}
                      </td>
                      <td className="px-4 py-2.5 text-right text-text-muted whitespace-nowrap">
                        {formatDate(session.updated_at ?? session.created_at)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
              {/* Pagination */}
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

        {/* Available Models */}
        {availableModels.length > 0 && (
          <Section title="Available Models">
            <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg p-4">
              <div className="flex flex-wrap gap-2">
                {availableModels.map((model) => (
                  <Badge
                    key={model}
                    color={model === configModel ? ACCENT : "#6366f1"}
                  >
                    {model}
                    {model === configModel && " (active)"}
                  </Badge>
                ))}
              </div>
            </div>
          </Section>
        )}

        {/* Extra info badges — filter out keys we already display */}
        {(() => {
          const displayedKeys = new Set([
            "plan_type", "unlimited_credits", "total_sessions", "total_tokens_used",
            "sessions", "tokens_by_model", "sessions_by_project", "available_models",
            "config_model", "config_reasoning_effort", "account_name", "organizations",
            "estimated_total_cost", "cost_by_model", "start_today_cost",
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
