import { invoke } from "@tauri-apps/api/core";
import type { UniversalCapability, AgentDefinition, Preset } from "./types";

export async function getAllCapabilities(): Promise<UniversalCapability[]> {
  return invoke<UniversalCapability[]>("get_all_capabilities");
}

export async function getAllAgents(): Promise<AgentDefinition[]> {
  return invoke<AgentDefinition[]>("get_all_agents");
}

export async function getCapabilitiesByType(
  capabilityType: string
): Promise<UniversalCapability[]> {
  return invoke<UniversalCapability[]>("get_capabilities_by_type", {
    capabilityType,
  });
}

export async function searchCapabilities(
  query: string
): Promise<UniversalCapability[]> {
  return invoke<UniversalCapability[]>("search_capabilities", { query });
}

export async function searchAgents(query: string): Promise<AgentDefinition[]> {
  return invoke<AgentDefinition[]>("search_agents", { query });
}

export async function saveAgent(agent: AgentDefinition): Promise<AgentDefinition> {
  return invoke<AgentDefinition>("save_agent", { agent });
}

export async function deleteAgent(id: string): Promise<void> {
  return invoke<void>("delete_agent", { id });
}

export async function getPresets(): Promise<Preset[]> {
  return invoke<Preset[]>("get_presets");
}

export async function savePreset(preset: Preset): Promise<Preset> {
  return invoke<Preset>("save_preset", { preset });
}

export async function updatePreset(preset: Preset): Promise<Preset> {
  return invoke<Preset>("update_preset", { preset });
}

export async function deletePreset(id: string): Promise<void> {
  return invoke<void>("delete_preset", { id });
}

export async function addCapabilityToPreset(presetId: string, capabilityId: string): Promise<Preset> {
  return invoke<Preset>("add_capability_to_preset", { presetId, capabilityId });
}

export async function removeCapabilityFromPreset(presetId: string, capabilityId: string): Promise<Preset> {
  return invoke<Preset>("remove_capability_from_preset", { presetId, capabilityId });
}

export interface NotesEntry {
  name: string;
  relative_path: string;
  is_folder: boolean;
}

export async function listNotesEntries(relativePath: string): Promise<NotesEntry[]> {
  return invoke<NotesEntry[]>("list_notes_entries", { relativePath });
}

export async function readNoteContent(relativePath: string): Promise<string> {
  return invoke<string>("read_note_content", { relativePath });
}

export async function writeNoteContent(relativePath: string, content: string): Promise<void> {
  return invoke<void>("write_note_content", { relativePath, content });
}

export async function createNotesFolder(parentRelativePath: string, name: string): Promise<NotesEntry> {
  return invoke<NotesEntry>("create_notes_folder", { parentRelativePath: parentRelativePath, name });
}

export async function createNotesFile(parentRelativePath: string, name: string): Promise<NotesEntry> {
  return invoke<NotesEntry>("create_notes_file", { parentRelativePath: parentRelativePath, name });
}

export async function renameNotesEntry(relativePath: string, newName: string): Promise<NotesEntry> {
  return invoke<NotesEntry>("rename_notes_entry", { relativePath, newName });
}

export async function deleteNotesEntry(relativePath: string): Promise<void> {
  return invoke<void>("delete_notes_entry", { relativePath });
}

export async function moveNotesEntry(relativePath: string, newParentRelativePath: string): Promise<NotesEntry> {
  return invoke<NotesEntry>("move_notes_entry", { relativePath, newParentRelativePath });
}

export interface DiffEntry {
  file_path: string;
  change_type: "add" | "modify" | "remove";
  current_content: string | null;
  proposed_content: string;
}

export interface DeployResultResponse {
  success: boolean;
  files_written: string[];
  errors: string[];
  warnings: string[];
}

export type ClaudeSettingsTarget = "local" | "project" | "user";

export async function previewDeploy(
  projectPath: string,
  adapterId: string,
  capabilityIds: string[],
  agentIds: string[],
  claudeSettingsTarget?: ClaudeSettingsTarget,
  globalDeploy?: boolean
): Promise<DiffEntry[]> {
  return invoke<DiffEntry[]>("preview_deploy", {
    projectPath,
    adapterId,
    capabilityIds,
    agentIds,
    claudeSettingsTarget: claudeSettingsTarget ?? null,
    globalDeploy: globalDeploy ?? false,
  });
}

