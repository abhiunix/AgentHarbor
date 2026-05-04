/**
 * Shared limit / billing state banner (tray popover + Claude analytics).
 * Matches Rust `analytics::types::LimitState` (serde tag = "kind", snake_case).
 */
import { openUrl } from "@tauri-apps/plugin-opener";

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

const BILLING_URL = "https://console.anthropic.com/settings/billing";

/** Friendly title + body for known `api_disabled_reason` codes. */
function describeApiDisabled(
  reason: string,
  orgName: string
): { title: string; body: string } {
  const norm = reason.trim().toLowerCase();
  const org = orgName.trim() || "Your organization";
  switch (norm) {
    case "out_of_credits":
      return {
        title: `${org} has reached its monthly usage limit`,
        body: "Top up credits or ask an admin for /extra-usage to keep going.",
      };
    case "trial_expired":
      return {
        title: `${org}'s Claude Code trial has ended`,
        body: "Add a payment method to keep using Claude Code.",
      };
    case "payment_failed":
    case "payment_required":
      return {
        title: `${org} — payment couldn't be processed`,
        body: "Update your card to resume API access.",
      };
    case "usage_policy_violation":
      return {
        title: `${org}'s API access is paused for review`,
        body: "Anthropic flagged recent usage. Contact support to restore access.",
      };
    case "manual_disable":
    case "admin_disabled":
      return {
        title: `${org} — API access turned off by an admin`,
        body: "Ask an admin in your organization to re-enable Claude Code access.",
      };
    case "subscription_canceled":
    case "subscription_expired":
      return {
        title: `${org}'s Claude subscription is inactive`,
        body: "Re-activate billing in the Anthropic console to restore access.",
      };
    default: {
      const friendly = reason
        .replace(/_/g, " ")
        .replace(/\b\w/g, (c) => c.toUpperCase());
      return {
        title: `${org} — API access paused`,
        body: friendly || "Open Anthropic billing for details.",
      };
    }
  }
}

