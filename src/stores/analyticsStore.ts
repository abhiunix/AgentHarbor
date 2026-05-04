/**
 * Zustand store for the unified AI Analytics dashboard.
 * Manages provider analytics data, connection states, and refresh logic.
 */
import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";

// ── Types matching Rust analytics::types ──────────────────────────────────────

export interface RateLimitWindow {
  provider_id: string;
  label: string;
  used_percent: number;
  remaining_percent: number;
  resets_at: string | null;
  resets_in_seconds: number | null;
  window_seconds: number | null;
}

export interface CreditUsage {
  provider_id: string;
  used: number;
  limit: number | null;
  remaining: number;
  currency: string;
  billing_cycle_end: string | null;
  plan_name: string | null;
}

export interface TokenCounts {
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  estimated_cost_usd: number | null;
}

export interface ProviderStatus {
  provider_id: string;
  provider_name: string;
  connected: boolean;
  connection_method: string;
  account_email: string | null;
  plan_name: string | null;
  org_name: string | null;
  error: string | null;
}

/** Mirrors Rust `analytics::types::LimitState` (tag = kind, snake_case). */
export type LimitState =
  | { kind: "healthy" }
  | {
      kind: "approaching";
      worst_pct: number;
      label: string;
      resets_at: string | null;
      scope: string;
    }
  | {
      kind: "reached";
      scope: string;
      used_pct: number;
      cap: number | null;
      resets_at: string | null;
    }
  | {
      kind: "api_disabled";
      reason: string;
      until: string | null;
      org_name: string;
    }
  | { kind: "subscription_issue"; status: string; org_name: string }
  | { kind: "billable_paused"; until: string; org_name: string }
  | {
      kind: "rate_limited";
      retry_after_secs: number | null;
      message: string;
    }
  | { kind: "unauthenticated"; message: string };

export interface ProviderAnalytics {
  provider_id: string;
  provider_name: string;
  status: ProviderStatus;
  rate_limits: RateLimitWindow[];
  credit_usage: CreditUsage | null;
  token_counts: TokenCounts | null;
  limit_state?: LimitState | null;
  extra: Record<string, unknown>;
  fetched_at: string;
}

export interface ProviderInfo {
  id: string;
  name: string;
  auth_type: string;
  description: string;
  has_local_data: boolean;
  has_api: boolean;
}

// ── Store ─────────────────────────────────────────────────────────────────────

interface AnalyticsState {
  /** All provider analytics data */
  providers: ProviderAnalytics[];
  /** Provider info registry */
  providerInfo: ProviderInfo[];
  /** Loading state */
  loading: boolean;
  /** Error message */
  error: string | null;
  /** Last full refresh timestamp */
  lastRefreshed: string | null;
  /** Provider filter (null = show all) */
  providerFilter: string | null;

  // Actions
  fetchAllAnalytics: () => Promise<void>;
  fetchProviderAnalytics: (providerId: string) => Promise<void>;
  fetchProviderInfo: () => Promise<void>;
  fetchStatuses: () => Promise<void>;
  setProviderFilter: (providerId: string | null) => void;
  saveProviderToken: (providerId: string, keyType: string, value: string) => Promise<void>;
  deleteProviderToken: (providerId: string, keyType: string) => Promise<void>;
}

export const useAnalyticsStore = create<AnalyticsState>((set, get) => ({
  providers: [],
  providerInfo: [],
  loading: false,
  error: null,
  lastRefreshed: null,
  providerFilter: null,

  fetchAllAnalytics: async () => {
    set({ loading: true, error: null });
    try {
      const data = await invoke<ProviderAnalytics[]>("get_all_provider_analytics");
      set({
        providers: data,
        loading: false,
        lastRefreshed: new Date().toISOString(),
      });
    } catch (e) {
      set({ loading: false, error: String(e) });
    }
  },

  fetchProviderAnalytics: async (providerId: string) => {
    try {
      const data = await invoke<ProviderAnalytics>("get_provider_analytics", { providerId });
      set((state) => ({
        providers: state.providers.map((p) =>
          p.provider_id === providerId ? data : p
        ).concat(
          state.providers.some((p) => p.provider_id === providerId) ? [] : [data]
        ),
      }));
    } catch (e) {
      console.error(`Failed to fetch ${providerId} analytics:`, e);
    }
  },

  fetchProviderInfo: async () => {
    try {
      const info = await invoke<ProviderInfo[]>("get_all_provider_info");
      set({ providerInfo: info });
    } catch (e) {
      console.error("Failed to fetch provider info:", e);
    }
  },

  fetchStatuses: async () => {
    try {
      const statuses = await invoke<ProviderStatus[]>("get_all_provider_status");
      // Update existing providers' status or add new ones
      set((state) => {
        const map = new Map(state.providers.map((p) => [p.provider_id, p]));
        for (const s of statuses) {
          const existing = map.get(s.provider_id);
          if (existing) {
            map.set(s.provider_id, { ...existing, status: s });
          } else {
            map.set(s.provider_id, {
              provider_id: s.provider_id,
              provider_name: s.provider_name,
              status: s,
              rate_limits: [],
              credit_usage: null,
              token_counts: null,
              extra: {},
              fetched_at: new Date().toISOString(),
            });
          }
        }
        return { providers: Array.from(map.values()) };
      });
    } catch (e) {
      console.error("Failed to fetch statuses:", e);
    }
  },

  setProviderFilter: (providerId) => set({ providerFilter: providerId }),

  saveProviderToken: async (providerId, keyType, value) => {
    await invoke("save_provider_token", { providerId, keyType, value });
    // Refresh this provider after saving token
    get().fetchProviderAnalytics(providerId);
  },

  deleteProviderToken: async (providerId, keyType) => {
    await invoke("delete_provider_token", { providerId, keyType });
    get().fetchStatuses();
  },
}));

// ── Derived selectors ─────────────────────────────────────────────────────────

/** Get all rate limits across all providers, sorted by urgency (lowest remaining first) */
export function useAllRateLimits(): RateLimitWindow[] {
  const providers = useAnalyticsStore((s) => s.providers);
  const filter = useAnalyticsStore((s) => s.providerFilter);
  return providers
    .filter((p) => !filter || p.provider_id === filter)
    .flatMap((p) => p.rate_limits)
    .sort((a, b) => a.remaining_percent - b.remaining_percent);
}

/** Get all credit usages across providers */
export function useAllCredits(): CreditUsage[] {
  const providers = useAnalyticsStore((s) => s.providers);
  const filter = useAnalyticsStore((s) => s.providerFilter);
  return providers
    .filter((p) => !filter || p.provider_id === filter)
    .map((p) => p.credit_usage)
    .filter((c): c is CreditUsage => c !== null);
}

/** Get connected providers */
export function useConnectedProviders(): ProviderAnalytics[] {
  return useAnalyticsStore((s) => s.providers.filter((p) => p.status.connected));
}

/** Get disconnected providers */
export function useDisconnectedProviders(): ProviderAnalytics[] {
  return useAnalyticsStore((s) => s.providers.filter((p) => !p.status.connected));
}
