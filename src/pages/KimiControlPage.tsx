/**
 * Kimi "Permissions & Control" — Kimi's control settings from ~/.kimi/config.toml
 * plus model switching (there is intentionally no separate "Switch Model"
 * section — it lives here). Mirrors PermissionsPage's section/card styling.
 */
import { useState, useEffect, useCallback, useRef } from "react";
import {
  getKimiControlSettings,
  setKimiDefaultModel,
  getKimiConfigTunables,
  setKimiConfigValue,
  type KimiControlSettings,
  type KimiModelInfo,
  type KimiConfigSection,
  type KimiConfigEntry,
  type KimiConfigValueType,
} from "../lib/tauri";
import { DebugPath } from "../components/common/DebugPath";

const SECTION_ACRONYMS: Record<string, string> = { mcp: "MCP" };
const KEY_ACRONYMS: Record<string, string> = { yolo: "YOLO", afk: "AFK", mcp: "MCP" };

function humanizeSection(section: string | null): string {
  if (section === null) return "General";
  let firstIsAcronym = false;
  const parts = section.split(".").map((p, i) => {
    const acronym = SECTION_ACRONYMS[p.toLowerCase()];
    if (acronym) {
      if (i === 0) firstIsAcronym = true;
      return acronym;
    }
    return p.replace(/_/g, " ");
  });
  const joined = parts.join(" ");
  return firstIsAcronym ? joined : joined.charAt(0).toUpperCase() + joined.slice(1);
}

function humanizeKey(key: string): string {
  const words = key.split("_");
  let suffix = "";
  const last = words[words.length - 1];
  if (words.length > 1 && (last === "ms" || last === "s")) {
    suffix = ` (${words.pop()})`;
  }
  const label = words
    .map((w, i) => {
      const lower = w.toLowerCase();
      if (KEY_ACRONYMS[lower]) return KEY_ACRONYMS[lower];
      return i === 0 ? lower.charAt(0).toUpperCase() + lower.slice(1) : lower;
    })
    .join(" ");
  return label + suffix;
}

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

type SaveStatus = "idle" | "saving" | "saved" | "error";

type SaveTunable = (
  section: string | null,
  key: string,
  rawValue: string,
  valueType: KimiConfigValueType
) => Promise<void>;