function formatCountdown(iso: string | null | undefined): string {
  if (!iso) return "soon";
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return "soon";
  const diff = t - Date.now();
  if (diff <= 0) return "now";
  const s = Math.floor(diff / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (h > 0) return `${h}h ${m}m`;
  return `${m}m`;
}

/** Human-readable form of a "retry after N seconds" duration. */
function formatRetryAfter(seconds: number): string {
  if (seconds <= 60) return `${Math.max(1, Math.round(seconds))}s`;
  if (seconds < 3600) {
    const m = Math.round(seconds / 60);
    return `${m}m`;
  }
  const h = Math.floor(seconds / 3600);
  const m = Math.round((seconds % 3600) / 60);
  return m > 0 ? `${h}h ${m}m` : `${h}h`;
}

function formatAbsolute(iso: string | null | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleString("en-US", {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
    hour12: true,
  });
}

export function limitStateIsDanger(ls: LimitState | null | undefined): boolean {
  if (!ls || ls.kind === "healthy") return false;
  return (
    ls.kind === "reached" ||
    ls.kind === "api_disabled" ||
    ls.kind === "subscription_issue" ||
    ls.kind === "billable_paused" ||
    ls.kind === "rate_limited" ||
    ls.kind === "unauthenticated"
  );
}

export function limitStateIsWarning(ls: LimitState | null | undefined): boolean {
  if (!ls || ls.kind === "healthy") return false;
  if (limitStateIsDanger(ls)) return false;
  return ls.kind === "approaching";
}

interface LimitStateBannerProps {
  limitState: LimitState | null | undefined;
  /** "compact" = tray pill; "full" = analytics callout */
  variant?: "compact" | "full";
  className?: string;
  /**
   * Optional handler for the "Reconnect" button on `unauthenticated`. When
   * absent, the button is hidden — useful in places that don't have a sign-in
   * flow on screen (e.g. tray popover).
   */
  onReconnect?: () => void;
}

export function LimitStateBanner({
  limitState,
  variant = "compact",
  className = "",
  onReconnect,
}: LimitStateBannerProps) {
  if (!limitState || limitState.kind === "healthy") return null;

  const isFull = variant === "full";

  switch (limitState.kind) {
    case "approaching": {
      const pill = (
        <div
          className={`flex items-center gap-2 rounded-md border border-amber-500/40 bg-amber-500/10 px-2.5 py-1.5 text-[11px] text-amber-200 ${className}`}
        >
          <span className="h-1.5 w-1.5 rounded-full bg-amber-400 shrink-0" />
          <span>
            <span className="font-medium">{limitState.label}</span>
            {" · "}
            {limitState.worst_pct.toFixed(0)}% — resets in{" "}
            {formatCountdown(limitState.resets_at)}
          </span>
        </div>
      );
      if (!isFull) return pill;
      return (
        <div className={`mb-4 ${className}`}>
          {pill}
          {limitState.resets_at && (
            <p className="text-[10px] text-text-muted mt-1.5">
              Resets at {formatAbsolute(limitState.resets_at)}
            </p>
          )}
        </div>
      );
    }
    case "reached": {
      const pill = (
        <div
          className={`flex items-center gap-2 rounded-md border border-red-500/50 bg-red-500/15 px-2.5 py-1.5 text-[11px] text-red-200 ${className}`}
        >
          <span className="relative flex h-2 w-2 shrink-0">
            <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-red-400 opacity-40" />
            <span className="relative inline-flex h-2 w-2 rounded-full bg-red-500" />
          </span>
          <span>
            Limit reached ({limitState.used_pct.toFixed(0)}%)
            {limitState.resets_at
              ? ` — resets in ${formatCountdown(limitState.resets_at)}`
              : ""}
          </span>
        </div>
      );
      if (!isFull) return pill;
      return (
        <div className={`mb-4 ${className}`}>
          {pill}
          {limitState.resets_at && (
            <p className="text-[10px] text-text-muted mt-1.5">
              Resets at {formatAbsolute(limitState.resets_at)}
            </p>
          )}
        </div>
      );
    }
    case "api_disabled": {
      const { title, body } = describeApiDisabled(
        limitState.reason,
        limitState.org_name
      );
      const block = (
        <div
          className={`rounded-md border border-red-500/40 bg-red-950/40 px-3 py-2.5 text-[11px] text-red-100 ${className}`}
        >
          <div className="font-medium">{title}</div>
          <div className="text-red-200/90 mt-0.5">{body}</div>
          <button
            type="button"
            onClick={() => openUrl(BILLING_URL)}
            className="mt-2 text-[11px] text-blue-400 hover:text-blue-300 underline"
          >
            Manage billing
          </button>
        </div>
      );
      return isFull ? <div className="mb-4">{block}</div> : block;
    }
    case "subscription_issue": {
      const status = limitState.status.replace(/_/g, " ");
      const block = (
        <div
          className={`rounded-md border border-red-500/40 bg-red-950/30 px-3 py-2 text-[11px] text-red-100 ${className}`}
        >
          {limitState.org_name}'s subscription is {status} — update billing to
          continue.
        </div>
      );
      return isFull ? <div className="mb-4">{block}</div> : block;
    }
    case "billable_paused": {
      const until = formatAbsolute(limitState.until) || limitState.until;
      const block = (
        <div
          className={`rounded-md border border-amber-500/40 bg-amber-950/30 px-3 py-2 text-[11px] text-amber-100 ${className}`}
        >
          {limitState.org_name} — billing paused until {until}.
        </div>
      );
      return isFull ? <div className="mb-4">{block}</div> : block;
    }
    case "rate_limited": {
      const retry =
        limitState.retry_after_secs != null
          ? ` Retry in ${formatRetryAfter(limitState.retry_after_secs)}.`
          : "";
      const block = (
        <div
          className={`rounded-md border border-orange-500/40 bg-orange-950/30 px-3 py-2 text-[11px] text-orange-100 ${className}`}
        >
          Slow down — Anthropic is rate-limiting requests right now.{retry}
        </div>
      );
      return isFull ? <div className="mb-4">{block}</div> : block;
    }
    case "unauthenticated": {
      const block = (
        <div
          className={`rounded-md border border-red-500/40 bg-red-950/40 px-3 py-2.5 text-[11px] text-red-100 ${className}`}
        >
          <div className="font-medium">Claude Code needs to reconnect</div>
          <div className="text-red-200/90 mt-0.5">
            Stored credentials are no longer valid. Sign in again to keep
            tracking usage.
          </div>
          {onReconnect && (
            <button
              type="button"
              onClick={onReconnect}
              className="mt-2 text-[11px] text-blue-400 hover:text-blue-300 underline"
            >
              Reconnect Claude Code
            </button>
          )}
        </div>
      );
      return isFull ? <div className="mb-4">{block}</div> : block;
    }
  }
}
