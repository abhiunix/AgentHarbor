/**
 * Claude Code Analytics V2 — comprehensive dashboard powered by OAuth API + local data.
 * Zero-config: OAuth token auto-detected, local JSONL/stats always available.
 */
import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid, Legend,
  PieChart, Pie, Cell, AreaChart, Area,
} from "recharts";

// ── Types ───────────────────────────────────────────────────────────────────

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
}

interface DailyActivity {
  date: string;
  message_count: number;
  session_count: number;
  tool_call_count: number;
}

interface ModelStat {
  model: string;
  message_count: number;
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  estimated_cost_usd: number;
}

interface ToolStat { tool_name: string; count: number }
interface GeoStat { region: string; count: number }
interface ProjectStat { project_path: string; project_name: string; message_count: number; total_tokens: number; estimated_cost_usd: number }
interface CacheStats { hit_rate_percent: number; ephemeral_5m_percent: number; ephemeral_1h_percent: number; total_cache_read: number; total_cache_write: number }
interface ActiveSession { pid: number; session_id: string; cwd: string; started_at: number; is_running: boolean }
interface TodosSummary { total_sessions_with_todos: number; total_todos: number; completed_todos: number; pending_todos: number }
interface TokenTimePoint { date: string; input: number; output: number; cache_read: number; cache_write: number; estimated_cost: number }
interface ModelTimePoint { date: string; models: Record<string, number> }
interface MessageLogEntry { timestamp: string; model: string | null; input_tokens: number; output_tokens: number; cache_read: number; cache_write: number; tools: string[]; service_tier: string | null; geo: string | null; speed: string | null; has_thinking: boolean; estimated_cost: number; project: string | null }
interface MessageLogPage { entries: MessageLogEntry[]; total_count: number; page: number; page_size: number }

interface Overview {
  connected: boolean;
  connection_method: string;
  email: string | null;
  plan: string | null;
  org_name: string | null;
  rate_limits: RateLimitWindow[];
  credit_usage: CreditUsage | null;
  extra: Record<string, unknown>;
  account_info: Record<string, unknown> | null;
  total_sessions: number;
  total_messages: number;
  total_tool_calls: number;
  longest_session_duration: number;
  longest_session_messages: number;
  first_session_date: string | null;
  hour_counts: number[];
  daily_activity: DailyActivity[];
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_read: number;
  total_cache_write: number;
  total_web_searches: number;
  total_web_fetches: number;
  thinking_message_count: number;
  total_message_count: number;
  estimated_total_cost: number;
  model_breakdown: ModelStat[];
  tool_usage: ToolStat[];
  cache_stats: CacheStats;
  geo_breakdown: GeoStat[];
  project_breakdown: ProjectStat[];
  active_sessions: ActiveSession[];
  stats_cache_models: Record<string, { input_tokens: number; output_tokens: number; cache_read_input_tokens: number; cache_creation_input_tokens: number; cost_usd: number; web_search_requests: number; context_window: number; max_output_tokens: number }>;
  active_session_count: number;
  num_startups: number;
  install_method: string | null;
  plugins_count: number;
  custom_commands_count: number;
  todos_summary: TodosSummary;
  plans_count: number;
  hooks_count: number;
  file_history_checkpoints: number;
  favorite_model: string | null;
  active_days: number;
  total_days: number;
  longest_streak: number;
  current_streak: number;
  most_active_weekday: string | null;
  peak_hour: number | null;
}

// ── Helpers ─────────────────────────────────────────────────────────────────

