import { AgentCard } from "./AgentCard";
import { useAgentStore, useFilteredAgents } from "../../stores/agentStore";
import type { AgentDefinition } from "../../lib/types";
import type { Visibility } from "../../lib/types";
import logoIcon from "../../assets/icon.png";

const VISIBILITY_OPTIONS: { value: Visibility | "all"; label: string }[] = [
  { value: "all", label: "All" },
  { value: "public", label: "Public" },
  { value: "private", label: "Private" },
];

interface AgentListProps {
  onOpenDetail: (agent: AgentDefinition) => void;
  onDeploy: (agent: AgentDefinition) => void;
  onEdit?: (agent: AgentDefinition) => void;
  onDelete?: (id: string, name?: string) => void;
  onImport?: () => void;
  importing?: boolean;
}

export function AgentList({
  onOpenDetail,
  onDeploy,
  onEdit,
  onDelete,
  onImport,
  importing,
}: AgentListProps) {
  const { loading, error, agents, filters, setVisibilityFilter } = useAgentStore();
  const filteredAgents = useFilteredAgents();
  const visibilityFilter = filters.visibility;

  if (loading) {
    return (
      <div className="p-6">
        <div className="mb-4 p-4 bg-accent-blue/10 border border-accent-blue/20 rounded-lg animate-pulse h-16" />
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {[...Array(6)].map((_, i) => (
            <div
              key={i}
              className="bg-app-card border border-border rounded-lg h-64 animate-pulse"
            />
          ))}
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-6 text-center">
        <p className="text-accent-red mb-2">Failed to load agents</p>
        <p className="text-sm text-text-secondary">{error}</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="px-6 py-4">
        <div className="p-4 bg-accent-blue/10 border border-accent-blue/20 rounded-lg mb-4">
          <p className="text-sm text-text-primary">
            <span className="font-medium">Agents</span> are{" "}
            <code className="px-1.5 py-0.5 bg-app-bg rounded text-accent-blue font-mono text-xs">
              .md
            </code>{" "}
            files deployed to{" "}
            <code className="px-1.5 py-0.5 bg-app-bg rounded text-accent-blue font-mono text-xs">
              agents/
            </code>{" "}
            — works with both Claude Code and Cursor.
          </p>
        </div>

        <div className="flex items-center justify-between gap-4 flex-wrap">
          <div className="flex items-center gap-1 p-0.5 bg-app-bg rounded-lg border border-border">
            {VISIBILITY_OPTIONS.map((opt) => (
              <button
                key={opt.value}
                type="button"
                onClick={() => setVisibilityFilter(opt.value)}
                className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors ${
                  visibilityFilter === opt.value
                    ? "bg-app-card-hover text-text-primary"
                    : "text-text-muted hover:text-text-secondary"
                }`}
              >
                {opt.label}
              </button>
            ))}
          </div>
          <div className="flex items-center gap-3">
            {onImport && (
              <button
                type="button"
                onClick={onImport}
                disabled={importing}
                data-testid="agents-import"
                className="h-9 px-3 flex items-center gap-1.5 rounded-md bg-app-card border border-border text-sm text-text-primary hover:bg-app-card-hover transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                <span>↧</span>
                <span>{importing ? "Scanning…" : "Import agents"}</span>
              </button>
            )}
            <p className="text-sm text-text-muted">
              Showing {filteredAgents.length} of {agents.length} agents
            </p>
          </div>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-6 pb-6">
        {filteredAgents.length === 0 ? (
          <div className="text-center py-12">
            <div className="w-16 h-16 mx-auto mb-4 opacity-30">
              <img src={logoIcon} alt="AgentHarbor" className="w-full h-full" />
            </div>
            <p className="text-text-muted text-lg mb-2">No agents found</p>
            <p className="text-text-secondary text-sm">
              Create your first agent or adjust your search.
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {filteredAgents.map((agent) => (
              <AgentCard
                key={agent.id}
                agent={agent}
                onClick={onOpenDetail}
                onDeploy={onDeploy}
                onEdit={agent.visibility === "private" ? onEdit : undefined}
                onDelete={agent.visibility === "private" ? onDelete : undefined}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
