import { useCallback, useEffect, useRef, useState } from "react";
import { DebugPath } from "../components/common/DebugPath";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";
import {
  getCodexMemoryStatus,
  readCodexMemoryDocument,
  type CodexMemoryDocumentContent,
  type CodexMemoryStatus,
} from "../lib/tauri";

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDate(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value || "Unknown time";
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

export function CodexMemoryPage() {
  const [projectScope, setProjectScope] = useState<string | null>(null);
  const [status, setStatus] = useState<CodexMemoryStatus | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [document, setDocument] = useState<CodexMemoryDocumentContent | null>(
    null,
  );
  const [loadingStatus, setLoadingStatus] = useState(true);
  const [loadingDocument, setLoadingDocument] = useState(false);
  const [statusError, setStatusError] = useState<string | null>(null);
  const [documentError, setDocumentError] = useState<string | null>(null);
  const statusRequestIdRef = useRef(0);
  const documentRequestIdRef = useRef(0);
  const selectedIdRef = useRef<string | null>(null);

  useEffect(() => {
    selectedIdRef.current = selectedId;
  }, [selectedId]);

  const loadStatus = useCallback(async () => {
    const requestId = ++statusRequestIdRef.current;
    const previousSelection = selectedIdRef.current;
    documentRequestIdRef.current += 1;
    setLoadingStatus(true);
    setLoadingDocument(false);
    setStatusError(null);
    setDocumentError(null);
    setStatus(null);
    setDocument(null);
    setSelectedId(null);

    try {
      const next = await getCodexMemoryStatus(projectScope);
      if (requestId !== statusRequestIdRef.current) return;
      setStatus(next);
      const selection =
        next.documents.find((item) => item.id === previousSelection)?.id ??
        next.documents[0]?.id ??
        null;
      setSelectedId(selection);
    } catch (caught) {
      if (requestId !== statusRequestIdRef.current) return;
      setStatus(null);
      setStatusError(caught instanceof Error ? caught.message : String(caught));
    } finally {
      if (requestId === statusRequestIdRef.current) setLoadingStatus(false);
    }
  }, [projectScope]);

  useEffect(() => {
    void loadStatus();
    return () => {
      statusRequestIdRef.current += 1;
    };
  }, [loadStatus]);

  useEffect(() => {
    if (!selectedId) {
      setDocument(null);
      setLoadingDocument(false);
      return;
    }

    const requestId = ++documentRequestIdRef.current;
    setLoadingDocument(true);
    setDocumentError(null);
    setDocument(null);
    void readCodexMemoryDocument(selectedId)
      .then((next) => {
        if (requestId === documentRequestIdRef.current) setDocument(next);
      })
      .catch((caught) => {
        if (requestId !== documentRequestIdRef.current) return;
        setDocumentError(
          caught instanceof Error ? caught.message : String(caught),
        );
      })
      .finally(() => {
        if (requestId === documentRequestIdRef.current) {
          setLoadingDocument(false);
        }
      });

    return () => {
      documentRequestIdRef.current += 1;
    };
  }, [selectedId]);

  return (
    <div className="h-full flex flex-col overflow-hidden">
      <header className="p-6 border-b border-border shrink-0">
        <div className="flex items-start justify-between gap-4">
          <div>
            <div className="flex items-center gap-2">
              <h1 className="text-2xl font-semibold text-text-primary">
                Memory
              </h1>
              <span className="px-2 py-0.5 rounded-full bg-text-muted/10 border border-border text-[10px] uppercase tracking-wide text-text-muted">
                Read only
              </span>
            </div>
            <DebugPath path={status?.sourcePath ?? "Codex memory"} />
            <p className="text-sm text-text-secondary mt-1">
              Generated Codex memory for future sessions
            </p>
          </div>
          <div className="flex items-center gap-3">
            <ProjectScopeSelector
              value={projectScope}
              onChange={setProjectScope}
            />
            <button
              type="button"
              onClick={() => void loadStatus()}
              disabled={loadingStatus}
              className="px-3 py-2 text-sm bg-app-card border border-border rounded-lg text-text-secondary hover:text-text-primary hover:bg-app-card-hover disabled:opacity-50"
            >
              {loadingStatus ? "Refreshing..." : "Refresh"}
            </button>
          </div>
        </div>
      </header>

      <div className="px-6 py-3 border-b border-border bg-blue-500/5 shrink-0">
        <p className="text-xs text-text-secondary">
          This page shows only Codex&apos;s generated memory index and summary.
          Raw rollout summaries and their underlying session records are never
          exposed here. Memory changes are managed by Codex, not AgentHarbor.
        </p>
      </div>

      {statusError && (
        <div
          role="alert"
          className="mx-6 mt-4 px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 text-sm shrink-0"
        >
          {statusError}
        </div>
      )}

      {status?.warning && (
        <div
          role="status"
          className="mx-6 mt-4 px-4 py-3 bg-amber-500/10 border border-amber-500/30 rounded-lg text-amber-300 text-sm shrink-0"
        >
          {status.warning}
        </div>
      )}

      {loadingStatus ? (
        <div
          role="status"
          aria-live="polite"
          className="flex-1 flex items-center justify-center text-sm text-text-muted"
        >
          Loading generated memory...
        </div>
      ) : statusError && !status ? (
        <div className="flex-1 flex items-center justify-center text-sm text-text-muted p-6 text-center">
          Generated memory could not be loaded. Use Refresh to try again.
        </div>
      ) : !status?.available || status.documents.length === 0 ? (
        <div className="flex-1 flex flex-col items-center justify-center text-center p-6">
          <p className="text-sm text-text-secondary">
            No generated Codex memory is available.
          </p>
          <p className="text-xs text-text-muted mt-1 max-w-lg">
            Codex creates this global memory after it has enough eligible
            session context. AgentHarbor does not create or edit it.
          </p>
        </div>
      ) : (
        <div className="flex-1 flex min-h-0 overflow-hidden">
          <aside
            className="w-64 shrink-0 border-r border-border flex flex-col overflow-hidden"
            aria-label="Generated Codex memory documents"
          >
            <div className="px-4 py-3 border-b border-border">
              <p className="text-xs font-medium text-text-secondary uppercase tracking-wide">
                Generated documents
              </p>
              <p className="text-[10px] text-text-muted mt-1">
                Scope: {status.scope === "global" ? "Global" : status.scope}
              </p>
            </div>
            <div className="flex-1 overflow-y-auto">
              {status.documents.map((item) => (
                <button
                  key={item.id}
                  type="button"
                  onClick={() => setSelectedId(item.id)}
                  aria-pressed={selectedId === item.id}
                  className={`w-full text-left px-4 py-3 border-b border-border/60 transition-colors ${
                    selectedId === item.id
                      ? "bg-accent-blue/10 border-l-2 border-l-accent-blue"
                      : "hover:bg-app-card-hover"
                  }`}
                >
                  <p className="text-sm font-medium text-text-primary truncate">
                    {item.title}
                  </p>
                  <p className="text-xs text-text-muted font-mono truncate mt-1">
                    {item.relativePath}
                  </p>
                  <p className="text-[10px] text-text-muted mt-1">
                    {formatSize(item.sizeBytes)} &middot;{" "}
                    {formatDate(item.modifiedAt)}
                  </p>
                </button>
              ))}
            </div>
          </aside>

          <main className="flex-1 flex flex-col min-w-0 overflow-hidden">
            {documentError && (
              <div
                role="alert"
                className="m-4 px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-red-400 text-sm"
              >
                {documentError}
              </div>
            )}

            {loadingDocument ? (
              <div
                role="status"
                aria-live="polite"
                className="flex-1 flex items-center justify-center text-sm text-text-muted"
              >
                Loading memory document...
              </div>
            ) : document ? (
              <>
                <div className="px-5 py-3 border-b border-border flex items-center justify-between gap-3 shrink-0">
                  <h2 className="text-sm font-semibold text-text-primary">
                    {document.title}
                  </h2>
                  <span className="text-[10px] uppercase tracking-wide text-text-muted">
                    Read only
                  </span>
                </div>
                {document.truncated && (
                  <div
                    role="status"
                    className="mx-5 mt-4 px-4 py-3 bg-amber-500/10 border border-amber-500/30 rounded-lg text-amber-300 text-xs shrink-0"
                  >
                    This generated document is larger than the safe read limit,
                    so only the first portion is shown.
                  </div>
                )}
                <div className="flex-1 overflow-auto min-h-0 p-5">
                  <pre className="min-h-full bg-app-card border border-border rounded-lg p-4 text-sm leading-relaxed text-text-primary font-mono whitespace-pre-wrap break-words select-text">
                    {document.content || "This generated document is empty."}
                  </pre>
                </div>
              </>
            ) : (
              <div className="flex-1 flex items-center justify-center text-sm text-text-muted p-6 text-center">
                Select a generated memory document to read it.
              </div>
            )}
          </main>
        </div>
      )}
    </div>
  );
}