export async function executeDeploy(
  projectPath: string,
  adapterId: string,
  capabilityIds: string[],
  agentIds: string[],
  strategies: Record<string, string>,
  claudeSettingsTarget?: ClaudeSettingsTarget,
  globalDeploy?: boolean
): Promise<DeployResultResponse> {
  return invoke<DeployResultResponse>("execute_deploy", {
    projectPath,
    adapterId,
    capabilityIds,
    agentIds,
    strategies,
    claudeSettingsTarget: claudeSettingsTarget ?? null,
    globalDeploy: globalDeploy ?? false,
  });
}

export interface RecentProject {
  name: string;
  path: string;
  last_opened: string;
}

export interface DetectedAdapter {
  id: string;
  name: string;
  capabilities: {
    mcp: boolean;
    rules: boolean;
    skills: boolean;
    hooks: boolean;
    plugins: boolean;
    agents: boolean;
  };
}

export async function selectProjectFolder(): Promise<string | null> {
  return invoke<string | null>("select_project_folder");
}

export async function detectAdapters(projectPath: string): Promise<DetectedAdapter[]> {
  return invoke<DetectedAdapter[]>("detect_adapters", { projectPath });
}

export async function getRecentProjects(): Promise<RecentProject[]> {
  return invoke<RecentProject[]>("get_recent_projects");
}

export interface ProjectWithMcp {
  path: string;
  name: string;
}

export async function listProjectsWithMcp(): Promise<ProjectWithMcp[]> {
  return invoke<ProjectWithMcp[]>("list_projects_with_mcp");
}

export async function addRecentProject(projectPath: string): Promise<RecentProject[]> {
  return invoke<RecentProject[]>("add_recent_project", { projectPath });
}

export async function removeRecentProject(projectPath: string): Promise<RecentProject[]> {
  return invoke<RecentProject[]>("remove_recent_project", { projectPath });
}

export interface GeneralSettings {
  theme: string;
  launch_at_login: boolean;
  username: string;
  show_in_menu_bar: boolean;
  keep_running_on_close: boolean;
}

export interface RegistrySettings {
  github_repo: string;
  github_branch: string;
  poll_interval_minutes: number;
  auto_update: boolean;
  last_sync: string | null;
}

export interface DeploySettings {
  default_strategy: string;
  create_backups: boolean;
  default_adapters: string[];
}

export interface SecretsInfo {
  count: number;
}

export interface AnalyticsSettings {
  refresh_interval_minutes: number;
  /** Show Anthropic internal usage buckets (omelette, tangelo, …) on Claude analytics */
  show_internal_usage_buckets?: boolean;
  /** Native macOS notifications when usage limits change state */
  limit_notifications_enabled?: boolean;
}

export interface AppSettings {
  general: GeneralSettings;
  registry: RegistrySettings;
  deploy: DeploySettings;
  secrets: SecretsInfo;
  analytics: AnalyticsSettings;
}

export async function getSettings(): Promise<AppSettings> {
  return invoke<AppSettings>("get_settings");
}

export async function updateSettings(settings: AppSettings): Promise<AppSettings> {
  return invoke<AppSettings>("update_settings", { settings });
}

export async function updateGeneralSettings(general: GeneralSettings): Promise<AppSettings> {
  return invoke<AppSettings>("update_general_settings", { general });
}

export async function updateRegistrySettings(registry: RegistrySettings): Promise<AppSettings> {
  return invoke<AppSettings>("update_registry_settings", { registry });
}

export async function updateDeploySettings(deploy: DeploySettings): Promise<AppSettings> {
  return invoke<AppSettings>("update_deploy_settings", { deploy });
}

export async function updateAnalyticsSettings(analytics: AnalyticsSettings): Promise<AppSettings> {
  return invoke<AppSettings>("update_analytics_settings", { analytics });
}

export async function getUsername(): Promise<string> {
  return invoke<string>("get_username");
}

