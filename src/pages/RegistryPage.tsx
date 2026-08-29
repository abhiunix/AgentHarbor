import { useCallback, useState } from "react";
import { useRegistryStore } from "../stores/registryStore";
import { CapabilityList } from "../components/registry/CapabilityList";
import { CapabilityDetail } from "../components/registry/CapabilityDetail";
import { CapabilityEditor } from "../components/registry/CapabilityEditor";
import { DeployWizard } from "../components/deploy/DeployWizard";
import { SaveAsPresetModal } from "../components/presets/SaveAsPresetModal";
import { ConfirmDialog } from "../components/common/ConfirmDialog";
import { saveCustomCapability, deleteCustomCapability, fetchGithubSkill, getAuthorId } from "../lib/tauri";
import type { OfficialSkillEntry, McpRegistryEntry } from "../lib/tauri";
import type { UniversalCapability, Skill, McpServer, EnvVariable } from "../lib/types";
import { makeStableCompositeIdWithRetry } from "../lib/stableId";

export function RegistryPage() {
  const {
    detailCapability,
    setDetailCapability,
    clearSelection,
    loadCapabilities,
    editorOpen,
    editingCapability,
    openEditor,
    closeEditor,
    deployWizardOpen,
    deployCapabilityIds,
    openDeployWizard,
    closeDeployWizard,
  } = useRegistryStore();
  const [presetIds, setPresetIds] = useState<string[] | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<{
    id: string;
    name: string;
    type: string;
  } | null>(null);
  const [useCapabilityPendingIds, setUseCapabilityPendingIds] = useState<Set<string>>(new Set());
  const [useCapabilityError, setUseCapabilityError] = useState<string | null>(null);

  const handleOpenDetail = useCallback(
    (capability: UniversalCapability) => {
      setDetailCapability(capability);
    },
    [setDetailCapability]
  );

  const handleCloseDetail = useCallback(() => {
    setDetailCapability(null);
  }, [setDetailCapability]);

  const handleDeploy = useCallback((ids: string[]) => {
    openDeployWizard(ids);
  }, [openDeployWizard]);

  const handleSaveAsPreset = useCallback((ids: string[]) => {
    setPresetIds(ids);
  }, []);

  const handleEdit = useCallback((capability: UniversalCapability) => {
    openEditor(capability);
  }, [openEditor]);

  const handleFork = useCallback((capability: UniversalCapability) => {
    const { id: _id, source: _src, ...rest } = capability as UniversalCapability & { source?: string };
    const forked = { ...rest, visibility: "private" as const };
    openEditor(forked as unknown as UniversalCapability);
  }, [openEditor]);

  const handleUseCapability = useCallback(async (capability: UniversalCapability) => {
    if (useCapabilityPendingIds.has(capability.id)) return;
    setUseCapabilityPendingIds((prev) => new Set(prev).add(capability.id));
    setUseCapabilityError(null);
    try {
      if (capability.visibility === "private") {
        openDeployWizard([capability.id]);
        return;
      }
      const { id: _id, source: _src, ...rest } = capability as UniversalCapability & { source?: string };
      const authorId = await getAuthorId();
      const existingIds = new Set(useRegistryStore.getState().capabilities.map((c) => c.id));
      const newId = await makeStableCompositeIdWithRetry(authorId, existingIds);
      const forked = { ...rest, id: newId, visibility: "private" as const } as unknown as UniversalCapability;
      await saveCustomCapability(forked, undefined);
      await loadCapabilities();
      openDeployWizard([newId]);
    } catch (error) {
      const msg = error instanceof Error ? error.message : String(error);
      console.error("Failed to fork and deploy capability:", error);
      setUseCapabilityError(`Failed to use "${capability.name}": ${msg}`);
    } finally {
      setUseCapabilityPendingIds((prev) => {
        const next = new Set(prev);
        next.delete(capability.id);
        return next;
      });
    }
  }, [useCapabilityPendingIds, openDeployWizard, loadCapabilities]);

  const handleDeleteClick = useCallback((id: string) => {
    const cap = useRegistryStore.getState().capabilities.find((c) => c.id === id);
    if (!cap) return;
    setDeleteConfirm({ id: cap.id, name: cap.name, type: cap.type });
  }, []);

  const handleDeleteConfirm = useCallback(async () => {
    if (!deleteConfirm) return;
    try {
      await deleteCustomCapability(deleteConfirm.id, deleteConfirm.type);
      loadCapabilities();
    } catch (error) {
      console.error("Failed to delete capability:", error);
    } finally {
      setDeleteConfirm(null);
    }
  }, [deleteConfirm, loadCapabilities]);
  
  const handleOpenNewCapability = useCallback(() => {
    openEditor();
  }, [openEditor]);
  
  const handleSaveCapability = useCallback(async (capability: UniversalCapability) => {
    try {
      const originalId = editingCapability?.id;
      const saved = await saveCustomCapability(capability, originalId);
      closeEditor();
      loadCapabilities();
      // Update detail panel if it's showing the same capability
      if (detailCapability && detailCapability.id === saved.id) {
        setDetailCapability(saved);
      }
    } catch (error) {
      console.error("Failed to save capability:", error);
      throw error;
    }
  }, [loadCapabilities, closeEditor, editingCapability?.id, detailCapability, setDetailCapability]);
  
  const handleCloseEditor = useCallback(() => {
    closeEditor();
  }, [closeEditor]);

  const handleCopyJson = useCallback((capability: UniversalCapability) => {
    navigator.clipboard.writeText(JSON.stringify(capability, null, 2));
  }, []);

  const handleDeploySingle = useCallback((capability: UniversalCapability) => {
    openDeployWizard([capability.id]);
    setDetailCapability(null);
  }, [setDetailCapability, openDeployWizard]);

  const handleCloseDeployWizard = useCallback(() => {
    closeDeployWizard();
    clearSelection();
  }, [clearSelection, closeDeployWizard]);

  const handleClosePresetModal = useCallback(() => {
    setPresetIds(null);
  }, []);

  const handlePresetSaved = useCallback(() => {
    clearSelection();
  }, [clearSelection]);

  const handleImportOfficialSkill = useCallback(async (entry: OfficialSkillEntry) => {
    try {
      const fetched = await fetchGithubSkill(entry.github_url);
      // Sort files so SKILL.md is first
      const sortedFiles = [...fetched.files].sort((a, b) => {
        if (a.path === "SKILL.md") return -1;
        if (b.path === "SKILL.md") return 1;
        return 0;
      });
      // Create a partial Skill capability to populate the editor
      const skillData: Partial<Skill> & { type: "skill" } = {
        type: "skill",
        name: fetched.name,
        description: fetched.description,
        version: "1.0.0",
        author: "",
        visibility: "private",
        tags: [],
        compatible_agents: ["claude-code", "cursor", "windsurf"],
        files: sortedFiles,
        allowed_tools: fetched.allowed_tools,
        model: fetched.model,
        context: fetched.context,
        agent: fetched.agent,
        argument_hint: fetched.argument_hint,
        license: fetched.license,
      };
      openEditor(skillData as unknown as UniversalCapability);
    } catch (error) {
      console.error("Failed to import official skill:", error);
    }
  }, [openEditor]);

  const handleImportOfficialMcp = useCallback((entry: McpRegistryEntry) => {
    let serverConfig: Record<string, unknown>;
    if (entry.command) {
      serverConfig = {
        command: entry.command,
        args: entry.args,
      };
    } else if (entry.url) {
      serverConfig = { url: entry.url };
    } else {
      serverConfig = {};
    }

    if (entry.env_vars.length > 0) {
      serverConfig.env = Object.fromEntries(
        entry.env_vars.map((v) => [v.name, `\${${v.name}}`])
      );
    }

    const env: Record<string, EnvVariable> = {};
    for (const v of entry.env_vars) {
      env[v.name] = {
        type: v.is_secret ? "secret" : "string",
        label: `\${${v.name}}`,
        required: v.is_required,
      };
    }

    const mcpData: Partial<McpServer> & { type: "mcp" } = {
      type: "mcp",
      name: entry.title || entry.name.split("/").pop() || "MCP Server",
      description: entry.description,
      version: entry.version || "1.0.0",
      author: "",
      visibility: "private",
      tags: [],
      compatible_agents: ["claude-code", "cursor", "windsurf"],
      transport: entry.transport === "streamable-http" ? "http" : entry.transport,
      command: entry.command || "",
      args: entry.args,
      url: entry.url || "",
      env,
    };

    openEditor(mcpData as unknown as UniversalCapability);
  }, [openEditor]);

  return (
    <div className="h-full relative">
      {useCapabilityError && (
        <div className="fixed top-4 right-4 z-50 max-w-sm bg-red-500/10 border border-red-500/30 rounded-lg p-3 text-xs text-red-400 shadow-xl flex items-start justify-between gap-3">
          <span>{useCapabilityError}</span>
          <button
            onClick={() => setUseCapabilityError(null)}
            className="text-red-400 hover:text-red-300 shrink-0"
          >
            ✕
          </button>
        </div>
      )}

      <CapabilityList
        onOpenDetail={handleOpenDetail}
        onEdit={handleEdit}
        onDelete={handleDeleteClick}
        onFork={handleFork}
        onUseCapability={handleUseCapability}
        useCapabilityPendingIds={useCapabilityPendingIds}
        onDeploy={handleDeploy}
        onSaveAsPreset={handleSaveAsPreset}
        onNewCapability={handleOpenNewCapability}
        onImportOfficialSkill={handleImportOfficialSkill}
        onImportOfficialMcp={handleImportOfficialMcp}
      />

      {detailCapability && (
        <>
          <div
            className="fixed inset-0 bg-black/40 z-40"
            onClick={handleCloseDetail}
          />
          <CapabilityDetail
            capability={detailCapability}
            onClose={handleCloseDetail}
            onDeploy={handleDeploySingle}
            onEdit={detailCapability.visibility === "private" ? handleEdit : undefined}
            onFork={(detailCapability.visibility === "public" || detailCapability.visibility === "discovered") ? handleFork : undefined}
            onCopyJson={handleCopyJson}
          />
        </>
      )}

      {deployWizardOpen && (
        <DeployWizard
          isOpen={true}
          onClose={handleCloseDeployWizard}
          initialCapabilityIds={deployCapabilityIds}
        />
      )}

      {presetIds && (
        <SaveAsPresetModal
          capabilityIds={presetIds}
          onClose={handleClosePresetModal}
          onSaved={handlePresetSaved}
        />
      )}
      
      {editorOpen && (
        <CapabilityEditor
          capability={editingCapability || undefined}
          onSave={handleSaveCapability}
          onCancel={handleCloseEditor}
        />
      )}

      <ConfirmDialog
        isOpen={!!deleteConfirm}
        title="Delete Capability"
        message={
          deleteConfirm
            ? `Are you sure you want to delete "${deleteConfirm.name}"? This action cannot be undone.`
            : ""
        }
        onConfirm={handleDeleteConfirm}
        onCancel={() => setDeleteConfirm(null)}
      />
    </div>
  );
}
