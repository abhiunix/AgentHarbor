import { create } from "zustand";
import type {
  UniversalCapability,
  CapabilityType,
  Visibility,
  AdapterType,
  RegistryFilters,
} from "../lib/types";
import { getAllCapabilities, discoverCapabilities, discoverSkills, discoverPlugins } from "../lib/tauri";
import type { McpServer, Skill, Plugin } from "../lib/types";

function slugify(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

/** Stable short hash so same-named skills in different directories keep distinct ids. */
function sourceHash(source: string): string {
  let h = 2166136261;
  for (let i = 0; i < source.length; i++) {
    h ^= source.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return (h >>> 0).toString(16).padStart(8, "0");
}

function discoveredId(name: string, source: string): string {
  return `discovered/${slugify(name)}-${sourceHash(source)}`;
}

const NEW_ITEM_DAYS = 7;
const SEEN_ITEMS_KEY = "agentharbor_seen_capabilities";

function getSeenItems(): Record<string, number> {
  try {
    const stored = localStorage.getItem(SEEN_ITEMS_KEY);
    return stored ? JSON.parse(stored) : {};
  } catch {
    return {};
  }
}

function saveSeenItems(items: Record<string, number>) {
  try {
    localStorage.setItem(SEEN_ITEMS_KEY, JSON.stringify(items));
  } catch {
    // ignore storage errors
  }
}

function isItemNew(id: string, seenItems: Record<string, number>): boolean {
  const firstSeen = seenItems[id];
  if (!firstSeen) return true;
  
  const daysSinceSeen = (Date.now() - firstSeen) / (1000 * 60 * 60 * 24);
  return daysSinceSeen < NEW_ITEM_DAYS;
}

interface RegistryState {
  capabilities: UniversalCapability[];
  loading: boolean;
  error: string | null;
  filters: RegistryFilters;
  selectedIds: Set<string>;
  detailCapability: UniversalCapability | null;
  newItemIds: Set<string>;
  editorOpen: boolean;
  editingCapability: UniversalCapability | null;
  deployWizardOpen: boolean;
  deployCapabilityIds: string[];
}

interface RegistryActions {
  loadCapabilities: () => Promise<void>;
  setTypeFilter: (type: CapabilityType | "all") => void;
  setVisibilityFilter: (visibility: Visibility | "all") => void;
  setAdapterFilter: (adapter: AdapterType | "all") => void;
  setSearch: (search: string) => void;
  setCategoryFilter: (category: string | "all") => void;
  setSortFilter: (sort: import("../lib/types").RegistrySort) => void;
  clearFilters: () => void;
  toggleSelection: (id: string) => void;
  selectAll: (ids: string[]) => void;
  clearSelection: () => void;
  setDetailCapability: (capability: UniversalCapability | null) => void;
  markAsSeen: (id: string) => void;
  openEditor: (capability?: UniversalCapability) => void;
  closeEditor: () => void;
  openDeployWizard: (capabilityIds?: string[]) => void;
  closeDeployWizard: () => void;
}

const initialFilters: RegistryFilters = {
  type: "all",
  visibility: "all",
  adapter: "all",
  search: "",
  category: "all",
  sort: "name",
};

export const useRegistryStore = create<RegistryState & RegistryActions>(
  (set) => ({
    capabilities: [],
    loading: false,
    error: null,
    filters: initialFilters,
    selectedIds: new Set<string>(),
    detailCapability: null,
    newItemIds: new Set<string>(),
    editorOpen: false,
    editingCapability: null,
    deployWizardOpen: false,
    deployCapabilityIds: [],

    loadCapabilities: async () => {
      set({ loading: true, error: null });
      try {
        const [capabilities, discovered, discoveredSkills, discoveredPlugins] = await Promise.all([
          getAllCapabilities(),
          discoverCapabilities().catch(() => []),
          discoverSkills().catch(() => []),
          discoverPlugins().catch(() => []),
        ]);
        const discoveredAsCapabilities: McpServer[] = discovered.map((d) => {
          const slug = d.name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") +
            "-" + d.source.toLowerCase().replace(/[^a-z0-9]+/g, "-").slice(0, 20);
          const id = `discovered/${slug}` as const;
          const env: Record<string, { type: string; label: string; required: boolean }> = {};
          if (d.env && typeof d.env === "object") {
            for (const [k] of Object.entries(d.env)) {
              env[k] = {
                type: "string",
                label: k,
                required: false,
              };
            }
          }
          return {
            type: "mcp",
            id,
            name: d.name,
            description: d.description,
            version: "1.0.0",
            author: "discovered",
            visibility: "discovered" as const,
            tags: [],
            compatible_agents: d.adapter_ids && d.adapter_ids.length > 0 ? d.adapter_ids : [d.adapter_id],
            transport: d.transport || "stdio",
            command: d.command,
            args: d.args || [],
            url: d.url || "",
            env,
            source: d.source,
          };
        });
        const discoveredSkillCapabilities: Skill[] = discoveredSkills.map((s) => ({
          type: "skill",
          id: discoveredId(s.name, s.source),
          name: s.name,
          description: s.description,
          version: "1.0.0",
          author: "discovered",
          visibility: "discovered" as const,
          tags: s.tags,
          compatible_agents: s.adapter_ids,
          managed: s.managed,
          files: [{ path: `${s.source}/SKILL.md`, content: "" }],
          source: s.source,
        }));

        const discoveredPluginCapabilities: Plugin[] = discoveredPlugins.map((p) => ({
          type: "plugin",
          id: discoveredId(`${p.name}-${p.marketplace}`, p.source),
          name: p.name,
          description: p.description,
          version: p.version || "1.0.0",
          author: p.author || "discovered",
          visibility: "discovered" as const,
          tags: [],
          compatible_agents: ["claude-code"],
          install_command: `/plugin install ${p.name}@${p.marketplace}`,
          config: {
            marketplace: p.marketplace,
            enabled: p.enabled,
            scope: p.scope,
            skill_count: p.skill_count,
            install_path: p.source,
          },
          source: p.source,
          source_info: p.homepage ? { url: p.homepage } : undefined,
        }));

        const merged = [
          ...capabilities,
          ...discoveredAsCapabilities,
          ...discoveredSkillCapabilities,
          ...discoveredPluginCapabilities,
        ];
        const idMap = new Map<string, typeof merged[number]>();
        for (const cap of merged) {
          const existing = idMap.get(cap.id);
          if (existing) {
            for (const a of cap.compatible_agents) {
              if (!existing.compatible_agents.includes(a)) {
                existing.compatible_agents.push(a);
              }
            }
          } else {
            idMap.set(cap.id, cap);
          }
        }
        const allCapabilities = [...idMap.values()];

        const seenItems = getSeenItems();
        const newIds = new Set<string>();
        const now = Date.now();

        for (const cap of allCapabilities) {
          if (!seenItems[cap.id]) {
            seenItems[cap.id] = now;
            newIds.add(cap.id);
          } else if (isItemNew(cap.id, seenItems)) {
            newIds.add(cap.id);
          }
        }

        saveSeenItems(seenItems);

        set({ capabilities: allCapabilities, loading: false, newItemIds: newIds });
      } catch (error) {
        console.error("Failed to load capabilities:", error);
        set({
          error: error instanceof Error ? error.message : "Failed to load capabilities",
          loading: false,
        });
      }
    },
    
    markAsSeen: (id: string) => {
      const seenItems = getSeenItems();
      if (!seenItems[id]) {
        seenItems[id] = Date.now();
        saveSeenItems(seenItems);
      }
      set((state) => {
        const newIds = new Set(state.newItemIds);
        newIds.delete(id);
        return { newItemIds: newIds };
      });
    },

    setTypeFilter: (type) => {
      set((state) => ({
        filters: { ...state.filters, type },
      }));
    },

    setVisibilityFilter: (visibility) => {
      set((state) => ({
        filters: { ...state.filters, visibility },
      }));
    },

    setAdapterFilter: (adapter) => {
      set((state) => ({
        filters: { ...state.filters, adapter },
      }));
    },

    setSearch: (search) => {
      set((state) => ({
        filters: { ...state.filters, search },
      }));
    },

    setCategoryFilter: (category) => {
      set((state) => ({
        filters: { ...state.filters, category },
      }));
    },

    setSortFilter: (sort) => {
      set((state) => ({
        filters: { ...state.filters, sort },
      }));
    },

    clearFilters: () => {
      set({ filters: initialFilters });
    },

    toggleSelection: (id) => {
      set((state) => {
        const newSet = new Set(state.selectedIds);
        if (newSet.has(id)) {
          newSet.delete(id);
        } else {
          newSet.add(id);
        }
        return { selectedIds: newSet };
      });
    },

    selectAll: (ids) => {
      set({ selectedIds: new Set(ids) });
    },

    clearSelection: () => {
      set({ selectedIds: new Set() });
    },

    setDetailCapability: (capability) => {
      set({ detailCapability: capability });
    },

    openEditor: (capability) => {
      set({ editorOpen: true, editingCapability: capability || null });
    },

    closeEditor: () => {
      set({ editorOpen: false, editingCapability: null });
    },

    openDeployWizard: (capabilityIds) => {
      set({
        deployWizardOpen: true,
        deployCapabilityIds: capabilityIds || [],
      });
    },

    closeDeployWizard: () => {
      set({ deployWizardOpen: false, deployCapabilityIds: [] });
    },
  })
);

export function useFilteredCapabilities(): UniversalCapability[] {
  const { capabilities, filters } = useRegistryStore();

  const filtered = capabilities.filter((capability) => {
    if (filters.type !== "all" && capability.type !== filters.type) {
      return false;
    }

    // "All" shows public + private only; discovered requires explicit filter
    if (filters.visibility === "all") {
      if (capability.visibility === "discovered") {
        return false;
      }
    } else if (capability.visibility !== filters.visibility) {
      return false;
    }

    if (filters.adapter !== "all") {
      if (!capability.compatible_agents.includes(filters.adapter)) {
        return false;
      }
    }

    if (filters.category !== "all" && capability.category !== filters.category) {
      return false;
    }

    if (filters.search) {
      const searchLower = filters.search.toLowerCase();
      const matchesName = capability.name.toLowerCase().includes(searchLower);
      const matchesId = capability.id.toLowerCase().includes(searchLower);
      const matchesDesc = capability.description.toLowerCase().includes(searchLower);
      const matchesTags = capability.tags.some((tag) =>
        tag.toLowerCase().includes(searchLower)
      );
      if (!matchesName && !matchesId && !matchesDesc && !matchesTags) {
        return false;
      }
    }

    return true;
  });

  if (filters.sort === "stars") {
    filtered.sort((a, b) => (b.stats?.github_stars ?? 0) - (a.stats?.github_stars ?? 0));
  } else if (filters.sort === "newest") {
    filtered.sort((a, b) => {
      const dateA = a.stats?.updated_at ?? "";
      const dateB = b.stats?.updated_at ?? "";
      return dateB.localeCompare(dateA);
    });
  } else {
    filtered.sort((a, b) => a.name.localeCompare(b.name));
  }

  return filtered;
}

export function useCapabilityCounts(): Record<CapabilityType | "all", number> {
  const { capabilities } = useRegistryStore();

  // Exclude discovered from counts (discovered only shown with explicit filter)
  const nonDiscovered = capabilities.filter((c) => c.visibility !== "discovered");
  const counts: Record<CapabilityType | "all", number> = {
    all: nonDiscovered.length,
    mcp: 0,
    rule: 0,
    skill: 0,
    hook: 0,
    plugin: 0,
    custom: 0,
  };

  for (const cap of nonDiscovered) {
    counts[cap.type]++;
  }

  return counts;
}

export function useNewItemsCount(): number {
  const { newItemIds } = useRegistryStore();
  return newItemIds.size;
}

export function useIsCapabilityNew(id: string): boolean {
  const { newItemIds } = useRegistryStore();
  return newItemIds.has(id);
}
