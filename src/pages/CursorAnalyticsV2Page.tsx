/**
 * Cursor Analytics V2 — comprehensive dashboard powered by Cursor's dashboard APIs.
 * Auth: auto-detected from Cursor's local SQLite DB (zero friction).
 */
import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import {
  BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid, Legend,
  PieChart, Pie, Cell, AreaChart, Area,
} from "recharts";
import { useAiTrackingStore } from "../stores/aiTrackingStore";
import type { ScoredCommit } from "../lib/tauri";
import { DebugPath } from "../components/common/DebugPath";

// ── Types ───────────────────────────────────────────────────────────────────

interface ConnectionStatus {
  connected: boolean;
  connection_method: string;
  email: string | null;
  plan: string | null;
  team_name: string | null;
  team_id: number | null;
  error: string | null;
}

interface UsagePlanBreakdown {
  included?: number;
  bonus?: number;
  total?: number;
}

interface UsagePlan {
  enabled?: boolean;
  used?: number;
  limit?: number;
  remaining?: number;
  breakdown?: UsagePlanBreakdown;
  autoPercentUsed?: number;
  apiPercentUsed?: number;
  totalPercentUsed?: number;
}

interface UsageOnDemand {
  enabled?: boolean;
  used?: number;
  limit?: number;
  remaining?: number;
}

interface StripeInfo {
  membershipType?: string;
  paymentId?: string;
  isTeamMember?: boolean;
  teamId?: number;
  teamMembershipType?: string;
  individualMembershipType?: string;
  isOnBillableAuto?: boolean;
  isYearlyPlan?: boolean;
  lastPaymentFailed?: boolean;
  pendingCancellationDate?: string;
  customerBalance?: number;
  verifiedStudent?: boolean;
}

interface AuthInfo {
  email?: string;
  name?: string;
  sub?: string;
  id?: number;
  created_at?: string;
  updated_at?: string;
  picture?: string;
  email_verified?: boolean;
}

interface TeamInfo {
  name?: string;
  id?: number;
  role?: string;
  seats?: number;
  hasBilling?: boolean;
  privacyModeForced?: boolean;
  subscriptionStatus?: string;
  pricingStrategy?: string;
  billingCycleStart?: string;
  billingCycleEnd?: string;
  ssoEnabled?: boolean;
  dashboardAnalyticsRequiresAdmin?: boolean;
}

interface Overview {
  auth: AuthInfo | null;
  stripe: StripeInfo | null;
  usage_summary: {
    billingCycleStart?: string;
    billingCycleEnd?: string;
    membershipType?: string;
    limitType?: string;
    isUnlimited?: boolean;
    individualUsage?: { plan?: UsagePlan; onDemand?: UsageOnDemand };
    teamUsage?: { onDemand?: UsageOnDemand };
  } | null;
  hard_limit: { hardLimit?: number; isDynamicTeamLimit?: boolean } | null;
  ai_commits: { total_commits?: number; total_lines_added?: number; ai_lines_added?: number; ai_impact_percentage?: number; unique_repos?: number; avg_ai_lines_per_commit?: number } | null;
  model_aggregated: { model_intent?: string; total_requests?: number; total_unique_users?: number }[] | null;
  team: TeamInfo | null;
  team_members: { teamMembers?: { id?: number; email?: string; name?: string; role?: string }[] } | null;
  composer_stats: ClickHouseResponse | null;
  tab_stats: ClickHouseResponse | null;
  top_files: ClickHouseResponse | null;
  request_breakdown: ClickHouseResponse | null;
  ai_commits_by_repo: ClickHouseResponse | null;
  sessions: unknown[] | null;
  connection_method: string;
  error: string | null;
}

interface TokenUsage {
  inputTokens?: number;
  outputTokens?: number;
  cacheWriteTokens?: number;
  cacheReadTokens?: number;
  totalCents?: number;
}

interface UsageEvent {
  timestamp?: string;
  model?: string;
  kind?: string;
  maxMode?: boolean;
  requestsCosts?: number;
  usageBasedCosts?: string;
  isTokenBasedCall?: boolean;
  tokenUsage?: TokenUsage;
  owningUser?: string;
  owningTeam?: string;
  chargedCents?: number;
  cursorTokenFee?: number;
  isChargeable?: boolean;
  isHeadless?: boolean;
}

interface UsageEventsPage {
  events: UsageEvent[];
  total_count: number;
  page: number;
  page_size: number;
}

interface ClickHouseResponse {
  meta?: { name: string; type: string }[];
  data?: Record<string, unknown>[];
}

// ── Helpers ─────────────────────────────────────────────────────────────────

function cents(v: number | undefined | null): string {
  if (v == null) return "$0.00";
  return `$${(v / 100).toFixed(2)}`;
}

function formatNum(n: number | undefined | null): string {
  if (n == null || n === 0) return "0";
  if (n >= 1_000_000_000) return `${(n / 1_000_000_000).toFixed(1)}B`;
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

function formatDate(epochMs: string | undefined | null): string {
  if (!epochMs) return "";
  const d = new Date(parseInt(epochMs));
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

function formatIsoDate(iso: string | undefined | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
}

function formatEpochDate(epochMs: string | undefined | null): string {
  if (!epochMs) return "";
  const d = new Date(parseInt(epochMs));
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
}

function shortModel(model: string | undefined | null): string {
  if (!model) return "unknown";
  return model
    .replace("claude-", "")
    .replace("-high-thinking", " HT")
    .replace("-max-thinking-fast", " Max")
    .replace("-max-thinking", " Max")
    .replace("gpt-", "GPT ")
    .replace("gemini-", "Gem ")
    .replace("composer-", "Comp ");
}

function pct(n: number | undefined | null): string {
  if (n == null) return "0%";
  return `${n.toFixed(1)}%`;
}

const COLORS = ["#3b82f6", "#8b5cf6", "#22c55e", "#f59e0b", "#ef4444", "#06b6d4", "#ec4899", "#84cc16", "#a855f7", "#14b8a6"];

// ── Human-readable kind/type labels ─────────────────────────────────────────

function formatKind(kind: string | undefined | null, isChargeable?: boolean): string {
  if (!kind) return "-";
  const k = kind.toLowerCase().replace(/[-_]/g, " ").trim();
  // Map known API values to Cursor-dashboard-style labels
  const MAP: Record<string, string> = {
    "included": "Included",
    "free": "Free",
    "on demand": "On-Demand",
    "on_demand": "On-Demand",
    "ondemand": "On-Demand",
    "premium": "Premium",
    "usage based": "Usage-Based",
    "usage_based": "Usage-Based",
    "bonus": "Bonus",
    "trial": "Trial",
  };
  if (MAP[k]) return MAP[k];
  // If chargeable but kind not in map, label as On-Demand
  if (isChargeable) return "On-Demand";
  // Title-case fallback
  return k.split(" ").map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(" ");
}

// ── Human-readable role labels ──────────────────────────────────────────────

function formatRole(role: string | undefined | null): string {
  if (!role) return "Member";
  // Strip common prefixes like TEAM_ROLE_
  let cleaned = role.replace(/^TEAM_ROLE_/i, "").replace(/_/g, " ").trim();
  if (!cleaned) return "Member";
  // Title-case
  return cleaned.split(" ").map((w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase()).join(" ");
}

// Role priority for sorting (lower = higher privilege)
function rolePriority(role: string | undefined | null): number {
  const r = formatRole(role).toLowerCase();
  if (r === "owner") return 0;
  if (r === "admin") return 1;
  if (r === "free owner") return 2;
  if (r === "manager") return 3;
  if (r === "member") return 4;
  return 5;
}

const TOOLTIP_STYLE = { background: "#1a1b23", border: "1px solid #2a2b36", borderRadius: "8px", fontSize: "11px" };

// ── Stat Card ───────────────────────────────────────────────────────────────

function StatCard({ label, value, sub, color }: { label: string; value: string; sub?: string; color?: string }) {
  return (
    <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] px-4 py-3">
      <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">{label}</div>
      <div className={`text-xl font-semibold ${color || "text-text-primary"}`}>{value}</div>
      {sub && <div className="text-[11px] text-text-muted mt-0.5">{sub}</div>}
    </div>
  );
}

// ── Multi-Segment Usage Bar ─────────────────────────────────────────────────

interface UsageSegment {
  label: string;
  value: number; // in cents
  color: string; // tailwind bg class
  textColor: string; // tailwind text class
}

function MultiSegmentUsageBar({ segments, sub }: { segments: UsageSegment[]; sub?: string }) {
  const total = segments.reduce((s, seg) => s + seg.value, 0);
  if (total === 0) return null;
  return (
    <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] px-4 py-3">
      <div className="flex justify-between items-baseline mb-2">
        <span className="text-xs text-text-muted">Usage Breakdown</span>
        <span className="text-sm font-semibold text-text-primary">{cents(total)} total spent</span>
      </div>
      <div className="h-3 bg-[#0e0f13] rounded-full overflow-hidden flex">
        {segments.map((seg, i) => {
          const pctVal = (seg.value / total) * 100;
          if (pctVal <= 0) return null;
          return (
            <div
              key={i}
              className={`h-full transition-all duration-500 ${seg.color} ${i === 0 ? "rounded-l-full" : ""} ${i === segments.length - 1 ? "rounded-r-full" : ""}`}
              style={{ width: `${pctVal}%` }}
              title={`${seg.label}: ${cents(seg.value)}`}
            />
          );
        })}
      </div>
      <div className="flex flex-wrap gap-3 mt-2">
        {segments.map((seg, i) => (
          <div key={i} className="flex items-center gap-1.5 text-[10px]">
            <div className={`w-2 h-2 rounded-full ${seg.color}`} />
            <span className={seg.textColor}>{cents(seg.value)} {seg.label}</span>
          </div>
        ))}
      </div>
      {sub && <div className="text-[10px] text-text-muted mt-1.5">{sub}</div>}
    </div>
  );
}

// ── Section (collapsible) ───────────────────────────────────────────────────

function Section({ title, children, defaultOpen = true }: { title: string; children: React.ReactNode; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="mb-6">
      <button
        onClick={() => setOpen(!open)}
        className="flex items-center gap-2 w-full text-left mb-3 group"
      >
        <svg
          className={`w-3 h-3 text-text-muted transition-transform ${open ? "rotate-90" : ""}`}
          fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}
        >
          <path strokeLinecap="round" strokeLinejoin="round" d="M9 5l7 7-7 7" />
        </svg>
        <h3 className="text-xs font-semibold uppercase tracking-wider text-text-muted group-hover:text-text-secondary">{title}</h3>
      </button>
      {open && children}
    </div>
  );
}

