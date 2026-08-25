/**
 * Kimi Code Analytics V2 — local session analytics (Phase 1, no auth/API).
 * Reads Kimi Code CLI's local files under ~/.kimi. Modeled on ClaudeAnalyticsV2Page.
 */
import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  getKimiV2Overview,
  getKimiV2PromptHistory,
  startKimiInProject,
  type KimiV2Overview,
  type KimiProjectStat,
  type KimiDailyActivity,
  type KimiPromptEntry,
  type KimiRateLimitWindow,
} from "../lib/tauri";
import type { ProviderAnalytics, ProviderInfo } from "../stores/analyticsStore";
import { ProviderConnectModal } from "../components/analytics/ProviderConnectModal";

// ── Helpers ───────────────────────────────────────────────────────────────────

function formatNum(n: number | null | undefined): string {
  if (n == null || n === 0) return "0";
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

function shellQuote(p: string): string { return `'${p.replace(/'/g, "'\\''")}'`; }

function formatMoney(n: number, currency: string): string {
  if (currency === "USD") return `$${n.toFixed(2)}`;
  return `${n.toFixed(2)} ${currency}`;
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

const WEEKDAY_SHORT = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

// ── Usage-limit helpers (mirror ClaudeAnalyticsV2Page severity/countdown) ──────

function severityLevel(pct: number): "critical" | "warning" | "normal" {
  if (pct >= 90) return "critical";
  if (pct >= 75) return "warning";
  return "normal";
}
function severityFillClass(pct: number): string {
  switch (severityLevel(pct)) {
    case "critical": return "bg-red-500";
    case "warning": return "bg-amber-500";
    default: return "bg-emerald-500";
  }
}
function severityTextClass(pct: number): string {
  switch (severityLevel(pct)) {
    case "critical": return "text-red-400";
    case "warning": return "text-amber-400";
    default: return "text-emerald-400";
  }
}

function resetCountdown(resetsAt: string | null, windowSeconds?: number | null): string {
  if (resetsAt) {
    const diff = new Date(resetsAt).getTime() - Date.now();
    if (Number.isNaN(diff)) return "—";
    if (diff <= 0) return "now";
    const s = Math.floor(diff / 1000);
    const h = Math.floor(s / 3600);
    const m = Math.floor((s % 3600) / 60);
    if (h > 24) return `${Math.floor(h / 24)}d ${h % 24}h`;
    if (h > 0) return `${h}h ${m}m`;
    return `${m}m`;
  }
  if (windowSeconds) {
    const h = Math.floor(windowSeconds / 3600);
    if (h >= 24) return `~${Math.floor(h / 24)}d`;
    return `~${h}h`;
  }
  return "—";
}

function isSessionWindow(rl: KimiRateLimitWindow): boolean {
  const l = rl.label.toLowerCase();
  return l.includes("session") || l.includes("5h");
}

function LimitMeter({ rl }: { rl: KimiRateLimitWindow }) {
  const exhausted = rl.used_percent >= 99;
  return (
    <div className="mb-3 last:mb-0">
      <div className="flex justify-between text-xs mb-1">
        <div className="flex items-center gap-2">
          <span className="text-text-secondary">{rl.label}</span>
          <span className="text-text-muted text-[10px]">
            Resets in {resetCountdown(rl.resets_at, rl.window_seconds)}
          </span>
        </div>
      </div>
      <div className="flex items-center gap-2">
        {exhausted ? (
          <div className="relative flex-1 h-2.5 rounded-full overflow-hidden bg-red-600">
            <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
              <span className="text-[8px] font-bold text-white tracking-wider drop-shadow-sm">
                {resetCountdown(rl.resets_at, rl.window_seconds) !== "—"
                  ? `RESET · ${resetCountdown(rl.resets_at, rl.window_seconds).toUpperCase()}`
                  : "LIMIT REACHED"}
              </span>
            </div>
          </div>
        ) : (
          <div className="flex-1 h-2.5 bg-[#0e0f13] rounded-full overflow-hidden">
            <div
              className={`h-full rounded-full transition-all duration-700 ${severityFillClass(rl.used_percent)}`}
              style={{ width: `${Math.min(rl.used_percent, 100)}%` }}
            />
          </div>
        )}
        <span className={`font-semibold text-xs whitespace-nowrap ${severityTextClass(rl.used_percent)}`}>
          {rl.used_percent.toFixed(0)}% Used
        </span>
      </div>
    </div>
  );
}

// ── Small shared components (mirrors ClaudeAnalyticsV2Page) ────────────────────

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
  const storageKey = `kimi-v2-${title}`;
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

// ── Projects table row (Start-session / Copy-resume / Copy-directory) ──────────

function ProjectRow({ p }: { p: KimiProjectStat }) {
  const [copied, setCopied] = useState<"resume" | "dir" | null>(null);
  const copy = async (text: string, which: "resume" | "dir") => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(which);
      setTimeout(() => setCopied(null), 1500);
    } catch { /* clipboard unavailable */ }
  };
  const resumeCmd = p.last_session_id
    ? `cd ${shellQuote(p.project_path)} && kimi --resume ${p.last_session_id}`
    : `cd ${shellQuote(p.project_path)} && kimi`;
  return (
    <tr className="border-b border-[#1e1f2a] hover:bg-[#22232e]">
      <td className="px-3 py-2 w-[220px] max-w-[220px]">
        <div className="flex flex-col gap-0.5 min-w-0">
          <span className="block truncate text-text-primary" title={p.project_path}>{p.project_name}</span>
          {p.last_activity && <span className="text-[10px] text-text-muted">{timeAgo(p.last_activity)}</span>}
        </div>
      </td>
      <td className="px-3 py-2">
        <div className="flex items-center gap-1">
          <button
            onClick={() => startKimiInProject(p.project_path).catch(() => {})}
            title="Start a Kimi session in this project directory"
            className="text-[10px] px-1.5 py-0.5 rounded bg-cyan-500/15 text-cyan-400 hover:bg-cyan-500/25 whitespace-nowrap"
          >
            Start session
          </button>
          <button
            onClick={() => copy(resumeCmd, "resume")}
            title={resumeCmd}
            className="text-[10px] px-1.5 py-0.5 rounded border border-[#2a2b36] text-text-secondary hover:text-text-primary whitespace-nowrap w-[86px]"
          >
            {copied === "resume" ? "Copied" : "Copy resume"}
          </button>
          <button
            onClick={() => copy(p.project_path, "dir")}
            title="Copy the project directory path"
            className="text-[10px] px-1.5 py-0.5 rounded border border-[#2a2b36] text-text-secondary hover:text-text-primary whitespace-nowrap w-[92px]"
          >
            {copied === "dir" ? "Copied" : "Copy directory"}
          </button>
        </div>
      </td>
      <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(p.sessions)}</td>
      <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(p.messages)}</td>
      <td className="px-3 py-2 text-right text-text-muted font-mono">{formatNum(p.context_tokens_peak)}</td>
    </tr>
  );
}

