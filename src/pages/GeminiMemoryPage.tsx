import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";
import { DebugPath } from "../components/common/DebugPath";

const GEMINI_COLOR = "#4285f4";

export function GeminiMemoryPage() {
  const [projectScope, setProjectScope] = useState<string | null>(null);
  const [content, setContent] = useState("");
  const [savedContent, setSavedContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const isGlobal = projectScope == null;

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      if (isGlobal) {
        const text = await invoke<string>("read_gemini_memory");
        setContent(text);
        setSavedContent(text);
      } else {
        const text = await invoke<string>("read_gemini_project_memory", {
          projectPath: projectScope,
        });
        setContent(text);
        setSavedContent(text);
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [projectScope, isGlobal]);

  useEffect(() => {
    load();
  }, [load]);

  const isDirty = content !== savedContent;

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      if (isGlobal) {
        await invoke("write_gemini_memory", { content });
      } else if (projectScope) {
        await invoke("write_gemini_project_memory", { projectPath: projectScope, content });
      }
      setSavedContent(content);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const filePath = isGlobal ? "~/.gemini/GEMINI.md" : `${projectScope}/GEMINI.md`;

  return (
    <div className="h-full flex flex-col">
      <div className="p-6 border-b border-border flex items-center justify-between flex-wrap gap-3">
        <div>
          <div className="flex items-center gap-2 mb-1">
            <span className="w-3 h-3 rounded-full" style={{ backgroundColor: GEMINI_COLOR }} />
            <h1 className="text-2xl font-semibold text-text-primary">Gemini CLI — Memory</h1>
          </div>
          <DebugPath path={filePath} className="text-sm" />
        </div>
        <div className="flex items-center gap-2">
          <ProjectScopeSelector value={projectScope} onChange={setProjectScope} />
          {isDirty && (
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
            disabled={loading || saving || !isDirty}
            className="px-4 py-2 rounded-lg text-white text-sm font-medium hover:opacity-90 disabled:opacity-50"
            style={{ backgroundColor: GEMINI_COLOR }}
          >
            {saving ? "Saving..." : "Save"}
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-hidden p-6 flex flex-col gap-4">
        {error && <p className="text-sm text-accent-red">{error}</p>}
        {loading ? (
          <div className="h-64 flex items-center justify-center text-text-muted">Loading...</div>
        ) : (
          <textarea
            value={content}
            onChange={(e) => setContent(e.target.value)}
            className="w-full flex-1 min-h-[200px] px-4 py-3 bg-app-card border border-border rounded-lg font-mono text-sm text-text-primary focus:outline-none focus:border-accent-blue resize-none"
            placeholder={
              isGlobal
                ? "# GEMINI.md\n\nAdd notes or context that Gemini CLI can use globally."
                : "# GEMINI.md\n\nAdd project-specific notes or context for Gemini CLI."
            }
          />
        )}
      </div>
    </div>
  );
}
