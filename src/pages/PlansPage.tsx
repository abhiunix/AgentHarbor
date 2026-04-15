import { useState, useEffect, useCallback } from "react";
import { useParams } from "react-router-dom";
import {
  listPlans,
  listProjectPlans,
  readPlan,
  listTodos,
} from "../lib/tauri";
import type { PlanEntry, TodoItem, TodoStats } from "../lib/tauri";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";
import { DebugPath } from "../components/common/DebugPath";

type Tab = "plans" | "todos";
type SourceFilter = "all" | "claude" | "cursor";

export function PlansPage() {
  const { adapterId } = useParams<{ adapterId?: string }>();
  const defaultSource: SourceFilter =
    adapterId === "claude-code" ? "claude" : adapterId === "cursor" ? "cursor" : "all";

  const [projectScope, setProjectScope] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<Tab>("plans");
  const [sourceFilter, setSourceFilter] = useState<SourceFilter>(defaultSource);

  return (
    <div className="h-full flex flex-col">
      <div className="px-6 pt-6 pb-0">
        <div className="flex items-center justify-between flex-wrap gap-3 mb-4">
          <div>
            <h1 className="text-2xl font-semibold text-text-primary mb-1">Plans & Todos</h1>
            <DebugPath path={
              adapterId === "claude-code"
                ? "~/.claude/plans/ · ~/.claude/todos/"
                : adapterId === "cursor"
                ? "~/.cursor/plans/ · ~/.cursor/todos/"
                : "~/.claude/plans/ · ~/.cursor/plans/ · ~/.claude/todos/"
            } className="text-sm" />
          </div>
          <ProjectScopeSelector value={projectScope} onChange={setProjectScope} />
        </div>

        <div className="flex items-center justify-between mb-0">
          <div className="flex gap-1 border-b border-border">
            <button
              onClick={() => setActiveTab("plans")}
              className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
                activeTab === "plans"
                  ? "border-accent-blue text-text-primary"
                  : "border-transparent text-text-secondary hover:text-text-primary"
              }`}
            >
              Plans
            </button>
            <button
              onClick={() => setActiveTab("todos")}
              className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
                activeTab === "todos"
                  ? "border-accent-blue text-text-primary"
                  : "border-transparent text-text-secondary hover:text-text-primary"
              }`}
            >
              Todos
            </button>
          </div>

          {defaultSource === "all" && (
            <div className="flex items-center gap-2">
              <span className="text-xs text-text-secondary">Source:</span>
              {(["all", "claude", "cursor"] as const).map((s) => (
                <button
                  key={s}
                  onClick={() => setSourceFilter(s)}
                  className={`px-2.5 py-1 text-xs font-medium rounded transition-colors ${
                    sourceFilter === s
                      ? "bg-accent-blue text-white"
                      : "bg-app-card border border-border text-text-secondary hover:text-text-primary"
                  }`}
                >
                  {s === "all" ? "All" : s === "claude" ? "Claude" : "Cursor"}
                </button>
              ))}
            </div>
          )}
        </div>
      </div>

      {activeTab === "plans" ? (
        <PlansTab sourceFilter={sourceFilter} projectScope={projectScope} />
      ) : (
        <TodosTab sourceFilter={sourceFilter} projectScope={projectScope} />
      )}
    </div>
  );
}

