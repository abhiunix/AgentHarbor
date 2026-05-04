import { getAdapterIconImg } from "../../lib/adapterPlugins";
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { openUrl } from "@tauri-apps/plugin-opener";
import { LimitStateBanner, type LimitState } from "../analytics/LimitStateBanner";

// ── Types ────────────────────────────────────────────────────────────────────

export interface RateLimitWindow {
  provider_id: string;
  label: string;
  used_percent: number;
  remaining_percent: number;
  resets_at: string | null;
  resets_in_seconds: number | null;
  window_seconds: number | null;
}

export interface CreditUsage {
  provider_id: string;
  used: number;
  limit: number | null;
  remaining: number;
  currency: string;
  billing_cycle_end: string | null;
  plan_name: string | null;
}

export interface TrayProviderSummary {
  provider_id: string;
  provider_name: string;
  connected: boolean;
  connection_method: string;
  email: string | null;
  plan: string | null;
  rate_limits: RateLimitWindow[];
  credit_usage: CreditUsage | null;
  error: string | null;
  extra: Record<string, unknown>;
  fetched_at: string;
  limit_state?: LimitState | null;
}

// ── Helpers ──────────────────────────────────────────────────────────────────

const PROVIDER_ROUTES: Record<string, string> = {
  "claude-code": "/adapters/claude-code/analytics-v2",
  cursor: "/adapters/cursor/analytics-v2",
  codex: "/adapters/codex/analytics",
  gemini: "/adapters/gemini/analytics",
};

function barColor(percent: number): string {
  if (percent > 30) return "bg-green-500";
  if (percent > 10) return "bg-yellow-500";
  return "bg-red-500";
}

function isSessionWindow(label: string): boolean {
  const lower = label.toLowerCase();
  return lower.includes("session") || lower.includes("5h");
}

function resolveResetDate(rl: RateLimitWindow): Date | null {
  if (rl.resets_at) {
    const d = new Date(rl.resets_at);
    if (!isNaN(d.getTime())) return d;
  }

  if (rl.resets_in_seconds != null && rl.resets_in_seconds > 0) {
    return new Date(Date.now() + rl.resets_in_seconds * 1000);
  }

  if (rl.window_seconds != null && rl.window_seconds > 0) {
    return new Date(Date.now() + rl.window_seconds * 1000);
  }

  return null;
}

function formatAbsoluteResetDate(d: Date): string {
  const datePart = d.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
  });
  const timePart = d
    .toLocaleTimeString("en-US", {
      hour: "numeric",
      minute: "2-digit",
      hour12: true,
    })
    .toLowerCase();
  return `${datePart} at ${timePart}`;
}

function formatResetLabel(rl: RateLimitWindow): string {
  const resetDate = resolveResetDate(rl);

  if (resetDate) {
    if (isSessionWindow(rl.label)) {
      const diffMs = resetDate.getTime() - Date.now();
      if (diffMs > 0 && diffMs < 24 * 3600 * 1000) {
        const h = Math.floor(diffMs / 3600000);
        const m = Math.floor((diffMs % 3600000) / 60000);
        return h > 0 ? `Resets in ${h}h ${m}m` : `Resets in ${m}m`;
      }
    }

    return `Resets ${formatAbsoluteResetDate(resetDate)}`;
  }

  // Final fallback to duration format when date resolution isn't possible.
  if (rl.resets_in_seconds != null && rl.resets_in_seconds > 0) {
    const s = rl.resets_in_seconds;
    if (s < 3600) return `Resets in ${Math.round(s / 60)}m`;
    const h = Math.floor(s / 3600);
    const m = Math.round((s % 3600) / 60);
    return `Resets in ${h}h ${m}m`;
  }
  return "";
}

function formatCurrency(amount: number, currency: string): string {
  return `${currency === "USD" ? "$" : currency}${amount.toFixed(2)}`;
}

function formatTimeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const sec = Math.floor(diff / 1000);
  if (sec < 60) return "just now";
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min}m ago`;
  const hrs = Math.floor(min / 60);
  if (hrs < 24) return `${hrs}h ago`;
  return `${Math.floor(hrs / 24)}d ago`;
}

// ── Today Stats Row ─────────────────────────────────────────────────────────

function formatCost(n: number): string {
  if (n >= 1) return `$${n.toFixed(2)}`;
  if (n >= 0.01) return `$${n.toFixed(2)}`;
  if (n > 0) return `$${n.toFixed(4)}`;
  return "$0.00";
}

function TodayStats({ extra, providerId }: { extra: Record<string, unknown>; providerId: string }) {
  const todaySessions = extra.start_today_sessions as number | undefined;
  const todayTokens = extra.start_today_tokens as number | undefined;
  const todayMessages = extra.start_today_messages as number | undefined;
  const todayCost = extra.start_today_cost as number | undefined;
  const todayCommits = extra.start_today_commits as number | undefined;
  const todayAiLines = extra.start_today_ai_lines as number | undefined;

  const parts: string[] = [];

  if (providerId === "claude-code") {
    // Claude: calendar-day sessions, messages, cost
    if (todaySessions != null && todaySessions > 0)
      parts.push(`${todaySessions} session${todaySessions !== 1 ? "s" : ""}`);
    if (todayMessages != null && todayMessages > 0)
      parts.push(`${todayMessages} msg${todayMessages !== 1 ? "s" : ""}`);
    if (todayCost != null && todayCost > 0)
      parts.push(formatCost(todayCost));
  } else if (providerId === "cursor") {
    // Cursor: today commits, AI lines
    if (todayCommits != null && todayCommits > 0)
      parts.push(`${todayCommits} commit${todayCommits !== 1 ? "s" : ""}`);
    if (todayAiLines != null && todayAiLines > 0)
      parts.push(`${todayAiLines} AI lines`);
  } else if (providerId === "codex") {
    // Codex: today sessions, tokens, cost
    if (todaySessions != null && todaySessions > 0)
      parts.push(`${todaySessions} session${todaySessions !== 1 ? "s" : ""}`);
    if (todayTokens != null && todayTokens > 0) {
      const formatted = todayTokens >= 1_000_000
        ? `${(todayTokens / 1_000_000).toFixed(1)}M`
        : todayTokens >= 1_000
        ? `${(todayTokens / 1_000).toFixed(1)}K`
        : `${todayTokens}`;
      parts.push(`${formatted} tokens`);
    }
    if (todayCost != null && todayCost > 0)
      parts.push(formatCost(todayCost));
  } else {
    // Generic fallback
    if (todaySessions != null && todaySessions > 0)
      parts.push(`${todaySessions} session${todaySessions !== 1 ? "s" : ""}`);
    if (todayMessages != null && todayMessages > 0)
      parts.push(`${todayMessages} msg${todayMessages !== 1 ? "s" : ""}`);
    if (todayTokens != null && todayTokens > 0) {
      const formatted = todayTokens >= 1_000_000
        ? `${(todayTokens / 1_000_000).toFixed(1)}M`
        : todayTokens >= 1_000
        ? `${(todayTokens / 1_000).toFixed(1)}K`
        : `${todayTokens}`;
      parts.push(`${formatted} tokens`);
    }
  }

  if (parts.length === 0) return null;

  return (
    <div className="mt-2 pt-2 border-t border-[#2a2b36]">
      <div className="flex items-center justify-between text-[11px]">
        <span className="text-[#9394a1]">Today</span>
        <span className="text-[#e8e9ed]">{parts.join(" · ")}</span>
      </div>
    </div>
  );
}

// ── Rate Limit Bar ───────────────────────────────────────────────────────────

function RateLimitBar({ rl }: { rl: RateLimitWindow }) {
  const used = Math.max(0, Math.min(100, rl.used_percent));
  const remaining = Math.max(0, 100 - used);
  const resetLabel = formatResetLabel(rl);

  return (
    <div className="mb-2 last:mb-0">
      <div className="flex items-center justify-between text-[11px] mb-0.5">
        <span className="text-[#e8e9ed]">
          {rl.label}
          {resetLabel && (
            <span className="text-[9px] text-[#9394a1] ml-2">{resetLabel}</span>
          )}
        </span>
      </div>
      <div className="flex items-center gap-2">
        <div className="flex-1 h-1.5 rounded-full bg-[#0e0f13] overflow-hidden flex">
          <div
            className="h-full rounded-l-full transition-all duration-300 bg-red-500"
            style={{ width: `${used}%` }}
          />
          <div
            className="h-full rounded-r-full transition-all duration-300 bg-emerald-500"
            style={{ width: `${remaining}%` }}
          />
        </div>
        <span className="text-red-400 font-mono font-semibold text-[11px] whitespace-nowrap">
          {used.toFixed(0)}% Used
        </span>
      </div>
    </div>
  );
}

// ── Disconnected Card ────────────────────────────────────────────────────────

function DisconnectedCard({
  provider,
}: {
  provider: TrayProviderSummary;
}) {
  const iconImg = getAdapterIconImg(provider.provider_id);

  const handleConnect = async () => {
    const route = PROVIDER_ROUTES[provider.provider_id];
    if (route) {
      await emitTo("main", "navigate-to", route);
    }
    try {
      await getCurrentWindow().hide();
    } catch { /* ignore */ }
  };

  return (
    <div className="rounded-lg border border-[#2a2b36] bg-[#1a1b23]/60 p-3">
      <div className="flex items-center gap-2 mb-2">
        {iconImg ? (
          <img src={iconImg} alt="" className="w-5 h-5 rounded opacity-40" />
        ) : (
          <div className="w-5 h-5 rounded bg-[#2a2b36]" />
        )}
        <span className="text-[#9394a1] text-sm font-medium">
          {provider.provider_name}
        </span>
      </div>
      <p className="text-[#9394a1] text-xs mb-2">Not connected</p>
      <button
        onClick={handleConnect}
        className="text-xs px-3 py-1 rounded bg-[#2a2b36] text-[#e8e9ed] hover:bg-[#333440] transition-colors"
      >
        Connect
      </button>
    </div>
  );
}

// ── Claude Card ──────────────────────────────────────────────────────────────

function formatTokenCount(n: number): string {
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return `${n}`;
}

// Helper to extract a time-window's stats from extra using a prefix
function getWindowStats(ex: Record<string, unknown>, prefix: string) {
  return {
    sessions: ex[`${prefix}_sessions`] as number | undefined,
    messages: ex[`${prefix}_messages`] as number | undefined,
    tokens: ex[`${prefix}_tokens`] as number | undefined,
    cost: ex[`${prefix}_cost`] as number | undefined,
    inputTokens: ex[`${prefix}_input_tokens`] as number | undefined,
    outputTokens: ex[`${prefix}_output_tokens`] as number | undefined,
    cacheRead: ex[`${prefix}_cache_read`] as number | undefined,
    cacheWrite: ex[`${prefix}_cache_write`] as number | undefined,
    inputCost: ex[`${prefix}_input_cost`] as number | undefined,
    outputCost: ex[`${prefix}_output_cost`] as number | undefined,
    cacheReadCost: ex[`${prefix}_cache_read_cost`] as number | undefined,
    cacheWriteCost: ex[`${prefix}_cache_write_cost`] as number | undefined,
  };
}

function TokenWindowRow({ label, stats }: {
  label: string;
  stats: ReturnType<typeof getWindowStats>;
}) {
  const hasData = (stats.tokens ?? 0) > 0 || (stats.sessions ?? 0) > 0;
  if (!hasData) return null;

  return (
    <div className="bg-[#0e0f13] rounded px-2.5 py-2 mb-1.5 last:mb-0">
      <div className="flex items-center justify-between mb-1">
        <span className="text-[10px] text-[#9394a1] font-medium uppercase tracking-wider">{label}</span>
        <span className="text-emerald-400 font-mono text-[11px] font-semibold">
          {formatCost(stats.cost ?? 0)}
        </span>
      </div>
      <div className="grid grid-cols-4 gap-1 text-[9px] mb-1">
        <div>
          <div className="text-[#9394a1]">Input</div>
          <div className="text-[#e8e9ed] font-mono">{formatCost(stats.inputCost ?? 0)}</div>
          <div className="text-[#9394a1] font-mono">{formatTokenCount(stats.inputTokens ?? 0)}</div>
        </div>
        <div>
          <div className="text-[#9394a1]">Output</div>
          <div className="text-[#e8e9ed] font-mono">{formatCost(stats.outputCost ?? 0)}</div>
          <div className="text-[#9394a1] font-mono">{formatTokenCount(stats.outputTokens ?? 0)}</div>
        </div>
        <div>
          <div className="text-[#9394a1]">Cache R</div>
          <div className="text-emerald-400 font-mono">{formatCost(stats.cacheReadCost ?? 0)}</div>
          <div className="text-[#9394a1] font-mono">{formatTokenCount(stats.cacheRead ?? 0)}</div>
        </div>
        <div>
          <div className="text-[#9394a1]">Cache W</div>
          <div className="text-amber-400 font-mono">{formatCost(stats.cacheWriteCost ?? 0)}</div>
          <div className="text-[#9394a1] font-mono">{formatTokenCount(stats.cacheWrite ?? 0)}</div>
        </div>
      </div>
      <div className="flex gap-3 text-[9px] text-[#9394a1]">
        <span>{stats.sessions ?? 0} sessions</span>
        <span>{stats.messages ?? 0} msgs</span>
        <span>{formatTokenCount(stats.tokens ?? 0)} total</span>
      </div>
    </div>
  );
}

function EnterpriseSpendBar({
  used,
  limit,
  currency,
}: {
  used: number;
  limit: number | null;
  currency: string;
}) {
  const cur = currency || "USD";
  const hasCap = limit != null && limit > 0;
  const pct = hasCap ? Math.min(100, Math.max(0, (used / (limit as number)) * 100)) : 0;
  // Render small percentages with one decimal so they don't all collapse to "0%"
  // (e.g. $10 of $3000 = 0.33%, not 0%).
  const pctLabel = pct >= 10 ? `${pct.toFixed(0)}%` : `${pct.toFixed(1)}%`;

  return (
    <div className="mb-2">
      <div className="flex items-center justify-between text-[11px] mb-0.5">
        <span className="text-[#e8e9ed]">
          Monthly spend
          <span className="text-[9px] text-[#9394a1] ml-2">Enterprise · usage-based</span>
        </span>
        <span className="text-[#e8e9ed] font-mono">
          {formatCurrency(used, cur)}
          {hasCap ? (
            <>
              <span className="text-[#9394a1]"> / {formatCurrency(limit as number, cur)}</span>
              <span className="text-[#9394a1] ml-1.5">· {pctLabel}</span>
            </>
          ) : (
            <span className="text-[#9394a1]"> · no cap</span>
          )}
        </span>
      </div>
      {hasCap ? (
        <div className="h-1.5 rounded-full bg-[#0e0f13] overflow-hidden">
          <div
            className={`h-full rounded-full transition-all duration-300 ${barColor(100 - pct)}`}
            style={{ width: `${pct}%` }}
          />
        </div>
      ) : (
        <div className="h-1.5 rounded-full bg-purple-500/40 overflow-hidden">
          <div className="h-full rounded-full bg-purple-500/80 w-full" />
        </div>
      )}
    </div>
  );
}

function ClaudeCard({ provider }: { provider: TrayProviderSummary }) {
  const ex = provider.extra;
  const statsToday = getWindowStats(ex, "start_today");
  const statsWeek = getWindowStats(ex, "this_week");
  const statsAll = getWindowStats(ex, "all_time");

  const hasAnyData = (statsToday.tokens ?? 0) > 0 || (statsWeek.tokens ?? 0) > 0 || (statsAll.tokens ?? 0) > 0;
  const isEnterprise = (ex.is_enterprise as boolean | undefined) === true;
  const cu = provider.credit_usage;

  return (
    <div>
      {/* Enterprise plans have no 5h/7d windows — show $-spend instead. */}
      {isEnterprise && cu && (
        <EnterpriseSpendBar
          used={cu.used}
          limit={cu.limit}
          currency={cu.currency}
        />
      )}

      {/* Pro/Max session + weekly bars */}
      {!isEnterprise && provider.rate_limits.map((rl, i) => (
        <RateLimitBar key={i} rl={rl} />
      ))}

      {/* Pro/Max overage meter (only when extra_usage explicitly enabled) */}
      {!isEnterprise && cu && (
        <div className="mt-2 pt-2 border-t border-[#2a2b36]">
          <div className="flex items-center justify-between text-[11px]">
            <span className="text-[#9394a1]">Credits</span>
            <span className="text-[#e8e9ed] font-mono">
              {formatCurrency(cu.used, cu.currency)}
              {cu.limit != null && (
                <span className="text-[#9394a1]">
                  {" "}/ {formatCurrency(cu.limit, cu.currency)}
                </span>
              )}
            </span>
          </div>
        </div>
      )}

      {/* Row-based usage breakdown: Today (IST), This Week, All Time */}
      {hasAnyData && (
        <div className="mt-2 pt-2 border-t border-[#2a2b36]">
          <TokenWindowRow label="Today" stats={statsToday} />
          <TokenWindowRow label="This Week" stats={statsWeek} />
          <TokenWindowRow label="All Time" stats={statsAll} />
        </div>
      )}

      {!hasAnyData && <TodayStats extra={provider.extra} providerId={provider.provider_id} />}
    </div>
  );
}

// ── Cursor Card ──────────────────────────────────────────────────────────────

function CursorCard({ provider }: { provider: TrayProviderSummary }) {
  const cu = provider.credit_usage;
  const ex = provider.extra;
  const onDemand = (ex.on_demand_used_usd as number) ?? 0;
  const included = ex.plan_included_usd as number | undefined;
  const bonus = ex.plan_bonus_usd as number | undefined;
  const totalBudget = ex.plan_total_budget_usd as number | undefined;
  const billingStart = ex.billing_cycle_start as string | undefined;
  const teamOdEnabled = ex.team_od_enabled as boolean | undefined;
  const teamOdUsed = ex.team_od_used_usd as number | undefined;
  const teamOdLimit = ex.team_od_limit_usd as number | undefined;
  const teamHardLimit = ex.team_hard_limit_usd as number | undefined;
  const teamHardLimitDynamic = ex.team_hard_limit_dynamic as boolean | undefined;

  const includedVal = included ?? 0;
  const bonusVal = bonus ?? 0;
  const onDemandVal = onDemand ?? 0;
  const totalSpent = includedVal + bonusVal + onDemandVal;

  return (
    <div>
      {cu && (
        <>
          {/* Usage breakdown bar */}
          <div className="mb-1.5">
            <div className="flex items-center justify-between text-[11px] mb-0.5">
              <span className="text-[#9394a1]">Usage Breakdown</span>
              <span className="text-[#e8e9ed] font-mono">
                {formatCurrency(totalSpent > 0 ? totalSpent : (totalBudget ?? cu.used), cu.currency)} <span className="text-[#9394a1]">total spent</span>
              </span>
            </div>
            {totalSpent > 0 && (
              <div className="h-1.5 rounded-full bg-[#2a2b36] overflow-hidden flex">
                {includedVal > 0 && (
                  <div
                    className="h-full bg-blue-500 transition-all duration-300"
                    style={{ width: `${(includedVal / totalSpent) * 100}%` }}
                  />
                )}
                {bonusVal > 0 && (
                  <div
                    className="h-full bg-emerald-500 transition-all duration-300"
                    style={{ width: `${(bonusVal / totalSpent) * 100}%` }}
                  />
                )}
                {onDemandVal > 0 && (
                  <div
                    className="h-full bg-amber-500 transition-all duration-300"
                    style={{ width: `${(onDemandVal / totalSpent) * 100}%` }}
                  />
                )}
              </div>
            )}
            {(included != null || bonus != null || onDemandVal > 0) && (
              <div className="flex flex-wrap gap-x-3 gap-y-0.5 mt-0.5 text-[9px]">
                {included != null && (
                  <span className="flex items-center gap-1">
                    <span className="inline-block w-1.5 h-1.5 rounded-full bg-blue-500" />
                    <span className="text-[#9394a1]">{formatCurrency(includedVal, "USD")} included</span>
                  </span>
                )}
                {bonus != null && bonusVal > 0 && (
                  <span className="flex items-center gap-1">
                    <span className="inline-block w-1.5 h-1.5 rounded-full bg-emerald-500" />
                    <span className="text-[#9394a1]">{formatCurrency(bonusVal, "USD")} bonus</span>
                  </span>
                )}
                {onDemandVal > 0 && (
                  <span className="flex items-center gap-1">
                    <span className="inline-block w-1.5 h-1.5 rounded-full bg-amber-500" />
                    <span className="text-[#9394a1]">{formatCurrency(onDemandVal, "USD")} on-demand</span>
                  </span>
                )}
              </div>
            )}
            {(billingStart || cu.billing_cycle_end) && (
              <div className="flex items-center justify-between mt-0.5">
                <span className="text-[9px] text-[#9394a1]">
                  Billing cycle: {billingStart ? new Date(billingStart).toLocaleDateString() : "?"} – {cu.billing_cycle_end ? new Date(cu.billing_cycle_end).toLocaleDateString() : "?"}
                </span>
                <button
                  onClick={() => openUrl("https://cursor.com/dashboard/usage")}
                  className="text-[9px] text-blue-400 hover:text-blue-300 hover:underline"
                  title="Open cursor.com/dashboard/usage"
                >
                  View on Cursor ↗
                </button>
              </div>
            )}
            {!billingStart && !cu.billing_cycle_end && (
              <div className="flex justify-end mt-0.5">
                <button
                  onClick={() => openUrl("https://cursor.com/dashboard/usage")}
                  className="text-[9px] text-blue-400 hover:text-blue-300 hover:underline"
                  title="Open cursor.com/dashboard/usage"
                >
                  View on Cursor ↗
                </button>
              </div>
            )}
          </div>

          {/* Stat cards row */}
          <div className="grid grid-cols-3 gap-1 mb-1.5">
            {included != null && (
              <div className="bg-[#0e0f13] rounded p-1.5">
                <div className="text-[8px] text-[#9394a1] uppercase">Included</div>
                <div className="text-[11px] text-blue-400 font-mono">{formatCurrency(included, "USD")}</div>
                <div className="text-[8px] text-[#9394a1]">Plan allowance</div>
              </div>
            )}
            {bonus != null && (
              <div className="bg-[#0e0f13] rounded p-1.5">
                <div className="text-[8px] text-[#9394a1] uppercase">Bonus</div>
                <div className="text-[11px] text-emerald-400 font-mono">{formatCurrency(bonus, "USD")}</div>
                <div className="text-[8px] text-[#9394a1]">Extra credits</div>
              </div>
            )}
            {totalBudget != null && (
              <div className="bg-[#0e0f13] rounded p-1.5">
                <div className="text-[8px] text-[#9394a1] uppercase">Your Usage</div>
                <div className="text-[11px] text-emerald-400 font-mono">{formatCurrency(totalBudget, "USD")}</div>
                <div className="text-[8px] text-[#9394a1]">Included + Bonus</div>
              </div>
            )}
          </div>

          {/* Individual on-demand */}
          {onDemand > 0 && (
            <div className="flex items-center justify-between text-[11px] text-[#9394a1] mb-1">
              <span>Individual On-demand</span>
              <span className="text-[#e8e9ed] font-mono">{formatCurrency(onDemand, cu.currency)}</span>
            </div>
          )}
        </>
      )}

      {/* Team On-Demand section */}
      {teamOdEnabled && (
        <div className="mt-2 pt-2 border-t border-[#2a2b36]">
          <div className="flex items-center justify-between text-[11px] mb-1">
            <span className="text-[#9394a1] flex items-center gap-1.5">
              Team On-Demand
              <span className="text-[8px] px-1 py-0.5 rounded bg-emerald-500/20 text-emerald-400 uppercase">Enabled</span>
            </span>
            <span className="text-[#e8e9ed] font-mono">
              {formatCurrency(teamOdUsed ?? 0, "USD")}
              {teamOdLimit != null && (
                <>
                  <span className="text-[#9394a1]"> / {formatCurrency(teamOdLimit, "USD")}</span>
                  {teamOdLimit > 0 && (
                    <span className="text-[#9394a1] ml-1.5">
                      ·{" "}
                      {(() => {
                        const p = ((teamOdUsed ?? 0) / teamOdLimit) * 100;
                        return p >= 10 ? `${p.toFixed(0)}%` : `${p.toFixed(1)}%`;
                      })()}
                    </span>
                  )}
                </>
              )}
            </span>
          </div>
          {teamOdLimit != null && teamOdLimit > 0 && (
            <div className="h-1.5 rounded-full bg-[#2a2b36] overflow-hidden mb-0.5">
              <div
                className={`h-full rounded-full transition-all duration-300 ${barColor(((teamOdLimit - (teamOdUsed ?? 0)) / teamOdLimit) * 100)}`}
                style={{ width: `${Math.min(100, ((teamOdUsed ?? 0) / teamOdLimit) * 100)}%` }}
              />
            </div>
          )}
          {teamHardLimit != null && (
            <div className="text-[9px] text-[#9394a1]">
              Hard limit: {formatCurrency(teamHardLimit, "USD")}
              {teamHardLimitDynamic && <span className="ml-1 text-[8px] px-1 py-0.5 rounded bg-yellow-500/20 text-yellow-400 uppercase">Dynamic</span>}
            </div>
          )}
        </div>
      )}

      {/* AI Code Attribution */}
      {(() => {
        const totalCommits = ex.ai_total_commits as number | undefined;
        const aiLines = ex.ai_total_ai_lines as number | undefined;
        const humanLines = ex.ai_total_human_lines as number | undefined;
        if (!totalCommits && !aiLines) return null;
        const total = (aiLines ?? 0) + (humanLines ?? 0);
        const aiPct = total > 0 ? ((aiLines ?? 0) / total) * 100 : 0;
        return (
          <div className="mt-2 pt-2 border-t border-[#2a2b36]">
            <div className="text-[10px] text-[#9394a1] uppercase tracking-wider mb-1.5">AI Code Attribution</div>
            <div className="grid grid-cols-4 gap-1">
              <div className="bg-[#0e0f13] rounded p-1.5">
                <div className="text-[8px] text-[#9394a1] uppercase">Commits</div>
                <div className="text-[11px] text-[#e8e9ed] font-mono">{(totalCommits ?? 0).toLocaleString()}</div>
              </div>
              <div className="bg-[#0e0f13] rounded p-1.5">
                <div className="text-[8px] text-[#9394a1] uppercase">AI %</div>
                <div className="text-[11px] text-purple-400 font-mono">{aiPct.toFixed(1)}%</div>
              </div>
              <div className="bg-[#0e0f13] rounded p-1.5">
                <div className="text-[8px] text-[#9394a1] uppercase">AI Lines</div>
                <div className="text-[11px] text-purple-400 font-mono">{formatTokenCount(aiLines ?? 0)}</div>
              </div>
              <div className="bg-[#0e0f13] rounded p-1.5">
                <div className="text-[8px] text-[#9394a1] uppercase">Human</div>
                <div className="text-[11px] text-[#e8e9ed] font-mono">{formatTokenCount(humanLines ?? 0)}</div>
              </div>
            </div>
          </div>
        );
      })()}

      {provider.rate_limits.length > 0 && (
        <div className="mt-2 pt-2 border-t border-[#2a2b36]">
          {provider.rate_limits.slice(0, 3).map((rl, i) => (
            <RateLimitBar key={i} rl={rl} />
          ))}
        </div>
      )}
    </div>
  );
}

// ── Codex Card ───────────────────────────────────────────────────────────────

function CodexCard({ provider }: { provider: TrayProviderSummary }) {
  const unlimitedCredits = provider.extra.unlimited_credits as boolean | undefined;
  const ex = provider.extra;
  const totalSessions = ex.total_sessions as number | undefined;
  const totalTokens = ex.total_tokens_used as number | undefined;
  const totalCost = ex.estimated_total_cost as number | undefined;
  const costByModel = ex.cost_by_model as Record<string, number> | undefined;
  const tokensByModel = ex.tokens_by_model as Record<string, number> | undefined;
  const projectCount = ex.sessions_by_project ? Object.keys(ex.sessions_by_project as Record<string, unknown>).length : 0;
  const modelCount = tokensByModel ? Object.keys(tokensByModel).length : 0;

  const todayStats = {
    sessions: ex.start_today_sessions as number | undefined,
    tokens: ex.start_today_tokens as number | undefined,
    cost: ex.start_today_cost as number | undefined,
  };
  const weekStats = {
    sessions: ex.this_week_sessions as number | undefined,
    tokens: ex.this_week_tokens as number | undefined,
    cost: ex.this_week_cost as number | undefined,
  };

  return (
    <div>
      {/* Rate limits */}
      {provider.rate_limits.slice(0, 3).map((rl, i) => (
        <RateLimitBar key={i} rl={rl} />
      ))}

      {/* Credits */}
      {provider.credit_usage && (
        <div className={`${provider.rate_limits.length > 0 ? "mt-2 pt-2 border-t border-[#2a2b36]" : ""}`}>
          <div className="flex items-center justify-between text-[11px]">
            <span className="text-[#9394a1]">Credits</span>
            {unlimitedCredits ? (
              <span className="text-xs px-1.5 py-0.5 rounded bg-green-500/20 text-green-400 font-medium">
                Unlimited
              </span>
            ) : (
              <span className="text-[#e8e9ed] font-mono">
                {formatCurrency(provider.credit_usage.remaining, provider.credit_usage.currency)} remaining
              </span>
            )}
          </div>
        </div>
      )}

      {/* Session stats (all time) */}
      {(totalSessions != null || totalTokens != null) && (
        <div className="mt-2 pt-2 border-t border-[#2a2b36]">
          <div className="text-[10px] text-[#9394a1] uppercase tracking-wider mb-1.5">Session Stats</div>
          <div className="grid grid-cols-4 gap-1 mb-1.5">
            <div className="bg-[#0e0f13] rounded p-1.5">
              <div className="text-[8px] text-[#9394a1] uppercase">Sessions</div>
              <div className="text-[11px] text-[#e8e9ed] font-mono">{(totalSessions ?? 0).toLocaleString()}</div>
              <div className="text-[8px] text-[#9394a1]">All time</div>
            </div>
            <div className="bg-[#0e0f13] rounded p-1.5">
              <div className="text-[8px] text-[#9394a1] uppercase">Tokens</div>
              <div className="text-[11px] text-[#e8e9ed] font-mono">{formatTokenCount(totalTokens ?? 0)}</div>
            </div>
            <div className="bg-[#0e0f13] rounded p-1.5">
              <div className="text-[8px] text-[#9394a1] uppercase">Models</div>
              <div className="text-[11px] text-[#e8e9ed] font-mono">{modelCount}</div>
            </div>
            <div className="bg-[#0e0f13] rounded p-1.5">
              <div className="text-[8px] text-[#9394a1] uppercase">Projects</div>
              <div className="text-[11px] text-[#e8e9ed] font-mono">{projectCount}</div>
            </div>
          </div>
        </div>
      )}

      {/* Cost analysis */}
      {totalCost != null && totalCost > 0 && (
        <div className="mt-2 pt-2 border-t border-[#2a2b36]">
          <div className="text-[10px] text-[#9394a1] uppercase tracking-wider mb-1.5">Cost Analysis</div>
          <div className="flex items-center justify-between text-[11px] mb-1">
            <span className="text-[#9394a1]">API-Equivalent Value</span>
            <span className="text-emerald-400 font-mono font-semibold">{formatCost(totalCost)}</span>
          </div>
          {totalSessions != null && totalSessions > 0 && (
            <div className="flex items-center justify-between text-[11px]">
              <span className="text-[#9394a1]">Per Message</span>
              <span className="text-[#e8e9ed] font-mono">{formatCost(totalCost / totalSessions)}</span>
            </div>
          )}
          {costByModel && Object.keys(costByModel).length > 0 && (
            <div className="mt-1.5 space-y-0.5">
              {Object.entries(costByModel)
                .sort(([, a], [, b]) => b - a)
                .slice(0, 3)
                .map(([model, cost]) => (
                  <div key={model} className="flex items-center justify-between text-[9px]">
                    <span className="text-[#9394a1] truncate mr-2">{model}</span>
                    <span className="text-emerald-400 font-mono">{formatCost(cost)}</span>
                  </div>
                ))}
            </div>
          )}
        </div>
      )}

      {/* Time windows: Today / This Week */}
      {((todayStats.sessions ?? 0) > 0 || (weekStats.sessions ?? 0) > 0) && (
        <div className="mt-2 pt-2 border-t border-[#2a2b36]">
          <div className="grid grid-cols-2 gap-1">
            <div className="bg-[#0e0f13] rounded p-1.5">
              <div className="text-[9px] text-[#9394a1] mb-0.5">Today</div>
              <div className="text-[10px] text-[#e8e9ed]">{todayStats.sessions ?? 0} sessions</div>
              {(todayStats.tokens ?? 0) > 0 && <div className="text-[9px] text-[#9394a1]">{formatTokenCount(todayStats.tokens!)} tokens</div>}
              {(todayStats.cost ?? 0) > 0 && <div className="text-[9px] text-emerald-400 font-mono">{formatCost(todayStats.cost!)}</div>}
            </div>
            <div className="bg-[#0e0f13] rounded p-1.5">
              <div className="text-[9px] text-[#9394a1] mb-0.5">This Week</div>
              <div className="text-[10px] text-[#e8e9ed]">{weekStats.sessions ?? 0} sessions</div>
              {(weekStats.tokens ?? 0) > 0 && <div className="text-[9px] text-[#9394a1]">{formatTokenCount(weekStats.tokens!)} tokens</div>}
              {(weekStats.cost ?? 0) > 0 && <div className="text-[9px] text-emerald-400 font-mono">{formatCost(weekStats.cost!)}</div>}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ── Gemini Card ──────────────────────────────────────────────────────────

function GeminiCard({ provider }: { provider: TrayProviderSummary }) {
  const ex = provider.extra;
  const totalSessions = ex.gemini_total_sessions as number | undefined;
  const totalMessages = ex.gemini_total_messages as number | undefined;
  const upgradeUri = ex.upgrade_uri as string | undefined;

  return (
    <div>
      {/* Rate limits (Pro / Flash / Flash Lite) */}
      {provider.rate_limits.length > 0 && (
        <div className="mb-2">
          {provider.rate_limits.map((rl, i) => (
            <RateLimitBar key={i} rl={rl} />
          ))}
        </div>
      )}

      {/* Session stats */}
      {(totalSessions != null || totalMessages != null) && (
        <div className={`${provider.rate_limits.length > 0 ? "mt-2 pt-2 border-t border-[#2a2b36]" : ""}`}>
          <div className="text-[10px] text-[#9394a1] uppercase tracking-wider mb-1.5">Session Stats</div>
          <div className="grid grid-cols-2 gap-1">
            <div className="bg-[#0e0f13] rounded p-1.5">
              <div className="text-[8px] text-[#9394a1] uppercase">Sessions</div>
              <div className="text-[11px] text-[#e8e9ed] font-mono">{(totalSessions ?? 0).toLocaleString()}</div>
              <div className="text-[8px] text-[#9394a1]">All time</div>
            </div>
            <div className="bg-[#0e0f13] rounded p-1.5">
              <div className="text-[8px] text-[#9394a1] uppercase">Messages</div>
              <div className="text-[11px] text-[#e8e9ed] font-mono">{(totalMessages ?? 0).toLocaleString()}</div>
              <div className="text-[8px] text-[#9394a1]">All time</div>
            </div>
          </div>
        </div>
      )}

      {/* Upgrade link */}
      {upgradeUri && (
        <div className="mt-2 pt-2 border-t border-[#2a2b36]">
          <div className="text-[10px] text-[#9394a1]">
            Need higher limits?{" "}
            <a
              href={upgradeUri}
              target="_blank"
              rel="noopener noreferrer"
              className="text-[#4285f4] hover:underline"
            >
              Upgrade your plan
            </a>
          </div>
        </div>
      )}

      <TodayStats extra={provider.extra} providerId={provider.provider_id} />
    </div>
  );
}

// ── Main Export ───────────────────────────────────────────────────────────────

export function TrayProviderCard({
  provider,
}: {
  provider: TrayProviderSummary;
}) {
  if (!provider.connected) {
    return <DisconnectedCard provider={provider} />;
  }

  const iconImg = getAdapterIconImg(provider.provider_id);

  const renderContent = () => {
    switch (provider.provider_id) {
      case "claude-code":
        return <ClaudeCard provider={provider} />;
      case "cursor":
        return <CursorCard provider={provider} />;
      case "codex":
        return <CodexCard provider={provider} />;
      case "gemini":
        return <GeminiCard provider={provider} />;
      default:
        // Generic: show rate limits and credits
        return (
          <div>
            {provider.rate_limits.slice(0, 3).map((rl, i) => (
              <RateLimitBar key={i} rl={rl} />
            ))}
            {provider.credit_usage && (
              <div className="mt-2 text-[11px] text-[#9394a1]">
                Credits: {formatCurrency(provider.credit_usage.used, provider.credit_usage.currency)}
              </div>
            )}
          </div>
        );
    }
  };

  return (
    <div className="rounded-lg border border-[#2a2b36] bg-[#1a1b23] p-3">
      {/* Header */}
      <div className="flex items-center gap-2 mb-2.5">
        {iconImg ? (
          <img src={iconImg} alt="" className="w-5 h-5 rounded" />
        ) : (
          <div className="w-5 h-5 rounded bg-[#2a2b36]" />
        )}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-[#e8e9ed] text-sm font-medium truncate">
              {provider.provider_name}
            </span>
            {provider.plan && (
              <span className={`text-[10px] px-1.5 py-0.5 rounded whitespace-nowrap ${
                provider.plan.toLowerCase().includes("enterprise")
                  ? "bg-amber-500/20 text-amber-400"
                  : provider.plan.toLowerCase().includes("max")
                    ? "bg-purple-500/20 text-purple-400"
                    : "bg-[#2a2b36] text-[#9394a1]"
              }`}>
                {provider.plan}
              </span>
            )}
          </div>
          {provider.email && (
            <div className="text-[10px] text-[#9394a1] truncate">
              {provider.email}
            </div>
          )}
        </div>
        {/* Last refreshed timestamp */}
        {provider.fetched_at && (
          <span className="text-[9px] text-[#5f6070] whitespace-nowrap shrink-0" title={provider.fetched_at}>
            {formatTimeAgo(provider.fetched_at)}
          </span>
        )}
      </div>

      {provider.limit_state ? (
        <div className="mb-2">
          <LimitStateBanner
            limitState={provider.limit_state}
            variant="compact"
          />
        </div>
      ) : null}

      {/* Provider-specific content */}
      {renderContent()}

      {/* Error banner */}
      {provider.error && (
        <div className="mt-2 p-2 rounded bg-red-500/10 border border-red-500/20 text-[11px] text-red-400">
          {provider.error}
        </div>
      )}
    </div>
  );
}
