import { useState, useEffect } from "react";
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
  BarChart,
  Bar,
  PieChart,
  Pie,
  Cell,
  Legend,
} from "recharts";
import { useUsageStore, useFilteredUsage, type TimeRange } from "../stores/usageStore";
import { formatLargeNumber, formatCurrency } from "../lib/utils";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";
import { computeCost, computeCostBreakdown, getModelCostsConfig } from "../lib/modelCosts";
import { getClaudeSessionStats } from "../lib/tauri";
import type { ProjectUsageRecord, SessionStats } from "../lib/tauri";
import { DebugPath } from "../components/common/DebugPath";

const TIME_RANGE_OPTIONS: { value: TimeRange; label: string }[] = [
  { value: "5h", label: "Last 5 hours" },
  { value: "today", label: "Start of today" },
  { value: "7d", label: "Last 7 days" },
  { value: "week", label: "Start of this week" },
  { value: "30d", label: "Last 30 days" },
  { value: "month", label: "Start of this month" },
  { value: "all", label: "All time" },
];

const CHART_SERIES = [
  { key: "input" as const, color: "#3b82f6", tokenLabel: "Input Tokens", costLabel: "Input Cost" },
  { key: "output" as const, color: "#22c55e", tokenLabel: "Output Tokens", costLabel: "Output Cost" },
  { key: "cacheRead" as const, color: "#f59e0b", tokenLabel: "Cache Read Tokens", costLabel: "Cache Read Cost" },
];

function getRangeStartMs(range: TimeRange): number {
  const now = Date.now();
  const d = new Date(now);
  switch (range) {
    case "5h": return now - 5 * 60 * 60 * 1000;
    case "today": d.setHours(0, 0, 0, 0); return d.getTime();
    case "7d": return now - 7 * 24 * 60 * 60 * 1000;
    case "week": d.setDate(d.getDate() - d.getDay()); d.setHours(0, 0, 0, 0); return d.getTime();
    case "30d": return now - 30 * 24 * 60 * 60 * 1000;
    case "month": d.setDate(1); d.setHours(0, 0, 0, 0); return d.getTime();
    case "all": return 0;
    default: return now - 7 * 24 * 60 * 60 * 1000;
  }
}

type ChartPoint = { bucket: string; timestamp: number; input: number; output: number; cacheRead: number; total: number };

function toBucketKey(ts: number, range: TimeRange): string {
  const d = new Date(ts);
  if (range === "5h") {
    const slot = Math.floor((d.getHours() * 60 + d.getMinutes()) / 30) * 30;
    return `${String(Math.floor(slot / 60)).padStart(2, "0")}:${String(slot % 60).padStart(2, "0")}`;
  }
  if (range === "today") return `${String(d.getHours()).padStart(2, "0")}:00`;
  if (range === "all") {
    const w = new Date(ts);
    w.setDate(w.getDate() - w.getDay());
    w.setHours(0, 0, 0, 0);
    return `${w.getFullYear()}-${String(w.getMonth() + 1).padStart(2, "0")}-${String(w.getDate()).padStart(2, "0")}`;
  }
  const dayStart = new Date(ts);
  dayStart.setHours(0, 0, 0, 0);
  return `${dayStart.getFullYear()}-${String(dayStart.getMonth() + 1).padStart(2, "0")}-${String(dayStart.getDate()).padStart(2, "0")}`;
}

