import { create } from "zustand";
import { check } from "@tauri-apps/plugin-updater";

const SNOOZE_HOURS = 24;
const CHECK_INTERVAL_MS = 4 * 60 * 60 * 1000; // 4 hours

interface UpdateState {
  latestVersion: string | null;
  isAvailable: boolean;
  notes: string;
  lastChecked: Date | null;
  isChecking: boolean;
  snoozedUntil: Date | null;
  checkError: string | null;

  checkForUpdate: () => Promise<void>;
  snooze: () => void;
  clearSnooze: () => void;
  shouldShowBanner: () => boolean;
}

export const useUpdateStore = create<UpdateState>((set, get) => ({
  latestVersion: null,
  isAvailable: false,
  notes: "",
  lastChecked: null,
  isChecking: false,
  snoozedUntil: null,
  checkError: null,

  checkForUpdate: async () => {
    const { isChecking } = get();
    if (isChecking) return;

    set({ isChecking: true, checkError: null });
    try {
      const update = await check();
      set({
        isAvailable: update?.available ?? false,
        latestVersion: update?.available ? (update.version ?? null) : null,
        notes: update?.available ? (update.body ?? "") : "",
        lastChecked: new Date(),
      });
    } catch {
      set({ checkError: "Could not check for updates", lastChecked: new Date() });
    } finally {
      set({ isChecking: false });
    }
  },

  snooze: () => {
    const until = new Date();
    until.setHours(until.getHours() + SNOOZE_HOURS);
    set({ snoozedUntil: until });
  },

  clearSnooze: () => set({ snoozedUntil: null }),

  shouldShowBanner: () => {
    const { isAvailable, snoozedUntil } = get();
    if (!isAvailable) return false;
    if (!snoozedUntil) return true;
    return new Date() > snoozedUntil;
  },
}));

let _intervalId: ReturnType<typeof setInterval> | null = null;

export function startUpdatePolling() {
  if (_intervalId) return;
  const { checkForUpdate } = useUpdateStore.getState();
  checkForUpdate();
  _intervalId = setInterval(checkForUpdate, CHECK_INTERVAL_MS);
}

export function stopUpdatePolling() {
  if (_intervalId) {
    clearInterval(_intervalId);
    _intervalId = null;
  }
}
