import { useCallback, useEffect, useRef, useState } from "react";
import { DebugPath } from "../components/common/DebugPath";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";
import {
  getCodexPlansAndTodos,
  type CodexPlanSnapshot,
  type CodexPlansAndTodos,
} from "../lib/tauri";

type Tab = "plans" | "todos";

function formatDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value || "Unknown time";
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function projectName(path: string): string {
  const parts = path.split("/").filter(Boolean);
  return parts[parts.length - 1] || path || "Unknown project";
}

function normalizeStatus(status: string): string {
  return status.trim().toLowerCase().replace(/[ -]+/g, "_");
}

function statusLabel(status: string): string {
  const normalized = normalizeStatus(status);
  if (normalized === "in_progress") return "In progress";
  if (normalized === "completed" || normalized === "done") return "Completed";
  if (normalized === "pending") return "Pending";
  return status || "Unknown";
}

function StatusBadge({ status }: { status: string }) {
  const normalized = normalizeStatus(status);
  const colors =
    normalized === "completed" || normalized === "done"
      ? "bg-emerald-500/15 text-emerald-300 border-emerald-500/25"
      : normalized === "in_progress"
        ? "bg-blue-500/15 text-blue-300 border-blue-500/25"
        : normalized === "pending"
          ? "bg-amber-500/15 text-amber-300 border-amber-500/25"
          : "bg-gray-500/15 text-gray-300 border-gray-500/25";

  return (
    <span
      className={`inline-flex px-2 py-0.5 rounded-full border text-[10px] font-medium whitespace-nowrap ${colors}`}
    >
      {statusLabel(status)}
    </span>
  );
}

function overallPlanStatus(plan: CodexPlanSnapshot): string {
  if (plan.items.length === 0) return "No items";
  const statuses = plan.items.map((item) => normalizeStatus(item.status));
  if (statuses.every((status) => status === "completed" || status === "done")) {
    return "Completed";
  }
  if (statuses.some((status) => status === "in_progress")) return "In progress";
  return "Pending";
}