function formatNum(n: number | null | undefined): string {
  if (n == null || n === 0) return "0";
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

function formatUsd(n: number): string { return `$${n.toFixed(2)}`; }

/** Numeric values from `overview.extra` (serde_json can deserialize as number). */
function extraNum(v: unknown): number | undefined {
  if (v == null) return undefined;
  if (typeof v === "number" && Number.isFinite(v)) return v;
  if (typeof v === "string") {
    const n = parseFloat(v);
    return Number.isFinite(n) ? n : undefined;
  }
  return undefined;
}

/** Menu bar tray "Today" row uses `start_today_*` from `enrich_with_today_stats` (Rust). */
type ClaudeTrayTodayDisplay = {
  estimatedTotalCost: number;
  inputCost: number;
  outputCost: number;
  cacheReadCost: number;
  cacheWriteCost: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  totalCacheRead: number;
  totalCacheWrite: number;
  totalMessageCount: number;
  totalTokensSum: number;
};

/** Calendar day since midnight IST (`start_today_*` keys). */
function getClaudeTrayIstTodayDisplay(extra: Record<string, unknown>): ClaudeTrayTodayDisplay | null {
  const cost = extraNum(extra.start_today_cost);
  if (cost == null || cost <= 0) return null;
  const inputTokens = extraNum(extra.start_today_input_tokens);
  const outputTokens = extraNum(extra.start_today_output_tokens);
  const cacheRead = extraNum(extra.start_today_cache_read);
  const cacheWrite = extraNum(extra.start_today_cache_write);
  if (inputTokens == null || outputTokens == null || cacheRead == null || cacheWrite == null) return null;
  const messages = extraNum(extra.start_today_messages) ?? 0;
  const totalTok = extraNum(extra.start_today_tokens) ?? inputTokens + outputTokens + cacheRead + cacheWrite;
  return {
    estimatedTotalCost: cost,
    inputCost: extraNum(extra.start_today_input_cost) ?? 0,
    outputCost: extraNum(extra.start_today_output_cost) ?? 0,
    cacheReadCost: extraNum(extra.start_today_cache_read_cost) ?? 0,
    cacheWriteCost: extraNum(extra.start_today_cache_write_cost) ?? 0,
    totalInputTokens: inputTokens,
    totalOutputTokens: outputTokens,
    totalCacheRead: cacheRead,
    totalCacheWrite: cacheWrite,
    totalMessageCount: messages,
    totalTokensSum: totalTok,
  };
}

function formatDuration(ms: number): string {
  const s = Math.floor(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

function timeAgo(iso: string | null): string {
  if (!iso) return "";
  const diff = Date.now() - new Date(iso).getTime();
  const s = Math.floor(diff / 1000);
  if (s < 60) return "just now";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ago`;
  return `${Math.floor(m / 60)}h ago`;
}

function resetCountdown(resetsAt: string | null): string {
  if (!resetsAt) return "";
  const diff = new Date(resetsAt).getTime() - Date.now();
  if (diff <= 0) return "now";
  const s = Math.floor(diff / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 24) return `${Math.floor(h / 24)}d ${h % 24}h`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

const COLORS = ["#3b82f6", "#8b5cf6", "#22c55e", "#f59e0b", "#ef4444", "#06b6d4", "#ec4899", "#84cc16", "#a855f7", "#14b8a6"];
const TOOLTIP_STYLE = { background: "#1a1b23", border: "1px solid #2a2b36", borderRadius: "8px", fontSize: "11px" };

// ── Skeleton Components ──────────────────────────────────────────────────────

function SkeletonPulse({ className = "", style }: { className?: string; style?: React.CSSProperties }) {
  return <div className={`animate-pulse bg-[#2a2b36] rounded ${className}`} style={style} />;
}

function SkeletonStatCards({ count = 4 }: { count?: number }) {
  return (
    <div className={`grid grid-cols-2 md:grid-cols-${count} gap-3`}>
      {Array.from({ length: count }).map((_, i) => (
        <div key={i} className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] px-4 py-3">
          <SkeletonPulse className="h-3 w-20 mb-2" />
          <SkeletonPulse className="h-6 w-16 mb-1" />
          <SkeletonPulse className="h-2.5 w-24" />
        </div>
      ))}
    </div>
  );
}

function SkeletonChart() {
  return (
    <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
      <div className="flex justify-end mb-2"><SkeletonPulse className="h-5 w-24" /></div>
      <div className="space-y-3 py-4">
        {[80, 60, 90, 40, 70, 55, 85].map((w, i) => (
          <div key={i} className="flex items-end gap-1">
            <SkeletonPulse className="h-4 w-8" />
            <SkeletonPulse className={`h-4`} style={{ width: `${w}%` }} />
          </div>
        ))}
      </div>
    </div>
  );
}

function SkeletonTable({ rows = 4 }: { rows?: number }) {
  return (
    <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-hidden">
      <div className="flex gap-4 px-3 py-2 border-b border-[#2a2b36]">
        {[60, 40, 40, 40, 50].map((w, i) => <SkeletonPulse key={i} className="h-3" style={{ width: w }} />)}
      </div>
      {Array.from({ length: rows }).map((_, i) => (
        <div key={i} className="flex gap-4 px-3 py-2.5 border-b border-[#1e1f2a]">
          {[80, 30, 30, 30, 40].map((w, j) => <SkeletonPulse key={j} className="h-3" style={{ width: w }} />)}
        </div>
      ))}
    </div>
  );
}

function DashboardSkeleton() {
  return (
    <div className="h-full overflow-y-auto">
      <div className="px-6 py-6">
        {/* Header skeleton */}
        <div className="flex items-center justify-between mb-6">
          <div>
            <SkeletonPulse className="h-5 w-56 mb-2" />
            <SkeletonPulse className="h-3 w-72" />
          </div>
          <div className="flex gap-2">
            {[1,2,3,4,5].map(i => <SkeletonPulse key={i} className="h-7 w-10 rounded" />)}
          </div>
        </div>

        {/* Account section skeleton */}
        <div className="mb-6">
          <SkeletonPulse className="h-3 w-32 mb-3" />
          <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
            <div className="grid grid-cols-4 gap-4">
              {[1,2,3,4].map(i => (
                <div key={i}><SkeletonPulse className="h-3 w-16 mb-1.5" /><SkeletonPulse className="h-4 w-28" /></div>
              ))}
            </div>
          </div>
        </div>

        {/* Rate limits skeleton */}
        <div className="mb-6">
          <SkeletonPulse className="h-3 w-36 mb-3" />
          <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4 space-y-3">
            {[1,2,3].map(i => (
              <div key={i} className="flex items-center gap-3">
                <SkeletonPulse className="h-3 w-24" />
                <div className="flex-1"><SkeletonPulse className="h-2.5 w-full rounded-full" /></div>
                <SkeletonPulse className="h-3 w-12" />
              </div>
            ))}
          </div>
        </div>

        {/* Stat cards skeleton */}
        <div className="mb-6">
          <SkeletonPulse className="h-3 w-28 mb-3" />
          <SkeletonStatCards count={6} />
        </div>

        {/* Chart skeleton */}
        <div className="mb-6">
          <SkeletonPulse className="h-3 w-40 mb-3" />
          <SkeletonChart />
        </div>

        {/* Table skeleton */}
        <div className="mb-6">
          <SkeletonPulse className="h-3 w-32 mb-3" />
          <SkeletonTable rows={5} />
        </div>
      </div>
    </div>
  );
}

// ── Components ──────────────────────────────────────────────────────────────

function StatCard({ label, value, sub, color }: { label: string; value: string; sub?: string; color?: string }) {
  return (
    <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] px-4 py-3">
      <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">{label}</div>
      <div className={`text-xl font-semibold ${color || "text-text-primary"}`}>{value}</div>
      {sub && <div className="text-[11px] text-text-muted mt-0.5">{sub}</div>}
    </div>
  );
}

function Badge({ text, color = "bg-accent-blue/20 text-accent-blue" }: { text: string; color?: string }) {
  return <span className={`text-[9px] px-1.5 py-0.5 rounded font-medium ${color}`}>{text}</span>;
}

function Section({ title, children, defaultOpen = true }: { title: string; children: React.ReactNode; defaultOpen?: boolean }) {
  const storageKey = `claude-v2-${title}`;
  const [open, setOpen] = useState(() => {
    try { const s = localStorage.getItem(storageKey); return s !== "0"; } catch { return defaultOpen; }
  });
  const toggle = () => { const next = !open; setOpen(next); try { localStorage.setItem(storageKey, next ? "1" : "0"); } catch {} };
  return (
    <div className="mb-6">
      <button onClick={toggle} className="flex items-center gap-2 w-full text-left mb-3 group">
        <svg className={`w-3 h-3 text-text-muted transition-transform ${open ? "rotate-90" : ""}`} fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M9 5l7 7-7 7" />
        </svg>
        <h3 className="text-xs font-semibold uppercase tracking-wider text-text-muted group-hover:text-text-secondary">{title}</h3>
      </button>
      {open && children}
    </div>
  );
}

// (ActivityHeatmap replaced by ActivityCalendar + inline hourly bar chart)

// ── GitHub-Style Activity Heatmap (same as Legacy) ──────────────────────────

const HEATMAP_COLORS_CAL = ["#161b22", "#39d353", "#26a641", "#006d32", "#0e4429"];

function ActivityCalendar({ tokenTimeseries }: { tokenTimeseries: TokenTimePoint[] }) {
  // Build day → total tokens map from timeseries data
  const dayMap = new Map<string, number>();
  for (const tp of tokenTimeseries) {
    dayMap.set(tp.date, tp.input + tp.output + tp.cache_read);
  }

  // Build weeks grid: start from first Sunday before start of year, end today
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

  // Month labels — placed at the first week of each month
  const monthLabels: { month: string; weekIndex: number }[] = [];
  weeks.forEach((week, wi) => {
    if (!week.length) return;
    const mn = week[0].date.toLocaleDateString(undefined, { month: "short" });
    const prev = wi > 0 && weeks[wi - 1]?.[0] ? weeks[wi - 1][0].date.toLocaleDateString(undefined, { month: "short" }) : "";
    if (wi === 0 || prev !== mn) monthLabels.push({ month: mn, weekIndex: wi });
  });

  // Percentile-based 5-level intensity (same algorithm as Legacy)
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
      <p className="text-sm font-medium text-text-primary mb-3">Activity</p>
      <div
        className="w-full min-h-[100px]"
        style={{
          display: "grid",
          gridTemplateColumns: `repeat(${weekCount}, minmax(0, 1fr))`,
          gridTemplateRows: "auto repeat(7, minmax(0, 1fr))",
          gap: 1,
        }}
      >
        {/* Month labels row */}
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
        {/* Heatmap cells: 7 rows (Sun–Sat), weekCount columns */}
        {weeks.map((week, wi) =>
          week.map((day, di) => {
            const level = getLevel(day.value);
            const color = HEATMAP_COLORS_CAL[level];
            return (
              <div
                key={`${wi}-${di}`}
                className="rounded-sm border border-white/5 min-h-[10px] min-w-[8px]"
                style={{
                  backgroundColor: color,
                  gridColumn: wi + 1,
                  gridRow: di + 2,
                }}
                title={`${day.date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })} — ${day.value > 0 ? formatNum(day.value) + " tokens" : "No activity"}`}
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

// ── Main Inner Component ────────────────────────────────────────────────────

// ── Connection Flow Screen ──────────────────────────────────────────────────

function ConnectionFlow({ onConnected, onLocalOnly }: { onConnected: () => void; onLocalOnly: () => void }) {
  const [showKeychainWarning, setShowKeychainWarning] = useState(false);
  const [keychainLoading, setKeychainLoading] = useState(false);
  const [keychainError, setKeychainError] = useState<string | null>(null);
  const [showManualToken, setShowManualToken] = useState(false);
  const [manualToken, setManualToken] = useState("");
  const [tokenLoading, setTokenLoading] = useState(false);
  const [tokenError, setTokenError] = useState<string | null>(null);

  const handleKeychainImport = async () => {
    setKeychainLoading(true);
    setKeychainError(null);
    try {
      const result = await invoke<{ success: boolean; error: string | null }>("claude_import_from_keychain");
      if (result.success) {
        onConnected();
      } else {
        setKeychainError(result.error || "Failed to import from keychain");
      }
    } catch (err) {
      setKeychainError(String(err));
    } finally {
      setKeychainLoading(false);
      setShowKeychainWarning(false);
    }
  };

  return (
    <div className="flex items-center justify-center py-16">
      <div className="max-w-lg w-full px-4">
        <div className="text-center mb-8">
          <div className="text-3xl mb-3">&#9041;</div>
          <h2 className="text-base font-semibold text-text-primary mb-2">Connect to Claude Code</h2>
          <p className="text-xs text-text-muted max-w-sm mx-auto">
            Connect your account to see rate limits, usage analytics, model breakdown, and cost insights.
          </p>
        </div>

        <div className="space-y-3">
          {/* Option A: Import from Keychain (recommended) */}
          <div className="bg-[#1a1b23] rounded-lg border border-accent-blue/30 p-4">
            <div className="flex items-start gap-3">
              <div className="text-lg mt-0.5">&#128273;</div>
              <div className="flex-1">
                <div className="flex items-center gap-2 mb-1">
                  <h3 className="text-sm font-medium text-text-primary">Import from Keychain</h3>
                  <Badge text="RECOMMENDED" color="bg-accent-blue/20 text-accent-blue" />
                </div>
                <p className="text-[11px] text-text-muted mb-3">
                  If you've used Claude Code on this machine, your credentials are already stored securely
                  in macOS Keychain. This will trigger a one-time system password or Touch ID prompt.
                </p>
                {keychainError && <p className="text-[10px] text-red-400 mb-2">{keychainError}</p>}
                <button
                  onClick={() => setShowKeychainWarning(true)}
                  disabled={keychainLoading}
                  className="px-4 py-2 bg-accent-blue text-white rounded-lg text-xs font-medium hover:bg-accent-blue/90 transition-colors"
                >
                  {keychainLoading ? "Importing..." : "Import from Keychain \u2192"}
                </button>
              </div>
            </div>
          </div>

          {/* Option B: Sign In via Browser OAuth */}
          <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
            <div className="flex items-start gap-3">
              <div className="text-lg mt-0.5">&#127760;</div>
              <div className="flex-1">
                <h3 className="text-sm font-medium text-text-primary mb-1">Sign In to Claude</h3>
                {!showManualToken ? (
                  <>
                    <p className="text-[11px] text-text-muted mb-3">
                      Opens Claude's authorization page in your browser. After signing in, copy the code and paste it here.
                    </p>
                    <button
                      onClick={async () => {
                        setTokenError(null);
                        try {
                          const url = await invoke<string>("claude_start_oauth");
                          try {
                            const { openUrl } = await import("@tauri-apps/plugin-opener");
                            await openUrl(url);
                          } catch {
                            window.open(url, "_blank");
                          }
                          setShowManualToken(true);
                        } catch (err) {
                          setTokenError(String(err));
                        }
                      }}
                      className="px-4 py-2 bg-[#2a2b36] text-text-primary rounded-lg text-xs font-medium hover:bg-[#33344a] transition-colors"
                    >
                      Sign In with Claude &rarr;
                    </button>
                    {tokenError && <p className="text-[10px] text-red-400 mt-2">{tokenError}</p>}
                  </>
                ) : (
                  <div className="space-y-2">
                    <div className="bg-[#0e0f13] rounded-lg p-3 text-[10px] text-text-muted space-y-1">
                      <div className="flex items-start gap-1.5">
                        <span className="text-accent-blue font-bold">1.</span>
                        <span>Authorize AgentHarbor in the browser window that opened</span>
                      </div>
                      <div className="flex items-start gap-1.5">
                        <span className="text-accent-blue font-bold">2.</span>
                        <span>You'll see an <strong className="text-text-secondary">Authentication Code</strong> page</span>
                      </div>
                      <div className="flex items-start gap-1.5">
                        <span className="text-accent-blue font-bold">3.</span>
                        <span>Click <strong className="text-text-secondary">Copy Code</strong> and paste it below</span>
                      </div>
                    </div>
                    <input
                      type="text"
                      value={manualToken}
                      onChange={(e) => setManualToken(e.target.value)}
                      placeholder="Paste authentication code here..."
                      className="w-full bg-[#0e0f13] border border-[#2a2b36] rounded px-3 py-2 text-xs text-text-primary font-mono focus:outline-none focus:border-accent-blue"
                      autoFocus
                    />
                    {tokenError && <p className="text-[10px] text-red-400">{tokenError}</p>}
                    <div className="flex gap-2">
                      <button
                        onClick={async () => {
                          setTokenLoading(true);
                          setTokenError(null);
                          try {
                            await invoke("claude_exchange_oauth_code", { authCode: manualToken.trim() });
                            onConnected();
                          } catch (err) {
                            setTokenError(String(err));
                          } finally {
                            setTokenLoading(false);
                          }
                        }}
                        disabled={!manualToken.trim() || tokenLoading}
                        className="px-4 py-2 bg-accent-blue text-white rounded-lg text-xs font-medium hover:bg-accent-blue/90 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
                      >
                        {tokenLoading ? "Signing in..." : "Connect"}
                      </button>
                      <button onClick={() => { setShowManualToken(false); setTokenError(null); setManualToken(""); }} className="px-3 py-2 text-xs text-text-muted hover:text-text-primary">
                        Cancel
                      </button>
                    </div>
                  </div>
                )}
              </div>
            </div>
          </div>

          {/* Option C: Local Data Only */}
          <div className="text-center pt-2">
            <div className="flex items-center gap-3 mb-3">
              <div className="flex-1 h-px bg-[#2a2b36]" />
              <span className="text-[10px] text-text-muted uppercase">or</span>
              <div className="flex-1 h-px bg-[#2a2b36]" />
            </div>
            <button onClick={onLocalOnly} className="text-xs text-text-muted hover:text-text-primary transition-colors">
              &#128202; View Local Data Only
              <span className="block text-[10px] mt-0.5">Token usage, tool stats, and session data from local files</span>
            </button>
          </div>
        </div>

        {/* Keychain Warning Modal */}
        {showKeychainWarning && (
          <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50" onClick={() => setShowKeychainWarning(false)}>
            <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-xl p-6 max-w-md mx-4 shadow-2xl" onClick={e => e.stopPropagation()}>
              <div className="flex items-center gap-2 mb-3">
                <span className="text-amber-400 text-lg">&#9888;&#65039;</span>
                <h3 className="text-sm font-semibold text-text-primary">Keychain Access Required</h3>
              </div>
              <p className="text-xs text-text-muted mb-4 leading-relaxed">
                AgentHarbor needs to read Claude Code's credentials from your macOS Keychain.
              </p>
              <div className="bg-[#0e0f13] rounded-lg p-3 mb-4 text-[11px] text-text-muted space-y-1.5">
                <div className="flex items-start gap-2">
                  <span className="text-text-secondary mt-0.5">&bull;</span>
                  <span>macOS will show a system dialog asking for your <strong className="text-text-secondary">password or Touch ID</strong></span>
                </div>
                <div className="flex items-start gap-2">
                  <span className="text-text-secondary mt-0.5">&bull;</span>
                  <span>AgentHarbor will read the Claude Code OAuth token</span>
                </div>
                <div className="flex items-start gap-2">
                  <span className="text-text-secondary mt-0.5">&bull;</span>
                  <span>The token will be <strong className="text-text-secondary">saved locally</strong> so you won't be prompted again</span>
                </div>
              </div>
              <p className="text-[10px] text-text-muted mb-4">
                AgentHarbor does NOT modify any Claude Code files or settings.
              </p>
              <div className="flex justify-end gap-2">
                <button onClick={() => setShowKeychainWarning(false)} className="px-4 py-2 text-xs text-text-muted hover:text-text-primary transition-colors">
                  Cancel
                </button>
                <button
                  onClick={handleKeychainImport}
                  disabled={keychainLoading}
                  className="px-4 py-2 bg-amber-500/20 text-amber-400 rounded-lg text-xs font-medium hover:bg-amber-500/30 transition-colors"
                >
                  {keychainLoading ? "Reading Keychain..." : "Allow Keychain Access"}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

// ── Main Inner Component ────────────────────────────────────────────────────

type AuthState = "checking" | "not-connected" | "connected" | "local-only";

const CLAUDE_AUTH_KEY = "claude-analytics-authenticated";

function ClaudeAnalyticsV2Inner() {
  // Check if user previously authenticated — if so, try silent check; otherwise show Connect page
  const wasAuthenticated = localStorage.getItem(CLAUDE_AUTH_KEY) === "true";
  const [authState, setAuthState] = useState<AuthState>(wasAuthenticated ? "checking" : "not-connected");
  const [overview, setOverview] = useState<Overview | null>(null);
  const [tokenTs, setTokenTs] = useState<TokenTimePoint[]>([]);
  const [_modelTs, setModelTs] = useState<ModelTimePoint[]>([]);
  const [messageLog, setMessageLog] = useState<MessageLogPage | null>(null);
  const [timeRange, setTimeRange] = useState("30d");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [lastRefreshed, setLastRefreshed] = useState<string | null>(null);
  const [chartMode, setChartMode] = useState<"tokens" | "cost">("tokens");
  const [logPage, setLogPage] = useState(1);
  const [projectFilter, _setProjectFilter] = useState<string | null>(null);

  // Tick for "ago" display
  const [, setTick] = useState(0);
  useEffect(() => { const i = setInterval(() => setTick(t => t + 1), 30000); return () => clearInterval(i); }, []);

  // Silent credential check — only runs if user previously authenticated (avoids keychain prompt on first visit)
  useEffect(() => {
    if (!wasAuthenticated) return;
    (async () => {
      try {
        const result = await invoke<{ found: boolean; method: string }>("claude_check_silent_credentials");
        if (result.found) {
          setAuthState("connected");
        } else {
          // Token expired or gone — try local-only mode
          setAuthState("local-only");
        }
      } catch {
        // Silent check failed — show local data
        setAuthState("local-only");
      }
    })();
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const loadData = useCallback(async (range: string, force: boolean) => {
    setLoading(true);
    setError(null);
    try {
      const [o, ts, ms] = await Promise.all([
        invoke<Overview>("get_claude_v2_overview", { timeRange: range, forceRefresh: force }),
        invoke<TokenTimePoint[]>("get_claude_v2_token_timeseries", { timeRange: range, projectFilter, forceRefresh: force }),
        invoke<ModelTimePoint[]>("get_claude_v2_model_timeseries", { timeRange: range, forceRefresh: force }),
      ]);
      setOverview(o);
      setTokenTs(ts);
      setModelTs(ms);
      setLastRefreshed(new Date().toISOString());

      invoke<MessageLogPage>("get_claude_v2_message_log", { page: 1, pageSize: 50, projectFilter: null })
        .then(ml => { setMessageLog(ml); setLogPage(1); })
        .catch(() => {});
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  // Load data when auth state changes — force refresh to get fresh data with new credentials
  useEffect(() => {
    if (authState === "connected" || authState === "local-only") {
      loadData(timeRange, true);
    }
  }, [authState]); // eslint-disable-line react-hooks/exhaustive-deps

  // Auto-refresh every 5 minutes
  useEffect(() => {
    if (authState !== "connected" && authState !== "local-only") return;
    const i = setInterval(() => loadData(timeRange, true), 5 * 60 * 1000);
    return () => clearInterval(i);
  }, [loadData, timeRange, authState]);

  const loadMoreLog = useCallback(async () => {
    const next = logPage + 1;
    try {
      const ml = await invoke<MessageLogPage>("get_claude_v2_message_log", { page: next, pageSize: 50, projectFilter: null });
      setMessageLog(prev => prev ? { ...ml, entries: [...prev.entries, ...ml.entries] } : ml);
      setLogPage(next);
    } catch {}
  }, [logPage]);

  const handleExportCsv = useCallback(async () => {
    try {
      const csv = await invoke<string>("export_claude_v2_csv", { timeRange, projectFilter: null });
      const blob = new Blob([csv], { type: "text/csv" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = `claude-analytics-${timeRange}.csv`;
      a.click();
      URL.revokeObjectURL(url);
    } catch {}
  }, [timeRange]);

  const handleDisconnect = useCallback(async () => {
    await invoke("claude_disconnect").catch(() => {});
    setOverview(null);
    localStorage.removeItem(CLAUDE_AUTH_KEY);
    setAuthState("not-connected");
  }, []);

  // ── Auth state rendering ────────────────────────────────────────────

  if (authState === "checking") {
    return <DashboardSkeleton />;
  }

  if (authState === "not-connected") {
    return (
      <ConnectionFlow
        onConnected={() => {
          setOverview(null);
          localStorage.setItem(CLAUDE_AUTH_KEY, "true");
          setAuthState("connected");
        }}
        onLocalOnly={() => {
          localStorage.setItem(CLAUDE_AUTH_KEY, "true");
          setAuthState("local-only");
        }}
      />
    );
  }

  // Loading data after auth — show skeleton
  if (loading && !overview) {
    return <DashboardSkeleton />;
  }

  if (error && !overview) {
    return (
      <div className="p-6">
        <div className="bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-xs text-red-400">
          <p className="font-medium mb-1">Failed to load analytics</p>
          <p className="text-red-400/70">{error}</p>
          <div className="flex gap-2 mt-3">
            <button onClick={() => loadData(timeRange, true)} className="px-3 py-1.5 bg-red-500/20 rounded text-red-300 hover:bg-red-500/30">Retry</button>
            <button onClick={() => setAuthState("not-connected")} className="px-3 py-1.5 bg-[#2a2b36] rounded text-text-muted hover:text-text-primary">Reconnect</button>
          </div>
        </div>
      </div>
    );
  }

  if (!overview) return null;

  const isLocalOnly = authState === "local-only";

  // ── Derived data ────────────────────────────────────────────────────

  const trayFromExtra =
    timeRange === "today" ? getClaudeTrayIstTodayDisplay(overview.extra) : null;

  const displayEstimatedCost = trayFromExtra?.estimatedTotalCost ?? overview.estimated_total_cost;
  const displayTotalInput = trayFromExtra?.totalInputTokens ?? overview.total_input_tokens;
  const displayTotalOutput = trayFromExtra?.totalOutputTokens ?? overview.total_output_tokens;
  const displayTotalCacheRead = trayFromExtra?.totalCacheRead ?? overview.total_cache_read;
  const displayTotalCacheWrite = trayFromExtra?.totalCacheWrite ?? overview.total_cache_write;
  const displayTotalMessageCount = trayFromExtra?.totalMessageCount ?? overview.total_message_count;
  const totalTokens = trayFromExtra?.totalTokensSum
    ?? overview.total_input_tokens + overview.total_output_tokens + overview.total_cache_read + overview.total_cache_write;

  // Account info helpers — /api/oauth/account returns flat top-level fields
  const ai = overview.account_info as Record<string, any> | null;
  // Find the active team org (raven_type === "team") or fall back to first membership
  const memberships = ai?.memberships as any[] | undefined;
  const teamOrg = memberships?.find((m: any) => m?.organization?.raven_type === "team");
  const primaryOrg = teamOrg ?? memberships?.[0];

  // _modelTs available for future model timeseries chart

  return (
    <div className="h-full overflow-y-auto relative">
      {/* Top loading bar — visible during any data refresh */}
      {loading && (
        <div className="sticky top-0 z-50 w-full">
          <div className="h-0.5 bg-[#0e0f13] w-full overflow-hidden">
            <div className="h-full bg-accent-blue animate-[loading-bar_1.5s_ease-in-out_infinite] w-1/3 rounded-full" />
          </div>
          <style>{`@keyframes loading-bar { 0% { transform: translateX(-100%); } 100% { transform: translateX(400%); } }`}</style>
        </div>
      )}

      <div className="px-6 py-6">
        {/* Header */}
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-lg font-semibold text-text-primary flex items-center gap-2">
              Claude Code Analytics
              {loading && <span className="inline-block w-2 h-2 rounded-full bg-accent-blue animate-pulse" title="Refreshing..." />}
            </h1>
            <p className="text-xs text-text-muted">
              {overview.email ?? "Local data"}
              {overview.plan && <span> &middot; <Badge text={overview.plan.toUpperCase()} color="bg-purple-500/20 text-purple-400" /></span>}
              {overview.org_name && <span> &middot; {overview.org_name}</span>}
              {overview.connected && <span className="text-emerald-500"> &middot; {overview.connection_method === "oauth-auto" || overview.connection_method === "credentials-file" ? "Auto-detected" : overview.connection_method === "stored" ? "Connected" : "Connected"}</span>}
              {isLocalOnly && <span className="text-amber-400"> &middot; Local data only</span>}
              {overview.active_session_count > 0 && (
                <span className="ml-2">
                  <span className="inline-block w-1.5 h-1.5 rounded-full bg-emerald-400 animate-pulse mr-1" />
                  <span className="text-emerald-400">{overview.active_session_count} active</span>
                </span>
              )}
            </p>
          </div>
          <div className="flex items-center gap-2">
            {[
              { key: "5h", label: "5h" },
              { key: "today", label: "Today" },
              { key: "week", label: "This Week" },
              { key: "month", label: "This Month" },
              { key: "7d", label: "7d" },
              { key: "30d", label: "30d" },
              { key: "all", label: "all" },
            ].map(r => (
              <button
                key={r.key}
                onClick={() => { setTimeRange(r.key); loadData(r.key, false); }}
                className={`px-2.5 py-1 rounded text-[11px] font-medium transition-colors ${timeRange === r.key ? "bg-accent-blue text-white" : "bg-[#1a1b23] text-text-secondary hover:bg-[#22232e]"}`}
              >{r.label}</button>
            ))}
            {loading ? (
              <span className="text-[10px] text-accent-blue font-medium flex items-center gap-1">
                <svg className="w-3 h-3 animate-spin" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
                </svg>
                Updating...
              </span>
            ) : lastRefreshed ? (
              <span className="text-[10px] text-text-muted">Updated {timeAgo(lastRefreshed)}</span>
            ) : null}
            <button onClick={() => loadData(timeRange, true)} className={`p-1.5 rounded transition-colors ${loading ? "text-accent-blue bg-accent-blue/10" : "text-text-muted hover:text-text-primary hover:bg-[#1a1b23]"}`} title="Force refresh">
              <svg className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`} fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
            </button>
            {isLocalOnly && (
              <button onClick={() => setAuthState("not-connected")} className="px-2.5 py-1 rounded text-[11px] font-medium bg-accent-blue/20 text-accent-blue hover:bg-accent-blue/30 transition-colors">
                Connect
              </button>
            )}
            {!isLocalOnly && overview.connected && (
              <button onClick={handleDisconnect} className="p-1.5 rounded text-text-muted hover:text-red-400 hover:bg-red-500/10" title="Disconnect">
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M17 16l4-4m0 0l-4-4m4 4H7m6 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h4a3 3 0 013 3v1" />
                </svg>
              </button>
            )}
          </div>
        </div>

        {/* Data sections — fade during refresh */}
        <div className={`transition-opacity duration-300 ${loading ? "opacity-60" : "opacity-100"}`}>

        {/* ── Section 1: Account & Billing (API required) ─────────────── */}
        {!isLocalOnly && overview.connected && (
        <Section title="Account & Billing">
          <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-xs">
              <div>
                <span className="text-text-muted block mb-0.5">Email</span>
                <div className="text-text-primary font-medium truncate">{ai?.email_address ?? overview.email ?? "-"}</div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Name</span>
                <div className="text-text-primary font-medium">{ai?.full_name ?? ai?.display_name ?? "-"}</div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Member Since</span>
                <div className="text-text-primary font-medium">
                  {(() => {
                    const created = ai?.created_at ?? (overview.extra as any)?.account_created_at;
                    return created ? new Date(created).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" }) : "-";
                  })()}
                </div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Account ID</span>
                <div className="text-text-primary font-mono text-[10px] truncate">{ai?.uuid ?? "-"}</div>
              </div>
            </div>

            <div className="border-t border-[#2a2b36] mt-3 pt-3 grid grid-cols-2 md:grid-cols-4 gap-4 text-xs">
              <div>
                <span className="text-text-muted block mb-0.5">Plan</span>
                <div className="flex items-center gap-1.5">
                  {(() => {
                    // Derive plan from org raven_type or rate_limit_tier
                    const ravenType = primaryOrg?.organization?.raven_type;
                    const tier = primaryOrg?.organization?.rate_limit_tier ?? (overview.extra as any)?.rate_limit_tier ?? "";
                    const planStr = overview.plan;
                    let label = "Free";
                    let color = "bg-[#2a2b36] text-text-muted";
                    if (ravenType === "team") { label = "Team"; color = "bg-emerald-500/20 text-emerald-400"; }
                    else if (tier.includes("max")) { label = "Max"; color = "bg-purple-500/20 text-purple-400"; }
                    else if (tier.includes("pro")) { label = "Pro"; color = "bg-blue-500/20 text-blue-400"; }
                    else if (planStr) { label = planStr; color = "bg-blue-500/20 text-blue-400"; }
                    return <Badge text={label.toUpperCase()} color={color} />;
                  })()}
                  {primaryOrg?.seat_tier && <Badge text={String(primaryOrg.seat_tier).replace(/_/g, " ")} color="bg-[#2a2b36] text-text-muted" />}
                </div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Organization</span>
                <div className="text-text-primary font-medium">{primaryOrg?.organization?.name ?? overview.org_name ?? "-"}</div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Role</span>
                <div className="text-text-primary font-medium capitalize">{primaryOrg?.member_role ?? "User"}</div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Subscription</span>
                <div className="flex items-center gap-1.5">
                  {(() => {
                    const status = (overview.extra as any)?.subscription_status;
                    const billing = primaryOrg?.organization?.billing_type;
                    if (status === "active") return <Badge text="ACTIVE" color="bg-emerald-500/20 text-emerald-400" />;
                    if (billing) return <Badge text={String(billing).replace(/_/g, " ").toUpperCase()} color="bg-blue-500/20 text-blue-400" />;
                    return <span className="text-text-muted">-</span>;
                  })()}
                </div>
              </div>
            </div>

            <div className="border-t border-[#2a2b36] mt-3 pt-3 grid grid-cols-2 md:grid-cols-4 gap-4 text-xs">
              <div>
                <span className="text-text-muted block mb-0.5">Rate Limit Tier</span>
                <div className="text-text-primary font-medium text-[10px] font-mono">
                  {(() => {
                    const tier = primaryOrg?.organization?.rate_limit_tier ?? (overview.extra as any)?.rate_limit_tier;
                    if (!tier) return "-";
                    return String(tier).replace(/_/g, " ").replace(/^default /i, "");
                  })()}
                </div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Extra Usage</span>
                <div className="flex items-center gap-1.5">
                  {overview.credit_usage ? (
                    <Badge text="ENABLED" color="bg-emerald-500/20 text-emerald-400" />
                  ) : (
                    <Badge text="DISABLED" color="bg-[#2a2b36] text-text-muted" />
                  )}
                </div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">API-Equiv. Value</span>
                <div className="text-emerald-400 font-semibold">
                  {formatUsd(displayEstimatedCost)}
                  <span className="text-[9px] text-text-muted font-normal ml-1">({timeRange})</span>
                </div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Billing Type</span>
                <div className="text-text-primary font-medium capitalize">
                  {(() => {
                    const bt = primaryOrg?.organization?.billing_type;
                    return bt ? String(bt).replace(/_/g, " ") : "-";
                  })()}
                </div>
              </div>
            </div>

            {/* Org capabilities & verification */}
            {(primaryOrg?.organization?.capabilities || ai?.is_verified != null) && (
              <div className="border-t border-[#2a2b36] mt-3 pt-3 flex items-center gap-3 text-xs flex-wrap">
                {Array.isArray(primaryOrg?.organization?.capabilities) && primaryOrg.organization.capabilities.map((cap: string, i: number) => (
                  <Badge key={i} text={cap.toUpperCase()} color="bg-[#2a2b36] text-text-muted" />
                ))}
                {ai?.is_verified && <Badge text="VERIFIED" color="bg-emerald-500/20 text-emerald-400" />}
                {memberships && memberships.length > 1 && (
                  <span className="text-text-muted text-[10px]">{memberships.length} organizations</span>
                )}
              </div>
            )}
          </div>
        </Section>
        )}

        {/* ── Section 2: Your Usage Limits (API required) ──────────────── */}
        {!isLocalOnly && overview.rate_limits.length > 0 && (
          <Section title="Your Usage Limits">
            <div className="space-y-4">
              {/* Session limits */}
              {overview.rate_limits.filter(rl => rl.label.toLowerCase().includes("session") || rl.label.toLowerCase().includes("5h")).length > 0 && (
                <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
                  <h4 className="text-xs font-semibold text-text-primary mb-3">Current Session</h4>
                  {overview.rate_limits
                    .filter(rl => rl.label.toLowerCase().includes("session") || rl.label.toLowerCase().includes("5h"))
                    .map((rl, i) => (
                      <div key={i} className="mb-3 last:mb-0">
                        <div className="flex justify-between text-xs mb-1">
                          <div>
                            <span className="text-text-secondary">{rl.label}</span>
                            <span className="text-text-muted text-[10px] ml-2">Resets in {resetCountdown(rl.resets_at)}</span>
                          </div>
                        </div>
                        <div className="flex items-center gap-2">
                          <div className="flex-1 h-2.5 bg-[#0e0f13] rounded-full overflow-hidden flex">
                            <div
                              className="h-full rounded-l-full transition-all duration-700 bg-red-500"
                              style={{ width: `${Math.min(rl.used_percent, 100)}%` }}
                            />
                            <div
                              className="h-full rounded-r-full transition-all duration-700 bg-emerald-500"
                              style={{ width: `${Math.max(100 - rl.used_percent, 0)}%` }}
                            />
                          </div>
                          <span className="text-red-400 font-semibold text-xs whitespace-nowrap">
                            {rl.used_percent.toFixed(0)}% Used
                          </span>
                        </div>
                      </div>
                    ))}
                </div>
              )}

              {/* Weekly limits */}
              {overview.rate_limits.filter(rl => !rl.label.toLowerCase().includes("session") && !rl.label.toLowerCase().includes("5h")).length > 0 && (
                <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
                  <h4 className="text-xs font-semibold text-text-primary mb-3">Weekly Limits</h4>
                  <div className="space-y-3">
                    {overview.rate_limits
                      .filter(rl => !rl.label.toLowerCase().includes("session") && !rl.label.toLowerCase().includes("5h"))
                      .map((rl, i) => (
                        <div key={i}>
                          <div className="flex justify-between text-xs mb-1">
                            <div>
                              <span className="text-text-secondary">{rl.label}</span>
                              <span className="text-text-muted text-[10px] ml-2">
                                Resets {rl.resets_at ? new Date(rl.resets_at).toLocaleDateString("en-US", { weekday: "short", hour: "numeric", minute: "2-digit" }) : "—"}
                              </span>
                            </div>
                          </div>
                          <div className="flex items-center gap-2">
                            <div className="flex-1 h-2.5 bg-[#0e0f13] rounded-full overflow-hidden flex">
                              <div
                                className="h-full rounded-l-full transition-all duration-700 bg-red-500"
                                style={{ width: `${Math.min(rl.used_percent, 100)}%` }}
                              />
                              <div
                                className="h-full rounded-r-full transition-all duration-700 bg-emerald-500"
                                style={{ width: `${Math.max(100 - rl.used_percent, 0)}%` }}
                              />
                            </div>
                            <span className="text-red-400 font-semibold text-xs whitespace-nowrap">
                              {rl.used_percent.toFixed(0)}% Used
                            </span>
                          </div>
                        </div>
                      ))}
                  </div>
                </div>
              )}

              {/* Extra usage credits */}
              {overview.credit_usage && (
                <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
                  <div className="flex items-center justify-between mb-3">
                    <h4 className="text-xs font-semibold text-text-primary">Extra Usage Credits</h4>
                    <Badge text="ENABLED" color="bg-emerald-500/20 text-emerald-400" />
                  </div>
                  <div className="flex justify-between text-xs mb-1">
                    <span className="text-text-muted">Used</span>
                    <span className="text-text-primary font-semibold">
                      {formatUsd(overview.credit_usage.used)} {overview.credit_usage.limit != null && `/ ${formatUsd(overview.credit_usage.limit)}`}
                    </span>
                  </div>
                  <div className="h-2.5 bg-[#0e0f13] rounded-full overflow-hidden">
                    <div
                      className="h-full bg-amber-500 rounded-full transition-all"
                      style={{ width: `${overview.credit_usage.limit ? Math.min((overview.credit_usage.used / overview.credit_usage.limit) * 100, 100) : 0}%` }}
                    />
                  </div>
                  <div className="text-[10px] text-text-muted mt-1.5">
                    {formatUsd(overview.credit_usage.remaining)} remaining
                    {!overview.credit_usage.limit && <span className="ml-1"><Badge text="NO LIMIT" color="bg-purple-500/20 text-purple-400" /></span>}
                  </div>
                </div>
              )}
            </div>
          </Section>
        )}

        {/* Local-only banner */}
        {isLocalOnly && (
          <div className="bg-amber-500/10 border border-amber-500/20 rounded-lg px-4 py-3 mb-6 flex items-center justify-between">
            <div className="text-xs text-amber-400">
              <span className="font-medium">Local data only</span> — Rate limits, account info, and usage limits require connecting to Claude's API.
            </div>
            <button onClick={() => setAuthState("not-connected")} className="px-3 py-1 bg-amber-500/20 text-amber-400 rounded text-[11px] font-medium hover:bg-amber-500/30 transition-colors shrink-0 ml-3">
              Connect Now
            </button>
          </div>
        )}

        {/* ── Section 2.5: Usage Stats ─────────────────────────────────── */}
        <Section title="Usage Stats">
          <div className="grid grid-cols-2 md:grid-cols-2 gap-3">
            {/* Favorite Model */}
            <StatCard
              label="Favorite Model"
              value={overview.favorite_model ?? "--"}
              sub="Most-used model"
            />
            {/* Total Tokens */}
            <StatCard
              label="Total Tokens"
              value={formatNum(totalTokens)}
              sub={`${(totalTokens).toLocaleString()} exact`}
            />
            {/* Sessions */}
            <StatCard
              label="Sessions"
              value={formatNum(overview.total_sessions)}
              sub="All time"
            />
            {/* Longest Session */}
            <StatCard
              label="Longest Session"
              value={(() => {
                const dur = overview.longest_session_duration;
                if (!dur) return "--";
                const s = Math.floor(dur / 1000);
                const d = Math.floor(s / 86400);
                const h = Math.floor((s % 86400) / 3600);
                const m = Math.floor((s % 3600) / 60);
                if (d > 0) return `${d}d ${h}h ${m}m`;
                if (h > 0) return `${h}h ${m}m`;
                return `${m}m`;
              })()}
              sub={`${overview.longest_session_messages} messages`}
            />
            {/* Active Days */}
            <StatCard
              label="Active Days"
              value={overview.total_days > 0 ? `${overview.active_days}/${overview.total_days}` : `${overview.active_days}`}
              sub={overview.total_days > 0 ? `${Math.round((overview.active_days / overview.total_days) * 100)}% of days active` : "days with activity"}
            />
            {/* Longest Streak */}
            <StatCard
              label="Longest Streak"
              value={overview.longest_streak > 0 ? `${overview.longest_streak} day${overview.longest_streak !== 1 ? "s" : ""}` : "--"}
              sub="Consecutive active days"
            />
            {/* Current Streak */}
            <StatCard
              label="Current Streak"
              value={overview.current_streak > 0 ? `${overview.current_streak} day${overview.current_streak !== 1 ? "s" : ""}` : "--"}
              sub="Ongoing streak"
            />
            {/* Peak Hour */}
            <StatCard
              label="Peak Hour"
              value={overview.peak_hour != null ? `${overview.peak_hour}:00 - ${overview.peak_hour + 1}:00` : "--"}
              sub="Most active hour of day"
            />
            {/* Most Active Day */}
            <StatCard
              label="Most Active Day"
              value={overview.most_active_weekday ?? "--"}
              sub="Busiest day of the week"
            />
          </div>
        </Section>

        {/* ── Section 3: Cost Analysis ──────────────────────────────────── */}
        <Section title="Cost Analysis">
          {/* ROI Summary Banner */}
          {(() => {
            const apiEquiv = displayEstimatedCost;
            const costPerMsg = displayTotalMessageCount > 0 ? apiEquiv / displayTotalMessageCount : 0;
            const totalTokensAll = totalTokens;
            // Cost breakdown by type — "today" uses tray `start_today_*_cost` when available
            let inputCost = 0;
            let outputCost = 0;
            let cacheReadCost = 0;
            let cacheWriteCost = 0;
            if (trayFromExtra) {
              inputCost = trayFromExtra.inputCost;
              outputCost = trayFromExtra.outputCost;
              cacheReadCost = trayFromExtra.cacheReadCost;
              cacheWriteCost = trayFromExtra.cacheWriteCost;
            } else {
              overview.model_breakdown.forEach(m => {
                const ml = m.model.toLowerCase();
                const pi = ml.includes("opus") ? 15 : ml.includes("haiku") ? 0.8 : 3;
                const po = ml.includes("opus") ? 75 : ml.includes("haiku") ? 4 : 15;
                const pr = ml.includes("opus") ? 1.5 : ml.includes("haiku") ? 0.08 : 0.3;
                const pw = ml.includes("opus") ? 18.75 : ml.includes("haiku") ? 1 : 3.75;
                inputCost += (m.input_tokens / 1_000_000) * pi;
                outputCost += (m.output_tokens / 1_000_000) * po;
                cacheReadCost += (m.cache_read_tokens / 1_000_000) * pr;
                cacheWriteCost += (m.cache_write_tokens / 1_000_000) * pw;
              });
            }

            return (
              <>
                {/* ROI insight card */}
                <div className="bg-gradient-to-r from-[#1a1b23] to-[#1e1f2a] rounded-lg border border-[#2a2b36] p-4 mb-3">
                  <div className="flex items-start justify-between mb-3">
                    <div>
                      <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">API-Equivalent Value</div>
                      <div className="text-2xl font-bold text-emerald-400">{formatUsd(apiEquiv)}</div>
                      <div className="text-[10px] text-text-muted mt-0.5">What this usage would cost at pay-per-token API rates ({timeRange})</div>
                    </div>
                    <div className="text-right">
                      <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">Per Message</div>
                      <div className="text-lg font-semibold text-text-primary">{formatUsd(costPerMsg)}</div>
                      <div className="text-[10px] text-text-muted mt-0.5">avg cost at API rates</div>
                    </div>
                  </div>
                  <div className="bg-[#0e0f13] rounded-lg p-3 text-[10px] text-text-muted leading-relaxed">
                    <span className="text-amber-400 font-medium">How to read this:</span> Your Team/Max subscription includes all this usage in your flat monthly fee.
                    The "{formatUsd(apiEquiv)}" represents what equivalent API usage would cost without a subscription — it shows the
                    <span className="text-emerald-400 font-medium"> compute value</span> you&apos;re getting from your plan, not what you&apos;re being charged.
                  </div>
                </div>

                {/* Cost breakdown cards */}
                <div className="grid grid-cols-2 md:grid-cols-5 gap-3 mb-3">
                  <StatCard label="Input Tokens" value={formatUsd(inputCost)} sub={formatNum(displayTotalInput)} />
                  <StatCard label="Output Tokens" value={formatUsd(outputCost)} sub={formatNum(displayTotalOutput)} />
                  <StatCard label="Cache Reads" value={formatUsd(cacheReadCost)} sub={formatNum(displayTotalCacheRead)} color="text-emerald-400" />
                  <StatCard label="Cache Writes" value={formatUsd(cacheWriteCost)} sub={formatNum(displayTotalCacheWrite)} color="text-amber-400" />
                  <StatCard label="Total Tokens" value={formatNum(totalTokensAll)} sub={`${displayTotalMessageCount.toLocaleString()} messages`} />
                </div>

                {/* Per-model table */}
                {trayFromExtra && (
                  <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-hidden mb-3">
                    <div className="px-3 py-2 text-[9px] text-text-muted border-b border-[#2a2b36]">
                      <Badge text="MENU BAR SYNC" color="bg-emerald-500/20 text-emerald-400" />
                      <span className="ml-2">
                        {`Totals match the tray "Today" row (since midnight IST, Asia/Kolkata). Per-model rows hidden — local JSONL parsing can differ from that scan.`}
                      </span>
                    </div>
                    <table className="w-full text-xs">
                      <thead><tr className="border-b border-[#2a2b36] text-text-muted">
                        <th className="text-left px-3 py-2">Scope</th>
                        <th className="text-right px-3 py-2">Messages</th>
                        <th className="text-right px-3 py-2">Input</th>
                        <th className="text-right px-3 py-2">Output</th>
                        <th className="text-right px-3 py-2">Cache R</th>
                        <th className="text-right px-3 py-2">Cache W</th>
                        <th className="text-right px-3 py-2">API-Equiv.</th>
                      </tr></thead>
                      <tbody>
                        <tr className="bg-[#22232e]">
                          <td className="px-3 py-2 text-text-primary font-semibold">
                            Today IST (tray)
                          </td>
                          <td className="px-3 py-2 text-right text-text-primary font-mono font-semibold">{formatNum(displayTotalMessageCount)}</td>
                          <td className="px-3 py-2 text-right text-text-primary font-mono">{formatNum(displayTotalInput)}</td>
                          <td className="px-3 py-2 text-right text-text-primary font-mono">{formatNum(displayTotalOutput)}</td>
                          <td className="px-3 py-2 text-right text-emerald-400 font-mono">{formatNum(displayTotalCacheRead)}</td>
                          <td className="px-3 py-2 text-right text-amber-400 font-mono">{formatNum(displayTotalCacheWrite)}</td>
                          <td className="px-3 py-2 text-right text-emerald-400 font-mono font-semibold">{formatUsd(displayEstimatedCost)}</td>
                        </tr>
                      </tbody>
                    </table>
                  </div>
                )}
                {!trayFromExtra && overview.model_breakdown.length > 0 && (
                  <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-hidden">
                    <table className="w-full text-xs">
                      <thead><tr className="border-b border-[#2a2b36] text-text-muted">
                        <th className="text-left px-3 py-2">Model</th>
                        <th className="text-right px-3 py-2">Messages</th>
                        <th className="text-right px-3 py-2">Input</th>
                        <th className="text-right px-3 py-2">Output</th>
                        <th className="text-right px-3 py-2">Cache R</th>
                        <th className="text-right px-3 py-2">Cache W</th>
                        <th className="text-right px-3 py-2">API-Equiv.</th>
                      </tr></thead>
                      <tbody>
                        {overview.model_breakdown.map((m, i) => (
                          <tr key={i} className="border-b border-[#1e1f2a] hover:bg-[#22232e]">
                            <td className="px-3 py-2 text-text-primary font-medium">{m.model}</td>
                            <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(m.message_count)}</td>
                            <td className="px-3 py-2 text-right text-text-muted font-mono">{formatNum(m.input_tokens)}</td>
                            <td className="px-3 py-2 text-right text-text-muted font-mono">{formatNum(m.output_tokens)}</td>
                            <td className="px-3 py-2 text-right text-emerald-400 font-mono">{formatNum(m.cache_read_tokens)}</td>
                            <td className="px-3 py-2 text-right text-amber-400 font-mono">{formatNum(m.cache_write_tokens)}</td>
                            <td className="px-3 py-2 text-right text-text-primary font-mono font-semibold">{formatUsd(m.estimated_cost_usd)}</td>
                          </tr>
                        ))}
                        <tr className="bg-[#22232e]">
                          <td className="px-3 py-2 text-text-primary font-semibold">Total</td>
                          <td className="px-3 py-2 text-right text-text-primary font-mono font-semibold">{formatNum(overview.total_message_count)}</td>
                          <td className="px-3 py-2 text-right text-text-primary font-mono">{formatNum(overview.total_input_tokens)}</td>
                          <td className="px-3 py-2 text-right text-text-primary font-mono">{formatNum(overview.total_output_tokens)}</td>
                          <td className="px-3 py-2 text-right text-emerald-400 font-mono">{formatNum(overview.total_cache_read)}</td>
                          <td className="px-3 py-2 text-right text-amber-400 font-mono">{formatNum(overview.total_cache_write)}</td>
                          <td className="px-3 py-2 text-right text-emerald-400 font-mono font-semibold">{formatUsd(overview.estimated_total_cost)}</td>
                        </tr>
                      </tbody>
                    </table>

                    {/* Pricing reference */}
                    <div className="border-t border-[#2a2b36] px-3 py-2">
                      <div className="text-[9px] text-text-muted mb-1.5">
                        <Badge text="API REFERENCE RATES" color="bg-blue-500/20 text-blue-400" /> Published per-token pricing used for estimation:
                      </div>
                      <div className="flex flex-wrap gap-x-4 gap-y-1 text-[9px] text-text-muted font-mono">
                        <span><span className="text-purple-400">Opus:</span> $15/$75 in/out &middot; $1.50/$18.75 cache r/w per Mtok</span>
                        <span><span className="text-blue-400">Sonnet:</span> $3/$15 &middot; $0.30/$3.75</span>
                        <span><span className="text-cyan-400">Haiku:</span> $0.80/$4 &middot; $0.08/$1</span>
                      </div>
                    </div>

                    <div className="border-t border-[#2a2b36] px-3 py-1.5 text-[9px] text-text-muted flex items-center justify-between">
                      <span><Badge text="NOTE" color="bg-amber-500/20 text-amber-400" /> These are API-equivalent costs for reference. Subscription plans (Pro/Max/Team) include usage in the flat monthly fee — you are <span className="text-emerald-400">not</span> billed per token.</span>
                      <a
                        href="https://platform.claude.com/docs/en/about-claude/pricing"
                        target="_blank"
                        rel="noopener noreferrer"
                        className="text-xs text-text-muted hover:text-blue-400 transition-colors inline-flex items-center gap-1 shrink-0 ml-3"
                        onClick={async (e) => {
                          e.preventDefault();
                          const { openUrl } = await import("@tauri-apps/plugin-opener");
                          await openUrl("https://platform.claude.com/docs/en/about-claude/pricing");
                        }}
                      >
                        View pricing
                        <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                          <path strokeLinecap="round" strokeLinejoin="round" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
                        </svg>
                      </a>
                    </div>
                  </div>
                )}
              </>
            );
          })()}
        </Section>

        {/* Section 4: Session Overview */}
        <Section title="Session Overview">
          <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-3">
            <StatCard label="Sessions" value={`${overview.total_sessions}`} sub={overview.first_session_date ? `Since ${new Date(overview.first_session_date).toLocaleDateString("en-US", { month: "short", year: "numeric" })}` : undefined} />
            <StatCard label="Messages" value={formatNum(overview.total_messages)} />
            <StatCard label="Tool Calls" value={formatNum(overview.total_tool_calls)} />
            <StatCard label="Longest Session" value={formatDuration(overview.longest_session_duration)} sub={`${overview.longest_session_messages} messages`} />
            <StatCard label="Startups" value={`${overview.num_startups}`} sub={overview.install_method ?? undefined} />
            <StatCard label="Active Now" value={`${overview.active_session_count}`} color={overview.active_session_count > 0 ? "text-emerald-400" : undefined} />
          </div>
        </Section>

        {/* Section 4: Token Usage & Cost */}
        <Section title={`Token Usage (${formatNum(totalTokens)} total · ${formatUsd(displayEstimatedCost)} est.)`}>
          <div className="grid grid-cols-2 md:grid-cols-5 gap-3 mb-3">
            <StatCard label="Total Tokens" value={formatNum(totalTokens)} sub={`${displayTotalMessageCount.toLocaleString()} messages`} />
            <StatCard label="Input" value={formatNum(displayTotalInput)} />
            <StatCard label="Output" value={formatNum(displayTotalOutput)} />
            <StatCard label="Cache Read" value={formatNum(displayTotalCacheRead)} color="text-emerald-400" />
            <StatCard label="Cache Write" value={formatNum(displayTotalCacheWrite)} color="text-amber-400" />
          </div>
          {tokenTs.length > 0 && (
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
              {trayFromExtra && (
                <p className="text-[9px] text-text-muted mb-2">
                  Chart data is built from local project logs and may not match the menu bar totals above.
                </p>
              )}
              <div className="flex justify-end gap-2 mb-2">
                <button onClick={() => setChartMode("tokens")} className={`text-[10px] px-2 py-0.5 rounded ${chartMode === "tokens" ? "bg-accent-blue text-white" : "text-text-muted"}`}>Tokens</button>
                <button onClick={() => setChartMode("cost")} className={`text-[10px] px-2 py-0.5 rounded ${chartMode === "cost" ? "bg-accent-blue text-white" : "text-text-muted"}`}>Cost $</button>
              </div>
              <ResponsiveContainer width="100%" height={250}>
                {chartMode === "tokens" ? (
                  <AreaChart data={tokenTs}>
                    <CartesianGrid strokeDasharray="3 3" stroke="#2a2b36" />
                    <XAxis dataKey="date" tick={{ fontSize: 10, fill: "#9394a1" }} tickFormatter={v => v.slice(5)} />
                    <YAxis tick={{ fontSize: 10, fill: "#9394a1" }} tickFormatter={v => formatNum(v)} />
                    <Tooltip contentStyle={TOOLTIP_STYLE} />
                    <Legend wrapperStyle={{ fontSize: "10px" }} />
                    <Area type="monotone" dataKey="cache_read" stackId="1" stroke="#22c55e" fill="#22c55e" fillOpacity={0.3} name="Cache Read" />
                    <Area type="monotone" dataKey="cache_write" stackId="1" stroke="#f59e0b" fill="#f59e0b" fillOpacity={0.3} name="Cache Write" />
                    <Area type="monotone" dataKey="input" stackId="1" stroke="#3b82f6" fill="#3b82f6" fillOpacity={0.3} name="Input" />
                    <Area type="monotone" dataKey="output" stackId="1" stroke="#8b5cf6" fill="#8b5cf6" fillOpacity={0.3} name="Output" />
                  </AreaChart>
                ) : (
                  <AreaChart data={tokenTs}>
                    <CartesianGrid strokeDasharray="3 3" stroke="#2a2b36" />
                    <XAxis dataKey="date" tick={{ fontSize: 10, fill: "#9394a1" }} tickFormatter={v => v.slice(5)} />
                    <YAxis tick={{ fontSize: 10, fill: "#9394a1" }} tickFormatter={v => `$${v.toFixed(2)}`} />
                    <Tooltip contentStyle={TOOLTIP_STYLE} formatter={(v: number) => `$${v.toFixed(4)}`} />
                    <Area type="monotone" dataKey="estimated_cost" stroke="#22c55e" fill="#22c55e" fillOpacity={0.3} name="Estimated Cost" />
                  </AreaChart>
                )}
              </ResponsiveContainer>
            </div>
          )}
        </Section>

        {/* Section 5: Model Breakdown */}
        {!trayFromExtra && overview.model_breakdown.length > 0 && (
          <Section title="Model Breakdown">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
                <ResponsiveContainer width="100%" height={300}>
                  <PieChart>
                    <Pie data={overview.model_breakdown.map(m => ({ name: m.model, value: m.message_count }))} dataKey="value" nameKey="name" cx="50%" cy="45%" innerRadius="45%" outerRadius="80%" paddingAngle={2}>
                      {overview.model_breakdown.map((_, i) => <Cell key={i} fill={COLORS[i % COLORS.length]} />)}
                    </Pie>
                    <Tooltip contentStyle={TOOLTIP_STYLE} />
                    <Legend wrapperStyle={{ fontSize: "10px" }} formatter={v => <span className="text-text-secondary">{v}</span>} />
                  </PieChart>
                </ResponsiveContainer>
              </div>
              <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-hidden">
                <table className="w-full text-xs">
                  <thead><tr className="border-b border-[#2a2b36] text-text-muted">
                    <th className="text-left px-3 py-2">Model</th>
                    <th className="text-right px-3 py-2">Msgs</th>
                    <th className="text-right px-3 py-2">Tokens</th>
                    <th className="text-right px-3 py-2">Cost</th>
                  </tr></thead>
                  <tbody>
                    {overview.model_breakdown.map((m, i) => (
                      <tr key={i} className="border-b border-[#1e1f2a] hover:bg-[#22232e]">
                        <td className="px-3 py-2 text-text-primary"><div className="flex items-center gap-1.5"><div className="w-2 h-2 rounded-full" style={{ background: COLORS[i % COLORS.length] }} />{m.model}</div></td>
                        <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(m.message_count)}</td>
                        <td className="px-3 py-2 text-right text-text-muted font-mono">{formatNum(m.input_tokens + m.output_tokens)}</td>
                        <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatUsd(m.estimated_cost_usd)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          </Section>
        )}



        {/* Section 6: Tool Usage */}
        {overview.tool_usage.length > 0 && (
          <Section title="Tool Usage">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-3 mb-3">
              <StatCard label="Web Searches" value={formatNum(
                overview.total_web_searches || overview.tool_usage.find(t => t.tool_name === "WebSearch")?.count || 0
              )} />
              <StatCard label="Web Fetches" value={formatNum(
                overview.total_web_fetches || overview.tool_usage.find(t => t.tool_name === "WebFetch")?.count || 0
              )} />
              <StatCard label="Thinking Messages" value={formatNum(overview.thinking_message_count)} sub={displayTotalMessageCount > 0 ? `${(overview.thinking_message_count / displayTotalMessageCount * 100).toFixed(0)}% of messages` : undefined} />
              <StatCard label="Unique Tools" value={`${overview.tool_usage.length}`} />
            </div>
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
              <ResponsiveContainer width="100%" height={Math.max(200, overview.tool_usage.slice(0, 15).length * 28)}>
                <BarChart data={overview.tool_usage.slice(0, 15)} layout="vertical">
                  <CartesianGrid strokeDasharray="3 3" stroke="#2a2b36" />
                  <XAxis type="number" tick={{ fontSize: 10, fill: "#9394a1" }} />
                  <YAxis dataKey="tool_name" type="category" width={80} tick={{ fontSize: 10, fill: "#9394a1" }} />
                  <Tooltip contentStyle={TOOLTIP_STYLE} />
                  <Bar dataKey="count" fill="#3b82f6" radius={[0, 4, 4, 0]} />
                </BarChart>
              </ResponsiveContainer>
            </div>
          </Section>
        )}

        {/* Section 7: Activity */}
        <Section title="Activity">
          <div className="space-y-3">
            <ActivityCalendar tokenTimeseries={tokenTs} />
            {/* Hourly Activity Bar Chart */}
            {(() => {
              const maxHour = Math.max(...overview.hour_counts, 1);
              return (
                <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
                  <p className="text-sm font-medium text-text-primary mb-3">Hourly Activity</p>
                  <div className="flex items-end gap-[2px]" style={{ height: 120 }}>
                    {overview.hour_counts.map((count, hour) => {
                      const heightPx = count > 0 ? Math.max((count / maxHour) * 100, 4) : 0;
                      return (
                        <div key={hour} className="flex-1 flex flex-col items-center justify-end h-full">
                          <div
                            className="w-full bg-blue-500 rounded-t-sm min-w-[6px]"
                            style={{ height: heightPx }}
                            title={`${String(hour).padStart(2, "0")}:00 — ${count} messages`}
                          />
                        </div>
                      );
                    })}
                  </div>
                  <div className="flex gap-[2px] mt-1">
                    {overview.hour_counts.map((_, hour) => (
                      <div key={hour} className="flex-1 text-center">
                        {hour % 3 === 0 && <span className="text-[8px] text-text-muted">{String(hour).padStart(2, "0")}:00</span>}
                      </div>
                    ))}
                  </div>
                </div>
              );
            })()}
          </div>
        </Section>

        {/* Section 8: Cache Efficiency */}
        <Section title="Cache Efficiency">
          <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
            <StatCard label="Cache Hit Rate" value={`${overview.cache_stats.hit_rate_percent.toFixed(1)}%`} color={overview.cache_stats.hit_rate_percent > 80 ? "text-emerald-400" : "text-amber-400"} />
            <StatCard label="5min Cache" value={`${overview.cache_stats.ephemeral_5m_percent.toFixed(1)}%`} sub="of cache writes" />
            <StatCard label="1h Cache" value={`${overview.cache_stats.ephemeral_1h_percent.toFixed(1)}%`} sub="of cache writes" />
            <StatCard label="Total Cache" value={formatNum(overview.cache_stats.total_cache_read + overview.cache_stats.total_cache_write)} sub={`Read: ${formatNum(overview.cache_stats.total_cache_read)}`} />
          </div>
        </Section>

        {/* Section 9: Inference Geography */}
        {overview.geo_breakdown.length > 0 && (
          <Section title="Inference Geography">
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
              <div className="space-y-2">
                {overview.geo_breakdown.map((g, i) => {
                  const total = overview.geo_breakdown.reduce((s, x) => s + x.count, 0);
                  const pct = total > 0 ? (g.count / total) * 100 : 0;
                  return (
                    <div key={i} className="flex items-center gap-3 text-xs">
                      <span className="text-text-secondary w-24 truncate font-mono">{g.region}</span>
                      <div className="flex-1 h-2 bg-[#0e0f13] rounded-full overflow-hidden">
                        <div className="h-full bg-cyan-500 rounded-full" style={{ width: `${pct}%` }} />
                      </div>
                      <span className="text-text-muted w-16 text-right">{pct.toFixed(1)}%</span>
                      <span className="text-text-muted w-12 text-right font-mono">{formatNum(g.count)}</span>
                    </div>
                  );
                })}
              </div>
            </div>
          </Section>
        )}

        {/* Section 10: Per-Project Breakdown */}
        {overview.project_breakdown.length > 0 && (
          <Section title={`Projects (${overview.project_breakdown.length})`}>
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-hidden">
              <table className="w-full text-xs">
                <thead><tr className="border-b border-[#2a2b36] text-text-muted">
                  <th className="text-left px-3 py-2">Project</th>
                  <th className="text-right px-3 py-2">Messages</th>
                  <th className="text-right px-3 py-2">Tokens</th>
                  <th className="text-right px-3 py-2">Cost</th>
                </tr></thead>
                <tbody>
                  {overview.project_breakdown.slice(0, 20).map((p, i) => (
                    <tr key={i} className="border-b border-[#1e1f2a] hover:bg-[#22232e]">
                      <td className="px-3 py-2 text-text-primary truncate max-w-[250px]" title={p.project_path}>{p.project_name}</td>
                      <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(p.message_count)}</td>
                      <td className="px-3 py-2 text-right text-text-muted font-mono">{formatNum(p.total_tokens)}</td>
                      <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatUsd(p.estimated_cost_usd)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Section>
        )}

        {/* Section 11: Plugins & Productivity */}
        <Section title="Plugins & Productivity" defaultOpen={false}>
          <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-6 gap-3">
            <StatCard label="Plugins" value={`${overview.plugins_count}`} />
            <StatCard label="Commands" value={`${overview.custom_commands_count}`} />
            <StatCard label="Plans" value={`${overview.plans_count}`} />
            <StatCard label="Todos" value={`${overview.todos_summary.total_todos}`} sub={overview.todos_summary.total_todos > 0 ? `${overview.todos_summary.completed_todos} done` : undefined} />
            <StatCard label="Hooks" value={`${overview.hooks_count}`} />
            <StatCard label="Checkpoints" value={`${overview.file_history_checkpoints}`} />
          </div>
        </Section>

        {/* Section 12: Message Log */}
        {messageLog && messageLog.entries.length > 0 && (
          <Section title={`Message Log (${formatNum(messageLog.total_count)} total)`}>
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-hidden">
              <div className="overflow-x-auto">
                <table className="w-full text-xs">
                  <thead><tr className="border-b border-[#2a2b36] text-text-muted">
                    <th className="text-left px-3 py-2">Time</th>
                    <th className="text-left px-3 py-2">Model</th>
                    <th className="text-right px-3 py-2">Input</th>
                    <th className="text-right px-3 py-2">Output</th>
                    <th className="text-left px-3 py-2">Tools</th>
                    <th className="text-left px-3 py-2">Tier</th>
                    <th className="text-left px-3 py-2">Geo</th>
                    <th className="text-right px-3 py-2">Cost</th>
                  </tr></thead>
                  <tbody>
                    {messageLog.entries.map((e, i) => (
                      <tr key={i} className="border-b border-[#1e1f2a] hover:bg-[#22232e]">
                        <td className="px-3 py-2 text-text-muted whitespace-nowrap">{new Date(e.timestamp).toLocaleString("en-US", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" })}</td>
                        <td className="px-3 py-2 text-text-primary">
                          {e.model?.replace("claude-", "").replace("-high-thinking", " HT") ?? "-"}
                          {e.has_thinking && <Badge text="THINK" color="bg-purple-500/20 text-purple-400" />}
                          {e.speed === "fast" && <Badge text="FAST" color="bg-cyan-500/20 text-cyan-400" />}
                        </td>
                        <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(e.input_tokens)}</td>
                        <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(e.output_tokens)}</td>
                        <td className="px-3 py-2 text-text-muted truncate max-w-[120px]">{e.tools.length > 0 ? e.tools.slice(0, 3).join(", ") : "-"}</td>
                        <td className="px-3 py-2 text-text-muted">{e.service_tier ?? "-"}</td>
                        <td className="px-3 py-2 text-text-muted font-mono text-[10px]">{e.geo ?? "-"}</td>
                        <td className="px-3 py-2 text-right text-text-primary font-mono">{formatUsd(e.estimated_cost)}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <div className="flex items-center justify-between px-3 py-2 border-t border-[#2a2b36]">
                {messageLog.entries.length < messageLog.total_count && (
                  <button onClick={loadMoreLog} className="text-xs text-accent-blue hover:underline">
                    Load More ({messageLog.total_count - messageLog.entries.length} remaining)
                  </button>
                )}
                <button onClick={handleExportCsv} className="text-xs text-text-muted hover:text-text-primary flex items-center gap-1">
                  <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}><path strokeLinecap="round" strokeLinejoin="round" d="M12 10v6m0 0l-3-3m3 3l3-3m2 8H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" /></svg>
                  Export CSV
                </button>
              </div>
            </div>
          </Section>
        )}

        {/* Section 13: Enterprise Placeholder */}
        <Section title="Enterprise Analytics" defaultOpen={false}>
          <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-6 text-center">
            <div className="text-2xl mb-2">🔒</div>
            <h3 className="text-sm font-semibold text-text-primary mb-1">Enterprise Features</h3>
            <p className="text-xs text-text-muted mb-3 max-w-md mx-auto">
              Connect an Admin API key to unlock real USD costs, team productivity metrics, DAU/WAU/MAU, skill usage, and more.
            </p>
            <div className="flex flex-wrap justify-center gap-2 mb-4">
              <Badge text="Real Costs" color="bg-emerald-500/20 text-emerald-400" />
              <Badge text="Team Productivity" color="bg-blue-500/20 text-blue-400" />
              <Badge text="DAU/WAU/MAU" color="bg-purple-500/20 text-purple-400" />
              <Badge text="Skill Usage" color="bg-amber-500/20 text-amber-400" />
              <Badge text="Connector Usage" color="bg-cyan-500/20 text-cyan-400" />
            </div>
            <button disabled className="px-4 py-2 bg-[#2a2b36] text-text-muted rounded-lg text-xs cursor-not-allowed">
              Coming Soon
            </button>
          </div>
        </Section>

        </div>{/* end data sections opacity wrapper */}
      </div>
    </div>
  );
}

// ── Error Boundary ──────────────────────────────────────────────────────────

import { Component, type ErrorInfo, type ReactNode } from "react";

class AnalyticsErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  constructor(props: { children: ReactNode }) { super(props); this.state = { error: null }; }
  static getDerivedStateFromError(error: Error) { return { error }; }
  componentDidCatch(error: Error, info: ErrorInfo) { console.error("[ClaudeV2] Error:", error, info); }
  render() {
    if (this.state.error) {
      return (
        <div className="p-6">
          <div className="bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-xs">
            <p className="font-medium text-red-400 mb-2">Analytics crashed</p>
            <pre className="text-red-400/70 whitespace-pre-wrap">{this.state.error.message}</pre>
            <button onClick={() => this.setState({ error: null })} className="mt-3 px-3 py-1.5 bg-red-500/20 rounded text-red-300 hover:bg-red-500/30">Retry</button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

export function ClaudeAnalyticsV2Page() {
  return <AnalyticsErrorBoundary><ClaudeAnalyticsV2Inner /></AnalyticsErrorBoundary>;
}