function toDisplayLabel(ts: number, range: TimeRange): string {
  const d = new Date(ts);
  if (range === "5h") return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  if (range === "today") return `${String(d.getHours()).padStart(2, "0")}:00`;
  if (range === "all") return d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "2-digit" });
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function buildChartData(
  records: ProjectUsageRecord[],
  range: TimeRange,
  viewMode: "token" | "cost"
): ChartPoint[] {
  const now = Date.now();
  const startMs = getRangeStartMs(range);
  const points: ChartPoint[] = [];

  if (range === "5h") {
    const intervalMs = 30 * 60 * 1000;
    const base = Math.floor(now / intervalMs) * intervalMs;
    for (let i = 10; i >= 0; i--) {
      const ts = base - i * intervalMs;
      if (ts >= startMs) {
        points.push({
          bucket: toDisplayLabel(ts, range),
          timestamp: ts,
          input: 0,
          output: 0,
          cacheRead: 0,
          total: 0,
        });
      }
    }
  } else if (range === "today") {
    const dStart = new Date(startMs);
    const hourStart = dStart.getHours();
    const dEnd = new Date(now);
    const hourEnd = dEnd.getHours();
    for (let h = hourStart; h <= hourEnd; h++) {
      const d = new Date(dStart);
      d.setHours(h, 0, 0, 0);
      const ts = d.getTime();
      points.push({ bucket: toDisplayLabel(ts, range), timestamp: ts, input: 0, output: 0, cacheRead: 0, total: 0 });
    }
  } else if (range === "all") {
    const weekMs = 7 * 24 * 60 * 60 * 1000;
    const earliest = records.length > 0
      ? Math.min(...records.map((r) => new Date(r.timestamp).getTime()))
      : now - 12 * weekMs;
    let weekStart = new Date(earliest);
    weekStart.setDate(weekStart.getDate() - weekStart.getDay());
    weekStart.setHours(0, 0, 0, 0);
    let ts = weekStart.getTime();
    const nowWeekStart = new Date(now);
    nowWeekStart.setDate(nowWeekStart.getDate() - nowWeekStart.getDay());
    nowWeekStart.setHours(0, 0, 0, 0);
    const endMs = nowWeekStart.getTime() + weekMs;
    while (ts <= endMs) {
      points.push({
        bucket: toDisplayLabel(ts, range),
        timestamp: ts,
        input: 0,
        output: 0,
        cacheRead: 0,
        total: 0,
      });
      ts += weekMs;
    }
  } else {
    const dayMs = 24 * 60 * 60 * 1000;
    const dStart = new Date(startMs);
    dStart.setHours(0, 0, 0, 0);
    let ts = dStart.getTime();
    const todayStart = new Date(now);
    todayStart.setHours(0, 0, 0, 0);
    const endMs = todayStart.getTime() + dayMs;
    while (ts < endMs) {
      points.push({
        bucket: toDisplayLabel(ts, range),
        timestamp: ts,
        input: 0,
        output: 0,
        cacheRead: 0,
        total: 0,
      });
      ts += dayMs;
    }
  }

  const keyToIndex = new Map<string, number>();
  points.forEach((p, i) => keyToIndex.set(toBucketKey(p.timestamp, range), i));

  records.forEach((r) => {
    const t = new Date(r.timestamp).getTime();
    if (t < startMs) return;
    const key = toBucketKey(t, range);
    const idx = keyToIndex.get(key);
    if (idx == null) return;
    const u = r.usage;
    const input = u?.input_tokens ?? 0;
    const output = u?.output_tokens ?? 0;
    const cacheRead = u?.cache_read_input_tokens ?? 0;
    if (viewMode === "cost") {
      const c = computeCostBreakdown(input, output, cacheRead, r.model);
      points[idx].input += c.inputCost;
      points[idx].output += c.outputCost;
      points[idx].cacheRead += c.cacheCost;
    } else {
      points[idx].input += input;
      points[idx].output += output;
      points[idx].cacheRead += cacheRead;
    }
    points[idx].total = points[idx].input + points[idx].output + points[idx].cacheRead;
  });

  return points;
}

interface DayCell {
  dateKey: string;
  date: Date;
  value: number;
}

