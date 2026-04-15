import { useEffect, useState } from "react";
import { useProjectStore, getProjectName } from "../../stores/projectStore";
import type { DetectedAdapter, RecentProject } from "../../lib/tauri";
import { ConfirmDialog } from "../common/ConfirmDialog";

interface ProjectSelectorProps {
  onProjectSelected?: (path: string) => void;
  onGlobalDeploy?: () => void;
}

export function ProjectSelector({ onProjectSelected, onGlobalDeploy }: ProjectSelectorProps) {
  const {
    selectedProject,
    detectedAdapters,
    recentProjects,
    loading,
    selectProject,
    setSelectedProject,
    loadRecentProjects,
    removeProject,
    clearSelection,
  } = useProjectStore();

  useEffect(() => {
    loadRecentProjects();
  }, [loadRecentProjects]);

  useEffect(() => {
    if (selectedProject && onProjectSelected) {
      onProjectSelected(selectedProject);
    }
  }, [selectedProject, onProjectSelected]);

  const handleBrowse = async () => {
    await selectProject();
  };

  const handleSelectRecent = async (path: string) => {
    await setSelectedProject(path);
  };

  const [removeConfirmPath, setRemoveConfirmPath] = useState<string | null>(null);

  const handleRemoveRecentClick = (e: React.MouseEvent, path: string) => {
    e.stopPropagation();
    setRemoveConfirmPath(path);
  };

  const handleRemoveRecentConfirm = async () => {
    if (!removeConfirmPath) return;
    try {
      await removeProject(removeConfirmPath);
    } finally {
      setRemoveConfirmPath(null);
    }
  };

  if (selectedProject) {
    return (
      <div className="p-4">
        <SelectedProjectPill
          path={selectedProject}
          adapters={detectedAdapters}
          onClear={clearSelection}
        />
      </div>
    );
  }

  return (
    <div className="p-4 space-y-4">
      {onGlobalDeploy && (
        <div className="p-4 bg-app-card border border-border rounded-lg flex items-center justify-between gap-4 hover:bg-app-card-hover transition-colors">
          <div className="flex items-center gap-3">
            <span className="text-2xl">🌐</span>
            <div>
              <p className="font-medium text-text-primary">Deploy to Global Config</p>
              <p className="text-xs text-text-muted">Write MCPs and rules to your system-wide IDE settings. No project folder needed.</p>
            </div>
          </div>
          <button
            onClick={onGlobalDeploy}
            data-testid="deploy-globally"
            className="shrink-0 px-4 py-2 bg-accent-blue text-white rounded-lg text-sm font-medium hover:bg-accent-blue/80 transition-colors"
          >
            Deploy Globally →
          </button>
        </div>
      )}

      {onGlobalDeploy && (
        <div className="flex items-center gap-3 text-xs text-text-muted">
          <div className="flex-1 h-px bg-border" />
          <span>or select a project</span>
          <div className="flex-1 h-px bg-border" />
        </div>
      )}

      <div
        onClick={handleBrowse}
        data-testid="project-browse"
        className="border-2 border-dashed border-border rounded-lg p-8 text-center cursor-pointer hover:border-accent-blue hover:bg-accent-blue/5 transition-colors"
      >
        <div className="text-4xl mb-3">📁</div>
        <p className="text-text-primary font-medium mb-1">
          {loading ? "Opening..." : "Drop a folder or click to browse"}
        </p>
        <p className="text-sm text-text-muted">
          Select a project folder to deploy capabilities
        </p>
      </div>

      {recentProjects.length > 0 && (
        <div>
          <p className="text-xs text-text-muted uppercase mb-3">Recent Projects</p>
          <div className="space-y-2">
            {recentProjects.map((project) => (
              <RecentProjectItem
                key={project.path}
                project={project}
                onSelect={() => handleSelectRecent(project.path)}
                onRemove={(e) => handleRemoveRecentClick(e, project.path)}
              />
            ))}
          </div>
        </div>
      )}

      <ConfirmDialog
        isOpen={!!removeConfirmPath}
        title="Remove from Recents"
        message={
          removeConfirmPath
            ? `Are you sure you want to remove this project from the list? The project folder and files are not deleted.`
            : ""
        }
        onConfirm={handleRemoveRecentConfirm}
        onCancel={() => setRemoveConfirmPath(null)}
      />
    </div>
  );
}

function SelectedProjectPill({
  path,
  adapters,
  onClear,
}: {
  path: string;
  adapters: DetectedAdapter[];
  onClear: () => void;
}) {
  return (
    <div className="flex items-center gap-3 p-4 bg-accent-blue/10 border border-accent-blue/30 rounded-lg">
      <div className="flex-1">
        <div className="flex items-center gap-2 mb-1">
          <span className="text-2xl">📁</span>
          <span className="font-medium text-text-primary">{getProjectName(path)}</span>
        </div>
        <p className="text-xs text-text-muted truncate ml-9">{path}</p>
        {adapters.length > 0 && (
          <div className="flex items-center gap-2 mt-2 ml-9">
            {adapters.map((adapter) => (
              <AdapterBadge key={adapter.id} adapter={adapter} />
            ))}
          </div>
        )}
      </div>
      <button
        onClick={onClear}
        className="w-8 h-8 flex items-center justify-center rounded-full hover:bg-white/10 text-text-muted hover:text-text-primary transition-colors"
        title="Clear selection"
      >
        ✕
      </button>
    </div>
  );
}

function RecentProjectItem({
  project,
  onSelect,
  onRemove,
}: {
  project: RecentProject;
  onSelect: () => void;
  onRemove: (e: React.MouseEvent) => void;
}) {
  return (
    <div
      onClick={onSelect}
      data-testid={`recent-project-${project.name}`}
      className="group flex items-center gap-3 p-3 rounded-lg bg-app-card border border-border hover:bg-app-card-hover cursor-pointer transition-colors"
    >
      <span className="text-xl">📁</span>
      <div className="flex-1 min-w-0">
        <p className="font-medium text-text-primary truncate">{project.name}</p>
        <p className="text-xs text-text-muted truncate">{project.path}</p>
      </div>
      <button
        onClick={onRemove}
        className="w-6 h-6 flex items-center justify-center rounded opacity-0 group-hover:opacity-100 hover:bg-accent-red/20 text-text-muted hover:text-accent-red transition-all"
        title="Remove from recents"
      >
        ✕
      </button>
    </div>
  );
}

function AdapterBadge({ adapter }: { adapter: DetectedAdapter }) {
  const colors: Record<string, string> = {
    "claude-code": "bg-accent-purple/20 text-accent-purple",
    cursor: "bg-accent-blue/20 text-accent-blue",
    windsurf: "bg-accent-green/20 text-accent-green",
  };

  return (
    <span className={`text-[10px] font-medium px-2 py-1 rounded ${colors[adapter.id] || "bg-text-muted/20 text-text-muted"}`}>
      {adapter.name}
    </span>
  );
}
