/**
 * DeepSeek Analytics V2 — local session analytics (Phase 1, no network for
 * sessions). Reads DeepSeek Harness (dsh) local caches under ~/.dsh. Balance
 * (DeepSeek platform API key) is folded in as its own card. Modeled on
 * KimiAnalyticsV2Page.
 */
import { useEffect, useState, useCallback } from "react";
import {
  getDeepSeekV2Overview,
  type DeepSeekV2Overview,
  type DeepSeekWorkspaceStat,
  type DeepSeekSessionStat,
  type KimiDailyActivity,
} from "../lib/tauri";
import { ProviderConnectModal } from "../components/analytics/ProviderConnectModal";

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatNum(n: number | null | undefined): string {
  if (n == null || n === 0) return "0";
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

function formatMoney(n: number, currency: string): string {
  if (currency === "USD") return `$${n.toFixed(2)}`;
  return `${n.toFixed(2)} ${currency}`;
}

function formatMs(ms: number | null | undefined): string {
  if (!ms) return "0s";
  const secs = ms / 1000;
  if (secs < 60) return `${secs.toFixed(1)}s`;
  const mins = secs / 60;
  if (mins < 60) return `${mins.toFixed(1)}m`;
  return `${(mins / 60).toFixed(1)}h`;
}

function shellQuote(p: string): string { return `'${p.replace(/'/g, "'\\''")}'`; }

function timeAgo(iso: string | null): string {
  if (!iso) return "never";
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "—";
  const secs = Math.floor((Date.now() - then) / 1000);
  if (secs < 0) return "just now";
  if (secs < 60) return "just now";
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.floor(hrs / 24);
  return `${days}d ago`;
}

const WEEKDAY_SHORT = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

// ── Small shared components (mirrors KimiAnalyticsV2Page) ──────────────────────

function StatCard({ label, value, sub, color }: { label: string; value: string; sub?: string; color?: string }) {
  return (
    <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] px-4 py-3">
      <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">{label}</div>
      <div className={`text-xl font-semibold ${color || "text-text-primary"}`}>{value}</div>
      {sub && <div className="text-[11px] text-text-muted mt-0.5">{sub}</div>}
    </div>
  );
}

function InfoTip({ text }: { text: string }) {
  const [show, setShow] = useState(false);
  useEffect(() => {
    if (!show) return;
    const close = () => setShow(false);
    const id = setTimeout(() => document.addEventListener("click", close), 0);
    return () => { clearTimeout(id); document.removeEventListener("click", close); };
  }, [show]);
  return (
    <span className="relative inline-flex">
      <button
        type="button"
        onClick={(e) => { e.stopPropagation(); setShow((s) => !s); }}
        title="What is this?"
        className="w-4 h-4 inline-flex items-center justify-center rounded-full border border-[#3a3b46] italic font-serif text-[10px] leading-none text-text-muted hover:text-text-primary hover:border-text-secondary"
      >
        i
      </button>
      {show && (
        <span className="absolute left-5 top-1/2 -translate-y-1/2 z-20 w-80 normal-case tracking-normal font-normal bg-[#1a1b23] border border-[#2a2b36] rounded-lg px-3 py-2 text-[11px] leading-relaxed text-text-secondary shadow-lg">
          {text}
        </span>
      )}
    </span>
  );
}

function Section({ title, children, defaultOpen = true, info }: { title: string; children: React.ReactNode; defaultOpen?: boolean; info?: string }) {
  const storageKey = `deepseek-v2-${title}`;
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
        {info && <InfoTip text={info} />}
      </div>
      {open && children}
    </div>
  );
}

// ── Workspaces table row ─────────────────────────────────────────────────────

