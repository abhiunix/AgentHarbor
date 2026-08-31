import { useEffect, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useAiTrackingStore } from "../stores/aiTrackingStore";
import {
  PieChart, Pie, Cell, BarChart, Bar, XAxis, YAxis, Tooltip,
  ResponsiveContainer, Legend, AreaChart, Area, CartesianGrid,
} from "recharts";
import type { ScoredCommit } from "../lib/tauri";
import { DebugPath } from "../components/common/DebugPath";

const COLORS = ["#6366f1", "#22d3ee", "#f59e0b", "#ef4444", "#10b981", "#8b5cf6", "#ec4899", "#14b8a6"];

function SummaryCard({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div className="bg-app-card border border-border rounded-lg p-4">
      <div className="text-xs text-text-muted uppercase tracking-wider mb-1">{label}</div>
      <div className="text-2xl font-bold text-text-primary">{value}</div>
      {sub && <div className="text-xs text-text-secondary mt-1">{sub}</div>}
    </div>
  );
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
            <th className="px-3 py-2 text-right">Tab Lines</th>
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
              <td className="px-3 py-2 text-right font-mono text-text-muted">
                +{c.tab_lines_added ?? 0}/-{c.tab_lines_deleted ?? 0}
              </td>
              <td className="px-3 py-2 text-xs text-text-muted whitespace-nowrap">
                {c.commit_date ? formatCommitDate(c.commit_date) : formatEpoch(c.scored_at)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
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

function formatEpoch(ms: number): string {
  return new Date(ms).toLocaleDateString("en-US", { month: "short", day: "numeric", year: "numeric" });
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

function AiTrendChart({ commits }: { commits: ScoredCommit[] }) {
  if (commits.length === 0) return <p className="text-text-muted text-sm">No trend data.</p>;

  const sorted = [...commits].sort((a, b) => a.scored_at - b.scored_at);
  const chartData = sorted.map((c) => ({
    date: formatEpoch(c.scored_at),
    ai: c.ai_percentage,
    ts: c.scored_at,
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
            <Cell key={i} fill={COLORS[i % COLORS.length]} />
          ))}
        </Pie>
        <Tooltip contentStyle={{ background: "#1a1b23", border: "1px solid #2a2b36", borderRadius: 8 }} />
      </PieChart>
    </ResponsiveContainer>
  );
}

export function AiAttributionPage() {
  const navigate = useNavigate();
  const { commits, summary, fileTypes, modelBreakdown, loading, error, fetchAll } = useAiTrackingStore();
  const [activeTab, setActiveTab] = useState<"overview" | "commits" | "conversations">("overview");

  useEffect(() => {
    fetchAll();
  }, [fetchAll]);

  const tabs = [
    { id: "overview" as const, label: "Overview" },
    { id: "commits" as const, label: "Commits" },
    { id: "conversations" as const, label: "Conversations" },
  ];

  return (
    <div className="p-6 space-y-6 overflow-y-auto h-full">
      <div className="flex items-center justify-between flex-wrap gap-3">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">AI Code Attribution</h1>
          <p className="text-sm text-text-secondary mt-1">Analyze AI vs human code contributions from Cursor tracking data</p>
          <DebugPath path="~/.cursor/ai-tracking/ai-code-tracking.db" />
        </div>
        <button
          onClick={fetchAll}
          disabled={loading}
          className="px-3 py-1.5 text-sm bg-app-card border border-border rounded-lg hover:bg-app-card-hover disabled:opacity-50"
        >
          {loading ? "Loading..." : "Refresh"}
        </button>
      </div>

      <div className="bg-blue-900/20 border border-blue-500/30 rounded-lg px-4 py-3 text-sm text-blue-200 flex items-center justify-between gap-3 flex-wrap">
        <span>Commits are attributed per project on the Projects page; unmatched commits are listed globally below.</span>
        <button
          onClick={() => navigate("/adapters/cursor/projects")}
          className="text-xs px-2 py-1 rounded border border-blue-500/40 text-blue-200 hover:bg-blue-500/20 whitespace-nowrap"
        >
          Open Projects →
        </button>
      </div>

      {error && <p className="text-sm text-accent-red">{error}</p>}

      {summary && (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <SummaryCard label="Total Commits Scored" value={summary.total_commits.toLocaleString()} />
          <SummaryCard
            label="Avg AI Percentage"
            value={`${summary.avg_ai_percentage.toFixed(1)}%`}
            sub={summary.avg_ai_percentage > 50 ? "Mostly AI-generated" : "Mostly human-written"}
          />
          <SummaryCard label="AI Lines Added" value={summary.total_composer_lines.toLocaleString()} />
          <SummaryCard label="Human Lines Added" value={summary.total_human_lines.toLocaleString()} />
        </div>
      )}

      <div className="flex gap-1 border-b border-border">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
              activeTab === tab.id
                ? "border-indigo-500 text-text-primary"
                : "border-transparent text-text-muted hover:text-text-secondary"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      {activeTab === "overview" && (
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          <div className="bg-app-card border border-border rounded-lg p-4">
            <h3 className="text-sm font-semibold text-text-primary mb-3">AI vs Human Code</h3>
            <AiVsHumanPie
              aiLines={summary?.total_composer_lines ?? 0}
              humanLines={summary?.total_human_lines ?? 0}
            />
          </div>
          <div className="bg-app-card border border-border rounded-lg p-4">
            <h3 className="text-sm font-semibold text-text-primary mb-3">AI % Over Time</h3>
            <AiTrendChart commits={commits} />
          </div>
          <div className="bg-app-card border border-border rounded-lg p-4">
            <h3 className="text-sm font-semibold text-text-primary mb-3">AI Code by File Type</h3>
            <FileTypeChart data={fileTypes} />
          </div>
          <div className="bg-app-card border border-border rounded-lg p-4">
            <h3 className="text-sm font-semibold text-text-primary mb-3">Models Used</h3>
            <ModelBreakdownChart data={modelBreakdown} />
          </div>
        </div>
      )}

      {activeTab === "commits" && (
        <div>
          <h3 className="text-sm font-semibold text-text-primary mb-3">
            Per-Commit Breakdown ({commits.length} commits)
          </h3>
          <CommitTable commits={commits} />
        </div>
      )}

      {activeTab === "conversations" && <ConversationsTab />}
    </div>
  );
}

function ConversationsTab() {
  const { conversations } = useAiTrackingStore();

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
          <div className="text-xs text-text-muted mt-2">{formatEpoch(c.updated_at)}</div>
        </div>
      ))}
    </div>
  );
}
