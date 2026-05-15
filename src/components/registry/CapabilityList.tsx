import { useState, useEffect, useCallback, useRef } from "react";
import { useNavigate, useLocation } from "react-router-dom";
import { CapabilityCard } from "./CapabilityCard";
import {
  useRegistryStore,
  useFilteredCapabilities,
} from "../../stores/registryStore";
import type {
  UniversalCapability,
  Visibility,
  AdapterType,
} from "../../lib/types";
import { getOfficialSkillsIndex, getMcpRegistryPopular, searchMcpRegistry } from "../../lib/tauri";
import type { OfficialSkillEntry, McpRegistryEntry } from "../../lib/tauri";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getAdapterIconImg } from "../../lib/adapterPlugins";
import logoIcon from "../../assets/icon.png";

interface CapabilityListProps {
  onOpenDetail: (capability: UniversalCapability) => void;
  onEdit?: (capability: UniversalCapability) => void;
  onDelete?: (id: string) => void;
  onFork?: (capability: UniversalCapability) => void;
  onDeploy: (ids: string[]) => void;
  onSaveAsPreset: (ids: string[]) => void;
  onNewCapability?: () => void;
  onImportOfficialSkill?: (entry: OfficialSkillEntry) => void;
  onImportOfficialMcp?: (entry: McpRegistryEntry) => void;
}

const visibilityOptions: { value: Visibility | "all"; label: string }[] = [
  { value: "all", label: "All" },
  { value: "public", label: "Public" },
  { value: "private", label: "Private" },
  { value: "discovered", label: "Discovered" },
];

const adapterOptions: { value: AdapterType | "all"; label: string }[] = [
  { value: "all", label: "All Adapters" },
  { value: "claude-code", label: "Claude Code" },
  { value: "cursor", label: "Cursor" },
  { value: "windsurf", label: "Windsurf" },
];

function OfficialSkillCard({ skill, onImport }: { skill: OfficialSkillEntry; onImport?: (entry: OfficialSkillEntry) => void }) {
  const [expanded, setExpanded] = useState(false);
  const descRef = useRef<HTMLParagraphElement>(null);
  const [isClamped, setIsClamped] = useState(false);

  useEffect(() => {
    const el = descRef.current;
    if (el) {
      setIsClamped(el.scrollHeight > el.clientHeight + 1);
    }
  }, [skill.description]);

  return (
    <div className="bg-app-card border border-border rounded-lg p-4 hover:border-accent-blue/50 transition-colors">
      <div className="flex items-start justify-between mb-2">
        <span className="text-sm font-medium text-text-primary">{skill.name}</span>
        {skill.has_scripts && (
          <span className="text-[9px] bg-yellow-500/15 text-yellow-400 px-1.5 py-0.5 rounded">
            scripts
          </span>
        )}
      </div>
      <p
        ref={descRef}
        className={`text-xs text-text-secondary mb-1 ${expanded ? "" : "line-clamp-2"}`}
      >
        {skill.description || "No description"}
      </p>
      {isClamped && (
        <button
          onClick={() => setExpanded(!expanded)}
          className="text-[10px] text-accent-blue hover:underline mb-2 block"
        >
          {expanded ? "Show less" : "Show more"}
        </button>
      )}
      {!isClamped && <div className="mb-2" />}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="text-[10px] text-text-muted">{skill.file_count} files</span>
          {skill.github_url && (
            <button
              onClick={() => openUrl(skill.github_url)}
              className="w-5 h-5 flex items-center justify-center rounded-full bg-accent-blue/15 hover:bg-accent-blue/30 transition-colors cursor-pointer"
              title={skill.github_url}
            >
              <span className="text-[10px]">🔗</span>
            </button>
          )}
        </div>
        {onImport && (
          <button
            onClick={() => onImport(skill)}
            className="text-xs text-accent-blue hover:underline font-medium"
          >
            Import →
          </button>
        )}
      </div>
    </div>
  );
}

