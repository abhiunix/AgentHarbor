export type CompositeId = string;

export type Visibility = "public" | "private" | "discovered";

export type CapabilityType = "mcp" | "rule" | "skill" | "hook" | "plugin" | "custom";

export type AgentModel = "haiku" | "sonnet" | "opus";

export type AgentColor = "red" | "blue" | "green" | "yellow" | "purple" | "orange" | "pink" | "cyan";

export type MemoryScope = "project" | "user" | "none";

export type ToolAccess = "all" | "read-only" | "edit" | "execution" | "mcp" | "other";

export interface EnvVariable {
  type: string;
  label: string;
  required: boolean;
  /** Value to write to .env on deploy (for string type; use Secrets Manager for secret type). */
  value?: string;
}

export interface SkillFile {
  path: string;
  content: string;
}

export interface CapabilitySource {
  repo?: string;
  url?: string;
  path?: string;
  branch?: string;
}

export interface CapabilityStats {
  github_stars?: number;
  updated_at?: string;
}

export interface CapabilityMetadata {
  id: CompositeId;
  name: string;
  description: string;
  version: string;
  author: string;
  visibility: Visibility;
  tags: string[];
  compatible_agents: string[];
  /** Set for discovered capabilities (source label). */
  source?: string;
  /** Category within its type (e.g. "development", "devtools") */
  category?: string;
  /** GitHub username of the author */
  author_github?: string;
  /** Source repository info */
  source_info?: CapabilitySource;
  /** GitHub stars and update info */
  stats?: CapabilityStats;
  /** License */
  license?: string;
}

export interface McpTool {
  name: string;
  description: string;
}

export interface McpServer extends CapabilityMetadata {
  type: "mcp";
  transport: string;
  command: string;
  args: string[];
  url: string;
  env: Record<string, EnvVariable>;
  /** Windsurf: disable this server entirely */
  disabled?: boolean;
  /** Tools to auto-approve (Windsurf: alwaysAllow, Claude Code: allowedTools) */
  always_allow?: string[];
  /** Tools to deny/disable (Windsurf: disabledTools, Claude Code: disallowedTools) */
  disabled_tools?: string[];
  /** Cached tool list from MCP tools/list discovery */
  tool_list?: McpTool[];
}

export interface Rule extends CapabilityMetadata {
  type: "rule";
  scope: string;
  content: string;
  env?: Record<string, EnvVariable>;
}

export interface Skill extends CapabilityMetadata {
  type: "skill";
  /** @deprecated scope is now a deployment decision, kept for backward compat */
  scope?: string;
  files: SkillFile[];
  env?: Record<string, EnvVariable>;
  /** Tools the skill can use without permission (e.g. ["Read", "Glob", "Grep"]) */
  allowed_tools?: string[];
  /** Model override when skill is active (free text: "sonnet", "opus", any model ID) */
  model?: string;
  /** "fork" to run in subagent context */
  context?: string;
  /** Subagent type when context="fork" */
  agent?: string;
  /** Autocomplete hint for slash command (e.g. "[file-path]") */
  argument_hint?: string;
  /** License name (e.g. "MIT", "Apache-2.0") */
  license?: string;
  /** Discovered skills only: deployed by AgentHarbor (hash-suffixed dir + our frontmatter). */
  managed?: boolean;
}

export interface Hook extends CapabilityMetadata {
  type: "hook";
  event: string;
  matcher: string;
  command: string;
  timeout_ms: number;
  env?: Record<string, EnvVariable>;
  adapter_configs?: Record<string, unknown>;
}

export interface Plugin extends CapabilityMetadata {
  type: "plugin";
  install_command: string;
  config: Record<string, unknown>;
  env?: Record<string, EnvVariable>;
}

export interface Custom extends CapabilityMetadata {
  type: "custom";
  env?: Record<string, EnvVariable>;
  adapter_configs: Record<string, unknown>;
}

export type UniversalCapability = McpServer | Rule | Skill | Hook | Plugin | Custom;

export interface AgentExample {
  user: string;
  agent: string;
}

export interface AgentDefinition {
  id: CompositeId;
  name: string;
  description: string;
  version: string;
  author: string;
  visibility: Visibility;
  tags: string[];
  model: AgentModel;
  color: AgentColor;
  memory: MemoryScope;
  tools: ToolAccess[];
  required_capabilities: CompositeId[];
  prompt: string;
  examples: AgentExample[];
}

export interface Preset {
  id: string;
  name: string;
  description: string;
  capability_ids: string[];
  tags: string[];
  is_bundled: boolean;
}

export type AdapterType = "claude-code" | "cursor" | "windsurf";

export interface Project {
  id: string;
  name: string;
  path: string;
  adapters: AdapterType[];
  lastOpened?: string;
}

export type RegistrySort = "name" | "stars" | "newest";

export interface RegistryFilters {
  type: CapabilityType | "all";
  visibility: Visibility | "all";
  adapter: AdapterType | "all";
  search: string;
  category: string | "all";
  sort: RegistrySort;
}

export interface AgentFilters {
  search: string;
  visibility: Visibility | "all";
}

export function parseCompositeId(id: CompositeId): { author: string; name: string } {
  const [author, name] = id.split("/");
  return { author, name };
}

export function isPublic(id: CompositeId): boolean {
  return id.startsWith("community/");
}

export function getCapabilityType(capability: UniversalCapability): CapabilityType {
  return capability.type;
}

export function getCapabilityTypeLabel(type: CapabilityType): string {
  const labels: Record<CapabilityType, string> = {
    mcp: "MCP",
    rule: "Rule",
    skill: "Skill",
    hook: "Hook",
    plugin: "Plugin",
    custom: "Custom",
  };
  return labels[type];
}

export function getModelLabel(model: AgentModel): string {
  const labels: Record<AgentModel, string> = {
    haiku: "Haiku",
    sonnet: "Sonnet",
    opus: "Opus",
  };
  return labels[model];
}

export function getColorHex(color: AgentColor): string {
  const colors: Record<AgentColor, string> = {
    red: "#f87171",
    blue: "#5b8af5",
    green: "#34d399",
    yellow: "#fbbf24",
    purple: "#a78bfa",
    orange: "#fb923c",
    pink: "#f472b6",
    cyan: "#22d3ee",
  };
  return colors[color];
}