export async function getAuthorId(): Promise<string> {
  return invoke<string>("get_author_id");
}

export interface BackupInfo {
  backup_id: string;
  adapter_id: string;
  timestamp: number;
  file_count: number;
}

export async function createProjectBackup(
  projectPath: string,
  adapterId: string,
  files: string[]
): Promise<string> {
  return invoke<string>("create_project_backup", {
    projectPath,
    adapterId,
    files,
  });
}

export async function restoreProjectBackup(backupId: string): Promise<string[]> {
  return invoke<string[]>("restore_project_backup", { backupId });
}

export async function getProjectBackups(projectPath: string): Promise<BackupInfo[]> {
  return invoke<BackupInfo[]>("get_project_backups", { projectPath });
}

export async function cleanupBackups(retentionDays: number): Promise<number> {
  return invoke<number>("cleanup_backups", { retentionDays });
}

export async function deleteProjectBackup(backupId: string): Promise<void> {
  return invoke<void>("delete_project_backup", { backupId });
}

export async function runBackupCleanupOnLaunch(): Promise<number> {
  return invoke<number>("run_backup_cleanup_on_launch");
}

export interface SyncConfig {
  repo_url: string;
  branch: string;
  polling_interval_minutes: number;
  auto_update: boolean;
  github_pat: string | null;
}

export interface SyncState {
  last_etag: string | null;
  last_sync_time: string | null;
  last_error: string | null;
  is_syncing: boolean;
  capabilities_count: number;
  agents_count: number;
}

export interface SyncResult {
  success: boolean;
  message: string;
  new_capabilities: number;
  new_agents: number;
  updated: boolean;
}

export async function syncRegistryNow(config: SyncConfig): Promise<SyncResult> {
  return invoke<SyncResult>("sync_registry_now", { config });
}

export async function getSyncStatus(): Promise<SyncState> {
  return invoke<SyncState>("get_sync_status");
}

export async function startRegistryPolling(config: SyncConfig): Promise<void> {
  return invoke<void>("start_registry_polling", { config });
}

export async function stopRegistryPolling(): Promise<void> {
  return invoke<void>("stop_registry_polling");
}

export async function isRegistryPollingActive(): Promise<boolean> {
  return invoke<boolean>("is_registry_polling_active");
}

export async function getCommunityRegistryDir(): Promise<string> {
  return invoke<string>("get_community_registry_dir");
}

export interface GlobalConfigInfo {
  mcp_servers: string[];
  has_config: boolean;
}

export async function getGlobalConfig(adapterId: string): Promise<GlobalConfigInfo> {
  return invoke<GlobalConfigInfo>("get_global_config", { adapterId });
}

export async function saveCustomCapability(
  capability: UniversalCapability,
  originalId?: string
): Promise<UniversalCapability> {
  return invoke<UniversalCapability>("save_custom_capability", {
    original_id: originalId ?? null,
    capability,
  });
}

export async function deleteCustomCapability(id: string, capabilityType: string): Promise<void> {
  return invoke<void>("delete_custom_capability", { id, capabilityType });
}

export async function getCustomCapabilitiesDir(): Promise<string> {
  return invoke<string>("get_custom_capabilities_dir");
}

export async function listCustomCapabilities(): Promise<string[]> {
  return invoke<string[]>("list_custom_capabilities");
}

export interface ProjectDeployment {
  adapter: string;
  capability_ids: string[];
  agent_ids: string[];
  timestamp: string;
}

export interface ProjectInfo {
  path: string;
  name: string;
  last_deployed: string | null;
  deployments_count: number;
  detected_adapters: string[];
  capabilities_count: number;
  agents_count: number;
}

export interface AdapterStatus {
  id: string;
  name: string;
  has_config: boolean;
  config_path: string;
}

export interface ProjectDetail {
  path: string;
  name: string;
  last_deployed: string | null;
  deployments: ProjectDeployment[];
  detected_adapters: AdapterStatus[];
  deployed_capabilities: string[];
  deployed_agents: string[];
  is_tracked?: boolean;
}

export async function getAllProjects(): Promise<ProjectInfo[]> {
  return invoke<ProjectInfo[]>("get_all_projects");
}

