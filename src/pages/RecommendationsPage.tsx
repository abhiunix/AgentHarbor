import { useState, useEffect, useCallback } from "react";
import {
  getRecommendations,
  hasAnthropicApiKey,
  storeSecret,
  clearRecommendationsCache,
  previewDeploy,
  executeDeploy,
  type Recommendation,
  type RecommendationsPayload,
  type DiffEntry,
} from "../lib/tauri";

const PRIORITY_STYLES: Record<string, string> = {
  high: "bg-accent-red-dim text-accent-red",
  medium: "bg-accent-yellow-dim text-accent-yellow",
  low: "bg-accent-cyan-dim text-accent-cyan",
};

const TYPE_BADGES: Record<string, string> = {
  mcp: "bg-accent-purple-dim text-accent-purple",
  rule: "bg-accent-blue-dim text-accent-blue",
  skill: "bg-accent-green-dim text-accent-green",
  hook: "bg-accent-orange-dim text-accent-orange",
  plugin: "bg-accent-pink-dim text-accent-pink",
  custom: "bg-app-card-hover text-text-secondary",
};

function ApiKeySetup({ onSaved }: { onSaved: () => void }) {
  const [key, setKey] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSave = async () => {
    if (!key.trim()) return;
    setSaving(true);
    setError(null);
    try {
      await storeSecret("anthropic_api_key", key.trim());
      onSaved();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="max-w-xl mx-auto mt-12 p-6 bg-app-card border border-border rounded-lg">
      <h2 className="text-lg font-semibold text-text-primary mb-2">
        Connect Anthropic API
      </h2>
      <p className="text-sm text-text-secondary mb-4">
        AI recommendations use Claude to analyze your installed tools, projects,
        and registry — and suggest capabilities worth deploying. Paste an
        Anthropic API key to get started. The key is stored in your OS Keychain.
      </p>
      <input
        type="password"
        value={key}
        onChange={(e) => setKey(e.target.value)}
        placeholder="sk-ant-..."
        className="w-full bg-app-input border border-border rounded-md px-3 py-2 text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent-blue"
      />
      {error && (
        <p className="mt-2 text-xs text-accent-red">{error}</p>
      )}
      <button
        onClick={handleSave}
        disabled={!key.trim() || saving}
        className="mt-4 px-4 py-2 bg-accent-blue text-white text-sm font-medium rounded-md disabled:opacity-50 disabled:cursor-not-allowed hover:opacity-90"
      >
        {saving ? "Saving…" : "Save API Key"}
      </button>
      <p className="mt-3 text-xs text-text-muted">
        Get a key at{" "}
        <span className="font-mono">console.anthropic.com</span>. We default to
        Claude Haiku 4.5 — a typical recommendation run costs less than $0.01.
      </p>
    </div>
  );
}

function PriorityBadge({ priority }: { priority: string }) {
  const cls = PRIORITY_STYLES[priority] ?? PRIORITY_STYLES.medium;
  return (
    <span
      className={`text-[10px] font-semibold uppercase tracking-wider px-2 py-0.5 rounded ${cls}`}
    >
      {priority}
    </span>
  );
}

function TypeBadge({ type }: { type: string | null }) {
  if (!type) return null;
  const cls = TYPE_BADGES[type] ?? TYPE_BADGES.custom;
  return (
    <span
      className={`text-[10px] font-mono uppercase px-1.5 py-0.5 rounded ${cls}`}
    >
      {type}
    </span>
  );
}

interface RecommendationCardProps {
  rec: Recommendation;
  onDeploy: (rec: Recommendation) => void;
  busyId: string | null;
}

function RecommendationCard({ rec, onDeploy, busyId }: RecommendationCardProps) {
  const canDeploy =
    rec.action === "deploy" && rec.capability_id && rec.target_adapter_id;
  const isBusy = busyId === rec.id;

  return (
    <div className="bg-app-card border border-border rounded-lg p-4 hover:border-border-light transition-colors">
      <div className="flex items-start justify-between gap-3 mb-2">
        <div className="flex items-center gap-2 flex-wrap">
          <PriorityBadge priority={rec.priority} />
          <TypeBadge type={rec.capability_type} />
          {rec.target_adapter_name && (
            <span className="text-xs text-text-secondary">
              → <span className="font-medium text-text-primary">{rec.target_adapter_name}</span>
            </span>
          )}
        </div>
      </div>

      <h3 className="text-sm font-semibold text-text-primary mb-1">
        {rec.capability_name ?? "Suggestion"}
      </h3>

      {rec.capability_id && (
        <p className="text-[11px] font-mono text-text-muted mb-2">
          {rec.capability_id}
        </p>
      )}

      <p className="text-sm text-text-secondary leading-relaxed mb-4">
        {rec.reason}
      </p>

      <div className="flex items-center gap-2">
        <button
          onClick={() => onDeploy(rec)}
          disabled={!canDeploy || isBusy}
          className="px-3 py-1.5 bg-accent-blue text-white text-xs font-medium rounded-md disabled:opacity-40 disabled:cursor-not-allowed hover:opacity-90"
        >
          {isBusy ? "Deploying…" : canDeploy ? "Deploy globally" : "No action"}
        </button>
      </div>
    </div>
  );
}

interface DeployToastState {
  kind: "success" | "error";
  text: string;
}

export function RecommendationsPage() {
  const [hasKey, setHasKey] = useState<boolean | null>(null);
  const [payload, setPayload] = useState<RecommendationsPayload | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);
  const [toast, setToast] = useState<DeployToastState | null>(null);
  const [previewDiff, setPreviewDiff] = useState<{
    rec: Recommendation;
    entries: DiffEntry[];
  } | null>(null);

  const checkKey = useCallback(async () => {
    try {
      const has = await hasAnthropicApiKey();
      setHasKey(has);
    } catch {
      setHasKey(false);
    }
  }, []);

  const load = useCallback(
    async (force: boolean) => {
      setLoading(true);
      setError(null);
      try {
        const result = await getRecommendations(force);
        setPayload(result);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setLoading(false);
      }
    },
    []
  );

  useEffect(() => {
    checkKey();
  }, [checkKey]);

  useEffect(() => {
    if (hasKey) load(false);
  }, [hasKey, load]);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => setToast(null), 4000);
    return () => clearTimeout(t);
  }, [toast]);

  const handleDeploy = async (rec: Recommendation) => {
    if (!rec.capability_id || !rec.target_adapter_id) return;
    setBusyId(rec.id);
    setError(null);
    try {
      // Show diff preview first so the user can confirm.
      const entries = await previewDeploy(
        "",
        rec.target_adapter_id,
        [rec.capability_id],
        [],
        undefined,
        true
      );
      setPreviewDiff({ rec, entries });
    } catch (e) {
      setToast({
        kind: "error",
        text: `Preview failed: ${e instanceof Error ? e.message : String(e)}`,
      });
    } finally {
      setBusyId(null);
    }
  };

  const confirmDeploy = async () => {
    if (!previewDiff) return;
    const { rec } = previewDiff;
    if (!rec.capability_id || !rec.target_adapter_id) return;
    setBusyId(rec.id);
    setError(null);
    try {
      const result = await executeDeploy(
        "",
        rec.target_adapter_id,
        [rec.capability_id],
        [],
        {},
        undefined,
        true
      );
      if (result.success) {
        setToast({
          kind: "success",
          text: `Deployed ${rec.capability_name ?? rec.capability_id} to ${rec.target_adapter_name}.`,
        });
        // Invalidate cache so the next refresh excludes this capability.
        await clearRecommendationsCache();
        await load(true);
      } else {
        setToast({
          kind: "error",
          text: result.errors.join("; ") || "Deploy failed.",
        });
      }
    } catch (e) {
      setToast({
        kind: "error",
        text: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setBusyId(null);
      setPreviewDiff(null);
    }
  };

  if (hasKey === null) {
    return (
      <div className="p-6 text-text-muted text-sm">Checking API key…</div>
    );
  }

  if (!hasKey) {
    return (
      <ApiKeySetup
        onSaved={() => {
          setHasKey(true);
          load(true);
        }}
      />
    );
  }

  return (
    <div className="h-full overflow-y-auto">
      <div className="max-w-4xl mx-auto p-6">
        {/* Header */}
        <div className="flex items-start justify-between mb-6">
          <div>
            <h1 className="text-2xl font-semibold text-text-primary mb-1">
              AI Recommendations
            </h1>
            <p className="text-sm text-text-secondary">
              Claude analyzes your installed AI tools, projects, and registry to
              suggest high-leverage actions.
            </p>
          </div>
          <button
            onClick={() => load(true)}
            disabled={loading}
            className="px-3 py-1.5 bg-app-card border border-border text-text-primary text-xs font-medium rounded-md disabled:opacity-50 hover:border-border-light"
          >
            {loading ? "Thinking…" : "Refresh"}
          </button>
        </div>

        {/* Summary */}
        {payload?.summary && !loading && (
          <div className="mb-4 p-3 bg-accent-blue-glow border border-accent-blue/30 rounded-md text-sm text-text-primary">
            {payload.summary}
          </div>
        )}

        {payload && (
          <div className="mb-4 text-xs text-text-muted">
            {payload.from_cache ? "Cached" : "Fresh"} •{" "}
            {new Date(payload.generated_at).toLocaleString()} •{" "}
            {payload.recommendations.length} recommendation
            {payload.recommendations.length === 1 ? "" : "s"}
          </div>
        )}

        {/* Loading */}
        {loading && !payload && (
          <div className="space-y-3">
            {[0, 1, 2].map((i) => (
              <div
                key={i}
                className="h-32 bg-app-card border border-border rounded-lg animate-pulse"
              />
            ))}
          </div>
        )}

        {/* Error */}
        {error && (
          <div className="mb-4 p-3 bg-accent-red-dim border border-accent-red/30 rounded-md text-sm text-accent-red">
            {error}
          </div>
        )}

        {/* Recommendations */}
        {payload && payload.recommendations.length === 0 && !loading && (
          <div className="text-center py-12 text-text-muted text-sm">
            Nothing to suggest right now. Your setup looks balanced — try
            refreshing after you deploy more or connect a provider.
          </div>
        )}

        <div className="space-y-3">
          {payload?.recommendations.map((rec) => (
            <RecommendationCard
              key={rec.id}
              rec={rec}
              onDeploy={handleDeploy}
              busyId={busyId}
            />
          ))}
        </div>
      </div>

      {/* Preview / confirm dialog */}
      {previewDiff && (
        <div
          className="fixed inset-0 z-50 bg-black/60 flex items-center justify-center p-4"
          onClick={() => setPreviewDiff(null)}
        >
          <div
            className="bg-app-modal border border-border rounded-lg max-w-2xl w-full max-h-[80vh] flex flex-col shadow-modal"
            onClick={(e) => e.stopPropagation()}
          >
            <div className="px-5 py-4 border-b border-border">
              <h2 className="text-sm font-semibold text-text-primary">
                Deploy {previewDiff.rec.capability_name} to{" "}
                {previewDiff.rec.target_adapter_name}
              </h2>
              <p className="text-xs text-text-muted mt-1">
                {previewDiff.entries.length} file
                {previewDiff.entries.length === 1 ? "" : "s"} will change.
              </p>
            </div>
            <div className="flex-1 overflow-y-auto p-4 space-y-2">
              {previewDiff.entries.length === 0 ? (
                <p className="text-sm text-text-secondary">
                  No file changes — the target may already be up to date.
                </p>
              ) : (
                previewDiff.entries.map((entry, i) => (
                  <div
                    key={i}
                    className="bg-app-card border border-border rounded-md p-3"
                  >
                    <div className="flex items-center gap-2 mb-1">
                      <span
                        className={`text-[10px] font-mono uppercase px-1.5 py-0.5 rounded ${
                          entry.change_type === "add"
                            ? "bg-accent-green-dim text-accent-green"
                            : entry.change_type === "modify"
                            ? "bg-accent-yellow-dim text-accent-yellow"
                            : "bg-accent-red-dim text-accent-red"
                        }`}
                      >
                        {entry.change_type}
                      </span>
                      <span className="text-xs font-mono text-text-secondary truncate">
                        {entry.file_path}
                      </span>
                    </div>
                  </div>
                ))
              )}
            </div>
            <div className="px-5 py-3 border-t border-border flex items-center justify-end gap-2">
              <button
                onClick={() => setPreviewDiff(null)}
                className="px-3 py-1.5 text-xs text-text-secondary hover:text-text-primary"
              >
                Cancel
              </button>
              <button
                onClick={confirmDeploy}
                disabled={busyId !== null}
                className="px-3 py-1.5 bg-accent-blue text-white text-xs font-medium rounded-md disabled:opacity-50"
              >
                {busyId ? "Deploying…" : "Confirm Deploy"}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Toast */}
      {toast && (
        <div
          className={`fixed bottom-4 right-4 px-4 py-3 rounded-md text-sm shadow-modal border ${
            toast.kind === "success"
              ? "bg-accent-green-dim border-accent-green/40 text-accent-green"
              : "bg-accent-red-dim border-accent-red/40 text-accent-red"
          }`}
        >
          {toast.text}
        </div>
      )}
    </div>
  );
}
