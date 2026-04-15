/**
 * Unified AI Analytics dashboard — single page showing all providers.
 * Merges rate limits, credits, token usage, and activity across providers.
 */
import { useEffect, useState, useCallback } from "react";
import {
  useAnalyticsStore,
  useAllRateLimits,
  useAllCredits,
} from "../stores/analyticsStore";
import { RateLimitBar } from "../components/analytics/RateLimitBar";
import { ProviderStatusGrid } from "../components/analytics/ProviderStatusGrid";
import { ProviderConnectModal } from "../components/analytics/ProviderConnectModal";
import { getAdapterIconImg } from "../lib/adapterPlugins";

// ── Section wrapper ─────────────────────────────────────────────────────────

function Section({
  title,
  icon,
  children,
  count,
  collapsible = true,
}: {
  title: string;
  icon: string;
  children: React.ReactNode;
  count?: number;
  collapsible?: boolean;
}) {
  const storageKey = `analytics-section-${title}`;
  const [expanded, setExpanded] = useState(() => {
    try {
      const stored = localStorage.getItem(storageKey);
      return stored !== "0";
    } catch {
      return true;
    }
  });

  const toggle = () => {
    const next = !expanded;
    setExpanded(next);
    try { localStorage.setItem(storageKey, next ? "1" : "0"); } catch {}
  };

  return (
    <div className="mb-4">
      <button
        onClick={collapsible ? toggle : undefined}
        className="flex items-center gap-2 w-full text-left mb-2 group"
      >
        {collapsible && (
          <svg
            className={`w-3 h-3 text-text-muted transition-transform duration-200 ${expanded ? "rotate-90" : ""}`}
            fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}
          >
            <path strokeLinecap="round" strokeLinejoin="round" d="M9 5l7 7-7 7" />
          </svg>
        )}
        <span className="text-xs font-semibold uppercase tracking-wider text-text-muted">
          {icon} {title}
        </span>
        {count !== undefined && (
          <span className="text-[10px] text-text-muted font-mono">({count})</span>
        )}
      </button>
      {expanded && <div>{children}</div>}
    </div>
  );
}

// ── Credits card ────────────────────────────────────────────────────────────

function CreditCard({ credit }: { credit: ReturnType<typeof useAllCredits>[0] }) {
  const icon = getAdapterIconImg(credit.provider_id);
  const pct = credit.limit ? Math.round((credit.used / credit.limit) * 100) : null;

  return (
    <div className="flex items-center gap-3 px-3 py-2.5 bg-[#1a1b23] rounded-lg border border-[#2a2b36]">
      <div className="w-5 h-5 shrink-0 flex items-center justify-center">
        {icon ? <img src={icon} alt="" className="w-4 h-4 object-contain" /> : null}
      </div>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-xs font-medium text-text-primary">
            {credit.currency === "USD" ? `$${credit.used.toFixed(2)}` : `${Math.round(credit.used)}`}
            {credit.limit != null && (
              <span className="text-text-muted">
                {" / "}
                {credit.currency === "USD" ? `$${credit.limit.toFixed(2)}` : `${Math.round(credit.limit)}`}
              </span>
            )}
          </span>
          <span className="text-[10px] text-text-muted">{credit.currency !== "USD" ? credit.currency : ""}</span>
        </div>
        {credit.plan_name && (
          <span className="text-[10px] text-text-muted">{credit.plan_name}</span>
        )}
      </div>
      {pct !== null && (
        <div className="w-16">
          <div className="h-1.5 bg-[#0e0f13] rounded-full overflow-hidden">
            <div
              className={`h-full rounded-full ${pct > 80 ? "bg-red-500" : pct > 50 ? "bg-amber-500" : "bg-emerald-500"}`}
              style={{ width: `${Math.min(pct, 100)}%` }}
            />
          </div>
        </div>
      )}
      {credit.billing_cycle_end && (
        <span className="text-[10px] text-text-muted shrink-0">
          Resets {new Date(credit.billing_cycle_end).toLocaleDateString()}
        </span>
      )}
    </div>
  );
}