function WorkspaceRow({ w }: { w: DeepSeekWorkspaceStat }) {
  const [copied, setCopied] = useState(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(shellQuote(w.path));
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch { /* clipboard unavailable */ }
  };
  return (
    <tr className="border-b border-[#1e1f2a] hover:bg-[#22232e]">
      <td className="px-3 py-2 w-[220px] max-w-[220px]">
        <div className="flex flex-col gap-0.5 min-w-0">
          <span className="block truncate text-text-primary" title={w.path}>{w.title}</span>
          {w.last_activity && <span className="text-[10px] text-text-muted">{timeAgo(w.last_activity)}</span>}
        </div>
      </td>
      <td className="px-3 py-2">
        <button
          onClick={copy}
          title="Copy the workspace directory path"
          className="text-[10px] px-1.5 py-0.5 rounded border border-[#2a2b36] text-text-secondary hover:text-text-primary whitespace-nowrap w-[92px]"
        >
          {copied ? "Copied" : "Copy directory"}
        </button>
      </td>
      <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(w.sessions)}</td>
      <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(w.turns)}</td>
      <td className="px-3 py-2 text-right text-text-muted font-mono">{formatNum(w.tokens)}</td>
    </tr>
  );
}

// ── Recent sessions list ─────────────────────────────────────────────────────

function RecentSessionRow({ s }: { s: DeepSeekSessionStat }) {
  return (
    <div className="flex items-center justify-between gap-3 px-3 py-2 border-b border-[#1e1f2a] last:border-b-0 hover:bg-[#22232e]">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2 min-w-0">
          <span className="text-xs text-text-primary truncate" title={s.title}>{s.title || "Untitled session"}</span>
          <span className="text-[9px] px-1.5 py-0.5 rounded border border-[#2a2b36] text-text-secondary whitespace-nowrap" title={s.workspace_path}>
            {s.workspace_name}
          </span>
        </div>
        <div className="text-[10px] text-text-muted mt-0.5">{timeAgo(s.created_at)}</div>
      </div>
      <div className="flex items-center gap-3 text-[11px] text-text-muted font-mono whitespace-nowrap">
        <span>{formatNum(s.turns)} turns</span>
        <span>{formatNum(s.tokens)} tok</span>
      </div>
    </div>
  );
}

// ── Activity calendar + hourly bar (mirrors KimiAnalyticsV2Page) ───────────────

const HEATMAP_COLORS_CAL = ["#161b22", "#39d353", "#26a641", "#006d32", "#0e4429"];

