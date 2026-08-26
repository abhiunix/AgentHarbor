/**
 * DeepSeek "Permissions & Control" — model + reasoning-effort switching from
 * ~/.dsh/settings.yaml (agent-default-model) plus a read-only per-session
 * policy view. Mirrors KimiControlPage's section/card styling.
 */
import { useState, useEffect, useCallback } from "react";
import {
  getDeepSeekControlSettings,
  setDeepSeekDefaultModel,
  setDeepSeekReasoningEffort,
  type DeepSeekControlSettings,
} from "../lib/tauri";
import { DebugPath } from "../components/common/DebugPath";

function ModelCard({
  model,
  isDefault,
  switching,
  onSelect,
}: {
  model: string;
  isDefault: boolean;
  switching: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onSelect}
      disabled={isDefault || switching}
      className={`w-full text-left px-3 py-2.5 rounded-lg border transition-colors ${
        isDefault
          ? "border-blue-500 bg-blue-500/10"
          : "border-border bg-[#13141a] hover:bg-app-card-hover"
      } disabled:cursor-default`}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="text-sm font-mono text-text-primary truncate">{model}</span>
        {isDefault && (
          <span className="shrink-0 text-[10px] font-semibold uppercase tracking-wide px-1.5 py-0.5 rounded bg-blue-500 text-white">
            Active
          </span>
        )}
      </div>
    </button>
  );
}

function ReasoningEffortControl({
  options,
  current,
  switching,
  onSelect,
}: {
  options: string[];
  current: string | null;
  switching: boolean;
  onSelect: (effort: string) => void;
}) {
  return (
    <div className="flex flex-wrap gap-2">
      {options.map((effort) => {
        const isActive = effort === current;
        return (
          <button
            key={effort}
            type="button"
            onClick={() => onSelect(effort)}
            disabled={isActive || switching}
            className={`px-3 py-1.5 rounded-md border text-sm font-mono capitalize transition-colors ${
              isActive
                ? "border-blue-500 bg-blue-500/10 text-text-primary"
                : "border-border bg-[#13141a] text-text-secondary hover:bg-app-card-hover"
            } disabled:cursor-default`}
          >
            {effort}
          </button>
        );
      })}
    </div>
  );
}

export function DeepSeekControlPage() {
  const [settings, setSettings] = useState<DeepSeekControlSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [switchingModel, setSwitchingModel] = useState(false);
  const [switchingEffort, setSwitchingEffort] = useState(false);
  const [switchMessage, setSwitchMessage] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    try {
      const s = await getDeepSeekControlSettings();
      setSettings(s);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadData();
  }, [loadData]);

  async function handleSelectModel(model: string) {
    setSwitchingModel(true);
    setSwitchMessage(null);
    try {
      await setDeepSeekDefaultModel(model);
      await loadData();
      setSwitchMessage("Switched — applies to new dsh sessions.");
      setTimeout(() => setSwitchMessage(null), 4000);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSwitchingModel(false);
    }
  }

  async function handleSelectEffort(effort: string) {
    setSwitchingEffort(true);
    setSwitchMessage(null);
    try {
      await setDeepSeekReasoningEffort(effort);
      await loadData();
      setSwitchMessage("Switched — applies to new dsh sessions.");
      setTimeout(() => setSwitchMessage(null), 4000);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSwitchingEffort(false);
    }
  }

  if (loading) {
    return (
      <div className="p-6 flex items-center justify-center h-64">
        <p className="text-text-secondary">Loading DeepSeek control settings…</p>
      </div>
    );
  }

  if (error && !settings) {
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

  if (!settings) return null;

  return (
    <div className="p-6 max-w-6xl">
      <div className="mb-4">
        <h1 className="text-2xl font-bold text-text-primary mb-2">Permissions &amp; Control</h1>
        <p className="text-text-secondary text-sm">
          Model switching and reasoning effort for DeepSeek Harness, read from{" "}
          <code className="font-mono text-text-primary">~/.dsh/settings.yaml</code>.
        </p>
      </div>

      {error && (
        <div className="bg-red-900/30 border border-red-700 rounded-lg px-4 py-3 mb-4">
          <p className="text-red-300 text-sm">{error}</p>
        </div>
      )}

      {switchMessage && (
        <div className="mb-4 px-3 py-2 rounded-md border border-accent-green/40 bg-accent-green/10 text-sm text-accent-green">
          {switchMessage}
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="bg-app-card border border-border rounded-lg p-5">
          <h2 className="text-lg font-semibold text-text-primary mb-1">Model</h2>
          <DebugPath path="~/.dsh/settings.yaml (agent-default-model.model)" className="mb-3" />

          {settings.model_options.length === 0 ? (
            <p className="text-sm text-text-muted italic">No models found.</p>
          ) : (
            <div className="space-y-2">
              {settings.model_options.map((m) => (
                <ModelCard
                  key={m}
                  model={m}
                  isDefault={m === settings.model}
                  switching={switchingModel}
                  onSelect={() => handleSelectModel(m)}
                />
              ))}
            </div>
          )}

          {settings.provider && (
            <p className="text-xs text-text-muted mt-3">
              Provider: <span className="font-mono text-text-secondary">{settings.provider}</span>
            </p>
          )}
        </div>

        <div className="bg-app-card border border-border rounded-lg p-5">
          <h2 className="text-lg font-semibold text-text-primary mb-1">Reasoning effort</h2>
          <DebugPath path="~/.dsh/settings.yaml (agent-default-model.reasoningEffort)" className="mb-3" />

          <ReasoningEffortControl
            options={settings.reasoning_options}
            current={settings.reasoning_effort}
            switching={switchingEffort}
            onSelect={handleSelectEffort}
          />

          {settings.other_settings.length > 0 && (
            <div className="mt-6">
              <h3 className="text-sm font-medium text-text-secondary mb-2">Other settings</h3>
              <div className="flex flex-wrap gap-1.5">
                {settings.other_settings.map((key) => (
                  <span
                    key={key}
                    className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-[#2a2b36] text-text-secondary"
                  >
                    {key}
                  </span>
                ))}
              </div>
            </div>
          )}
        </div>
      </div>

      <div className="mt-6">
        <div className="bg-app-card border border-border rounded-lg p-5">
          <h2 className="text-lg font-semibold text-text-primary mb-1">Per-session policies</h2>
          <DebugPath path="~/.dsh/storages/session_projcache.json (rows.permissions)" className="mb-3" />
          {settings.sessions_policies.length === 0 ? (
            <p className="text-sm text-text-muted italic">No session policy data found.</p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead>
                  <tr className="text-left text-xs text-text-muted border-b border-border">
                    <th className="pb-2 pr-3 font-medium">Session</th>
                    <th className="pb-2 pr-3 font-medium">Permission preset</th>
                    <th className="pb-2 pr-3 font-medium">Sandbox mode</th>
                    <th className="pb-2 font-medium">Approval policy</th>
                  </tr>
                </thead>
                <tbody>
                  {settings.sessions_policies.map((s) => (
                    <tr key={s.session_id} className="border-b border-border/50 last:border-0">
                      <td className="py-2 pr-3">
                        <p className="text-text-primary">{s.workspace_name}</p>
                        <p className="text-text-muted text-xs font-mono truncate max-w-[10rem]">
                          {s.session_id}
                        </p>
                      </td>
                      <td className="py-2 pr-3 text-text-secondary">{s.permission_preset ?? "—"}</td>
                      <td className="py-2 pr-3 text-text-secondary">{s.sandbox_mode ?? "—"}</td>
                      <td className="py-2 text-text-secondary">{s.approval_policy ?? "—"}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
