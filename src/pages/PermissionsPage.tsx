import { useState, useEffect, useCallback, useRef } from "react";
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
  const [highlightCleanup, setHighlightCleanup] = useState(false);
  // Serialized snapshot of the settings as last loaded/saved. Comparing the
  // live payload against it is simpler and less error-prone than tracking a
  // dirty flag across ~30 individual field setters.
  const [claudeBaseline, setClaudeBaseline] = useState<string | null>(null);
  // Pending navigation held while the unsaved-changes modal is open.
  const [pendingNav, setPendingNav] = useState<(() => void) | null>(null);
  // Set while replaying a confirmed navigation so the guard ignores that click.
  const pendingNavRef = useRef(false);

  // Deep link support: `/adapters/claude-code/permissions#cleanup-period`
  // scrolls to the retention field and flashes it, so a link from Analytics
  // lands on the right control instead of the top of a long page.
  //
  // The page renders a loading screen first, so the target element does not
  // exist on mount; poll briefly until it appears rather than firing once.
  useEffect(() => {
    if (window.location.hash !== "#cleanup-period") return;
    let cancelled = false;
    let clearHighlight: ReturnType<typeof setTimeout> | undefined;
    let retry: ReturnType<typeof setTimeout> | undefined;
    const deadline = Date.now() + 10_000;

    const tryScroll = () => {
      if (cancelled) return;
      const target = document.getElementById("cleanup-period");
      if (target) {
        target.scrollIntoView({ behavior: "smooth", block: "center" });
        setHighlightCleanup(true);
        clearHighlight = setTimeout(() => setHighlightCleanup(false), 3000);
        return;
      }
      if (Date.now() < deadline) retry = setTimeout(tryScroll, 100);
    };
    tryScroll();

    return () => {
      cancelled = true;
      if (clearHighlight) clearTimeout(clearHighlight);
      if (retry) clearTimeout(retry);
    };
  }, []);
  const [claudeMdExcludes, setClaudeMdExcludes] = useState<string[]>([]);
  const [availableModels, setAvailableModels] = useState<string[]>([]);
  const [statusLineEnabled, setStatusLineEnabled] = useState(false);
  const [statusLineCommand, setStatusLineCommand] = useState("");
  const [statusLinePadding, setStatusLinePadding] = useState<number | undefined>(undefined);

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
      status_line_command:
        statusLineEnabled && statusLineCommand.trim() ? statusLineCommand.trim() : undefined,
      status_line_padding: statusLineEnabled ? statusLinePadding : undefined,
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
    if (payload.status_line_command)
      obj.statusLine = {
        type: "command",
        command: payload.status_line_command,
        ...(payload.status_line_padding != null ? { padding: payload.status_line_padding } : {}),
      };
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
      setStatusLineEnabled(!!cp.status_line_command);
      if (cp.status_line_command) setStatusLineCommand(cp.status_line_command);
      setStatusLinePadding(cp.status_line_padding);

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

  // Snapshot the baseline once freshly loaded values are committed to state.
  // Runs after every load (including the reload following a save), so saving
  // clears the dirty state automatically.
  useEffect(() => {
    if (loading) return;
    setClaudeBaseline(JSON.stringify(buildClaudePayload()));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loading, projectScope]);

  // Dirty when the live payload differs from the last loaded/saved snapshot.
  const claudeDirty =
    !loading && claudeBaseline !== null &&
    JSON.stringify(buildClaudePayload()) !== claudeBaseline;

  const discardClaudeChanges = useCallback(() => {
    void loadData();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [projectScope]);

  // In-app navigation guard.
  //
  // Two constraints shape this:
  //  * The app uses BrowserRouter (not a data router), so `useBlocker` is
  //    unavailable; sidebar clicks are intercepted in the capture phase.
  //  * `window.confirm` is NON-BLOCKING inside a Tauri webview: it is
  //    intercepted and shown as a native async dialog, returning a Promise
  //    rather than a boolean. Using it here let navigation continue behind
  //    the dialog, so the answer arrived too late to matter. The prompt is
  //    therefore a normal in-app modal: the click is always cancelled first,
  //    and the navigation is replayed only if the user confirms.
  useEffect(() => {
    if (!claudeDirty) return;
    const onClickCapture = (event: MouseEvent) => {
      if (pendingNavRef.current) {
        pendingNavRef.current = false;
        return; // replayed click: the user already confirmed
      }
      if (event.defaultPrevented || event.button !== 0) return;
      const target = event.target as HTMLElement | null;
      const nav = target?.closest(".sidebar-item, a[href]");
      if (!nav || !(nav instanceof HTMLElement)) return;
      if (nav.getAttribute("href")?.startsWith("#")) return;
      if (nav.closest("[data-permissions-root]")) return;
      if (nav.classList.contains("active")) return;

      // Always stop this click; it is replayed after the user decides.
      event.preventDefault();
      event.stopPropagation();
      setPendingNav(() => () => {
        pendingNavRef.current = true;
        nav.click();
      });
    };
    document.addEventListener("click", onClickCapture, true);
    return () => document.removeEventListener("click", onClickCapture, true);
  }, [claudeDirty]);

  // Warn before leaving the page (window close / reload) with unsaved edits.
  useEffect(() => {
    if (!claudeDirty) return;
    const onBeforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  }, [claudeDirty]);


  // `silent` skips the extra confirm when the caller already asked (e.g. the
  // unsaved-changes modal). Note `window.confirm` is async inside a Tauri
  // webview, so it is awaited rather than used as a blocking boolean.
  async function handleClaudeSave(options?: { silent?: boolean }) {
    if (!options?.silent) {
      const ok = await Promise.resolve(
        window.confirm("Save Claude Code permissions? This directly affects IDE behavior."),
      );
      if (!ok) return;
    }
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
      status_line_command: optStr(
        (data.statusLine as Record<string, unknown> | undefined)?.command
      ),
      status_line_padding: optNum(
        (data.statusLine as Record<string, unknown> | undefined)?.padding
      ),
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
    <div data-permissions-root className={`p-6 max-w-6xl${claudeDirty ? " pb-24" : ""}`}>
      {pendingNav && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
          <div className="w-full max-w-md rounded-lg border border-border bg-[#12131a] p-5 shadow-xl">
            <h2 className="text-sm font-semibold text-text-primary mb-2">
              Unsaved permission changes
            </h2>
            <p className="text-xs text-text-secondary leading-relaxed mb-4">
              You have changes that have not been applied. Apply them now, discard
              them and continue, or stay on this page.
            </p>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setPendingNav(null)}
                className="px-3 py-1.5 text-xs rounded bg-[#1a1b23] text-text-secondary hover:text-text-primary hover:bg-[#22232e]"
              >
                Stay here
              </button>
              <button
                type="button"
                onClick={() => {
                  const go = pendingNav;
                  setPendingNav(null);
                  setClaudeBaseline(null); // suppress the guard during unmount
                  go?.();
                }}
                className="px-3 py-1.5 text-xs rounded bg-[#1a1b23] text-amber-300 hover:bg-[#22232e]"
              >
                Discard &amp; leave
              </button>
              <button
                type="button"
                disabled={claudeSaving}
                onClick={async () => {
                  const go = pendingNav;
                  setPendingNav(null);
                  await handleClaudeSave({ silent: true });
                  go?.();
                }}
                className="px-3 py-1.5 text-xs rounded bg-accent-blue text-white hover:bg-accent-blue/90 disabled:opacity-50"
              >
                {claudeSaving ? "Applying…" : "Apply & leave"}
              </button>
            </div>
          </div>
        </div>
      )}
      {/* Unsaved-changes bar: pinned to the bottom of the scroll container so
          it stays reachable no matter how far down the form the edit was. */}
      {claudeDirty && (
        <div className="fixed bottom-4 right-6 z-40 flex items-center gap-3 rounded-lg border border-accent-blue/40 bg-[#12131a] px-4 py-3 shadow-lg shadow-black/40">
          <span className="text-xs text-text-secondary">
            You have unsaved changes
          </span>
          <button
            type="button"
            onClick={discardClaudeChanges}
            disabled={claudeSaving}
            className="px-3 py-1.5 text-xs rounded bg-[#1a1b23] text-text-secondary hover:text-text-primary hover:bg-[#22232e] disabled:opacity-50"
          >
            Discard
          </button>
          <button
            type="button"
            onClick={() => void handleClaudeSave()}
            disabled={claudeSaving}
            className="px-3 py-1.5 text-xs rounded bg-accent-blue text-white hover:bg-accent-blue/90 disabled:opacity-50"
          >
            {claudeSaving ? "Applying…" : "Apply changes"}
          </button>
        </div>
      )}
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
                <h4 className="text-sm font-medium text-text-secondary mb-2 inline-flex items-center gap-2">
                  Status Line
                  <span className="text-[9px] font-semibold uppercase tracking-wider text-blue-400 bg-blue-500/20 rounded px-1.5 py-0.5">
                    New
                  </span>
                </h4>
                <SwitchRow
                  label="Custom status line"
                  info="Show a custom status line in the Claude Code terminal, rendered by the command below."
                  checked={statusLineEnabled}
                  onChange={(v) => {
                    setStatusLineEnabled(v);
                    if (v && !statusLineCommand.trim())
                      setStatusLineCommand("bash ~/.claude/statusline-command.sh");
                  }}
                />
                {statusLineEnabled && (
                  <TextRow
                    label="Command"
                    info="Shell command that prints the status line; receives session JSON on stdin. With the default command, AgentHarbor creates the script (model | git branch | session cost) if it doesn't exist yet."
                    value={statusLineCommand}
                    onChange={setStatusLineCommand}
                    placeholder="e.g. bash ~/.claude/statusline-command.sh"
                  />
                )}
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
                <div
                  id="cleanup-period"
                  className={
                    highlightCleanup
                      ? "rounded-md ring-2 ring-amber-400/70 bg-amber-500/5 transition-shadow"
                      : "transition-shadow"
                  }
                >
                  <NumberRow
                    label="Cleanup period (days)"
                    info="Days before Claude Code deletes session transcripts at startup (default 30). Those transcripts are the only source of token and cost history, so a short window permanently removes analytics data."
                    value={cleanupPeriodDays}
                    onChange={setCleanupPeriodDays}
                    placeholder="30"
                  />
                </div>
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
                onClick={() => void handleClaudeSave()}
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
