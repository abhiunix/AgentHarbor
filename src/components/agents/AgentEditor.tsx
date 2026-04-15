import { useState } from "react";
import type {
  AgentDefinition,
  AgentModel,
  AgentColor,
  MemoryScope,
  ToolAccess,
  UniversalCapability,
} from "../../lib/types";
import { getColorHex, getCapabilityTypeLabel } from "../../lib/types";
import { useRegistryStore } from "../../stores/registryStore";
import { useAgentStore } from "../../stores/agentStore";
import { useSettingsStore } from "../../stores/settingsStore";
import { getAuthorId } from "../../lib/tauri";
import { makeStableCompositeIdWithRetry } from "../../lib/stableId";
import { ConfirmDialog } from "../common/ConfirmDialog";

interface AgentEditorProps {
  agent?: AgentDefinition;
  onSave: (agent: AgentDefinition) => void;
  onDelete?: (id: string) => void;
  onClose: () => void;
}

const modelOptions: { value: AgentModel; label: string }[] = [
  { value: "haiku", label: "Haiku" },
  { value: "sonnet", label: "Sonnet" },
  { value: "opus", label: "Opus" },
];

const memoryOptions: { value: MemoryScope; label: string }[] = [
  { value: "none", label: "None" },
  { value: "project", label: "Project" },
  { value: "user", label: "User" },
];

const colorOptions: AgentColor[] = [
  "red", "blue", "green", "yellow", "purple", "orange", "pink", "cyan"
];

const toolOptions: { value: ToolAccess; label: string }[] = [
  { value: "all", label: "All tools" },
  { value: "read-only", label: "Read-only" },
  { value: "edit", label: "Edit" },
  { value: "execution", label: "Execution" },
  { value: "mcp", label: "MCP" },
  { value: "other", label: "Other" },
];

