import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { emitTo } from "@tauri-apps/api/event";
import { getAdapterIconImg } from "../../lib/adapterPlugins";
import {
  TrayProviderCard,
  type RateLimitWindow,
  type TrayProviderSummary,
} from "./TrayProviderCard";
import {
  limitStateIsDanger,
  limitStateIsWarning,
} from "../analytics/LimitStateBanner";

// ── Types ────────────────────────────────────────────────────────────────────

interface TraySummary {
  providers: TrayProviderSummary[];
  connected_count: number;
  total_count: number;
  worst_rate_limit: RateLimitWindow | null;
  fetched_at: string;
}

// ── Provider config ──────────────────────────────────────────────────────────

interface ProviderTabConfig {
  id: string;
  name: string;
  accent: string;
  route: string;
}

const PROVIDER_TABS: ProviderTabConfig[] = [
  {
    id: "claude-code",
    name: "Claude Code",
    accent: "#d97706",
    route: "/adapters/claude-code/analytics-v2",
  },
  {
    id: "cursor",
    name: "Cursor",
    accent: "#2563eb",
    route: "/adapters/cursor/analytics-v2",
  },
  {
    id: "codex",
    name: "Codex",
    accent: "#10a37f",
    route: "/adapters/codex/analytics",
  },
  {
    id: "gemini",
    name: "Gemini",
    accent: "#4285f4",
    route: "/adapters/gemini/analytics",
  },
];

const TAB_STORAGE_KEY = "agentharbor-tray-active-tab";

/** Gemini: show Pro if it has quota, else Flash, else Flash Lite (CodexBar-style). */
const GEMINI_TIER_PRIORITY = ["Pro", "Flash", "Flash Lite"] as const;

function pickGeminiHighlightLimit(
  rateLimits: RateLimitWindow[]
): RateLimitWindow | null {
  if (rateLimits.length === 0) return null;
  const byLabel = new Map<string, RateLimitWindow>();
  for (const rl of rateLimits) {
    byLabel.set(rl.label, rl);
  }
  const REMAINING_EPS = 0.05;
  for (const tier of GEMINI_TIER_PRIORITY) {
    const rl = byLabel.get(tier);
    if (rl != null && rl.remaining_percent > REMAINING_EPS) {
      return rl;
    }
  }
  for (const tier of GEMINI_TIER_PRIORITY) {
    const rl = byLabel.get(tier);
    if (rl != null) return rl;
  }
  return null;
}

// ── TrayHealthBar ────────────────────────────────────────────────────────────

function formatEnterpriseSpend(amount: number, currency: string): string {
  const symbol = currency === "USD" ? "$" : `${currency} `;
  if (amount >= 10) return `${symbol}${Math.round(amount)}`;
  if (amount >= 1) return `${symbol}${amount.toFixed(1)}`;
  return `${symbol}${amount.toFixed(2)}`;
}

