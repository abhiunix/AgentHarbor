import { create } from "zustand";
import {
  selectProjectFolder,
  detectAdapters,
  getRecentProjects,
  addRecentProject,
  removeRecentProject,
  RecentProject,
  DetectedAdapter,
} from "../lib/tauri";
import { basename } from "../lib/platform";

interface ProjectState {
  selectedProject: string | null;
  detectedAdapters: DetectedAdapter[];
  recentProjects: RecentProject[];
  loading: boolean;
  error: string | null;
}

interface ProjectActions {
  selectProject: () => Promise<void>;
  setSelectedProject: (path: string | null) => Promise<void>;
  loadRecentProjects: () => Promise<void>;
  removeProject: (path: string) => Promise<void>;
  clearSelection: () => void;
}

export const useProjectStore = create<ProjectState & ProjectActions>(
  (set) => ({
    selectedProject: null,
    detectedAdapters: [],
    recentProjects: [],
    loading: false,
    error: null,

    selectProject: async () => {
      set({ loading: true, error: null });
      try {
        const path = await selectProjectFolder();
        if (path) {
          const adapters = await detectAdapters(path);
          await addRecentProject(path);
          const projects = await getRecentProjects();
          set({
            selectedProject: path,
            detectedAdapters: adapters,
            recentProjects: projects,
            loading: false,
          });
        } else {
          set({ loading: false });
        }
      } catch (error) {
        set({
          error: error instanceof Error ? error.message : "Failed to select project",
          loading: false,
        });
      }
    },

    setSelectedProject: async (path) => {
      if (!path) {
        set({ selectedProject: null, detectedAdapters: [] });
        return;
      }

      set({ loading: true, error: null });
      try {
        const adapters = await detectAdapters(path);
        await addRecentProject(path);
        const projects = await getRecentProjects();
        set({
          selectedProject: path,
          detectedAdapters: adapters,
          recentProjects: projects,
          loading: false,
        });
      } catch (error) {
        set({
          error: error instanceof Error ? error.message : "Failed to set project",
          loading: false,
        });
      }
    },

    loadRecentProjects: async () => {
      set({ loading: true, error: null });
      try {
        const projects = await getRecentProjects();
        set({ recentProjects: projects, loading: false });
      } catch (error) {
        set({
          error: error instanceof Error ? error.message : "Failed to load projects",
          loading: false,
        });
      }
    },

    removeProject: async (path) => {
      try {
        const projects = await removeRecentProject(path);
        set({ recentProjects: projects });
      } catch (error) {
        console.error("Failed to remove project:", error);
      }
    },

    clearSelection: () => {
      set({ selectedProject: null, detectedAdapters: [] });
    },
  })
);

export function getProjectName(path: string): string {
  return basename(path) || "Unknown";
}
