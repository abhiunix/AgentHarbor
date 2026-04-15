/**
 * A single rate-limit progress bar with label, percentage, and reset timer.
 */
import type { RateLimitWindow } from "../../stores/analyticsStore";
import { getAdapterIconImg } from "../../lib/adapterPlugins";

function formatResetTime(resetsAt: string | null, resetsInSeconds: number | null): string {
  let seconds = resetsInSeconds;
  if (!seconds && resetsAt) {
    const diff = new Date(resetsAt).getTime() - Date.now();
    seconds = Math.max(0, Math.floor(diff / 1000));
  }
  if (!seconds || seconds <= 0) return "";
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (d > 0) return `${d}d ${h}h`;
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

function barColor(remaining: number): string {
  if (remaining > 60) return "bg-emerald-500";
  if (remaining > 30) return "bg-amber-500";
  return "bg-red-500";
}

export function RateLimitBar({ window }: { window: RateLimitWindow }) {
  const remaining = Math.round(window.remaining_percent);
  const resetStr = formatResetTime(window.resets_at, window.resets_in_seconds);
  const icon = getAdapterIconImg(window.provider_id);

  return (
    <div className="flex items-center gap-3 py-1.5">
      {/* Provider icon */}
      <div className="w-5 h-5 shrink-0 flex items-center justify-center">
        {icon ? (
          <img src={icon} alt="" className="w-4 h-4 object-contain" />
        ) : (
          <span className="text-xs text-text-muted">?</span>
        )}
      </div>

      {/* Label */}
      <span className="text-xs text-text-secondary w-28 shrink-0 truncate" title={window.label}>
        {window.label}
      </span>

      {/* Bar */}
      <div className="flex-1 h-2 bg-[#1a1b23] rounded-full overflow-hidden">
        <div
          className={`h-full rounded-full transition-all duration-500 ${barColor(remaining)}`}
          style={{ width: `${remaining}%` }}
        />
      </div>

      {/* Percent */}
      <span className={`text-xs font-mono w-12 text-right ${remaining < 20 ? "text-red-400" : "text-text-secondary"}`}>
        {remaining}%
      </span>

      {/* Reset timer */}
      <span className="text-[10px] text-text-muted w-14 text-right">
        {resetStr && `${resetStr}`}
      </span>
    </div>
  );
}