function ActivityCalendar({ daily }: { daily: KimiDailyActivity[] }) {
  const dayMap = new Map<string, number>();
  for (const d of daily) dayMap.set(d.date, d.message_count);

  const today = new Date(); today.setHours(0, 0, 0, 0);
  const startOfYear = new Date(today.getFullYear(), 0, 1);
  const firstSunday = new Date(startOfYear);
  firstSunday.setDate(firstSunday.getDate() - firstSunday.getDay());
  const dayMs = 86400000;

  type DayCell = { dateKey: string; date: Date; value: number };
  const weeks: DayCell[][] = [];
  const cur = new Date(firstSunday.getTime());
  while (cur.getTime() <= today.getTime()) {
    const week: DayCell[] = [];
    for (let d = 0; d < 7; d++) {
      const t = cur.getTime();
      if (t > today.getTime()) break;
      const dk = `${cur.getFullYear()}-${String(cur.getMonth() + 1).padStart(2, "0")}-${String(cur.getDate()).padStart(2, "0")}`;
      week.push({ dateKey: dk, date: new Date(t), value: dayMap.get(dk) ?? 0 });
      cur.setTime(t + dayMs);
    }
    if (week.length > 0) weeks.push(week);
    if (cur.getTime() > today.getTime()) break;
  }

  const monthLabels: { month: string; weekIndex: number }[] = [];
  weeks.forEach((week, wi) => {
    if (!week.length) return;
    const mn = week[0].date.toLocaleDateString(undefined, { month: "short" });
    const prev = wi > 0 && weeks[wi - 1]?.[0] ? weeks[wi - 1][0].date.toLocaleDateString(undefined, { month: "short" }) : "";
    if (wi === 0 || prev !== mn) monthLabels.push({ month: mn, weekIndex: wi });
  });

  const allValues = weeks.flatMap(w => w.map(d => d.value)).filter(v => v > 0);
  const sorted = [...allValues].sort((a, b) => a - b);
  const len = sorted.length;
  const getLevel = (v: number) => {
    if (v === 0 || len === 0) return 0;
    if (v >= sorted[Math.floor(len * 0.9)]) return 4;
    if (v >= sorted[Math.floor(len * 0.7)]) return 3;
    if (v >= sorted[Math.floor(len * 0.5)]) return 2;
    if (v >= sorted[Math.floor(len * 0.3)]) return 1;
    return 0;
  };

  const weekCount = weeks.length;
  if (weekCount === 0) return null;

  return (
    <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4 flex flex-col min-w-0">
      <p className="text-sm font-medium text-text-primary mb-3">Turns per day</p>
      <div
        className="w-full min-h-[100px]"
        style={{
          display: "grid",
          gridTemplateColumns: `repeat(${weekCount}, minmax(0, 1fr))`,
          gridTemplateRows: "auto repeat(7, minmax(0, 1fr))",
          gap: 1,
        }}
      >
        {weeks.map((_, wi) => {
          const label = monthLabels.find(m => m.weekIndex === wi);
          return (
            <div
              key={`month-${wi}`}
              className="min-w-0 h-4 flex items-center justify-center overflow-hidden"
              title={label?.month}
              style={{ gridColumn: wi + 1, gridRow: 1 }}
            >
              {label && <span className="text-[10px] text-text-muted truncate">{label.month}</span>}
            </div>
          );
        })}
        {weeks.map((week, wi) =>
          week.map((day, di) => {
            const level = getLevel(day.value);
            const color = HEATMAP_COLORS_CAL[level];
            return (
              <div
                key={`${wi}-${di}`}
                className="rounded-sm border border-white/5 min-h-[10px] min-w-[8px]"
                style={{ backgroundColor: color, gridColumn: wi + 1, gridRow: di + 2 }}
                title={`${day.date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })} — ${day.value > 0 ? formatNum(day.value) + " turns" : "No activity"}`}
              />
            );
          })
        )}
      </div>
      <div className="flex items-center gap-2 mt-2 text-xs text-text-muted flex-shrink-0">
        <span>Less</span>
        {HEATMAP_COLORS_CAL.map((color, i) => (
          <div key={i} className="w-3 h-3 rounded-sm border border-white/5" style={{ backgroundColor: color }} />
        ))}
        <span>More</span>
      </div>
    </div>
  );
}

function HourlyBar({ hourCounts, peakHour }: { hourCounts: number[]; peakHour: number | null }) {
  const max = Math.max(1, ...hourCounts);
  return (
    <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
      <p className="text-sm font-medium text-text-primary mb-3">Activity by hour (local)</p>
      <div className="flex items-end gap-[2px] h-24">
        {hourCounts.map((c, h) => (
          <div key={h} className="flex-1 flex flex-col items-center justify-end h-full" title={`${h}:00 — ${c} turns`}>
            <div
              className={`w-full rounded-t-sm ${h === peakHour ? "bg-blue-400" : "bg-blue-500/40"}`}
              style={{ height: `${(c / max) * 100}%` }}
            />
          </div>
        ))}
      </div>
      <div className="flex justify-between mt-1 text-[9px] text-text-muted">
        <span>0h</span><span>6h</span><span>12h</span><span>18h</span><span>23h</span>
      </div>
    </div>
  );
}

// ── Balance card ──────────────────────────────────────────────────────────────

