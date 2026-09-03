import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";

const CODEX_COLOR = "#10a37f";

interface CodexPermissionProfile {
  id: string;
  name: string;
  description: string;
  allowed: boolean;
}

interface CodexControlSnapshot {
  scope: string;
  sourcePath: string;
  model: string;
  modelReasoningEffort: string;
  approvalPolicy: string;
  sandboxMode: string;
  webSearch: boolean;
  networkAccess: boolean;
  permissionProfiles: CodexPermissionProfile[];
  warnings: string[];
  appServerAvailable: boolean;
}

interface CodexControlUpdates {
  approvalPolicy?: string;
  sandboxMode?: string;
  webSearch?: boolean;
  networkAccess?: boolean;
}

interface ControlForm {
  approvalPolicy: string;
  sandboxMode: string;
  webSearch: boolean;
  networkAccess: boolean;
}

const APPROVAL_OPTIONS = [
  {
    value: "untrusted",
    label: "Untrusted commands require approval",
  },
  {
    value: "on-request",
    label: "Codex asks when needed",
  },
  {
    value: "never",
    label: "Never ask for approval",
  },
];

const SANDBOX_OPTIONS = [
  {
    value: "read-only",
    label: "Read only",
  },
  {
    value: "workspace-write",
    label: "Workspace write",
  },
  {
    value: "danger-full-access",
    label: "Full access",
  },
];

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function snapshotToForm(snapshot: CodexControlSnapshot): ControlForm {
  return {
    approvalPolicy: snapshot.approvalPolicy,
    sandboxMode: snapshot.sandboxMode,
    webSearch: snapshot.webSearch,
    networkAccess: snapshot.networkAccess,
  };
}

function SelectField({
  id,
  label,
  value,
  options,
  disabled,
  onChange,
  help,
}: {
  id: string;
  label: string;
  value: string;
  options: Array<{ value: string; label: string }>;
  disabled: boolean;
  onChange: (value: string) => void;
  help: string;
}) {
  const hasKnownValue = options.some((option) => option.value === value);

  return (
    <div>
      <label
        htmlFor={id}
        className="block text-sm font-medium text-text-primary mb-1.5"
      >
        {label}
      </label>
      <select
        id={id}
        value={value}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value)}
        className="w-full px-3 py-2.5 bg-app-bg border border-border rounded-lg text-sm text-text-primary focus:outline-none focus:border-[#10a37f] disabled:opacity-50"
      >
        {!value && <option value="">Not configured</option>}
        {!hasKnownValue && value && (
          <option value={value}>Current custom value: {value}</option>
        )}
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      <p className="text-xs text-text-muted mt-1.5 leading-relaxed">{help}</p>
    </div>
  );
}

function ToggleField({
  label,
  description,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  description: string;
  checked: boolean;
  disabled: boolean;
  onChange: (value: boolean) => void;
}) {
  return (
    <div className="flex items-start justify-between gap-4 py-3">
      <div>
        <p className="text-sm font-medium text-text-primary">{label}</p>
        <p className="text-xs text-text-muted mt-1 leading-relaxed">
          {description}
        </p>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        disabled={disabled}
        onClick={() => onChange(!checked)}
        className={`relative mt-0.5 h-6 w-11 shrink-0 rounded-full transition-colors focus:outline-none focus:ring-2 focus:ring-[#10a37f]/50 disabled:opacity-50 ${
          checked ? "bg-[#10a37f]" : "bg-[#343641]"
        }`}
      >
        <span
          className={`absolute top-1 h-4 w-4 rounded-full bg-white transition-transform ${
            checked ? "translate-x-6" : "translate-x-1"
          }`}
        />
      </button>
    </div>
  );
}

