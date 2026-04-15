import { useState, useEffect } from "react";
import {
  getAllProjects,
  selectProjectFolder,
  addProject,
  removeProject,
  type ProjectInfo,
} from "../../lib/tauri";
import type { UniversalCapability } from "../../lib/types";
import { useRegistryStore } from "../../stores/registryStore";
import { ConfirmDialog } from "../common/ConfirmDialog";
import { basename } from "../../lib/platform";

export const PROJECTS_RELOAD_EVENT = "agentharbor-projects-reload";

function countDiscoveredInProject(capabilities: UniversalCapability[], projectPath: string): number {
  const norm = (p: string) => p.replace(/\\/g, "/").replace(/\/$/, "") || p;
  const base = norm(projectPath);
  return capabilities.filter((c) => {
    if (c.visibility !== "discovered") return false;
    const src = "source" in c && typeof c.source === "string" ? c.source : "";
    if (!src) return false;
    const s = norm(src);
    return s === base || s.startsWith(base + "/");
  }).length;
}

interface ProjectListProps {
  onSelectProject: (path: string) => void;
  selectedPath: string | null;
}

export function ProjectList({ onSelectProject, selectedPath }: ProjectListProps) {
  const loadCapabilities = useRegistryStore((s) => s.loadCapabilities);
  const capabilities = useRegistryStore((s) => s.capabilities);
  const [projects, setProjects] = useState<ProjectInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState("");
  const [sortBy, setSortBy] = useState<"name" | "path" | "last_deployed">("last_deployed");
  const [sortDesc, setSortDesc] = useState(true);
  const [removeConfirm, setRemoveConfirm] = useState<ProjectInfo | null>(null);
  const [reloading, setReloading] = useState(false);

  useEffect(() => {
    loadProjects();
  }, []);

  useEffect(() => {
    const handler = () => loadProjects();
    window.addEventListener(PROJECTS_RELOAD_EVENT, handler);
    return () => window.removeEventListener(PROJECTS_RELOAD_EVENT, handler);
  }, []);

  const loadProjects = async () => {
    try {
      const data = await getAllProjects();
      setProjects(data);
    } catch (error) {
      console.error("Failed to load projects:", error);
    } finally {
      setLoading(false);
    }
  };

  const handleReload = async () => {
    setReloading(true);
    try {
      await loadProjects();
      window.dispatchEvent(new CustomEvent(PROJECTS_RELOAD_EVENT));
      await loadCapabilities();
    } finally {
      setReloading(false);
    }
  };

  const handleAddProject = async () => {
    try {
      const path = await selectProjectFolder();
      if (path) {
        const name = basename(path) || "Project";
        await addProject(path, name);
        loadProjects();
      }
    } catch (error) {
      console.error("Failed to add project:", error);
    }
  };

  const handleRemoveProjectClick = (project: ProjectInfo, e: React.MouseEvent) => {
    e.stopPropagation();
    setRemoveConfirm(project);
  };

  const handleRemoveProjectConfirm = async () => {
    if (!removeConfirm) return;
    try {
      await removeProject(removeConfirm.path);
      loadProjects();
      if (selectedPath === removeConfirm.path) {
        onSelectProject("");
      }
    } catch (error) {
      console.error("Failed to remove project:", error);
    } finally {
      setRemoveConfirm(null);
    }
  };

  const filteredProjects = projects
    .filter((p) => {
      if (!search) return true;
      const s = search.toLowerCase();
      return p.name.toLowerCase().includes(s) || p.path.toLowerCase().includes(s);
    })
    .sort((a, b) => {
      let cmp = 0;
      if (sortBy === "name") {
        cmp = a.name.localeCompare(b.name);
      } else if (sortBy === "path") {
        cmp = a.path.localeCompare(b.path);
      } else if (sortBy === "last_deployed") {
        const aTime = a.last_deployed ? new Date(a.last_deployed).getTime() : 0;
        const bTime = b.last_deployed ? new Date(b.last_deployed).getTime() : 0;
        cmp = aTime - bTime;
      }
      return sortDesc ? -cmp : cmp;
    });

  const toggleSort = (field: typeof sortBy) => {
    if (sortBy === field) {
      setSortDesc(!sortDesc);
    } else {
      setSortBy(field);
      setSortDesc(true);
    }
  };

  if (loading) {
    return (
      <div className="p-6 text-center text-text-muted">
        Loading projects...
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="px-6 py-4 border-b border-border flex items-center justify-between gap-4">
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search projects..."
          className="w-64 px-3 py-2 bg-app-bg border border-border rounded-lg text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:border-accent-blue"
        />
        <div className="flex items-center gap-2">
          <button
            onClick={handleReload}
            disabled={reloading}
            className="h-9 px-4 rounded-md bg-app-card border border-border text-text-secondary text-sm font-medium hover:bg-app-card-hover hover:text-text-primary transition-colors disabled:opacity-50"
            title="Reload projects and discovered capabilities"
          >
            {reloading ? "…" : "Reload"}
          </button>
          <button
            onClick={handleAddProject}
            className="h-9 px-4 rounded-md bg-accent-blue text-white text-sm font-medium hover:bg-accent-blue/90 transition-colors flex items-center gap-1.5"
          >
            <span>+</span>
            <span>Add Project</span>
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto">
        <div className="px-6 py-4">
          <h3 className="text-xs font-semibold uppercase tracking-wider text-text-muted mb-3">
            Tracked projects
          </h3>
        </div>
        {projects.length === 0 ? (
          <div className="px-6 pb-8 text-center">
            <p className="text-text-muted text-sm mb-2">No tracked projects yet</p>
            <p className="text-text-secondary text-xs mb-4">
              Add a project folder to track deployments
            </p>
            <button
              onClick={handleAddProject}
              className="px-4 py-2 bg-accent-blue text-white text-sm rounded-lg hover:bg-accent-blue/90"
            >
              Add Project
            </button>
          </div>
        ) : (
          <table className="w-full">
            <thead className="bg-app-bg sticky top-0">
              <tr className="border-b border-border">
                <th
                  className="text-left px-6 py-3 text-xs font-semibold text-text-muted uppercase tracking-wider cursor-pointer hover:text-text-primary"
                  onClick={() => toggleSort("name")}
                >
                  Name {sortBy === "name" && (sortDesc ? "↓" : "↑")}
                </th>
                <th
                  className="text-left px-6 py-3 text-xs font-semibold text-text-muted uppercase tracking-wider cursor-pointer hover:text-text-primary"
                  onClick={() => toggleSort("path")}
                >
                  Path {sortBy === "path" && (sortDesc ? "↓" : "↑")}
                </th>
                <th className="text-left px-6 py-3 text-xs font-semibold text-text-muted uppercase tracking-wider">
                  Adapters
                </th>
                <th className="text-center px-6 py-3 text-xs font-semibold text-text-muted uppercase tracking-wider">
                  Capabilities
                </th>
                <th className="text-center px-6 py-3 text-xs font-semibold text-text-muted uppercase tracking-wider">
                  Agents
                </th>
                <th
                  className="text-left px-6 py-3 text-xs font-semibold text-text-muted uppercase tracking-wider cursor-pointer hover:text-text-primary"
                  onClick={() => toggleSort("last_deployed")}
                >
                  Last Deployed {sortBy === "last_deployed" && (sortDesc ? "↓" : "↑")}
                </th>
                <th className="px-6 py-3"></th>
              </tr>
            </thead>
            <tbody>
              {filteredProjects.map((project) => (
                <tr
                  key={project.path}
                  onClick={() => onSelectProject(project.path)}
                  className={`border-b border-border cursor-pointer transition-colors ${
                    selectedPath === project.path
                      ? "bg-accent-blue/10"
                      : "hover:bg-app-card-hover"
                  }`}
                >
                  <td className="px-6 py-4">
                    <span className="text-sm font-medium text-text-primary">
                      {project.name}
                    </span>
                  </td>
                  <td className="px-6 py-4">
                    <span className="text-xs font-mono text-text-muted truncate max-w-xs block">
                      {project.path}
                    </span>
                  </td>
                  <td className="px-6 py-4">
                    <div className="flex gap-1">
                      {project.detected_adapters.map((adapter) => (
                        <span
                          key={adapter}
                          className="text-[10px] px-1.5 py-0.5 rounded bg-accent-green/20 text-accent-green font-medium"
                        >
                          {adapter === "claude-code" ? "CC" : adapter === "cursor" ? "Cu" : "Wi"}
                        </span>
                      ))}
                      {project.detected_adapters.length === 0 && (
                        <span className="text-xs text-text-muted">—</span>
                      )}
                    </div>
                  </td>
                  <td className="px-6 py-4 text-center">
                    <span className="text-sm text-text-secondary">
                      {(() => {
                        const discovered = countDiscoveredInProject(capabilities, project.path);
                        const deployed = project.capabilities_count;
                        if (discovered > 0 && deployed === 0) {
                          return `${deployed} (${discovered} discovered)`;
                        }
                        if (discovered > 0 && deployed > 0) {
                          return `${deployed} (${discovered} discovered)`;
                        }
                        return String(deployed);
                      })()}
                    </span>
                  </td>
                  <td className="px-6 py-4 text-center">
                    <span className="text-sm text-text-secondary">
                      {project.agents_count}
                    </span>
                  </td>
                  <td className="px-6 py-4">
                    <span className="text-xs text-text-muted">
                      {project.last_deployed
                        ? new Date(project.last_deployed).toLocaleDateString()
                        : "Never"}
                    </span>
                  </td>
                  <td className="px-6 py-4">
                    <button
                      onClick={(e) => handleRemoveProjectClick(project, e)}
                      className="text-text-muted hover:text-accent-red transition-colors"
                      title="Remove project"
                    >
                      ✕
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      <ConfirmDialog
        isOpen={!!removeConfirm}
        title="Remove Project"
        message={
          removeConfirm
            ? `Are you sure you want to remove "${removeConfirm.name}" from the list? The project folder and files are not deleted.`
            : ""
        }
        onConfirm={handleRemoveProjectConfirm}
        onCancel={() => setRemoveConfirm(null)}
      />
    </div>
  );
}