function buildGitHubStyleHeatmap(records: ProjectUsageRecord[], viewMode: "token" | "cost"): {
  weeks: DayCell[][];
  monthLabels: { month: string; weekIndex: number }[];
} {
  const dayMap = new Map<string, number>();
  for (const r of records) {
    const d = new Date(r.timestamp);
    const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
    const u = r.usage;
    const input = u?.input_tokens ?? 0;
    const output = u?.output_tokens ?? 0;
    const cacheRead = u?.cache_read_input_tokens ?? 0;
    const val = viewMode === "cost"
      ? computeCost(input, output, cacheRead, r.model)
      : input + output + cacheRead;
    dayMap.set(key, (dayMap.get(key) ?? 0) + val);
  }

  const today = new Date();
  today.setHours(0, 0, 0, 0);
  const startOfYear = new Date(today.getFullYear(), 0, 1);
  const firstSunday = new Date(startOfYear);
  firstSunday.setDate(firstSunday.getDate() - firstSunday.getDay());
  const weeks: DayCell[][] = [];
  let current = new Date(firstSunday.getTime());
  const dayMs = 24 * 60 * 60 * 1000;

  while (current.getTime() <= today.getTime()) {
    const week: DayCell[] = [];
    for (let d = 0; d < 7; d++) {
      const t = current.getTime();
      if (t > today.getTime()) break;
      const dateKey = `${current.getFullYear()}-${String(current.getMonth() + 1).padStart(2, "0")}-${String(current.getDate()).padStart(2, "0")}`;
      week.push({
        dateKey,
        date: new Date(t),
        value: dayMap.get(dateKey) ?? 0,
      });
      current.setTime(t + dayMs);
    }
    if (week.length > 0) weeks.push(week);
    if (current.getTime() > today.getTime()) break;
    current.setTime(current.getTime() + (7 - week.length) * dayMs);
  }

  const monthLabels: { month: string; weekIndex: number }[] = [];
  weeks.forEach((week, weekIndex) => {
    if (week.length === 0) return;
    const firstDay = week[0].date;
    const monthName = firstDay.toLocaleDateString(undefined, { month: "short" });
    const isFirstWeek = weekIndex === 0;
    const prevMonth = weekIndex > 0 && weeks[weekIndex - 1]?.[0]
      ? weeks[weekIndex - 1][0].date.toLocaleDateString(undefined, { month: "short" })
      : "";
    const isFirstWeekOfMonth = !isFirstWeek && prevMonth !== monthName;
    if (isFirstWeek || isFirstWeekOfMonth) {
      monthLabels.push({ month: monthName, weekIndex });
    }
  });

  return { weeks, monthLabels };
}

const HEATMAP_COLORS = ["#161b22", "#39d353", "#26a641", "#006d32", "#0e4429"];