export function CodexControlPage() {
  const [projectPath, setProjectPath] = useState<string | null>(null);
  const [snapshot, setSnapshot] = useState<CodexControlSnapshot | null>(null);
  const [form, setForm] = useState<ControlForm | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const requestId = useRef(0);
  const scopeVersion = useRef(0);

  const load = useCallback(async () => {
    const currentRequest = ++requestId.current;
    setLoading(true);
    setError(null);
    setSuccess(null);
    try {
      const next = await invoke<CodexControlSnapshot>(
        "get_codex_control_snapshot",
        {
          projectPath,
        },
      );
      if (currentRequest !== requestId.current) return;
      setSnapshot(next);
      setForm(snapshotToForm(next));
    } catch (loadError) {
      if (currentRequest !== requestId.current) return;
      setError(errorMessage(loadError));
    } finally {
      if (currentRequest === requestId.current) setLoading(false);
    }
  }, [projectPath]);

  useEffect(() => {
    void load();
  }, [load]);

  const updates = useMemo<CodexControlUpdates>(() => {
    if (!snapshot || !form) return {};
    const next: CodexControlUpdates = {};
    if (form.approvalPolicy !== snapshot.approvalPolicy) {
      next.approvalPolicy = form.approvalPolicy;
    }
    if (form.sandboxMode !== snapshot.sandboxMode) {
      next.sandboxMode = form.sandboxMode;
    }
    if (form.webSearch !== snapshot.webSearch) next.webSearch = form.webSearch;
    if (form.networkAccess !== snapshot.networkAccess) {
      next.networkAccess = form.networkAccess;
    }
    return next;
  }, [form, snapshot]);

  const dirty = Object.keys(updates).length > 0;

  function handleProjectPathChange(nextProjectPath: string | null) {
    if (nextProjectPath === projectPath || saving) return;
    if (
      dirty &&
      !window.confirm("Discard unsaved Codex control changes and switch scope?")
    ) {
      return;
    }
    scopeVersion.current += 1;
    requestId.current += 1;
    setProjectPath(nextProjectPath);
    setSnapshot(null);
    setForm(null);
    setLoading(true);
    setError(null);
    setSuccess(null);
  }

  function handleRefresh() {
    if (
      dirty &&
      !window.confirm(
        "Discard unsaved Codex control changes and refresh from disk?",
      )
    ) {
      return;
    }
    void load();
  }

  function updateFormField<Key extends keyof ControlForm>(
    key: Key,
    value: ControlForm[Key],
  ) {
    setForm((current) => (current ? { ...current, [key]: value } : current));
    setError(null);
    setSuccess(null);
  }

  async function handleSave() {
    if (!snapshot || !form || !dirty) return;
    const enablesHighRiskMode =
      updates.approvalPolicy === "never" ||
      updates.sandboxMode === "danger-full-access";
    if (
      enablesHighRiskMode &&
      !window.confirm(
        "Save a high-risk Codex configuration? This can remove approval prompts or file-system isolation for new sessions.",
      )
    ) {
      return;
    }
    const saveScopeVersion = scopeVersion.current;
    setSaving(true);
    setError(null);
    setSuccess(null);
    try {
      const refreshed = await invoke<CodexControlSnapshot>(
        "update_codex_control",
        {
          projectPath,
          updates,
        },
      );
      if (saveScopeVersion === scopeVersion.current) {
        setSnapshot(refreshed);
        setForm(snapshotToForm(refreshed));
        const wasOverridden = refreshed.warnings.some((warning) =>
          warning.startsWith("Codex saved the file, but"),
        );
        setSuccess(
          wasOverridden
            ? "Saved to the Codex configuration file."
            : "Saved. These defaults apply to new Codex sessions.",
        );
      }
    } catch (saveError) {
      if (saveScopeVersion === scopeVersion.current) {
        setError(errorMessage(saveError));
      }
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="h-full flex flex-col">
      <div className="p-6 border-b border-border flex items-start justify-between gap-4 flex-wrap">
        <div>
          <div className="flex items-center gap-2 mb-1">
            <span
              className="w-3 h-3 rounded-full"
              style={{ backgroundColor: CODEX_COLOR }}
            />
            <h1 className="text-2xl font-semibold text-text-primary">
              Codex Permissions &amp; Control
            </h1>
          </div>
          <p className="text-sm text-text-muted">
            Configure approval, sandbox, search, and network defaults.
          </p>
        </div>
        <ProjectScopeSelector
          value={projectPath}
          onChange={handleProjectPathChange}
        />
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        {loading ? (
          <div className="h-64 flex items-center justify-center text-text-muted">
            Loading Codex control settings...
          </div>
        ) : (
          <div className="max-w-4xl space-y-5">
            {error && (
              <div
                role="alert"
                aria-live="assertive"
                className="px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400"
              >
                {error}
              </div>
            )}

            {success && (
              <div
                role="status"
                aria-live="polite"
                className="px-4 py-3 bg-emerald-500/10 border border-emerald-500/30 rounded-lg text-sm text-emerald-400"
              >
                {success}
              </div>
            )}

            {snapshot && form && (
              <>
                <div className="bg-app-card border border-border rounded-lg px-4 py-3 flex items-center justify-between gap-4 flex-wrap">
                  <div>
                    <p className="text-xs uppercase tracking-wider text-text-muted">
                      Write target
                    </p>
                    <p className="text-sm font-mono text-text-primary mt-1 break-all">
                      {snapshot.sourcePath}
                    </p>
                  </div>
                  <div className="flex items-center gap-2">
                    <span className="px-2 py-1 rounded bg-[#2a2b36] text-xs text-text-secondary">
                      {snapshot.scope}
                    </span>
                    <span
                      className={`px-2 py-1 rounded text-xs ${
                        snapshot.appServerAvailable
                          ? "bg-emerald-500/15 text-emerald-400"
                          : "bg-amber-500/15 text-amber-400"
                      }`}
                    >
                      App Server{" "}
                      {snapshot.appServerAvailable
                        ? "available"
                        : "unavailable"}
                    </span>
                  </div>
                </div>

                {!snapshot.appServerAvailable && (
                  <div className="px-4 py-3 bg-amber-500/10 border border-amber-500/30 rounded-lg text-sm text-amber-300 leading-relaxed">
                    Codex App Server is unavailable. These values come from the
                    configuration file fallback, so permission profiles may be
                    incomplete.
                  </div>
                )}

                {snapshot.warnings.map((warning, index) => (
                  <div
                    key={`${warning}-${index}`}
                    className="px-4 py-3 bg-amber-500/10 border border-amber-500/30 rounded-lg text-sm text-amber-300"
                  >
                    {warning}
                  </div>
                ))}

                <div className="px-4 py-3 bg-amber-500/10 border border-amber-500/30 rounded-lg text-sm text-amber-200 leading-relaxed">
                  <p className="font-medium text-amber-300 mb-1">
                    Legacy sandbox setting and beta permission profiles
                  </p>
                  <p>
                    This page edits the legacy sandbox setting. Newer Codex App
                    Server versions also expose beta permission profiles. An
                    active profile can apply more specific runtime permissions,
                    so an already-running session may behave differently from
                    these saved defaults.
                  </p>
                </div>

                <div className="bg-app-card border border-border rounded-lg p-5">
                  <div className="mb-5">
                    <h2 className="text-lg font-semibold text-text-primary">
                      Execution safety
                    </h2>
                    <p className="text-xs text-text-muted mt-1">
                      These settings affect how new sessions approve commands
                      and limit file access.
                    </p>
                  </div>

                  <div className="grid grid-cols-1 md:grid-cols-2 gap-5">
                    <SelectField
                      id="codex-approval-policy"
                      label="Approval policy"
                      value={form.approvalPolicy}
                      options={APPROVAL_OPTIONS}
                      disabled={saving}
                      onChange={(approvalPolicy) =>
                        updateFormField("approvalPolicy", approvalPolicy)
                      }
                      help="On request lets Codex ask before higher-risk actions. The Never option removes approval prompts and should only be used in a trusted environment."
                    />
                    <SelectField
                      id="codex-sandbox-mode"
                      label="Sandbox mode"
                      value={form.sandboxMode}
                      options={SANDBOX_OPTIONS}
                      disabled={saving}
                      onChange={(sandboxMode) =>
                        updateFormField("sandboxMode", sandboxMode)
                      }
                      help="Read only prevents file changes. Workspace write allows changes inside the workspace. Full access removes Codex file-system isolation."
                    />
                  </div>
                  {(form.approvalPolicy === "never" ||
                    form.sandboxMode === "danger-full-access") && (
                    <div className="mt-4 px-3 py-2.5 rounded-md border border-red-500/40 bg-red-500/10 text-xs text-red-300 leading-relaxed">
                      {form.approvalPolicy === "never" &&
                        "Approval prompts are disabled. "}
                      {form.sandboxMode === "danger-full-access" &&
                        "Full access bypasses workspace file isolation, and the network toggle below cannot restrict commands in this mode."}
                    </div>
                  )}
                </div>

                <div className="bg-app-card border border-border rounded-lg p-5">
                  <h2 className="text-lg font-semibold text-text-primary">
                    External access
                  </h2>
                  <p className="text-xs text-text-muted mt-1 mb-2">
                    Web search and command network access are separate controls.
                  </p>
                  <div className="divide-y divide-border/60">
                    <ToggleField
                      label="Web search"
                      description="Allows Codex to use its native web search tool when it needs current information."
                      checked={form.webSearch}
                      disabled={saving}
                      onChange={(webSearch) =>
                        updateFormField("webSearch", webSearch)
                      }
                    />
                    <ToggleField
                      label="Network access for commands"
                      description="Allows commands running in the workspace sandbox to reach network services. This can send request data outside the computer."
                      checked={form.networkAccess}
                      disabled={
                        saving || form.sandboxMode === "danger-full-access"
                      }
                      onChange={(networkAccess) =>
                        updateFormField("networkAccess", networkAccess)
                      }
                    />
                  </div>
                </div>

                <div className="bg-app-card border border-border rounded-lg p-5">
                  <div className="flex items-start justify-between gap-3 mb-4">
                    <div>
                      <h2 className="text-lg font-semibold text-text-primary">
                        Permission profiles
                      </h2>
                      <p className="text-xs text-text-muted mt-1">
                        Beta profiles reported by Codex App Server. This page
                        does not change them.
                      </p>
                    </div>
                    <span className="text-xs text-text-muted">
                      {snapshot.permissionProfiles.length} found
                    </span>
                  </div>

                  {snapshot.permissionProfiles.length === 0 ? (
                    <p className="text-sm text-text-muted italic">
                      No permission profiles reported.
                    </p>
                  ) : (
                    <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
                      {snapshot.permissionProfiles.map((profile) => (
                        <div
                          key={profile.id}
                          className="bg-app-bg border border-border rounded-lg p-3"
                        >
                          <div className="flex items-start justify-between gap-3">
                            <div>
                              <p className="text-sm font-medium text-text-primary">
                                {profile.name}
                              </p>
                              <p className="text-[11px] font-mono text-text-muted mt-0.5">
                                {profile.id}
                              </p>
                            </div>
                            <span
                              className={`px-2 py-0.5 rounded text-[10px] font-medium ${
                                profile.allowed
                                  ? "bg-emerald-500/15 text-emerald-400"
                                  : "bg-red-500/15 text-red-400"
                              }`}
                            >
                              {profile.allowed ? "Available" : "Restricted"}
                            </span>
                          </div>
                          {profile.description && (
                            <p className="text-xs text-text-muted mt-2 leading-relaxed">
                              {profile.description}
                            </p>
                          )}
                        </div>
                      ))}
                    </div>
                  )}
                </div>

                <div className="bg-app-card border border-border rounded-lg p-5">
                  <h2 className="text-lg font-semibold text-text-primary">
                    Current model defaults
                  </h2>
                  <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mt-3">
                    <div className="bg-app-bg rounded-lg px-3 py-2.5">
                      <p className="text-[10px] uppercase tracking-wider text-text-muted">
                        Model
                      </p>
                      <p className="text-sm font-mono text-text-primary mt-1">
                        {snapshot.model || "Not configured"}
                      </p>
                    </div>
                    <div className="bg-app-bg rounded-lg px-3 py-2.5">
                      <p className="text-[10px] uppercase tracking-wider text-text-muted">
                        Reasoning effort
                      </p>
                      <p className="text-sm font-mono text-text-primary mt-1">
                        {snapshot.modelReasoningEffort || "Model default"}
                      </p>
                    </div>
                  </div>
                  <p className="text-xs text-text-muted mt-3">
                    Use the Codex Switch Model action to change these values
                    together.
                  </p>
                </div>

                <div className="flex items-center justify-end gap-3 pb-6">
                  {dirty && (
                    <span className="text-xs text-amber-400">
                      Unsaved changes
                    </span>
                  )}
                  <button
                    type="button"
                    onClick={handleRefresh}
                    disabled={loading || saving}
                    className="px-4 py-2 text-sm text-text-secondary hover:text-text-primary disabled:opacity-50"
                  >
                    Refresh
                  </button>
                  <button
                    type="button"
                    onClick={() => void handleSave()}
                    disabled={saving || !dirty}
                    className="px-5 py-2 rounded-lg text-white text-sm font-medium hover:opacity-90 disabled:opacity-50"
                    style={{ backgroundColor: CODEX_COLOR }}
                  >
                    {saving ? "Saving..." : "Save changes"}
                  </button>
                </div>
              </>
            )}

            {!snapshot && !error && (
              <div className="text-sm text-text-muted">
                No Codex control settings are available.
              </div>
            )}

            {!snapshot && error && (
              <button
                type="button"
                onClick={handleRefresh}
                className="px-4 py-2 rounded-lg bg-[#10a37f] text-white text-sm font-medium"
              >
                Try again
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
