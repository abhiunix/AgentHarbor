import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  getClaudePermissions,
  updateClaudePermissions,
  getClaudeProjectPermissions,
  updateClaudeProjectPermissions,
  readClaudeSettings,
} from "../lib/tauri";
import type { ClaudePermissions } from "../lib/tauri";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";
import { DebugPath } from "../components/common/DebugPath";
import {
  AdapterConfigSection,
  type AdapterGlobalConfig,
} from "../components/global/AdapterConfigSection";

function InfoIcon({ text }: { text: string }) {
  return (
    <span className="relative inline-flex group ml-1.5 align-middle">
      <span className="w-4 h-4 rounded-full bg-[#2a2b36] text-text-muted text-[10px] font-bold flex items-center justify-center cursor-help select-none">
        i
      </span>
      <span className="absolute left-1/2 -translate-x-1/2 bottom-full mb-1 px-2 py-1 text-xs text-text-primary bg-[#13141a] border border-border rounded opacity-0 group-hover:opacity-100 pointer-events-none transition-opacity z-10 w-56 max-w-xs leading-snug">
        {text}
      </span>
    </span>
  );
}

function SettingRowLabel({ label, info }: { label: string; info: string }) {
  return (
    <span className="text-sm font-medium text-text-primary inline-flex items-center">
      {label}
      <InfoIcon text={info} />
    </span>
  );
}

function SwitchRow({
  label,
  info,
  checked,
  onChange,
}: {
  label: string;
  info: string;
  checked: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <label className="flex items-center justify-between cursor-pointer py-1.5">
      <SettingRowLabel label={label} info={info} />
      <div
        className={`relative w-10 h-5 rounded-full transition-colors ${
          checked ? "bg-blue-500" : "bg-[#2a2b36]"
        }`}
        onClick={() => onChange(!checked)}
      >
        <div
          className={`absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
            checked ? "translate-x-5" : "translate-x-0.5"
          }`}
        />
      </div>
    </label>
  );
}

function SelectRow({
  label,
  info,
  value,
  options,
  onChange,
}: {
  label: string;
  info: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (next: string) => void;
}) {
  return (
    <label className="flex items-center justify-between gap-3 py-1.5">
      <SettingRowLabel label={label} info={info} />
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="bg-[#13141a] border border-border rounded px-2 py-1 text-sm text-text-primary focus:outline-none focus:border-blue-500 min-w-[8rem]"
      >
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </label>
  );
}

function NumberRow({
  label,
  info,
  value,
  onChange,
  placeholder,
}: {
  label: string;
  info: string;
  value: string;
  onChange: (next: string) => void;
  placeholder?: string;
}) {
  return (
    <label className="flex items-center justify-between gap-3 py-1.5">
      <SettingRowLabel label={label} info={info} />
      <input
        type="number"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="bg-[#13141a] border border-border rounded px-2 py-1 text-sm text-text-primary focus:outline-none focus:border-blue-500 w-28"
        min={0}
      />
    </label>
  );
}

function TextRow({
  label,
  info,
  value,
  onChange,
  placeholder,
  datalistId,
  datalistOptions,
}: {
  label: string;
  info: string;
  value: string;
  onChange: (next: string) => void;
  placeholder?: string;
  datalistId?: string;
  datalistOptions?: string[];
}) {
  return (
    <label className="flex items-center justify-between gap-3 py-1.5">
      <SettingRowLabel label={label} info={info} />
      <input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        list={datalistId}
        className="bg-[#13141a] border border-border rounded px-2 py-1 text-sm text-text-primary focus:outline-none focus:border-blue-500 flex-1 min-w-0 max-w-[16rem]"
      />
      {datalistId && datalistOptions && (
        <datalist id={datalistId}>
          {datalistOptions.map((o) => (
            <option key={o} value={o} />
          ))}
        </datalist>
      )}
    </label>
  );
}