// ── Badge ───────────────────────────────────────────────────────────────────

function Badge({ text, color = "bg-accent-blue/20 text-accent-blue" }: { text: string; color?: string }) {
  return <span className={`text-[9px] px-1.5 py-0.5 rounded font-medium ${color}`}>{text}</span>;
}

// ── AI Code Attribution Helpers ──────────────────────────────────────────────

const ATTRIBUTION_COLORS = ["#6366f1", "#22d3ee", "#f59e0b", "#ef4444", "#10b981", "#8b5cf6", "#ec4899", "#14b8a6"];

function AttributionSummaryCard({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div className="bg-app-card border border-border rounded-lg p-4">
      <div className="text-xs text-text-muted uppercase tracking-wider mb-1">{label}</div>
      <div className="text-2xl font-bold text-text-primary">{value}</div>
      {sub && <div className="text-xs text-text-secondary mt-1">{sub}</div>}
    </div>
  );
}

function AiVsHumanPie({ aiLines, humanLines }: { aiLines: number; humanLines: number }) {
  const total = aiLines + humanLines;
  if (total === 0) return <p className="text-text-muted text-sm">No data.</p>;
  const data = [
    { name: "AI-generated", value: aiLines },
    { name: "Human-written", value: humanLines },
  ];
  return (
    <ResponsiveContainer width="100%" height={250}>
      <PieChart>
        <Pie data={data} cx="50%" cy="50%" innerRadius={60} outerRadius={90} dataKey="value" label={({ name, percent }) => `${name} ${(percent * 100).toFixed(0)}%`}>
          <Cell fill="#6366f1" />
          <Cell fill="#22d3ee" />
        </Pie>
        <Tooltip contentStyle={{ background: "#1a1b23", border: "1px solid #2a2b36", borderRadius: 8 }} />
        <Legend />
      </PieChart>
    </ResponsiveContainer>
  );
}

function AiTrendChart({ commits }: { commits: ScoredCommit[] }) {
  if (commits.length === 0) return <p className="text-text-muted text-sm">No trend data.</p>;
  const sorted = [...commits].sort((a, b) => a.scored_at - b.scored_at);
  const chartData = sorted.map((c) => ({
    date: new Date(c.scored_at).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" }),
    ai: c.ai_percentage,
  }));
  return (
    <ResponsiveContainer width="100%" height={250}>
      <AreaChart data={chartData}>
        <CartesianGrid strokeDasharray="3 3" stroke="#2a2b36" />
        <XAxis dataKey="date" tick={{ fill: "#9394a1", fontSize: 10 }} />
        <YAxis tick={{ fill: "#9394a1", fontSize: 11 }} domain={[0, 100]} unit="%" />
        <Tooltip contentStyle={{ background: "#1a1b23", border: "1px solid #2a2b36", borderRadius: 8 }} formatter={(v: number) => `${v.toFixed(1)}%`} />
        <Area type="monotone" dataKey="ai" stroke="#6366f1" fill="#6366f1" fillOpacity={0.2} name="AI %" />
      </AreaChart>
    </ResponsiveContainer>
  );
}

function FileTypeChart({ data }: { data: { file_extension: string; source: string; count: number }[] }) {
  const aiByExt: Record<string, number> = {};
  for (const entry of data) {
    if (entry.source !== "human") {
      aiByExt[entry.file_extension] = (aiByExt[entry.file_extension] ?? 0) + entry.count;
    }
  }
  const chartData = Object.entries(aiByExt)
    .sort((a, b) => b[1] - a[1])
    .slice(0, 12)
    .map(([ext, count]) => ({ ext: `.${ext}`, count }));
  if (chartData.length === 0) return <p className="text-text-muted text-sm">No data.</p>;
  return (
    <ResponsiveContainer width="100%" height={250}>
      <BarChart data={chartData} layout="vertical" margin={{ left: 40 }}>
        <CartesianGrid strokeDasharray="3 3" stroke="#2a2b36" />
        <XAxis type="number" tick={{ fill: "#9394a1", fontSize: 11 }} />
        <YAxis type="category" dataKey="ext" tick={{ fill: "#9394a1", fontSize: 11 }} width={50} />
        <Tooltip contentStyle={{ background: "#1a1b23", border: "1px solid #2a2b36", borderRadius: 8 }} />
        <Bar dataKey="count" fill="#6366f1" radius={[0, 4, 4, 0]} />
      </BarChart>
    </ResponsiveContainer>
  );
}

function ModelBreakdownChart({ data }: { data: Record<string, number> }) {
  const entries = Object.entries(data).filter(([k]) => k !== "unknown" && k !== "");
  if (entries.length === 0) return <p className="text-text-muted text-sm">No model data.</p>;
  const chartData = entries
    .sort((a, b) => b[1] - a[1])
    .slice(0, 8)
    .map(([name, value]) => ({ name: name.split("/").pop() ?? name, value }));
  return (
    <ResponsiveContainer width="100%" height={250}>
      <PieChart>
        <Pie data={chartData} cx="50%" cy="50%" outerRadius={90} dataKey="value" label={({ name, percent }) => `${name} ${(percent * 100).toFixed(0)}%`}>
          {chartData.map((_, i) => (
            <Cell key={i} fill={ATTRIBUTION_COLORS[i % ATTRIBUTION_COLORS.length]} />
          ))}
        </Pie>
        <Tooltip contentStyle={{ background: "#1a1b23", border: "1px solid #2a2b36", borderRadius: 8 }} />
      </PieChart>
    </ResponsiveContainer>
  );
}

function formatCommitDate(d: string): string {
  try {
    const date = new Date(d);
    if (isNaN(date.getTime())) return d;
    return date.toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
  } catch {
    return d;
  }
}

function formatEpochMs(ms: number): string {
  return new Date(ms).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
}

