import { create } from "zustand";
import type { Preset } from "../lib/types";
import {
  getPresets,
  savePreset as savePresetApi,
  deletePreset as deletePresetApi,
  addCapabilityToPreset as addCapabilityApi,
  removeCapabilityFromPreset as removeCapabilityApi,
} from "../lib/tauri";

interface PresetState {
  presets: Preset[];
  loading: boolean;
  error: string | null;
}

interface PresetActions {
  loadPresets: () => Promise<void>;
  savePreset: (preset: Preset) => Promise<void>;
  deletePreset: (id: string) => Promise<void>;
  addCapabilityToPreset: (presetId: string, capabilityId: string) => Promise<void>;
  removeCapabilityFromPreset: (presetId: string, capabilityId: string) => Promise<void>;
  createPreset: (name: string, description: string, capabilityIds: string[]) => Promise<void>;
}

export const usePresetStore = create<PresetState & PresetActions>((set, get) => ({
  presets: [],
  loading: false,
  error: null,

  loadPresets: async () => {
    set({ loading: true, error: null });
    try {
      const presets = await getPresets();
      set({ presets, loading: false });
    } catch (error) {
      set({
        error: error instanceof Error ? error.message : "Failed to load presets",
        loading: false,
      });
    }
  },

  savePreset: async (preset: Preset) => {
    set({ loading: true, error: null });
    try {
      const saved = await savePresetApi(preset);
      const existingIndex = get().presets.findIndex((p) => p.id === preset.id);
      if (existingIndex >= 0) {
        set((state) => ({
          presets: state.presets.map((p) => (p.id === preset.id ? saved : p)),
          loading: false,
        }));
      } else {
        set((state) => ({
          presets: [...state.presets, saved],
          loading: false,
        }));
      }
    } catch (error) {
      set({
        error: error instanceof Error ? error.message : "Failed to save preset",
        loading: false,
      });
    }
  },

  deletePreset: async (id: string) => {
    set({ loading: true, error: null });
    try {
      await deletePresetApi(id);
      set((state) => ({
        presets: state.presets.filter((p) => p.id !== id),
        loading: false,
      }));
    } catch (error) {
      set({
        error: error instanceof Error ? error.message : "Failed to delete preset",
        loading: false,
      });
    }
  },

  addCapabilityToPreset: async (presetId: string, capabilityId: string) => {
    try {
      const updated = await addCapabilityApi(presetId, capabilityId);
      set((state) => ({
        presets: state.presets.map((p) => (p.id === presetId ? updated : p)),
      }));
    } catch (error) {
      console.error("Failed to add capability:", error);
    }
  },

  removeCapabilityFromPreset: async (presetId: string, capabilityId: string) => {
    try {
      const updated = await removeCapabilityApi(presetId, capabilityId);
      set((state) => ({
        presets: state.presets.map((p) => (p.id === presetId ? updated : p)),
      }));
    } catch (error) {
      console.error("Failed to remove capability:", error);
    }
  },

  createPreset: async (name: string, description: string, capabilityIds: string[]) => {
    const id = `user/${name.toLowerCase().replace(/\s+/g, "-")}`;
    const preset: Preset = {
      id,
      name,
      description,
      capability_ids: capabilityIds,
      tags: [],
      is_bundled: false,
    };
    await get().savePreset(preset);
  },
}));

export function usePresetById(id: string): Preset | undefined {
  const { presets } = usePresetStore();
  return presets.find((p) => p.id === id);
}
