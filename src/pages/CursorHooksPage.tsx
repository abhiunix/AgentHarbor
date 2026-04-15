import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";
import { DebugPath } from "../components/common/DebugPath";

interface CursorHook {
  event: string;
  command: string;
  action: string;
}

interface CursorHooksConfig {
  hooks: CursorHook[];
}

const HOOK_EVENTS = [
  "beforeShellExecution",
  "beforeMCPExecution",
  "beforeReadFile",
  "afterFileEdit",
  "stop",
] as const;

const HOOK_ACTIONS = ["allow", "deny", "ask", "run"] as const;

const EVENT_COLORS: Record<string, { bg: string; text: string }> = {
  beforeShellExecution: { bg: "bg-orange-500/20", text: "text-orange-400" },
  beforeMCPExecution: { bg: "bg-purple-500/20", text: "text-purple-400" },
  beforeReadFile: { bg: "bg-cyan-500/20", text: "text-cyan-400" },
  afterFileEdit: { bg: "bg-green-500/20", text: "text-green-400" },
  stop: { bg: "bg-red-500/20", text: "text-red-400" },
};

const ACTION_COLORS: Record<string, { bg: string; text: string }> = {
  allow: { bg: "bg-green-500/20", text: "text-green-400" },
  deny: { bg: "bg-red-500/20", text: "text-red-400" },
  ask: { bg: "bg-amber-500/20", text: "text-amber-400" },
  run: { bg: "bg-blue-500/20", text: "text-blue-400" },
};

type Tab = "project" | "global";
type ViewMode = "visual" | "json";

export function CursorHooksPage() {
  const [activeTab, setActiveTab] = useState<Tab>("project");
  const [projectPath, setProjectPath] = useState<string | null>(null);

  return (
    <div className="h-full flex flex-col">
      <div className="px-6 pt-6 pb-0">
        <div className="flex items-center justify-between flex-wrap gap-3 mb-4">
          <div className="flex items-center gap-3">
            <span className="w-3 h-3 rounded-full bg-blue-500 flex-shrink-0" />
            <div>
              <h1 className="text-2xl font-semibold text-text-primary">
                Cursor — Hooks
              </h1>
              <p className="text-sm text-text-secondary">
                Manage Cursor hook configurations
              </p>
              <DebugPath path=".cursor/hooks.json · ~/.cursor/hooks.json" />
            </div>
          </div>
          {activeTab === "project" && (
            <ProjectScopeSelector value={projectPath} onChange={setProjectPath} />
          )}
        </div>

        <div className="flex gap-1 border-b border-border">
          <button
            onClick={() => setActiveTab("project")}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
              activeTab === "project"
                ? "border-accent-blue text-text-primary"
                : "border-transparent text-text-secondary hover:text-text-primary"
            }`}
          >
            Project Hooks
          </button>
          <button
            onClick={() => setActiveTab("global")}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
              activeTab === "global"
                ? "border-accent-blue text-text-primary"
                : "border-transparent text-text-secondary hover:text-text-primary"
            }`}
          >
            Global Hooks
          </button>
        </div>
      </div>

      {activeTab === "project" ? (
        <HooksTab projectPath={projectPath} />
      ) : (
        <HooksTab projectPath={null} isGlobal />
      )}
    </div>
  );
}