function Heatmap({ records, viewMode }: { records: ProjectUsageRecord[]; viewMode: "token" | "cost" }) {
  const { weeks, monthLabels } = buildGitHubStyleHeatmap(records, viewMode);
  const allValues = weeks.flatMap((w) => w.map((d) => d.value)).filter((v) => v > 0);
  const sorted = [...allValues].sort((a, b) => a - b);
  const len = sorted.length;

  const getLevel = (v: number): number => {
    if (v === 0 || len === 0) return 0;
    if (v >= sorted[Math.floor(len * 0.9)]) return 4;
    if (v >= sorted[Math.floor(len * 0.7)]) return 3;
    if (v >= sorted[Math.floor(len * 0.5)]) return 2;
    if (v >= sorted[Math.floor(len * 0.3)]) return 1;
    return 0;
  };

  const weekCount = weeks.length;
  if (weekCount === 0) {
    return (
      <div className="bg-app-card border border-border rounded-lg p-4">
        <p className="text-sm font-medium text-text-primary mb-3">Activity</p>
        <p className="text-text-muted text-sm">No activity data</p>
      </div>
    );
  }

  return (
    <div className="bg-app-card border border-border rounded-lg p-4 flex flex-col min-w-0">
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
        {weeks.map((_, weekIndex) => {
          const label = monthLabels.find((m) => m.weekIndex === weekIndex);
          return (
            <div
              key={`month-${weekIndex}`}
              className="min-w-0 h-4 flex items-center justify-center overflow-hidden"
              title={label?.month}
              style={{ gridColumn: weekIndex + 1, gridRow: 1 }}
            >
              {label && (
                <span className="text-[10px] text-text-muted truncate">{label.month}</span>
              )}
            </div>
          );
        })}
        {/* Heatmap cells: 7 rows, weekCount columns */}
        {weeks.map((week, weekIndex) =>
          week.map((day, dayIndex) => {
            const level = getLevel(day.value);
            const color = HEATMAP_COLORS[level];
            return (
              <div
                key={`${weekIndex}-${dayIndex}`}
                className="rounded-sm border border-white/5 min-h-[10px] min-w-[8px]"
                style={{
                  backgroundColor: color,
                  gridColumn: weekIndex + 1,
                  gridRow: dayIndex + 2,
                }}
                title={`${day.date.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" })} — ${viewMode === "cost" ? formatCurrency(day.value) : formatLargeNumber(day.value)}`}
              />
            );
          })
        )}
      </div>
      <div className="flex items-center gap-2 mt-2 text-xs text-text-muted flex-shrink-0">
        <span>Less</span>
        {HEATMAP_COLORS.map((color, i) => (
          <div key={i} className="w-3 h-3 rounded-sm border border-white/5" style={{ backgroundColor: color }} />
        ))}
        <span>More</span>
      </div>
    </div>
  );
}

type UsageTab = "tokens" | "sessions";

export function UsagePage() {
  const [activeTab, setActiveTab] = useState<UsageTab>("tokens");

  return (
    <div className="h-full flex flex-col">
      <div className="px-6 pt-6 pb-0">
        <div className="flex gap-1 border-b border-border">
          <button
            onClick={() => setActiveTab("tokens")}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
              activeTab === "tokens"
                ? "border-accent-blue text-text-primary"
                : "border-transparent text-text-muted hover:text-text-secondary"
            }`}
          >
            Token Usage
          </button>
          <button
            onClick={() => setActiveTab("sessions")}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
              activeTab === "sessions"
                ? "border-accent-blue text-text-primary"
                : "border-transparent text-text-muted hover:text-text-secondary"
            }`}
          >
            Session Stats
          </button>
        </div>
      </div>
      {activeTab === "tokens" ? <TokenUsageTab /> : <SessionStatsTab />}
    </div>
  );
}

const PIE_COLORS = ["#6366f1", "#22d3ee", "#f59e0b", "#ef4444", "#10b981", "#8b5cf6", "#ec4899", "#14b8a6"];