function parseArrayValue(raw: string): string[] {
  try {
    const parsed: unknown = JSON.parse(raw.replace(/'/g, '"'));
    return Array.isArray(parsed) ? parsed.map((v) => String(v)) : [];
  } catch {
    return [];
  }
}

function ConfigEntryRow({
  section,
  entry,
  onSave,
}: {
  section: string | null;
  entry: KimiConfigEntry;
  onSave: SaveTunable;
}) {
  const [value, setValue] = useState(entry.value);
  const [status, setStatus] = useState<SaveStatus>("idle");
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const label = humanizeKey(entry.key);

  useEffect(() => {
    setValue(entry.value);
  }, [entry.value]);

  const commit = useCallback(
    async (next: string) => {
      if (debounceRef.current) {
        clearTimeout(debounceRef.current);
        debounceRef.current = null;
      }
      if (next === entry.value) return;
      setStatus("saving");
      try {
        await onSave(section, entry.key, next, entry.value_type);
        setStatus("saved");
        setTimeout(() => setStatus((s) => (s === "saved" ? "idle" : s)), 2000);
      } catch {
        setValue(entry.value);
        setStatus("error");
        setTimeout(() => setStatus((s) => (s === "error" ? "idle" : s)), 3000);
      }
    },
    [entry.key, entry.value, entry.value_type, onSave, section]
  );

  if (entry.value_type === "bool") {
    return (
      <SwitchRow
        label={label}
        info={`~/.kimi/config.toml — ${entry.key}`}
        checked={value === "true"}
        disabled={status === "saving"}
        onChange={(next) => {
          const raw = next ? "true" : "false";
          setValue(raw);
          void commit(raw);
        }}
      />
    );
  }

  if (entry.value_type === "array") {
    const items = parseArrayValue(entry.value);
    return (
      <div className="py-1.5">
        <div className="flex items-center justify-between">
          <span className="text-sm font-medium text-text-primary">{label}</span>
          <span className="text-[10px] uppercase tracking-wide text-text-muted">Read-only</span>
        </div>
        {items.length > 0 ? (
          <div className="flex flex-wrap gap-1 mt-1.5">
            {items.map((item) => (
              <span
                key={item}
                className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-[#2a2b36] text-text-secondary"
              >
                {item}
              </span>
            ))}
          </div>
        ) : (
          <p className="text-xs font-mono text-text-muted mt-1">{entry.value || "[]"}</p>
        )}
      </div>
    );
  }

  const isNumeric = entry.value_type === "int" || entry.value_type === "float";

  return (
    <div className="flex items-center justify-between gap-3 py-1.5">
      <label className="text-sm font-medium text-text-primary shrink-0">{label}</label>
      <div className="flex items-center gap-2">
        {status === "saved" && <span className="text-xs text-accent-green">Saved</span>}
        {status === "error" && <span className="text-xs text-red-400">Error</span>}
        <input
          type={isNumeric ? "number" : "text"}
          step={entry.value_type === "float" ? "any" : undefined}
          value={value}
          disabled={status === "saving"}
          onChange={(e) => {
            const next = e.target.value;
            setValue(next);
            if (isNumeric) {
              if (debounceRef.current) clearTimeout(debounceRef.current);
              debounceRef.current = setTimeout(() => void commit(next), 600);
            }
          }}
          onBlur={() => void commit(value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              (e.target as HTMLInputElement).blur();
            }
          }}
          className="w-40 bg-[#13141a] border border-border rounded px-2 py-1 text-sm text-text-primary font-mono text-right focus:outline-none focus:border-blue-500 disabled:opacity-50"
        />
      </div>
    </div>
  );
}

function ConfigSectionCard({ section, onSave }: { section: KimiConfigSection; onSave: SaveTunable }) {
  if (section.entries.length === 0) return null;
  return (
    <div className="bg-app-card border border-border rounded-lg p-5">
      <h2 className="text-lg font-semibold text-text-primary mb-1">{humanizeSection(section.section)}</h2>
      <DebugPath
        path={section.section ? `~/.kimi/config.toml [${section.section}]` : "~/.kimi/config.toml"}
        className="mb-3"
      />
      <div className="divide-y divide-border/50">
        {section.entries.map((entry) => (
          <ConfigEntryRow key={entry.key} section={section.section} entry={entry} onSave={onSave} />
        ))}
      </div>
    </div>
  );
}

export function KimiControlPage() {
  const [settings, setSettings] = useState<KimiControlSettings | null>(null);
  const [tunables, setTunables] = useState<KimiConfigSection[] | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [switchingModel, setSwitchingModel] = useState(false);
  const [switchMessage, setSwitchMessage] = useState<string | null>(null);

  const loadData = useCallback(async () => {
    try {
      const [s, t] = await Promise.all([getKimiControlSettings(), getKimiConfigTunables()]);
      setSettings(s);
      setTunables(t);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  const loadTunables = useCallback(async () => {
    const t = await getKimiConfigTunables();
    setTunables(t);
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

  const handleSaveTunable: SaveTunable = useCallback(
    async (section, key, rawValue, valueType) => {
      await setKimiConfigValue(section, key, rawValue, valueType);
      await loadTunables();
    },
    [loadTunables]
  );

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
        </div>

        <div className="bg-app-card border border-border rounded-lg p-5">
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

      {tunables && tunables.some((s) => s.entries.length > 0) && (
        <div className="mt-6">
          <h3 className="text-sm font-medium text-text-secondary mb-3">Config settings</h3>
          <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
            {tunables.map((section) => (
              <ConfigSectionCard
                key={section.section ?? "__top_level__"}
                section={section}
                onSave={handleSaveTunable}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