function PlansTab({
  sourceFilter,
  projectScope,
}: {
  sourceFilter: SourceFilter;
  projectScope: string | null;
}) {
  const [plans, setPlans] = useState<PlanEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expandedPlan, setExpandedPlan] = useState<string | null>(null);
  const [planContent, setPlanContent] = useState<Record<string, string>>({});
  const [loadingContent, setLoadingContent] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = projectScope
        ? await listProjectPlans(projectScope)
        : await listPlans();
      setPlans(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [projectScope]);

  useEffect(() => {
    load();
  }, [load]);

  const handleExpand = async (plan: PlanEntry) => {
    const key = plan.file_path;
    if (expandedPlan === key) {
      setExpandedPlan(null);
      return;
    }
    setExpandedPlan(key);
    if (!planContent[key]) {
      setLoadingContent(key);
      try {
        const content = await readPlan(plan.file_path);
        setPlanContent((prev) => ({ ...prev, [key]: content }));
      } catch (e) {
        setPlanContent((prev) => ({ ...prev, [key]: `Error loading plan: ${e}` }));
      } finally {
        setLoadingContent(null);
      }
    }
  };

  const filtered = plans.filter(
    (p) => sourceFilter === "all" || p.source === sourceFilter
  );

  if (loading) {
    return <div className="p-6 text-text-secondary text-sm">Loading plans...</div>;
  }

  if (error) {
    return (
      <div className="p-6">
        <div className="px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400">
          {error}
        </div>
      </div>
    );
  }

  if (filtered.length === 0) {
    return <div className="p-6 text-text-secondary text-sm">No plans found.</div>;
  }

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-3">
      {filtered.map((plan) => {
        const isExpanded = expandedPlan === plan.file_path;
        const content = planContent[plan.file_path];
        const isLoadingThis = loadingContent === plan.file_path;

        return (
          <div
            key={plan.file_path}
            className="bg-app-card border border-border rounded-lg overflow-hidden"
          >
            <button
              onClick={() => handleExpand(plan)}
              className="w-full px-4 py-3 flex items-center gap-3 text-left hover:bg-app-card-hover transition-colors"
            >
              <span className="text-text-secondary text-sm flex-shrink-0">
                {isExpanded ? "▼" : "▶"}
              </span>
              <PlanSourceBadge source={plan.source} />
              <div className="flex-1 min-w-0">
                <h3 className="text-sm font-medium text-text-primary truncate">
                  {plan.name}
                </h3>
                <p className="text-xs text-text-secondary truncate mt-0.5">
                  {truncateOverview(plan.overview)}
                </p>
              </div>
              <span className="text-xs text-text-secondary flex-shrink-0">
                {formatDate(plan.modified_at)}
              </span>
            </button>

            {isExpanded && (
              <div className="border-t border-border px-4 py-3">
                {isLoadingThis ? (
                  <p className="text-text-secondary text-sm">Loading content...</p>
                ) : content ? (
                  <pre className="text-xs text-text-primary font-mono whitespace-pre-wrap break-words max-h-96 overflow-y-auto">
                    {content}
                  </pre>
                ) : null}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

type StatusFilter = "all" | "pending" | "in_progress" | "completed";

function TodosTab({
  sourceFilter,
  projectScope,
}: {
  sourceFilter: SourceFilter;
  projectScope: string | null;
}) {
  const [todos, setTodos] = useState<TodoItem[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [search, setSearch] = useState("");

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const todoList = await listTodos(projectScope ?? undefined);
      setTodos(todoList);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [projectScope]);

  useEffect(() => {
    load();
  }, [load]);

  const sourceFiltered = todos.filter(
    (t) => sourceFilter === "all" || t.source === sourceFilter
  );

  const stats: TodoStats = {
    total: sourceFiltered.length,
    pending: sourceFiltered.filter((t) => t.status === "pending").length,
    in_progress: sourceFiltered.filter((t) => t.status === "in_progress").length,
    completed: sourceFiltered.filter((t) => t.status === "completed").length,
  };

  const filtered = sourceFiltered.filter((t) => {
    if (statusFilter !== "all" && t.status !== statusFilter) return false;
    if (search.trim()) {
      const q = search.trim().toLowerCase();
      if (!t.content.toLowerCase().includes(q)) return false;
    }
    return true;
  });

  const grouped = {
    in_progress: filtered.filter((t) => t.status === "in_progress"),
    pending: filtered.filter((t) => t.status === "pending"),
    completed: filtered.filter((t) => t.status === "completed"),
  };

  if (loading) {
    return <div className="p-6 text-text-secondary text-sm">Loading todos...</div>;
  }

  if (error) {
    return (
      <div className="p-6">
        <div className="px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400">
          {error}
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-6">
      {stats.total > 0 && (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
          <StatCard
            label="Total"
            value={stats.total}
            color="text-text-primary"
            active={statusFilter === "all"}
            onClick={() => setStatusFilter("all")}
          />
          <StatCard
            label="Pending"
            value={stats.pending}
            color="text-amber-400"
            active={statusFilter === "pending"}
            onClick={() => setStatusFilter(statusFilter === "pending" ? "all" : "pending")}
          />
          <StatCard
            label="In Progress"
            value={stats.in_progress}
            color="text-blue-400"
            active={statusFilter === "in_progress"}
            onClick={() => setStatusFilter(statusFilter === "in_progress" ? "all" : "in_progress")}
          />
          <StatCard
            label="Completed"
            value={stats.completed}
            color="text-green-400"
            active={statusFilter === "completed"}
            onClick={() => setStatusFilter(statusFilter === "completed" ? "all" : "completed")}
          />
        </div>
      )}

      <div className="relative">
        <svg
          className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-secondary pointer-events-none"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          strokeWidth={2}
        >
          <circle cx="11" cy="11" r="8" />
          <path d="m21 21-4.35-4.35" strokeLinecap="round" />
        </svg>
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search todos..."
          className="w-full pl-9 pr-3 py-2 text-sm bg-app-card border border-border rounded-lg text-text-primary placeholder:text-text-secondary focus:outline-none focus:border-accent-blue transition-colors"
        />
        {search && (
          <button
            onClick={() => setSearch("")}
            className="absolute right-3 top-1/2 -translate-y-1/2 text-text-secondary hover:text-text-primary text-xs"
          >
            Clear
          </button>
        )}
      </div>

      {filtered.length === 0 ? (
        <p className="text-text-secondary text-sm">
          {search.trim() || statusFilter !== "all"
            ? "No todos match your filters."
            : "No todos found."}
        </p>
      ) : (
        <>
          <TodoGroup title="In Progress" items={grouped.in_progress} />
          <TodoGroup title="Pending" items={grouped.pending} />
          <TodoGroup title="Completed" items={grouped.completed} />
        </>
      )}
    </div>
  );
}

function StatCard({
  label,
  value,
  color,
  active,
  onClick,
}: {
  label: string;
  value: number;
  color: string;
  active?: boolean;
  onClick?: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className={`bg-app-card border rounded-lg p-4 text-left transition-colors ${
        active
          ? "border-accent-blue ring-1 ring-accent-blue/30"
          : "border-border hover:border-text-secondary"
      }`}
    >
      <div className="text-xs text-text-secondary uppercase tracking-wider mb-1">{label}</div>
      <div className={`text-2xl font-bold ${color}`}>{value}</div>
    </button>
  );
}

function TodoGroup({ title, items }: { title: string; items: TodoItem[] }) {
  if (items.length === 0) return null;

  return (
    <div>
      <h3 className="text-sm font-semibold text-text-primary mb-3">
        {title}
        <span className="ml-2 text-xs text-text-secondary font-normal">({items.length})</span>
      </h3>
      <div className="space-y-2">
        {items.map((item, i) => (
          <div
            key={`${item.source}-${i}`}
            className="bg-app-card border border-border rounded-lg px-4 py-3 flex items-start gap-3"
          >
            <StatusBadge status={item.status} />
            <p className="flex-1 text-sm text-text-primary leading-relaxed">{item.content}</p>
            <PlanSourceBadge source={item.source} />
          </div>
        ))}
      </div>
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const config: Record<string, { bg: string; text: string; label: string }> = {
    in_progress: { bg: "bg-blue-500/20", text: "text-blue-400", label: "In Progress" },
    pending: { bg: "bg-amber-500/20", text: "text-amber-400", label: "Pending" },
    completed: { bg: "bg-green-500/20", text: "text-green-400", label: "Completed" },
  };
  const c = config[status] ?? { bg: "bg-gray-500/20", text: "text-gray-400", label: status };
  return (
    <span className={`px-1.5 py-0.5 text-[10px] font-medium rounded flex-shrink-0 ${c.bg} ${c.text}`}>
      {c.label}
    </span>
  );
}

function PlanSourceBadge({ source }: { source: string }) {
  const isCI = source === "claude";
  return (
    <span
      className={`px-1.5 py-0.5 text-[10px] font-medium rounded flex-shrink-0 ${
        isCI ? "bg-blue-500/20 text-blue-400" : "bg-purple-500/20 text-purple-400"
      }`}
    >
      {isCI ? "Claude" : "Cursor"}
    </span>
  );
}

function truncateOverview(text: string, maxLen = 120): string {
  if (!text) return "";
  if (text.length <= maxLen) return text;
  return text.slice(0, maxLen).trimEnd() + "...";
}

function formatDate(dateStr: string): string {
  try {
    const d = new Date(dateStr);
    return d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
  } catch {
    return dateStr;
  }
}