function TrayHealthBar({
  summary,
  activeProviderId,
  onShowApp,
}: {
  summary: TraySummary;
  activeProviderId: string;
  onShowApp: () => void;
}) {
  const activeProvider = summary.providers.find(
    (p) => p.provider_id === activeProviderId
  );
  const isUncappedEnterprise =
    activeProvider?.provider_id === "claude-code" &&
    (activeProvider?.extra as Record<string, unknown> | undefined)?.is_enterprise === true &&
    (activeProvider?.credit_usage?.limit == null);

  // Warning is scoped to the ACTIVE PROVIDER's own rate limits only.
  // Using the global worst_rate_limit bleeds Gemini's "Pro" quota label
  // onto the Codex/Cursor tab, which is confusing.
  const activeLimit = activeProvider?.limit_state ?? null;
  const lsDanger = limitStateIsDanger(activeLimit ?? undefined);
  const lsWarn = limitStateIsWarning(activeLimit ?? undefined);

  const activeProviderWorstRL = activeProvider?.rate_limits
    .filter((rl) => rl.remaining_percent < 30)
    .sort((a, b) => a.remaining_percent - b.remaining_percent)[0] ?? null;

  const geminiHighlightLimit =
    activeProviderId === "gemini" && activeProvider
      ? pickGeminiHighlightLimit(activeProvider.rate_limits)
      : null;

  const isWarning =
    !isUncappedEnterprise &&
    (lsWarn || (!lsDanger && activeProviderWorstRL != null));

  return (
    <div className="flex items-center justify-between px-3 py-2 border-b border-[#2a2b36]">
      {/* Left: connection dots + active agent count */}
      <div className="flex items-center gap-1.5 text-xs text-[#9394a1]">
        <div className="flex items-center gap-1">
          {PROVIDER_TABS.map((tab) => {
            const provider = summary.providers.find(
              (p) => p.provider_id === tab.id
            );
            const connected = provider?.connected ?? false;
            return (
              <div
                key={tab.id}
                className={`w-2 h-2 rounded-full ${
                  connected ? "bg-green-500" : "bg-[#2a2b36]"
                }`}
                title={`${tab.name}: ${connected ? "connected" : "disconnected"}`}
              />
            );
          })}
        </div>
        <span>
          {summary.connected_count}/{summary.total_count} connected
        </span>
      </div>

      {/* Right: Enterprise spend, active-provider rate limit warning, or button */}
      <div className="text-xs">
        {isUncappedEnterprise && activeProvider?.credit_usage ? (
          <span className="text-amber-400">
            Enterprise ·{" "}
            {formatEnterpriseSpend(
              activeProvider.credit_usage.used,
              activeProvider.credit_usage.currency
            )}
          </span>
        ) : lsDanger ? (
          <span className="text-red-400 font-medium">Limit / billing issue</span>
        ) : geminiHighlightLimit ? (
          <span
            className={
              geminiHighlightLimit.remaining_percent < 30
                ? lsWarn
                  ? "text-amber-400 font-medium"
                  : "text-yellow-400"
                : "text-[#9394a1]"
            }
          >
            {`${geminiHighlightLimit.label}: ${geminiHighlightLimit.remaining_percent.toFixed(0)}%`}
          </span>
        ) : isWarning && activeProviderWorstRL ? (
          <span
            className={lsWarn ? "text-amber-400 font-medium" : "text-yellow-400"}
          >
            {lsWarn && activeLimit?.kind === "approaching"
              ? `${activeLimit.label} · ${activeLimit.worst_pct.toFixed(0)}%`
              : `${activeProviderWorstRL.label}: ${activeProviderWorstRL.remaining_percent.toFixed(0)}%`}
          </span>
        ) : (
          <button
            onClick={onShowApp}
            className="text-blue-400 hover:text-blue-300 transition-colors font-medium"
          >
            Show AgentHarbor
          </button>
        )}
      </div>
    </div>
  );
}

// ── Loading skeleton ─────────────────────────────────────────────────────────

function LoadingSkeleton() {
  return (
    <div className="p-3 space-y-3 animate-pulse">
      {/* Health bar skeleton */}
      <div className="flex items-center justify-between py-2">
        <div className="flex items-center gap-1.5">
          <div className="flex gap-1">
            <div className="w-2 h-2 rounded-full bg-[#2a2b36]" />
            <div className="w-2 h-2 rounded-full bg-[#2a2b36]" />
            <div className="w-2 h-2 rounded-full bg-[#2a2b36]" />
          </div>
          <div className="h-3 w-20 rounded bg-[#2a2b36]" />
        </div>
        <div className="h-3 w-16 rounded bg-[#2a2b36]" />
      </div>
      {/* Tab skeleton */}
      <div className="flex gap-1">
        <div className="h-7 w-24 rounded bg-[#2a2b36]" />
        <div className="h-7 w-16 rounded bg-[#2a2b36]" />
        <div className="h-7 w-14 rounded bg-[#2a2b36]" />
      </div>
      {/* Card skeleton */}
      <div className="rounded-lg border border-[#2a2b36] bg-[#1a1b23] p-3 space-y-2">
        <div className="flex items-center gap-2">
          <div className="w-5 h-5 rounded bg-[#2a2b36]" />
          <div className="h-4 w-28 rounded bg-[#2a2b36]" />
        </div>
        <div className="h-1.5 w-full rounded bg-[#2a2b36]" />
        <div className="h-1.5 w-3/4 rounded bg-[#2a2b36]" />
        <div className="h-1.5 w-1/2 rounded bg-[#2a2b36]" />
      </div>
    </div>
  );
}