function BalanceSection({ overview, onRefresh }: { overview: DeepSeekV2Overview; onRefresh: () => void }) {
  const [showConnect, setShowConnect] = useState(false);
  const balance = overview.balance;

  return (
    <Section
      title="Balance"
      info="Your DeepSeek platform API key balance (api.deepseek.com/user/balance) — a separate, prepaid billing system from the local session data above."
    >
      <div className="flex items-center justify-end gap-2 mb-3">
        <button
          onClick={onRefresh}
          className="px-2.5 py-1 text-[11px] rounded-lg bg-[#2a2b36] text-text-primary hover:bg-[#32333e]"
        >
          Refresh
        </button>
        <button
          onClick={() => setShowConnect(true)}
          className="px-2.5 py-1 text-[11px] rounded-lg bg-accent-blue text-white hover:bg-accent-blue/90"
        >
          {overview.balance_connected ? "Update key" : "Connect DeepSeek"}
        </button>
      </div>

      {!overview.balance_connected && (
        <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-xl p-6 text-center">
          <div className="text-3xl mb-3">🔑</div>
          <h4 className="text-sm font-semibold text-text-primary mb-1">Connect DeepSeek</h4>
          <p className="text-xs text-text-muted max-w-md mx-auto mb-4">
            Add your DeepSeek platform API key (sk-...) to see your account balance here. It is stored securely in your OS keychain.
          </p>
          <button
            onClick={() => setShowConnect(true)}
            className="px-4 py-2 text-xs rounded-lg bg-accent-blue text-white font-medium hover:bg-accent-blue/90"
          >
            Add API key
          </button>
        </div>
      )}

      {overview.balance_connected && balance && (
        <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-xl p-6">
          <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">Available balance</div>
          <div className="text-3xl font-semibold text-emerald-400">{formatMoney(balance.available, balance.currency)}</div>
        </div>
      )}

      {overview.balance_connected && !balance && (
        <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-xl p-6 text-center text-xs text-text-muted">
          Connected, but no balance data was returned. Try Refresh.
        </div>
      )}

      {showConnect && (
        <ProviderConnectModal
          providerId="deepseek"
          providerName="DeepSeek"
          authType="token"
          onClose={() => { setShowConnect(false); onRefresh(); }}
        />
      )}
    </Section>
  );
}

// ── Main page ──────────────────────────────────────────────────────────────────

const TIME_RANGES = [
  { key: "7d", label: "7d" },
  { key: "30d", label: "30d" },
  { key: "90d", label: "90d" },
  { key: "all", label: "All" },
];