export async function getProjectDetail(projectPath: string): Promise<ProjectDetail | null> {
  return invoke<ProjectDetail | null>("get_project_detail", { projectPath });
}

export async function addProject(projectPath: string, name: string): Promise<ProjectInfo> {
  return invoke<ProjectInfo>("add_project", { projectPath, name });
}

export async function removeProject(projectPath: string): Promise<void> {
  return invoke<void>("remove_project", { projectPath });
}

export async function recordDeployment(
  projectPath: string,
  adapter: string,
  capabilityIds: string[],
  agentIds: string[]
): Promise<void> {
  return invoke<void>("record_deployment", { projectPath, adapter, capabilityIds, agentIds });
}

export async function openProjectInFinder(projectPath: string): Promise<void> {
  return invoke<void>("open_project_in_finder", { projectPath });
}

export async function openProjectInCursor(projectPath: string): Promise<void> {
  return invoke<void>("open_project_in_cursor", { projectPath });
}

export async function openProjectInVscode(projectPath: string): Promise<void> {
  return invoke<void>("open_project_in_vscode", { projectPath });
}

export async function openProjectInTerminal(projectPath: string): Promise<void> {
  return invoke<void>("open_project_in_terminal", { projectPath });
}

export interface InstalledItem {
  name: string;
  item_type: string;
  adapter_id: string;
  adapter_name: string;
}

export async function getProjectInstalledItems(projectPath: string): Promise<InstalledItem[]> {
  return invoke<InstalledItem[]>("get_project_installed_items", { projectPath });
}

export async function removeProjectItem(
  projectPath: string,
  itemName: string,
  itemType: string,
  adapterId: string,
): Promise<void> {
  return invoke<void>("remove_project_item", { projectPath, itemName, itemType, adapterId });
}

export interface ExportedPreset {
  id: string;
  name: string;
  description: string;
  capability_ids: string[];
  tags: string[];
}

export interface ExportData {
  version: string;
  exported_at: string;
  capabilities: UniversalCapability[];
  agents: AgentDefinition[];
  presets: ExportedPreset[];
}

export interface ImportConflict {
  item_type: string;
  item_id: string;
  message: string;
}

export interface ImportResult {
  success: boolean;
  message: string;
  capabilities_imported: number;
  agents_imported: number;
  presets_imported: number;
  conflicts: ImportConflict[];
}

export async function exportData(
  capabilityIds: string[],
  agentIds: string[],
  presetIds: string[]
): Promise<ExportData> {
  return invoke<ExportData>("export_data", { capabilityIds, agentIds, presetIds });
}

export async function importData(
  data: ExportData,
  renameConflicts: boolean
): Promise<ImportResult> {
  return invoke<ImportResult>("import_data", { data, renameConflicts });
}

export async function validateImportData(jsonString: string): Promise<ExportData> {
  return invoke<ExportData>("validate_import_data", { jsonString });
}

export async function updateTrayVisibility(show: boolean): Promise<void> {
  return invoke<void>("update_tray_visibility", { show });
}

export interface DriftFile {
  path: string;
  relative_path: string;
  change_type: string;
  expected_hash: string | null;
  current_hash: string | null;
}

export interface DriftInfo {
  has_drift: boolean;
  files: DriftFile[];
}

export interface DriftDiff {
  expected: string;
  current: string;
}

export async function detectDrift(projectPath: string): Promise<DriftInfo> {
  return invoke<DriftInfo>("detect_drift", { projectPath });
}

export async function acceptDrift(projectPath: string): Promise<void> {
  return invoke<void>("accept_drift", { projectPath });
}

export async function restoreDrift(projectPath: string): Promise<void> {
  return invoke<void>("restore_drift", { projectPath });
}

export async function getDriftDiff(projectPath: string, filePath: string): Promise<DriftDiff | null> {
  return invoke<DriftDiff | null>("get_drift_diff", { projectPath, filePath });
}

export interface AgentMemory {
  agent_name: string;
  path: string;
  size_bytes: number;
  file_count: number;
}

export async function listAgentMemory(projectPath: string): Promise<AgentMemory[]> {
  return invoke<AgentMemory[]>("list_agent_memory", { projectPath });
}

