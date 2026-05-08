import { useState, useEffect, useCallback } from "react";
import {
  listMemoryFiles,
  readMemoryFile,
  writeMemoryFile,
  createMemoryFile,
  deleteMemoryFile,
  type MemoryFileEntry,
} from "../lib/tauri";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  return `${(bytes / 1024).toFixed(1)} KB`;
}

function formatDate(unixSecs: number): string {
  if (!unixSecs) return "";
  return new Date(unixSecs * 1000).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
  });
}

export function MemoryPage() {
  const [projectScope, setProjectScope] = useState<string | null>(null);
  const [files, setFiles] = useState<MemoryFileEntry[]>([]);
  const [selectedFile, setSelectedFile] = useState<MemoryFileEntry | null>(null);
  const [content, setContent] = useState("");
  const [savedContent, setSavedContent] = useState("");
  const [loadingList, setLoadingList] = useState(true);
  const [loadingFile, setLoadingFile] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [newFileName, setNewFileName] = useState("");
  const [showNewInput, setShowNewInput] = useState(false);
  const [deletingPath, setDeletingPath] = useState<string | null>(null);

  const loadList = useCallback(async () => {
    setLoadingList(true);
    setError(null);
    try {
      const list = await listMemoryFiles(projectScope);
      setFiles(list);
      // If currently selected file disappeared, clear editor
      setSelectedFile((prev) => {
        if (prev && !list.find((f) => f.path === prev.path)) {
          setContent("");
          setSavedContent("");
          return null;
        }
        return prev;
      });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoadingList(false);
    }
  }, [projectScope]);

  useEffect(() => {
    setSelectedFile(null);
    setContent("");
    setSavedContent("");
    loadList();
  }, [loadList]);

  const selectFile = async (file: MemoryFileEntry) => {
    if (selectedFile?.path === file.path) return;
    setLoadingFile(true);
    setError(null);
    try {
      const text = await readMemoryFile(file.path);
      setSelectedFile(file);
      setContent(text);
      setSavedContent(text);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoadingFile(false);
    }
  };

  const handleSave = async () => {
    if (!selectedFile) return;
    setSaving(true);
    setError(null);
    try {
      await writeMemoryFile(selectedFile.path, content);
      setSavedContent(content);
      await loadList();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  const handleCreate = async () => {
    const name = newFileName.trim();
    if (!name) return;
    setError(null);
    try {
      const entry = await createMemoryFile(projectScope, name);
      setNewFileName("");
      setShowNewInput(false);
      await loadList();
      await selectFile(entry);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleDelete = async (file: MemoryFileEntry) => {
    setDeletingPath(file.path);
    setError(null);
    try {
      await deleteMemoryFile(file.path);
      if (selectedFile?.path === file.path) {
        setSelectedFile(null);
        setContent("");
        setSavedContent("");
      }
      await loadList();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setDeletingPath(null);
    }
  };

  const isDirty = content !== savedContent;

  const memoryDir = projectScope
    ? `~/.claude/projects/${projectScope.replace(/\//g, "-")}/memory/`
    : "~/.claude/memory/";

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="p-6 border-b border-border flex items-center justify-between flex-wrap gap-3 shrink-0">
        <div>
          <h1 className="text-2xl font-semibold text-text-primary mb-1">Memory</h1>
          <p className="text-xs text-text-secondary font-mono">{memoryDir}</p>
        </div>
        <div className="flex items-center gap-2">
          <ProjectScopeSelector value={projectScope} onChange={setProjectScope} />
          <button
            onClick={loadList}
            disabled={loadingList}
            className="text-sm text-accent-blue hover:underline disabled:opacity-50"
          >
            Refresh
          </button>
        </div>
      </div>

      {error && (
        <p className="px-6 pt-3 text-sm text-accent-red shrink-0">{error}</p>
      )}

      {/* Two-panel body */}
      <div className="flex-1 flex overflow-hidden">
        {/* File list */}
        <div className="w-56 shrink-0 border-r border-border flex flex-col overflow-hidden">
          <div className="flex-1 overflow-y-auto">
            {loadingList ? (
              <p className="p-4 text-sm text-text-muted">Loading...</p>
            ) : files.length === 0 ? (
              <p className="p-4 text-sm text-text-muted">No memory files yet.</p>
            ) : (
              files.map((file) => (
                <button
                  key={file.path}
                  onClick={() => selectFile(file)}
                  className={`w-full text-left px-4 py-3 border-b border-border/50 hover:bg-app-card-hover transition-colors group relative ${
                    selectedFile?.path === file.path
                      ? "bg-app-card border-l-2 border-l-accent-purple"
                      : ""
                  }`}
                >
                  <p className="text-sm text-text-primary truncate pr-6">{file.name}</p>
                  <p className="text-xs text-text-muted mt-0.5">
                    {formatSize(file.size_bytes)}
                    {file.modified_at ? ` · ${formatDate(file.modified_at)}` : ""}
                  </p>
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      handleDelete(file);
                    }}
                    disabled={deletingPath === file.path}
                    className="absolute right-2 top-1/2 -translate-y-1/2 opacity-0 group-hover:opacity-100 text-text-muted hover:text-accent-red transition-opacity text-xs px-1"
                    title="Delete file"
                  >
                    ✕
                  </button>
                </button>
              ))
            )}
          </div>

          {/* New file footer */}
          <div className="p-3 border-t border-border shrink-0">
            {showNewInput ? (
              <div className="flex flex-col gap-2">
                <input
                  autoFocus
                  value={newFileName}
                  onChange={(e) => setNewFileName(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") handleCreate();
                    if (e.key === "Escape") { setShowNewInput(false); setNewFileName(""); }
                  }}
                  placeholder="filename.md"
                  className="w-full px-2 py-1.5 bg-app-bg border border-border rounded text-sm text-text-primary focus:outline-none focus:border-accent-blue font-mono"
                />
                <div className="flex gap-1">
                  <button
                    onClick={handleCreate}
                    disabled={!newFileName.trim()}
                    className="flex-1 py-1 rounded text-xs bg-accent-blue text-white disabled:opacity-50"
                  >
                    Create
                  </button>
                  <button
                    onClick={() => { setShowNewInput(false); setNewFileName(""); }}
                    className="flex-1 py-1 rounded text-xs bg-app-card text-text-secondary hover:bg-app-card-hover"
                  >
                    Cancel
                  </button>
                </div>
              </div>
            ) : (
              <button
                onClick={() => setShowNewInput(true)}
                className="w-full py-1.5 rounded text-sm text-text-secondary hover:text-text-primary hover:bg-app-card transition-colors flex items-center justify-center gap-1"
              >
                <span className="text-base leading-none">+</span> New File
              </button>
            )}
          </div>
        </div>

        {/* Editor */}
        <div className="flex-1 flex flex-col overflow-hidden">
          {selectedFile ? (
            <>
              <div className="px-4 py-2 border-b border-border flex items-center justify-between shrink-0">
                <span className="text-sm font-mono text-text-secondary">{selectedFile.name}</span>
                <div className="flex items-center gap-2">
                  {isDirty && (
                    <span className="text-xs text-amber-400 font-medium">Unsaved changes</span>
                  )}
                  <button
                    onClick={handleSave}
                    disabled={saving || !isDirty}
                    className="px-3 py-1.5 rounded bg-accent-blue text-white text-sm font-medium hover:bg-accent-blue/90 disabled:opacity-50"
                  >
                    {saving ? "Saving..." : "Save"}
                  </button>
                </div>
              </div>
              <div className="flex-1 overflow-hidden p-4">
                {loadingFile ? (
                  <div className="h-full flex items-center justify-center text-text-muted">
                    Loading...
                  </div>
                ) : (
                  <textarea
                    value={content}
                    onChange={(e) => setContent(e.target.value)}
                    className="w-full h-full px-4 py-3 bg-app-card border border-border rounded-lg font-mono text-sm text-text-primary focus:outline-none focus:border-accent-blue resize-none"
                    placeholder="Add memory content here..."
                  />
                )}
              </div>
            </>
          ) : (
            <div className="flex-1 flex items-center justify-center text-text-muted text-sm">
              {files.length === 0
                ? "Create a new file to get started"
                : "Select a file to edit"}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
