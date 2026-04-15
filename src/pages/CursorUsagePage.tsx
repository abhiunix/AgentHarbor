import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";

interface AiCodeSummary {
  total_commits: number;
  avg_ai_percentage: number;
  total_composer_lines: number;
  total_human_lines: number;
  total_lines_added: number;
  total_lines_deleted: number;
}

interface ConversationSummary {
  conversation_id: string;
  title?: string;
  tldr?: string;
  overview?: string;
  summary_bullets?: string;
  model?: string;
  mode?: string;
  updated_at: number;
}

export function CursorUsagePage() {
  const [summary, setSummary] = useState<AiCodeSummary | null>(null);
  const [modelBreakdown, setModelBreakdown] = useState<Record<string, number>>({});
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expandedConvo, setExpandedConvo] = useState<string | null>(null);

  useEffect(() => {
    loadAll();
  }, []);

  async function loadAll() {
    setLoading(true);
    setError(null);
    try {
      const [sum, models, convos] = await Promise.all([
        invoke<AiCodeSummary>("get_ai_code_summary").catch(() => null),
        invoke<Record<string, number>>("get_ai_tracking_model_breakdown").catch(() => ({})),
        invoke<ConversationSummary[]>("get_conversation_summaries", {
          limit: null,
          offset: null,
        }).catch(() => []),
      ]);
      setSummary(sum);
      setModelBreakdown(models ?? {});
      setConversations(convos ?? []);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  const modelEntries = Object.entries(modelBreakdown).sort(
    (a, b) => b[1] - a[1]
  );

  return (
    <div className="h-full flex flex-col">
      <div className="px-6 pt-6 pb-4">
        <div className="flex items-center justify-between flex-wrap gap-3 mb-4">
          <div className="flex items-center gap-3">
            <span className="w-3 h-3 rounded-full bg-blue-500 flex-shrink-0" />
            <div>
              <h1 className="text-2xl font-semibold text-text-primary">
                Cursor — Token Usage
              </h1>
              <p className="text-sm text-text-secondary">
                AI code tracking stats from ai-tracking.db
              </p>
            </div>
          </div>
          <button
            onClick={() => loadAll()}
            disabled={loading}
            className="px-3 py-1.5 text-sm bg-app-card border border-border rounded-lg hover:bg-app-card-hover text-text-primary disabled:opacity-50"
          >
            Refresh
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-6 pb-6 space-y-6">
        {loading && (
          <div className="text-text-secondary text-sm">Loading usage data...</div>
        )}

        {error && (
          <div className="px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400">
            {error}
          </div>
        )}

        {!loading && !error && !summary && modelEntries.length === 0 && conversations.length === 0 && (
          <div className="text-center py-12">
            <p className="text-text-secondary text-sm">
              No AI tracking data found.
            </p>
            <p className="text-text-muted text-xs mt-1">
              The ai-tracking.db may not exist or may be empty.
            </p>
          </div>
        )}

        {/* Summary cards */}
        {summary && (
          <div className="grid grid-cols-2 md:grid-cols-3 gap-4">
            <StatCard
              label="Total Commits"
              value={summary.total_commits.toLocaleString()}
            />
            <StatCard
              label="Avg AI %"
              value={`${summary.avg_ai_percentage.toFixed(1)}%`}
              color="text-blue-400"
            />
            <StatCard
              label="AI Lines"
              value={summary.total_composer_lines.toLocaleString()}
              color="text-purple-400"
            />
            <StatCard
              label="Human Lines"
              value={summary.total_human_lines.toLocaleString()}
              color="text-green-400"
            />
            <StatCard
              label="Lines Added"
              value={summary.total_lines_added.toLocaleString()}
              color="text-cyan-400"
            />
            <StatCard
              label="Lines Deleted"
              value={summary.total_lines_deleted.toLocaleString()}
              color="text-red-400"
            />
          </div>
        )}

        {/* Model breakdown */}
        {modelEntries.length > 0 && (
          <div className="bg-app-card border border-border rounded-lg p-4">
            <h3 className="text-sm font-semibold text-text-primary mb-3">
              Model Breakdown
            </h3>
            <div className="space-y-2">
              {modelEntries.map(([model, count]) => {
                const maxCount = modelEntries[0]?.[1] ?? 1;
                const pct = maxCount > 0 ? (count / maxCount) * 100 : 0;
                return (
                  <div key={model} className="flex items-center gap-3">
                    <span className="text-xs text-text-primary font-mono w-48 truncate flex-shrink-0">
                      {model}
                    </span>
                    <div className="flex-1 h-4 bg-[#13141a] rounded overflow-hidden">
                      <div
                        className="h-full bg-blue-500/40 rounded"
                        style={{ width: `${pct}%` }}
                      />
                    </div>
                    <span className="text-xs text-text-secondary w-12 text-right flex-shrink-0">
                      {count}
                    </span>
                  </div>
                );
              })}
            </div>
          </div>
        )}

        {/* Conversations */}
        {conversations.length > 0 && (
          <div>
            <h3 className="text-sm font-semibold text-text-primary mb-3">
              Conversations ({conversations.length})
            </h3>
            <div className="space-y-2">
              {conversations.map((convo) => {
                const isExpanded = expandedConvo === convo.conversation_id;
                return (
                  <div
                    key={convo.conversation_id}
                    className="bg-app-card border border-border rounded-lg overflow-hidden"
                  >
                    <button
                      onClick={() =>
                        setExpandedConvo(
                          isExpanded ? null : convo.conversation_id
                        )
                      }
                      className="w-full px-4 py-3 flex items-center gap-3 text-left hover:bg-app-card-hover transition-colors"
                    >
                      <span className="text-text-secondary text-sm flex-shrink-0">
                        {isExpanded ? "\u25BC" : "\u25B6"}
                      </span>
                      <div className="flex-1 min-w-0">
                        <h4 className="text-sm font-medium text-text-primary truncate">
                          {convo.title || convo.conversation_id.slice(0, 12)}
                        </h4>
                        {convo.tldr && (
                          <p className="text-xs text-text-secondary truncate mt-0.5">
                            {convo.tldr}
                          </p>
                        )}
                      </div>
                      {convo.model && (
                        <span className="px-1.5 py-0.5 text-[10px] font-medium rounded bg-blue-500/20 text-blue-400 flex-shrink-0">
                          {convo.model}
                        </span>
                      )}
                      <span className="text-xs text-text-muted flex-shrink-0">
                        {formatTimestamp(convo.updated_at)}
                      </span>
                    </button>
                    {isExpanded && (
                      <div className="border-t border-border px-4 py-3 space-y-2">
                        {convo.overview && (
                          <div>
                            <span className="text-[10px] text-text-muted uppercase">
                              Overview
                            </span>
                            <p className="text-xs text-text-primary mt-0.5">
                              {convo.overview}
                            </p>
                          </div>
                        )}
                        {convo.summary_bullets && (
                          <div>
                            <span className="text-[10px] text-text-muted uppercase">
                              Summary
                            </span>
                            <p className="text-xs text-text-primary mt-0.5 whitespace-pre-wrap">
                              {convo.summary_bullets}
                            </p>
                          </div>
                        )}
                        {convo.mode && (
                          <div className="flex items-center gap-2">
                            <span className="text-[10px] text-text-muted uppercase">
                              Mode
                            </span>
                            <span className="text-xs text-text-secondary">
                              {convo.mode}
                            </span>
                          </div>
                        )}
                        <div className="flex items-center gap-2">
                          <span className="text-[10px] text-text-muted uppercase">
                            ID
                          </span>
                          <span className="text-xs text-text-secondary font-mono">
                            {convo.conversation_id}
                          </span>
                        </div>
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function StatCard({
  label,
  value,
  color = "text-text-primary",
}: {
  label: string;
  value: string;
  color?: string;
}) {
  return (
    <div className="bg-app-card border border-border rounded-lg p-4">
      <div className="text-xs text-text-muted uppercase tracking-wider mb-1">
        {label}
      </div>
      <div className={`text-2xl font-bold ${color}`}>{value}</div>
    </div>
  );
}

function formatTimestamp(ts: number): string {
  try {
    const d = new Date(ts * 1000);
    return d.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  } catch {
    return String(ts);
  }
}