function OfficialMcpCard({ entry, onImport }: { entry: McpRegistryEntry; onImport?: (entry: McpRegistryEntry) => void }) {
  const [expanded, setExpanded] = useState(false);
  const descRef = useRef<HTMLParagraphElement>(null);
  const [isClamped, setIsClamped] = useState(false);

  useEffect(() => {
    const el = descRef.current;
    if (el) {
      setIsClamped(el.scrollHeight > el.clientHeight + 1);
    }
  }, [entry.description]);

  const transportColor = entry.transport === "stdio"
    ? "bg-blue-500/15 text-blue-400"
    : entry.transport === "streamable-http"
    ? "bg-green-500/15 text-green-400"
    : "bg-purple-500/15 text-purple-400";

  const displayName = entry.title || entry.name.split("/").pop() || entry.name;

  return (
    <div className="bg-app-card border border-border rounded-lg p-4 hover:border-accent-blue/50 transition-colors">
      <div className="flex items-start justify-between mb-2 gap-2">
        <div className="flex items-center gap-2 min-w-0">
          {entry.icon_url ? (
            <img
              src={entry.icon_url}
              alt=""
              className="w-5 h-5 rounded object-contain flex-shrink-0"
              onError={(e) => { (e.target as HTMLImageElement).style.display = "none"; }}
            />
          ) : (
            <span className="text-sm flex-shrink-0">🔌</span>
          )}
          <span className="text-sm font-medium text-text-primary truncate">{displayName}</span>
        </div>
        <span className={`text-[9px] px-1.5 py-0.5 rounded flex-shrink-0 ${transportColor}`}>
          {entry.transport === "streamable-http" ? "http" : entry.transport}
        </span>
      </div>
      <p
        ref={descRef}
        className={`text-xs text-text-secondary mb-1 ${expanded ? "" : "line-clamp-2"}`}
      >
        {entry.description || "No description"}
      </p>
      {isClamped && (
        <button
          onClick={() => setExpanded(!expanded)}
          className="text-[10px] text-accent-blue hover:underline mb-2 block"
        >
          {expanded ? "Show less" : "Show more"}
        </button>
      )}
      {!isClamped && <div className="mb-2" />}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          {entry.env_vars.length > 0 && (
            <span className="text-[10px] text-text-muted">
              {entry.env_vars.length} env var{entry.env_vars.length > 1 ? "s" : ""}
            </span>
          )}
          {entry.is_official && (
            <span className="text-[9px] bg-accent-blue/15 text-accent-blue px-1.5 py-0.5 rounded">
              official
            </span>
          )}
          {(entry.repository_url || entry.website_url) && (
            <button
              onClick={() => openUrl(entry.repository_url || entry.website_url || "")}
              className="w-5 h-5 flex items-center justify-center rounded-full bg-accent-blue/15 hover:bg-accent-blue/30 transition-colors cursor-pointer"
              title={entry.repository_url || entry.website_url}
            >
              <span className="text-[10px]">🔗</span>
            </button>
          )}
        </div>
        {onImport && (
          <button
            onClick={() => onImport(entry)}
            className="text-xs text-accent-blue hover:underline font-medium"
          >
            Import →
          </button>
        )}
      </div>
    </div>
  );
}