export async function listGlobalAgentMemory(): Promise<AgentMemory[]> {
  return invoke<AgentMemory[]>("list_global_agent_memory");
}

export async function clearAgentMemory(memoryPath: string): Promise<void> {
  return invoke<void>("clear_agent_memory", { memoryPath });
}

export async function clearAllAgentMemory(projectPath: string): Promise<void> {
  return invoke<void>("clear_all_agent_memory", { projectPath });
}

export async function clearAllGlobalMemory(): Promise<void> {
  return invoke<void>("clear_all_global_memory");
}

export async function readProjectMemory(projectPath: string): Promise<string> {
  return invoke<string>("read_project_memory", { projectPath });
}

export async function writeProjectMemory(projectPath: string, content: string): Promise<void> {
  return invoke<void>("write_project_memory", { projectPath, content });
}

export interface SecretInfo {
  key: string;
  has_value: boolean;
}

export async function storeSecret(key: string, value: string): Promise<void> {
  return invoke<void>("store_secret", { key, value });
}

export async function getSecret(key: string): Promise<string | null> {
  return invoke<string | null>("get_secret", { key });
}

export async function deleteSecret(key: string): Promise<void> {
  return invoke<void>("delete_secret", { key });
}

export async function listSecrets(): Promise<SecretInfo[]> {
  return invoke<SecretInfo[]>("list_secrets");
}

export async function getSecretsCount(): Promise<number> {
  return invoke<number>("get_secrets_count");
}

export async function readClaudeSettings(): Promise<string> {
  return invoke<string>("read_claude_settings");
}

export async function writeClaudeSettings(content: string): Promise<void> {
  return invoke<void>("write_claude_settings", { content });
}

export async function readClaudeMemory(): Promise<string> {
  return invoke<string>("read_claude_memory");
}

export async function writeClaudeMemory(content: string): Promise<void> {
  return invoke<void>("write_claude_memory", { content });
}

export async function readClaudeDesktopConfig(): Promise<string> {
  return invoke<string>("read_claude_desktop_config");
}

export async function writeClaudeDesktopConfig(content: string): Promise<void> {
  return invoke<void>("write_claude_desktop_config", { content });
}

export async function getClaudeDesktopConfigPath(): Promise<string> {
  return invoke<string>("get_claude_desktop_config_path");
}

export async function readGlobalConfigRaw(adapterId: string): Promise<string> {
  return invoke<string>("read_global_config_raw", { adapterId });
}

export async function writeGlobalConfigRaw(adapterId: string, content: string): Promise<void> {
  return invoke<void>("write_global_config_raw", { adapterId, content });
}

export async function addGlobalMcpServer(
  adapterId: string,
  name: string,
  config: Record<string, unknown>
): Promise<void> {
  return invoke<void>("add_global_mcp_server", { adapterId, name, config });
}

export async function removeGlobalMcpServer(adapterId: string, name: string): Promise<void> {
  return invoke<void>("remove_global_mcp_server", { adapterId, name });
}

export interface DiscoveredCapability {
  name: string;
  description: string;
  transport: string;
  command: string;
  args: string[];
  url: string;
  env: Record<string, unknown>;
  source: string;
  adapter_id: string;
  adapter_ids: string[];
}

export async function discoverCapabilities(): Promise<DiscoveredCapability[]> {
  return invoke<DiscoveredCapability[]>("discover_capabilities");
}

export interface UsageData {
  input_tokens?: number;
  cache_read_input_tokens?: number;
  cache_creation_input_tokens?: number;
  output_tokens?: number;
}

export interface ProjectUsageRecord {
  uuid: string;
  timestamp: string;
  model?: string;
  usage?: UsageData;
  project_path?: string | null;
}

export async function readProjectUsageFiles(): Promise<ProjectUsageRecord[]> {
  return invoke<ProjectUsageRecord[]>("read_project_usage_files");
}

// --- AI Code Attribution ---

