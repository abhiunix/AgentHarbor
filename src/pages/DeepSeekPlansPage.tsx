import { useState, useEffect, useCallback } from "react";
import { listDeepSeekPlans, listDeepSeekTodoGroups } from "../lib/tauri";
import type { DeepSeekPlan, DeepSeekTodoGroup } from "../lib/tauri";
import { DebugPath } from "../components/common/DebugPath";

type Tab = "plans" | "todos";

export function DeepSeekPlansPage() {
  const [activeTab, setActiveTab] = useState<Tab>("plans");

  return (
    <div className="h-full flex flex-col">
      <div className="px-6 pt-6 pb-0">
        <div className="flex items-center justify-between flex-wrap gap-3 mb-4">
          <div>
            <h1 className="text-2xl font-semibold text-text-primary mb-1">Plans & Todos</h1>
            <DebugPath path="~/.dsh/storages/session_projcache.json" className="text-sm" />
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

      {activeTab === "plans" ? <DeepSeekPlansTab /> : <DeepSeekTodosTab />}
    </div>
  );
}

function DeepSeekPlansTab() {
  const [plans, setPlans] = useState<DeepSeekPlan[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await listDeepSeekPlans();
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
      {plans.map((plan) => (
        <div key={plan.session_id} className="bg-app-card border border-border rounded-lg overflow-hidden">
          <div className="px-4 py-3 flex items-center gap-3 flex-wrap">
            <h3 className="text-sm font-medium text-text-primary truncate flex-1 min-w-0">{plan.title}</h3>
            <span className="px-1.5 py-0.5 text-[10px] font-medium rounded flex-shrink-0 bg-cyan-500/20 text-cyan-400">
              {plan.workspace_name}
            </span>
            {plan.active && (
              <span className="px-1.5 py-0.5 text-[10px] font-medium rounded flex-shrink-0 bg-green-500/20 text-green-400">
                Active
              </span>
            )}
          </div>
          {(plan.wanted || plan.running) && (
            <div className="border-t border-border px-4 py-3 space-y-2">
              {plan.wanted && (
                <div>
                  <p className="text-xs text-text-secondary mb-0.5">Wanted</p>
                  <p className="text-sm text-text-primary font-mono break-words">{plan.wanted}</p>
                </div>
              )}
              {plan.running && (
                <div>
                  <p className="text-xs text-text-secondary mb-0.5">Running</p>
                  <p className="text-sm text-text-primary font-mono break-words">{plan.running}</p>
                </div>
              )}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

function DeepSeekTodosTab() {
  const [groups, setGroups] = useState<DeepSeekTodoGroup[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await listDeepSeekTodoGroups();
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
        <div key={group.session_id} className="bg-app-card border border-border rounded-lg overflow-hidden">
          <div className="px-4 py-3 border-b border-border flex items-center gap-3 flex-wrap">
            <h3 className="text-sm font-medium text-text-primary truncate flex-1 min-w-0">{group.title}</h3>
            <span className="px-1.5 py-0.5 text-[10px] font-medium rounded flex-shrink-0 bg-cyan-500/20 text-cyan-400">
              {group.workspace_name}
            </span>
          </div>
          <div className="px-4 py-3 space-y-2">
            {group.todos.map((todo, i) => (
              <div key={i} className="flex items-start gap-3">
                <DeepSeekStatusBadge status={todo.status} />
                <p className="flex-1 text-sm text-text-primary leading-relaxed">{todo.title}</p>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

function DeepSeekStatusBadge({ status }: { status: string }) {
  const config: Record<string, { bg: string; text: string; label: string }> = {
    in_progress: { bg: "bg-blue-500/20", text: "text-blue-400", label: "In Progress" },
    pending: { bg: "bg-amber-500/20", text: "text-amber-400", label: "Pending" },
    completed: { bg: "bg-green-500/20", text: "text-green-400", label: "Completed" },
  };
  const c = config[status] ?? { bg: "bg-gray-500/20", text: "text-gray-400", label: status };
  return (
    <span className={`px-1.5 py-0.5 text-[10px] font-medium rounded flex-shrink-0 mt-0.5 ${c.bg} ${c.text}`}>
      {c.label}
    </span>
  );
}