function PermissionList({
  label,
  info,
  items,
  onRemove,
  onAdd,
  placeholder,
  emptyText,
}: {
  label: string;
  info?: string;
  items: string[];
  onRemove: (item: string) => void;
  onAdd: (item: string) => void;
  placeholder?: string;
  emptyText?: string;
}) {
  const [newItem, setNewItem] = useState("");

  function handleAdd() {
    const trimmed = newItem.trim();
    if (trimmed && !items.includes(trimmed)) {
      onAdd(trimmed);
      setNewItem("");
    }
  }

  return (
    <div className="mb-4">
      <h4 className="text-sm font-medium text-text-secondary mb-2 inline-flex items-center">
        {label}
        {info && <InfoIcon text={info} />}
      </h4>
      <div className="space-y-1 mb-2">
        {items.length === 0 && (
          <p className="text-xs text-text-muted italic">{emptyText ?? "None"}</p>
        )}
        {items.map((item) => (
          <div
            key={item}
            className="flex items-center justify-between bg-[#13141a] rounded px-3 py-1.5 text-sm text-text-primary group"
          >
            <span className="font-mono text-xs truncate mr-2">{item}</span>
            <button
              onClick={() => onRemove(item)}
              className="text-text-muted hover:text-red-400 transition-colors shrink-0"
            >
              ✕
            </button>
          </div>
        ))}
      </div>
      <div className="flex gap-2">
        <input
          type="text"
          value={newItem}
          onChange={(e) => setNewItem(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleAdd()}
          placeholder={placeholder ?? "e.g. Bash(npm*)"}
          className="flex-1 bg-[#13141a] border border-border rounded px-3 py-1.5 text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:border-blue-500"
        />
        <button
          onClick={handleAdd}
          disabled={!newItem.trim()}
          className="px-3 py-1.5 text-sm rounded bg-blue-600 hover:bg-blue-500 text-white disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
        >
          Add
        </button>
      </div>
    </div>
  );
}

export function PermissionsPage() {
  const [projectScope, setProjectScope] = useState<string | null>(null);
  const [, setClaudePerms] = useState<ClaudePermissions | null>(null);
  const [claudeAllow, setClaudeAllow] = useState<string[]>([]);
  const [claudeDeny, setClaudeDeny] = useState<string[]>([]);
  const [skipDangerous, setSkipDangerous] = useState(false);

  const [additionalDirectories, setAdditionalDirectories] = useState<string[]>([]);
  const [defaultMode, setDefaultMode] = useState("");
  const [alwaysThinking, setAlwaysThinking] = useState(false);
  const [autoMemory, setAutoMemory] = useState(true);
  const [includeGitInstructions, setIncludeGitInstructions] = useState(true);
  const [disableAllHooks, setDisableAllHooks] = useState(false);
  const [disableAgentView, setDisableAgentView] = useState(false);
  const [disableSkillShellExecution, setDisableSkillShellExecution] = useState(false);
  const [disableRemoteControl, setDisableRemoteControl] = useState(false);
  const [fastModePerSessionOptIn, setFastModePerSessionOptIn] = useState(false);
  const [respectGitignore, setRespectGitignore] = useState(true);
  const [showThinkingSummaries, setShowThinkingSummaries] = useState(false);
  const [effortLevel, setEffortLevel] = useState("");
  const [model, setModel] = useState("");
  const [autoUpdatesChannel, setAutoUpdatesChannel] = useState("");
  const [editorMode, setEditorMode] = useState("");
  const [viewMode, setViewMode] = useState("");
  const [defaultShell, setDefaultShell] = useState("");
  const [plansDirectory, setPlansDirectory] = useState("");
  const [cleanupPeriodDays, setCleanupPeriodDays] = useState("");
  const [claudeMdExcludes, setClaudeMdExcludes] = useState<string[]>([]);
  const [availableModels, setAvailableModels] = useState<string[]>([]);

  const [projectSettingsAllow, setProjectSettingsAllow] = useState<string[]>([]);
  const [projectSettingsDeny, setProjectSettingsDeny] = useState<string[]>([]);
  const [projectSettingsLocalAllow, setProjectSettingsLocalAllow] = useState<string[]>([]);
  const [projectSettingsLocalDeny, setProjectSettingsLocalDeny] = useState<string[]>([]);

  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [claudeSaving, setClaudeSaving] = useState(false);
  const [projectClaudeSavingSettings, setProjectClaudeSavingSettings] = useState(false);
  const [projectClaudeSavingSettingsLocal, setProjectClaudeSavingSettingsLocal] = useState(false);

  const [rawSettings, setRawSettings] = useState("");
  const [rawSettingsLoading, setRawSettingsLoading] = useState(false);

  const [claudeMcpConfig, setClaudeMcpConfig] = useState<AdapterGlobalConfig | null>(null);

  const [editJsonClaudeGlobal, setEditJsonClaudeGlobal] = useState(false);
  const [editJsonClaudeProjectSettings, setEditJsonClaudeProjectSettings] = useState(false);
  const [editJsonClaudeProjectSettingsLocal, setEditJsonClaudeProjectSettingsLocal] = useState(false);
  const [jsonDraftClaudeGlobal, setJsonDraftClaudeGlobal] = useState("");
  const [jsonDraftClaudeProjectSettings, setJsonDraftClaudeProjectSettings] = useState("");
  const [jsonDraftClaudeProjectSettingsLocal, setJsonDraftClaudeProjectSettingsLocal] = useState("");
  const [jsonError, setJsonError] = useState<string | null>(null);

  function buildClaudePayload(): ClaudePermissions {
    const cleanupNum = cleanupPeriodDays.trim() === "" ? undefined : Number(cleanupPeriodDays);
    return {
      allow: claudeAllow,
      deny: claudeDeny,
      enabled_plugins: {},
      skip_dangerous_mode: skipDangerous,
      additional_directories: additionalDirectories,
      default_mode: defaultMode || undefined,
      always_thinking_enabled: alwaysThinking,
      auto_memory_enabled: autoMemory,
      include_git_instructions: includeGitInstructions,
      disable_all_hooks: disableAllHooks,
      disable_agent_view: disableAgentView,
      disable_skill_shell_execution: disableSkillShellExecution,
      disable_remote_control: disableRemoteControl,
      fast_mode_per_session_opt_in: fastModePerSessionOptIn,
      respect_gitignore: respectGitignore,
      show_thinking_summaries: showThinkingSummaries,
      effort_level: effortLevel || undefined,
      model: model.trim() || undefined,
      auto_updates_channel: autoUpdatesChannel || undefined,
      editor_mode: editorMode || undefined,
      view_mode: viewMode || undefined,
      default_shell: defaultShell || undefined,
      plans_directory: plansDirectory.trim() || undefined,
      cleanup_period_days:
        cleanupNum != null && Number.isFinite(cleanupNum) && cleanupNum >= 0
          ? Math.floor(cleanupNum)
          : undefined,
      claude_md_excludes: claudeMdExcludes,
      available_models: availableModels,
    };
  }

  function buildClaudeJson() {
    const payload = buildClaudePayload();
    const obj: Record<string, unknown> = {
      permissions: {
        allow: payload.allow,
        deny: payload.deny,
        ...(payload.additional_directories && payload.additional_directories.length > 0
          ? { additionalDirectories: payload.additional_directories }
          : {}),
        ...(payload.default_mode ? { defaultMode: payload.default_mode } : {}),
      },
      skipDangerousModePermissionPrompt: payload.skip_dangerous_mode,
    };
    const camelMap: [keyof ClaudePermissions, string][] = [
      ["always_thinking_enabled", "alwaysThinkingEnabled"],
      ["auto_memory_enabled", "autoMemoryEnabled"],
      ["include_git_instructions", "includeGitInstructions"],
      ["disable_all_hooks", "disableAllHooks"],
      ["disable_agent_view", "disableAgentView"],
      ["disable_skill_shell_execution", "disableSkillShellExecution"],
      ["disable_remote_control", "disableRemoteControl"],
      ["fast_mode_per_session_opt_in", "fastModePerSessionOptIn"],
      ["respect_gitignore", "respectGitignore"],
      ["show_thinking_summaries", "showThinkingSummaries"],
      ["effort_level", "effortLevel"],
      ["model", "model"],
      ["auto_updates_channel", "autoUpdatesChannel"],
      ["editor_mode", "editorMode"],
      ["view_mode", "viewMode"],
      ["default_shell", "defaultShell"],
      ["plans_directory", "plansDirectory"],
      ["cleanup_period_days", "cleanupPeriodDays"],
    ];
    for (const [k, jsonKey] of camelMap) {
      const v = payload[k];
      if (v !== undefined && v !== "") obj[jsonKey] = v;
    }
    if (payload.claude_md_excludes && payload.claude_md_excludes.length > 0)
      obj.claudeMdExcludes = payload.claude_md_excludes;
    if (payload.available_models && payload.available_models.length > 0)
      obj.availableModels = payload.available_models;
    return JSON.stringify(obj, null, 2);
  }

  function openJsonClaudeGlobal() {
    setJsonDraftClaudeGlobal(buildClaudeJson());
    setEditJsonClaudeGlobal(true);
    setJsonError(null);
  }

  function openJsonClaudeProjectSettings() {
    setJsonDraftClaudeProjectSettings(
      JSON.stringify({ permissions: { allow: projectSettingsAllow, deny: projectSettingsDeny } }, null, 2)
    );
    setEditJsonClaudeProjectSettings(true);
    setJsonError(null);
  }

  function openJsonClaudeProjectSettingsLocal() {
    setJsonDraftClaudeProjectSettingsLocal(
      JSON.stringify(
        { permissions: { allow: projectSettingsLocalAllow, deny: projectSettingsLocalDeny } },
        null,
        2
      )
    );
    setEditJsonClaudeProjectSettingsLocal(true);
    setJsonError(null);
  }

  const loadRawSettings = useCallback(async () => {
    setRawSettingsLoading(true);
    try {
      const text = await readClaudeSettings();
      try {
        const parsed = JSON.parse(text || "{}");
        setRawSettings(JSON.stringify(parsed, null, 2));
      } catch {
        setRawSettings(text);
      }
    } catch {
      setRawSettings("{}");
    } finally {
      setRawSettingsLoading(false);
    }
  }, []);

  const loadClaudeMcpConfig = useCallback(async () => {
    try {
      const result = await invoke<{ mcp_servers: string[]; has_config: boolean }>(
        "get_global_config",
        { adapterId: "claude-code" }
      );
      setClaudeMcpConfig({
        id: "claude-code",
        name: "Claude Code",
        color: "#DA7756",
        globalPath: "~/.claude.json",
        mcpServers: result.mcp_servers,
        hasConfig: result.has_config,
      });
    } catch {
      setClaudeMcpConfig({
        id: "claude-code",
        name: "Claude Code",
        color: "#DA7756",
        globalPath: "~/.claude.json",
        mcpServers: [],
        hasConfig: false,
      });
    }
  }, []);

  async function loadData() {
    setLoading(true);
    setError(null);
    try {
      const [cp] = await Promise.all([
        getClaudePermissions(),
        loadRawSettings(),
        loadClaudeMcpConfig(),
      ]);
      setClaudePerms(cp);
      setClaudeAllow([...cp.allow]);
      setClaudeDeny([...cp.deny]);
      setSkipDangerous(cp.skip_dangerous_mode);

      setAdditionalDirectories([...(cp.additional_directories ?? [])]);
      setDefaultMode(cp.default_mode ?? "");
      setAlwaysThinking(cp.always_thinking_enabled ?? false);
      setAutoMemory(cp.auto_memory_enabled ?? true);
      setIncludeGitInstructions(cp.include_git_instructions ?? true);
      setDisableAllHooks(cp.disable_all_hooks ?? false);
      setDisableAgentView(cp.disable_agent_view ?? false);
      setDisableSkillShellExecution(cp.disable_skill_shell_execution ?? false);
      setDisableRemoteControl(cp.disable_remote_control ?? false);
      setFastModePerSessionOptIn(cp.fast_mode_per_session_opt_in ?? false);
      setRespectGitignore(cp.respect_gitignore ?? true);
      setShowThinkingSummaries(cp.show_thinking_summaries ?? false);
      setEffortLevel(cp.effort_level ?? "");
      setModel(cp.model ?? "");
      setAutoUpdatesChannel(cp.auto_updates_channel ?? "");
      setEditorMode(cp.editor_mode ?? "");
      setViewMode(cp.view_mode ?? "");
      setDefaultShell(cp.default_shell ?? "");
      setPlansDirectory(cp.plans_directory ?? "");
      setCleanupPeriodDays(cp.cleanup_period_days != null ? String(cp.cleanup_period_days) : "");
      setClaudeMdExcludes([...(cp.claude_md_excludes ?? [])]);
      setAvailableModels([...(cp.available_models ?? [])]);

      if (projectScope) {
        const proj = await getClaudeProjectPermissions(projectScope);
        setProjectSettingsAllow([...proj.settings.allow]);
        setProjectSettingsDeny([...proj.settings.deny]);
        setProjectSettingsLocalAllow([...proj.settings_local.allow]);
        setProjectSettingsLocalDeny([...proj.settings_local.deny]);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    loadData();
  }, [projectScope]);

  async function handleClaudeSave() {
    if (!window.confirm("Save Claude Code permissions? This directly affects IDE behavior.")) return;
    setClaudeSaving(true);
    try {
      await updateClaudePermissions(buildClaudePayload());
      await loadData();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setClaudeSaving(false);
    }
  }

  async function handleProjectSettingsSave() {
    if (!projectScope) return;
    if (!window.confirm("Save project settings.json permissions?")) return;
    setProjectClaudeSavingSettings(true);
    try {
      await updateClaudeProjectPermissions(projectScope, "settings", projectSettingsAllow, projectSettingsDeny);
      await loadData();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setProjectClaudeSavingSettings(false);
    }
  }

  async function handleProjectSettingsLocalSave() {
    if (!projectScope) return;
    if (!window.confirm("Save project settings.local.json permissions?")) return;
    setProjectClaudeSavingSettingsLocal(true);
    try {
      await updateClaudeProjectPermissions(projectScope, "settings_local", projectSettingsLocalAllow, projectSettingsLocalDeny);
      await loadData();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setProjectClaudeSavingSettingsLocal(false);
    }
  }

  function parseClaudeJson(text: string): {
    allow: string[];
    deny: string[];
    skipDangerousModePermissionPrompt?: boolean;
    payload: ClaudePermissions;
  } {
    const data = JSON.parse(text) as Record<string, unknown>;
    const perms = data.permissions as Record<string, unknown> | undefined;
    if (!perms || !Array.isArray(perms.allow) || !Array.isArray(perms.deny)) {
      throw new Error("JSON must have permissions.allow and permissions.deny arrays");
    }
    const allow = perms.allow.map(String);
    const deny = perms.deny.map(String);
    const skipDangerous = typeof data.skipDangerousModePermissionPrompt === "boolean"
      ? data.skipDangerousModePermissionPrompt
      : false;

    const strArr = (v: unknown): string[] =>
      Array.isArray(v) ? v.map(String) : [];
    const optStr = (v: unknown): string | undefined =>
      typeof v === "string" && v.length > 0 ? v : undefined;
    const optBool = (v: unknown): boolean | undefined =>
      typeof v === "boolean" ? v : undefined;
    const optNum = (v: unknown): number | undefined =>
      typeof v === "number" && Number.isFinite(v) ? v : undefined;

    const payload: ClaudePermissions = {
      allow,
      deny,
      enabled_plugins: {},
      skip_dangerous_mode: skipDangerous,
      additional_directories: strArr(perms.additionalDirectories),
      default_mode: optStr(perms.defaultMode),
      always_thinking_enabled: optBool(data.alwaysThinkingEnabled),
      auto_memory_enabled: optBool(data.autoMemoryEnabled),
      include_git_instructions: optBool(data.includeGitInstructions),
      disable_all_hooks: optBool(data.disableAllHooks),
      disable_agent_view: optBool(data.disableAgentView),
      disable_skill_shell_execution: optBool(data.disableSkillShellExecution),
      disable_remote_control: optBool(data.disableRemoteControl),
      fast_mode_per_session_opt_in: optBool(data.fastModePerSessionOptIn),
      respect_gitignore: optBool(data.respectGitignore),
      show_thinking_summaries: optBool(data.showThinkingSummaries),
      effort_level: optStr(data.effortLevel),
      model: optStr(data.model),
      auto_updates_channel: optStr(data.autoUpdatesChannel),
      editor_mode: optStr(data.editorMode),
      view_mode: optStr(data.viewMode),
      default_shell: optStr(data.defaultShell),
      plans_directory: optStr(data.plansDirectory),
      cleanup_period_days: optNum(data.cleanupPeriodDays),
      claude_md_excludes: strArr(data.claudeMdExcludes),
      available_models: strArr(data.availableModels),
    };
    return { allow, deny, skipDangerousModePermissionPrompt: skipDangerous, payload };
  }

  async function handleClaudeSaveFromJson() {
    setJsonError(null);
    try {
      const { payload } = parseClaudeJson(jsonDraftClaudeGlobal);
      if (!window.confirm("Save Claude Code permissions? This directly affects IDE behavior.")) return;
      setClaudeSaving(true);
      await updateClaudePermissions(payload);
      await loadData();
      setEditJsonClaudeGlobal(false);
    } catch (err) {
      setJsonError(err instanceof Error ? err.message : String(err));
    } finally {
      setClaudeSaving(false);
    }
  }

  async function handleProjectSettingsSaveFromJson() {
    if (!projectScope) return;
    setJsonError(null);
    try {
      const { allow, deny } = parseClaudeJson(jsonDraftClaudeProjectSettings);
      if (!window.confirm("Save project settings.json permissions?")) return;
      setProjectClaudeSavingSettings(true);
      await updateClaudeProjectPermissions(projectScope, "settings", allow, deny);
      setProjectSettingsAllow(allow);
      setProjectSettingsDeny(deny);
      await loadData();
      setEditJsonClaudeProjectSettings(false);
    } catch (err) {
      setJsonError(err instanceof Error ? err.message : String(err));
    } finally {
      setProjectClaudeSavingSettings(false);
    }
  }

  async function handleProjectSettingsLocalSaveFromJson() {
    if (!projectScope) return;
    setJsonError(null);
    try {
      const { allow, deny } = parseClaudeJson(jsonDraftClaudeProjectSettingsLocal);
      if (!window.confirm("Save project settings.local.json permissions?")) return;
      setProjectClaudeSavingSettingsLocal(true);
      await updateClaudeProjectPermissions(projectScope, "settings_local", allow, deny);
      setProjectSettingsLocalAllow(allow);
      setProjectSettingsLocalDeny(deny);
      await loadData();
      setEditJsonClaudeProjectSettingsLocal(false);
    } catch (err) {
      setJsonError(err instanceof Error ? err.message : String(err));
    } finally {
      setProjectClaudeSavingSettingsLocal(false);
    }
  }

  if (loading) {
    return (
      <div className="p-6 flex items-center justify-center h-64">
        <p className="text-text-secondary">Loading permissions…</p>
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-6">
        <div className="bg-red-900/30 border border-red-700 rounded-lg p-4 mb-4">
          <p className="text-red-300 text-sm">{error}</p>
        </div>
        <button
          onClick={loadData}
          className="px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded transition-colors"
        >
          Retry
        </button>
      </div>
    );
  }

  const isGlobal = projectScope == null;

  return (
    <div className="p-6 max-w-6xl">
      <div className="flex items-center justify-between flex-wrap gap-4 mb-4">
        <div>
          <h1 className="text-2xl font-bold text-text-primary mb-2">
            Permissions &amp; Control
          </h1>
          <p className="text-text-secondary text-sm">
            Manage permissions and global MCP config for Claude Code.
          </p>
        </div>
        <ProjectScopeSelector value={projectScope} onChange={setProjectScope} />
      </div>

      <div className="bg-amber-900/30 border border-amber-600/50 rounded-lg px-4 py-3 mb-6 flex items-start gap-3">
        <span className="text-amber-400 text-lg leading-none mt-0.5">⚠</span>
        <p className="text-amber-200 text-sm">
          Changes directly affect IDE behavior. Save with care.
        </p>
      </div>

      {isGlobal && claudeMcpConfig && (
        <div className="bg-app-card border border-border rounded-lg p-5 mb-6">
          <h2 className="text-lg font-semibold text-text-primary mb-4">
            Global MCP Configuration
          </h2>
          <AdapterConfigSection
            config={claudeMcpConfig}
            onRefresh={loadClaudeMcpConfig}
          />
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="bg-app-card border border-border rounded-lg p-5">
          <h2 className="text-lg font-semibold text-text-primary mb-1">
            Claude Code
          </h2>
          {isGlobal ? (
            <>
              <DebugPath path="~/.claude/settings.json" className="mb-4" />
              <div className="mb-3">
                {editJsonClaudeGlobal ? (
                  <button
                    type="button"
                    onClick={() => { setEditJsonClaudeGlobal(false); setJsonError(null); }}
                    className="text-sm text-blue-400 hover:text-blue-300"
                  >
                    ← Form view
                  </button>
                ) : (
                  <button
                    type="button"
                    onClick={openJsonClaudeGlobal}
                    className="text-sm text-blue-400 hover:text-blue-300"
                  >
                    Edit in JSON
                  </button>
                )}
              </div>

              {editJsonClaudeGlobal ? (
                <div className="space-y-2 mb-4">
                  {jsonError && (
                    <p className="text-sm text-red-400">{jsonError}</p>
                  )}
                  <textarea
                    value={jsonDraftClaudeGlobal}
                    onChange={(e) => setJsonDraftClaudeGlobal(e.target.value)}
                    className="w-full h-64 font-mono text-xs bg-[#13141a] border border-border rounded px-3 py-2 text-text-primary focus:outline-none focus:border-blue-500"
                    spellCheck={false}
                  />
                  <button
                    onClick={handleClaudeSaveFromJson}
                    disabled={claudeSaving}
                    className="w-full mt-2 px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                  >
                    {claudeSaving ? "Saving…" : "Save Claude Permissions"}
                  </button>
                </div>
              ) : (
                <>
              <PermissionList
                label="Allowed Permissions"
                items={claudeAllow}
                onRemove={(item) => setClaudeAllow((prev) => prev.filter((i) => i !== item))}
                onAdd={(item) => setClaudeAllow((prev) => [...prev, item])}
              />

              <PermissionList
                label="Denied Permissions"
                items={claudeDeny}
                onRemove={(item) => setClaudeDeny((prev) => prev.filter((i) => i !== item))}
                onAdd={(item) => setClaudeDeny((prev) => [...prev, item])}
              />

              <div className="border-t border-border pt-4 mt-4 mb-4">
            <label className="flex items-center justify-between cursor-pointer">
              <div>
                <span className="text-sm font-medium text-text-primary">
                  Dangerous Mode
                </span>
                <p className="text-xs text-text-muted mt-0.5">
                  Skip permission prompt for dangerous operations
                </p>
              </div>
              <div
                className={`relative w-10 h-5 rounded-full transition-colors ${
                  skipDangerous ? "bg-red-500" : "bg-[#2a2b36]"
                }`}
                onClick={() => setSkipDangerous((v) => !v)}
              >
                <div
                  className={`absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
                    skipDangerous ? "translate-x-5" : "translate-x-0.5"
                  }`}
                />
              </div>
            </label>
          </div>

              <div className="border-t border-border pt-4 mt-4 mb-2">
                <h4 className="text-sm font-medium text-text-secondary mb-2">Permissions</h4>
                <SelectRow
                  label="Default mode"
                  info="Default permission mode applied at startup."
                  value={defaultMode}
                  onChange={setDefaultMode}
                  options={[
                    { value: "", label: "(default)" },
                    { value: "default", label: "default" },
                    { value: "acceptEdits", label: "acceptEdits" },
                    { value: "plan", label: "plan" },
                    { value: "auto", label: "auto" },
                    { value: "dontAsk", label: "dontAsk" },
                    { value: "bypassPermissions", label: "bypassPermissions" },
                  ]}
                />
                <div className="mt-3">
                  <PermissionList
                    label="Additional Directories"
                    info="Extra working directories Claude is allowed to read and edit."
                    items={additionalDirectories}
                    onRemove={(item) =>
                      setAdditionalDirectories((prev) => prev.filter((i) => i !== item))
                    }
                    onAdd={(item) => setAdditionalDirectories((prev) => [...prev, item])}
                    placeholder="e.g. ~/projects/shared"
                  />
                </div>
              </div>

              <div className="border-t border-border pt-4 mt-2 mb-2">
                <h4 className="text-sm font-medium text-text-secondary mb-2">Behavior</h4>
                <SwitchRow
                  label="Always thinking"
                  info="Force extended thinking on for every session."
                  checked={alwaysThinking}
                  onChange={setAlwaysThinking}
                />
                <SwitchRow
                  label="Auto memory"
                  info="Let Claude read and write its auto-memory store."
                  checked={autoMemory}
                  onChange={setAutoMemory}
                />
                <SwitchRow
                  label="Include git instructions"
                  info="Inject git workflow instructions into the system prompt."
                  checked={includeGitInstructions}
                  onChange={setIncludeGitInstructions}
                />
                <SelectRow
                  label="Effort level"
                  info="How much effort Claude spends per response, persisted across sessions."
                  value={effortLevel}
                  onChange={setEffortLevel}
                  options={[
                    { value: "", label: "(default)" },
                    { value: "low", label: "low" },
                    { value: "medium", label: "medium" },
                    { value: "high", label: "high" },
                    { value: "xhigh", label: "xhigh" },
                    { value: "max", label: "max" },
                    { value: "auto", label: "auto" },
                  ]}
                />
                <TextRow
                  label="Model"
                  info="Override the default model ID (e.g. claude-sonnet-4-6)."
                  value={model}
                  onChange={setModel}
                  placeholder="(default)"
                  datalistId="claude-model-suggestions"
                  datalistOptions={[
                    "claude-opus-4-7",
                    "claude-sonnet-4-6",
                    "claude-haiku-4-5-20251001",
                  ]}
                />
              </div>

              <div className="border-t border-border pt-4 mt-2 mb-2">
                <h4 className="text-sm font-medium text-text-secondary mb-2">Safety &amp; Hooks</h4>
                <SwitchRow
                  label="Disable all hooks"
                  info="Kill switch for every hook and the custom status line."
                  checked={disableAllHooks}
                  onChange={setDisableAllHooks}
                />
                <SwitchRow
                  label="Disable agent view"
                  info="Turn off background agents and the agent view."
                  checked={disableAgentView}
                  onChange={setDisableAgentView}
                />
                <SwitchRow
                  label="Disable skill shell execution"
                  info="Block inline shell execution in skills and custom commands."
                  checked={disableSkillShellExecution}
                  onChange={setDisableSkillShellExecution}
                />
                <SwitchRow
                  label="Disable Remote Control"
                  info="Disable the Remote Control feature (v2.1.128+)."
                  checked={disableRemoteControl}
                  onChange={setDisableRemoteControl}
                />
                <SwitchRow
                  label="Fast mode per-session opt-in"
                  info="Don't persist fast mode; require /fast every session."
                  checked={fastModePerSessionOptIn}
                  onChange={setFastModePerSessionOptIn}
                />
              </div>

              <div className="border-t border-border pt-4 mt-2 mb-2">
                <h4 className="text-sm font-medium text-text-secondary mb-2">Files</h4>
                <SwitchRow
                  label="Respect .gitignore"
                  info="@ file picker respects .gitignore patterns."
                  checked={respectGitignore}
                  onChange={setRespectGitignore}
                />
                <div className="mt-3">
                  <PermissionList
                    label="CLAUDE.md excludes"
                    info="Glob patterns of CLAUDE.md files to skip when loading memory."
                    items={claudeMdExcludes}
                    onRemove={(item) =>
                      setClaudeMdExcludes((prev) => prev.filter((i) => i !== item))
                    }
                    onAdd={(item) => setClaudeMdExcludes((prev) => [...prev, item])}
                    placeholder="e.g. **/vendor/**/CLAUDE.md"
                  />
                </div>
                <TextRow
                  label="Plans directory"
                  info="Where /plan files are stored (default ~/.claude/plans)."
                  value={plansDirectory}
                  onChange={setPlansDirectory}
                  placeholder="~/.claude/plans"
                />
              </div>

              <div className="border-t border-border pt-4 mt-2 mb-2">
                <h4 className="text-sm font-medium text-text-secondary mb-2">UI</h4>
                <SelectRow
                  label="Editor mode"
                  info="Key binding mode for the input prompt."
                  value={editorMode}
                  onChange={setEditorMode}
                  options={[
                    { value: "", label: "(default)" },
                    { value: "normal", label: "normal" },
                    { value: "vim", label: "vim" },
                  ]}
                />
                <SelectRow
                  label="View mode"
                  info="Default transcript view mode on startup."
                  value={viewMode}
                  onChange={setViewMode}
                  options={[
                    { value: "", label: "(default)" },
                    { value: "default", label: "default" },
                    { value: "verbose", label: "verbose" },
                    { value: "focus", label: "focus" },
                  ]}
                />
                <SelectRow
                  label="Default shell"
                  info="Default shell for the input box's ! commands."
                  value={defaultShell}
                  onChange={setDefaultShell}
                  options={[
                    { value: "", label: "(default)" },
                    { value: "bash", label: "bash" },
                    { value: "powershell", label: "powershell" },
                  ]}
                />
                <SwitchRow
                  label="Show thinking summaries"
                  info="Show extended-thinking summaries inline in transcripts."
                  checked={showThinkingSummaries}
                  onChange={setShowThinkingSummaries}
                />
              </div>

              <div className="border-t border-border pt-4 mt-2 mb-2">
                <h4 className="text-sm font-medium text-text-secondary mb-2">Updates</h4>
                <SelectRow
                  label="Auto-updates channel"
                  info="Release channel followed for auto-updates."
                  value={autoUpdatesChannel}
                  onChange={setAutoUpdatesChannel}
                  options={[
                    { value: "", label: "(default)" },
                    { value: "stable", label: "stable" },
                    { value: "latest", label: "latest" },
                  ]}
                />
                <NumberRow
                  label="Cleanup period (days)"
                  info="Days before session files are deleted at startup (default 30)."
                  value={cleanupPeriodDays}
                  onChange={setCleanupPeriodDays}
                  placeholder="30"
                />
              </div>

              <div className="border-t border-border pt-4 mt-2 mb-4">
                <PermissionList
                  label="Available models"
                  info="Restrict which models users can pick via /model. Empty means all models are allowed."
                  items={availableModels}
                  onRemove={(item) =>
                    setAvailableModels((prev) => prev.filter((i) => i !== item))
                  }
                  onAdd={(item) => setAvailableModels((prev) => [...prev, item])}
                  placeholder="e.g. claude-sonnet-4-6"
                  emptyText="No restriction — all models are allowed."
                />
              </div>

              <button
                onClick={handleClaudeSave}
                disabled={claudeSaving}
                className="w-full mt-2 px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium rounded transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {claudeSaving ? "Saving…" : "Save Claude Permissions"}
              </button>
                </>
              )}
            </>
          ) : (
            <>
              <div className="mb-4">
                <DebugPath path={`${projectScope}/.claude/settings.json`} className="mb-2" />
                <div className="mb-2">
                  {editJsonClaudeProjectSettings ? (
                    <button
                      type="button"
                      onClick={() => { setEditJsonClaudeProjectSettings(false); setJsonError(null); }}
                      className="text-sm text-blue-400 hover:text-blue-300"
                    >
                      ← Form view
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={openJsonClaudeProjectSettings}
                      className="text-sm text-blue-400 hover:text-blue-300"
                    >
                      Edit in JSON
                    </button>
                  )}
                </div>
                {editJsonClaudeProjectSettings ? (
                  <div className="space-y-2">
                    {jsonError && <p className="text-sm text-red-400">{jsonError}</p>}
                    <textarea
                      value={jsonDraftClaudeProjectSettings}
                      onChange={(e) => setJsonDraftClaudeProjectSettings(e.target.value)}
                      className="w-full h-48 font-mono text-xs bg-[#13141a] border border-border rounded px-3 py-2 text-text-primary focus:outline-none focus:border-blue-500"
                      spellCheck={false}
                    />
                    <button
                      onClick={handleProjectSettingsSaveFromJson}
                      disabled={projectClaudeSavingSettings}
                      className="w-full mt-2 px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium rounded transition-colors disabled:opacity-50"
                    >
                      {projectClaudeSavingSettings ? "Saving…" : "Save settings.json"}
                    </button>
                  </div>
                ) : (
                  <>
                    <PermissionList
                      label="Allowed Permissions"
                      items={projectSettingsAllow}
                      onRemove={(item) => setProjectSettingsAllow((prev) => prev.filter((i) => i !== item))}
                      onAdd={(item) => setProjectSettingsAllow((prev) => [...prev, item])}
                    />
                    <PermissionList
                      label="Denied Permissions"
                      items={projectSettingsDeny}
                      onRemove={(item) => setProjectSettingsDeny((prev) => prev.filter((i) => i !== item))}
                      onAdd={(item) => setProjectSettingsDeny((prev) => [...prev, item])}
                    />
                    <button
                      onClick={handleProjectSettingsSave}
                      disabled={projectClaudeSavingSettings}
                      className="w-full mt-2 px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium rounded transition-colors disabled:opacity-50"
                    >
                      {projectClaudeSavingSettings ? "Saving…" : "Save settings.json"}
                    </button>
                  </>
                )}
              </div>
              <div className="border-t border-border pt-4">
                <DebugPath path={`${projectScope}/.claude/settings.local.json`} className="mb-2" />
                <div className="mb-2">
                  {editJsonClaudeProjectSettingsLocal ? (
                    <button
                      type="button"
                      onClick={() => { setEditJsonClaudeProjectSettingsLocal(false); setJsonError(null); }}
                      className="text-sm text-blue-400 hover:text-blue-300"
                    >
                      ← Form view
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={openJsonClaudeProjectSettingsLocal}
                      className="text-sm text-blue-400 hover:text-blue-300"
                    >
                      Edit in JSON
                    </button>
                  )}
                </div>
                {editJsonClaudeProjectSettingsLocal ? (
                  <div className="space-y-2">
                    {jsonError && <p className="text-sm text-red-400">{jsonError}</p>}
                    <textarea
                      value={jsonDraftClaudeProjectSettingsLocal}
                      onChange={(e) => setJsonDraftClaudeProjectSettingsLocal(e.target.value)}
                      className="w-full h-48 font-mono text-xs bg-[#13141a] border border-border rounded px-3 py-2 text-text-primary focus:outline-none focus:border-blue-500"
                      spellCheck={false}
                    />
                    <button
                      onClick={handleProjectSettingsLocalSaveFromJson}
                      disabled={projectClaudeSavingSettingsLocal}
                      className="w-full mt-2 px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium rounded transition-colors disabled:opacity-50"
                    >
                      {projectClaudeSavingSettingsLocal ? "Saving…" : "Save settings.local.json"}
                    </button>
                  </div>
                ) : (
                  <>
                    <PermissionList
                      label="Allowed Permissions"
                      items={projectSettingsLocalAllow}
                      onRemove={(item) => setProjectSettingsLocalAllow((prev) => prev.filter((i) => i !== item))}
                      onAdd={(item) => setProjectSettingsLocalAllow((prev) => [...prev, item])}
                    />
                    <PermissionList
                      label="Denied Permissions"
                      items={projectSettingsLocalDeny}
                      onRemove={(item) => setProjectSettingsLocalDeny((prev) => prev.filter((i) => i !== item))}
                      onAdd={(item) => setProjectSettingsLocalDeny((prev) => [...prev, item])}
                    />
                    <button
                      onClick={handleProjectSettingsLocalSave}
                      disabled={projectClaudeSavingSettingsLocal}
                      className="w-full mt-2 px-4 py-2 bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium rounded transition-colors disabled:opacity-50"
                    >
                      {projectClaudeSavingSettingsLocal ? "Saving…" : "Save settings.local.json"}
                    </button>
                  </>
                )}
              </div>
            </>
          )}
        </div>

        <div className="bg-app-card border border-border rounded-lg p-5 flex flex-col">
          <div className="flex items-center justify-between mb-3">
            <div>
              <h2 className="text-lg font-semibold text-text-primary mb-0.5">Settings Preview</h2>
              <DebugPath path="~/.claude/settings.json" />
            </div>
            <button
              onClick={loadRawSettings}
              disabled={rawSettingsLoading}
              className="text-xs text-accent-blue hover:underline disabled:opacity-50"
            >
              {rawSettingsLoading ? "Loading..." : "Refresh"}
            </button>
          </div>
          <pre className="flex-1 overflow-auto bg-[#13141a] border border-border rounded-lg px-4 py-3 font-mono text-xs text-text-primary whitespace-pre-wrap break-words select-text min-h-[200px]">
            {rawSettingsLoading ? "Loading..." : rawSettings || "{}"}
          </pre>
        </div>
      </div>
    </div>
  );
}