export function AgentEditor({ agent, onSave, onDelete, onClose }: AgentEditorProps) {
  const { capabilities } = useRegistryStore();
  const agents = useAgentStore((s) => s.agents);
  const { settings } = useSettingsStore();
  const username = settings?.general.username || "user";
  const isEditing = !!agent;
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);

  const [name, setName] = useState(agent?.name || "");
  const [description, setDescription] = useState(agent?.description || "");
  const [model, setModel] = useState<AgentModel>(agent?.model || "sonnet");
  const [memory, setMemory] = useState<MemoryScope>(agent?.memory || "none");
  const [color, setColor] = useState<AgentColor>(agent?.color || "blue");
  const [tools, setTools] = useState<ToolAccess[]>(agent?.tools || ["all"]);
  const [tags, setTags] = useState(agent?.tags.join(", ") || "");
  const [requiredCapabilities, setRequiredCapabilities] = useState<string[]>(
    agent?.required_capabilities || []
  );
  const [prompt, setPrompt] = useState(agent?.prompt || "");
  const [errors, setErrors] = useState<Record<string, string>>({});

  const displayId = agent?.id ?? "Assigned on create";

  const validate = (): boolean => {
    const newErrors: Record<string, string> = {};

    if (!name.trim()) {
      newErrors.name = "Name is required";
    }
    if (!description.trim()) {
      newErrors.description = "Description is required";
    }
    if (!prompt.trim()) {
      newErrors.prompt = "System prompt is required";
    }

    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!validate()) return;

    let id: string;
    if (agent?.id) {
      id = agent.id;
    } else {
      const authorId = await getAuthorId();
      const existingIds = new Set(agents.map((a) => a.id));
      id = await makeStableCompositeIdWithRetry(authorId, existingIds);
    }

    const agentData: AgentDefinition = {
      id,
      name: name.trim(),
      description: description.trim(),
      version: agent?.version || "1.0.0",
      author: username,
      visibility: agent?.visibility || "private",
      tags: tags
        .split(",")
        .map((t) => t.trim())
        .filter(Boolean),
      model,
      color,
      memory,
      tools,
      required_capabilities: requiredCapabilities,
      prompt: prompt.trim(),
      examples: agent?.examples || [],
    };

    onSave(agentData);
  };

  const toggleTool = (tool: ToolAccess) => {
    if (tools.includes(tool)) {
      setTools(tools.filter((t) => t !== tool));
    } else {
      setTools([...tools, tool]);
    }
  };

  const toggleCapability = (capId: string) => {
    if (requiredCapabilities.includes(capId)) {
      setRequiredCapabilities(requiredCapabilities.filter((id) => id !== capId));
    } else {
      setRequiredCapabilities([...requiredCapabilities, capId]);
    }
  };

  return (
    <div className="fixed inset-0 bg-black/60 z-50 flex items-center justify-center p-4">
      <div className="bg-app-sidebar border border-border rounded-lg w-full max-w-2xl max-h-[90vh] flex flex-col">
        <div className="flex items-center justify-between p-4 border-b border-border">
          <h2 className="text-lg font-semibold text-text-primary">
            {isEditing ? "Edit Agent" : "New Agent"}
          </h2>
          <button
            onClick={onClose}
            className="w-8 h-8 flex items-center justify-center rounded-md hover:bg-app-card-hover text-text-muted hover:text-text-primary transition-colors"
          >
            ✕
          </button>
        </div>

        <form onSubmit={handleSubmit} className="flex-1 overflow-y-auto p-6 space-y-6">
          <div>
            <label className="block text-sm font-medium text-text-primary mb-2">
              Agent Name <span className="text-accent-red">*</span>
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="My Agent"
              className={`w-full h-10 px-3 rounded-md bg-app-input border text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-blue ${
                errors.name ? "border-accent-red" : "border-border"
              }`}
            />
            <p className="mt-1 font-mono text-xs text-text-muted">{displayId}</p>
            {errors.name && (
              <p className="mt-1 text-xs text-accent-red">{errors.name}</p>
            )}
          </div>

          <div>
            <label className="block text-sm font-medium text-text-primary mb-2">
              Description <span className="text-accent-red">*</span>
            </label>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="Use this agent when..."
              rows={2}
              className={`w-full px-3 py-2 rounded-md bg-app-input border text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-blue resize-none ${
                errors.description ? "border-accent-red" : "border-border"
              }`}
            />
            {errors.description && (
              <p className="mt-1 text-xs text-accent-red">{errors.description}</p>
            )}
          </div>

          <div className="grid grid-cols-2 gap-4">
            <div>
              <label className="block text-sm font-medium text-text-primary mb-2">
                Model <span className="text-accent-red">*</span>
              </label>
              <select
                value={model}
                onChange={(e) => setModel(e.target.value as AgentModel)}
                className="w-full h-10 px-3 rounded-md bg-app-input border border-border text-text-primary focus:outline-none focus:border-accent-blue"
              >
                {modelOptions.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
            </div>

            <div>
              <label className="block text-sm font-medium text-text-primary mb-2">
                Memory
              </label>
              <select
                value={memory}
                onChange={(e) => setMemory(e.target.value as MemoryScope)}
                className="w-full h-10 px-3 rounded-md bg-app-input border border-border text-text-primary focus:outline-none focus:border-accent-blue"
              >
                {memoryOptions.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div>
            <label className="block text-sm font-medium text-text-primary mb-2">
              Color
            </label>
            <div className="flex items-center gap-2">
              {colorOptions.map((c) => (
                <button
                  key={c}
                  type="button"
                  onClick={() => setColor(c)}
                  className={`w-8 h-8 rounded-full transition-all hover:scale-110 ${
                    color === c
                      ? "ring-2 ring-white ring-offset-2 ring-offset-app-sidebar shadow-lg"
                      : ""
                  }`}
                  style={{ backgroundColor: getColorHex(c) }}
                  title={c}
                />
              ))}
            </div>
          </div>

          <div>
            <label className="block text-sm font-medium text-text-primary mb-2">
              Tools Access
            </label>
            <div className="flex flex-wrap gap-2">
              {toolOptions.map((opt) => (
                <label
                  key={opt.value}
                  className={`flex items-center gap-2 px-3 py-1.5 rounded-md cursor-pointer transition-colors ${
                    tools.includes(opt.value)
                      ? "bg-accent-blue/20 text-accent-blue"
                      : "bg-app-bg text-text-secondary hover:bg-app-card-hover"
                  }`}
                >
                  <input
                    type="checkbox"
                    checked={tools.includes(opt.value)}
                    onChange={() => toggleTool(opt.value)}
                    className="hidden"
                  />
                  <span className="text-sm">{opt.label}</span>
                </label>
              ))}
            </div>
          </div>

          <div>
            <label className="block text-sm font-medium text-text-primary mb-2">
              Tags
            </label>
            <input
              type="text"
              value={tags}
              onChange={(e) => setTags(e.target.value)}
              placeholder="testing, automation, api (comma-separated)"
              className="w-full h-10 px-3 rounded-md bg-app-input border border-border text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-blue"
            />
          </div>

          <div>
            <label className="block text-sm font-medium text-text-primary mb-2">
              Required Capabilities
            </label>
            <div className="max-h-40 overflow-y-auto border border-border rounded-md p-2 space-y-1">
              {capabilities.length === 0 ? (
                <p className="text-sm text-text-muted italic p-2">No capabilities available</p>
              ) : (
                capabilities.map((cap) => (
                  <CapabilityCheckbox
                    key={cap.id}
                    capability={cap}
                    checked={requiredCapabilities.includes(cap.id)}
                    onToggle={() => toggleCapability(cap.id)}
                  />
                ))
              )}
            </div>
          </div>

          <div>
            <label className="block text-sm font-medium text-text-primary mb-2">
              System Prompt <span className="text-accent-red">*</span>
            </label>
            <textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              placeholder="You are a specialized agent that..."
              rows={8}
              className={`w-full px-3 py-2 rounded-md bg-app-input border text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-blue font-mono text-sm resize-none min-h-[200px] ${
                errors.prompt ? "border-accent-red" : "border-border"
              }`}
            />
            {errors.prompt && (
              <p className="mt-1 text-xs text-accent-red">{errors.prompt}</p>
            )}
          </div>
        </form>

        <div className="p-4 border-t border-border space-y-4">
          <div className="p-3 bg-accent-blue/10 border border-accent-blue/20 rounded-md">
            <p className="text-sm text-text-secondary">
              Agent saved as <span className="font-medium text-text-primary">Private</span> (
              <code className="font-mono text-accent-blue">{username}/</code> namespace).
              Deploy to{" "}
              <code className="font-mono text-accent-blue">
                agents/{displayId === "Assigned on create" ? "agent" : displayId.split("/")[1]}.md
              </code>
            </p>
          </div>

          <div className="flex items-center justify-between">
            <div>
              {isEditing && onDelete && (
                <button
                  type="button"
                  onClick={() => setShowDeleteConfirm(true)}
                  className="h-10 px-4 rounded-md text-accent-red hover:bg-accent-red/10 transition-colors"
                >
                  Delete Agent
                </button>
              )}
            </div>
            <div className="flex items-center gap-3">
              <button
                type="button"
                onClick={onClose}
                className="h-10 px-4 rounded-md bg-app-card border border-border text-text-primary hover:bg-app-card-hover transition-colors"
              >
                Cancel
              </button>
              <button
                type="submit"
                onClick={handleSubmit}
                className="h-10 px-6 rounded-md bg-accent-blue text-white font-medium hover:bg-accent-blue/90 transition-colors"
              >
                {isEditing ? "Save Changes" : "Create Agent"}
              </button>
            </div>
          </div>
        </div>
      </div>

      <ConfirmDialog
        isOpen={showDeleteConfirm}
        title="Delete Agent"
        message={
          agent
            ? `Are you sure you want to delete the agent "${agent.name}"? This action cannot be undone.`
            : ""
        }
        onConfirm={() => {
          if (agent && onDelete) {
            onDelete(agent.id);
            setShowDeleteConfirm(false);
          }
        }}
        onCancel={() => setShowDeleteConfirm(false)}
      />
    </div>
  );
}

function CapabilityCheckbox({
  capability,
  checked,
  onToggle,
}: {
  capability: UniversalCapability;
  checked: boolean;
  onToggle: () => void;
}) {
  const typeBgColors: Record<string, string> = {
    mcp: "bg-accent-blue/10 text-accent-blue",
    rule: "bg-accent-green/10 text-accent-green",
    skill: "bg-accent-purple/10 text-accent-purple",
    hook: "bg-accent-orange/10 text-accent-orange",
    plugin: "bg-accent-yellow/10 text-accent-yellow",
  };

  return (
    <label
      className={`flex items-center gap-3 p-2 rounded-md cursor-pointer transition-colors ${
        checked ? "bg-accent-blue/10" : "hover:bg-app-card-hover"
      }`}
    >
      <input
        type="checkbox"
        checked={checked}
        onChange={onToggle}
        className="w-4 h-4 rounded border-border bg-app-input accent-accent-blue"
      />
      <span className={`text-[10px] font-semibold px-1.5 py-0.5 rounded ${typeBgColors[capability.type]}`}>
        {getCapabilityTypeLabel(capability.type)}
      </span>
      <span className="text-sm text-text-primary flex-1 truncate">{capability.name}</span>
    </label>
  );
}
