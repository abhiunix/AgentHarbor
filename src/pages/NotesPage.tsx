import { useState, useEffect, useCallback, useRef } from "react";
import { useLocation } from "react-router-dom";
import {
  listNotesEntries,
  readNoteContent,
  writeNoteContent,
  createNotesFolder,
  createNotesFile,
  renameNotesEntry,
  deleteNotesEntry,
  moveNotesEntry,
  type NotesEntry,
} from "../lib/tauri";
import { basename } from "../lib/platform";

type ContextMenu = {
  x: number;
  y: number;
  relativePath: string;
  isFolder: boolean;
};

const AUTO_SAVE_MS = 1200;

export function NotesPage() {
  const location = useLocation();
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set([""]));
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [selectedIsFolder, setSelectedIsFolder] = useState(false);
  const [contextMenu, setContextMenu] = useState<ContextMenu | null>(null);
  const [editorContent, setEditorContent] = useState("");
  const [savedContent, setSavedContent] = useState("");
  const [cache, setCache] = useState<Map<string, NotesEntry[]>>(new Map());
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [promptState, setPromptState] = useState<"folder" | "file" | "rename" | null>(null);
  const [promptValue, setPromptValue] = useState("");
  const [promptParentPath, setPromptParentPath] = useState("");
  const [promptRenamePath, setPromptRenamePath] = useState<string | null>(null);

  const [dragSourcePath, setDragSourcePath] = useState<string | null>(null);
  const [dragSourceName, setDragSourceName] = useState("");
  const [dragSourceIsFolder, setDragSourceIsFolder] = useState(false);
  const [dragOverTarget, setDragOverTarget] = useState<string | null>(null);
  const [isDragging, setIsDragging] = useState(false);
  const [cursorPos, setCursorPos] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const dragStartPos = useRef<{ x: number; y: number } | null>(null);
  const dragThreshold = 5;

  const [deleteConfirm, setDeleteConfirm] = useState<{ path: string; name: string } | null>(null);

  const autoSaveTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const selectedPathRef = useRef(selectedPath);
  const selectedIsFolderRef = useRef(selectedIsFolder);
  selectedPathRef.current = selectedPath;
  selectedIsFolderRef.current = selectedIsFolder;

  const invalidateCache = useCallback((parentPath: string) => {
    setCache((prev) => {
      const next = new Map(prev);
      next.delete(parentPath);
      return next;
    });
  }, []);

  const loadChildren = useCallback(async (relativePath: string): Promise<NotesEntry[]> => {
    try {
      const entries = await listNotesEntries(relativePath);
      setCache((prev) => new Map(prev).set(relativePath, entries));
      return entries;
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      return [];
    }
  }, []);

  const getChildren = useCallback(
    (relativePath: string): NotesEntry[] => cache.get(relativePath) ?? [],
    [cache]
  );

  useEffect(() => {
    loadChildren("");
  }, [loadChildren]);

  // --- Auto-save ---
  const doAutoSave = useCallback(async (path: string, content: string) => {
    try {
      await writeNoteContent(path, content);
      setSavedContent(content);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  useEffect(() => {
    if (autoSaveTimer.current) clearTimeout(autoSaveTimer.current);
    if (selectedPath === null || selectedIsFolder) return;
    if (editorContent === savedContent) return;
    const path = selectedPath;
    autoSaveTimer.current = setTimeout(() => {
      doAutoSave(path, editorContent);
    }, AUTO_SAVE_MS);
    return () => {
      if (autoSaveTimer.current) clearTimeout(autoSaveTimer.current);
    };
  }, [editorContent, savedContent, selectedPath, selectedIsFolder, doAutoSave]);

  // Flush auto-save immediately when switching files
  const flushAutoSave = useCallback(() => {
    if (autoSaveTimer.current) {
      clearTimeout(autoSaveTimer.current);
      autoSaveTimer.current = null;
    }
    const p = selectedPathRef.current;
    const isF = selectedIsFolderRef.current;
    if (p && !isF) {
      return { path: p };
    }
    return null;
  }, []);

  const handleSelect = useCallback(
    async (entry: NotesEntry) => {
      const pending = flushAutoSave();
      if (pending && pending.path !== entry.relative_path) {
        // editorContent is stale in this closure but the ref-based flush handles it
      }
      setSelectedPath(entry.relative_path);
      setSelectedIsFolder(entry.is_folder);
      setContextMenu(null);
      if (!entry.is_folder) {
        try {
          const content = await readNoteContent(entry.relative_path);
          setEditorContent(content);
          setSavedContent(content);
        } catch (e) {
          setError(e instanceof Error ? e.message : String(e));
        }
      } else {
        setEditorContent("");
        setSavedContent("");
      }
    },
    [flushAutoSave]
  );

  const handleToggleExpand = useCallback(
    (relativePath: string) => {
      setExpandedPaths((prev) => {
        const next = new Set(prev);
        if (next.has(relativePath)) {
          next.delete(relativePath);
        } else {
          next.add(relativePath);
          if (!cache.has(relativePath)) {
            loadChildren(relativePath);
          }
        }
        return next;
      });
    },
    [cache, loadChildren]
  );

  const handleContextMenu = useCallback(
    (e: React.MouseEvent, relativePath: string, isFolder: boolean) => {
      e.preventDefault();
      e.stopPropagation();
      setContextMenu({ x: e.clientX, y: e.clientY, relativePath, isFolder });
    },
    []
  );

  const closeContextMenu = useCallback(() => setContextMenu(null), []);

  useEffect(() => {
    const handleClick = () => closeContextMenu();
    window.addEventListener("click", handleClick);
    return () => window.removeEventListener("click", handleClick);
  }, [closeContextMenu]);

  const parentPath = (path: string): string => {
    const i = path.lastIndexOf("/");
    return i <= 0 ? "" : path.slice(0, i);
  };

  const handleNewFolder = useCallback(async () => {
    if (promptState !== "folder" || !promptValue.trim()) return;
    const parent = promptParentPath;
    const name = promptValue.trim();
    setPromptState(null);
    setPromptValue("");
    setPromptParentPath("");
    try {
      await createNotesFolder(parent, name);
      invalidateCache(parent);
      await loadChildren(parent);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [promptState, promptValue, promptParentPath, invalidateCache, loadChildren]);

  const handleNewFile = useCallback(async () => {
    if (promptState !== "file" || !promptValue.trim()) return;
    const parent = promptParentPath;
    let name = promptValue.trim();
    if (!name.includes(".")) {
      name = name + ".txt";
    }
    setPromptState(null);
    setPromptValue("");
    setPromptParentPath("");
    try {
      const entry = await createNotesFile(parent, name);
      invalidateCache(parent);
      await loadChildren(parent);
      handleSelect(entry);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [promptState, promptValue, promptParentPath, invalidateCache, loadChildren, handleSelect]);

  const handleRename = useCallback(async () => {
    if (promptState !== "rename" || !promptValue.trim() || promptRenamePath === null) return;
    const path = promptRenamePath;
    const name = promptValue.trim();
    setPromptState(null);
    setPromptValue("");
    setPromptRenamePath(null);
    try {
      await renameNotesEntry(path, name);
      const parent = parentPath(path);
      invalidateCache(parent);
      await loadChildren(parent);
      if (selectedPath === path) {
        const newPath = parent ? `${parent}/${name}` : name;
        setSelectedPath(newPath);
        if (!selectedIsFolder) {
          const content = await readNoteContent(newPath);
          setEditorContent(content);
          setSavedContent(content);
        }
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [promptState, promptValue, promptRenamePath, selectedPath, selectedIsFolder, invalidateCache, loadChildren]);

  const doDeleteEntry = useCallback(async (pathToDelete: string) => {
    try {
      await deleteNotesEntry(pathToDelete);
      const parent = parentPath(pathToDelete);
      invalidateCache(parent);
      await loadChildren(parent);
      if (selectedPath === pathToDelete) {
        setSelectedPath(null);
        setSelectedIsFolder(false);
        setEditorContent("");
        setSavedContent("");
      }
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [selectedPath, invalidateCache, loadChildren]);

  const triggerDelete = useCallback(() => {
    if (!contextMenu || !contextMenu.relativePath) return;
    const path = contextMenu.relativePath;
    const name = basename(path);
    closeContextMenu();
    setDeleteConfirm({ path, name });
  }, [contextMenu, closeContextMenu]);

  const handleMove = useCallback(
    async (fromPath: string, toParentPath: string) => {
      if (fromPath === toParentPath) return;
      if (toParentPath.startsWith(fromPath + "/")) return;
      const fromParent = parentPath(fromPath);
      if (fromParent === toParentPath) return;
      try {
        const entry = await moveNotesEntry(fromPath, toParentPath);
        invalidateCache(fromParent);
        invalidateCache(toParentPath);
        await loadChildren(fromParent);
        await loadChildren(toParentPath);
        if (selectedPath === fromPath) {
          setSelectedPath(entry.relative_path);
          setSelectedIsFolder(entry.is_folder);
          if (!entry.is_folder) {
            const content = await readNoteContent(entry.relative_path);
            setEditorContent(content);
            setSavedContent(content);
          }
        }
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    },
    [selectedPath, invalidateCache, loadChildren]
  );

  // --- Mouse-based drag system ---
  const handleMouseDown = useCallback((e: React.MouseEvent, path: string, name: string, isFolder: boolean) => {
    if (e.button !== 0) return;
    dragStartPos.current = { x: e.clientX, y: e.clientY };
    setCursorPos({ x: e.clientX, y: e.clientY });
    setDragSourcePath(path);
    setDragSourceName(name);
    setDragSourceIsFolder(isFolder);
  }, []);

  useEffect(() => {
    if (isDragging) {
      document.body.style.cursor = "grabbing";
      document.body.style.userSelect = "none";
    }
    return () => {
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
    };
  }, [isDragging]);

  useEffect(() => {
    if (dragSourcePath === null) return;

    const onMouseMove = (e: MouseEvent) => {
      if (!dragStartPos.current) return;
      const dx = e.clientX - dragStartPos.current.x;
      const dy = e.clientY - dragStartPos.current.y;
      if (Math.abs(dx) > dragThreshold || Math.abs(dy) > dragThreshold) {
        setIsDragging(true);
      }
      if (isDragging) {
        setCursorPos({ x: e.clientX, y: e.clientY });
      }
    };

    const onMouseUp = () => {
      if (isDragging && dragSourcePath && dragOverTarget !== null) {
        handleMove(dragSourcePath, dragOverTarget);
      }
      setDragSourcePath(null);
      setDragSourceName("");
      setDragSourceIsFolder(false);
      setIsDragging(false);
      setDragOverTarget(null);
      dragStartPos.current = null;
    };

    window.addEventListener("mousemove", onMouseMove);
    window.addEventListener("mouseup", onMouseUp);
    return () => {
      window.removeEventListener("mousemove", onMouseMove);
      window.removeEventListener("mouseup", onMouseUp);
    };
  }, [dragSourcePath, isDragging, dragOverTarget, handleMove]);

  const handleDragEnterFolder = useCallback((folderPath: string) => {
    setDragOverTarget(folderPath);
  }, []);

  const isDirty = editorContent !== savedContent;
  const showEditor = selectedPath !== null && !selectedIsFolder;

  const handleSave = useCallback(async () => {
    if (selectedPath === null || selectedIsFolder) return;
    setSaving(true);
    setError(null);
    try {
      await writeNoteContent(selectedPath, editorContent);
      setSavedContent(editorContent);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  }, [selectedPath, selectedIsFolder, editorContent]);

  const openNewFolderPrompt = useCallback((parent: string) => {
    setPromptParentPath(parent);
    setPromptState("folder");
    setPromptValue("");
    closeContextMenu();
  }, [closeContextMenu]);

  const openNewFilePrompt = useCallback((parent: string) => {
    setPromptParentPath(parent);
    setPromptState("file");
    setPromptValue("");
    closeContextMenu();
  }, [closeContextMenu]);

  const triggerNewFolder = () => {
    const parent = contextMenu
      ? (contextMenu.isFolder ? contextMenu.relativePath : parentPath(contextMenu.relativePath))
      : (selectedIsFolder ? selectedPath ?? "" : parentPath(selectedPath ?? ""));
    openNewFolderPrompt(parent);
  };
  const triggerNewFile = () => {
    const parent = contextMenu
      ? (contextMenu.isFolder ? contextMenu.relativePath : parentPath(contextMenu.relativePath))
      : (selectedIsFolder ? selectedPath ?? "" : parentPath(selectedPath ?? ""));
    openNewFilePrompt(parent);
  };
  const triggerRename = () => {
    const path = contextMenu?.relativePath ?? "";
    setPromptRenamePath(path || null);
    setPromptState("rename");
    setPromptValue(path ? basename(path) : "");
    closeContextMenu();
  };

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (location.pathname !== "/notes") return;
      if (e.target instanceof HTMLInputElement || e.target instanceof HTMLTextAreaElement) return;
      if (e.shiftKey && (e.metaKey || e.ctrlKey)) {
        if (e.key === "n") {
          e.preventDefault();
          triggerNewFile();
          return;
        }
        if (e.key === "f") {
          e.preventDefault();
          triggerNewFolder();
          return;
        }
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [location.pathname]);

  const submitPrompt = () => {
    if (promptState === "folder") handleNewFolder();
    else if (promptState === "file") handleNewFile();
    else if (promptState === "rename") handleRename();
  };

  return (
    <div className="h-full flex flex-col">
      {error && (
        <div className="px-6 py-2 bg-accent-red/10 border-b border-accent-red/20 text-sm text-accent-red flex items-center justify-between">
          <span>{error}</span>
          <button type="button" onClick={() => setError(null)} className="hover:underline">
            Dismiss
          </button>
        </div>
      )}

      <div className="flex-1 flex min-h-0">
        <div className="w-[260px] shrink-0 border-r border-border flex flex-col bg-app-sidebar overflow-hidden">
          <div className="p-2 border-b border-border flex items-center justify-between">
            <span className="text-[10px] font-semibold uppercase tracking-wider text-text-muted">Notes</span>
            <div className="flex items-center gap-0.5">
              <button
                type="button"
                title="New folder"
                onClick={(e) => { e.stopPropagation(); openNewFolderPrompt(""); }}
                className="w-5 h-5 flex items-center justify-center rounded text-text-muted hover:text-text-primary hover:bg-app-card-hover transition-colors"
              >
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M2 4h4.5l1.5 1.5H14v8H2z" />
                  <path d="M8 7.5v4M6 9.5h4" />
                </svg>
              </button>
              <button
                type="button"
                title="New note"
                onClick={(e) => { e.stopPropagation(); openNewFilePrompt(""); }}
                className="w-5 h-5 flex items-center justify-center rounded text-text-muted hover:text-text-primary hover:bg-app-card-hover transition-colors"
              >
                <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M4 2h5l3 3v9H4z" />
                  <path d="M8 6.5v4M6 8.5h4" />
                </svg>
              </button>
            </div>
          </div>
          <div
            className="flex-1 overflow-y-auto py-1"
            onContextMenu={(e) => {
              e.preventDefault();
              setContextMenu({ x: e.clientX, y: e.clientY, relativePath: "", isFolder: true });
            }}
            onMouseEnter={() => {
              if (isDragging) setDragOverTarget("");
            }}
          >
            {isDragging && (
              <div
                className={`mx-2 mb-1 rounded px-2 py-1.5 text-xs border border-dashed transition-colors ${
                  dragOverTarget === "" ? "border-accent-blue bg-accent-blue/10 text-accent-blue" : "border-border text-text-muted"
                }`}
                onMouseEnter={() => setDragOverTarget("")}
              >
                Drop here for root
              </div>
            )}
            <TreeLevel
              relativePath=""
              getChildren={getChildren}
              expandedPaths={expandedPaths}
              onToggleExpand={handleToggleExpand}
              selectedPath={selectedPath}
              onSelect={handleSelect}
              onContextMenu={handleContextMenu}
              loadChildren={loadChildren}
              isDragging={isDragging}
              dragSourcePath={dragSourcePath}
              dragOverTarget={dragOverTarget}
              onMouseDownEntry={handleMouseDown}
              onDragEnterFolder={handleDragEnterFolder}
              onQuickNewFolder={openNewFolderPrompt}
              onQuickNewFile={openNewFilePrompt}
            />
          </div>
        </div>

        <div className="flex-1 flex flex-col min-w-0 bg-app-bg">
          {showEditor ? (
            <>
              <div className="h-12 px-4 border-b border-border flex items-center justify-between gap-2">
                <span className="text-sm font-mono text-text-primary truncate">
                  {selectedPath ? basename(selectedPath) : ""}
                  {isDirty ? (
                    <span className="ml-2 text-accent-orange text-xs font-normal">Saving…</span>
                  ) : (
                    <span className="ml-2 text-accent-green text-xs font-normal">Saved</span>
                  )}
                </span>
                <button
                  type="button"
                  onClick={handleSave}
                  disabled={!isDirty || saving}
                  className="px-3 py-1.5 text-xs font-medium rounded-md bg-accent-blue text-white hover:bg-accent-blue/90 disabled:opacity-50 disabled:pointer-events-none"
                >
                  {saving ? "Saving…" : "Save"}
                </button>
              </div>
              <textarea
                className="flex-1 w-full p-4 font-mono text-sm text-text-primary bg-app-bg border-0 resize-none focus:outline-none"
                value={editorContent}
                onChange={(e) => setEditorContent(e.target.value)}
                spellCheck={false}
              />
            </>
          ) : (
            <div className="flex-1 flex items-center justify-center text-text-muted text-sm">
              Select a note or create one (right-click in tree for options).
            </div>
          )}
        </div>
      </div>

      {/* Context menu */}
      {contextMenu && (
        <div
          className="fixed z-50 min-w-[160px] py-1 bg-app-card border border-border rounded-lg shadow-xl"
          style={{ left: contextMenu.x, top: contextMenu.y }}
        >
          <button
            type="button"
            className="w-full px-3 py-2 text-left text-sm text-text-primary hover:bg-app-card-hover"
            onClick={triggerNewFolder}
          >
            New folder
          </button>
          <button
            type="button"
            className="w-full px-3 py-2 text-left text-sm text-text-primary hover:bg-app-card-hover"
            onClick={triggerNewFile}
          >
            New note
          </button>
          {contextMenu.relativePath && (
            <>
              {!contextMenu.isFolder && (
                <button
                  type="button"
                  className="w-full px-3 py-2 text-left text-sm text-text-primary hover:bg-app-card-hover"
                  onClick={() => {
                    handleSelect({
                      name: basename(contextMenu.relativePath),
                      relative_path: contextMenu.relativePath,
                      is_folder: false,
                    });
                    closeContextMenu();
                  }}
                >
                  Edit
                </button>
              )}
              <button
                type="button"
                className="w-full px-3 py-2 text-left text-sm text-text-primary hover:bg-app-card-hover"
                onClick={triggerRename}
              >
                Rename
              </button>
              <button
                type="button"
                className="w-full px-3 py-2 text-left text-sm text-accent-red hover:bg-app-card-hover"
                onClick={triggerDelete}
              >
                Delete
              </button>
            </>
          )}
        </div>
      )}

      {/* Prompt modal (new folder / new note / rename) */}
      {promptState && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center">
          <div className="absolute inset-0 bg-black/60" onClick={() => setPromptState(null)} />
          <div className="relative bg-app-card border border-border rounded-xl p-4 w-full max-w-sm shadow-xl">
            <h3 className="text-sm font-semibold text-text-primary mb-2">
              {promptState === "folder" && "New folder name"}
              {promptState === "file" && "New note name (e.g. file.md)"}
              {promptState === "rename" && "Rename to"}
            </h3>
            <input
              type="text"
              value={promptValue}
              onChange={(e) => setPromptValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") submitPrompt();
                if (e.key === "Escape") setPromptState(null);
              }}
              className="w-full h-9 px-3 rounded-md bg-app-input border border-border text-sm text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-blue"
              placeholder={promptState === "file" ? "note.md" : "name"}
              autoFocus
            />
            <div className="flex justify-end gap-2 mt-3">
              <button
                type="button"
                onClick={() => setPromptState(null)}
                className="px-3 py-1.5 text-sm text-text-muted hover:text-text-primary"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={submitPrompt}
                disabled={!promptValue.trim()}
                className="px-3 py-1.5 text-sm font-medium rounded-md bg-accent-blue text-white hover:bg-accent-blue/90 disabled:opacity-50"
              >
                {promptState === "rename" ? "Rename" : "Create"}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Drag ghost following cursor */}
      {isDragging && dragSourcePath !== null && (
        <div
          className="fixed z-[100] pointer-events-none"
          style={{ left: cursorPos.x + 12, top: cursorPos.y - 10 }}
        >
          <div className="flex items-center gap-1.5 px-3 py-1.5 bg-app-card border border-accent-blue/50 rounded-lg shadow-lg shadow-black/40 text-sm text-text-primary whitespace-nowrap">
            <span className="text-base">{dragSourceIsFolder ? "📁" : "📄"}</span>
            <span className="max-w-[150px] truncate">{dragSourceName}</span>
          </div>
        </div>
      )}

      {/* Delete confirmation modal */}
      {deleteConfirm && (
        <div className="fixed inset-0 z-[60] flex items-center justify-center">
          <div className="absolute inset-0 bg-black/60" onClick={() => setDeleteConfirm(null)} />
          <div className="relative bg-app-card border border-border rounded-xl p-5 w-full max-w-sm shadow-xl">
            <h3 className="text-sm font-semibold text-text-primary mb-3">Delete</h3>
            <p className="text-sm text-text-secondary mb-4">
              Are you sure you want to delete <span className="font-mono text-text-primary">{deleteConfirm.name}</span>? This action cannot be undone.
            </p>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setDeleteConfirm(null)}
                className="px-3 py-1.5 text-sm text-text-muted hover:text-text-primary"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => {
                  const path = deleteConfirm.path;
                  setDeleteConfirm(null);
                  doDeleteEntry(path);
                }}
                className="px-3 py-1.5 text-sm font-medium rounded-md bg-accent-red text-white hover:bg-accent-red/90"
              >
                Delete
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// --- Tree ---

interface TreeLevelProps {
  relativePath: string;
  getChildren: (path: string) => NotesEntry[];
  expandedPaths: Set<string>;
  onToggleExpand: (path: string) => void;
  selectedPath: string | null;
  onSelect: (entry: NotesEntry) => void;
  onContextMenu: (e: React.MouseEvent, path: string, isFolder: boolean) => void;
  loadChildren: (path: string) => Promise<NotesEntry[]>;
  isDragging: boolean;
  dragSourcePath: string | null;
  dragOverTarget: string | null;
  onMouseDownEntry: (e: React.MouseEvent, path: string, name: string, isFolder: boolean) => void;
  onDragEnterFolder: (path: string) => void;
  onQuickNewFolder: (parent: string) => void;
  onQuickNewFile: (parent: string) => void;
  depth?: number;
}

function TreeLevel({
  relativePath,
  getChildren,
  expandedPaths,
  onToggleExpand,
  selectedPath,
  onSelect,
  onContextMenu,
  loadChildren,
  isDragging,
  dragSourcePath,
  dragOverTarget,
  onMouseDownEntry,
  onDragEnterFolder,
  onQuickNewFolder,
  onQuickNewFile,
  depth = 0,
}: TreeLevelProps) {
  const entries = getChildren(relativePath);
  const isExpanded = expandedPaths.has(relativePath);

  useEffect(() => {
    if (relativePath !== "" && isExpanded && entries.length === 0) {
      loadChildren(relativePath);
    }
  }, [relativePath, isExpanded, entries.length, loadChildren]);

  if (entries.length === 0 && relativePath !== "") return null;

  const canDropOn = (folderPath: string) => {
    if (!isDragging || dragSourcePath === null) return false;
    if (folderPath === dragSourcePath) return false;
    if (folderPath.startsWith(dragSourcePath + "/")) return false;
    return true;
  };

  return (
    <div className="select-none" style={{ paddingLeft: depth * 12 }}>
      {entries.map((entry) => {
        const isDropTarget = entry.is_folder && dragOverTarget === entry.relative_path && canDropOn(entry.relative_path);
        const isBeingDragged = isDragging && dragSourcePath === entry.relative_path;

        return (
          <div key={entry.relative_path} className="group/row">
            <div
              className={`flex items-center min-h-7 cursor-pointer rounded ${
                isDropTarget ? "bg-accent-blue/15 ring-1 ring-accent-blue/40" : ""
              } ${isBeingDragged ? "opacity-40" : ""}`}
              onMouseDown={(e) => onMouseDownEntry(e, entry.relative_path, entry.name, entry.is_folder)}
              onMouseEnter={() => {
                if (isDragging && entry.is_folder && canDropOn(entry.relative_path)) {
                  onDragEnterFolder(entry.relative_path);
                }
              }}
              onClick={(e) => {
                e.stopPropagation();
                if (entry.is_folder) {
                  onToggleExpand(entry.relative_path);
                } else {
                  onSelect(entry);
                }
              }}
              onContextMenu={(e) => {
                e.preventDefault();
                e.stopPropagation();
                onContextMenu(e, entry.relative_path, entry.is_folder);
              }}
            >
              <div className="flex-1 flex items-center gap-1.5 min-w-0 px-2 py-1 text-sm rounded hover:bg-app-card-hover">
                <span className="shrink-0 w-4 text-text-muted">
                  {entry.is_folder ? (expandedPaths.has(entry.relative_path) ? "▾" : "▸") : " "}
                </span>
                <span className="shrink-0 text-base">
                  {entry.is_folder ? "📁" : "📄"}
                </span>
                <span
                  className={`truncate ${
                    selectedPath === entry.relative_path && !entry.is_folder
                      ? "text-accent-blue font-medium"
                      : "text-text-primary"
                  }`}
                >
                  {entry.name}
                </span>
              </div>
              {entry.is_folder && !isDragging && (
                <div className="hidden group-hover/row:flex items-center gap-0.5 pr-1 shrink-0">
                  <button
                    type="button"
                    title="New folder"
                    onClick={(e) => { e.stopPropagation(); onQuickNewFolder(entry.relative_path); }}
                    className="w-5 h-5 flex items-center justify-center rounded text-text-muted hover:text-text-primary hover:bg-[#2a2b36] transition-colors"
                  >
                    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M2 4h4.5l1.5 1.5H14v8H2z" />
                      <path d="M8 7.5v4M6 9.5h4" />
                    </svg>
                  </button>
                  <button
                    type="button"
                    title="New note"
                    onClick={(e) => { e.stopPropagation(); onQuickNewFile(entry.relative_path); }}
                    className="w-5 h-5 flex items-center justify-center rounded text-text-muted hover:text-text-primary hover:bg-[#2a2b36] transition-colors"
                  >
                    <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                      <path d="M4 2h5l3 3v9H4z" />
                      <path d="M8 6.5v4M6 8.5h4" />
                    </svg>
                  </button>
                </div>
              )}
            </div>
            {entry.is_folder && expandedPaths.has(entry.relative_path) && (
              <TreeLevel
                relativePath={entry.relative_path}
                getChildren={getChildren}
                expandedPaths={expandedPaths}
                onToggleExpand={onToggleExpand}
                selectedPath={selectedPath}
                onSelect={onSelect}
                onContextMenu={onContextMenu}
                loadChildren={loadChildren}
                isDragging={isDragging}
                dragSourcePath={dragSourcePath}
                dragOverTarget={dragOverTarget}
                onMouseDownEntry={onMouseDownEntry}
                onDragEnterFolder={onDragEnterFolder}
                onQuickNewFolder={onQuickNewFolder}
                onQuickNewFile={onQuickNewFile}
                depth={depth + 1}
              />
            )}
          </div>
        );
      })}
    </div>
  );
}
