/**
 * Kimi "Permissions & Control" — Kimi's control settings from ~/.kimi/config.toml
 * plus model switching (there is intentionally no separate "Switch Model"
 * section — it lives here). Mirrors PermissionsPage's section/card styling.
 */
import { useState, useEffect, useCallback } from "react";
import {
  getKimiControlSettings,
  setKimiDefaultModel,
  setKimiControlFlag,
  type KimiControlSettings,
  type KimiModelInfo,
} from "../lib/tauri";
import { DebugPath } from "../components/common/DebugPath";

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

function SwitchRow({
  label,
  info,
  checked,
  disabled,
  onChange,
}: {
  label: string;
  info: string;
  checked: boolean;
  disabled?: boolean;
  onChange: (next: boolean) => void;
}) {
  return (
    <label
      className={`flex items-center justify-between py-1.5 ${disabled ? "opacity-50" : "cursor-pointer"}`}
    >
      <span className="text-sm font-medium text-text-primary inline-flex items-center">
        {label}
        <InfoIcon text={info} />
      </span>
      <div
        className={`relative w-10 h-5 rounded-full transition-colors ${
          checked ? "bg-blue-500" : "bg-[#2a2b36]"
        }`}
        onClick={() => !disabled && onChange(!checked)}
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

function formatContextSize(n: number | null): string {
  if (n == null) return "—";
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M tokens`;
  if (n >= 1_000) return `${Math.round(n / 1_000)}K tokens`;
  return `${n} tokens`;
}

function ModelCard({
  model,
  isDefault,
  switching,
  onSelect,
}: {
  model: KimiModelInfo;
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
        <span className="text-sm font-mono text-text-primary truncate">{model.id}</span>
        {isDefault && (
          <span className="shrink-0 text-[10px] font-semibold uppercase tracking-wide px-1.5 py-0.5 rounded bg-blue-500 text-white">
            Active
          </span>
        )}
      </div>
      <div className="flex items-center gap-2 mt-1 text-xs text-text-muted">
        {model.provider && <span>{model.provider}</span>}
        {model.max_context_size != null && (
          <>
            <span>·</span>
            <span>{formatContextSize(model.max_context_size)}</span>
          </>
        )}
      </div>
      {model.capabilities.length > 0 && (
        <div className="flex flex-wrap gap-1 mt-2">
          {model.capabilities.map((c) => (
            <span
              key={c}
              className="text-[10px] px-1.5 py-0.5 rounded bg-[#2a2b36] text-text-secondary"
            >
              {c}
            </span>
          ))}
        </div>
      )}
    </button>
  );
}

export function KimiControlPage() {
  const [settings, setSettings] = useState<KimiControlSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [switchingModel, setSwitchingModel] = useState(false);
  const [switchMessage, setSwitchMessage] = useState<string | null>(null);
  const [savingFlag, setSavingFlag] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    try {
      const s = await getKimiControlSettings();
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

  async function handleSelectModel(modelId: string) {
    setSwitchingModel(true);
    setSwitchMessage(null);
    try {
      await setKimiDefaultModel(modelId);
      await loadData();
      setSwitchMessage("Switched — applies to new kimi sessions.");
      setTimeout(() => setSwitchMessage(null), 4000);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setSwitchingModel(false);
    }
  }

  async function handleToggleFlag(flag: string, next: boolean) {
    if (!settings) return;
    setSavingFlag(flag);
    // Optimistic update.
    setSettings({ ...settings, [flag]: next } as KimiControlSettings);
    try {
      await setKimiControlFlag(flag, next);
      await loadData();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
      await loadData();
    } finally {
      setSavingFlag(null);
    }
  }

  if (loading) {
    return (
      <div className="p-6 flex items-center justify-center h-64">
        <p className="text-text-secondary">Loading Kimi control settings…</p>
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

  const loop = settings.loop_control;

  return (
    <div className="p-6 max-w-6xl">
      <div className="mb-4">
        <h1 className="text-2xl font-bold text-text-primary mb-2">
          Permissions &amp; Control
        </h1>
        <p className="text-text-secondary text-sm">
          Model switching and global control defaults for Kimi Code, read from{" "}
          <code className="font-mono text-text-primary">~/.kimi/config.toml</code>.
        </p>
      </div>

      {error && (
        <div className="bg-red-900/30 border border-red-700 rounded-lg px-4 py-3 mb-4">
          <p className="text-red-300 text-sm">{error}</p>
        </div>
      )}

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        <div className="bg-app-card border border-border rounded-lg p-5">
          <h2 className="text-lg font-semibold text-text-primary mb-1">Model</h2>
          <DebugPath path="~/.kimi/config.toml (default_model)" className="mb-3" />

          {switchMessage && (
            <div className="mb-3 px-3 py-2 rounded-md border border-accent-green/40 bg-accent-green/10 text-sm text-accent-green">
              {switchMessage}
            </div>
          )}

          {settings.models.length === 0 ? (
            <p className="text-sm text-text-muted italic">No models found in config.toml.</p>
          ) : (
            <div className="space-y-2">
              {settings.models.map((m) => (
                <ModelCard
                  key={m.id}
                  model={m}
                  isDefault={m.id === settings.default_model}
                  switching={switchingModel}
                  onSelect={() => handleSelectModel(m.id)}
                />
              ))}
            </div>
          )}

          <div className="border-t border-border pt-4 mt-4">
            <h4 className="text-sm font-medium text-text-secondary mb-2">Global defaults</h4>
            <SwitchRow
              label="YOLO mode"
              info="Skip approval prompts by default for new Kimi sessions."
              checked={settings.default_yolo}
              disabled={savingFlag === "default_yolo"}
              onChange={(v) => handleToggleFlag("default_yolo", v)}
            />
            <SwitchRow
              label="Thinking"
              info="Enable extended thinking by default for new Kimi sessions."
              checked={settings.default_thinking}
              disabled={savingFlag === "default_thinking"}
              onChange={(v) => handleToggleFlag("default_thinking", v)}
            />
            <SwitchRow
              label="Plan mode"
              info="Start new Kimi sessions in plan mode by default."
              checked={settings.default_plan_mode}
              disabled={savingFlag === "default_plan_mode"}
              onChange={(v) => handleToggleFlag("default_plan_mode", v)}
            />
          </div>
        </div>

        <div className="flex flex-col gap-6">
          <div className="bg-app-card border border-border rounded-lg p-5">
            <h2 className="text-lg font-semibold text-text-primary mb-1">Loop control</h2>
            <DebugPath path="~/.kimi/config.toml [loop_control]" className="mb-3" />
            <div className="grid grid-cols-2 gap-3">
              <div className="bg-[#13141a] border border-border rounded-lg px-3 py-2.5">
                <p className="text-xs text-text-muted mb-0.5">Max steps / turn</p>
                <p className="text-sm font-mono text-text-primary">
                  {loop.max_steps_per_turn ?? "—"}
                </p>
              </div>
              <div className="bg-[#13141a] border border-border rounded-lg px-3 py-2.5">
                <p className="text-xs text-text-muted mb-0.5">Max retries / step</p>
                <p className="text-sm font-mono text-text-primary">
                  {loop.max_retries_per_step ?? "—"}
                </p>
              </div>
              <div className="bg-[#13141a] border border-border rounded-lg px-3 py-2.5">
                <p className="text-xs text-text-muted mb-0.5">Reserved context</p>
                <p className="text-sm font-mono text-text-primary">
                  {formatContextSize(loop.reserved_context_size)}
                </p>
              </div>
              <div className="bg-[#13141a] border border-border rounded-lg px-3 py-2.5">
                <p className="text-xs text-text-muted mb-0.5">Compaction trigger</p>
                <p className="text-sm font-mono text-text-primary">
                  {loop.compaction_trigger_ratio != null
                    ? `${Math.round(loop.compaction_trigger_ratio * 100)}%`
                    : "—"}
                </p>
              </div>
            </div>
          </div>

          <div className="bg-app-card border border-border rounded-lg p-5 flex-1">
            <h2 className="text-lg font-semibold text-text-primary mb-1">
              Per-session approval
            </h2>
            <DebugPath path="~/.kimi/sessions/*/*/state.json" className="mb-3" />
            {settings.sessions_approval.length === 0 ? (
              <p className="text-sm text-text-muted italic">
                No session approval data found.
              </p>
            ) : (
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="text-left text-xs text-text-muted border-b border-border">
                      <th className="pb-2 pr-3 font-medium">Session</th>
                      <th className="pb-2 pr-3 font-medium">YOLO</th>
                      <th className="pb-2 pr-3 font-medium">AFK</th>
                      <th className="pb-2 font-medium">Auto-approve</th>
                    </tr>
                  </thead>
                  <tbody>
                    {settings.sessions_approval.map((s) => (
                      <tr key={s.session_id} className="border-b border-border/50 last:border-0">
                        <td className="py-2 pr-3">
                          <p className="text-text-primary">{s.project_name}</p>
                          <p className="text-text-muted text-xs font-mono truncate max-w-[10rem]">
                            {s.session_id}
                          </p>
                        </td>
                        <td className="py-2 pr-3">
                          <span className={s.yolo ? "text-accent-green" : "text-text-muted"}>
                            {s.yolo ? "Yes" : "No"}
                          </span>
                        </td>
                        <td className="py-2 pr-3">
                          <span className={s.afk ? "text-accent-green" : "text-text-muted"}>
                            {s.afk ? "Yes" : "No"}
                          </span>
                        </td>
                        <td className="py-2 text-text-secondary">
                          {s.auto_approve_actions.length > 0
                            ? s.auto_approve_actions.join(", ")
                            : "—"}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
