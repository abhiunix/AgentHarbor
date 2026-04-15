import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DebugPath } from "../components/common/DebugPath";

interface PlanEntry {
  name: string;
  source: string;
  file_path: string;
  overview: string;
  modified_at: string;
}

export function CursorPlansPage() {
  const [plans, setPlans] = useState<PlanEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [expandedPlan, setExpandedPlan] = useState<string | null>(null);
  const [planContent, setPlanContent] = useState<Record<string, string>>({});
  const [loadingContent, setLoadingContent] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<PlanEntry[]>("list_plans");
      setPlans(result.filter((p) => p.source === "cursor"));
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

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
        const content = await invoke<string>("read_plan", {
          filePath: plan.file_path,
        });
        setPlanContent((prev) => ({ ...prev, [key]: content }));
      } catch (e) {
        setPlanContent((prev) => ({
          ...prev,
          [key]: `Error loading plan: ${e}`,
        }));
      } finally {
        setLoadingContent(null);
      }
    }
  };

  const filtered = plans.filter((p) => {
    if (!search.trim()) return true;
    const q = search.trim().toLowerCase();
    return (
      p.name.toLowerCase().includes(q) ||
      p.overview.toLowerCase().includes(q)
    );
  });

  return (
    <div className="h-full flex flex-col">
      <div className="px-6 pt-6 pb-4">
        <div className="flex items-center gap-3 mb-4">
          <span className="w-3 h-3 rounded-full bg-blue-500 flex-shrink-0" />
          <div>
            <h1 className="text-2xl font-semibold text-text-primary">
              Cursor — Plans
            </h1>
            <DebugPath path="~/.cursor/plans/" className="text-sm" />
          </div>
        </div>

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
            placeholder="Search plans..."
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
      </div>

      <div className="flex-1 overflow-y-auto px-6 pb-6 space-y-3">
        {loading && (
          <div className="text-text-secondary text-sm">Loading plans...</div>
        )}

        {error && (
          <div className="px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400">
            {error}
          </div>
        )}

        {!loading && !error && filtered.length === 0 && (
          <div className="text-center py-12">
            <p className="text-text-secondary text-sm">
              {search.trim()
                ? "No plans match your search."
                : "No Cursor plans found."}
            </p>
          </div>
        )}

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
                  {isExpanded ? "\u25BC" : "\u25B6"}
                </span>
                <div className="flex-1 min-w-0">
                  <h3 className="text-sm font-medium text-text-primary truncate">
                    {plan.name}
                  </h3>
                  {plan.overview && (
                    <p className="text-xs text-text-secondary truncate mt-0.5">
                      {truncateText(plan.overview, 120)}
                    </p>
                  )}
                </div>
                <span className="text-xs text-text-secondary flex-shrink-0">
                  {formatDate(plan.modified_at)}
                </span>
              </button>

              {isExpanded && (
                <div className="border-t border-border px-4 py-3">
                  {isLoadingThis ? (
                    <p className="text-text-secondary text-sm">
                      Loading content...
                    </p>
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
    </div>
  );
}

function truncateText(text: string, maxLen = 120): string {
  if (!text) return "";
  if (text.length <= maxLen) return text;
  return text.slice(0, maxLen).trimEnd() + "...";
}

function formatDate(dateStr: string): string {
  try {
    const d = new Date(dateStr);
    return d.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  } catch {
    return dateStr;
  }
}