function SessionStatsTab() {
  const [stats, setStats] = useState<SessionStats | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    getClaudeSessionStats()
      .then((s) => { setStats(s); setLoading(false); })
      .catch((e) => { setError(String(e)); setLoading(false); });
  }, []);

  if (loading) return <div className="p-6 text-text-muted">Loading session stats...</div>;
  if (error) return <div className="p-6 text-accent-red text-sm">{error}</div>;
  if (!stats) return null;

  const modelData = Object.entries(stats.model_usage)
    .filter(([, entry]) => entry.input_tokens + entry.output_tokens > 0)
    .sort((a, b) => (b[1].input_tokens + b[1].output_tokens) - (a[1].input_tokens + a[1].output_tokens))
    .slice(0, 8)
    .map(([name, entry]) => {
      const tokens = entry.input_tokens + entry.output_tokens;
      const cost = entry.cost_usd > 0
        ? entry.cost_usd
        : computeCost(entry.input_tokens, entry.output_tokens, entry.cache_read_input_tokens, name);
      return { name: name.split("/").pop() ?? name, value: tokens, cost };
    });

  const totalCost = stats.total_cost_usd > 0
    ? stats.total_cost_usd
    : Object.entries(stats.model_usage).reduce((sum, [model, entry]) =>
        sum + computeCost(entry.input_tokens, entry.output_tokens, entry.cache_read_input_tokens, model), 0);

  const hourData = stats.hour_counts.map((count, hour) => ({
    hour: `${String(hour).padStart(2, "0")}:00`,
    count,
  }));

  const longestDuration = stats.longest_session
    ? `${Math.round(stats.longest_session.duration / 60000)} min`
    : "—";

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-xl font-semibold text-text-primary">Claude Code Session Stats</h2>
          <DebugPath path="~/.claude/stats-cache.json" className="text-sm" />
        </div>
        <button
          onClick={() => {
            setLoading(true);
            getClaudeSessionStats()
              .then((s) => { setStats(s); setLoading(false); })
              .catch((e) => { setError(String(e)); setLoading(false); });
          }}
          className="px-3 py-1.5 text-sm bg-app-card border border-border rounded-lg hover:bg-app-card-hover"
        >
          Refresh
        </button>
      </div>

      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <div className="bg-app-card border border-border rounded-lg p-4">
          <div className="text-xs text-text-muted uppercase tracking-wider mb-1">Total Sessions</div>
          <div className="text-2xl font-bold text-text-primary">{stats.total_sessions.toLocaleString()}</div>
        </div>
        <div className="bg-app-card border border-border rounded-lg p-4">
          <div className="text-xs text-text-muted uppercase tracking-wider mb-1">Total Messages</div>
          <div className="text-2xl font-bold text-text-primary">{stats.total_messages.toLocaleString()}</div>
        </div>
        <div className="bg-app-card border border-border rounded-lg p-4">
          <div className="text-xs text-text-muted uppercase tracking-wider mb-1">Longest Session</div>
          <div className="text-2xl font-bold text-text-primary">{longestDuration}</div>
          {stats.longest_session && (
            <div className="text-xs text-text-muted mt-1">{stats.longest_session.message_count} messages</div>
          )}
        </div>
        <div className="bg-app-card border border-border rounded-lg p-4">
          <div className="text-xs text-text-muted uppercase tracking-wider mb-1">Total Cost</div>
          <div className="text-2xl font-bold text-text-primary">${totalCost.toFixed(2)}</div>
        </div>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="bg-app-card border border-border rounded-lg p-4">
          <h3 className="text-sm font-semibold text-text-primary mb-3">Hourly Activity</h3>
          <ResponsiveContainer width="100%" height={250}>
            <BarChart data={hourData}>
              <CartesianGrid strokeDasharray="3 3" stroke="#2a2b36" />
              <XAxis dataKey="hour" tick={{ fill: "#9394a1", fontSize: 10 }} interval={2} />
              <YAxis tick={{ fill: "#9394a1", fontSize: 11 }} />
              <Tooltip contentStyle={{ background: "#1a1b23", border: "1px solid #2a2b36", borderRadius: 8 }} />
              <Bar dataKey="count" fill="#3b82f6" radius={[2, 2, 0, 0]} name="Messages" />
            </BarChart>
          </ResponsiveContainer>
        </div>

        <div className="bg-app-card border border-border rounded-lg p-4">
          <h3 className="text-sm font-semibold text-text-primary mb-3">Model Usage</h3>
          {modelData.length > 0 ? (
            <ResponsiveContainer width="100%" height={250}>
              <PieChart>
                <Pie
                  data={modelData}
                  cx="50%"
                  cy="50%"
                  innerRadius={55}
                  outerRadius={85}
                  dataKey="value"
                  label={({ name, percent }) => `${name} ${(percent * 100).toFixed(0)}%`}
                >
                  {modelData.map((_, i) => (
                    <Cell key={i} fill={PIE_COLORS[i % PIE_COLORS.length]} />
                  ))}
                </Pie>
                <Tooltip
                  contentStyle={{ background: "#1a1b23", border: "1px solid #2a2b36", borderRadius: 8 }}
                  formatter={(v: number, _: string, entry: { payload?: { cost?: number } }) => [
                    `${formatLargeNumber(v)} tokens${entry.payload?.cost != null ? ` ($${entry.payload.cost.toFixed(2)})` : ""}`,
                    "Usage",
                  ]}
                />
                <Legend />
              </PieChart>
            </ResponsiveContainer>
          ) : (
            <p className="text-text-muted text-sm">No model data available.</p>
          )}
        </div>
      </div>

      {stats.first_session_date && (
        <p className="text-xs text-text-muted">Tracking since {new Date(stats.first_session_date).toLocaleDateString()}</p>
      )}
    </div>
  );
}

