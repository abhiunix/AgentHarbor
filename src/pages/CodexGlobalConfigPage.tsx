import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DebugPath } from "../components/common/DebugPath";

const CODEX_COLOR = "#10a37f";

export function CodexGlobalConfigPage() {
  const [content, setContent] = useState("");
  const [savedContent, setSavedContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const text = await invoke<string>("read_codex_config");
      setContent(text);
      setSavedContent(text);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      await invoke("write_codex_config", { content });
      setSavedContent(content);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const dirty = content !== savedContent;

  return (
    <div className="h-full flex flex-col">
      <div className="p-6 border-b border-border">
        <div className="flex items-center gap-2 mb-1">
          <span className="w-3 h-3 rounded-full" style={{ backgroundColor: CODEX_COLOR }} />
          <h1 className="text-2xl font-semibold text-text-primary">Codex — Global Config</h1>
        </div>
        <p className="text-text-muted text-sm">Manage global settings for Codex</p>
        <DebugPath path="~/.codex/config.toml" />
      </div>

      {/* Warning banner */}
      <div className="mx-6 mt-4 px-4 py-3 bg-amber-500/10 border border-amber-500/30 rounded-lg text-sm text-amber-400">
        Changes here affect Codex globally across all projects.
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        <div className="flex items-center justify-between mb-3">
          <h2 className="text-lg font-semibold text-text-primary">config.toml</h2>
          <div className="flex items-center gap-2">
            {dirty && (
              <span className="text-xs text-amber-400 font-medium">Unsaved changes</span>
            )}
            <button
              onClick={load}
              disabled={loading}
              className="text-sm text-accent-blue hover:underline disabled:opacity-50"
            >
              Refresh
            </button>
            <button
              onClick={handleSave}
              disabled={loading || saving || !dirty}
              className="px-4 py-2 rounded-lg text-white text-sm font-medium hover:opacity-90 disabled:opacity-50"
              style={{ backgroundColor: CODEX_COLOR }}
            >
              {saving ? "Saving..." : "Save"}
            </button>
          </div>
        </div>

        {error && <p className="text-sm text-accent-red mb-2">{error}</p>}

        {loading ? (
          <div className="h-64 flex items-center justify-center text-text-muted">Loading...</div>
        ) : (
          <textarea
            value={content}
            onChange={(e) => setContent(e.target.value)}
            className="w-full min-h-[400px] px-4 py-3 bg-app-card border border-border rounded-lg font-mono text-sm text-text-primary focus:outline-none focus:border-accent-blue resize-y"
            placeholder="# Codex config.toml"
            spellCheck={false}
          />
        )}
      </div>
    </div>
  );
}
