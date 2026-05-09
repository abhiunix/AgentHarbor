import { useState } from "react";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { useUpdateStore } from "../../stores/updateStore";

export function UpdateBanner() {
  const { latestVersion, notes, snooze, clearSnooze, shouldShowBanner } = useUpdateStore();
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);

  if (!shouldShowBanner()) return null;

  const handleInstall = async () => {
    setInstalling(true);
    setError(null);
    try {
      const update = await check();
      if (!update?.available) {
        setInstalling(false);
        return;
      }

      let downloaded = 0;
      let total = 0;

      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          if (total > 0) setProgress(Math.round((downloaded / total) * 100));
        }
      });

      await relaunch();
    } catch (e) {
      console.error("[updater] download/install failed:", e);
      setError(`Update failed: ${e}`);
      setInstalling(false);
    }
  };

  return (
    <div className="shrink-0 bg-accent-blue/10 border-b border-accent-blue/30 px-4 py-2 flex items-center justify-between gap-3">
      <div className="flex items-center gap-2 text-sm">
        <span className="w-1.5 h-1.5 rounded-full bg-accent-blue shrink-0" />
        <span className="text-text-primary font-medium">
          v{latestVersion} available
        </span>
        {notes && (
          <span className="text-text-muted truncate max-w-xs hidden sm:inline">
            — {notes.split("\n")[0]}
          </span>
        )}
      </div>
      <div className="flex items-center gap-2 shrink-0">
        {error && <span className="text-xs text-accent-red">{error}</span>}
        {installing ? (
          <span className="text-xs text-text-secondary">
            {progress !== null ? `Downloading… ${progress}%` : "Preparing…"}
          </span>
        ) : (
          <>
            <button
              onClick={() => { clearSnooze(); handleInstall(); }}
              className="px-3 py-1 rounded bg-accent-blue text-white text-xs font-medium hover:bg-accent-blue/90"
            >
              Update Now
            </button>
            <button
              onClick={snooze}
              className="px-2 py-1 text-xs text-text-muted hover:text-text-secondary"
            >
              Later
            </button>
          </>
        )}
      </div>
    </div>
  );
}