export interface ScoredCommit {
  commit_hash: string;
  branch_name: string;
  scored_at: number;
  lines_added?: number;
  lines_deleted?: number;
  composer_lines_added?: number;
  composer_lines_deleted?: number;
  human_lines_added?: number;
  human_lines_deleted?: number;
  blank_lines_added?: number;
  blank_lines_deleted?: number;
  commit_message?: string;
  commit_date?: string;
  ai_percentage: number;
}

export interface AiCodeSummary {
  total_commits: number;
  avg_ai_percentage: number;
  total_composer_lines: number;
  total_human_lines: number;
  total_lines_added: number;
  total_lines_deleted: number;
}

export interface ConversationSummary {
  conversation_id: string;
  title?: string;
  tldr?: string;
  overview?: string;
  summary_bullets?: string;
  model?: string;
  mode?: string;
  updated_at: number;
}

export interface FileTypeBreakdown {
  file_extension: string;
  source: string;
  count: number;
}

export async function getAiCommitScores(limit?: number, offset?: number): Promise<ScoredCommit[]> {
  return invoke<ScoredCommit[]>("get_ai_commit_scores", { limit: limit ?? null, offset: offset ?? null });
}

export async function getAiCodeSummary(): Promise<AiCodeSummary> {
  return invoke<AiCodeSummary>("get_ai_code_summary");
}

export async function getConversationSummaries(limit?: number, offset?: number): Promise<ConversationSummary[]> {
  return invoke<ConversationSummary[]>("get_conversation_summaries", { limit: limit ?? null, offset: offset ?? null });
}

export async function getAiFileTypeBreakdown(): Promise<FileTypeBreakdown[]> {
  return invoke<FileTypeBreakdown[]>("get_ai_file_type_breakdown");
}

export async function getAiTrackingModelBreakdown(): Promise<Record<string, number>> {
  return invoke<Record<string, number>>("get_ai_tracking_model_breakdown");
}

// --- Session Stats ---

export interface LongestSession {
  session_id: string;
  duration: number;
  message_count: number;
  timestamp: string;
}

export interface DailyActivity {
  date: string;
  message_count: number;
  session_count: number;
  tool_call_count: number;
}

export interface ModelUsageEntry {
  input_tokens: number;
  output_tokens: number;
  cache_read_input_tokens: number;
  cache_creation_input_tokens: number;
  cost_usd: number;
  context_window: number;
  max_output_tokens: number;
  web_search_requests: number;
}

export interface SessionStats {
  total_sessions: number;
  total_messages: number;
  longest_session?: LongestSession;
  hour_counts: number[];
  daily_activity: DailyActivity[];
  model_usage: Record<string, ModelUsageEntry>;
  first_session_date?: string;
  total_cost_usd: number;
}

export async function getClaudeSessionStats(): Promise<SessionStats> {
  return invoke<SessionStats>("get_claude_session_stats");
}

// --- Prompt History ---

export interface PromptEntry {
  display: string;
  timestamp: string;
  timestamp_ms: number;
  project?: string;
  project_name?: string;
  session_id?: string;
}

export interface PromptStats {
  total: number;
  projects: string[];
}

export async function getPromptHistory(limit?: number, offset?: number): Promise<PromptEntry[]> {
  return invoke<PromptEntry[]>("get_prompt_history", { limit: limit ?? null, offset: offset ?? null });
}

export async function searchPromptHistory(query: string): Promise<PromptEntry[]> {
  return invoke<PromptEntry[]>("search_prompt_history", { query });
}

export async function getPromptStats(): Promise<PromptStats> {
  return invoke<PromptStats>("get_prompt_stats");
}

// --- Transcripts ---

export interface TranscriptSession {
  session_id: string;
  project_name: string;
  source: string;
  modified_at: string;
  file_size_bytes: number;
  file_path: string;
}

export interface TranscriptMessage {
  role: string;
  content: string;
}

export async function listTranscriptSessions(): Promise<TranscriptSession[]> {
  return invoke<TranscriptSession[]>("list_transcript_sessions");
}

export async function readTranscript(filePath: string, limit?: number, offset?: number): Promise<TranscriptMessage[]> {
  return invoke<TranscriptMessage[]>("read_transcript", { filePath, limit: limit ?? null, offset: offset ?? null });
}

