import { useState } from "react";
import type { AgentDefinition, CapabilityType } from "../../lib/types";
import { getColorHex, getModelLabel, getModelBadgeClass, getCapabilityTypeLabel } from "../../lib/types";
import { useRegistryStore } from "../../stores/registryStore";

interface AgentDetailProps {
  agent: AgentDefinition;
  onClose: () => void;
  onDeploy: (agent: AgentDefinition) => void;
  onEdit?: (agent: AgentDefinition) => void;
}

type TabId = "overview" | "prompt" | "preview";

const tabs: { id: TabId; label: string }[] = [
  { id: "overview", label: "Overview" },
  { id: "prompt", label: "System Prompt" },
  { id: "preview", label: "File Preview" },
];

export function AgentDetail({ agent, onClose, onDeploy, onEdit }: AgentDetailProps) {
  const [activeTab, setActiveTab] = useState<TabId>("overview");
  const colorHex = getColorHex(agent.color);
  const isPrivate = agent.visibility === "private";

  return (
    <div
      className="fixed inset-y-0 right-0 w-[520px] bg-app-sidebar border-l border-border shadow-2xl z-50 flex flex-col"
      style={{ borderTopColor: colorHex, borderTopWidth: "3px" }}
    >
      <div className="flex items-center justify-between p-4 border-b border-border">
        <div className="flex items-center gap-3">
          <div
            className="w-4 h-4 rounded-full"
            style={{ backgroundColor: colorHex }}
          />
          <h2 className="font-semibold text-text-primary">{agent.name}</h2>
          <span
            className={`text-xs px-2 py-0.5 rounded ${
              isPrivate
                ? "bg-accent-cyan/10 text-accent-cyan"
                : "bg-text-muted/20 text-text-muted"
            }`}
          >
            {agent.visibility}
          </span>
        </div>
        <button
          onClick={onClose}
          className="w-8 h-8 flex items-center justify-center rounded-md hover:bg-app-card-hover text-text-muted hover:text-text-primary transition-colors"
        >
          ✕
        </button>
      </div>

      <div className="flex border-b border-border">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`flex-1 px-4 py-3 text-sm font-medium transition-colors ${
              activeTab === tab.id
                ? "text-accent-blue border-b-2 border-accent-blue"
                : "text-text-secondary hover:text-text-primary"
            }`}
          >
            {tab.label}
          </button>
        ))}
      </div>

      <div className="flex-1 overflow-y-auto">
        {activeTab === "overview" && <OverviewTab agent={agent} />}
        {activeTab === "prompt" && <PromptTab agent={agent} />}
        {activeTab === "preview" && <PreviewTab agent={agent} />}
      </div>

      <div className="p-4 border-t border-border flex items-center gap-3">
        <button
          onClick={() => onDeploy(agent)}
          className="flex-1 h-10 rounded-md bg-accent-blue text-white text-sm font-medium hover:bg-accent-blue/90 transition-colors"
        >
          Deploy Agent
        </button>
        {isPrivate && onEdit && (
          <button
            onClick={() => onEdit(agent)}
            className="h-10 px-4 rounded-md bg-app-card border border-border text-sm text-text-primary hover:bg-app-card-hover transition-colors"
          >
            Edit
          </button>
        )}
      </div>
    </div>
  );
}

