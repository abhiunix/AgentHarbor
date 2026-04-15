import { create } from "zustand";
import {
  getSettings,
  updateSettings as updateSettingsApi,
  getUsername,
  type AppSettings,
} from "../lib/tauri";

interface SettingsState {
  settings: AppSettings | null;
  loading: boolean;
  error: string | null;
}

interface SettingsActions {
  loadSettings: () => Promise<void>;
  updateSettings: (settings: AppSettings) => Promise<void>;
  getUsername: () => string;
}

export const useSettingsStore = create<SettingsState & SettingsActions>(
  (set, get) => ({
    settings: null,
    loading: false,
    error: null,

    loadSettings: async () => {
      set({ loading: true, error: null });
      try {
        const settings = await getSettings();
        set({ settings, loading: false });
      } catch (error) {
        set({
          error: error instanceof Error ? error.message : "Failed to load settings",
          loading: false,
        });
      }
    },

    updateSettings: async (settings) => {
      try {
        const updated = await updateSettingsApi(settings);
        set({ settings: updated });
      } catch (error) {
        console.error("Failed to update settings:", error);
      }
    },

    getUsername: () => {
      const state = get();
      return state.settings?.general.username || "user";
    },
  })
);

export async function fetchUsername(): Promise<string> {
  try {
    return await getUsername();
  } catch {
    return "user";
  }
}