// ── Filter bar ──────────────────────────────────────────────────────────────

function FilterBar() {
  const { providerFilter, setProviderFilter, providers, lastRefreshed, loading, fetchAllAnalytics } = useAnalyticsStore();
  const connected = providers.filter((p) => p.status.connected);

  return (
    <div className="flex items-center gap-2 flex-wrap">
      <button
        onClick={() => setProviderFilter(null)}
        className={`px-2.5 py-1 rounded-full text-[11px] font-medium transition-colors ${
          !providerFilter ? "bg-accent-blue text-white" : "bg-[#1a1b23] text-text-secondary hover:bg-[#22232e]"
        }`}
      >
        All
      </button>
      {connected.map((p) => {
        const icon = getAdapterIconImg(p.provider_id);
        return (
          <button
            key={p.provider_id}
            onClick={() => setProviderFilter(providerFilter === p.provider_id ? null : p.provider_id)}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded-full text-[11px] font-medium transition-colors ${
              providerFilter === p.provider_id
                ? "bg-accent-blue text-white"
                : "bg-[#1a1b23] text-text-secondary hover:bg-[#22232e]"
            }`}
          >
            {icon && <img src={icon} alt="" className="w-3 h-3 object-contain" />}
            {p.provider_name}
          </button>
        );
      })}

      <div className="flex-1" />

      {/* Last updated + refresh */}
      <span className="text-[10px] text-text-muted">
        {lastRefreshed ? `Updated ${timeAgo(lastRefreshed)}` : ""}
      </span>
      <button
        onClick={fetchAllAnalytics}
        disabled={loading}
        className={`p-1.5 rounded-lg text-text-muted hover:text-text-primary hover:bg-[#1a1b23] transition-colors ${loading ? "animate-spin" : ""}`}
        title="Refresh all"
      >
        <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2}>
          <path strokeLinecap="round" strokeLinejoin="round" d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15" />
        </svg>
      </button>
    </div>
  );
}

function timeAgo(iso: string): string {
  const s = Math.floor((Date.now() - new Date(iso).getTime()) / 1000);
  if (s < 60) return "just now";
  if (s < 3600) return `${Math.floor(s / 60)}m ago`;
  return `${Math.floor(s / 3600)}h ago`;
}

// ── Main page ───────────────────────────────────────────────────────────────

export function UnifiedAnalyticsPage() {
  const { fetchAllAnalytics, fetchProviderInfo, providers, providerInfo, loading, error } = useAnalyticsStore();
  const rateLimits = useAllRateLimits();
  const credits = useAllCredits();
  const [connectModal, setConnectModal] = useState<{ id: string; name: string; authType: string } | null>(null);

  useEffect(() => {
    fetchProviderInfo();
    fetchAllAnalytics();

    // Auto-refresh every 2 minutes
    const interval = setInterval(() => fetchAllAnalytics(), 120_000);
    return () => clearInterval(interval);
  }, [fetchAllAnalytics, fetchProviderInfo]);

  const handleConnect = useCallback((providerId: string) => {
    const info = providerInfo.find((p) => p.id === providerId);
    const provider = providers.find((p) => p.provider_id === providerId);
    setConnectModal({
      id: providerId,
      name: info?.name || provider?.provider_name || providerId,
      authType: info?.auth_type || "token",
    });
  }, [providerInfo, providers]);

  const connectedCount = providers.filter((p) => p.status.connected).length;

  return (
    <div className="h-full overflow-y-auto">
      <div className="max-w-5xl mx-auto px-6 py-6">
        {/* Header */}
        <div className="mb-6">
          <h1 className="text-lg font-semibold text-text-primary mb-1">AI Analytics</h1>
          <p className="text-xs text-text-muted">
            Unified view across {connectedCount} connected provider{connectedCount !== 1 ? "s" : ""}
          </p>
        </div>

        {/* Filter bar */}
        <div className="mb-5">
          <FilterBar />
        </div>

        {/* Loading state */}
        {loading && providers.length === 0 && (
          <div className="flex items-center justify-center py-20">
            <div className="text-sm text-text-muted">Loading analytics...</div>
          </div>
        )}

        {/* Error state */}
        {error && (
          <div className="mb-4 px-4 py-3 bg-red-500/10 border border-red-500/20 rounded-lg text-xs text-red-400">
            {error}
          </div>
        )}

        {/* Rate Limits */}
        {rateLimits.length > 0 && (
          <Section title="Rate Limits" icon="⟐" count={rateLimits.length}>
            <div className="bg-[#13141a] rounded-xl border border-[#2a2b36] px-4 py-3">
              {rateLimits.map((w, i) => (
                <RateLimitBar key={`${w.provider_id}-${w.label}-${i}`} window={w} />
              ))}
            </div>
          </Section>
        )}

        {/* Credits & Spend */}
        {credits.length > 0 && (
          <Section title="Credits & Spend" icon="$" count={credits.length}>
            <div className="space-y-2">
              {credits.map((c) => (
                <CreditCard key={c.provider_id} credit={c} />
              ))}
            </div>
          </Section>
        )}

        {/* Token Usage — link to per-adapter analytics for detailed charts */}
        {providers.some((p) => p.status.connected && (p.provider_id === "claude-code" || p.provider_id === "cursor" || p.provider_id === "codex")) && (
          <Section title="Token Usage" icon="▤">
            <div className="bg-[#13141a] rounded-xl border border-[#2a2b36] p-4">
              <p className="text-xs text-text-muted mb-3">
                Detailed token charts are available in each adapter's analytics page.
              </p>
              <div className="flex gap-2 flex-wrap">
                {providers.filter((p) => p.status.connected).map((p) => {
                  const icon = getAdapterIconImg(p.provider_id);
                  const route =
                    p.provider_id === "claude-code" ? "/adapters/claude-code/usage" :
                    p.provider_id === "cursor" ? "/adapters/cursor/attribution" :
                    null;
                  if (!route) return null;
                  return (
                    <a
                      key={p.provider_id}
                      href={route}
                      className="flex items-center gap-1.5 px-3 py-1.5 bg-[#1a1b23] rounded-lg text-xs text-accent-blue hover:bg-[#22232e] transition-colors"
                    >
                      {icon && <img src={icon} alt="" className="w-3.5 h-3.5 object-contain" />}
                      {p.provider_name} Analytics
                    </a>
                  );
                })}
              </div>
            </div>
          </Section>
        )}

        {/* Provider-specific insights */}
        {providers.filter((p) => p.status.connected && Object.keys(p.extra).length > 0).length > 0 && (
          <Section title="Provider Insights" icon="◈">
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2">
              {providers
                .filter((p) => p.status.connected && Object.keys(p.extra).length > 0)
                .map((p) => {
                  const icon = getAdapterIconImg(p.provider_id);
                  return (
                    <div
                      key={p.provider_id}
                      className="bg-[#1a1b23] rounded-lg border border-[#2a2b36] px-4 py-3"
                    >
                      <div className="flex items-center gap-2 mb-2">
                        {icon && <img src={icon} alt="" className="w-4 h-4 object-contain" />}
                        <span className="text-xs font-medium text-text-primary">{p.provider_name}</span>
                      </div>
                      <div className="space-y-1">
                        {Object.entries(p.extra).map(([key, value]) => (
                          <div key={key} className="flex justify-between text-[11px]">
                            <span className="text-text-muted capitalize">{key.replace(/_/g, " ")}</span>
                            <span className="text-text-secondary font-mono">
                              {typeof value === "boolean" ? (value ? "Yes" : "No") : String(value)}
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  );
                })}
            </div>
          </Section>
        )}

        {/* Provider Status */}
        <Section title="Provider Status" icon="⊛" count={providers.length}>
          <ProviderStatusGrid providers={providers} onConnect={handleConnect} />
        </Section>
      </div>

      {/* Connect modal */}
      {connectModal && (
        <ProviderConnectModal
          providerId={connectModal.id}
          providerName={connectModal.name}
          authType={connectModal.authType}
          onClose={() => setConnectModal(null)}
        />
      )}
    </div>
  );
}