// ── TrayPopover ──────────────────────────────────────────────────────────────

export function TrayPopover() {
  const [summary, setSummary] = useState<TraySummary | null>(null);
  const [initialLoad, setInitialLoad] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showSettingsMenu, setShowSettingsMenu] = useState(false);
  const [activeTab, setActiveTab] = useState<string>(() => {
    try {
      return localStorage.getItem(TAB_STORAGE_KEY) || "claude-code";
    } catch {
      return "claude-code";
    }
  });
  const blurTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const settingsMenuRef = useRef<HTMLDivElement | null>(null);

  // Keep Rust tray state in sync with the selected provider tab.
  useEffect(() => {
    invoke("set_tray_active_provider", { providerId: activeTab }).catch(() => {});
  }, [activeTab]);

  // Apply tray-specific transparent body/root classes so the rounded tray shell
  // can reveal transparent corners without any white frame.
  useEffect(() => {
    const root = document.getElementById("root");
    document.body.classList.add("tray-popover-body");
    root?.classList.add("tray-popover-root");

    return () => {
      document.body.classList.remove("tray-popover-body");
      root?.classList.remove("tray-popover-root");
    };
  }, []);

  // ── Read cached data (instant, sub-ms) ──

  const loadCachedSummary = useCallback(async () => {
    try {
      const data = await invoke<TraySummary>("get_tray_summary");
      setSummary(data);
      setError(null);
      setInitialLoad(false);

      // Default to first connected provider if current tab is disconnected
      const currentProvider = data.providers.find(
        (p) => p.provider_id === activeTab
      );
      if (!currentProvider?.connected) {
        const firstConnected = data.providers.find((p) => p.connected);
        if (firstConnected) {
          setActiveTab(firstConnected.provider_id);
          try {
            localStorage.setItem(TAB_STORAGE_KEY, firstConnected.provider_id);
          } catch { /* ignore */ }
        }
      }
    } catch (e) {
      setError(String(e));
      setInitialLoad(false);
    }
  }, [activeTab]);

  // ── Fire-and-forget background refresh ──

  const triggerBackgroundRefresh = useCallback(() => {
    invoke("refresh_tray_data").catch(() => {});
  }, []);

  // ── Event listeners ──

  useEffect(() => {
    // On mount, read cached data immediately
    loadCachedSummary();

    // On tray click: instant cache read + fire-and-forget refresh
    let unlistenPopover: (() => void) | null = null;
    listen("tray-popover-opened", () => {
      loadCachedSummary();
      triggerBackgroundRefresh();
    }).then((fn) => { unlistenPopover = fn; });

    // Listen for background refresh completions (push from Rust)
    let unlistenUpdate: (() => void) | null = null;
    listen<TraySummary>("tray-data-updated", (event) => {
      setSummary(event.payload);
      setError(null);
      setInitialLoad(false);
    }).then((fn) => { unlistenUpdate = fn; });

    return () => {
      if (unlistenPopover) unlistenPopover();
      if (unlistenUpdate) unlistenUpdate();
    };
  }, [loadCachedSummary, triggerBackgroundRefresh]);

  // (No 120s polling interval — background Rust thread handles refresh + push)

  // ── Blur-to-close with 200ms debounce ──

  useEffect(() => {
    const hidePopover = () => {
      blurTimerRef.current = setTimeout(async () => {
        try {
          await getCurrentWindow().hide();
        } catch { /* ignore */ }
      }, 150);
    };

    const cancelHide = () => {
      if (blurTimerRef.current) {
        clearTimeout(blurTimerRef.current);
        blurTimerRef.current = null;
      }
    };

    // Browser-level blur (clicks inside same app)
    window.addEventListener("blur", hidePopover);
    window.addEventListener("focus", cancelHide);

    // Tauri-level window blur (clicks on other macOS apps)
    let unlistenTauri: (() => void) | null = null;
    const win = getCurrentWindow();
    win.onFocusChanged(({ payload: focused }) => {
      if (!focused) hidePopover();
      else cancelHide();
    }).then(fn => { unlistenTauri = fn; });

    return () => {
      window.removeEventListener("blur", hidePopover);
      window.removeEventListener("focus", cancelHide);
      if (unlistenTauri) unlistenTauri();
      if (blurTimerRef.current) clearTimeout(blurTimerRef.current);
    };
  }, []);

  // ── Escape key closes ──

  useEffect(() => {
    const handleKeyDown = async (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        try {
          await getCurrentWindow().hide();
        } catch { /* ignore */ }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  // ── Settings menu close-on-outside-click ──

  useEffect(() => {
    if (!showSettingsMenu) return;

    const handlePointerDown = (event: MouseEvent) => {
      if (
        settingsMenuRef.current &&
        !settingsMenuRef.current.contains(event.target as Node)
      ) {
        setShowSettingsMenu(false);
      }
    };

    window.addEventListener("mousedown", handlePointerDown);
    return () => window.removeEventListener("mousedown", handlePointerDown);
  }, [showSettingsMenu]);

  // ── Tab change handler ──

  const handleTabChange = (tabId: string) => {
    setShowSettingsMenu(false);
    setActiveTab(tabId);
    try {
      localStorage.setItem(TAB_STORAGE_KEY, tabId);
    } catch { /* ignore */ }
  };

  // ── Navigation helpers ──

  const navigateAndHide = async (route: string) => {
    try {
      await invoke("show_main_window");
    } catch { /* ignore */ }
    await emitTo("main", "navigate-to", route);
    try {
      await getCurrentWindow().hide();
    } catch { /* ignore */ }
  };

  const handleOpenDashboard = async () => {
    setShowSettingsMenu(false);
    const tab = PROVIDER_TABS.find((t) => t.id === activeTab);
    if (tab) {
      await navigateAndHide(tab.route);
    }
  };

  const handleShowApp = async () => {
    setShowSettingsMenu(false);
    try {
      await invoke("show_main_window");
    } catch { /* ignore */ }
    try {
      await getCurrentWindow().hide();
    } catch { /* ignore */ }
  };

  const handleOpenSettings = async () => {
    setShowSettingsMenu(false);
    await navigateAndHide("/settings");
  };

  const handleQuitApp = async () => {
    setShowSettingsMenu(false);
    try {
      await invoke("quit_app");
    } catch { /* ignore */ }
  };

  // ── Render ──

  // Only show skeleton on the very first load (app just launched, no cached data yet)
  if (initialLoad && !summary) {
    return (
      <div className="w-full h-screen bg-transparent overflow-hidden">
        <div className="w-full h-full bg-[#0e0f13] text-[#e8e9ed] overflow-hidden rounded-xl">
          <LoadingSkeleton />
        </div>
      </div>
    );
  }

  if (error && !summary) {
    return (
      <div className="w-full h-screen bg-transparent overflow-hidden">
        <div className="w-full h-full bg-[#0e0f13] text-[#e8e9ed] p-4 overflow-hidden rounded-xl">
          <div className="text-red-400 text-sm">Failed to load: {error}</div>
          <button
            onClick={loadCachedSummary}
            className="mt-2 text-xs px-3 py-1 rounded bg-[#2a2b36] text-[#e8e9ed] hover:bg-[#333440] transition-colors"
          >
            Retry
          </button>
        </div>
      </div>
    );
  }

  const activeProvider = summary?.providers.find(
    (p) => p.provider_id === activeTab
  );
  const activeTabConfig = PROVIDER_TABS.find((t) => t.id === activeTab);

  return (
    <div className="w-full h-screen bg-transparent overflow-hidden">
      <div className="w-full h-full flex flex-col bg-[#0e0f13] text-[#e8e9ed] overflow-hidden rounded-xl">
        {/* Health bar */}
        {summary && (
          <TrayHealthBar
            summary={summary}
            activeProviderId={activeTab}
            onShowApp={handleShowApp}
          />
        )}

        {/* Provider tabs */}
        <div className="flex items-center gap-0.5 px-2 pt-2 pb-1">
          {PROVIDER_TABS.map((tab) => {
            const isActive = activeTab === tab.id;
            const provider = summary?.providers.find(
              (p) => p.provider_id === tab.id
            );
            const connected = provider?.connected ?? false;
            const iconImg = getAdapterIconImg(tab.id);

            return (
              <button
                key={tab.id}
                onClick={() => handleTabChange(tab.id)}
                className={`flex items-center gap-1.5 px-2.5 py-1.5 rounded-md text-xs font-medium transition-colors ${
                  isActive
                    ? "text-[#e8e9ed]"
                    : "text-[#9394a1] hover:text-[#e8e9ed] hover:bg-[#1a1b23]"
                }`}
                style={
                  isActive
                    ? { backgroundColor: `${tab.accent}20`, color: tab.accent }
                    : undefined
                }
              >
                {iconImg ? (
                  <img
                    src={iconImg}
                    alt=""
                    className={`w-3.5 h-3.5 rounded ${!connected ? "opacity-40" : ""}`}
                  />
                ) : null}
                <span>{tab.name}</span>
                {!connected && (
                  <div className="w-1.5 h-1.5 rounded-full bg-[#2a2b36]" />
                )}
              </button>
            );
          })}
        </div>

        {/* Provider card — scrollable */}
        <div className="flex-1 overflow-y-auto px-2 py-1.5">
          {activeProvider ? (
            <TrayProviderCard provider={activeProvider} />
          ) : (
            <div className="rounded-lg border border-[#2a2b36] bg-[#1a1b23]/60 p-3">
              <p className="text-[#9394a1] text-xs">
                No data available for this provider.
              </p>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="flex items-center gap-2 px-3 py-2.5 border-t border-[#2a2b36] bg-[#0e0f13]">
          <button
            onClick={handleOpenDashboard}
            className="flex-1 text-xs font-medium py-2 px-3 rounded-md transition-colors text-center"
            style={{
              backgroundColor: activeTabConfig
                ? `${activeTabConfig.accent}20`
                : "#2a2b36",
              color: activeTabConfig?.accent ?? "#e8e9ed",
            }}
          >
            Show Full Analytics
          </button>
          <div ref={settingsMenuRef} className="relative">
            {showSettingsMenu && (
              <div className="absolute bottom-full right-0 mb-2 w-56 rounded-md border border-[#2a2b36] bg-[#12131a] shadow-xl overflow-hidden z-30">
                <button
                  onClick={handleOpenSettings}
                  className="w-full text-left px-3 py-2 text-xs text-[#e8e9ed] hover:bg-[#1a1b23] transition-colors"
                >
                  Show AgentHarbor Settings
                </button>
                <button
                  onClick={handleQuitApp}
                  className="w-full text-left px-3 py-2 text-xs text-red-300 hover:bg-[#1a1b23] transition-colors border-t border-[#2a2b36]"
                >
                  Quit AgentHarbor
                </button>
              </div>
            )}
            <button
              onClick={() => setShowSettingsMenu((prev) => !prev)}
              className="p-1.5 rounded-md text-[#9394a1] hover:text-[#e8e9ed] hover:bg-[#1a1b23] transition-colors"
              title="Options"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <circle cx="12" cy="12" r="3" />
                <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
              </svg>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