function TokenUsageTab() {
  const { refetch, loading, error, timeRange, setTimeRange, modelFilter, setModelFilter, projectFilter, setProjectFilter } = useUsageStore();
  const { records, totals, models } = useFilteredUsage();
  const [mounted, setMounted] = useState(false);
  const [viewMode, setViewMode] = useState<"token" | "cost">("token");
  const [hiddenSeries, setHiddenSeries] = useState<string[]>([]);

  useEffect(() => {
    setMounted(true);
  }, []);
  useEffect(() => {
    if (mounted) refetch();
  }, [mounted, refetch]);

  const costTotals = viewMode === "cost"
    ? records.reduce(
        (acc, r) => {
          const u = r.usage;
          const c = computeCostBreakdown(
            u?.input_tokens ?? 0,
            u?.output_tokens ?? 0,
            u?.cache_read_input_tokens ?? 0,
            r.model
          );
          return {
            input: acc.input + c.inputCost,
            output: acc.output + c.outputCost,
            cacheRead: acc.cacheRead + c.cacheCost,
          };
        },
        { input: 0, output: 0, cacheRead: 0 }
      )
    : null;

  const displayTotals = viewMode === "cost" && costTotals ? costTotals : totals;

  const chartData = buildChartData(records, timeRange, viewMode);
  const visibleKeys = CHART_SERIES.filter((s) => !hiddenSeries.includes(s.key)).map((s) => s.key);
  const chartYMax = chartData.length > 0
    ? Math.max(1, ...chartData.map((p) => visibleKeys.reduce((sum, k) => sum + (p[k as keyof ChartPoint] as number), 0)))
    : 1;

  return (
    <>
      <div className="p-6 border-b border-border flex items-center justify-between flex-wrap gap-3">
        <div>
          <h1 className="text-2xl font-semibold text-text-primary mb-1">Token Usage</h1>
          <p className="text-text-muted text-sm">Monitor your token usage</p>
        </div>
        <div className="flex items-center gap-3 flex-wrap">
          <ProjectScopeSelector value={projectFilter} onChange={setProjectFilter} />
          <div className="flex gap-0.5 p-0.5 bg-app-bg rounded-lg border border-border">
            <button
              type="button"
              onClick={() => setViewMode("token")}
              className={`px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${
                viewMode === "token" ? "bg-accent-blue text-white" : "text-text-muted hover:text-text-primary"
              }`}
            >
              Token
            </button>
            <button
              type="button"
              onClick={() => setViewMode("cost")}
              className={`px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${
                viewMode === "cost" ? "bg-accent-blue text-white" : "text-text-muted hover:text-text-primary"
              }`}
            >
              Cost
            </button>
          </div>
          <button
            onClick={() => refetch()}
            disabled={loading}
            className="flex items-center gap-2 px-3 py-2 rounded-lg border border-border text-text-primary text-sm hover:bg-app-card-hover disabled:opacity-50"
          >
            <span className="text-base">↻</span> Refresh
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-6 space-y-6">
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          <Heatmap records={records} viewMode={viewMode} />
          <div className="lg:col-span-2 grid grid-cols-1 md:grid-cols-3 gap-4">
            <div className="bg-[#1e3a5f] border border-[#2563eb]/30 rounded-lg p-4 flex items-start justify-between">
              <div>
                <p className="text-xs text-blue-200/80 uppercase mb-1">
                  {viewMode === "token" ? "Input Tokens" : "Input Cost"}
                </p>
                <p className="text-xl font-semibold text-white">
                  {viewMode === "token"
                    ? formatLargeNumber(displayTotals.input)
                    : formatCurrency(displayTotals.input)}
                </p>
              </div>
              <span className="text-2xl text-blue-300/70">↓</span>
            </div>
            <div className="bg-[#1a3d2e] border border-[#22c55e]/30 rounded-lg p-4 flex items-start justify-between">
              <div>
                <p className="text-xs text-green-200/80 uppercase mb-1">
                  {viewMode === "token" ? "Output Tokens" : "Output Cost"}
                </p>
                <p className="text-xl font-semibold text-white">
                  {viewMode === "token"
                    ? formatLargeNumber(displayTotals.output)
                    : formatCurrency(displayTotals.output)}
                </p>
              </div>
              <span className="text-2xl text-green-300/70">↑</span>
            </div>
            <div className="bg-[#3d2e1a] border border-[#f59e0b]/30 rounded-lg p-4 flex items-start justify-between">
              <div>
                <p className="text-xs text-amber-200/80 uppercase mb-1">
                  {viewMode === "token" ? "Cache Read Tokens" : "Cache Read Cost"}
                </p>
                <p className="text-xl font-semibold text-white">
                  {viewMode === "token"
                    ? formatLargeNumber(displayTotals.cacheRead)
                    : formatCurrency(displayTotals.cacheRead)}
                </p>
              </div>
              <span className="text-2xl text-amber-300/70">ⓘ</span>
            </div>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-4">
          <div className="flex items-center gap-2">
            <span className="text-sm text-text-muted">Model:</span>
            <select
              value={modelFilter}
              onChange={(e) => setModelFilter(e.target.value)}
              className="px-3 py-2 bg-app-card border border-border rounded-lg text-text-primary text-sm"
            >
              <option value="">All models</option>
              {models.map((m) => (
                <option key={m} value={m}>{m}</option>
              ))}
            </select>
          </div>
          <div className="flex items-center gap-2">
            <span className="text-sm text-text-muted">Time Range:</span>
            <select
              value={timeRange}
              onChange={(e) => setTimeRange(e.target.value as TimeRange)}
              className="px-3 py-2 bg-app-card border border-border rounded-lg text-text-primary text-sm"
            >
              {TIME_RANGE_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>{opt.label}</option>
              ))}
            </select>
          </div>
        </div>

        {error && (
          <p className="text-sm text-accent-red">{error}</p>
        )}

        {chartData.length > 0 && (
          <div className="bg-app-card border border-border rounded-lg p-4">
            <p className="text-sm font-medium text-text-primary mb-4">
              {viewMode === "token" ? "Token usage over time" : "Cost over time"}
            </p>
            <div className="flex items-center justify-center gap-6 mb-3">
              {CHART_SERIES.map((s) => (
                <button
                  key={s.key}
                  type="button"
                  onClick={() => setHiddenSeries((prev) =>
                    prev.includes(s.key) ? prev.filter((k) => k !== s.key) : [...prev, s.key]
                  )}
                  className="flex items-center gap-1.5 text-xs transition-opacity"
                  style={{ opacity: hiddenSeries.includes(s.key) ? 0.35 : 1 }}
                >
                  <span className="inline-block w-2.5 h-2.5 rounded-full" style={{ backgroundColor: s.color }} />
                  <span className="text-text-secondary">{viewMode === "token" ? s.tokenLabel : s.costLabel}</span>
                </button>
              ))}
            </div>
            <div style={{ width: "100%", height: 280 }}>
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={chartData} margin={{ top: 8, right: 8, left: 8, bottom: 8 }}>
                  <defs>
                    {CHART_SERIES.map((s) => (
                      <linearGradient key={s.key} id={`grad-${s.key}`} x1="0" y1="0" x2="0" y2="1">
                        <stop offset="0%" stopColor={s.color} stopOpacity={0.25} />
                        <stop offset="100%" stopColor={s.color} stopOpacity={0} />
                      </linearGradient>
                    ))}
                  </defs>
                  <CartesianGrid strokeDasharray="3 3" stroke="#2a2b36" />
                  <XAxis
                    dataKey="bucket"
                    tick={{ fontSize: 10, fill: "#9394a1" }}
                  />
                  <YAxis
                    tick={{ fontSize: 10, fill: "#9394a1" }}
                    tickFormatter={(v) => (viewMode === "cost" ? formatCurrency(v) : formatLargeNumber(v))}
                    domain={[0, chartYMax]}
                  />
                  <Tooltip
                    contentStyle={{ backgroundColor: "#1a1b23", border: "1px solid #2a2b36" }}
                    labelStyle={{ color: "#e8e9ed" }}
                    formatter={(value: number, name: string) => [
                      viewMode === "cost" ? formatCurrency(value) : formatLargeNumber(value),
                      name,
                    ]}
                  />
                  {CHART_SERIES.map((s) => (
                    <Area
                      key={s.key}
                      type="monotone"
                      dataKey={s.key}
                      stroke={s.color}
                      strokeWidth={2}
                      fill={`url(#grad-${s.key})`}
                      name={viewMode === "token" ? s.tokenLabel : s.costLabel}
                      isAnimationActive={false}
                      hide={hiddenSeries.includes(s.key)}
                    />
                  ))}
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </div>
        )}

        {!loading && records.length === 0 && (
          <p className="text-text-muted text-sm">No usage data in the selected range.</p>
        )}

        {viewMode === "cost" && <ModelPricingSection />}
      </div>
    </>
  );
}

