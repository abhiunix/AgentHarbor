import { useSyncExternalStore, useCallback } from "react";

const STORAGE_KEY = "agentharbor-debug-mode";

let listeners: Array<() => void> = [];

function emitChange() {
  for (const listener of listeners) {
    listener();
  }
}

function subscribe(listener: () => void): () => void {
  listeners.push(listener);
  return () => {
    listeners = listeners.filter((l) => l !== listener);
  };
}

function getSnapshot(): boolean {
  try {
    return localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

/**
 * Hook that returns the current debug mode state and a toggle function.
 * When debug mode is enabled, pages show the file paths from which data is read.
 * Uses useSyncExternalStore so all components re-render when the value changes.
 */
export function useDebugMode(): [boolean, (value: boolean) => void] {
  const enabled = useSyncExternalStore(subscribe, getSnapshot, () => false);

  const setEnabled = useCallback((value: boolean) => {
    try {
      localStorage.setItem(STORAGE_KEY, value ? "1" : "0");
    } catch { /* noop */ }
    emitChange();
  }, []);

  return [enabled, setEnabled];
}

/**
 * Read-only check for debug mode (no toggle). Lighter weight for display-only pages.
 */
export function useIsDebugMode(): boolean {
  return useSyncExternalStore(subscribe, getSnapshot, () => false);
}
