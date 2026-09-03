import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DebugPath } from "../components/common/DebugPath";

const CODEX_COLOR = "#10a37f";

interface CodexConfigSnapshot {
  content: string;
  path: string;
  exists: boolean;
  revision: string;
}

export function CodexGlobalConfigPage() {
  const [content, setContent] = useState("");
  const [savedContent, setSavedContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState<string | null>(null);
  const [snapshot, setSnapshot] = useState<CodexConfigSnapshot | null>(null);
  const requestId = useRef(0);

  const load = useCallback(async () => {
    const currentRequest = ++requestId.current;
    setLoading(true);
    setError(null);
    setSuccess(null);
    try {
      const response = await invoke<CodexConfigSnapshot>(
        "read_codex_config_snapshot",
      );
      if (currentRequest !== requestId.current) return;
      setSnapshot(response);
      setContent(response.content);
      setSavedContent(response.content);
    } catch (e) {
      if (currentRequest !== requestId.current) return;
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      if (currentRequest === requestId.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const handleSave = async () => {
    if (!snapshot) return;
    setSaving(true);
    setError(null);
    setSuccess(null);
    try {
      const response = await invoke<CodexConfigSnapshot>(
        "write_codex_config_snapshot",
        { content, expectedRevision: snapshot.revision },
      );
      setSnapshot(response);
      setContent(response.content);
      setSavedContent(response.content);
      setSuccess("Codex configuration saved.");
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const dirty = content !== savedContent;

  const handleRefresh = () => {
    if (saving) return;
    if (
      dirty &&
      !window.confirm("Discard unsaved Codex configuration changes?")
    ) {
      return;
    }
    void load();
  };

  return (
    <div className="h-full flex flex-col">
      <div className="p-6 border-b border-border">
        <div className="flex items-center gap-2 mb-1">
          <span
            className="w-3 h-3 rounded-full"
            style={{ backgroundColor: CODEX_COLOR }}
          />
          <h1 className="text-2xl font-semibold text-text-primary">
            Codex - Global Config
          </h1>
        </div>
        <p className="text-text-muted text-sm">
          Manage global settings for Codex
        </p>
        <DebugPath path={snapshot?.path ?? "$CODEX_HOME/config.toml"} />
      </div>

      {/* Warning banner */}
      <div className="mx-6 mt-4 px-4 py-3 bg-amber-500/10 border border-amber-500/30 rounded-lg text-sm text-amber-400">
        Changes here affect Codex globally across all projects. AgentHarbor
        validates TOML before saving and preserves your original formatting and
        comments.
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        <div className="flex items-center justify-between mb-3">
          <h2 className="text-lg font-semibold text-text-primary">
            config.toml
          </h2>
          <div className="flex items-center gap-2">
            {dirty && (
              <span className="text-xs text-amber-400 font-medium">
                Unsaved changes
              </span>
            )}
            <button
              onClick={handleRefresh}
              disabled={loading || saving}
              className="text-sm text-accent-blue hover:underline disabled:opacity-50"
            >
              Refresh
            </button>
            <button
              onClick={handleSave}
              disabled={loading || saving || !dirty || !snapshot}
              className="px-4 py-2 rounded-lg text-white text-sm font-medium hover:opacity-90 disabled:opacity-50"
              style={{ backgroundColor: CODEX_COLOR }}
            >
              {saving ? "Saving..." : "Save"}
            </button>
          </div>
        </div>

        {error && (
          <p className="text-sm text-accent-red mb-2" role="alert">
            {error}
          </p>
        )}
        {success && (
          <p className="text-sm text-emerald-400 mb-2" role="status">
            {success}
          </p>
        )}

        {loading ? (
          <div className="h-64 flex items-center justify-center text-text-muted">
            Loading...
          </div>
        ) : (
          <div>
            <label htmlFor="codex-global-config" className="sr-only">
              Codex global config.toml content
            </label>
            <textarea
              id="codex-global-config"
              value={content}
              onChange={(e) => {
                setContent(e.target.value);
                setSuccess(null);
              }}
              className="w-full min-h-[400px] px-4 py-3 bg-app-card border border-border rounded-lg font-mono text-sm text-text-primary focus:outline-none focus:border-accent-blue resize-y"
              placeholder="# Codex config.toml"
              spellCheck={false}
            />
          </div>
        )}
      </div>
    </div>
  );
}