function ModelPricingSection() {
  const [expanded, setExpanded] = useState(false);
  const config = getModelCostsConfig();
  const modelList = Object.entries(config.models).sort((a, b) =>
    a[1].name.localeCompare(b[1].name)
  );
  return (
    <div className="bg-app-card border border-border rounded-lg overflow-hidden">
      <button
        type="button"
        onClick={() => setExpanded((e) => !e)}
        className="w-full px-4 py-3 flex items-center justify-between text-left hover:bg-app-card-hover transition-colors"
      >
        <span className="text-sm font-medium text-text-primary">Model pricing (per 1M tokens)</span>
        <span className="text-text-muted text-sm">{expanded ? "▼" : "▶"}</span>
      </button>
      {expanded && (
        <div className="px-4 pb-4 border-t border-border">
          {config.description && (
            <p className="text-xs text-text-muted mt-3 mb-2">{config.description}</p>
          )}
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="text-left text-text-muted border-b border-border">
                  <th className="py-2 pr-4 font-medium">Model</th>
                  <th className="py-2 pr-4 font-medium">Input ($/M)</th>
                  <th className="py-2 pr-4 font-medium">Output ($/M)</th>
                  <th className="py-2 pr-4 font-medium">Cache read ($/M)</th>
                </tr>
              </thead>
              <tbody>
                {modelList.map(([id, m]) => (
                  <tr key={id} className="border-b border-border/50 last:border-0">
                    <td className="py-2 pr-4 text-text-primary">
                      <span>{m.name}</span>
                      {m.deprecated && (
                        <span className="ml-2 text-[10px] px-1.5 py-0.5 rounded bg-text-muted/20 text-text-muted">
                          Deprecated
                        </span>
                      )}
                    </td>
                    <td className="py-2 pr-4 text-text-secondary font-mono">{m.input_per_million}</td>
                    <td className="py-2 pr-4 text-text-secondary font-mono">{m.output_per_million}</td>
                    <td className="py-2 pr-4 text-text-secondary font-mono">{m.cache_read_per_million}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
}
