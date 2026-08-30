/**
 * Gemini CLI Analytics Page
 * Shows connection status, rate limits, account/plan info, and session stats
 * powered by the unified provider analytics backend.
 */
import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

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

const ACCENT = "#4285f4";

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

function formatNum(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
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

function GeminiDashboardSkeleton() {
  return (
    <div className="px-6 py-6 animate-pulse">
      <div className="h-5 w-48 bg-[#1a1b23] rounded mb-2" />
      <div className="h-3 w-72 bg-[#1a1b23] rounded mb-6" />
      <div className="grid grid-cols-3 gap-4 mb-6">
        {[1, 2, 3].map((i) => (
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
      <h1 className="text-lg font-semibold text-text-primary mb-2">Gemini CLI Analytics</h1>
      <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg p-6 text-center max-w-md mx-auto mt-8">
        <div
          className="w-12 h-12 rounded-full mx-auto mb-4 flex items-center justify-center text-2xl"
          style={{ backgroundColor: ACCENT + "20" }}
        >
          <span style={{ color: ACCENT }}>&#10022;</span>
        </div>
        <h2 className="text-sm font-semibold text-text-primary mb-2">Gemini CLI Not Connected</h2>
        <p className="text-xs text-text-muted mb-4">
          Install Gemini CLI and sign in to see your analytics. AgentHarbor auto-detects
          credentials from{" "}
          <code className="font-mono text-[10px] bg-[#22232e] px-1 py-0.5 rounded">
            ~/.gemini/
          </code>.
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

// ── Main Component ──────────────────────────────────────────────────────────

export function GeminiAnalyticsPage() {
  const [analytics, setAnalytics] = useState<ProviderAnalytics | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [lastRefreshed, setLastRefreshed] = useState<string | null>(null);
  // Tick for "X ago" display refresh
  const [, setTick] = useState(0);
  useEffect(() => {
    const interval = setInterval(() => setTick((t) => t + 1), 30000);
    return () => clearInterval(interval);
  }, []);

  const loadData = useCallback(async (forceRefresh = false) => {
    setLoading(true);
    setLoadError(null);
    try {
      const cmd = forceRefresh ? "force_refresh_provider" : "get_provider_analytics";
      const data = await invoke<ProviderAnalytics>(cmd, {
        providerId: "gemini",
      });
      setAnalytics(data);
      setLastRefreshed(data.fetched_at);
    } catch (err) {
      const msg = String(err);
      console.error("[Gemini] Analytics load error:", msg);
      setLoadError(msg);
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
    return <GeminiDashboardSkeleton />;
  }

  if (loadError && !analytics) {
    return (
      <div className="p-6">
        <div className="bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-xs text-red-400">
          <p className="font-medium mb-1">Failed to load Gemini analytics</p>
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

  const { status, rate_limits, extra } = analytics;

  // Plan info
  const planTier = (extra.plan_tier as string) ?? status.plan_name ?? "Unknown";
  const projectId = (extra.project_id as string) ?? null;
  const upgradeUri = (extra.upgrade_uri as string) ?? null;

  // Session stats
  const totalSessions = (extra.gemini_total_sessions as number) ?? 0;
  const totalMessages = (extra.gemini_total_messages as number) ?? 0;
  const projectCount = (extra.gemini_project_count as number) ?? 0;
  const firstActivity = (extra.gemini_first_activity as string) ?? null;
  // lastActivity and installationId available in extra if needed

  // Telemetry stats (only available if user ran with --telemetry-outfile)
  const hasTelemetry = (extra.gemini_has_telemetry as boolean) ?? false;
  const apiRequests = (extra.gemini_api_requests as number) ?? 0;
  const apiErrors = (extra.gemini_api_errors as number) ?? 0;
  const avgLatencyMs = (extra.gemini_avg_latency_ms as number) ?? 0;
  const inputTokens = (extra.gemini_input_tokens as number) ?? 0;
  const outputTokens = (extra.gemini_output_tokens as number) ?? 0;
  const cachedTokens = (extra.gemini_cached_tokens as number) ?? 0;
  const thoughtTokens = (extra.gemini_thought_tokens as number) ?? 0;
  const toolCalls = (extra.gemini_tool_calls as number) ?? 0;
  const toolSuccess = (extra.gemini_tool_success as number) ?? 0;
  const modelBreakdown = (extra.gemini_model_breakdown as Record<string, number>) ?? null;

  // Auto-recorded chats-JSONL session stats (Gemini CLI 0.46.0+)
  const chatsSessions = (extra.gemini_chats_sessions as number) ?? 0;
  const startTodaySessions = (extra.start_today_sessions as number) ?? 0;
  const startTodayTokens = (extra.start_today_tokens as number) ?? 0;
  const thisWeekSessions = (extra.this_week_sessions as number) ?? 0;
  const thisWeekTokens = (extra.this_week_tokens as number) ?? 0;
  const chatsTokenTotals = (extra.gemini_token_totals as {
    input: number; output: number; cached: number; thoughts: number; tool: number;
  } | undefined) ?? null;
  const chatsModelsUsed = (extra.gemini_models_used as Record<string, number>) ?? null;
  const chatsTotalTokens = chatsTokenTotals
    ? chatsTokenTotals.input + chatsTokenTotals.output + chatsTokenTotals.cached + chatsTokenTotals.thoughts
    : 0;

  // Plan sunset (individual-tier OAuth no longer supported by loadCodeAssist)
  const planSunset = (extra.plan_sunset as boolean) ?? false;
  const planSunsetMessage = (extra.plan_sunset_message as string) ?? null;

  // Quota API unavailable (403 SUBSCRIPTION_REQUIRED — free/individual tier)
  const quotaUnavailableReason = (extra.quota_unavailable_reason as string) ?? null;

  // Connection method display
  const connectionLabel =
    status.connection_method === "oauth-auto"
      ? "Auto-detected"
      : status.connection_method === "local-file"
      ? "Local data"
      : status.connection_method === "adc"
      ? "Application Default Credentials"
      : "Connected";

  return (
    <div className="h-full overflow-y-auto relative">
      {/* Top loading bar */}
      {loading && (
        <div className="sticky top-0 z-50 w-full">
          <div className="h-0.5 bg-[#0e0f13] w-full overflow-hidden">
            <div
              className="h-full animate-[gemini-loading-bar_1.5s_ease-in-out_infinite] w-1/3 rounded-full"
              style={{ backgroundColor: ACCENT }}
            />
          </div>
          <style>{`@keyframes gemini-loading-bar { 0% { transform: translateX(-100%); } 100% { transform: translateX(400%); } }`}</style>
        </div>
      )}

      <div className="px-6 py-6">
        {/* Header */}
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-lg font-semibold text-text-primary flex items-center gap-2">
              Gemini CLI Analytics
              {loading && (
                <span
                  className="inline-block w-2 h-2 rounded-full animate-pulse"
                  style={{ backgroundColor: ACCENT }}
                  title="Refreshing..."
                />
              )}
            </h1>
            <p className="text-xs text-text-muted">
              {status.account_email ?? "Gemini CLI"}
              {planTier && (
                <span>
                  {" "}&middot;{" "}
                  <span className="capitalize">{planTier}</span>
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

        {/* Rate Limits */}
        {rate_limits.length > 0 ? (
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
        ) : quotaUnavailableReason === "subscription_required" ? (
          <Section title="Rate Limits">
            <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg p-4 text-xs text-text-muted">
              Quota API requires Gemini Code Assist Standard/Enterprise — not available on the free/individual tier.
            </div>
          </Section>
        ) : null}

        {/* Account & Plan */}
        <Section title="Account & Plan">
          <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
            <StatCard
              label="Email"
              value={status.account_email ?? "N/A"}
            />
            <StatCard
              label="Plan"
              value={planTier}
              sub={
                planTier === "Free"
                  ? "Free tier with rate limits"
                  : planTier === "Paid"
                  ? "Pay-as-you-go billing"
                  : undefined
              }
            />
            <StatCard
              label="Status"
              value={status.connected ? "Connected" : "Disconnected"}
              sub={connectionLabel}
            />
          </div>
          {planSunset && (
            <div className="mt-3 bg-amber-500/10 border border-amber-500/20 rounded-lg px-4 py-3">
              <p className="text-xs text-amber-300">
                {planSunsetMessage ??
                  "The individual Gemini Code Assist tier is being sunset for CLI clients. Migrate to Antigravity or a paid Code Assist tier to keep full access."}
              </p>
            </div>
          )}
          {(projectId || upgradeUri) && (
            <div className="mt-3 flex items-center gap-4">
              {projectId && (
                <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg px-4 py-3 flex-1">
                  <p className="text-[10px] text-text-muted uppercase tracking-wider mb-1">GCP Project ID</p>
                  <p className="text-sm text-text-primary font-mono truncate">{projectId}</p>
                </div>
              )}
              {upgradeUri && (
                <button
                  onClick={async () => {
                    try {
                      await openUrl(upgradeUri);
                    } catch {
                      window.open(upgradeUri, "_blank");
                    }
                  }}
                  className="px-4 py-2.5 rounded-lg text-xs font-medium transition-colors flex items-center gap-2 shrink-0"
                  style={{ backgroundColor: ACCENT + "20", color: ACCENT }}
                >
                  Upgrade Plan
                  <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                    <path strokeLinecap="round" strokeLinejoin="round" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                  </svg>
                </button>
              )}
            </div>
          )}
        </Section>

        {/* Session Stats */}
        {(totalSessions > 0 || totalMessages > 0) && (
          <Section title="Session Stats">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <StatCard label="Sessions" value={totalSessions.toLocaleString()} sub="All time" />
              <StatCard label="Messages" value={totalMessages.toLocaleString()} sub="User prompts" />
              <StatCard label="Projects" value={projectCount.toLocaleString()} sub="Directories used" />
              {firstActivity && (
                <StatCard label="First Activity" value={new Date(firstActivity).toLocaleDateString()} sub={firstActivity} />
              )}
            </div>
          </Section>
        )}

        {/* Telemetry Stats — only shown if user has telemetry file */}
        {hasTelemetry && apiRequests > 0 && (
          <Section title="Model Stats (from telemetry)">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <StatCard label="API Requests" value={apiRequests.toLocaleString()} sub={apiErrors > 0 ? `${apiErrors} errors (${((apiErrors / apiRequests) * 100).toFixed(1)}%)` : "0 errors"} />
              <StatCard label="Avg Latency" value={avgLatencyMs > 1000 ? `${(avgLatencyMs / 1000).toFixed(1)}s` : `${avgLatencyMs.toFixed(0)}ms`} />
              <StatCard label="Tool Calls" value={toolCalls.toLocaleString()} sub={toolCalls > 0 ? `${toolSuccess} success (${((toolSuccess / toolCalls) * 100).toFixed(0)}%)` : undefined} />
              <StatCard label="Total Tokens" value={formatNum(inputTokens + outputTokens + cachedTokens + thoughtTokens)} />
            </div>
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mt-3">
              <StatCard label="Input (Prompt)" value={formatNum(inputTokens)} />
              <StatCard label="Output" value={formatNum(outputTokens)} />
              <StatCard label="Cached" value={formatNum(cachedTokens)} sub={inputTokens > 0 ? `${((cachedTokens / inputTokens) * 100).toFixed(1)}% cache hit` : undefined} />
              <StatCard label="Thoughts" value={formatNum(thoughtTokens)} />
            </div>
            {modelBreakdown && Object.keys(modelBreakdown).length > 0 && (
              <div className="mt-3 bg-[#1a1b23] border border-[#2a2b36] rounded-lg p-4">
                <p className="text-[10px] text-text-muted uppercase tracking-wider mb-2">Model Usage</p>
                {Object.entries(modelBreakdown).sort((a, b) => b[1] - a[1]).map(([model, count]) => (
                  <div key={model} className="flex items-center justify-between text-xs py-1">
                    <span className="text-text-primary font-mono">{model}</span>
                    <span className="text-text-muted">{count} requests</span>
                  </div>
                ))}
              </div>
            )}
          </Section>
        )}

        {/* Token Usage — from auto-recorded chats JSONL sessions (Gemini CLI 0.46.0+) */}
        {chatsSessions > 0 && (
          <Section title="Token Usage (from sessions)">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
              <StatCard label="Sessions" value={chatsSessions.toLocaleString()} sub="All time" />
              <StatCard
                label="Today"
                value={startTodaySessions.toLocaleString()}
                sub={`${formatNum(startTodayTokens)} tokens`}
              />
              <StatCard
                label="This Week"
                value={thisWeekSessions.toLocaleString()}
                sub={`${formatNum(thisWeekTokens)} tokens`}
              />
              <StatCard label="Total Tokens" value={formatNum(chatsTotalTokens)} />
            </div>
            {chatsTokenTotals && (
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4 mt-3">
                <StatCard label="Input (Prompt)" value={formatNum(chatsTokenTotals.input)} />
                <StatCard label="Output" value={formatNum(chatsTokenTotals.output)} />
                <StatCard
                  label="Cached"
                  value={formatNum(chatsTokenTotals.cached)}
                  sub={chatsTokenTotals.input > 0 ? `${((chatsTokenTotals.cached / chatsTokenTotals.input) * 100).toFixed(1)}% cache hit` : undefined}
                />
                <StatCard label="Thoughts" value={formatNum(chatsTokenTotals.thoughts)} />
              </div>
            )}
            {chatsModelsUsed && Object.keys(chatsModelsUsed).length > 0 && (
              <div className="mt-3 bg-[#1a1b23] border border-[#2a2b36] rounded-lg p-4">
                <p className="text-[10px] text-text-muted uppercase tracking-wider mb-2">Model Usage</p>
                {Object.entries(chatsModelsUsed).sort((a, b) => b[1] - a[1]).map(([model, count]) => (
                  <div key={model} className="flex items-center justify-between text-xs py-1">
                    <span className="text-text-primary font-mono">{model}</span>
                    <span className="text-text-muted">{count} messages</span>
                  </div>
                ))}
              </div>
            )}
            <p className="text-[10px] text-text-muted mt-2">
              Sourced from ~/.gemini/tmp/*/chats — Gemini CLI retains these session logs for the last 30 days.
            </p>
          </Section>
        )}

        {/* Tips for getting more stats — only when neither chats data nor telemetry is available */}
        {!hasTelemetry && chatsSessions === 0 && (
          <Section title="Get Detailed Stats">
            <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg p-4">
              <p className="text-xs text-text-secondary mb-3">
                Newer Gemini CLI versions (0.46.0+) auto-record session token and model stats — nothing to
                configure, they just haven&apos;t shown up here yet. If you&apos;re on an older CLI version,
                enable telemetry output manually instead:
              </p>
              <div className="bg-[#13141a] rounded-md p-3 mb-3 font-mono text-[11px] text-green-400">
                gemini --telemetry --telemetry-outfile ~/.gemini/telemetry.jsonl
              </div>
              <p className="text-[10px] text-text-muted mb-2">
                This writes OpenTelemetry events (API requests, token counts, latency, tool calls) to a local file.
                AgentHarbor will automatically parse it and show detailed model stats above.
              </p>
              <p className="text-[10px] text-text-muted">
                <span className="text-text-secondary font-medium">Tip:</span> Add an alias to your shell config for persistent tracking:
              </p>
              <div className="bg-[#13141a] rounded-md p-2 mt-1 font-mono text-[10px] text-text-muted">
                alias gemini='gemini --telemetry --telemetry-outfile ~/.gemini/telemetry.jsonl'
              </div>
            </div>
          </Section>
        )}

        {/* Credits — if available */}
        {analytics.credit_usage && (
          <Section title="Credits">
            <div className="grid grid-cols-3 gap-4">
              <StatCard
                label="Balance"
                value={`${analytics.credit_usage.remaining.toFixed(2)} ${analytics.credit_usage.currency}`}
              />
              <StatCard
                label="Used"
                value={`${analytics.credit_usage.used.toFixed(2)} ${analytics.credit_usage.currency}`}
              />
              <StatCard
                label="Plan"
                value={analytics.credit_usage.plan_name ?? planTier}
                sub={
                  analytics.credit_usage.billing_cycle_end
                    ? `Resets ${formatResetTime(analytics.credit_usage.billing_cycle_end)}`
                    : undefined
                }
              />
            </div>
          </Section>
        )}

        {/* Extra info badges — filter out keys we already display */}
        {(() => {
          const displayedKeys = new Set([
            "plan_tier", "project_id", "upgrade_uri",
            "gemini_total_sessions", "gemini_total_messages", "gemini_project_count",
            "gemini_first_activity", "gemini_last_activity", "installation_id",
            "gemini_has_telemetry", "gemini_api_requests", "gemini_api_errors",
            "gemini_avg_latency_ms", "gemini_input_tokens", "gemini_output_tokens",
            "gemini_cached_tokens", "gemini_thought_tokens", "gemini_tool_calls",
            "gemini_tool_success", "gemini_model_breakdown",
            "gemini_chats_sessions", "gemini_token_totals", "gemini_models_used",
            "start_today_sessions", "start_today_tokens", "start_today_messages",
            "this_week_sessions", "this_week_tokens", "this_week_messages",
            "plan_sunset", "plan_sunset_message", "codeassist_error",
            "quota_unavailable_reason", "quota_absolute",
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