export function CapabilityList({
  onOpenDetail,
  onEdit,
  onDelete,
  onFork,
  onDeploy,
  onSaveAsPreset,
  onNewCapability,
  onImportOfficialSkill,
  onImportOfficialMcp,
}: CapabilityListProps) {
  const {
    loading,
    error,
    filters,
    selectedIds,
    newItemIds,
    setVisibilityFilter,
    setAdapterFilter,
    setCategoryFilter,
    setSortFilter,
    toggleSelection,
    clearSelection,
  } = useRegistryStore();

  const filteredCapabilities = useFilteredCapabilities();
  const { capabilities } = useRegistryStore();
  const navigate = useNavigate();
  const location = useLocation();
  const isRecommendedRoute = location.pathname === "/recommendations";

  const selectedArray = Array.from(selectedIds);

  // Official skills section — shown when type=skill and visibility=public
  const showOfficialSkills = (filters.type === "skill" || filters.type === "all") && (filters.visibility === "public" || filters.visibility === "all");
  const [officialSkills, setOfficialSkills] = useState<OfficialSkillEntry[]>([]);
  const [officialLoading, setOfficialLoading] = useState(false);
  const [officialError, setOfficialError] = useState<string | null>(null);

  // Official MCP Registry section — shown when type=mcp or all, and visibility=public or all
  const showOfficialMcps = (filters.type === "mcp" || filters.type === "all") && (filters.visibility === "public" || filters.visibility === "all");
  const [officialMcps, setOfficialMcps] = useState<McpRegistryEntry[]>([]);
  const [mcpRegistryLoading, setMcpRegistryLoading] = useState(false);
  const [mcpRegistryError, setMcpRegistryError] = useState<string | null>(null);
  const [mcpRegistrySearch, setMcpRegistrySearch] = useState("");
  const mcpSearchTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [mcpPage, setMcpPage] = useState(1);
  const [mcpNextCursor, setMcpNextCursor] = useState<string | undefined>(undefined);
  const [mcpCursorHistory, setMcpCursorHistory] = useState<(string | undefined)[]>([undefined]); // cursor per page index

  const fetchPopularMcps = useCallback(async (forceRefresh: boolean, cursor?: string, page?: number) => {
    setMcpRegistryLoading(true);
    setMcpRegistryError(null);
    try {
      const result = await getMcpRegistryPopular(forceRefresh, cursor, page);
      setOfficialMcps(result.entries);
      setMcpNextCursor(result.next_cursor);
      setMcpPage(result.page);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      setMcpRegistryError(msg);
    } finally {
      setMcpRegistryLoading(false);
    }
  }, []);

  const handleMcpPageNext = useCallback(() => {
    if (!mcpNextCursor) return;
    const nextPage = mcpPage + 1;
    // Save the cursor for the next page
    setMcpCursorHistory((prev) => {
      const updated = [...prev];
      updated[nextPage - 1] = mcpNextCursor;
      return updated;
    });
    if (mcpRegistrySearch.trim()) {
      searchMcpRegistry(mcpRegistrySearch.trim(), 50, mcpNextCursor, nextPage).then((result) => {
        setOfficialMcps(result.entries);
        setMcpNextCursor(result.next_cursor);
        setMcpPage(result.page);
      }).catch(() => {});
    } else {
      fetchPopularMcps(false, mcpNextCursor, nextPage);
    }
  }, [mcpNextCursor, mcpPage, mcpRegistrySearch, fetchPopularMcps]);

  const handleMcpPagePrev = useCallback(() => {
    if (mcpPage <= 1) return;
    const prevPage = mcpPage - 1;
    const prevCursor = mcpCursorHistory[prevPage - 1];
    if (mcpRegistrySearch.trim()) {
      searchMcpRegistry(mcpRegistrySearch.trim(), 50, prevCursor, prevPage).then((result) => {
        setOfficialMcps(result.entries);
        setMcpNextCursor(result.next_cursor);
        setMcpPage(result.page);
      }).catch(() => {});
    } else {
      fetchPopularMcps(prevPage === 1, prevCursor, prevPage);
    }
  }, [mcpPage, mcpCursorHistory, mcpRegistrySearch, fetchPopularMcps]);

  const searchMcpRegistryDebounced = useCallback((query: string) => {
    if (mcpSearchTimer.current) clearTimeout(mcpSearchTimer.current);
    if (!query.trim()) {
      setMcpPage(1);
      setMcpNextCursor(undefined);
      setMcpCursorHistory([undefined]);
      fetchPopularMcps(false);
      return;
    }
    mcpSearchTimer.current = setTimeout(async () => {
      setMcpRegistryLoading(true);
      setMcpRegistryError(null);
      setMcpPage(1);
      setMcpCursorHistory([undefined]);
      try {
        const result = await searchMcpRegistry(query.trim(), 50);
        setOfficialMcps(result.entries);
        setMcpNextCursor(result.next_cursor);
        setMcpPage(1);
      } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        setMcpRegistryError(msg);
      } finally {
        setMcpRegistryLoading(false);
      }
    }, 300);
  }, [fetchPopularMcps]);

  useEffect(() => {
    if (showOfficialMcps && officialMcps.length === 0 && !mcpRegistryLoading) {
      fetchPopularMcps(false);
    }
  }, [showOfficialMcps, officialMcps.length, mcpRegistryLoading, fetchPopularMcps]);

  // Filter official MCPs by global search query
  const filteredOfficialMcps = officialMcps.filter((entry) => {
    if (!filters.search) return true;
    const q = filters.search.toLowerCase();
    return entry.name.toLowerCase().includes(q) || entry.title.toLowerCase().includes(q) || entry.description.toLowerCase().includes(q);
  });

  // Skills pagination (client-side since we fetch all at once)
  const SKILLS_PER_PAGE = 50;
  const [skillsPage, setSkillsPage] = useState(1);

  const fetchOfficialSkills = useCallback(async (forceRefresh: boolean) => {
    setOfficialLoading(true);
    setOfficialError(null);
    try {
      const entries = await getOfficialSkillsIndex(forceRefresh);
      setOfficialSkills(entries);
    } catch (err: unknown) {
      const msg = err instanceof Error ? err.message : String(err);
      if (msg.startsWith("RATE_LIMITED:")) {
        const resetTs = parseInt(msg.split(":")[1], 10);
        const now = Math.floor(Date.now() / 1000);
        const waitMins = Math.max(1, Math.ceil((resetTs - now) / 60));
        setOfficialError(`GitHub API rate limited. Retry in ${waitMins} min.`);
      } else {
        setOfficialError(msg);
      }
    } finally {
      setOfficialLoading(false);
    }
  }, []);

  useEffect(() => {
    if (showOfficialSkills && officialSkills.length === 0 && !officialLoading) {
      fetchOfficialSkills(false);
    }
  }, [showOfficialSkills, officialSkills.length, officialLoading, fetchOfficialSkills]);

  // Filter official skills by search query
  const allFilteredOfficialSkills = officialSkills.filter((skill) => {
    if (!filters.search) return true;
    const q = filters.search.toLowerCase();
    return skill.name.toLowerCase().includes(q) || skill.description.toLowerCase().includes(q);
  });
  const skillsTotalPages = Math.ceil(allFilteredOfficialSkills.length / SKILLS_PER_PAGE);
  const filteredOfficialSkills = allFilteredOfficialSkills.slice(
    (skillsPage - 1) * SKILLS_PER_PAGE,
    skillsPage * SKILLS_PER_PAGE
  );

  if (loading) {
    return (
      <div className="p-6">
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {[...Array(6)].map((_, i) => (
            <div
              key={i}
              className="bg-app-card border border-border rounded-lg h-48 animate-pulse"
            />
          ))}
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-6 text-center">
        <p className="text-accent-red mb-2">Failed to load capabilities</p>
        <p className="text-sm text-text-secondary">{error}</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="px-6 py-4 border-b border-border flex items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <div className="flex rounded-md overflow-hidden border border-border">
            {visibilityOptions.map((opt) => (
              <button
                key={opt.value}
                onClick={() => setVisibilityFilter(opt.value)}
                className={`px-3 py-1.5 text-sm transition-colors ${
                  !isRecommendedRoute && filters.visibility === opt.value
                    ? "bg-accent-blue text-white"
                    : "bg-app-card text-text-secondary hover:text-text-primary"
                }`}
              >
                {opt.label}
              </button>
            ))}
            <button
              onClick={() => navigate("/recommendations")}
              className={`px-3 py-1.5 text-sm transition-colors ${
                isRecommendedRoute
                  ? "bg-accent-blue text-white"
                  : "bg-app-card text-text-secondary hover:text-text-primary"
              }`}
            >
              Recommended
            </button>
          </div>

          <div className="relative">
            <button
              onClick={() => {
                const el = document.getElementById("adapter-filter-dropdown");
                if (el) el.classList.toggle("hidden");
              }}
              className="h-9 px-3 rounded-md bg-app-card border border-border text-sm text-text-primary flex items-center gap-2"
            >
              {filters.adapter !== "all" && getAdapterIconImg(filters.adapter) && (
                <img src={getAdapterIconImg(filters.adapter)} alt="" className="w-3.5 h-3.5 object-contain" />
              )}
              <span>{adapterOptions.find((o) => o.value === filters.adapter)?.label ?? "All Adapters"}</span>
              <svg className="w-3 h-3 text-text-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" /></svg>
            </button>
            <div
              id="adapter-filter-dropdown"
              className="hidden absolute top-full left-0 mt-1 bg-app-card border border-border rounded-lg shadow-xl z-50 min-w-[160px] py-1"
            >
              {adapterOptions.map((opt) => (
                <button
                  key={opt.value}
                  onClick={() => {
                    setAdapterFilter(opt.value);
                    document.getElementById("adapter-filter-dropdown")?.classList.add("hidden");
                  }}
                  className={`w-full text-left px-3 py-1.5 text-sm flex items-center gap-2 hover:bg-white/5 ${
                    filters.adapter === opt.value ? "text-accent-blue" : "text-text-primary"
                  }`}
                >
                  {opt.value !== "all" && getAdapterIconImg(opt.value) ? (
                    <img src={getAdapterIconImg(opt.value)} alt="" className="w-3.5 h-3.5 object-contain" />
                  ) : opt.value === "all" ? (
                    <span className="text-xs">✓</span>
                  ) : null}
                  <span>{opt.label}</span>
                </button>
              ))}
            </div>
          </div>

          {/* Category dropdown */}
          {(() => {
            const categories = [...new Set(capabilities.filter(c => c.category).map(c => c.category!))].sort();
            const categoryCounts = categories.reduce<Record<string, number>>((acc, cat) => {
              acc[cat] = capabilities.filter(c => c.category === cat).length;
              return acc;
            }, {});
            return categories.length > 0 ? (
              <div className="relative">
                <button
                  onClick={() => {
                    const el = document.getElementById("category-filter-dropdown");
                    if (el) el.classList.toggle("hidden");
                  }}
                  className="h-9 px-3 rounded-md bg-app-card border border-border text-sm text-text-primary flex items-center gap-2"
                >
                  <span>{filters.category === "all" ? "All Categories" : filters.category}</span>
                  <svg className="w-3 h-3 text-text-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" /></svg>
                </button>
                <div
                  id="category-filter-dropdown"
                  className="hidden absolute top-full left-0 mt-1 bg-app-card border border-border rounded-lg shadow-xl z-50 min-w-[200px] py-1 max-h-64 overflow-y-auto"
                >
                  <button
                    onClick={() => {
                      setCategoryFilter("all");
                      document.getElementById("category-filter-dropdown")?.classList.add("hidden");
                    }}
                    className={`w-full text-left px-3 py-1.5 text-sm flex items-center justify-between hover:bg-white/5 ${
                      filters.category === "all" ? "text-accent-blue" : "text-text-primary"
                    }`}
                  >
                    <span>All Categories</span>
                    <span className="text-xs text-text-muted">{capabilities.length}</span>
                  </button>
                  {categories.map((cat) => (
                    <button
                      key={cat}
                      onClick={() => {
                        setCategoryFilter(cat);
                        document.getElementById("category-filter-dropdown")?.classList.add("hidden");
                      }}
                      className={`w-full text-left px-3 py-1.5 text-sm flex items-center justify-between hover:bg-white/5 ${
                        filters.category === cat ? "text-accent-blue" : "text-text-primary"
                      }`}
                    >
                      <span>{cat}</span>
                      <span className="text-xs text-text-muted">{categoryCounts[cat]}</span>
                    </button>
                  ))}
                </div>
              </div>
            ) : null;
          })()}

          {/* Sort dropdown */}
          <div className="relative">
            <button
              onClick={() => {
                const el = document.getElementById("sort-filter-dropdown");
                if (el) el.classList.toggle("hidden");
              }}
              className="h-9 px-3 rounded-md bg-app-card border border-border text-sm text-text-primary flex items-center gap-2"
            >
              <span>{filters.sort === "name" ? "Name" : filters.sort === "stars" ? "Most Stars" : "Recently Updated"}</span>
              <svg className="w-3 h-3 text-text-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" /></svg>
            </button>
            <div
              id="sort-filter-dropdown"
              className="hidden absolute top-full left-0 mt-1 bg-app-card border border-border rounded-lg shadow-xl z-50 min-w-[160px] py-1"
            >
              {([
                { value: "name" as const, label: "Name" },
                { value: "stars" as const, label: "Most Stars" },
                { value: "newest" as const, label: "Recently Updated" },
              ]).map((opt) => (
                <button
                  key={opt.value}
                  onClick={() => {
                    setSortFilter(opt.value);
                    document.getElementById("sort-filter-dropdown")?.classList.add("hidden");
                  }}
                  className={`w-full text-left px-3 py-1.5 text-sm hover:bg-white/5 ${
                    filters.sort === opt.value ? "text-accent-blue" : "text-text-primary"
                  }`}
                >
                  {opt.label}
                </button>
              ))}
            </div>
          </div>
        </div>

        <div className="flex items-center gap-3">
          <p className="text-sm text-text-muted">
            Showing {filteredCapabilities.length} of {capabilities.length} capabilities
          </p>
          {onNewCapability && (
            <button
              onClick={onNewCapability}
              className="h-9 px-4 rounded-md bg-accent-blue text-white text-sm font-medium hover:bg-accent-blue/90 transition-colors flex items-center gap-1.5"
            >
              <span>+</span>
              <span>New</span>
            </button>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        {filteredCapabilities.length === 0 ? (
          <div className="text-center py-12">
            <div className="w-16 h-16 mx-auto mb-4 opacity-30">
              <img src={logoIcon} alt="AgentHarbor" className="w-full h-full" />
            </div>
            <p className="text-text-muted text-lg mb-2">No capabilities found</p>
            <p className="text-text-secondary text-sm">
              Try adjusting your filters or search query.
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {filteredCapabilities.map((capability) => (
              <CapabilityCard
                key={capability.id}
                capability={capability}
                selected={selectedIds.has(capability.id)}
                onSelect={toggleSelection}
                onDoubleClick={onOpenDetail}
                onEdit={capability.visibility === "private" ? onEdit : undefined}
                onDelete={capability.visibility === "private" ? onDelete : undefined}
                onFork={(capability.visibility === "public" || capability.visibility === "discovered") ? onFork : undefined}
                isNew={newItemIds.has(capability.id)}
              />
            ))}
          </div>
        )}

        {/* Official Skills Section */}
        {showOfficialSkills && (filteredOfficialSkills.length > 0 || officialLoading || officialError) && (
          <div className="mt-8">
            <div className="flex items-center justify-between mb-4">
              <div>
                <h3 className="text-sm font-semibold text-text-primary">Anthropic Official</h3>
                <p className="text-xs text-text-muted mt-0.5">
                  From github.com/anthropics/skills
                  {filters.search && ` \u00B7 ${filteredOfficialSkills.length} of ${officialSkills.length} matching`}
                </p>
              </div>
              <button
                onClick={() => fetchOfficialSkills(true)}
                disabled={officialLoading}
                className="text-xs text-accent-blue hover:underline disabled:opacity-50"
              >
                {officialLoading ? "Loading..." : "Refresh"}
              </button>
            </div>

            {officialError && (
              <div className="bg-red-500/10 border border-red-500/30 rounded-lg p-3 text-xs text-red-400 mb-4">
                {officialError}
              </div>
            )}

            {officialLoading && officialSkills.length === 0 && (
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                {[...Array(3)].map((_, i) => (
                  <div key={i} className="bg-app-card border border-border rounded-lg h-32 animate-pulse" />
                ))}
              </div>
            )}

            {filteredOfficialSkills.length > 0 && (
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                {filteredOfficialSkills.map((skill) => (
                  <OfficialSkillCard
                    key={skill.name}
                    skill={skill}
                    onImport={onImportOfficialSkill}
                  />
                ))}
              </div>
            )}

            {skillsTotalPages > 1 && (
              <div className="flex items-center justify-center gap-3 mt-4">
                <button
                  onClick={() => setSkillsPage((p) => Math.max(1, p - 1))}
                  disabled={skillsPage <= 1}
                  className="px-3 py-1.5 text-xs rounded bg-app-card border border-border text-text-primary hover:bg-app-card-hover disabled:opacity-30 disabled:cursor-not-allowed"
                >
                  ← Prev
                </button>
                <span className="text-xs text-text-muted">
                  Page {skillsPage} of {skillsTotalPages}
                </span>
                <button
                  onClick={() => setSkillsPage((p) => Math.min(skillsTotalPages, p + 1))}
                  disabled={skillsPage >= skillsTotalPages}
                  className="px-3 py-1.5 text-xs rounded bg-app-card border border-border text-text-primary hover:bg-app-card-hover disabled:opacity-30 disabled:cursor-not-allowed"
                >
                  Next →
                </button>
              </div>
            )}
          </div>
        )}

        {/* Official MCP Registry Section */}
        {showOfficialMcps && (filteredOfficialMcps.length > 0 || mcpRegistryLoading || mcpRegistryError) && (
          <div className="mt-8">
            <div className="flex items-center justify-between mb-4">
              <div>
                <h3 className="text-sm font-semibold text-text-primary">Official MCP Registry</h3>
                <p className="text-xs text-text-muted mt-0.5">
                  From registry.modelcontextprotocol.io
                  {filters.search && ` · ${filteredOfficialMcps.length} of ${officialMcps.length} matching`}
                </p>
              </div>
              <div className="flex items-center gap-3">
                <div className="relative">
                  <input
                    type="text"
                    placeholder="Search MCPs..."
                    value={mcpRegistrySearch}
                    onChange={(e) => {
                      setMcpRegistrySearch(e.target.value);
                      searchMcpRegistryDebounced(e.target.value);
                    }}
                    className="h-7 w-48 px-2.5 text-xs rounded bg-app-bg border border-border text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent-blue"
                  />
                </div>
                <button
                  onClick={() => {
                    setMcpRegistrySearch("");
                    setMcpPage(1);
                    setMcpNextCursor(undefined);
                    setMcpCursorHistory([undefined]);
                    fetchPopularMcps(true);
                  }}
                  disabled={mcpRegistryLoading}
                  className="text-xs text-accent-blue hover:underline disabled:opacity-50"
                >
                  {mcpRegistryLoading ? "Loading..." : "↻"}
                </button>
              </div>
            </div>

            {mcpRegistryError && (
              <div className="bg-red-500/10 border border-red-500/30 rounded-lg p-3 text-xs text-red-400 mb-4">
                {mcpRegistryError}
              </div>
            )}

            {mcpRegistryLoading && officialMcps.length === 0 && (
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                {[...Array(6)].map((_, i) => (
                  <div key={i} className="bg-app-card border border-border rounded-lg h-32 animate-pulse" />
                ))}
              </div>
            )}

            {filteredOfficialMcps.length > 0 && (
              <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
                {filteredOfficialMcps.map((entry) => (
                  <OfficialMcpCard
                    key={entry.name}
                    entry={entry}
                    onImport={onImportOfficialMcp}
                  />
                ))}
              </div>
            )}

            {(mcpPage > 1 || mcpNextCursor) && (
              <div className="flex items-center justify-center gap-3 mt-4">
                <button
                  onClick={handleMcpPagePrev}
                  disabled={mcpPage <= 1 || mcpRegistryLoading}
                  className="px-3 py-1.5 text-xs rounded bg-app-card border border-border text-text-primary hover:bg-app-card-hover disabled:opacity-30 disabled:cursor-not-allowed"
                >
                  ← Prev
                </button>
                <span className="text-xs text-text-muted">
                  Page {mcpPage}
                </span>
                <button
                  onClick={handleMcpPageNext}
                  disabled={!mcpNextCursor || mcpRegistryLoading}
                  className="px-3 py-1.5 text-xs rounded bg-app-card border border-border text-text-primary hover:bg-app-card-hover disabled:opacity-30 disabled:cursor-not-allowed"
                >
                  Next →
                </button>
              </div>
            )}
          </div>
        )}
      </div>

      {selectedArray.length > 0 && (
        <div className="sticky bottom-0 border-t border-border bg-app-sidebar px-6 py-4 flex items-center justify-between">
          <div className="flex items-center gap-4">
            <span className="text-sm text-text-primary">
              {selectedArray.length} selected
            </span>
            <button
              onClick={clearSelection}
              data-testid="clear-selection"
              className="text-sm text-accent-blue hover:underline"
            >
              Clear
            </button>
          </div>
          <div className="flex items-center gap-3">
            <button
              onClick={() => onSaveAsPreset(selectedArray)}
              className="h-9 px-4 rounded-md bg-app-card border border-border text-sm text-text-primary hover:bg-app-card-hover transition-colors"
            >
              Save as Preset
            </button>
            <button
              onClick={() => onDeploy(selectedArray)}
              data-testid="deploy-selected"
              className="h-9 px-4 rounded-md bg-accent-blue text-white text-sm font-medium hover:bg-accent-blue/90 transition-colors"
            >
              Deploy {selectedArray.length} Selected
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
