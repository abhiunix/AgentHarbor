/**
 * Lightweight analytics page for balance-only API providers (DeepSeek, Moonshot).
 * Reads the adapter id from the route, fetches that single provider's analytics,
 * and shows connection status + account balance, with a connect flow.
 */
import { useCallback, useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import type { ProviderAnalytics, ProviderInfo } from "../stores/analyticsStore";
import { ProviderConnectModal } from "../components/analytics/ProviderConnectModal";
import { getAdapterName, getAdapterIconImg } from "../lib/adapterPlugins";

function formatMoney(n: number, currency: string): string {
  if (currency === "USD") return `$${n.toFixed(2)}`;
  return `${n.toFixed(2)} ${currency}`;
}

export function ProviderBalancePage() {
  const { adapterId } = useParams<{ adapterId: string }>();
  const providerId = adapterId ?? "";
  const [info, setInfo] = useState<ProviderInfo | null>(null);
  const [analytics, setAnalytics] = useState<ProviderAnalytics | null>(null);
  const [loading, setLoading] = useState(true);
  const [showConnect, setShowConnect] = useState(false);

  const load = useCallback(async () => {
    if (!providerId) return;
    setLoading(true);
    try {
      const infos = await invoke<ProviderInfo[]>("get_all_provider_info");
      setInfo(infos.find((i) => i.id === providerId) ?? null);
      const a = await invoke<ProviderAnalytics>("get_provider_analytics", { providerId });
      setAnalytics(a);
    } catch (e) {
      console.error("[ProviderBalance] load error:", e);
    } finally {
      setLoading(false);
    }
  }, [providerId]);

  useEffect(() => { load(); }, [load]);

  const name = info?.name ?? getAdapterName(providerId);
  const iconImg = getAdapterIconImg(providerId);
  const status = analytics?.status;
  const connected = !!status?.connected;
  const credit = analytics?.credit_usage;
  const extra = (analytics?.extra ?? {}) as Record<string, unknown>;

  const extraMoney = (key: string): number | null => {
    const v = extra[key];
    return typeof v === "number" ? v : null;
  };

  return (
    <div className="h-full overflow-y-auto p-6">
      <div className="max-w-3xl mx-auto">
        {/* Header */}
        <div className="flex items-center justify-between mb-6">
          <div className="flex items-center gap-3">
            {iconImg && <img src={iconImg} alt="" className="w-6 h-6 object-contain" />}
            <div>
              <h1 className="text-lg font-semibold text-text-primary">{name} Analytics</h1>
              <div className="flex items-center gap-2 text-xs text-text-muted">
                <span className={`w-1.5 h-1.5 rounded-full ${connected ? "bg-emerald-500" : "bg-text-muted"}`} />
                {loading ? "Loading…" : connected ? "Connected" : "Not connected"}
              </div>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={load}
              className="px-3 py-1.5 text-xs rounded-lg bg-[#2a2b36] text-text-primary hover:bg-[#32333e]"
            >
              Refresh
            </button>
            <button
              onClick={() => setShowConnect(true)}
              className="px-3 py-1.5 text-xs rounded-lg bg-accent-blue text-white hover:bg-accent-blue/90"
            >
              {connected ? "Update key" : "Connect"}
            </button>
          </div>
        </div>

        {/* Not connected */}
        {!loading && !connected && (
          <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-xl p-6 text-center">
            <div className="text-3xl mb-3">🔑</div>
            <h2 className="text-sm font-semibold text-text-primary mb-1">Connect {name}</h2>
            <p className="text-xs text-text-muted max-w-sm mx-auto mb-4">
              {status?.error || `Add your ${name} API key to see your account balance. It is stored securely in your OS keychain.`}
            </p>
            <button
              onClick={() => setShowConnect(true)}
              className="px-4 py-2 text-xs rounded-lg bg-accent-blue text-white font-medium hover:bg-accent-blue/90"
            >
              Add API key
            </button>
          </div>
        )}

        {/* Connected: balance */}
        {!loading && connected && credit && (
          <>
            <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-xl p-6 mb-4">
              <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">
                {credit.plan_name || "Available balance"}
              </div>
              <div className="text-3xl font-semibold text-emerald-400">
                {formatMoney(credit.remaining, credit.currency)}
              </div>
              {status?.error && (
                <div className="text-xs text-amber-400 mt-2">{status.error}</div>
              )}
            </div>

            {/* Extra balance components */}
            <div className="grid grid-cols-2 md:grid-cols-3 gap-3">
              {(["voucher_balance", "cash_balance"] as const).map((k) => {
                const v = extraMoney(k);
                if (v == null) return null;
                return (
                  <div key={k} className="bg-[#1a1b23] border border-[#2a2b36] rounded-lg px-4 py-3">
                    <div className="text-[10px] text-text-muted uppercase tracking-wider mb-1">
                      {k.replace(/_/g, " ")}
                    </div>
                    <div className="text-lg font-semibold text-text-primary">
                      {formatMoney(v, credit.currency)}
                    </div>
                  </div>
                );
              })}
            </div>
          </>
        )}

        {/* Connected but no balance (e.g. transient API error) */}
        {!loading && connected && !credit && (
          <div className="bg-[#1a1b23] border border-[#2a2b36] rounded-xl p-6 text-center text-xs text-text-muted">
            {status?.error || "Connected, but no balance data was returned. Try Refresh."}
          </div>
        )}
      </div>

      {showConnect && (
        <ProviderConnectModal
          providerId={providerId}
          providerName={name}
          authType={info?.auth_type ?? "token"}
          onClose={() => { setShowConnect(false); load(); }}
        />
      )}
    </div>
  );
}
