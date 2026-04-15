import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openPath } from "@tauri-apps/plugin-opener";
import { homeDir, join } from "@tauri-apps/api/path";
import { DebugPath } from "../components/common/DebugPath";

const GEMINI_COLOR = "#4285f4";

interface GeminiHook {
  hook_type: string;
  matcher: string;
  command: string;
}

const HOOK_TYPE_COLORS: Record<string, string> = {
  BeforeTool: "bg-orange-500/20 text-orange-400",
  AfterTool: "bg-blue-500/20 text-blue-400",
};

export function GeminiHooksPage() {
  const [hooks, setHooks] = useState<GeminiHook[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await invoke<GeminiHook[]>("get_gemini_hooks");
      setHooks(list);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  return (
    <div className="h-full flex flex-col">
      <div className="p-6 border-b border-border">
        <div className="flex items-center justify-between">
          <div>
            <div className="flex items-center gap-2 mb-1">
              <span
                className="w-3 h-3 rounded-full"
                style={{ backgroundColor: GEMINI_COLOR }}
              />
              <h1 className="text-2xl font-semibold text-text-primary">Gemini CLI — Hooks</h1>
            </div>
            <p className="text-text-muted text-sm">
              Hooks are configured in ~/.gemini/settings.json under the hooks key
            </p>
            <DebugPath path="~/.gemini/settings.json (hooks key)" />
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={load}
              disabled={loading}
              className="text-sm text-accent-blue hover:underline disabled:opacity-50"
            >
              Refresh
            </button>
            <button
              onClick={async () => {
                try {
                  const home = await homeDir();
                  const settingsPath = await join(home, ".gemini", "settings.json");
                  await openPath(settingsPath);
                } catch (err) {
                  console.error("Failed to open settings file:", err);
                }
              }}
              className="px-4 py-2 rounded-lg border border-border text-text-primary text-sm hover:bg-card-hover transition-colors"
            >
              Open Raw Settings
            </button>
          </div>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        {error && (
          <div className="mb-4 px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400">
            {error}
          </div>
        )}

        {loading ? (
          <div className="h-64 flex items-center justify-center text-text-muted">Loading...</div>
        ) : hooks.length === 0 ? (
          <div className="py-16 text-center">
            <p className="text-text-muted text-sm mb-2">No hooks configured.</p>
            <p className="text-text-muted text-xs">
              Add hooks to ~/.gemini/settings.json to automate actions before or after tool calls.
            </p>
          </div>
        ) : (
          <div className="space-y-3">
            {hooks.map((hook, idx) => {
              const typeClass =
                HOOK_TYPE_COLORS[hook.hook_type] ?? "bg-gray-500/20 text-gray-400";
              return (
                <div
                  key={idx}
                  className="p-4 bg-app-card border border-border rounded-lg hover:bg-card-hover transition-colors"
                >
                  <div className="flex items-center gap-3 mb-2">
                    <span
                      className={`px-2 py-0.5 text-xs font-medium rounded ${typeClass}`}
                    >
                      {hook.hook_type}
                    </span>
                    <span className="text-sm text-text-muted font-mono">{hook.matcher}</span>
                  </div>
                  <div className="px-3 py-2 bg-app-bg border border-border rounded font-mono text-sm text-text-primary">
                    {hook.command}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