// ── Activity calendar (GitHub-style, from daily_activity message counts) ───────

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
              className={`w-full rounded-t-sm ${h === peakHour ? "bg-cyan-400" : "bg-cyan-500/40"}`}
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

// ── Moonshot balance (separate prepaid platform API key, not the Kimi subscription) ──

function MoonshotBalanceSection() {
  const [info, setInfo] = useState<ProviderInfo | null>(null);
  const [analytics, setAnalytics] = useState<ProviderAnalytics | null>(null);
  const [loading, setLoading] = useState(true);
  const [showConnect, setShowConnect] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const infos = await invoke<ProviderInfo[]>("get_all_provider_info");
      setInfo(infos.find((i) => i.id === "moonshot") ?? null);
      const a = await invoke<ProviderAnalytics>("get_provider_analytics", { providerId: "moonshot" });
      setAnalytics(a);
    } catch (e) {
      console.error("[MoonshotBalance] load error:", e);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const status = analytics?.status;
  const connected = !!status?.connected;
  const credit = analytics?.credit_usage;
  const extra = (analytics?.extra ?? {}) as Record<string, unknown>;
  const extraMoney = (key: string): number | null => {
    const v = extra[key];
    return typeof v === "number" ? v : null;
  };

  return (
    <Section
      title="Moonshot Balance"
      info="Moonshot's platform API key (sk-...) is a separate, prepaid billing system from the Kimi subscription usage limits above."
    >
      <div className="flex items-center justify-end gap-2 mb-3">
        <button
          onClick={load}
          className="px-2.5 py-1 text-[11px] rounded-lg bg-[#2a2b36] text-text-primary hover:bg-[#32333e]"
        >
          Refresh
        </button>
        <button
          onClick={() => setShowConnect(true)}
          className="px-2.5 py-1 text-[11px] rounded-lg bg-accent-blue text-white hover:bg-accent-blue/90"
        >
          {connected ? "Update key" : "Add API key"}
        </button>
      </div>

      {!loading && !connected && (
        <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-xl p-6 text-center">
          <div className="text-3xl mb-3">🔑</div>
          <h4 className="text-sm font-semibold text-text-primary mb-1">Connect Moonshot</h4>
          <p className="text-xs text-text-muted max-w-md mx-auto mb-4">
            The Kimi subscription usage limits above and the Moonshot prepaid balance are two independent billing systems —
            add your Moonshot platform API key (sk-...) to see your prepaid balance here.
          </p>
          <button
            onClick={() => setShowConnect(true)}
            className="px-4 py-2 text-xs rounded-lg bg-accent-blue text-white font-medium hover:bg-accent-blue/90"
          >
            Add API key
          </button>
        </div>
      )}

      {!loading && connected && credit && (
        <>
          <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-xl p-6 mb-3">
            <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">
              {credit.plan_name || "Available balance"}
            </div>
            <div className="text-3xl font-semibold text-emerald-400">
              {formatMoney(credit.remaining, credit.currency)}
            </div>
            {status?.error && (
              <div className="text-xs text-amber-400 mt-2">{status.error}</div>
            )}
          </div>
          <div className="grid grid-cols-2 md:grid-cols-3 gap-3">
            {(["voucher_balance", "cash_balance"] as const).map((k) => {
              const v = extraMoney(k);
              if (v == null) return null;
              return (
                <div key={k} className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg px-4 py-3">
                  <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">
                    {k.replace(/_/g, " ")}
                  </div>
                  <div className="text-lg font-semibold text-text-primary">
                    {formatMoney(v, credit.currency)}
                  </div>
                </div>
              );
            })}
          </div>
        </>
      )}

      {!loading && connected && !credit && (
        <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-xl p-6 text-center text-xs text-text-muted">
          {status?.error || "Connected, but no balance data was returned. Try Refresh."}
        </div>
      )}

      {showConnect && (
        <ProviderConnectModal
          providerId="moonshot"
          providerName={info?.name ?? "Moonshot"}
          authType={info?.auth_type ?? "token"}
          onClose={() => { setShowConnect(false); load(); }}
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

export function KimiAnalyticsV2Page() {
  const [overview, setOverview] = useState<KimiV2Overview | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [timeRange, setTimeRange] = useState("all");
  const [lastRefreshed, setLastRefreshed] = useState<string | null>(null);

  const [prompts, setPrompts] = useState<KimiPromptEntry[]>([]);
  const [promptTotal, setPromptTotal] = useState(0);
  const [promptQuery, setPromptQuery] = useState("");

  const loadData = useCallback(async (range: string, force: boolean) => {
    setLoading(true);
    setError(null);
    try {
      const data = await getKimiV2Overview(range, force);
      setOverview(data);
      setLastRefreshed(new Date().toISOString());
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const loadPrompts = useCallback(async (query: string) => {
    try {
      const page = await getKimiV2PromptHistory(query.trim() ? query.trim() : null, 1, 200);
      setPrompts(page.entries);
      setPromptTotal(page.total_count);
    } catch { /* noop */ }
  }, []);

  useEffect(() => { loadData("all", false); }, [loadData]);
  useEffect(() => {
    const id = setTimeout(() => loadPrompts(promptQuery), 250);
    return () => clearTimeout(id);
  }, [promptQuery, loadPrompts]);

  if (loading && !overview) {
    return <div className="p-6 text-text-muted">Loading Kimi analytics…</div>;
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
            <span className="text-lg">🌙</span>
            <h2 className="text-lg font-semibold text-text-primary">Kimi Code Analytics</h2>
            <span
              className={`inline-block w-2 h-2 rounded-full ${overview.connected ? "bg-emerald-400" : "bg-text-muted"}`}
              title={overview.connected ? "Local data available" : "No local data found"}
            />
          </div>
          <p className="text-xs text-text-muted mt-0.5">
            {overview.default_model && <span>Model: {overview.default_model}</span>}
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
              className={`px-2.5 py-1 rounded text-[11px] font-medium transition-colors ${timeRange === r.key ? "bg-cyan-500 text-white" : "bg-[#1a1b23] text-text-secondary hover:bg-[#22232e]"}`}
            >{r.label}</button>
          ))}
          {lastRefreshed && !loading && (
            <span className="text-[10px] text-text-muted">Updated {timeAgo(lastRefreshed)}</span>
          )}
          <button
            onClick={() => loadData(timeRange, true)}
            className={`p-1.5 rounded transition-colors ${loading ? "text-cyan-400 bg-cyan-500/10" : "text-text-muted hover:text-text-primary hover:bg-[#1a1b23]"}`}
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
          No Kimi Code data found under <code>~/.kimi/sessions</code>. Start a session with the Kimi CLI to see analytics here.
        </div>
      )}

      <div className={`transition-opacity duration-300 ${loading ? "opacity-60" : "opacity-100"}`}>
        {/* ── Usage Limits: Moonshot API-key balance OR OAuth subscription quotas ── */}
        {(() => {
          if (overview.auth_mode === "api") {
            const bal = overview.moonshot_balance;
            if (!bal) return null;
            return (
              <Section
                title="Usage Limits"
                info="Your Kimi CLI is authenticated with a Moonshot platform API key (found in ~/.kimi/config.toml), not the Kimi Code subscription — this shows your prepaid Moonshot balance instead of subscription quotas."
              >
                <div className="text-[10px] text-cyan-400 uppercase tracking-wider mb-2">Auth: Moonshot API key</div>
                <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-xl p-6 mb-3">
                  <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">Available balance</div>
                  <div className="text-3xl font-semibold text-emerald-400">{formatMoney(bal.available, bal.currency)}</div>
                  <div className="text-[11px] text-text-muted mt-2">From your Kimi config (Moonshot API key)</div>
                </div>
                <div className="grid grid-cols-2 md:grid-cols-3 gap-3">
                  <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg px-4 py-3">
                    <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">Voucher balance</div>
                    <div className="text-lg font-semibold text-text-primary">{formatMoney(bal.voucher, bal.currency)}</div>
                  </div>
                  <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg px-4 py-3">
                    <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">Cash balance</div>
                    <div className="text-lg font-semibold text-text-primary">{formatMoney(bal.cash, bal.currency)}</div>
                  </div>
                </div>
              </Section>
            );
          }
          const needsReconnect =
            overview.limit_state?.kind === "unauthenticated" ||
            (!overview.usage_connected && overview.limit_state == null && overview.rate_limits.length === 0);
          const sessionLimits = overview.rate_limits.filter(isSessionWindow);
          const weeklyLimits = overview.rate_limits.filter((rl) => !isSessionWindow(rl));
          const hasLimits = overview.rate_limits.length > 0;
          if (!hasLimits && !needsReconnect) return null;
          return (
            <Section
              title="Usage Limits"
              info="Kimi subscription quotas from the Kimi Code OAuth token: the rolling 5h session window and weekly request allowance. Read live from api.kimi.com; the token is auto-refreshed."
            >
              {needsReconnect && (
                <div className="mb-3 rounded-lg border border-amber-500/30 bg-amber-500/10 px-4 py-3 text-sm text-amber-300">
                  {overview.limit_state?.kind === "unauthenticated" && "message" in overview.limit_state
                    ? overview.limit_state.message
                    : "Kimi is not connected."}{" "}
                  Run <code>kimi login</code> in your terminal to view subscription usage limits.
                </div>
              )}
              {hasLimits && (
                <div className="space-y-4">
                  {sessionLimits.length > 0 && (
                    <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
                      <h4 className="text-xs font-semibold text-text-primary mb-3">Current Session</h4>
                      {sessionLimits.map((rl, i) => <LimitMeter key={i} rl={rl} />)}
                    </div>
                  )}
                  {weeklyLimits.length > 0 && (
                    <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
                      <h4 className="text-xs font-semibold text-text-primary mb-3">Weekly Limits</h4>
                      {weeklyLimits.map((rl, i) => <LimitMeter key={i} rl={rl} />)}
                    </div>
                  )}
                </div>
              )}
            </Section>
          );
        })()}

        {/* ── Session Overview ── */}
        <Section title="Session Overview">
          <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
            <StatCard label="Sessions" value={formatNum(overview.total_sessions)} sub={`${formatNum(overview.total_turns)} turns`} />
            <StatCard label="Messages" value={formatNum(overview.total_messages)} sub={`${formatNum(overview.user_messages)} user · ${formatNum(overview.assistant_messages)} assistant`} />
            <StatCard label="Peak context" value={formatNum(overview.context_tokens_peak)} sub="largest single session" color="text-cyan-400" />
            <StatCard label="Active now" value={formatNum(overview.active_now)} sub="last 15 min" color={overview.active_now > 0 ? "text-emerald-400" : undefined} />
            <StatCard label="Active days" value={formatNum(overview.active_days)} sub={overview.total_days > 0 ? `of ${overview.total_days} tracked` : undefined} />
            <StatCard label="Current streak" value={`${overview.current_streak}d`} sub={`longest ${overview.longest_streak}d`} />
            <StatCard label="Peak hour" value={overview.peak_hour != null ? `${overview.peak_hour}:00` : "—"} />
            <StatCard label="Most active" value={weekdayIdx >= 0 ? WEEKDAY_SHORT[weekdayIdx] : "—"} sub={overview.first_session_date ? `since ${overview.first_session_date}` : undefined} />
          </div>
        </Section>

        {/* ── Projects ── */}
        <Section title="Projects" info="Kimi sessions grouped by working directory. 'Copy resume' copies a `cd … && kimi --resume <sessionId>` command for the project's most recent session.">
          {overview.projects.length === 0 ? (
            <p className="text-sm text-text-muted">No projects yet.</p>
          ) : (
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-x-auto">
              <table className="w-full text-xs">
                <thead>
                  <tr className="text-left text-text-muted border-b border-[#2a2b36]">
                    <th className="px-3 py-2 font-medium">Project</th>
                    <th className="px-3 py-2 font-medium">Actions</th>
                    <th className="px-3 py-2 font-medium text-right">Sessions</th>
                    <th className="px-3 py-2 font-medium text-right">Messages</th>
                    <th className="px-3 py-2 font-medium text-right">Peak ctx</th>
                  </tr>
                </thead>
                <tbody>
                  {overview.projects.map(p => <ProjectRow key={p.project_path} p={p} />)}
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

        {/* ── Model catalog ── */}
        <Section title="Model Catalog" info="Models declared in ~/.kimi/config.toml, with each model's max context window and capabilities.">
          {overview.models.length === 0 ? (
            <p className="text-sm text-text-muted">No models configured.</p>
          ) : (
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-x-auto">
              <table className="w-full text-xs">
                <thead>
                  <tr className="text-left text-text-muted border-b border-[#2a2b36]">
                    <th className="px-3 py-2 font-medium">Model</th>
                    <th className="px-3 py-2 font-medium text-right">Context window</th>
                    <th className="px-3 py-2 font-medium">Capabilities</th>
                  </tr>
                </thead>
                <tbody>
                  {overview.models.map(m => (
                    <tr key={m.id} className="border-b border-[#1e1f2a] hover:bg-[#22232e]">
                      <td className="px-3 py-2">
                        <div className="flex items-center gap-2">
                          <span className="text-text-primary">{m.model ?? m.id}</span>
                          {overview.default_model === m.id && (
                            <span className="text-[9px] px-1.5 py-0.5 rounded bg-cyan-500/20 text-cyan-400">default</span>
                          )}
                        </div>
                        <span className="text-[10px] text-text-muted">{m.provider ?? ""}</span>
                      </td>
                      <td className="px-3 py-2 text-right text-text-secondary font-mono">
                        {m.max_context_size != null ? `${formatNum(m.max_context_size)}` : "—"}
                      </td>
                      <td className="px-3 py-2">
                        <div className="flex flex-wrap gap-1">
                          {m.capabilities.map(c => (
                            <span key={c} className="text-[9px] px-1.5 py-0.5 rounded border border-[#2a2b36] text-text-secondary">{c}</span>
                          ))}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </Section>

        {/* ── Moonshot Balance (manual connect) — hidden when the API key was
             already auto-detected from ~/.kimi/config.toml above ── */}
        {overview.moonshot_source !== "local-config" && <MoonshotBalanceSection />}

        {/* ── Prompt History ── */}
        <Section title="Prompt History" defaultOpen={false} info="Your prompts across Kimi projects, read from ~/.kimi/user-history. Searchable.">
          <div className="mb-3">
            <input
              type="text"
              value={promptQuery}
              onChange={(e) => setPromptQuery(e.target.value)}
              placeholder="Search prompts…"
              className="w-full max-w-md px-3 py-1.5 rounded bg-[#1a1b23] border border-[#2a2b36] text-xs text-text-primary placeholder-text-muted focus:outline-none focus:border-cyan-500/50"
            />
            <span className="ml-2 text-[10px] text-text-muted">{formatNum(promptTotal)} total</span>
          </div>
          {prompts.length === 0 ? (
            <p className="text-sm text-text-muted">No prompts found.</p>
          ) : (
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] divide-y divide-[#1e1f2a] max-h-[420px] overflow-y-auto">
              {prompts.map((p, i) => (
                <div key={i} className="px-3 py-2 hover:bg-[#22232e]">
                  <div className="text-xs text-text-primary whitespace-pre-wrap break-words">{p.content}</div>
                  <div className="text-[10px] text-text-muted mt-0.5">{p.project_name}</div>
                </div>
              ))}
            </div>
          )}
        </Section>
      </div>
    </div>
  );
}
