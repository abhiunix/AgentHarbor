import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DebugPath } from "../components/common/DebugPath";

const GEMINI_COLOR = "#4285f4";

interface GeminiExtension {
  name: string;
  dir_path: string;
  has_manifest: boolean;
}

export function GeminiExtensionsPage() {
  const [extensions, setExtensions] = useState<GeminiExtension[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await invoke<GeminiExtension[]>("list_gemini_extensions");
      setExtensions(list);
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
              <h1 className="text-2xl font-semibold text-text-primary">
                Gemini CLI — Extensions
              </h1>
            </div>
            <p className="text-text-muted text-sm">Browse installed Gemini CLI extensions.</p>
            <DebugPath path="~/.gemini/extensions/" className="text-sm" />
          </div>
          <button
            onClick={load}
            disabled={loading}
            className="flex items-center gap-2 px-3 py-2 rounded-lg border border-border text-text-primary text-sm hover:bg-card-hover disabled:opacity-50"
          >
            Refresh
          </button>
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
        ) : extensions.length === 0 ? (
          <div className="py-16 text-center">
            <p className="text-text-muted text-sm mb-2">No extensions installed.</p>
            <p className="text-text-muted text-xs">
              Install Gemini CLI extensions to ~/.gemini/extensions/ to see them here.
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {extensions.map((ext) => (
              <div
                key={ext.dir_path}
                className="p-4 bg-app-card border border-border rounded-lg hover:bg-card-hover transition-colors"
              >
                <div className="flex items-start justify-between mb-2">
                  <h3 className="text-sm font-semibold text-text-primary truncate mr-2">
                    {ext.name}
                  </h3>
                  {ext.has_manifest && (
                    <span className="px-1.5 py-0.5 text-[10px] font-medium rounded bg-green-500/20 text-green-400 flex-shrink-0">
                      Manifest
                    </span>
                  )}
                </div>
                <p
                  className="text-xs text-text-muted font-mono truncate"
                  title={ext.dir_path}
                >
                  {ext.dir_path}
                </p>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