export function CodexPlansPage() {
  const [activeTab, setActiveTab] = useState<Tab>("plans");
  const [projectScope, setProjectScope] = useState<string | null>(null);
  const [data, setData] = useState<CodexPlansAndTodos | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expandedPlans, setExpandedPlans] = useState<Set<string>>(new Set());
  const requestIdRef = useRef(0);

  const load = useCallback(async () => {
    const requestId = ++requestIdRef.current;
    setLoading(true);
    setError(null);
    setData(null);
    try {
      const next = await getCodexPlansAndTodos(200, projectScope);
      if (requestId !== requestIdRef.current) return;
      setData(next);
    } catch (caught) {
      if (requestId !== requestIdRef.current) return;
      setData(null);
      setError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      if (requestId === requestIdRef.current) setLoading(false);
    }
  }, [projectScope]);

  useEffect(() => {
    setExpandedPlans(new Set());
    void load();
    return () => {
      requestIdRef.current += 1;
    };
  }, [load]);

  const togglePlan = (sessionId: string) => {
    setExpandedPlans((current) => {
      const next = new Set(current);
      if (next.has(sessionId)) next.delete(sessionId);
      else next.add(sessionId);
      return next;
    });
  };

  const activeItems = activeTab === "plans" ? data?.plans : data?.todos;

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <header className="px-6 pt-6 border-b border-border shrink-0">
        <div className="flex items-start justify-between gap-4 mb-4">
          <div>
            <div className="flex items-center gap-2">
              <h1 className="text-2xl font-semibold text-text-primary">
                Plans &amp; Todos
              </h1>
              <span className="px-2 py-0.5 rounded-full bg-text-muted/10 border border-border text-[10px] uppercase tracking-wide text-text-muted">
                Read only
              </span>
            </div>
            <DebugPath path={data?.sourcePath ?? "Codex sessions"} />
            <p className="text-sm text-text-secondary mt-1">
              Latest explicit plan updates saved in Codex sessions
            </p>
          </div>
          <div className="flex items-center gap-3">
            <ProjectScopeSelector
              value={projectScope}
              onChange={setProjectScope}
            />
            <button
              type="button"
              onClick={() => void load()}
              disabled={loading}
              className="px-3 py-2 text-sm bg-app-card border border-border rounded-lg text-text-secondary hover:text-text-primary hover:bg-app-card-hover disabled:opacity-50"
            >
              {loading ? "Refreshing..." : "Refresh"}
            </button>
          </div>
        </div>

        <div
          className="flex gap-1"
          role="tablist"
          aria-label="Codex plans and todos"
        >
          <button
            id="codex-plans-tab"
            type="button"
            role="tab"
            aria-selected={activeTab === "plans"}
            aria-controls="codex-plans-panel"
            onClick={() => setActiveTab("plans")}
            className={`px-4 py-2.5 text-sm font-medium border-b-2 transition-colors ${
              activeTab === "plans"
                ? "border-accent-blue text-text-primary"
                : "border-transparent text-text-secondary hover:text-text-primary"
            }`}
          >
            Plans {data ? `(${data.plans.length})` : ""}
          </button>
          <button
            id="codex-todos-tab"
            type="button"
            role="tab"
            aria-selected={activeTab === "todos"}
            aria-controls="codex-todos-panel"
            onClick={() => setActiveTab("todos")}
            className={`px-4 py-2.5 text-sm font-medium border-b-2 transition-colors ${
              activeTab === "todos"
                ? "border-accent-blue text-text-primary"
                : "border-transparent text-text-secondary hover:text-text-primary"
            }`}
          >
            Todos {data ? `(${data.todos.length})` : ""}
          </button>
        </div>
      </header>

      <div className="px-6 py-2.5 border-b border-border bg-blue-500/5 text-xs text-text-secondary shrink-0">
        This view reflects Codex plan snapshots. Update plans from Codex itself,
        then refresh this page.
      </div>

      {error && (
        <div
          role="alert"
          className="mx-6 mt-4 px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 text-sm shrink-0"
        >
          {error}
        </div>
      )}

      {data?.truncated && (
        <div
          role="status"
          className="mx-6 mt-4 px-4 py-3 bg-amber-500/10 border border-amber-500/30 rounded-lg text-amber-300 text-sm shrink-0"
        >
          The safe scan limit was reached. Older plans or todos may be missing.
        </div>
      )}

      {loading ? (
        <div
          role="status"
          aria-live="polite"
          className="flex-1 flex items-center justify-center text-sm text-text-muted"
        >
          Loading plans and todos...
        </div>
      ) : error && !data ? (
        <div className="flex-1 flex items-center justify-center text-sm text-text-muted p-6 text-center">
          Plans and todos could not be loaded. Use Refresh to try again.
        </div>
      ) : !data || activeItems?.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center text-center p-6">
          <p className="text-sm text-text-secondary">
            {activeTab === "plans"
              ? "No Codex plan snapshots were found."
              : "No todos were found in Codex plan snapshots."}
          </p>
          <p className="text-xs text-text-muted mt-1">
            {projectScope
              ? "Clear the project scope to check all sessions."
              : "Plans appear after a Codex session records an explicit plan update."}
          </p>
        </div>
      ) : activeTab === "plans" ? (
        <div
          id="codex-plans-panel"
          role="tabpanel"
          aria-labelledby="codex-plans-tab"
          className="flex-1 overflow-y-auto min-h-0 p-6 space-y-3"
        >
          {data.plans.map((plan) => {
            const isExpanded = expandedPlans.has(plan.sessionId);
            return (
              <article
                key={plan.sessionId}
                className="bg-app-card border border-border rounded-lg overflow-hidden"
              >
                <button
                  type="button"
                  onClick={() => togglePlan(plan.sessionId)}
                  aria-expanded={isExpanded}
                  className="w-full px-4 py-3 flex items-start gap-3 text-left hover:bg-app-card-hover transition-colors"
                >
                  <span
                    aria-hidden="true"
                    className="text-text-muted text-xs mt-1 shrink-0"
                  >
                    {isExpanded ? "▼" : "▶"}
                  </span>
                  <div className="flex-1 min-w-0">
                    <h2 className="text-sm font-medium text-text-primary truncate">
                      {plan.title}
                    </h2>
                    <div className="flex items-center gap-2 flex-wrap mt-1">
                      <span
                        className="text-xs text-cyan-300"
                        title={plan.projectPath}
                      >
                        {projectName(plan.projectPath)}
                      </span>
                      <span className="text-xs text-text-muted">
                        {plan.items.length} item
                        {plan.items.length === 1 ? "" : "s"}
                      </span>
                      <time
                        dateTime={plan.updatedAt}
                        className="text-xs text-text-muted"
                      >
                        {formatDate(plan.updatedAt)}
                      </time>
                    </div>
                  </div>
                  <StatusBadge status={overallPlanStatus(plan)} />
                </button>

                {isExpanded && (
                  <div className="border-t border-border px-4 py-4">
                    {plan.explanation && (
                      <p className="text-sm text-text-secondary whitespace-pre-wrap break-words mb-4">
                        {plan.explanation}
                      </p>
                    )}
                    {plan.items.length === 0 ? (
                      <p className="text-sm text-text-muted">
                        This plan snapshot has no items.
                      </p>
                    ) : (
                      <ol className="space-y-2">
                        {plan.items.map((item, index) => (
                          <li
                            key={`${plan.sessionId}:${index}`}
                            className="flex items-start gap-3"
                          >
                            <StatusBadge status={item.status} />
                            <p className="text-sm text-text-primary leading-relaxed whitespace-pre-wrap break-words">
                              {item.content}
                            </p>
                          </li>
                        ))}
                      </ol>
                    )}
                    <p
                      className="text-[10px] text-text-muted font-mono mt-4 truncate"
                      title={plan.sessionId}
                    >
                      Session {plan.sessionId}
                    </p>
                  </div>
                )}
              </article>
            );
          })}
        </div>
      ) : (
        <div
          id="codex-todos-panel"
          role="tabpanel"
          aria-labelledby="codex-todos-tab"
          className="flex-1 overflow-y-auto min-h-0 p-6 space-y-2"
        >
          {data.todos.map((todo, index) => (
            <article
              key={`${todo.sessionId}:${index}`}
              className="bg-app-card border border-border rounded-lg px-4 py-3"
            >
              <div className="flex items-start gap-3">
                <StatusBadge status={todo.status} />
                <div className="flex-1 min-w-0">
                  <p className="text-sm text-text-primary whitespace-pre-wrap break-words leading-relaxed">
                    {todo.content}
                  </p>
                  <div className="flex items-center gap-2 flex-wrap mt-2 text-xs text-text-muted">
                    <span className="text-text-secondary truncate max-w-[360px]">
                      {todo.sessionTitle}
                    </span>
                    <span title={todo.projectPath} className="text-cyan-300">
                      {projectName(todo.projectPath)}
                    </span>
                    <time dateTime={todo.updatedAt}>
                      {formatDate(todo.updatedAt)}
                    </time>
                  </div>
                </div>
              </div>
            </article>
          ))}
        </div>
      )}
    </div>
  );
}
