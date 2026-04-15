import { useCallback, useState } from "react";
import { useAgentStore } from "../stores/agentStore";
import { AgentList } from "../components/agents/AgentList";
import { AgentDetail } from "../components/agents/AgentDetail";
import { AgentEditor } from "../components/agents/AgentEditor";
import { AgentDeployWizard } from "../components/deploy/AgentDeployWizard";
import { ConfirmDialog } from "../components/common/ConfirmDialog";
import { saveAgent as saveAgentApi, deleteAgent as deleteAgentApi } from "../lib/tauri";
import type { AgentDefinition } from "../lib/types";

export function AgentsPage() {
  const {
    detailAgent,
    setDetailAgent,
    loadAgents,
    editorAgent,
    showEditor,
    openEditor,
    closeEditor,
  } = useAgentStore();

  const [deployAgent, setDeployAgent] = useState<AgentDefinition | null>(null);
  const [deleteConfirm, setDeleteConfirm] = useState<{ id: string; name: string } | null>(null);

  const handleOpenDetail = useCallback(
    (agent: AgentDefinition) => {
      setDetailAgent(agent);
    },
    [setDetailAgent]
  );

  const handleCloseDetail = useCallback(() => {
    setDetailAgent(null);
  }, [setDetailAgent]);

  const handleDeploy = useCallback((agent: AgentDefinition) => {
    setDeployAgent(agent);
    setDetailAgent(null);
  }, [setDetailAgent]);

  const handleEdit = useCallback(
    (agent: AgentDefinition) => {
      openEditor(agent);
    },
    [openEditor]
  );

  const handleDeleteClick = useCallback((id: string, name?: string) => {
    setDeleteConfirm({ id, name: name ?? id });
  }, []);

  const handleDeleteConfirm = useCallback(
    async () => {
      if (!deleteConfirm) return;
      try {
        await deleteAgentApi(deleteConfirm.id);
        await loadAgents();
        setDetailAgent(null);
      } catch (error) {
        console.error("Failed to delete agent:", error);
      } finally {
        setDeleteConfirm(null);
      }
    },
    [deleteConfirm, loadAgents, setDetailAgent]
  );

  const handleSaveAgent = useCallback(
    async (agent: AgentDefinition) => {
      try {
        await saveAgentApi(agent);
        await loadAgents();
        closeEditor();
      } catch (error) {
        console.error("Failed to save agent:", error);
      }
    },
    [loadAgents, closeEditor]
  );

  const handleDeleteFromEditor = useCallback(
    async (id: string) => {
      try {
        await deleteAgentApi(id);
        await loadAgents();
        closeEditor();
      } catch (error) {
        console.error("Failed to delete agent:", error);
      }
    },
    [loadAgents, closeEditor]
  );

  return (
    <div className="h-full relative">
      <AgentList
        onOpenDetail={handleOpenDetail}
        onDeploy={handleDeploy}
        onEdit={handleEdit}
        onDelete={handleDeleteClick}
      />

      {detailAgent && (
        <>
          <div
            className="fixed inset-0 bg-black/40 z-40"
            onClick={handleCloseDetail}
          />
          <AgentDetail
            agent={detailAgent}
            onClose={handleCloseDetail}
            onDeploy={handleDeploy}
            onEdit={detailAgent.visibility === "private" ? handleEdit : undefined}
          />
        </>
      )}

      {showEditor && (
        <AgentEditor
          agent={editorAgent || undefined}
          onSave={handleSaveAgent}
          onDelete={editorAgent ? handleDeleteFromEditor : undefined}
          onClose={closeEditor}
        />
      )}

      {deployAgent && (
        <AgentDeployWizard
          isOpen={true}
          onClose={() => setDeployAgent(null)}
          agent={deployAgent}
        />
      )}

      <ConfirmDialog
        isOpen={!!deleteConfirm}
        title="Delete Agent"
        message={
          deleteConfirm
            ? `Are you sure you want to delete the agent "${deleteConfirm.name}"? This action cannot be undone.`
            : ""
        }
        onConfirm={handleDeleteConfirm}
        onCancel={() => setDeleteConfirm(null)}
      />
    </div>
  );
}
