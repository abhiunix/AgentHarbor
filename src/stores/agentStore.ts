import { create } from "zustand";
import type { AgentDefinition, Visibility, AgentFilters } from "../lib/types";
import { getAllAgents } from "../lib/tauri";

interface AgentState {
  agents: AgentDefinition[];
  loading: boolean;
  error: string | null;
  filters: AgentFilters;
  detailAgent: AgentDefinition | null;
  editorAgent: AgentDefinition | null;
  showEditor: boolean;
}

interface AgentActions {
  loadAgents: () => Promise<void>;
  setSearch: (search: string) => void;
  setVisibilityFilter: (visibility: Visibility | "all") => void;
  clearFilters: () => void;
  setDetailAgent: (agent: AgentDefinition | null) => void;
  openEditor: (agent?: AgentDefinition) => void;
  closeEditor: () => void;
}

const initialFilters: AgentFilters = {
  search: "",
  visibility: "all",
};

export const useAgentStore = create<AgentState & AgentActions>((set) => ({
  agents: [],
  loading: false,
  error: null,
  filters: initialFilters,
  detailAgent: null,
  editorAgent: null,
  showEditor: false,

  loadAgents: async () => {
    set({ loading: true, error: null });
    try {
      const agents = await getAllAgents();
      set({ agents, loading: false });
    } catch (error) {
      set({
        error: error instanceof Error ? error.message : "Failed to load agents",
        loading: false,
      });
    }
  },

  setSearch: (search) => {
    set((state) => ({
      filters: { ...state.filters, search },
    }));
  },

  setVisibilityFilter: (visibility) => {
    set((state) => ({
      filters: { ...state.filters, visibility },
    }));
  },

  clearFilters: () => {
    set({ filters: initialFilters });
  },

  setDetailAgent: (agent) => {
    set({ detailAgent: agent });
  },

  openEditor: (agent) => {
    set({ editorAgent: agent || null, showEditor: true, detailAgent: null });
  },

  closeEditor: () => {
    set({ editorAgent: null, showEditor: false });
  },
}));

export function useFilteredAgents(): AgentDefinition[] {
  const { agents, filters } = useAgentStore();

  return agents.filter((agent) => {
    if (filters.visibility !== "all" && agent.visibility !== filters.visibility) {
      return false;
    }

    if (filters.search) {
      const searchLower = filters.search.toLowerCase();
      const matchesName = agent.name.toLowerCase().includes(searchLower);
      const matchesId = agent.id.toLowerCase().includes(searchLower);
      const matchesDesc = agent.description.toLowerCase().includes(searchLower);
      const matchesTags = agent.tags.some((tag) =>
        tag.toLowerCase().includes(searchLower)
      );
      if (!matchesName && !matchesId && !matchesDesc && !matchesTags) {
        return false;
      }
    }

    return true;
  });
}

export function useAgentCounts(): { total: number; public: number; private: number } {
  const { agents } = useAgentStore();

  return {
    total: agents.length,
    public: agents.filter((a) => a.visibility === "public").length,
    private: agents.filter((a) => a.visibility === "private").length,
  };
}
