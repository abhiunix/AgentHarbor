import { useState, useEffect, useCallback } from "react";
import {
  getClaudePermissions,
  updateClaudePermissions,
  getClaudePolicy,
  updateClaudePolicy,
  getClaudeProjectPermissions,
  updateClaudeProjectPermissions,
  readClaudeSettings,
} from "../lib/tauri";
import type {
  ClaudePermissions,
  PolicyRestriction,
} from "../lib/tauri";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";
import { DebugPath } from "../components/common/DebugPath";

function PermissionList({
  label,
  items,
  onRemove,
  onAdd,
}: {
  label: string;
  items: string[];
  onRemove: (item: string) => void;
  onAdd: (item: string) => void;
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
      <h4 className="text-sm font-medium text-text-secondary mb-2">{label}</h4>
      <div className="space-y-1 mb-2">
        {items.length === 0 && (
          <p className="text-xs text-text-muted italic">None</p>
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
          placeholder="e.g. Bash(npm*)"
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
  const [policy, setPolicy] = useState<PolicyRestriction[]>([]);
  const [claudeAllow, setClaudeAllow] = useState<string[]>([]);
  const [claudeDeny, setClaudeDeny] = useState<string[]>([]);
  const [skipDangerous, setSkipDangerous] = useState(false);
  const [policyDraft, setPolicyDraft] = useState<PolicyRestriction[]>([]);

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

  const [editJsonClaudeGlobal, setEditJsonClaudeGlobal] = useState(false);
  const [editJsonClaudeProjectSettings, setEditJsonClaudeProjectSettings] = useState(false);
  const [editJsonClaudeProjectSettingsLocal, setEditJsonClaudeProjectSettingsLocal] = useState(false);
  const [jsonDraftClaudeGlobal, setJsonDraftClaudeGlobal] = useState("");
  const [jsonDraftClaudeProjectSettings, setJsonDraftClaudeProjectSettings] = useState("");
  const [jsonDraftClaudeProjectSettingsLocal, setJsonDraftClaudeProjectSettingsLocal] = useState("");
  const [jsonError, setJsonError] = useState<string | null>(null);

  function buildClaudeJson(allow: string[], deny: string[], skipDangerous: boolean) {
    return JSON.stringify(
      {
        permissions: { allow, deny },
        skipDangerousModePermissionPrompt: skipDangerous,
      },
      null,
      2
    );
  }

  function openJsonClaudeGlobal() {
    setJsonDraftClaudeGlobal(buildClaudeJson(claudeAllow, claudeDeny, skipDangerous));
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

  async function loadData() {
    setLoading(true);
    setError(null);
    try {
      const [cp, pol] = await Promise.all([
        getClaudePermissions(),
        getClaudePolicy(),
        loadRawSettings(),
      ]);
      setClaudePerms(cp);
      setClaudeAllow([...cp.allow]);
      setClaudeDeny([...cp.deny]);
      setSkipDangerous(cp.skip_dangerous_mode);

      setPolicy(pol);
      setPolicyDraft(pol.map((p) => ({ ...p })));

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
      await updateClaudePermissions(claudeAllow, claudeDeny, skipDangerous);
      for (const p of policyDraft) {
        const original = policy.find((o) => o.key === p.key);
        if (original && original.allowed !== p.allowed) {
          await updateClaudePolicy(p.key, p.allowed);
        }
      }
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

  function parseClaudeJson(text: string): { allow: string[]; deny: string[]; skipDangerousModePermissionPrompt?: boolean } {
    const data = JSON.parse(text) as Record<string, unknown>;
    const perms = data.permissions as Record<string, unknown> | undefined;
    if (!perms || !Array.isArray(perms.allow) || !Array.isArray(perms.deny)) {
      throw new Error("JSON must have permissions.allow and permissions.deny arrays");
    }
    const allow = perms.allow.every((x): x is string => typeof x === "string")
      ? (perms.allow as string[])
      : perms.allow.map(String);
    const deny = perms.deny.every((x): x is string => typeof x === "string")
      ? (perms.deny as string[])
      : perms.deny.map(String);
    const skipDangerous = typeof data.skipDangerousModePermissionPrompt === "boolean"
      ? data.skipDangerousModePermissionPrompt
      : false;
    return { allow, deny, skipDangerousModePermissionPrompt: skipDangerous };
  }

  async function handleClaudeSaveFromJson() {
    setJsonError(null);
    try {
      const { allow, deny, skipDangerousModePermissionPrompt } = parseClaudeJson(jsonDraftClaudeGlobal);
      if (!window.confirm("Save Claude Code permissions? This directly affects IDE behavior.")) return;
      setClaudeSaving(true);
      await updateClaudePermissions(allow, deny, skipDangerousModePermissionPrompt ?? false);
      setClaudeAllow(allow);
      setClaudeDeny(deny);
      setSkipDangerous(skipDangerousModePermissionPrompt ?? false);
      for (const p of policyDraft) {
        const original = policy.find((o) => o.key === p.key);
        if (original && original.allowed !== p.allowed) {
          await updateClaudePolicy(p.key, p.allowed);
        }
      }
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
            Permission &amp; Policy Controls
          </h1>
          <p className="text-text-secondary text-sm">
            Manage allowed and denied tool permissions for Claude Code.
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

              {policyDraft.length > 0 && (
                <div className="border-t border-border pt-4 mt-4 mb-4">
                  <h4 className="text-sm font-medium text-text-secondary mb-1">
                    Policy Restrictions
                  </h4>
                  <DebugPath path="~/.claude/policy-limits.json" className="mb-3" />
                  <div className="space-y-2">
                    {policyDraft.map((p, idx) => (
                      <label
                        key={p.key}
                        className="flex items-center justify-between cursor-pointer py-1"
                      >
                        <span className="text-sm text-text-primary font-mono">
                          {p.key}
                        </span>
                        <div
                          className={`relative w-10 h-5 rounded-full transition-colors ${
                            p.allowed ? "bg-green-500" : "bg-[#2a2b36]"
                          }`}
                          onClick={() =>
                            setPolicyDraft((prev) =>
                              prev.map((item, i) =>
                                i === idx ? { ...item, allowed: !item.allowed } : item
                              )
                            )
                          }
                        >
                          <div
                            className={`absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
                              p.allowed ? "translate-x-5" : "translate-x-0.5"
                            }`}
                          />
                        </div>
                      </label>
                    ))}
                  </div>
                </div>
              )}

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