function HooksTab({
  projectPath,
  isGlobal = false,
}: {
  projectPath: string | null;
  isGlobal?: boolean;
}) {
  const [hooks, setHooks] = useState<CursorHook[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>("visual");
  const [jsonDraft, setJsonDraft] = useState("");
  const [jsonError, setJsonError] = useState<string | null>(null);
  const [showAddForm, setShowAddForm] = useState(false);

  const path = isGlobal ? null : projectPath;

  const load = useCallback(async () => {
    if (!isGlobal && !projectPath) {
      setHooks([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const config = await invoke<CursorHooksConfig>("list_cursor_hooks", {
        projectPath: path,
      });
      setHooks(config.hooks ?? []);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [projectPath, isGlobal, path]);

  useEffect(() => {
    load();
  }, [load]);

  useEffect(() => {
    if (viewMode === "json") {
      setJsonDraft(JSON.stringify({ hooks }, null, 2));
      setJsonError(null);
    }
  }, [viewMode, hooks]);

  const handleRemove = async (index: number) => {
    if (!window.confirm("Remove this hook?")) return;
    try {
      await invoke("remove_cursor_hook", { projectPath: path, index });
      await load();
    } catch (e) {
      setError(String(e));
    }
  };

  const handleSaveJson = async () => {
    setJsonError(null);
    try {
      JSON.parse(jsonDraft);
    } catch {
      setJsonError("Invalid JSON");
      return;
    }
    try {
      await invoke("save_cursor_hooks", {
        projectPath: path,
        hooksJson: jsonDraft,
      });
      await load();
      setViewMode("visual");
    } catch (e) {
      setJsonError(String(e));
    }
  };

  if (!isGlobal && !projectPath) {
    return (
      <div className="p-6 text-text-secondary text-sm">
        Select a project to view its Cursor hooks.
      </div>
    );
  }

  if (loading) {
    return (
      <div className="p-6 text-text-secondary text-sm">Loading hooks...</div>
    );
  }

  if (error) {
    return (
      <div className="p-6">
        <div className="px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400">
          {error}
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-3">
      <div className="flex items-center justify-between mb-2">
        <span className="text-sm text-text-secondary">
          {hooks.length} hook{hooks.length !== 1 ? "s" : ""}
        </span>
        <div className="flex items-center gap-2">
          <div className="flex gap-0.5 p-0.5 bg-app-bg rounded-lg border border-border">
            <button
              onClick={() => setViewMode("visual")}
              className={`px-3 py-1 rounded-md text-xs font-medium transition-colors ${
                viewMode === "visual"
                  ? "bg-accent-blue text-white"
                  : "text-text-muted hover:text-text-primary"
              }`}
            >
              Visual
            </button>
            <button
              onClick={() => setViewMode("json")}
              className={`px-3 py-1 rounded-md text-xs font-medium transition-colors ${
                viewMode === "json"
                  ? "bg-accent-blue text-white"
                  : "text-text-muted hover:text-text-primary"
              }`}
            >
              JSON
            </button>
          </div>
          {viewMode === "visual" && (
            <button
              onClick={() => setShowAddForm(true)}
              className="px-3 py-1.5 text-sm rounded bg-blue-600 hover:bg-blue-500 text-white transition-colors"
            >
              + Add Hook
            </button>
          )}
        </div>
      </div>

      {viewMode === "json" ? (
        <div className="space-y-3">
          {jsonError && (
            <div className="px-3 py-2 bg-red-500/10 border border-red-500/30 rounded text-sm text-red-400">
              {jsonError}
            </div>
          )}
          <textarea
            value={jsonDraft}
            onChange={(e) => setJsonDraft(e.target.value)}
            rows={20}
            className="w-full bg-[#13141a] border border-border rounded px-3 py-2 text-sm text-text-primary font-mono focus:outline-none focus:border-blue-500 resize-y"
            spellCheck={false}
          />
          <button
            onClick={handleSaveJson}
            className="px-4 py-2 text-sm rounded bg-blue-600 hover:bg-blue-500 text-white transition-colors"
          >
            Save JSON
          </button>
        </div>
      ) : (
        <>
          {showAddForm && (
            <AddHookForm
              projectPath={path}
              onClose={() => setShowAddForm(false)}
              onSaved={() => {
                setShowAddForm(false);
                load();
              }}
            />
          )}

          {hooks.length === 0 && !showAddForm && (
            <div className="text-center py-12">
              <p className="text-text-secondary text-sm">No hooks configured.</p>
              <p className="text-text-muted text-xs mt-1">
                Click "+ Add Hook" to create one.
              </p>
            </div>
          )}

          {hooks.map((hook, index) => {
            const eventColor = EVENT_COLORS[hook.event] ?? {
              bg: "bg-gray-500/20",
              text: "text-gray-400",
            };
            const actionColor = ACTION_COLORS[hook.action] ?? {
              bg: "bg-gray-500/20",
              text: "text-gray-400",
            };

            return (
              <div
                key={index}
                className="bg-app-card border border-border rounded-lg px-4 py-3 flex items-center gap-3"
              >
                <span
                  className={`px-2 py-0.5 text-xs font-medium rounded flex-shrink-0 ${eventColor.bg} ${eventColor.text}`}
                >
                  {hook.event}
                </span>
                <span className="flex-1 text-sm text-text-primary font-mono truncate">
                  {hook.command}
                </span>
                <span
                  className={`px-2 py-0.5 text-xs font-medium rounded flex-shrink-0 ${actionColor.bg} ${actionColor.text}`}
                >
                  {hook.action}
                </span>
                <button
                  onClick={() => handleRemove(index)}
                  className="text-text-muted hover:text-red-400 transition-colors flex-shrink-0"
                >
                  &#x2715;
                </button>
              </div>
            );
          })}
        </>
      )}
    </div>
  );
}

function AddHookForm({
  projectPath,
  onClose,
  onSaved,
}: {
  projectPath: string | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [event, setEvent] = useState<string>(HOOK_EVENTS[0]);
  const [command, setCommand] = useState("");
  const [action, setAction] = useState<string>(HOOK_ACTIONS[0]);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSave = async () => {
    if (!command.trim()) return;
    setSaving(true);
    setError(null);
    try {
      await invoke("add_cursor_hook", {
        projectPath,
        event,
        command: command.trim(),
        action,
      });
      onSaved();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="bg-app-card border border-border rounded-lg p-4 space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-text-primary">Add Hook</h3>
        <button
          onClick={onClose}
          className="text-text-muted hover:text-text-primary text-sm"
        >
          Cancel
        </button>
      </div>

      {error && (
        <div className="px-3 py-2 bg-red-500/10 border border-red-500/30 rounded text-sm text-red-400">
          {error}
        </div>
      )}

      <div>
        <label className="block text-xs text-text-secondary mb-1">Event</label>
        <select
          value={event}
          onChange={(e) => setEvent(e.target.value)}
          className="w-full bg-[#13141a] border border-border rounded px-3 py-1.5 text-sm text-text-primary focus:outline-none focus:border-blue-500"
        >
          {HOOK_EVENTS.map((e) => (
            <option key={e} value={e}>
              {e}
            </option>
          ))}
        </select>
      </div>

      <div>
        <label className="block text-xs text-text-secondary mb-1">
          Command
        </label>
        <input
          type="text"
          value={command}
          onChange={(e) => setCommand(e.target.value)}
          placeholder="e.g. npm test"
          className="w-full bg-[#13141a] border border-border rounded px-3 py-1.5 text-sm text-text-primary font-mono placeholder:text-text-muted focus:outline-none focus:border-blue-500"
        />
      </div>

      <div>
        <label className="block text-xs text-text-secondary mb-1">
          Action
        </label>
        <select
          value={action}
          onChange={(e) => setAction(e.target.value)}
          className="w-full bg-[#13141a] border border-border rounded px-3 py-1.5 text-sm text-text-primary focus:outline-none focus:border-blue-500"
        >
          {HOOK_ACTIONS.map((a) => (
            <option key={a} value={a}>
              {a}
            </option>
          ))}
        </select>
      </div>

      <button
        onClick={handleSave}
        disabled={saving || !command.trim()}
        className="px-4 py-2 text-sm rounded bg-blue-600 hover:bg-blue-500 text-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {saving ? "Saving..." : "Add Hook"}
      </button>
    </div>
  );
}
