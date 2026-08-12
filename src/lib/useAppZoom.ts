import { useEffect } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";

const STORAGE_KEY = "app-zoom-factor";
const MIN = 0.5;
const MAX = 2.0;
const STEP = 0.1;

const isMac = typeof navigator !== "undefined" && /mac/i.test(navigator.platform);

function clamp(z: number): number {
  return Math.min(MAX, Math.max(MIN, Math.round(z * 10) / 10));
}

function loadZoom(): number {
  try {
    const v = parseFloat(localStorage.getItem(STORAGE_KEY) ?? "1");
    return Number.isFinite(v) ? clamp(v) : 1;
  } catch {
    return 1;
  }
}

async function applyZoom(z: number) {
  try {
    await getCurrentWebview().setZoom(z);
  } catch {
    /* not in a Tauri webview */
  }
}

/**
 * Cmd +/-/0 on macOS, Ctrl +/-/0 on Windows/Linux, to zoom the whole app.
 * The level persists across restarts via localStorage.
 */
export function useAppZoom() {
  useEffect(() => {
    let zoom = loadZoom();
    applyZoom(zoom);

    const setZoom = (next: number) => {
      zoom = clamp(next);
      try { localStorage.setItem(STORAGE_KEY, String(zoom)); } catch { /* ignore */ }
      applyZoom(zoom);
    };

    const onKeyDown = (e: KeyboardEvent) => {
      const mod = isMac ? e.metaKey : e.ctrlKey;
      if (!mod) return;
      // Support both the main row keys and the numpad.
      if (e.key === "+" || e.key === "=" || e.code === "NumpadAdd") {
        e.preventDefault();
        setZoom(zoom + STEP);
      } else if (e.key === "-" || e.key === "_" || e.code === "NumpadSubtract") {
        e.preventDefault();
        setZoom(zoom - STEP);
      } else if (e.key === "0" || e.code === "Numpad0") {
        e.preventDefault();
        setZoom(1);
      }
    };

    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);
}