function OverviewTab({ agent }: { agent: AgentDefinition }) {
  const { capabilities } = useRegistryStore();

  const requiredCapabilities = agent.required_capabilities
    .map((capId) => capabilities.find((c) => c.id === capId))
    .filter(Boolean);

  const memoryLabels = { project: "Project", user: "User", none: "None" };
  const modelLabel = getModelLabel(agent.model);

  return (
    <div className="p-6 space-y-6">
      <div>
        <p className="font-mono text-sm text-text-muted mb-2">{agent.id}</p>
        <p className="text-text-secondary">{agent.description}</p>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <ConfigCard label="Model">
          {modelLabel ? (
            <span className={`text-xs font-semibold px-2 py-1 rounded ${getModelBadgeClass(agent.model)}`}>
              {modelLabel.toUpperCase()}
            </span>
          ) : (
            <span className="text-sm text-text-muted italic">Adapter default</span>
          )}
        </ConfigCard>

        <ConfigCard label="Color">
          <div className="flex items-center gap-2">
            <div
              className="w-5 h-5 rounded-full border border-white/20"
              style={{ backgroundColor: getColorHex(agent.color) }}
            />
            <span className="text-sm text-text-primary capitalize">{agent.color}</span>
          </div>
        </ConfigCard>

        <ConfigCard label="Memory">
          <span className="text-sm text-text-primary">{memoryLabels[agent.memory]}</span>
        </ConfigCard>

        <ConfigCard label="Tools">
          <div className="flex flex-wrap gap-1">
            {agent.tools.map((tool) => (
              <span
                key={tool}
                className="text-[10px] px-1.5 py-0.5 rounded bg-app-bg text-text-muted uppercase"
              >
                {tool}
              </span>
            ))}
          </div>
        </ConfigCard>
      </div>

      <div>
        <p className="text-xs text-text-muted uppercase mb-3">Required Capabilities</p>
        {requiredCapabilities.length > 0 ? (
          <div className="space-y-2">
            {requiredCapabilities.map((cap) => (
              <RequiredCapabilityItem key={cap!.id} type={cap!.type} name={cap!.name} id={cap!.id} />
            ))}
          </div>
        ) : (
          <p className="text-sm text-text-muted italic">No required capabilities</p>
        )}
      </div>

      {agent.examples && agent.examples.length > 0 && (
        <div>
          <p className="text-xs text-text-muted uppercase mb-3">Usage Examples</p>
          <div className="space-y-3">
            {agent.examples.map((example, i) => (
              <div key={i} className="p-3 bg-app-bg rounded-md space-y-2">
                <div>
                  <p className="text-[10px] text-text-muted uppercase mb-1">User</p>
                  <p className="text-sm text-text-primary">{example.user}</p>
                </div>
                <div>
                  <p className="text-[10px] text-text-muted uppercase mb-1">Agent</p>
                  <p className="text-sm text-text-secondary">{example.agent}</p>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {agent.tags.length > 0 && (
        <div>
          <p className="text-xs text-text-muted uppercase mb-2">Tags</p>
          <div className="flex flex-wrap gap-2">
            {agent.tags.map((tag) => (
              <span
                key={tag}
                className="px-2 py-1 bg-app-bg text-text-secondary text-sm rounded-md"
              >
                {tag}
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function PromptTab({ agent }: { agent: AgentDefinition }) {
  return (
    <div className="p-6">
      <pre className="p-4 bg-app-bg rounded-lg font-mono text-sm text-text-secondary whitespace-pre-wrap overflow-x-auto max-h-[calc(100vh-320px)]">
        {agent.prompt}
      </pre>
    </div>
  );
}

function PreviewTab({ agent }: { agent: AgentDefinition }) {
  const generateMarkdown = () => {
    const lines: string[] = [];
    lines.push("---");
    lines.push(`name: ${agent.name}`);
    lines.push(`description: "${agent.description}"`);
    if (agent.model && agent.model.trim()) {
      lines.push(`model: ${agent.model.trim()}`);
    }
    if (agent.color !== "blue") {
      lines.push(`color: ${agent.color}`);
    }
    if (agent.memory !== "none") {
      lines.push(`memory: ${agent.memory}`);
    }
    lines.push("---");
    lines.push("");
    lines.push(agent.prompt);
    return lines.join("\n");
  };

  const markdown = generateMarkdown();

  const handleCopy = () => {
    navigator.clipboard.writeText(markdown);
  };

  return (
    <div className="p-6">
      <div className="flex items-center justify-between mb-4">
        <p className="text-sm text-text-muted">
          <code className="px-1.5 py-0.5 bg-app-bg rounded font-mono text-accent-blue">
            agents/{agent.artifact ?? agent.id.split("/")[1]}.md
          </code>
        </p>
        <button
          onClick={handleCopy}
          className="text-xs text-accent-blue hover:underline"
        >
          Copy to clipboard
        </button>
      </div>
      <pre className="p-4 bg-app-bg rounded-lg font-mono text-sm text-text-secondary whitespace-pre-wrap overflow-x-auto max-h-[calc(100vh-360px)]">
        {markdown}
      </pre>
    </div>
  );
}

function ConfigCard({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="p-3 bg-app-bg rounded-md">
      <p className="text-[10px] text-text-muted uppercase mb-2">{label}</p>
      {children}
    </div>
  );
}

function RequiredCapabilityItem({
  type,
  name,
  id,
}: {
  type: CapabilityType;
  name: string;
  id: string;
}) {
  const typeBgColors: Record<CapabilityType, string> = {
    mcp: "bg-accent-blue/10 text-accent-blue",
    rule: "bg-accent-green/10 text-accent-green",
    skill: "bg-accent-purple/10 text-accent-purple",
    hook: "bg-accent-orange/10 text-accent-orange",
    plugin: "bg-accent-yellow/10 text-accent-yellow",
    custom: "bg-teal-400/10 text-teal-400",
  };

  return (
    <div className="flex items-center gap-3 p-3 bg-app-bg rounded-md">
      <span className={`text-[10px] font-semibold px-1.5 py-0.5 rounded ${typeBgColors[type]}`}>
        {getCapabilityTypeLabel(type)}
      </span>
      <div className="flex-1 min-w-0">
        <p className="text-sm text-text-primary truncate">{name}</p>
        <p className="text-xs font-mono text-text-muted truncate">{id}</p>
      </div>
    </div>
  );
}