export async function searchTranscripts(query: string): Promise<TranscriptSession[]> {
  return invoke<TranscriptSession[]>("search_transcripts", { query });
}

// --- Extensions ---

export interface ClaudePlugin {
  name: string;
  version: string;
  scope: string;
  project_path?: string;
  installed_at: string;
  last_updated: string;
  enabled: boolean;
}

export interface CursorExtension {
  id: string;
  name: string;
  version: string;
  publisher: string;
  installed_timestamp?: number;
  source: string;
  is_builtin: boolean;
}

export interface CursorPlugin {
  name: string;
  path: string;
}

export async function listClaudePlugins(): Promise<ClaudePlugin[]> {
  return invoke<ClaudePlugin[]>("list_claude_plugins");
}

export async function listCursorExtensions(): Promise<CursorExtension[]> {
  return invoke<CursorExtension[]>("list_cursor_extensions");
}

export async function listCursorPlugins(): Promise<CursorPlugin[]> {
  return invoke<CursorPlugin[]>("list_cursor_plugins");
}

export async function toggleClaudePlugin(pluginName: string, enabled: boolean): Promise<void> {
  return invoke<void>("toggle_claude_plugin", { pluginName, enabled });
}

// --- Plans & Todos ---

export interface PlanEntry {
  name: string;
  source: string;
  file_path: string;
  overview: string;
  modified_at: string;
}

export interface TodoItem {
  content: string;
  status: string;
  source: string;
  session_id?: string;
}

export interface TodoStats {
  total: number;
  pending: number;
  in_progress: number;
  completed: number;
}

export async function listPlans(): Promise<PlanEntry[]> {
  return invoke<PlanEntry[]>("list_plans");
}

export async function listProjectPlans(projectPath: string): Promise<PlanEntry[]> {
  return invoke<PlanEntry[]>("list_project_plans", { projectPath });
}

export async function readPlan(filePath: string): Promise<string> {
  return invoke<string>("read_plan", { filePath });
}

export async function listTodos(projectPath?: string | null): Promise<TodoItem[]> {
  return invoke<TodoItem[]>("list_todos", { projectPath: projectPath ?? null });
}

export async function getTodoStats(projectPath?: string | null): Promise<TodoStats> {
  return invoke<TodoStats>("get_todo_stats", { projectPath: projectPath ?? null });
}

// --- Permissions ---

export interface ClaudePermissions {
  allow: string[];
  deny: string[];
  enabled_plugins: Record<string, boolean>;
  skip_dangerous_mode: boolean;
}

export interface PolicyRestriction {
  key: string;
  allowed: boolean;
}

export interface CursorPermissions {
  allow: string[];
  deny: string[];
}

export interface ClaudeProjectFilePermissions {
  allow: string[];
  deny: string[];
}

export interface ClaudeProjectPermissions {
  settings: ClaudeProjectFilePermissions;
  settings_local: ClaudeProjectFilePermissions;
}

export async function getClaudePermissions(): Promise<ClaudePermissions> {
  return invoke<ClaudePermissions>("get_claude_permissions");
}

export async function updateClaudePermissions(
  allow: string[],
  deny: string[],
  skipDangerousMode: boolean
): Promise<void> {
  return invoke<void>("update_claude_permissions", {
    allow,
    deny,
    skipDangerousMode,
  });
}

export async function getClaudePolicy(): Promise<PolicyRestriction[]> {
  return invoke<PolicyRestriction[]>("get_claude_policy");
}

export async function updateClaudePolicy(key: string, allowed: boolean): Promise<void> {
  return invoke<void>("update_claude_policy", { key, allowed });
}

export async function getClaudeProjectPermissions(projectPath: string): Promise<ClaudeProjectPermissions> {
  return invoke<ClaudeProjectPermissions>("get_claude_project_permissions", { projectPath });
}

export async function updateClaudeProjectPermissions(
  projectPath: string,
  file: "settings" | "settings_local",
  allow: string[],
  deny: string[]
): Promise<void> {
  return invoke<void>("update_claude_project_permissions", { projectPath, file, allow, deny });
}

export async function getCursorPermissions(): Promise<CursorPermissions> {
  return invoke<CursorPermissions>("get_cursor_permissions");
}

