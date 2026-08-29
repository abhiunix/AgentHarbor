import { useState } from "react";
import { useFilteredCapabilities } from "../../stores/registryStore";
import { useFilteredAgents } from "../../stores/agentStore";
import type { UniversalCapability, AgentDefinition } from "../../lib/types";
import type { ClaudeSettingsTarget } from "../../lib/tauri";
import { getAdapterIconImg } from "../../lib/adapterPlugins";

interface AdapterSelectorProps {
  selectedAdapterIds: string[];
  onAdapterChange: (ids: string[]) => void;
  claudeSettingsTargets?: Set<ClaudeSettingsTarget>;
  onClaudeSettingsTargetsChange?: (v: Set<ClaudeSettingsTarget>) => void;
  initialCapabilityIds: string[];
  initialAgentIds: string[];
  onComplete: (capabilityIds: string[], agentIds: string[]) => void;
  onBack?: () => void;
  loading: boolean;
  isGlobalDeploy?: boolean;
}

interface AdapterInfo {
  id: string;
  name: string;
  color: string;
  supportsAgents: boolean;
  supportsMcp: boolean;
  supportsRules: boolean;
  supportsSkills: boolean;
}

const ADAPTERS: AdapterInfo[] = [
  { id: "claude-code", name: "Claude Code", color: "#9333ea", supportsAgents: true, supportsMcp: true, supportsRules: true, supportsSkills: true },
  { id: "cursor", name: "Cursor", color: "#3b82f6", supportsAgents: true, supportsMcp: true, supportsRules: true, supportsSkills: true },
  { id: "windsurf", name: "Windsurf", color: "#22c55e", supportsAgents: false, supportsMcp: true, supportsRules: true, supportsSkills: false },
  { id: "gemini", name: "Gemini CLI", color: "#4285f4", supportsAgents: true, supportsMcp: true, supportsRules: true, supportsSkills: true },
  { id: "codex", name: "Codex", color: "#10a37f", supportsAgents: false, supportsMcp: false, supportsRules: false, supportsSkills: true },
  { id: "opencode", name: "OpenCode", color: "#f5a623", supportsAgents: true, supportsMcp: true, supportsRules: true, supportsSkills: true },
];

const CLAUDE_TARGETS: { value: ClaudeSettingsTarget; label: string; sub: string }[] = [
  { value: "local", label: "Project settings (local)", sub: ".claude/settings.local.json" },
  { value: "project", label: "Project settings", sub: ".claude/settings.json" },
];

