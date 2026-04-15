import { useState, useEffect } from "react";
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export function UpdateBanner() {
  const [updateInfo, setUpdateInfo] = useState<{
    version: string;
    body: string;
  } | null>(null);
  const [updating, setUpdating] = useState(false);
  const [dismissed, setDismissed] = useState(false);
  const [progress, setProgress] = useState<number | null>(null);

  useEffect(() => {
    checkForUpdate();
  }, []);

  const checkForUpdate = async () => {
    try {
      const update = await check();
      if (update?.available) {
        setUpdateInfo({
          version: update.version,
          body: update.body || "",
        });
      }
    } catch (error) {
      console.error("Failed to check for updates:", error);
    }
  };

  const handleUpdate = async () => {
    setUpdating(true);
    try {
      const update = await check();
      if (update?.available) {
        let downloaded = 0;
        let contentLength = 0;
        
        await update.downloadAndInstall((event) => {
          if (event.event === "Started") {
            contentLength = event.data.contentLength || 0;
          } else if (event.event === "Progress") {
            downloaded += event.data.chunkLength;
            if (contentLength > 0) {
              setProgress(Math.round((downloaded / contentLength) * 100));
            }
          }
        });
        
        await relaunch();
      }
    } catch (error) {
      console.error("Failed to update:", error);
      setUpdating(false);
    }
  };

  if (!updateInfo || dismissed) {
    return null;
  }

  return (
    <div className="fixed top-0 left-0 right-0 z-50 bg-accent-blue text-white px-4 py-2">
      <div className="max-w-screen-xl mx-auto flex items-center justify-between">
        <div className="flex items-center gap-3">
          <span className="text-lg">✨</span>
          <span className="text-sm">
            AgentHarbor v{updateInfo.version} is available
          </span>
        </div>
        <div className="flex items-center gap-2">
          {updating ? (
            <span className="text-sm">
              {progress !== null ? `Downloading... ${progress}%` : "Preparing..."}
            </span>
          ) : (
            <>
              <button
                onClick={handleUpdate}
                className="px-3 py-1 text-sm bg-white text-accent-blue rounded hover:bg-white/90 font-medium"
              >
                Update Now
              </button>
              <button
                onClick={() => setDismissed(true)}
                className="px-3 py-1 text-sm text-white/80 hover:text-white"
              >
                Later
              </button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