export function DeepSeekAnalyticsV2Page() {
  const [overview, setOverview] = useState<DeepSeekV2Overview | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [timeRange, setTimeRange] = useState("all");
  const [lastRefreshed, setLastRefreshed] = useState<string | null>(null);

  const loadData = useCallback(async (range: string, force: boolean) => {
    setLoading(true);
    setError(null);
    try {
      const data = await getDeepSeekV2Overview(range, force);
      setOverview(data);
      setLastRefreshed(new Date().toISOString());
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { loadData("all", false); }, [loadData]);

  if (loading && !overview) {
    return <div className="p-6 text-text-muted">Loading DeepSeek analytics…</div>;
  }
  if (error && !overview) {
    return <div className="p-6 text-red-400">Failed to load: {error}</div>;
  }
  if (!overview) return null;

  const weekdayIdx = overview.most_active_weekday
    ? ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"].indexOf(overview.most_active_weekday)
    : -1;

  return (
    <div className="p-6 max-w-[1200px] mx-auto">
      {/* ── Header ── */}
      <div className="flex items-start justify-between mb-6">
        <div>
          <div className="flex items-center gap-2">
            <span className="text-lg">🐋</span>
            <h2 className="text-lg font-semibold text-text-primary">DeepSeek Analytics</h2>
            <span
              className={`inline-block w-2 h-2 rounded-full ${overview.connected ? "bg-emerald-400" : "bg-text-muted"}`}
              title={overview.connected ? "Local data available" : "No local data found"}
            />
          </div>
          <p className="text-xs text-text-muted mt-0.5">
            {overview.default_model && (
              <span>
                Model: {overview.default_model}
                {overview.default_model_reasoning_effort && ` (${overview.default_model_reasoning_effort})`}
              </span>
            )}
            {overview.connected && <span className="text-emerald-500"> · Local data</span>}
            {overview.active_now > 0 && (
              <span className="ml-2">
                <span className="inline-block w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse mr-1" />
                <span className="text-emerald-400">{overview.active_now} active now</span>
              </span>
            )}
          </p>
        </div>
        <div className="flex items-center gap-2">
          {TIME_RANGES.map(r => (
            <button
              key={r.key}
              onClick={() => { setTimeRange(r.key); loadData(r.key, false); }}
              className={`px-2.5 py-1 rounded text-[11px] font-medium transition-colors ${timeRange === r.key ? "bg-blue-500 text-white" : "bg-[#1a1b23] text-text-secondary hover:bg-[#22232e]"}`}
            >{r.label}</button>
          ))}
          {lastRefreshed && !loading && (
            <span className="text-[10px] text-text-muted">Updated {timeAgo(lastRefreshed)}</span>
          )}
          <button
            onClick={() => loadData(timeRange, true)}
            className={`p-1.5 rounded transition-colors ${loading ? "text-blue-400 bg-blue-500/10" : "text-text-muted hover:text-text-primary hover:bg-[#1a1b23]"}`}
            title="Force refresh"
          >
            <svg className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`} fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
              <path strokeLinecap="round" strokeLinejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
            </svg>
          </button>
        </div>
      </div>

      {!overview.connected && (
        <div className="mb-6 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-300">
          No DeepSeek Harness data found under <code>~/.dsh</code>. Start a session with the DeepSeek CLI (<code>dsh</code>) to see analytics here.
        </div>
      )}

      <div className={`transition-opacity duration-300 ${loading ? "opacity-60" : "opacity-100"}`}>
        {/* ── Session Overview ── */}
        <Section title="Session Overview">
          <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
            <StatCard label="Sessions" value={formatNum(overview.total_sessions)} sub={`${formatNum(overview.total_turns)} turns`} />
            <StatCard label="Steps" value={formatNum(overview.total_steps)} />
            <StatCard label="Tokens" value={formatNum(overview.total_tokens)} color="text-blue-400" />
            <StatCard label="Active now" value={formatNum(overview.active_now)} sub="last 15 min" color={overview.active_now > 0 ? "text-emerald-400" : undefined} />
            <StatCard label="LLM time" value={formatMs(overview.total_llm_ms)} sub={`tool time ${formatMs(overview.total_tool_ms)}`} />
            <StatCard label="Active days" value={formatNum(overview.active_days)} sub={overview.total_days > 0 ? `of ${overview.total_days} tracked` : undefined} />
            <StatCard label="Current streak" value={`${overview.current_streak}d`} sub={`longest ${overview.longest_streak}d`} />
            <StatCard label="Peak hour" value={overview.peak_hour != null ? `${overview.peak_hour}:00` : "—"} sub={weekdayIdx >= 0 ? `most active ${WEEKDAY_SHORT[weekdayIdx]}` : undefined} />
          </div>
        </Section>

        {/* ── Workspaces ── */}
        <Section title="Workspaces" info="DeepSeek Harness sessions grouped by workspace directory (~/.dsh/storages/workspace.json).">
          {overview.workspaces.length === 0 ? (
            <p className="text-sm text-text-muted">No workspaces yet.</p>
          ) : (
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-x-auto">
              <table className="w-full text-xs">
                <thead>
                  <tr className="text-left text-text-muted border-b border-[#2a2b36]">
                    <th className="px-3 py-2 font-medium">Workspace</th>
                    <th className="px-3 py-2 font-medium">Actions</th>
                    <th className="px-3 py-2 font-medium text-right">Sessions</th>
                    <th className="px-3 py-2 font-medium text-right">Turns</th>
                    <th className="px-3 py-2 font-medium text-right">Tokens</th>
                  </tr>
                </thead>
                <tbody>
                  {overview.workspaces.map(w => <WorkspaceRow key={w.path} w={w} />)}
                </tbody>
              </table>
            </div>
          )}
        </Section>

        {/* ── Activity ── */}
        <Section title="Activity">
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-3">
            <ActivityCalendar daily={overview.daily_activity} />
            <HourlyBar hourCounts={overview.hour_counts} peakHour={overview.peak_hour} />
          </div>
        </Section>

        {/* ── Recent sessions ── */}
        <Section title="Recent Sessions">
          {overview.recent_sessions.length === 0 ? (
            <p className="text-sm text-text-muted">No sessions yet.</p>
          ) : (
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-hidden">
              {overview.recent_sessions.slice(0, 20).map(s => <RecentSessionRow key={s.session_id} s={s} />)}
            </div>
          )}
        </Section>

        {/* ── Balance ── */}
        <BalanceSection overview={overview} onRefresh={() => loadData(timeRange, true)} />
      </div>
    </div>
  );
}
