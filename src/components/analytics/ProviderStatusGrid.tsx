/**
 * Grid showing connection status for all providers.
 */
import type { ProviderAnalytics } from "../../stores/analyticsStore";
import { getAdapterIconImg } from "../../lib/adapterPlugins";

function StatusDot({ connected, error }: { connected: boolean; error: string | null }) {
  if (connected && !error) return <span className="w-2 h-2 rounded-full bg-emerald-500 shrink-0" />;
  if (connected && error) return <span className="w-2 h-2 rounded-full bg-amber-500 shrink-0" />;
  return <span className="w-2 h-2 rounded-full bg-[#2a2b36] shrink-0" />;
}

export function ProviderStatusGrid({
  providers,
  onConnect,
}: {
  providers: ProviderAnalytics[];
  onConnect: (providerId: string) => void;
}) {
  const connected = providers.filter((p) => p.status.connected);
  const disconnected = providers.filter((p) => !p.status.connected);

  return (
    <div className="space-y-3">
      {/* Connected */}
      {connected.length > 0 && (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
          {connected.map((p) => {
            const icon = getAdapterIconImg(p.provider_id);
            return (
              <div
                key={p.provider_id}
                className="flex items-center gap-2.5 px-3 py-2 bg-[#1a1b23] rounded-lg border border-[#2a2b36]"
              >
                <StatusDot connected={p.status.connected} error={p.status.error} />
                {icon ? (
                  <img src={icon} alt="" className="w-4 h-4 object-contain shrink-0" />
                ) : (
                  <span className="w-4 h-4 text-xs text-text-muted shrink-0">{p.provider_name[0]}</span>
                )}
                <div className="flex-1 min-w-0">
                  <div className="text-xs font-medium text-text-primary truncate">
                    {p.provider_name}
                  </div>
                  <div className="text-[10px] text-text-muted truncate">
                    {p.status.plan_name && <span>{p.status.plan_name}</span>}
                    {p.status.plan_name && p.status.account_email && <span> · </span>}
                    {p.status.account_email && <span>{p.status.account_email}</span>}
                    {!p.status.plan_name && !p.status.account_email && (
                      <span className="capitalize">{p.status.connection_method.replace("-", " ")}</span>
                    )}
                  </div>
                </div>
                {p.status.error && (
                  <span className="text-[9px] text-amber-400 shrink-0" title={p.status.error}>!</span>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* Disconnected */}
      {disconnected.length > 0 && (
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
          {disconnected.map((p) => {
            const icon = getAdapterIconImg(p.provider_id);
            return (
              <div
                key={p.provider_id}
                className="flex items-center gap-2.5 px-3 py-2 bg-[#13141a] rounded-lg border border-[#1e1f2a] opacity-60"
              >
                <StatusDot connected={false} error={null} />
                {icon ? (
                  <img src={icon} alt="" className="w-4 h-4 object-contain shrink-0 grayscale" />
                ) : (
                  <span className="w-4 h-4 text-xs text-text-muted shrink-0">{p.provider_name[0]}</span>
                )}
                <span className="flex-1 text-xs text-text-muted truncate">{p.provider_name}</span>
                <button
                  onClick={() => onConnect(p.provider_id)}
                  className="text-[10px] text-accent-blue hover:text-accent-blue/80 shrink-0"
                >
                  Connect
                </button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
