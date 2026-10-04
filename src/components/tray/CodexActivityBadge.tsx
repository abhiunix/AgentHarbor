import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getCodexActivity, type CodexActivity } from "../../lib/tauri";

type ActivityState =
  | { kind: "loading" }
  | { kind: "available"; counts: CodexActivity }
  | { kind: "unavailable"; message: string };

export function CodexActivityBadge() {
  const [activity, setActivity] = useState<ActivityState>({ kind: "loading" });

  useEffect(() => {
    let disposed = false;
    let inFlight = false;
    let unlisten: (() => void) | undefined;

    const refresh = async () => {
      if (disposed || inFlight) return;
      inFlight = true;
      try {
        if (!(await getCurrentWindow().isVisible()) || disposed) return;
        const counts = await getCodexActivity();
        if (!disposed) setActivity({ kind: "available", counts });
      } catch (error) {
        // A missing service or failed read is unknown, never a zero count.
        if (!disposed) setActivity({ kind: "unavailable", message: String(error) });
      } finally {
        inFlight = false;
      }
    };

    void refresh();
    const timer = window.setInterval(() => { void refresh(); }, 3000);
    listen("tray-popover-opened", () => {
      if (disposed) return;
      setActivity({ kind: "loading" });
      void refresh();
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch(() => { /* The visibility-checked timer still refreshes activity. */ });

    return () => {
      disposed = true;
      window.clearInterval(timer);
      unlisten?.();
    };
  }, []);

  if (activity.kind !== "available") {
    return (
      <div
        role="status"
        className="mt-1 text-[10px] text-[#9394a1]"
        title={activity.kind === "unavailable" ? activity.message : undefined}
      >
        {activity.kind === "loading" ? "Checking live activity…" : "Live status unavailable"}
      </div>
    );
  }

  const { working_conversations, working_helpers, waiting, idle, errored } = activity.counts;
  const badge = "inline-flex items-center gap-1 rounded px-1.5 py-0.5 whitespace-nowrap";
  const workingStyle = "bg-emerald-500/20 text-emerald-400";
  const idleStyle = "bg-[#2a2b36] text-[#9394a1]";

  return (
    <div
      role="status"
      aria-live="polite"
      aria-label="Live Codex activity"
      className="mt-1 flex flex-wrap items-center gap-1 text-[10px]"
      title={`Activity from the local Codex service. ${idle} idle sessions are excluded.`}
    >
      <span
        className={`${badge} ${working_conversations > 0 ? workingStyle : idleStyle}`}
        title="Main conversations currently working"
      >
        {working_conversations > 0 && (
          <span className="h-1.5 w-1.5 rounded-full bg-emerald-400 animate-pulse" aria-hidden="true" />
        )}
        {working_conversations} working
      </span>
      <span
        className={`${badge} ${working_helpers > 0 ? workingStyle : idleStyle}`}
        title="Helper agents currently working on tasks started by another Codex agent"
      >
        {working_helpers} helper{working_helpers !== 1 ? "s" : ""}
      </span>
      <span
        className={`${badge} ${waiting > 0 ? "bg-amber-500/20 text-amber-400" : idleStyle}`}
        title="Conversations or helper agents waiting for your reply or approval"
      >
        {waiting} waiting
      </span>
      {errored > 0 && (
        <span className={`${badge} bg-red-500/20 text-red-400`}>
          {errored} need{errored === 1 ? "s" : ""} attention
        </span>
      )}
    </div>
  );
}