export async function updateCursorPermissions(allow: string[], deny: string[]): Promise<void> {
  return invoke<void>("update_cursor_permissions", { allow, deny });
}

// --- GitHub Skill Fetch ---

export interface FetchedSkill {
  name: string;
  description: string;
  license?: string;
  allowed_tools?: string[];
  model?: string;
  context?: string;
  agent?: string;
  argument_hint?: string;
  files: import("./types").SkillFile[];
  has_scripts: boolean;
  rate_limit_remaining?: number;
  rate_limit_reset?: number;
}

export interface OfficialSkillEntry {
  name: string;
  description: string;
  github_url: string;
  has_scripts: boolean;
  file_count: number;
}

export async function fetchGithubSkill(url: string, githubToken?: string): Promise<FetchedSkill> {
  return invoke<FetchedSkill>("fetch_github_skill", {
    url,
    githubToken: githubToken ?? null,
  });
}

export async function getOfficialSkillsIndex(forceRefresh: boolean, githubToken?: string): Promise<OfficialSkillEntry[]> {
  return invoke<OfficialSkillEntry[]>("get_official_skills_index", {
    forceRefresh,
    githubToken: githubToken ?? null,
  });
}

// --- OpenClaw Skills Registry ---

export interface OpenClawSkillEntry {
  name: string;
  display_name: string;
  description: string;
  category: string;
  author: string;
  author_github: string;
  github_url: string;
  github_stars: number;
  has_scripts: boolean;
  file_count: number;
  tags: string[];
  version: string;
  license: string;
  compatible_adapters: string[];
}

export async function getOpenClawSkillsIndex(forceRefresh: boolean, minStars?: number): Promise<OpenClawSkillEntry[]> {
  return invoke<OpenClawSkillEntry[]>("get_openclaw_skills_index", {
    forceRefresh,
    minStars: minStars ?? null,
  });
}

export async function searchOpenClawSkills(query: string, minStars?: number): Promise<OpenClawSkillEntry[]> {
  return invoke<OpenClawSkillEntry[]>("search_openclaw_skills", {
    query,
    minStars: minStars ?? null,
  });
}

// --- MCP Tool Discovery ---

export interface McpServerConfig {
  transport?: string;
  command?: string;
  args?: string[];
  url?: string;
  serverUrl?: string;
  env?: Record<string, string>;
  headers?: Record<string, string>;
}

/** Unified tool discovery — auto-detects stdio/http/sse transport */
export async function discoverMcpTools(config: McpServerConfig): Promise<import("./types").McpTool[]> {
  return invoke<import("./types").McpTool[]>("discover_mcp_tools", { config });
}

// --- MCP Registry ---

export interface McpRegistryEnvVar {
  name: string;
  description: string;
  is_required: boolean;
  is_secret: boolean;
}

export interface McpRegistryEntry {
  name: string;
  title: string;
  description: string;
  version: string;
  icon_url?: string;
  website_url?: string;
  repository_url?: string;
  transport: string;
  command?: string;
  args: string[];
  url?: string;
  env_vars: McpRegistryEnvVar[];
  is_official: boolean;
}

export interface McpRegistryPage {
  entries: McpRegistryEntry[];
  next_cursor?: string;
  page: number;
}

export async function searchMcpRegistry(query: string, limit?: number, cursor?: string, page?: number): Promise<McpRegistryPage> {
  return invoke<McpRegistryPage>("search_mcp_registry", { query, limit: limit ?? null, cursor: cursor ?? null, page: page ?? null });
}

export async function getMcpRegistryPopular(forceRefresh: boolean, cursor?: string, page?: number): Promise<McpRegistryPage> {
  return invoke<McpRegistryPage>("get_mcp_registry_popular", { forceRefresh, cursor: cursor ?? null, page: page ?? null });
}

/** HTTP-only tool discovery (kept for direct use) */
export async function discoverMcpToolsHttp(
  url: string,
  headers: Record<string, string>,
): Promise<import("./types").McpTool[]> {
  return invoke<import("./types").McpTool[]>("discover_mcp_tools_http", { url, headers });
}
