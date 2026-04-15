import { useState, useEffect } from "react";
import {
  getProjectDetail,
  addProject,
  openProjectInFinder,
  openProjectInTerminal,
  getProjectInstalledItems,
  removeProjectItem,
  type ProjectDetail as ProjectDetailType,
  type InstalledItem,
} from "../../lib/tauri";
import { PROJECTS_RELOAD_EVENT } from "./ProjectList";
import { DriftIndicator } from "./DriftIndicator";
import { DriftReview } from "./DriftReview";
import { AgentMemorySection } from "./AgentMemorySection";
import { ConfirmDialog } from "../common/ConfirmDialog";
import { fileManagerName, terminalName } from "../../lib/platform";

interface ProjectDetailProps {
  projectPath: string;
  onClose: () => void;
  onRedeploy: (projectPath: string) => void;
}

export function ProjectDetail({ projectPath, onClose, onRedeploy }: ProjectDetailProps) {
  const [project, setProject] = useState<ProjectDetailType | null>(null);
  const [loading, setLoading] = useState(true);
  const [activeTab, setActiveTab] = useState<"overview" | "history">("overview");
  const [showDriftReview, setShowDriftReview] = useState(false);
  const [addingToTracked, setAddingToTracked] = useState(false);
  const [installedItems, setInstalledItems] = useState<InstalledItem[]>([]);
  const [removingItem, setRemovingItem] = useState<string | null>(null);
  const [itemToRemove, setItemToRemove] = useState<InstalledItem | null>(null);

  useEffect(() => {
    loadProject();
    loadInstalledItems();
  }, [projectPath]);

  const loadProject = async () => {
    setLoading(true);
    try {
      const data = await getProjectDetail(projectPath);
      setProject(data);
    } catch (error) {
      console.error("Failed to load project:", error);
    } finally {
      setLoading(false);
    }
  };

  const loadInstalledItems = async () => {
    try {
      const items = await getProjectInstalledItems(projectPath);
      setInstalledItems(items);
    } catch (error) {
      console.error("Failed to load installed items:", error);
    }
  };

  const handleRemoveItem = (item: InstalledItem) => {
    setItemToRemove(item);
  };

  const confirmRemoveItem = async () => {
    if (!itemToRemove) return;
    const key = `${itemToRemove.adapter_id}:${itemToRemove.item_type}:${itemToRemove.name}`;
    setItemToRemove(null);
    setRemovingItem(key);
    try {
      await removeProjectItem(projectPath, itemToRemove.name, itemToRemove.item_type, itemToRemove.adapter_id);
      await loadInstalledItems();
      await loadProject();
    } catch (error) {
      console.error("Failed to remove item:", error);
    } finally {
      setRemovingItem(null);
    }
  };

  const handleOpenInFinder = async () => {
    try {
      await openProjectInFinder(projectPath);
    } catch (error) {
      console.error(`Failed to open in ${fileManagerName}:`, error);
    }
  };

  const handleOpenInTerminal = async () => {
    try {
      await openProjectInTerminal(projectPath);
    } catch (error) {
      console.error(`Failed to open in ${terminalName}:`, error);
    }
  };

  const handleAddToTracked = async () => {
    if (!project) return;
    setAddingToTracked(true);
    try {
      await addProject(projectPath, project.name);
      window.dispatchEvent(new CustomEvent(PROJECTS_RELOAD_EVENT));
      await loadProject();
    } catch (error) {
      console.error("Failed to add to tracked projects:", error);
    } finally {
      setAddingToTracked(false);
    }
  };

  if (loading) {
    return (
      <div className="w-96 border-l border-border bg-app-sidebar h-full flex items-center justify-center">
        <p className="text-text-muted">Loading...</p>
      </div>
    );
  }

  if (!project) {
    return (
      <div className="w-96 border-l border-border bg-app-sidebar h-full flex items-center justify-center">
        <p className="text-text-muted">Project not found</p>
      </div>
    );
  }

  return (
    <div className="w-96 border-l border-border bg-app-sidebar h-full flex flex-col">
      <div className="px-4 py-4 border-b border-border flex items-start justify-between">
        <div className="flex-1 min-w-0">
          <h3 className="text-lg font-semibold text-text-primary truncate">
            {project.name}
          </h3>
          <p className="text-xs font-mono text-text-muted truncate mt-0.5">
            {project.path}
          </p>
          <div className="mt-2">
            <DriftIndicator 
              projectPath={projectPath} 
              onShowDrift={() => setShowDriftReview(true)} 
            />
          </div>
        </div>
        <button
          onClick={onClose}
          className="text-text-muted hover:text-text-primary ml-2"
        >
          ✕
        </button>
      </div>

      {showDriftReview && (
        <DriftReview
          projectPath={projectPath}
          onClose={() => setShowDriftReview(false)}
          onResolved={() => {
            setShowDriftReview(false);
            loadProject();
          }}
        />
      )}

      <div className="flex border-b border-border">
        <button
          onClick={() => setActiveTab("overview")}
          className={`flex-1 px-4 py-2 text-sm font-medium ${
            activeTab === "overview"
              ? "text-accent-blue border-b-2 border-accent-blue"
              : "text-text-secondary hover:text-text-primary"
          }`}
        >
          Overview
        </button>
        <button
          onClick={() => setActiveTab("history")}
          className={`flex-1 px-4 py-2 text-sm font-medium ${
            activeTab === "history"
              ? "text-accent-blue border-b-2 border-accent-blue"
              : "text-text-secondary hover:text-text-primary"
          }`}
        >
          History ({project.deployments.length})
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-4">
        {activeTab === "overview" && (
          <div className="space-y-6">
            <section>
              <h4 className="text-xs font-semibold text-text-muted uppercase tracking-wider mb-2">
                Detected Adapters
              </h4>
              <div className="space-y-2">
                {project.detected_adapters.map((adapter) => (
                  <div
                    key={adapter.id}
                    className={`flex items-center justify-between p-2 rounded-lg ${
                      adapter.has_config ? "bg-accent-green/10" : "bg-app-card"
                    }`}
                  >
                    <div className="flex items-center gap-2">
                      <span
                        className={`w-2 h-2 rounded-full ${
                          adapter.has_config ? "bg-accent-green" : "bg-text-muted"
                        }`}
                      />
                      <span className="text-sm text-text-primary">{adapter.name}</span>
                    </div>
                    <span className="text-xs text-text-muted">
                      {adapter.has_config ? "Configured" : "Not configured"}
                    </span>
                  </div>
                ))}
              </div>
            </section>

            {installedItems.length > 0 && (
              <section>
                <h4 className="text-xs font-semibold text-text-muted uppercase tracking-wider mb-2">
                  Installed Items ({installedItems.length})
                </h4>
                <div className="space-y-1.5">
                  {installedItems.map((item) => {
                    const key = `${item.adapter_id}:${item.item_type}:${item.name}`;
                    const isRemoving = removingItem === key;
                    const typeColors: Record<string, string> = {
                      mcp: "text-accent-blue bg-accent-blue/10",
                      rule: "text-accent-green bg-accent-green/10",
                      skill: "text-yellow-400 bg-yellow-400/10",
                      hook: "text-orange-400 bg-orange-400/10",
                      plugin: "text-pink-400 bg-pink-400/10",
                      agent: "text-purple-400 bg-purple-400/10",
                    };
                    const colorClass = typeColors[item.item_type] || "text-text-secondary bg-app-card";

                    return (
                      <div
                        key={key}
                        className="flex items-center justify-between p-2 rounded-lg bg-app-card group"
                      >
                        <div className="flex items-center gap-2 min-w-0 flex-1">
                          <span className={`text-[10px] px-1.5 py-0.5 rounded font-medium uppercase shrink-0 ${colorClass}`}>
                            {item.item_type}
                          </span>
                          <span className="text-sm text-text-primary font-mono truncate">
                            {item.name}
                          </span>
                          <span className="text-[10px] text-text-muted shrink-0">
                            {item.adapter_name}
                          </span>
                        </div>
                        <button
                          onClick={() => handleRemoveItem(item)}
                          disabled={isRemoving}
                          className="text-text-muted hover:text-red-400 opacity-0 group-hover:opacity-100 transition-all ml-2 shrink-0 disabled:opacity-50"
                          title={`Remove ${item.name}`}
                        >
                          {isRemoving ? "..." : "✕"}
                        </button>
                      </div>
                    );
                  })}
                </div>
              </section>
            )}

            {installedItems.length === 0 && (
              <section>
                <h4 className="text-xs font-semibold text-text-muted uppercase tracking-wider mb-2">
                  Installed Items
                </h4>
                <p className="text-sm text-text-muted">
                  No capabilities or agents installed
                </p>
              </section>
            )}

            <section>
              <h4 className="text-xs font-semibold text-text-muted uppercase tracking-wider mb-2">
                Last Deployed
              </h4>
              <p className="text-sm text-text-secondary">
                {project.last_deployed
                  ? new Date(project.last_deployed).toLocaleString()
                  : "Never"}
              </p>
            </section>

            <AgentMemorySection projectPath={projectPath} />
          </div>
        )}

        {activeTab === "history" && (
          <div className="space-y-3">
            {project.deployments.length === 0 ? (
              <p className="text-sm text-text-muted text-center py-4">
                No deployments yet
              </p>
            ) : (
              [...project.deployments].reverse().map((deployment, idx) => (
                <div
                  key={`${deployment.timestamp}-${idx}`}
                  className="p-3 rounded-lg bg-app-card border border-border"
                >
                  <div className="flex items-center justify-between mb-2">
                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-accent-blue/20 text-accent-blue font-medium uppercase">
                      {deployment.adapter}
                    </span>
                    <span className="text-xs text-text-muted">
                      {new Date(deployment.timestamp).toLocaleString()}
                    </span>
                  </div>
                  <div className="text-xs text-text-secondary">
                    {deployment.capability_ids.length > 0 && (
                      <p>{deployment.capability_ids.length} capabilities</p>
                    )}
                    {deployment.agent_ids.length > 0 && (
                      <p>{deployment.agent_ids.length} agents</p>
                    )}
                  </div>
                </div>
              ))
            )}
          </div>
        )}
      </div>

      <div className="p-4 border-t border-border space-y-2">
        {project.is_tracked === false && (
          <button
            onClick={handleAddToTracked}
            disabled={addingToTracked}
            className="w-full px-3 py-2 text-sm bg-accent-green/20 text-accent-green border border-accent-green/40 rounded-lg hover:bg-accent-green/30 transition-colors disabled:opacity-50"
          >
            {addingToTracked ? "Adding…" : "Add to tracked projects"}
          </button>
        )}
        <div className="flex gap-2">
          <button
            onClick={handleOpenInFinder}
            className="flex-1 px-3 py-2 text-sm bg-app-card border border-border rounded-lg text-text-primary hover:bg-app-card-hover transition-colors"
          >
            Open in {fileManagerName}
          </button>
          <button
            onClick={handleOpenInTerminal}
            className="flex-1 px-3 py-2 text-sm bg-app-card border border-border rounded-lg text-text-primary hover:bg-app-card-hover transition-colors"
          >
            Open {terminalName}
          </button>
        </div>
        <button
          onClick={() => onRedeploy(projectPath)}
          className="w-full px-3 py-2 text-sm bg-accent-blue text-white rounded-lg hover:bg-accent-blue/90 transition-colors"
        >
          Deploy to Project
        </button>
      </div>

      <ConfirmDialog
        isOpen={!!itemToRemove}
        title="Remove Item"
        message={
          itemToRemove
            ? `Are you sure you want to remove "${itemToRemove.name}" (${itemToRemove.item_type}) from ${itemToRemove.adapter_name}? This will modify the project's configuration files.`
            : ""
        }
        confirmLabel="Remove"
        cancelLabel="Cancel"
        onConfirm={confirmRemoveItem}
        onCancel={() => setItemToRemove(null)}
      />
    </div>
  );
}