function CommitTable({ commits }: { commits: ScoredCommit[] }) {
  if (commits.length === 0) {
    return <p className="text-text-muted text-sm">No commit data available.</p>;
  }
  return (
    <div className="overflow-auto max-h-[400px] border border-border rounded-lg">
      <table className="w-full text-sm">
        <thead className="bg-app-sidebar sticky top-0">
          <tr className="text-left text-text-muted text-xs uppercase tracking-wider">
            <th className="px-3 py-2">Commit</th>
            <th className="px-3 py-2">Branch</th>
            <th className="px-3 py-2">Message</th>
            <th className="px-3 py-2 text-right">AI %</th>
            <th className="px-3 py-2 text-right">Agent +</th>
            <th className="px-3 py-2 text-right">Human +</th>
            <th className="px-3 py-2 text-right">+Lines</th>
            <th className="px-3 py-2 text-right">-Lines</th>
            <th className="px-3 py-2">Date</th>
          </tr>
        </thead>
        <tbody>
          {commits.map((c) => (
            <tr key={`${c.commit_hash}-${c.branch_name}`} className="border-t border-border hover:bg-app-card-hover">
              <td className="px-3 py-2 font-mono text-xs text-text-secondary">{c.commit_hash.slice(0, 8)}</td>
              <td className="px-3 py-2 text-text-secondary">{c.branch_name}</td>
              <td className="px-3 py-2 text-text-primary truncate max-w-[300px]">{c.commit_message ?? "—"}</td>
              <td className="px-3 py-2 text-right font-mono">
                <span className={c.ai_percentage > 50 ? "text-indigo-400" : "text-emerald-400"}>
                  {c.ai_percentage.toFixed(1)}%
                </span>
              </td>
              <td className="px-3 py-2 text-right font-mono text-purple-400">{c.composer_lines_added ?? 0}</td>
              <td className="px-3 py-2 text-right font-mono text-cyan-400">{c.human_lines_added ?? 0}</td>
              <td className="px-3 py-2 text-right font-mono text-emerald-400">+{c.lines_added ?? 0}</td>
              <td className="px-3 py-2 text-right font-mono text-red-400">-{c.lines_deleted ?? 0}</td>
              <td className="px-3 py-2 text-xs text-text-muted whitespace-nowrap">
                {c.commit_date ? formatCommitDate(c.commit_date) : formatEpochMs(c.scored_at)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ConversationsTab({ conversations }: { conversations: { conversation_id: string; title?: string; model?: string; mode?: string; tldr?: string; updated_at: number }[] }) {
  if (conversations.length === 0) {
    return (
      <div className="bg-app-card border border-border rounded-lg p-8 text-center">
        <p className="text-text-muted">No conversation data available yet.</p>
        <p className="text-xs text-text-muted mt-1">Cursor populates this data as you use it over time.</p>
      </div>
    );
  }
  return (
    <div className="space-y-3">
      {conversations.map((c) => (
        <div key={c.conversation_id} className="bg-app-card border border-border rounded-lg p-4 hover:bg-app-card-hover">
          <div className="flex items-center justify-between mb-1">
            <span className="font-medium text-text-primary">{c.title ?? "Untitled"}</span>
            <div className="flex items-center gap-2">
              {c.model && <span className="text-xs px-2 py-0.5 rounded bg-indigo-500/20 text-indigo-400">{c.model}</span>}
              {c.mode && <span className="text-xs px-2 py-0.5 rounded bg-app-card-hover text-text-muted">{c.mode}</span>}
            </div>
          </div>
          {c.tldr && <p className="text-sm text-text-secondary">{c.tldr}</p>}
          <div className="text-xs text-text-muted mt-2">{formatEpochMs(c.updated_at)}</div>
        </div>
      ))}
    </div>
  );
}

// ── Not Connected State ─────────────────────────────────────────────────────

function NotConnected({ status, onSignIn }: { status: ConnectionStatus; onSignIn: () => void }) {
  const isCursorInstalled = !status.error?.includes("not installed");

  return (
    <div className="flex items-center justify-center py-20">
      <div className="text-center max-w-md">
        <div className="text-4xl mb-4">&#8862;</div>
        <h2 className="text-sm font-semibold text-text-primary mb-2">Cursor Not Connected</h2>

        {!isCursorInstalled ? (
          <>
            <p className="text-xs text-text-muted mb-4">
              Cursor IDE is not installed. Install it to see detailed analytics.
            </p>
            <a
              href="https://cursor.com"
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1.5 px-4 py-2 bg-accent-blue text-white rounded-lg text-xs font-medium hover:bg-accent-blue/90"
            >
              Download Cursor
              <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
              </svg>
            </a>
          </>
        ) : (
          <>
            <p className="text-xs text-text-muted mb-4">
              {status.error || "We auto-detect your session from Cursor's local data. If auto-detect failed, you can sign in manually."}
            </p>

            <div className="flex flex-col items-center gap-3">
              <button
                onClick={onSignIn}
                className="inline-flex items-center gap-2 px-5 py-2.5 bg-accent-blue text-white rounded-lg text-sm font-medium hover:bg-accent-blue/90 transition-colors"
              >
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M11 16l-4-4m0 0l4-4m-4 4h14m-5 4v1a3 3 0 01-3 3H6a3 3 0 01-3-3V7a3 3 0 013-3h7a3 3 0 013 3v1" />
                </svg>
                Sign in to get insights
              </button>

              <p className="text-[10px] text-text-muted max-w-xs">
                This will open Cursor's login page. After signing in, come back and click Refresh.
                Your session token is stored securely in your OS keychain.
              </p>
            </div>
          </>
        )}
      </div>
    </div>
  );
}

// ── Error Boundary ──────────────────────────────────────────────────────────

import { Component, type ErrorInfo, type ReactNode } from "react";

class AnalyticsErrorBoundary extends Component<{ children: ReactNode }, { error: Error | null }> {
  constructor(props: { children: ReactNode }) {
    super(props);
    this.state = { error: null };
  }
  static getDerivedStateFromError(error: Error) {
    return { error };
  }
  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error("[CursorV2] React error boundary caught:", error, info);
  }
  render() {
    if (this.state.error) {
      return (
        <div className="p-6">
          <div className="bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-xs">
            <p className="font-medium text-red-400 mb-2">Cursor Analytics crashed</p>
            <pre className="text-red-400/70 whitespace-pre-wrap">{this.state.error.message}</pre>
            <pre className="text-red-400/50 mt-2 whitespace-pre-wrap text-[10px]">{this.state.error.stack}</pre>
            <button
              onClick={() => this.setState({ error: null })}
              className="mt-3 px-3 py-1.5 bg-red-500/20 rounded text-red-300 hover:bg-red-500/30"
            >
              Retry
            </button>
          </div>
        </div>
      );
    }
    return this.props.children;
  }
}

// ── Time ago helper ─────────────────────────────────────────────────────────

function timeAgo(isoString: string | null): string {
  if (!isoString) return "";
  const diff = Date.now() - new Date(isoString).getTime();
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  return `${hours}h ago`;
}

// ── ClickHouse data helpers ─────────────────────────────────────────────────

function chData(resp: ClickHouseResponse | null | undefined): Record<string, unknown>[] {
  return resp?.data ?? [];
}

function chNum(row: Record<string, unknown>, key: string): number {
  const v = row[key];
  if (v == null) return 0;
  if (typeof v === "number") return v;
  return Number(v) || 0;
}

function chStr(row: Record<string, unknown>, key: string): string {
  const v = row[key];
  if (v == null) return "";
  return String(v);
}

// ── Loading Skeleton ─────────────────────────────────────────────────────────

function CursorDashboardSkeleton() {
  return (
    <div className="h-full overflow-y-auto">
      <div className="px-6 py-6">
        <div className="flex items-center justify-between mb-6">
          <div>
            <div className="animate-pulse bg-[#2a2b36] rounded h-5 w-52 mb-2" />
            <div className="animate-pulse bg-[#2a2b36] rounded h-3 w-72" />
          </div>
          <div className="flex gap-2">
            {[1,2,3,4].map(i => <div key={i} className="animate-pulse bg-[#2a2b36] rounded h-7 w-10" />)}
          </div>
        </div>
        {/* Account section */}
        <div className="mb-6">
          <div className="animate-pulse bg-[#2a2b36] rounded h-3 w-32 mb-3" />
          <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
            <div className="grid grid-cols-4 gap-4">
              {[1,2,3,4].map(i => (
                <div key={i}><div className="animate-pulse bg-[#2a2b36] rounded h-3 w-16 mb-1.5" /><div className="animate-pulse bg-[#2a2b36] rounded h-4 w-28" /></div>
              ))}
            </div>
          </div>
        </div>
        {/* Stat cards */}
        <div className="mb-6">
          <div className="animate-pulse bg-[#2a2b36] rounded h-3 w-28 mb-3" />
          <div className="grid grid-cols-4 gap-3">
            {[1,2,3,4].map(i => (
              <div key={i} className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] px-4 py-3">
                <div className="animate-pulse bg-[#2a2b36] rounded h-3 w-20 mb-2" />
                <div className="animate-pulse bg-[#2a2b36] rounded h-6 w-16" />
              </div>
            ))}
          </div>
        </div>
        {/* Chart */}
        <div className="mb-6">
          <div className="animate-pulse bg-[#2a2b36] rounded h-3 w-40 mb-3" />
          <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4 space-y-3">
            {[80,60,90,40,70].map((w,i) => (
              <div key={i} className="flex items-end gap-1">
                <div className="animate-pulse bg-[#2a2b36] rounded h-4 w-8" />
                <div className="animate-pulse bg-[#2a2b36] rounded h-4" style={{ width: `${w}%` }} />
              </div>
            ))}
          </div>
        </div>
        {/* Table */}
        <div className="mb-6">
          <div className="animate-pulse bg-[#2a2b36] rounded h-3 w-32 mb-3" />
          <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-hidden">
            {[1,2,3,4,5].map(i => (
              <div key={i} className="flex gap-4 px-3 py-2.5 border-b border-[#1e1f2a]">
                {[80,30,30,30,40].map((w,j) => <div key={j} className="animate-pulse bg-[#2a2b36] rounded h-3" style={{ width: w }} />)}
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Main Page ───────────────────────────────────────────────────────────────

function CursorAnalyticsV2Inner() {
  console.log("[CursorV2] Component rendering");
  const [status, setStatus] = useState<ConnectionStatus | null>(null);
  const [overview, setOverview] = useState<Overview | null>(null);
  const [events, setEvents] = useState<UsageEventsPage | null>(null);
  const [aiCommits, setAiCommits] = useState<ClickHouseResponse | null>(null);
  const [modelUsage, setModelUsage] = useState<{ data?: { date?: string; model_breakdown?: Record<string, Record<string, unknown>> }[] } | null>(null);
  const [timeRange, setTimeRange] = useState("30d");
  const [loading, setLoading] = useState(true);
  const [eventsPage, setEventsPage] = useState(1);
  const [lastRefreshed, setLastRefreshed] = useState<string | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [teamSortBy, setTeamSortBy] = useState<"name" | "role">("role");
  const [customDateFrom, setCustomDateFrom] = useState("");
  const [customDateTo, setCustomDateTo] = useState("");
  const [showDatePicker, setShowDatePicker] = useState(false);

  // AI Code Attribution store
  const {
    commits: attrCommits,
    summary: attrSummary,
    fileTypes: attrFileTypes,
    conversations: attrConversations,
    modelBreakdown: attrModelBreakdown,
    loading: attrLoading,
    fetchAll: attrFetchAll,
  } = useAiTrackingStore();
  const [attrTab, setAttrTab] = useState<"overview" | "commits" | "conversations">("overview");

  // Fetch AI attribution data on mount
  useEffect(() => {
    attrFetchAll();
  }, [attrFetchAll]);

  // Update the "X ago" display every 30s
  const [, setTick] = useState(0);
  useEffect(() => {
    const interval = setInterval(() => setTick((t) => t + 1), 30000);
    return () => clearInterval(interval);
  }, []);

  const loadData = useCallback(async (range: string, forceRefresh = false) => {
    console.log("[CursorV2] loadData called, range:", range, "forceRefresh:", forceRefresh);
    setLoading(true);
    setLoadError(null);
    try {
      const s = await invoke<ConnectionStatus>("get_cursor_v2_connection_status").catch((err) => {
        console.error("[CursorV2] connection_status error:", err);
        return {
          connected: false, connection_method: "none",
          email: null, plan: null, team_name: null, team_id: null,
          error: String(err),
        } as ConnectionStatus;
      });

      setStatus(s);

      if (!s.connected) {
        setLoading(false);
        return;
      }

      const [o, e] = await Promise.all([
        invoke<Overview>("get_cursor_v2_overview", { timeRange: range, forceRefresh }).catch((err) => {
          console.error("[CursorV2] overview error:", err);
          return null;
        }),
        invoke<UsageEventsPage>("get_cursor_v2_usage_events", { timeRange: range, page: 1, pageSize: 50, forceRefresh }).catch((err) => {
          console.error("[CursorV2] events error:", err);
          return null;
        }),
      ]);
      setOverview(o);
      setEvents(e);
      setEventsPage(1);
      setLastRefreshed(new Date().toISOString());

      // Load chart data in background
      invoke<ClickHouseResponse>("get_cursor_v2_ai_commits", { timeRange: range, forceRefresh })
        .then((d) => setAiCommits(d))
        .catch((err) => console.error("[CursorV2] ai_commits error:", err));
      invoke<typeof modelUsage>("get_cursor_v2_model_usage", { timeRange: range, forceRefresh })
        .then((d) => setModelUsage(d))
        .catch((err) => console.error("[CursorV2] model_usage error:", err));

      invoke<{ last_refreshed: string | null }>("get_cursor_v2_cache_info")
        .then((info) => { if (info.last_refreshed) setLastRefreshed(info.last_refreshed); })
        .catch(() => {});
    } catch (err) {
      console.error("[CursorV2] FATAL load error:", err);
      setLoadError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  const handleSignIn = useCallback(async () => {
    try {
      await openUrl("https://cursor.com/loginDeepPage");
    } catch {
      window.open("https://cursor.com/loginDeepPage", "_blank");
    }
  }, []);

  const loadMoreEvents = useCallback(async () => {
    const nextPage = eventsPage + 1;
    try {
      const e = await invoke<UsageEventsPage>("get_cursor_v2_usage_events", { timeRange, page: nextPage, pageSize: 50 });
      setEvents(prev => prev ? { ...e, events: [...prev.events, ...e.events] } : e);
      setEventsPage(nextPage);
    } catch { /* ignore */ }
  }, [eventsPage, timeRange]);

  // Load data on mount only (timeRange changes handled by button clicks)
  useEffect(() => { loadData(timeRange, false); }, [loadData]); // eslint-disable-line react-hooks/exhaustive-deps

  // Auto-refresh data every 5 minutes
  useEffect(() => {
    const interval = setInterval(() => {
      console.log("[CursorV2] Auto-refresh triggered");
      loadData(timeRange, true);
    }, 5 * 60 * 1000);
    return () => clearInterval(interval);
  }, [loadData, timeRange]);

  // ── Early returns (AFTER all hooks) ───────────────────────────────────

  if (loading && !status) {
    return <CursorDashboardSkeleton />;
  }

  if (loadError) {
    return (
      <div className="p-6">
        <div className="bg-red-500/10 border border-red-500/20 rounded-lg p-4 text-xs text-red-400">
          <p className="font-medium mb-1">Failed to load Cursor analytics</p>
          <p className="text-red-400/70">{loadError}</p>
          <button onClick={() => loadData(timeRange, true)} className="mt-3 px-3 py-1.5 bg-red-500/20 rounded text-red-300 hover:bg-red-500/30">
            Retry
          </button>
        </div>
      </div>
    );
  }

  if (!status?.connected) {
    return <NotConnected status={status!} onSignIn={handleSignIn} />;
  }

  // ── Derived data ──────────────────────────────────────────────────────

  const auth = overview?.auth;
  const stripe = overview?.stripe;
  const usageSummary = overview?.usage_summary;
  const plan = usageSummary?.individualUsage?.plan;
  const onDemand = usageSummary?.individualUsage?.onDemand;
  const teamOnDemand = usageSummary?.teamUsage?.onDemand;
  const hardLimit = overview?.hard_limit;
  const ai = overview?.ai_commits;
  const team = overview?.team;
  const sessions = overview?.sessions;

  // Model aggregated totals
  const totalModelRequests = (overview?.model_aggregated ?? []).reduce((s, m) => s + (m.total_requests ?? 0), 0);

  // Composer stats — ClickHouse snake_case field names
  const composerData = chData(overview?.composer_stats);
  type ComposerTotals = { suggested: number; accepted: number; rejected: number; greenSuggested: number; greenAccepted: number; greenRejected: number; redSuggested: number; redAccepted: number; redRejected: number; linesSuggested: number; linesAccepted: number };
  const composerTotals = composerData.reduce<ComposerTotals>(
    (acc, row) => ({
      suggested: acc.suggested + chNum(row, "total_suggested_diffs"),
      accepted: acc.accepted + chNum(row, "total_accepted_diffs"),
      rejected: acc.rejected + chNum(row, "total_rejected_diffs"),
      greenSuggested: acc.greenSuggested + chNum(row, "total_green_lines_suggested"),
      greenAccepted: acc.greenAccepted + chNum(row, "total_green_lines_accepted"),
      greenRejected: acc.greenRejected + chNum(row, "total_green_lines_rejected"),
      redSuggested: acc.redSuggested + chNum(row, "total_red_lines_suggested"),
      redAccepted: acc.redAccepted + chNum(row, "total_red_lines_accepted"),
      redRejected: acc.redRejected + chNum(row, "total_red_lines_rejected"),
      linesSuggested: acc.linesSuggested + chNum(row, "total_lines_suggested"),
      linesAccepted: acc.linesAccepted + chNum(row, "total_lines_accepted"),
    }),
    { suggested: 0, accepted: 0, rejected: 0, greenSuggested: 0, greenAccepted: 0, greenRejected: 0, redSuggested: 0, redAccepted: 0, redRejected: 0, linesSuggested: 0, linesAccepted: 0 }
  );

  // Tab stats — different field names from composer: total_suggestions/total_accepts/total_rejects
  const tabData = chData(overview?.tab_stats);
  type TabTotals = { shown: number; accepted: number; rejected: number; greenSuggested: number; greenAccepted: number; greenRejected: number; redSuggested: number; redAccepted: number; redRejected: number };
  const tabTotals = tabData.reduce<TabTotals>(
    (acc, row) => ({
      shown: acc.shown + chNum(row, "total_suggestions"),
      accepted: acc.accepted + chNum(row, "total_accepts"),
      rejected: acc.rejected + chNum(row, "total_rejects"),
      greenSuggested: acc.greenSuggested + chNum(row, "total_green_lines_suggested"),
      greenAccepted: acc.greenAccepted + chNum(row, "total_green_lines_accepted"),
      greenRejected: acc.greenRejected + chNum(row, "total_green_lines_rejected"),
      redSuggested: acc.redSuggested + chNum(row, "total_red_lines_suggested"),
      redAccepted: acc.redAccepted + chNum(row, "total_red_lines_accepted"),
      redRejected: acc.redRejected + chNum(row, "total_red_lines_rejected"),
    }),
    { shown: 0, accepted: 0, rejected: 0, greenSuggested: 0, greenAccepted: 0, greenRejected: 0, redSuggested: 0, redAccepted: 0, redRejected: 0 }
  );

  // Top files — GROUP BY file_extension, SUM values across all dates
  const topFilesRaw = chData(overview?.top_files);
  const topFilesGrouped: Record<string, { file_extension: string; total_files_touched: number; total_accepts: number; total_rejects: number; total_lines_accepted: number; total_lines_rejected: number; total_lines_suggested: number }> = {};
  for (const row of topFilesRaw) {
    const ext = chStr(row, "file_extension");
    if (!ext) continue;
    if (!topFilesGrouped[ext]) {
      topFilesGrouped[ext] = { file_extension: ext, total_files_touched: 0, total_accepts: 0, total_rejects: 0, total_lines_accepted: 0, total_lines_rejected: 0, total_lines_suggested: 0 };
    }
    const g = topFilesGrouped[ext];
    g.total_files_touched += chNum(row, "total_files_touched");
    g.total_accepts += chNum(row, "total_accepts");
    g.total_rejects += chNum(row, "total_rejects");
    g.total_lines_accepted += chNum(row, "total_lines_accepted");
    g.total_lines_rejected += chNum(row, "total_lines_rejected");
    g.total_lines_suggested += chNum(row, "total_lines_suggested");
  }
  const topFilesData = Object.values(topFilesGrouped);

  // Request breakdown — mixed casing: event_date, agent_requests, composer_requests, chat, bugBot, cmdK, etc.
  const requestBreakdownData = chData(overview?.request_breakdown);
  const REQUEST_SKIP_KEYS = new Set(["event_date", "date", "day"]);
  const requestTotals: Record<string, number> = {};
  for (const row of requestBreakdownData) {
    for (const key of Object.keys(row)) {
      if (!REQUEST_SKIP_KEYS.has(key)) {
        requestTotals[key] = (requestTotals[key] ?? 0) + chNum(row, key);
      }
    }
  }

  // AI commits by repo
  const repoData = chData(overview?.ai_commits_by_repo);

  // Billing cycle
  const cycleStart = usageSummary?.billingCycleStart ? formatIsoDate(usageSummary.billingCycleStart) : "";
  const cycleEnd = usageSummary?.billingCycleEnd ? formatIsoDate(usageSummary.billingCycleEnd) : "";

  return (
    <div className="h-full overflow-y-auto relative">
      {/* Top loading bar */}
      {loading && (
        <div className="sticky top-0 z-50 w-full">
          <div className="h-0.5 bg-[#0e0f13] w-full overflow-hidden">
            <div className="h-full bg-accent-blue animate-[cursor-loading-bar_1.5s_ease-in-out_infinite] w-1/3 rounded-full" />
          </div>
          <style>{`@keyframes cursor-loading-bar { 0% { transform: translateX(-100%); } 100% { transform: translateX(400%); } }`}</style>
        </div>
      )}

      <div className="px-6 py-6">
        {/* Header */}
        <div className="flex items-center justify-between mb-6">
          <div>
            <h1 className="text-lg font-semibold text-text-primary flex items-center gap-2">
              Cursor Analytics
              {loading && <span className="inline-block w-2 h-2 rounded-full bg-accent-blue animate-pulse" title="Refreshing..." />}
            </h1>
            <p className="text-xs text-text-muted">
              {auth?.email}
              {stripe?.membershipType && <span> &middot; <span className="capitalize">{stripe?.isTeamMember ? "Team" : stripe.membershipType}</span></span>}
              {team?.name && <span> &middot; {team.name}</span>}
              <span className="text-emerald-500"> &middot; {status.connection_method === "auto-detect" ? "Auto-detected" : "Connected"}</span>
            </p>
          </div>
          <div className="flex items-center gap-2 flex-wrap">
            {["1d", "7d", "30d", "90d", "all"].map((r) => (
              <button
                key={r}
                onClick={() => {
                  setTimeRange(r);
                  setShowDatePicker(false);
                  loadData(r, false); // Use cache for timeline buttons
                }}
                className={`px-2.5 py-1 rounded text-[11px] font-medium transition-colors ${
                  timeRange === r && !showDatePicker ? "bg-accent-blue text-white" : "bg-[#1a1b23] text-text-secondary hover:bg-[#22232e]"
                }`}
              >
                {r === "all" ? "All" : r}
              </button>
            ))}
            <button
              onClick={() => setShowDatePicker(!showDatePicker)}
              className={`px-2.5 py-1 rounded text-[11px] font-medium transition-colors flex items-center gap-1 ${
                showDatePicker ? "bg-accent-blue text-white" : "bg-[#1a1b23] text-text-secondary hover:bg-[#22232e]"
              }`}
            >
              <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
              </svg>
              Custom
            </button>
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
            <button onClick={() => loadData(timeRange, true)} className={`p-1.5 rounded transition-colors ${loading ? "text-accent-blue bg-accent-blue/10" : "text-text-muted hover:text-text-primary hover:bg-[#1a1b23]"}`} title="Force refresh (bypass cache)">
              <svg className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`} fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
              </svg>
            </button>
          </div>
        </div>

        {/* Date picker row */}
        {showDatePicker && (
          <div className="flex items-center gap-2 mb-4 px-1">
            <label className="text-[10px] text-text-muted uppercase tracking-wider">From</label>
            <input
              type="date"
              value={customDateFrom}
              onChange={(e) => setCustomDateFrom(e.target.value)}
              className="bg-[#1a1b23] border border-[#2a2b36] rounded px-2 py-1 text-xs text-text-primary focus:outline-none focus:border-accent-blue"
            />
            <label className="text-[10px] text-text-muted uppercase tracking-wider">To</label>
            <input
              type="date"
              value={customDateTo}
              onChange={(e) => setCustomDateTo(e.target.value)}
              className="bg-[#1a1b23] border border-[#2a2b36] rounded px-2 py-1 text-xs text-text-primary focus:outline-none focus:border-accent-blue"
            />
            <button
              onClick={() => {
                if (customDateFrom && customDateTo) {
                  const from = new Date(customDateFrom);
                  const to = new Date(customDateTo);
                  const diffDays = Math.ceil((to.getTime() - from.getTime()) / (1000 * 60 * 60 * 24));
                  if (diffDays > 0) {
                    const range = `${diffDays}d`;
                    setTimeRange(range);
                    loadData(range, false);
                  }
                }
              }}
              disabled={!customDateFrom || !customDateTo}
              className="px-3 py-1 rounded text-[11px] font-medium bg-accent-blue text-white hover:bg-accent-blue/90 disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
            >
              Apply
            </button>
          </div>
        )}

        {/* Data sections — fade during refresh */}
        <div className={`transition-opacity duration-300 ${loading ? "opacity-60" : "opacity-100"}`}>

        {/* ── Section 1: Account & Billing ──────────────────────────────── */}
        <Section title="Account & Billing">
          <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-xs">
              <div>
                <span className="text-text-muted block mb-0.5">Email</span>
                <div className="text-text-primary font-medium truncate">{auth?.email || "-"}</div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Name</span>
                <div className="text-text-primary font-medium">{auth?.name || "-"}</div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Member Since</span>
                <div className="text-text-primary font-medium">{auth?.created_at ? formatIsoDate(auth.created_at) : "-"}</div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Account ID</span>
                <div className="text-text-primary font-mono text-[10px]">{auth?.id ?? auth?.sub ?? "-"}</div>
              </div>
            </div>

            <div className="border-t border-[#2a2b36] mt-3 pt-3 grid grid-cols-2 md:grid-cols-4 gap-4 text-xs">
              <div>
                <span className="text-text-muted block mb-0.5">Plan</span>
                <div className="flex items-center gap-1.5">
                  {stripe?.isTeamMember ? (
                    <Badge text="TEAM" color="bg-blue-500/20 text-blue-400" />
                  ) : stripe?.membershipType === "enterprise" ? (
                    <Badge text="ENTERPRISE" color="bg-emerald-500/20 text-emerald-400" />
                  ) : (
                    <Badge text={(stripe?.membershipType ?? "free").toUpperCase()} color="bg-emerald-500/20 text-emerald-400" />
                  )}
                </div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Team</span>
                <div className="text-text-primary font-medium">{team?.name ?? "-"}</div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Role</span>
                <div className="text-text-primary font-medium">{formatRole(team?.role)}</div>
              </div>
            </div>

          </div>
        </Section>

        {/* ── Section 2: Plan Usage ─────────────────────────────────────── */}
        <Section title="Plan Usage - This Cycle">
          <div className="space-y-3">
            {/* Multi-segment usage bar */}
            {(plan || onDemand) && (() => {
              const includedVal = plan?.breakdown?.included ?? plan?.limit ?? 0;
              const bonusVal = plan?.breakdown?.bonus ?? 0;
              const onDemandVal = onDemand?.used ?? 0;
              const segments: UsageSegment[] = [];
              if (includedVal > 0) segments.push({ label: "included", value: includedVal, color: "bg-blue-500", textColor: "text-blue-400" });
              if (bonusVal > 0) segments.push({ label: "bonus", value: bonusVal, color: "bg-emerald-500", textColor: "text-emerald-400" });
              if (onDemandVal > 0) segments.push({ label: "on-demand", value: onDemandVal, color: "bg-amber-500", textColor: "text-amber-400" });
              return (
                <MultiSegmentUsageBar
                  segments={segments}
                  sub={cycleEnd ? `Billing cycle: ${cycleStart} - ${cycleEnd}` : undefined}
                />
              );
            })()}

            {/* Plan breakdown cards */}
            <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
              <StatCard
                label="Included"
                value={cents(plan?.breakdown?.included ?? plan?.limit)}
                sub="Plan allowance"
              />
              <StatCard
                label="Bonus Credits"
                value={cents(plan?.breakdown?.bonus)}
                sub="Extra credits"
                color="text-amber-400"
              />
              <StatCard
                label="Total Budget"
                value={cents(plan?.breakdown?.total ?? (plan?.limit ?? 0) + (plan?.breakdown?.bonus ?? 0))}
                sub="Included + Bonus"
                color="text-emerald-400"
              />
              <StatCard
                label="Used"
                value={cents(plan?.used)}
                sub={plan?.totalPercentUsed != null ? `${plan.totalPercentUsed.toFixed(1)}% of total` : undefined}
              />
            </div>

            {/* Usage percentages */}
            <div className="grid grid-cols-2 gap-3">
              <StatCard label="API Usage" value={pct(plan?.apiPercentUsed)} />
              <StatCard label="Total Usage" value={pct(plan?.totalPercentUsed)} color={
                (plan?.totalPercentUsed ?? 0) > 90 ? "text-red-400" :
                (plan?.totalPercentUsed ?? 0) > 70 ? "text-amber-400" : "text-emerald-400"
              } />
            </div>

            {/* On-demand + Team on-demand */}
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] px-4 py-3">
                <div className="flex items-center gap-2 mb-1">
                  <div className="text-[10px] text-text-muted uppercase tracking-wider">Individual On-Demand</div>
                  {onDemand?.enabled && <Badge text="ENABLED" color="bg-emerald-500/20 text-emerald-400" />}
                  {onDemand?.limit == null && onDemand?.enabled && <Badge text="UNLIMITED" color="bg-purple-500/20 text-purple-400" />}
                </div>
                <div className="text-xl font-semibold text-text-primary">{cents(onDemand?.used)}</div>
                {onDemand?.limit != null && (
                  <div className="text-[11px] text-text-muted mt-0.5">Limit: {cents(onDemand.limit)}</div>
                )}
              </div>

              {teamOnDemand && (
                <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] px-4 py-3">
                  <div className="flex items-center gap-2 mb-1">
                    <div className="text-[10px] text-text-muted uppercase tracking-wider">Team On-Demand</div>
                    {teamOnDemand.enabled && <Badge text="ENABLED" color="bg-emerald-500/20 text-emerald-400" />}
                  </div>
                  <div className="text-xl font-semibold text-text-primary">
                    {cents(teamOnDemand.used)}
                    {hardLimit?.hardLimit != null && (
                      <span className="text-sm text-text-muted font-normal"> / {cents((hardLimit.hardLimit) * 100)}</span>
                    )}
                  </div>
                  {hardLimit && (
                    <div className="flex items-center gap-1.5 mt-0.5">
                      <span className="text-[11px] text-text-muted">
                        Hard limit: {cents((hardLimit.hardLimit ?? 0) * 100)}
                      </span>
                      {hardLimit.isDynamicTeamLimit && <Badge text="DYNAMIC" color="bg-cyan-500/20 text-cyan-400" />}
                    </div>
                  )}
                </div>
              )}
            </div>
          </div>
          <div className="flex justify-end mt-2">
            <a
              href="https://www.cursor.com/pricing"
              target="_blank"
              rel="noopener noreferrer"
              className="text-xs text-text-muted hover:text-blue-400 transition-colors inline-flex items-center gap-1"
              onClick={async (e) => {
                e.preventDefault();
                await openUrl("https://www.cursor.com/pricing");
              }}
            >
              View pricing
              <svg className="w-3 h-3" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
                <path strokeLinecap="round" strokeLinejoin="round" d="M10 6H6a2 2 0 00-2 2v10a2 2 0 002 2h10a2 2 0 002-2v-4M14 4h6m0 0v6m0-6L10 14" />
              </svg>
            </a>
          </div>
        </Section>

        {/* ── Section 3: AI Impact Stats ────────────────────────────────── */}
        {ai && (
          <Section title="AI Impact">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
              <StatCard label="AI Share" value={`${(ai.ai_impact_percentage ?? 0).toFixed(1)}%`} color="text-emerald-400" />
              <StatCard label="Total Commits" value={`${ai.total_commits ?? 0}`} sub={`${ai.unique_repos ?? 0} repos`} />
              <StatCard label="AI Lines Added" value={formatNum(ai.ai_lines_added)} sub={`of ${formatNum(ai.total_lines_added)} total`} />
              <StatCard label="Avg AI Lines/Commit" value={`${(ai.avg_ai_lines_per_commit ?? 0).toFixed(0)}`} />
            </div>
          </Section>
        )}

        {/* ── Section 4: AI Commits by Repository ──────────────────────── */}
        {repoData.length > 0 && (
          <Section title="AI Commits by Repository">
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-hidden">
              <table className="w-full text-xs">
                <thead>
                  <tr className="border-b border-[#2a2b36] text-text-muted">
                    <th className="text-left px-3 py-2 font-medium">Repository</th>
                    <th className="text-right px-3 py-2 font-medium">Commits</th>
                    <th className="text-right px-3 py-2 font-medium">AI Impact</th>
                    <th className="text-right px-3 py-2 font-medium">AI Lines</th>
                    <th className="text-right px-3 py-2 font-medium">Non-AI Lines</th>
                  </tr>
                </thead>
                <tbody>
                  {[...repoData]
                    .sort((a, b) => chNum(b, "ai_impact_percentage") - chNum(a, "ai_impact_percentage"))
                    .map((row, i) => (
                    <tr key={i} className="border-b border-[#1e1f2a] hover:bg-[#22232e]">
                      <td className="px-3 py-2 text-text-primary font-medium truncate max-w-[200px]">{chStr(row, "repo_name") || chStr(row, "repository")}</td>
                      <td className="px-3 py-2 text-right text-text-secondary font-mono">{chNum(row, "total_commits")}</td>
                      <td className="px-3 py-2 text-right">
                        <span className="text-emerald-400 font-mono">{chNum(row, "ai_impact_percentage").toFixed(1)}%</span>
                      </td>
                      <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(chNum(row, "ai_lines_added"))}</td>
                      <td className="px-3 py-2 text-right text-text-muted font-mono">{formatNum(chNum(row, "non_ai_lines_added"))}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </Section>
        )}

        {/* ── Section 5: Model Usage ────────────────────────────────────── */}
        {overview?.model_aggregated && overview.model_aggregated.length > 0 && (
          <Section title="Model Usage">
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3 items-stretch">
              {/* Donut chart */}
              <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4 h-full">
                <div className="h-full min-h-[360px]">
                  <ResponsiveContainer width="100%" height="100%">
                    <PieChart>
                      <Pie
                        data={overview.model_aggregated.map((m) => ({
                          name: shortModel(m.model_intent),
                          value: m.total_requests || 0,
                        }))}
                        dataKey="value"
                        nameKey="name"
                        cx="50%"
                        cy="43%"
                        innerRadius="52%"
                        outerRadius="84%"
                        paddingAngle={2}
                      >
                        {overview.model_aggregated.map((_, i) => (
                          <Cell key={i} fill={COLORS[i % COLORS.length]} />
                        ))}
                      </Pie>
                      <Tooltip contentStyle={TOOLTIP_STYLE} itemStyle={{ color: "#e8e9ed" }} />
                      <Legend
                        wrapperStyle={{ fontSize: "10px", color: "#9394a1" }}
                        formatter={(v) => <span className="text-text-secondary">{v}</span>}
                      />
                    </PieChart>
                  </ResponsiveContainer>
                </div>
              </div>

              {/* Model table */}
              <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-hidden h-full">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="border-b border-[#2a2b36] text-text-muted">
                      <th className="text-left px-3 py-2 font-medium">Model</th>
                      <th className="text-right px-3 py-2 font-medium">Requests</th>
                      <th className="text-right px-3 py-2 font-medium">Users</th>
                      <th className="text-right px-3 py-2 font-medium">% Total</th>
                    </tr>
                  </thead>
                  <tbody>
                    {[...overview.model_aggregated]
                      .sort((a, b) => (b.total_requests ?? 0) - (a.total_requests ?? 0))
                      .map((m, i) => (
                      <tr key={i} className="border-b border-[#1e1f2a] hover:bg-[#22232e]">
                        <td className="px-3 py-2 text-text-primary">
                          <div className="flex items-center gap-1.5">
                            <div className="w-2 h-2 rounded-full" style={{ background: COLORS[i % COLORS.length] }} />
                            {shortModel(m.model_intent)}
                          </div>
                        </td>
                        <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(m.total_requests)}</td>
                        <td className="px-3 py-2 text-right text-text-secondary font-mono">{m.total_unique_users ?? 0}</td>
                        <td className="px-3 py-2 text-right text-text-muted font-mono">
                          {totalModelRequests > 0 ? `${((m.total_requests ?? 0) / totalModelRequests * 100).toFixed(1)}%` : "-"}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          </Section>
        )}

        {/* ── Section 6: AI Commits Over Time ──────────────────────────── */}
        {aiCommits?.data && aiCommits.data.length > 0 && (
          <Section title="AI Commits Over Time">
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
              <ResponsiveContainer width="100%" height={250}>
                <BarChart data={aiCommits.data.map((d) => ({
                  date: (String(d.date ?? "")).slice(5),
                  ai_ide: Number(d.ai_ide_lines_added ?? 0),
                  ai_cli: Number(d.ai_cli_lines_added ?? 0),
                  ai_cloud: Number(d.ai_cloud_lines_added ?? 0),
                  non_ai: Number(d.non_ai_lines_added ?? 0),
                }))}>
                  <CartesianGrid strokeDasharray="3 3" stroke="#2a2b36" />
                  <XAxis dataKey="date" tick={{ fontSize: 10, fill: "#9394a1" }} />
                  <YAxis tick={{ fontSize: 10, fill: "#9394a1" }} />
                  <Tooltip contentStyle={TOOLTIP_STYLE} />
                  <Legend wrapperStyle={{ fontSize: "10px" }} />
                  <Bar dataKey="ai_ide" stackId="a" fill="#22c55e" name="IDE" />
                  <Bar dataKey="ai_cli" stackId="a" fill="#06b6d4" name="CLI" />
                  <Bar dataKey="ai_cloud" stackId="a" fill="#8b5cf6" name="Cloud Agent" />
                  <Bar dataKey="non_ai" stackId="a" fill="#374151" name="Non-AI" />
                </BarChart>
              </ResponsiveContainer>
            </div>
          </Section>
        )}

        {/* ── Section 7: Agent Edits (Composer) Stats ──────────────────── */}
        {composerData.length > 0 && (
          <Section title="Agent Edits (Composer)">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-3 mb-3">
              <StatCard
                label="Diffs Suggested"
                value={formatNum(composerTotals.suggested)}
              />
              <StatCard
                label="Diffs Accepted"
                value={formatNum(composerTotals.accepted)}
                color="text-emerald-400"
              />
              <StatCard
                label="Acceptance Rate"
                value={composerTotals.suggested > 0 ? `${(composerTotals.accepted / composerTotals.suggested * 100).toFixed(1)}%` : "0%"}
                color="text-accent-blue"
              />
              <StatCard
                label="Lines Accepted"
                value={formatNum(composerTotals.greenAccepted + composerTotals.redAccepted)}
                sub={`+${formatNum(composerTotals.greenAccepted)} / -${formatNum(composerTotals.redAccepted)}`}
              />
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-3 mb-3">
              <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] px-4 py-3">
                <div className="text-[10px] text-text-muted uppercase tracking-wider mb-2">Green Lines (Additions)</div>
                <div className="grid grid-cols-3 gap-2 text-xs">
                  <div><span className="text-text-muted">Suggested</span><div className="text-text-primary font-mono">{formatNum(composerTotals.greenSuggested)}</div></div>
                  <div><span className="text-text-muted">Accepted</span><div className="text-emerald-400 font-mono">{formatNum(composerTotals.greenAccepted)}</div></div>
                  <div><span className="text-text-muted">Rejected</span><div className="text-red-400 font-mono">{formatNum(composerTotals.greenRejected)}</div></div>
                </div>
              </div>
              <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] px-4 py-3">
                <div className="text-[10px] text-text-muted uppercase tracking-wider mb-2">Red Lines (Deletions)</div>
                <div className="grid grid-cols-3 gap-2 text-xs">
                  <div><span className="text-text-muted">Suggested</span><div className="text-text-primary font-mono">{formatNum(composerTotals.redSuggested)}</div></div>
                  <div><span className="text-text-muted">Accepted</span><div className="text-emerald-400 font-mono">{formatNum(composerTotals.redAccepted)}</div></div>
                  <div><span className="text-text-muted">Rejected</span><div className="text-red-400 font-mono">{formatNum(composerTotals.redRejected)}</div></div>
                </div>
              </div>
            </div>

            {/* Composer timeseries chart */}
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
              <ResponsiveContainer width="100%" height={200}>
                <AreaChart data={composerData.map((d) => ({
                  date: String(d.event_date ?? d.date ?? "").slice(5),
                  suggested: chNum(d, "total_suggested_diffs"),
                  accepted: chNum(d, "total_accepted_diffs"),
                }))}>
                  <CartesianGrid strokeDasharray="3 3" stroke="#2a2b36" />
                  <XAxis dataKey="date" tick={{ fontSize: 10, fill: "#9394a1" }} />
                  <YAxis tick={{ fontSize: 10, fill: "#9394a1" }} />
                  <Tooltip contentStyle={TOOLTIP_STYLE} />
                  <Legend wrapperStyle={{ fontSize: "10px" }} />
                  <Area type="monotone" dataKey="suggested" stroke="#8b5cf6" fill="#8b5cf6" fillOpacity={0.15} name="Suggested" />
                  <Area type="monotone" dataKey="accepted" stroke="#22c55e" fill="#22c55e" fillOpacity={0.15} name="Accepted" />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </Section>
        )}

        {/* ── Section 8: Tab Completions ────────────────────────────────── */}
        {tabData.length > 0 && (
          <Section title="Tab Completions">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-3 mb-3">
              <StatCard label="Shown" value={formatNum(tabTotals.shown)} />
              <StatCard label="Accepted" value={formatNum(tabTotals.accepted)} color="text-emerald-400" />
              <StatCard
                label="Acceptance Rate"
                value={tabTotals.shown > 0 ? `${(tabTotals.accepted / tabTotals.shown * 100).toFixed(1)}%` : "0%"}
                color="text-accent-blue"
              />
              <StatCard
                label="Lines Accepted"
                value={formatNum(tabTotals.greenAccepted + tabTotals.redAccepted)}
                sub={`+${formatNum(tabTotals.greenAccepted)} / -${formatNum(tabTotals.redAccepted)}`}
              />
            </div>

            <div className="grid grid-cols-1 md:grid-cols-2 gap-3 mb-3">
              <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] px-4 py-3">
                <div className="text-[10px] text-text-muted uppercase tracking-wider mb-2">Green Lines (Additions)</div>
                <div className="grid grid-cols-3 gap-2 text-xs">
                  <div><span className="text-text-muted">Suggested</span><div className="text-text-primary font-mono">{formatNum(tabTotals.greenSuggested)}</div></div>
                  <div><span className="text-text-muted">Accepted</span><div className="text-emerald-400 font-mono">{formatNum(tabTotals.greenAccepted)}</div></div>
                  <div><span className="text-text-muted">Rejected</span><div className="text-red-400 font-mono">{formatNum(tabTotals.greenRejected)}</div></div>
                </div>
              </div>
              <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] px-4 py-3">
                <div className="text-[10px] text-text-muted uppercase tracking-wider mb-2">Red Lines (Deletions)</div>
                <div className="grid grid-cols-3 gap-2 text-xs">
                  <div><span className="text-text-muted">Suggested</span><div className="text-text-primary font-mono">{formatNum(tabTotals.redSuggested)}</div></div>
                  <div><span className="text-text-muted">Accepted</span><div className="text-emerald-400 font-mono">{formatNum(tabTotals.redAccepted)}</div></div>
                  <div><span className="text-text-muted">Rejected</span><div className="text-red-400 font-mono">{formatNum(tabTotals.redRejected)}</div></div>
                </div>
              </div>
            </div>

            {/* Tabs timeseries chart */}
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
              <ResponsiveContainer width="100%" height={200}>
                <AreaChart data={tabData.map((d) => ({
                  date: String(d.event_date ?? d.date ?? "").slice(5),
                  shown: chNum(d, "total_suggestions"),
                  accepted: chNum(d, "total_accepts"),
                }))}>
                  <CartesianGrid strokeDasharray="3 3" stroke="#2a2b36" />
                  <XAxis dataKey="date" tick={{ fontSize: 10, fill: "#9394a1" }} />
                  <YAxis tick={{ fontSize: 10, fill: "#9394a1" }} />
                  <Tooltip contentStyle={TOOLTIP_STYLE} />
                  <Legend wrapperStyle={{ fontSize: "10px" }} />
                  <Area type="monotone" dataKey="shown" stroke="#f59e0b" fill="#f59e0b" fillOpacity={0.15} name="Suggestions" />
                  <Area type="monotone" dataKey="accepted" stroke="#22c55e" fill="#22c55e" fillOpacity={0.15} name="Accepted" />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </Section>
        )}

        {/* ── Section 9: Top File Extensions ───────────────────────────── */}
        {topFilesData.length > 0 && (
          <Section title="Top File Extensions">
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-hidden">
              <table className="w-full text-xs">
                <thead>
                  <tr className="border-b border-[#2a2b36] text-text-muted">
                    <th className="text-left px-3 py-2 font-medium">Extension</th>
                    <th className="text-right px-3 py-2 font-medium">Files</th>
                    <th className="text-right px-3 py-2 font-medium">Accepts</th>
                    <th className="text-right px-3 py-2 font-medium">Rejects</th>
                    <th className="text-right px-3 py-2 font-medium">Lines Accepted</th>
                    <th className="text-right px-3 py-2 font-medium">Lines Suggested</th>
                    <th className="text-right px-3 py-2 font-medium">Accept Rate</th>
                  </tr>
                </thead>
                <tbody>
                  {[...topFilesData]
                    .sort((a, b) => b.total_files_touched - a.total_files_touched)
                    .slice(0, 20)
                    .map((row, i) => {
                      const accepts = row.total_accepts;
                      const rejects = row.total_rejects;
                      const total = accepts + rejects;
                      const rate = total > 0 ? (accepts / total * 100).toFixed(1) : "-";
                      return (
                        <tr key={i} className="border-b border-[#1e1f2a] hover:bg-[#22232e]">
                          <td className="px-3 py-2 text-text-primary font-mono">{row.file_extension}</td>
                          <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(row.total_files_touched)}</td>
                          <td className="px-3 py-2 text-right text-emerald-400 font-mono">{formatNum(accepts)}</td>
                          <td className="px-3 py-2 text-right text-red-400 font-mono">{formatNum(rejects)}</td>
                          <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(row.total_lines_accepted)}</td>
                          <td className="px-3 py-2 text-right text-text-muted font-mono">{formatNum(row.total_lines_suggested)}</td>
                          <td className="px-3 py-2 text-right text-text-secondary font-mono">{rate}{rate !== "-" ? "%" : ""}</td>
                        </tr>
                      );
                    })}
                </tbody>
              </table>
            </div>
          </Section>
        )}

        {/* ── Section 10: Request Breakdown ─────────────────────────────── */}
        {requestBreakdownData.length > 0 && (
          <Section title="Request Breakdown">
            {/* Totals */}
            <div className="grid grid-cols-2 md:grid-cols-3 lg:grid-cols-6 gap-3 mb-3">
              {Object.entries(requestTotals)
                .sort(([, a], [, b]) => b - a)
                .map(([key, val]) => (
                  <StatCard
                    key={key}
                    label={key.replace(/_/g, " ").replace(/([A-Z])/g, " $1").trim()}
                    value={formatNum(val)}
                  />
                ))}
            </div>

            {/* Stacked bar chart */}
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
              <ResponsiveContainer width="100%" height={250}>
                <BarChart data={requestBreakdownData.map((d) => {
                  const mapped: Record<string, unknown> = { date: String(d.event_date ?? d.date ?? d.day ?? "").slice(5) };
                  for (const key of Object.keys(d)) {
                    if (!REQUEST_SKIP_KEYS.has(key)) {
                      mapped[key] = chNum(d, key);
                    }
                  }
                  return mapped;
                })}>
                  <CartesianGrid strokeDasharray="3 3" stroke="#2a2b36" />
                  <XAxis dataKey="date" tick={{ fontSize: 10, fill: "#9394a1" }} />
                  <YAxis tick={{ fontSize: 10, fill: "#9394a1" }} />
                  <Tooltip contentStyle={TOOLTIP_STYLE} />
                  <Legend wrapperStyle={{ fontSize: "10px" }} />
                  {Object.keys(requestTotals).map((key, i) => (
                    <Bar key={key} dataKey={key} stackId="a" fill={COLORS[i % COLORS.length]} name={key.replace(/_/g, " ")} />
                  ))}
                </BarChart>
              </ResponsiveContainer>
            </div>
          </Section>
        )}

        {/* ── Section 11: Usage Events Table ───────────────────────────── */}
        {events && events.events.length > 0 && (
          <Section title={`Usage Events (${events.total_count} total)`}>
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-hidden">
              <div className="overflow-x-auto">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="border-b border-[#2a2b36] text-text-muted">
                      <th className="text-left px-3 py-2 font-medium">Date</th>
                      <th className="text-left px-3 py-2 font-medium">User</th>
                      <th className="text-left px-3 py-2 font-medium">Model</th>
                      <th className="text-left px-3 py-2 font-medium">Type</th>
                      <th className="text-right px-3 py-2 font-medium">Input</th>
                      <th className="text-right px-3 py-2 font-medium">Output</th>
                      <th className="text-right px-3 py-2 font-medium">Cache W</th>
                      <th className="text-right px-3 py-2 font-medium">Cache R</th>
                      <th className="text-right px-3 py-2 font-medium">Fee</th>
                      <th className="text-right px-3 py-2 font-medium">Charged</th>
                    </tr>
                  </thead>
                  <tbody>
                    {events.events.map((e, i) => (
                      <tr key={i} className="border-b border-[#1e1f2a] hover:bg-[#22232e] transition-colors">
                        <td className="px-3 py-2 text-text-muted whitespace-nowrap">{formatDate(e.timestamp)}</td>
                        <td className="px-3 py-2 text-text-secondary truncate max-w-[100px]" title={e.owningUser ?? ""}>
                          {e.owningUser ? e.owningUser.split("@")[0] : "-"}
                        </td>
                        <td className="px-3 py-2 text-text-primary">
                          <span>{shortModel(e.model)}</span>
                          {e.maxMode && <Badge text="MAX" color="bg-amber-500/20 text-amber-400" />}
                          {e.isHeadless && <Badge text="BG" color="bg-purple-500/20 text-purple-400" />}
                        </td>
                        <td className="px-3 py-2 text-text-secondary">
                          <span>{formatKind(e.kind, e.isChargeable)}</span>
                          {e.isChargeable && formatKind(e.kind, e.isChargeable) !== "On-Demand" && (
                            <Badge text="ON-DEMAND" color="bg-orange-500/20 text-orange-400" />
                          )}
                        </td>
                        <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(e.tokenUsage?.inputTokens)}</td>
                        <td className="px-3 py-2 text-right text-text-secondary font-mono">{formatNum(e.tokenUsage?.outputTokens)}</td>
                        <td className="px-3 py-2 text-right text-text-muted font-mono">{formatNum(e.tokenUsage?.cacheWriteTokens)}</td>
                        <td className="px-3 py-2 text-right text-text-muted font-mono">{formatNum(e.tokenUsage?.cacheReadTokens)}</td>
                        <td className="px-3 py-2 text-right text-text-muted font-mono">
                          {e.cursorTokenFee != null ? `$${(e.cursorTokenFee / 100).toFixed(3)}` : "-"}
                        </td>
                        <td className="px-3 py-2 text-right text-text-primary font-mono">
                          {e.chargedCents != null ? `$${(e.chargedCents / 100).toFixed(2)}` : "-"}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {events.events.length < events.total_count && (
                <button
                  onClick={loadMoreEvents}
                  className="w-full px-3 py-2 text-xs text-accent-blue hover:bg-[#22232e] transition-colors"
                >
                  Load More ({events.total_count - events.events.length} remaining)
                </button>
              )}
            </div>
          </Section>
        )}

        {/* ── Other Details (collapsed by default) ───────────────────── */}
        <Section title="Other Details" defaultOpen={false}>
          <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] p-4">
            <div className="grid grid-cols-2 md:grid-cols-4 gap-4 text-xs">
              <div>
                <span className="text-text-muted block mb-0.5">Team Membership</span>
                <div className="text-text-primary font-medium capitalize">{stripe?.teamMembershipType ?? "-"}</div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Individual Type</span>
                <div className="text-text-primary font-medium capitalize">{stripe?.individualMembershipType ?? "-"}</div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Payment Status</span>
                <div className={`font-medium ${stripe?.lastPaymentFailed ? "text-red-400" : "text-emerald-400"}`}>
                  {stripe?.lastPaymentFailed ? "Failed" : "OK"}
                </div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Student</span>
                <div className="text-text-primary font-medium">{stripe?.verifiedStudent ? "Verified" : "No"}</div>
              </div>
            </div>

            <div className="border-t border-[#2a2b36] mt-3 pt-3 grid grid-cols-2 md:grid-cols-4 gap-4 text-xs">
              <div>
                <span className="text-text-muted block mb-0.5">Sessions</span>
                <div className="text-text-primary font-medium">{Array.isArray(sessions) ? sessions.length : "-"}</div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Seats</span>
                <div className="text-text-primary font-medium">{team?.seats ?? "-"}</div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Pricing</span>
                <div className="text-text-primary font-medium capitalize">{team?.pricingStrategy ?? "-"}</div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Billing</span>
                <div className="text-text-primary font-medium">{stripe?.isYearlyPlan ? "Yearly" : "Monthly"}</div>
              </div>
            </div>

            <div className="border-t border-[#2a2b36] mt-3 pt-3 grid grid-cols-2 md:grid-cols-4 gap-4 text-xs">
              <div>
                <span className="text-text-muted block mb-0.5">SSO</span>
                <div className="flex items-center gap-1">
                  <Badge
                    text={team?.ssoEnabled ? "ENABLED" : "DISABLED"}
                    color={team?.ssoEnabled ? "bg-emerald-500/20 text-emerald-400" : "bg-[#2a2b36] text-text-muted"}
                  />
                </div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Privacy Mode</span>
                <div className="flex items-center gap-1">
                  <Badge
                    text={team?.privacyModeForced ? "FORCED" : "OFF"}
                    color={team?.privacyModeForced ? "bg-amber-500/20 text-amber-400" : "bg-[#2a2b36] text-text-muted"}
                  />
                </div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Admin-Only Analytics</span>
                <div className="flex items-center gap-1">
                  <Badge
                    text={team?.dashboardAnalyticsRequiresAdmin ? "YES" : "NO"}
                    color={team?.dashboardAnalyticsRequiresAdmin ? "bg-amber-500/20 text-amber-400" : "bg-[#2a2b36] text-text-muted"}
                  />
                </div>
              </div>
              <div>
                <span className="text-text-muted block mb-0.5">Auto Usage</span>
                <div className="text-text-primary font-medium">{pct(plan?.autoPercentUsed)}</div>
              </div>
            </div>

            {team && (
              <div className="border-t border-[#2a2b36] mt-3 pt-3 grid grid-cols-2 md:grid-cols-4 gap-4 text-xs">
                <div>
                  <span className="text-text-muted block mb-0.5">Team Billing Cycle</span>
                  <div className="text-text-primary font-medium text-[10px]">
                    {team.billingCycleStart ? formatEpochDate(team.billingCycleStart) : "-"}
                    {" - "}
                    {team.billingCycleEnd ? formatEpochDate(team.billingCycleEnd) : "-"}
                  </div>
                </div>
              </div>
            )}
          </div>
        </Section>

        {/* ── AI Code Attribution ──────────────────────────────────────── */}
        <Section title="AI Code Attribution" defaultOpen={true}>
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-xs text-text-secondary">Analyze AI vs human code contributions from Cursor tracking data</p>
                <DebugPath path="~/.cursor/ai-tracking/ai-code-tracking.db" />
              </div>
              <button
                onClick={attrFetchAll}
                disabled={attrLoading}
                className="px-3 py-1.5 text-xs bg-app-card border border-border rounded-lg hover:bg-app-card-hover disabled:opacity-50"
              >
                {attrLoading ? "Loading..." : "Refresh"}
              </button>
            </div>

            <div className="bg-blue-900/20 border border-blue-500/30 rounded-lg px-4 py-3 text-sm text-blue-200">
              Global data only. Cursor&apos;s ai-tracking database does not store project or repo, so attribution cannot be filtered by project.
            </div>

            {attrSummary && (
              <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
                <AttributionSummaryCard label="Total Commits Scored" value={attrSummary.total_commits.toLocaleString()} />
                <AttributionSummaryCard
                  label="Avg AI Percentage"
                  value={`${attrSummary.avg_ai_percentage.toFixed(1)}%`}
                  sub={attrSummary.avg_ai_percentage > 50 ? "Mostly AI-generated" : "Mostly human-written"}
                />
                <AttributionSummaryCard label="AI Lines Added" value={attrSummary.total_composer_lines.toLocaleString()} />
                <AttributionSummaryCard label="Human Lines Added" value={attrSummary.total_human_lines.toLocaleString()} />
              </div>
            )}

            <div className="flex gap-1 border-b border-border">
              {(["overview", "commits", "conversations"] as const).map((tab) => (
                <button
                  key={tab}
                  onClick={() => setAttrTab(tab)}
                  className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
                    attrTab === tab
                      ? "border-indigo-500 text-text-primary"
                      : "border-transparent text-text-muted hover:text-text-secondary"
                  }`}
                >
                  {tab.charAt(0).toUpperCase() + tab.slice(1)}
                </button>
              ))}
            </div>

            {attrTab === "overview" && (
              <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
                <div className="bg-app-card border border-border rounded-lg p-4">
                  <h3 className="text-sm font-semibold text-text-primary mb-3">AI vs Human Code</h3>
                  <AiVsHumanPie
                    aiLines={attrSummary?.total_composer_lines ?? 0}
                    humanLines={attrSummary?.total_human_lines ?? 0}
                  />
                </div>
                <div className="bg-app-card border border-border rounded-lg p-4">
                  <h3 className="text-sm font-semibold text-text-primary mb-3">AI % Over Time</h3>
                  <AiTrendChart commits={attrCommits} />
                </div>
                <div className="bg-app-card border border-border rounded-lg p-4">
                  <h3 className="text-sm font-semibold text-text-primary mb-3">AI Code by File Type</h3>
                  <FileTypeChart data={attrFileTypes} />
                </div>
                <div className="bg-app-card border border-border rounded-lg p-4">
                  <h3 className="text-sm font-semibold text-text-primary mb-3">Models Used</h3>
                  <ModelBreakdownChart data={attrModelBreakdown} />
                </div>
              </div>
            )}

            {attrTab === "commits" && (
              <div>
                <h3 className="text-sm font-semibold text-text-primary mb-3">
                  Per-Commit Breakdown ({attrCommits.length} commits)
                </h3>
                <CommitTable commits={attrCommits} />
              </div>
            )}

            {attrTab === "conversations" && <ConversationsTab conversations={attrConversations} />}
          </div>
        </Section>

        {/* ── Team Members (collapsed by default) ──────────────────────── */}
        {overview?.team_members?.teamMembers && overview.team_members.teamMembers.length > 0 && (
          <Section title={`Team Members (${overview.team_members.teamMembers.length})`} defaultOpen={false}>
            <div className="flex items-center gap-2 mb-2">
              <span className="text-[10px] text-text-muted uppercase tracking-wider">Sort by:</span>
              <button
                onClick={() => setTeamSortBy("name")}
                className={`px-2 py-0.5 rounded text-[10px] font-medium transition-colors ${
                  teamSortBy === "name" ? "bg-accent-blue text-white" : "bg-[#1a1b23] text-text-secondary hover:bg-[#22232e]"
                }`}
              >
                Name
              </button>
              <button
                onClick={() => setTeamSortBy("role")}
                className={`px-2 py-0.5 rounded text-[10px] font-medium transition-colors ${
                  teamSortBy === "role" ? "bg-accent-blue text-white" : "bg-[#1a1b23] text-text-secondary hover:bg-[#22232e]"
                }`}
              >
                Role
              </button>
            </div>
            <div className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] overflow-hidden max-h-96 overflow-y-auto">
              <table className="w-full text-xs">
                <thead className="sticky top-0 bg-[#1a1b23]">
                  <tr className="border-b border-[#2a2b36] text-text-muted">
                    <th className="text-left px-3 py-2 font-medium">Member</th>
                    <th className="text-left px-3 py-2 font-medium">Email</th>
                    <th className="text-left px-3 py-2 font-medium">Role</th>
                  </tr>
                </thead>
                <tbody>
                  {[...overview.team_members.teamMembers]
                    .sort((a, b) => {
                      if (teamSortBy === "role") {
                        const diff = rolePriority(a.role) - rolePriority(b.role);
                        if (diff !== 0) return diff;
                        return (a.name ?? a.email ?? "").localeCompare(b.name ?? b.email ?? "");
                      }
                      return (a.name ?? a.email ?? "").localeCompare(b.name ?? b.email ?? "");
                    })
                    .map((member, i) => {
                    const initial = (member.name ?? member.email ?? "?").charAt(0).toUpperCase();
                    const roleLabel = formatRole(member.role);
                    const roleLower = roleLabel.toLowerCase();
                    const roleBadgeColor = roleLower === "owner"
                      ? "bg-amber-500/20 text-amber-400"
                      : roleLower === "admin"
                        ? "bg-red-500/20 text-red-400"
                        : roleLower === "free owner"
                          ? "bg-emerald-500/20 text-emerald-400"
                          : "bg-[#2a2b36] text-text-muted";
                    return (
                      <tr key={member.id ?? i} className="border-b border-[#1e1f2a] hover:bg-[#22232e]">
                        <td className="px-3 py-2">
                          <div className="flex items-center gap-2">
                            <div className="w-6 h-6 rounded-full bg-accent-blue/20 text-accent-blue flex items-center justify-center text-[10px] font-semibold shrink-0">
                              {initial}
                            </div>
                            <span className="text-text-primary font-medium truncate">{member.name || "-"}</span>
                          </div>
                        </td>
                        <td className="px-3 py-2 text-text-secondary truncate max-w-[200px]">{member.email || "-"}</td>
                        <td className="px-3 py-2">
                          <Badge text={roleLabel} color={roleBadgeColor} />
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </Section>
        )}

        </div>{/* end data sections opacity wrapper */}
      </div>
    </div>
  );
}

export function CursorAnalyticsV2Page() {
  return (
    <AnalyticsErrorBoundary>
      <CursorAnalyticsV2Inner />
    </AnalyticsErrorBoundary>
  );
}
