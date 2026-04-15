import type { AgentDefinition, AgentModel, MemoryScope, ToolAccess, CapabilityType } from "../../lib/types";
import { getColorHex, getModelLabel, getCapabilityTypeLabel } from "../../lib/types";
import { useRegistryStore } from "../../stores/registryStore";

interface AgentCardProps {
  agent: AgentDefinition;
  onDeploy: (agent: AgentDefinition) => void;
  onEdit?: (agent: AgentDefinition) => void;
  onDelete?: (id: string, name?: string) => void;
  onClick: (agent: AgentDefinition) => void;
}

const modelBgColors: Record<AgentModel, string> = {
  haiku: "bg-accent-cyan/20 text-accent-cyan",
  sonnet: "bg-accent-purple/20 text-accent-purple",
  opus: "bg-accent-orange/20 text-accent-orange",
};

const memoryLabels: Record<MemoryScope, string> = {
  project: "Project",
  user: "User",
  none: "None",
};

export function AgentCard({
  agent,
  onDeploy,
  onEdit,
  onDelete,
  onClick,
}: AgentCardProps) {
  const { capabilities } = useRegistryStore();
  const isPrivate = agent.visibility === "private";
  const colorHex = getColorHex(agent.color);

  const requiredCapabilities = agent.required_capabilities
    .map((capId) => capabilities.find((c) => c.id === capId))
    .filter(Boolean);

  return (
    <div
      onClick={() => onClick(agent)}
      className="group relative bg-app-card border border-border rounded-lg overflow-hidden cursor-pointer transition-all hover:bg-app-card-hover"
      style={{ borderTopColor: colorHex, borderTopWidth: "3px" }}
    >
      {isPrivate && (
        <div className="absolute top-3 right-3 z-10 opacity-0 group-hover:opacity-100 transition-opacity flex gap-1">
          {onEdit && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onEdit(agent);
              }}
              className="w-6 h-6 flex items-center justify-center rounded bg-app-bg/80 hover:bg-app-card-hover text-text-secondary hover:text-text-primary transition-colors"
              title="Edit"
            >
              ✎
            </button>
          )}
          {onDelete && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onDelete(agent.id, agent.name);
              }}
              className="w-6 h-6 flex items-center justify-center rounded bg-app-bg/80 hover:bg-accent-red/20 text-text-secondary hover:text-accent-red transition-colors"
              title="Delete"
            >
              ✕
            </button>
          )}
        </div>
      )}

      <div className="p-4">
        <div className="flex items-center gap-2 mb-2">
          <div
            className="w-3 h-3 rounded-full flex-shrink-0"
            style={{ backgroundColor: colorHex }}
          />
          <h3 className="font-medium text-text-primary line-clamp-1 flex-1">
            {agent.name}
          </h3>
          <span
            className={`text-[10px] px-1.5 py-0.5 rounded flex-shrink-0 ${
              isPrivate
                ? "bg-accent-cyan/10 text-accent-cyan"
                : "bg-text-muted/20 text-text-muted"
            }`}
          >
            {agent.visibility}
          </span>
        </div>

        <p className="text-xs font-mono text-text-muted mb-3">{agent.id}</p>

        <div className="flex items-center gap-2 mb-3">
          <span className={`text-[10px] font-semibold px-2 py-1 rounded ${modelBgColors[agent.model]}`}>
            {getModelLabel(agent.model).toUpperCase()}
          </span>
          {agent.memory !== "none" && (
            <span className="text-[10px] px-2 py-1 rounded bg-accent-yellow/20 text-accent-yellow flex items-center gap-1">
              📁 {memoryLabels[agent.memory]}
            </span>
          )}
        </div>

        <p className="text-sm text-text-secondary line-clamp-2 mb-3">
          {agent.description}
        </p>

        <div className="mb-3">
          <p className="text-[10px] text-text-muted uppercase mb-1.5">Required Capabilities</p>
          {requiredCapabilities.length > 0 ? (
            <div className="flex flex-wrap gap-1">
              {requiredCapabilities.slice(0, 3).map((cap) => (
                <CapabilityPill key={cap!.id} type={cap!.type} name={cap!.name} />
              ))}
              {requiredCapabilities.length > 3 && (
                <span className="text-[10px] px-1.5 py-0.5 text-text-muted">
                  +{requiredCapabilities.length - 3} more
                </span>
              )}
            </div>
          ) : (
            <p className="text-[11px] text-text-muted italic">No dependencies</p>
          )}
        </div>

        <div className="flex items-center justify-between pt-3 border-t border-border">
          <div className="flex flex-wrap gap-1">
            {agent.tools.slice(0, 3).map((tool) => (
              <ToolChip key={tool} tool={tool} />
            ))}
            {agent.tools.length > 3 && (
              <span className="text-[10px] text-text-muted">+{agent.tools.length - 3}</span>
            )}
          </div>

          <button
            onClick={(e) => {
              e.stopPropagation();
              onDeploy(agent);
            }}
            className="h-7 px-3 rounded bg-accent-blue text-white text-xs font-medium hover:bg-accent-blue/90 transition-colors"
          >
            Deploy
          </button>
        </div>
      </div>
    </div>
  );
}

function CapabilityPill({ type, name }: { type: CapabilityType; name: string }) {
  const typeBgColors: Record<CapabilityType, string> = {
    mcp: "bg-accent-blue/10 text-accent-blue",
    rule: "bg-accent-green/10 text-accent-green",
    skill: "bg-accent-purple/10 text-accent-purple",
    hook: "bg-accent-orange/10 text-accent-orange",
    plugin: "bg-accent-yellow/10 text-accent-yellow",
    custom: "bg-teal-400/10 text-teal-400",
  };

  return (
    <span className={`text-[10px] px-1.5 py-0.5 rounded flex items-center gap-1 ${typeBgColors[type]}`}>
      <span className="font-semibold">[{getCapabilityTypeLabel(type)}]</span>
      <span className="text-text-secondary truncate max-w-[80px]">{name}</span>
    </span>
  );
}

function ToolChip({ tool }: { tool: ToolAccess }) {
  const toolLabels: Record<ToolAccess, string> = {
    all: "all",
    "read-only": "read-only",
    edit: "edit",
    execution: "exec",
    mcp: "mcp",
    other: "other",
  };

  return (
    <span className="text-[9px] px-1.5 py-0.5 rounded bg-app-bg text-text-muted uppercase">
      {toolLabels[tool]}
    </span>
  );
}