export function AdapterSelector({
  selectedAdapterIds,
  onAdapterChange,
  claudeSettingsTargets = new Set(["local"]),
  onClaudeSettingsTargetsChange,
  initialCapabilityIds,
  initialAgentIds,
  onComplete,
  onBack,
  loading,
  isGlobalDeploy = false,
}: AdapterSelectorProps) {
  const [selectedCaps, setSelectedCaps] = useState<Set<string>>(new Set(initialCapabilityIds));
  const [selectedAgents, setSelectedAgents] = useState<Set<string>>(new Set(initialAgentIds));

  const capabilities = useFilteredCapabilities();
  const agents = useFilteredAgents();

  const toggleAdapter = (id: string) => {
    if (selectedAdapterIds.includes(id)) {
      if (selectedAdapterIds.length > 1) {
        onAdapterChange(selectedAdapterIds.filter((a) => a !== id));
      }
    } else {
      onAdapterChange([...selectedAdapterIds, id]);
    }
  };

  // When initial selections are provided, only show those items (no browsing)
  const hasInitialSelection = initialCapabilityIds.length > 0 || initialAgentIds.length > 0;

  const compatibleCapabilities = capabilities.filter((c) => {
    // Check explicit adapter compatibility OR type-based compatibility
    const adapterCompatible = selectedAdapterIds.some((adapterId) => {
      // Explicit match: capability lists this adapter in compatible_agents
      if (c.compatible_agents.includes(adapterId)) return true;
      // Type-based match: the adapter supports this capability type
      const adapterInfo = ADAPTERS.find((a) => a.id === adapterId);
      if (!adapterInfo) return false;
      const typeMap: Record<string, keyof AdapterInfo> = {
        mcp: "supportsMcp",
        rule: "supportsRules",
        skill: "supportsSkills",
        hook: "supportsSkills", // hooks deploy to skill-supporting adapters
        plugin: "supportsMcp",
      };
      const key = typeMap[c.type];
      return key ? Boolean(adapterInfo[key]) : false;
    });
    if (!adapterCompatible) return false;
    if (hasInitialSelection) return selectedCaps.has(c.id);
    return true;
  });

  const selectedAdaptersInfo = ADAPTERS.filter((a) => selectedAdapterIds.includes(a.id));
  const anyAdapterSupportsAgents = selectedAdaptersInfo.some((a) => a.supportsAgents);
  const agentsNote = selectedAdapterIds.includes("windsurf") && !selectedAdapterIds.includes("claude-code") && !selectedAdapterIds.includes("cursor")
    ? "Windsurf does not support agents"
    : selectedAdapterIds.includes("windsurf") && (selectedAdapterIds.includes("claude-code") || selectedAdapterIds.includes("cursor"))
    ? "Agents will deploy to Claude Code/Cursor only (Windsurf not supported)"
    : null;

  const toggleCapability = (id: string) => {
    setSelectedCaps((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const toggleAgent = (id: string) => {
    setSelectedAgents((prev) => {
      const next = new Set(prev);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };

  const totalSelected = selectedCaps.size + selectedAgents.size;

  const showAgentsSection = !isGlobalDeploy && !(hasInitialSelection && initialAgentIds.length === 0);
  const filteredAgents = hasInitialSelection ? agents.filter((a) => selectedAgents.has(a.id)) : agents;

  return (
    <div className="flex flex-col flex-1 min-h-0">
      <div className="flex-1 overflow-y-auto min-h-0 p-6 space-y-6">
        {isGlobalDeploy && (
          <div className="flex items-center gap-3 p-3 bg-accent-blue/10 border border-accent-blue/30 rounded-lg">
            <span className="text-lg">🌐</span>
            <div>
              <p className="text-sm font-medium text-accent-blue">Global Deploy Mode</p>
              <p className="text-xs text-text-muted">Capabilities will be written to your system-wide IDE configs. Skills deploy to your global skills folder. Agents, hooks, and plugins are project-scoped and excluded.</p>
            </div>
          </div>
        )}

        <div>
          <p className="text-xs text-text-muted uppercase mb-3">Select Adapters (multiple allowed)</p>
          <div className="flex gap-3">
            {ADAPTERS.map((adapter) => {
              const isSelected = selectedAdapterIds.includes(adapter.id);
              return (
                <button
                  key={adapter.id}
                  onClick={() => toggleAdapter(adapter.id)}
                  data-testid={`adapter-${adapter.id}`}
                  className={`flex-1 p-4 rounded-lg border transition-all ${
                    isSelected
                      ? "border-accent-blue bg-accent-blue/10"
                      : "border-border hover:border-white/30 bg-app-card"
                  }`}
                >
                  <div className="flex items-center gap-2 mb-2">
                    {getAdapterIconImg(adapter.id) ? (
                      <img src={getAdapterIconImg(adapter.id)} alt="" className="w-5 h-5 object-contain" />
                    ) : (
                      <div
                        className="w-3 h-3 rounded-full"
                        style={{ backgroundColor: adapter.color }}
                      />
                    )}
                    {isSelected && (
                      <span className="text-accent-blue text-xs">✓</span>
                    )}
                  </div>
                  <p className="font-medium text-text-primary">{adapter.name}</p>
                  <div className="flex flex-wrap gap-1 mt-2">
                    {adapter.supportsMcp && (
                      <span className="text-[9px] px-1.5 py-0.5 rounded bg-blue-500/20 text-blue-400">MCP</span>
                    )}
                    {adapter.supportsRules && (
                      <span className="text-[9px] px-1.5 py-0.5 rounded bg-green-500/20 text-green-400">Rules</span>
                    )}
                    {adapter.supportsSkills && (
                      <span className="text-[9px] px-1.5 py-0.5 rounded bg-yellow-500/20 text-yellow-400">Skills</span>
                    )}
                    {adapter.supportsAgents && (
                      <span className="text-[9px] px-1.5 py-0.5 rounded bg-purple-500/20 text-purple-400">Agents</span>
                    )}
                  </div>
                </button>
              );
            })}
          </div>
        </div>

        {selectedAdapterIds.includes("claude-code") && !isGlobalDeploy && onClaudeSettingsTargetsChange && (
          <div className="flex items-center gap-4">
            <p className="text-xs text-text-muted uppercase whitespace-nowrap">Claude Scope:</p>
            {CLAUDE_TARGETS.map((opt) => (
              <label
                key={opt.value}
                className="flex items-center gap-1.5 cursor-pointer"
              >
                <input
                  type="checkbox"
                  checked={claudeSettingsTargets.has(opt.value)}
                  onChange={() => {
                    const next = new Set(claudeSettingsTargets);
                    if (next.has(opt.value)) {
                      if (next.size > 1) next.delete(opt.value); // keep at least one
                    } else {
                      next.add(opt.value);
                    }
                    onClaudeSettingsTargetsChange(next);
                  }}
                  className="w-3.5 h-3.5 rounded accent-accent-blue"
                />
                <span className={`text-sm ${claudeSettingsTargets.has(opt.value) ? "text-text-primary" : "text-text-secondary"}`}>
                  {opt.label}
                </span>
                <span className="text-[10px] text-text-muted font-mono">{opt.sub}</span>
              </label>
            ))}
          </div>
        )}

        {selectedAdapterIds.includes("cursor") && (() => {
          const selectedMcps = compatibleCapabilities.filter(
            (c) => c.type === "mcp" && selectedCaps.has(c.id) && (c as import("../../lib/types").McpServer).disabled_tools?.length
          );
          return selectedMcps.length > 0 ? (
            <div className="flex items-start gap-2 px-3 py-2 bg-yellow-500/10 border border-yellow-500/20 rounded-lg text-xs text-yellow-400">
              <span className="mt-0.5">!</span>
              <span>Tool filtering is not supported for Cursor. Disabled tools will still appear in Cursor's UI.</span>
            </div>
          ) : null;
        })()}

        <div>
          <p className="text-xs text-text-muted uppercase mb-3">
            Capabilities ({compatibleCapabilities.length} {hasInitialSelection ? "selected" : "compatible"})
          </p>
          <div className="space-y-2">
            {compatibleCapabilities.map((cap) => (
              <CapabilityRow
                key={cap.id}
                capability={cap}
                selected={selectedCaps.has(cap.id)}
                onToggle={hasInitialSelection ? undefined : () => toggleCapability(cap.id)}
                selectedAdapters={selectedAdapterIds}
              />
            ))}
            {compatibleCapabilities.length === 0 && (
              <p className="text-text-muted text-sm py-4 text-center">
                No capabilities compatible with selected adapters.
              </p>
            )}
          </div>
        </div>

        {showAgentsSection && (
          <div>
            <div className="flex items-center justify-between mb-3">
              <p className="text-xs text-text-muted uppercase">
                Agents ({filteredAgents.length} {hasInitialSelection ? "selected" : "available"})
              </p>
              {agentsNote && (
                <p className="text-xs text-accent-orange">{agentsNote}</p>
              )}
            </div>
            <div className="space-y-2">
              {anyAdapterSupportsAgents ? (
                filteredAgents.map((agent) => (
                  <AgentRow
                    key={agent.id}
                    agent={agent}
                    selected={selectedAgents.has(agent.id)}
                    onToggle={hasInitialSelection ? undefined : () => toggleAgent(agent.id)}
                    selectedAdapters={selectedAdapterIds}
                  />
                ))
              ) : (
                <p className="text-text-muted text-sm py-4 text-center">
                  Selected adapters do not support agents.
                </p>
              )}
              {anyAdapterSupportsAgents && filteredAgents.length === 0 && (
                <p className="text-text-muted text-sm py-4 text-center">
                  No agents available.
                </p>
              )}
            </div>
          </div>
        )}
      </div>

      <div className="flex items-center justify-between px-6 py-4 border-t border-border flex-shrink-0">
        {onBack ? (
          <button
            onClick={onBack}
            data-testid="wizard-back"
            className="px-4 py-2 text-sm text-text-muted hover:text-text-primary transition-colors"
          >
            ← Back
          </button>
        ) : (
          <div />
        )}
        <div className="flex items-center gap-4">
          <span className="text-sm text-text-muted">
            {isGlobalDeploy
              ? `${selectedCaps.size} capability${selectedCaps.size !== 1 ? "ies" : "y"} to ${selectedAdapterIds.length} adapter${selectedAdapterIds.length !== 1 ? "s" : ""} (global)`
              : `${totalSelected} item${totalSelected !== 1 ? "s" : ""} to ${selectedAdapterIds.length} adapter${selectedAdapterIds.length !== 1 ? "s" : ""}`}
          </span>
          <button
            onClick={() => onComplete(Array.from(selectedCaps), Array.from(isGlobalDeploy ? new Set<string>() : selectedAgents))}
            disabled={(isGlobalDeploy ? selectedCaps.size === 0 : totalSelected === 0) || loading}
            data-testid="wizard-next"
            className="px-6 py-2 bg-accent-blue text-white rounded-lg font-medium hover:bg-accent-blue/80 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {loading ? "Loading..." : "Preview Changes"}
          </button>
        </div>
      </div>
    </div>
  );
}

function CapabilityRow({
  capability,
  selected,
  onToggle,
  selectedAdapters,
}: {
  capability: UniversalCapability;
  selected: boolean;
  onToggle?: () => void;
  selectedAdapters: string[];
}) {
  const typeColors: Record<string, string> = {
    mcp: "#3b82f6",
    rule: "#22c55e",
    skill: "#f59e0b",
    hook: "#ec4899",
    plugin: "#8b5cf6",
  };

  const compatibleWith = selectedAdapters.filter((a) => capability.compatible_agents.includes(a));

  return (
    <div
      onClick={onToggle}
      role={onToggle ? "button" : undefined}
      data-testid={`deploy-cap-${capability.id}`}
      data-cap-type={capability.type}
      data-cap-name={capability.name}
      className={`w-full flex items-center gap-3 p-3 rounded-lg border transition-all ${
        selected
          ? "border-accent-blue bg-accent-blue/10"
          : "border-border hover:border-white/30 bg-app-card"
      } ${onToggle ? "cursor-pointer" : ""}`}
    >
      {onToggle && (
        <div
          className={`w-4 h-4 rounded border flex items-center justify-center ${
            selected ? "bg-accent-blue border-accent-blue" : "border-text-muted"
          }`}
        >
          {selected && <span className="text-white text-xs">✓</span>}
        </div>
      )}
      <div
        className="w-2 h-2 rounded-full"
        style={{ backgroundColor: typeColors[capability.type] || "#666" }}
      />
      <div className="flex-1 text-left">
        <p className="text-sm font-medium text-text-primary">{capability.name}</p>
        <p className="text-xs text-text-muted">{capability.id}</p>
      </div>
      <div className="flex gap-1">
        {compatibleWith.map((adapter) => {
          const adapterInfo = ADAPTERS.find((a) => a.id === adapter);
          return (
            <div
              key={adapter}
              className="w-2 h-2 rounded-full"
              style={{ backgroundColor: adapterInfo?.color || "#666" }}
              title={adapterInfo?.name}
            />
          );
        })}
      </div>
      <span className="text-[10px] uppercase px-2 py-0.5 rounded bg-white/5 text-text-muted">
        {capability.type}
      </span>
    </div>
  );
}

function AgentRow({
  agent,
  selected,
  onToggle,
  selectedAdapters,
}: {
  agent: AgentDefinition;
  selected: boolean;
  onToggle?: () => void;
  selectedAdapters: string[];
}) {
  const colorMap: Record<string, string> = {
    red: "#ef4444",
    blue: "#3b82f6",
    green: "#22c55e",
    yellow: "#eab308",
    purple: "#9333ea",
    orange: "#f97316",
    pink: "#ec4899",
    cyan: "#06b6d4",
  };

  const agentSupportingAdapters = selectedAdapters.filter((a) => 
    ADAPTERS.find((ad) => ad.id === a)?.supportsAgents
  );

  return (
    <div
      onClick={onToggle}
      role={onToggle ? "button" : undefined}
      data-testid={`deploy-agent-${agent.id}`}
      data-agent-name={agent.name}
      data-agent-model={agent.model}
      className={`w-full flex items-center gap-3 p-3 rounded-lg border transition-all ${
        selected
          ? "border-accent-blue bg-accent-blue/10"
          : "border-border hover:border-white/30 bg-app-card"
      } ${onToggle ? "cursor-pointer" : ""}`}
    >
      {onToggle && (
        <div
          className={`w-4 h-4 rounded border flex items-center justify-center ${
            selected ? "bg-accent-blue border-accent-blue" : "border-text-muted"
          }`}
        >
          {selected && <span className="text-white text-xs">✓</span>}
        </div>
      )}
      <div
        className="w-2 h-2 rounded-full"
        style={{ backgroundColor: colorMap[agent.color] || "#3b82f6" }}
      />
      <div className="flex-1 text-left">
        <p className="text-sm font-medium text-text-primary">{agent.name}</p>
        <p className="text-xs text-text-muted">{agent.id}</p>
      </div>
      <div className="flex gap-1">
        {agentSupportingAdapters.map((adapter) => {
          const adapterInfo = ADAPTERS.find((a) => a.id === adapter);
          return (
            <div
              key={adapter}
              className="w-2 h-2 rounded-full"
              style={{ backgroundColor: adapterInfo?.color || "#666" }}
              title={`${adapterInfo?.name} (shared agents/)`}
            />
          );
        })}
      </div>
      <span className="text-[10px] uppercase px-2 py-0.5 rounded bg-white/5 text-text-muted">
        {agent.model}
      </span>
    </div>
  );
}
