import { useState, useEffect, useCallback } from "react";
import { listKimiPlans, readKimiPlan, listKimiTodoGroups } from "../lib/tauri";
import type { KimiPlan, KimiTodoGroup } from "../lib/tauri";
import { DebugPath } from "../components/common/DebugPath";

type Tab = "plans" | "todos";

export function KimiPlansPage() {
  const [activeTab, setActiveTab] = useState<Tab>("plans");

  return (
    <div className="h-full flex flex-col">
      <div className="px-6 pt-6 pb-0">
        <div className="flex items-center justify-between flex-wrap gap-3 mb-4">
          <div>
            <h1 className="text-2xl font-semibold text-text-primary mb-1">Plans & Todos</h1>
            <DebugPath path="~/.kimi/plans/ · ~/.kimi/sessions/*/*/state.json" className="text-sm" />
          </div>
        </div>

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
      </div>

      {activeTab === "plans" ? <KimiPlansTab /> : <KimiTodosTab />}
    </div>
  );
}

function KimiPlansTab() {
  const [plans, setPlans] = useState<KimiPlan[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expandedPlan, setExpandedPlan] = useState<string | null>(null);
  const [planContent, setPlanContent] = useState<Record<string, string>>({});
  const [loadingContent, setLoadingContent] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await listKimiPlans();
      setPlans(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const handleExpand = async (plan: KimiPlan) => {
    const key = plan.path;
    if (expandedPlan === key) {
      setExpandedPlan(null);
      return;
    }
    setExpandedPlan(key);
    if (!planContent[key]) {
      setLoadingContent(key);
      try {
        const content = await readKimiPlan(plan.path);
        setPlanContent((prev) => ({ ...prev, [key]: content }));
      } catch (e) {
        setPlanContent((prev) => ({ ...prev, [key]: `Error loading plan: ${e}` }));
      } finally {
        setLoadingContent(null);
      }
    }
  };

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

  if (plans.length === 0) {
    return (
      <div className="p-6">
        <div className="text-center py-12">
          <p className="text-text-secondary text-sm">No plans yet.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-3">
      {plans.map((plan) => {
        const isExpanded = expandedPlan === plan.path;
        const content = planContent[plan.path];
        const isLoadingThis = loadingContent === plan.path;

        return (
          <div
            key={plan.path}
            className="bg-app-card border border-border rounded-lg overflow-hidden"
          >
            <button
              onClick={() => handleExpand(plan)}
              className="w-full px-4 py-3 flex items-center gap-3 text-left hover:bg-app-card-hover transition-colors"
            >
              <span className="text-text-secondary text-sm flex-shrink-0">
                {isExpanded ? "▼" : "▶"}
              </span>
              <div className="flex-1 min-w-0">
                <h3 className="text-sm font-medium text-text-primary truncate">
                  {plan.name}
                </h3>
                <p className="text-xs text-text-secondary truncate mt-0.5">
                  {formatSize(plan.size_bytes)}
                </p>
              </div>
              <span className="text-xs text-text-secondary flex-shrink-0">
                {formatDate(plan.modified)}
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

function KimiTodosTab() {
  const [groups, setGroups] = useState<KimiTodoGroup[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await listKimiTodoGroups();
      setGroups(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

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

  if (groups.length === 0) {
    return (
      <div className="p-6">
        <div className="text-center py-12">
          <p className="text-text-secondary text-sm">No todos yet.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-4">
      {groups.map((group) => (
        <div
          key={group.session_id}
          className="bg-app-card border border-border rounded-lg overflow-hidden"
        >
          <div className="px-4 py-3 border-b border-border flex items-center gap-3 flex-wrap">
            <h3 className="text-sm font-medium text-text-primary truncate flex-1 min-w-0">
              {group.title}
            </h3>
            <span className="px-1.5 py-0.5 text-[10px] font-medium rounded flex-shrink-0 bg-cyan-500/20 text-cyan-400">
              {group.project_name}
            </span>
            {group.plan_slug && (
              <span className="px-1.5 py-0.5 text-[10px] font-medium rounded flex-shrink-0 bg-purple-500/20 text-purple-400">
                {group.plan_slug}
              </span>
            )}
          </div>
          <div className="px-4 py-3 space-y-2">
            {group.todos.map((todo, i) => (
              <div key={i} className="flex items-start gap-3">
                <KimiStatusBadge status={todo.status} />
                <p className="flex-1 text-sm text-text-primary leading-relaxed">{todo.title}</p>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function KimiStatusBadge({ status }: { status: string }) {
  const config: Record<string, { bg: string; text: string; label: string }> = {
    in_progress: { bg: "bg-blue-500/20", text: "text-blue-400", label: "In Progress" },
    pending: { bg: "bg-amber-500/20", text: "text-amber-400", label: "Pending" },
    done: { bg: "bg-green-500/20", text: "text-green-400", label: "Done" },
  };
  const c = config[status] ?? { bg: "bg-gray-500/20", text: "text-gray-400", label: status };
  return (
    <span className={`px-1.5 py-0.5 text-[10px] font-medium rounded flex-shrink-0 mt-0.5 ${c.bg} ${c.text}`}>
      {c.label}
    </span>
  );
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDate(dateStr: string | null): string {
  if (!dateStr) return "";
  try {
    const d = new Date(dateStr);
    return d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
  } catch {
    return dateStr;
  }
}
