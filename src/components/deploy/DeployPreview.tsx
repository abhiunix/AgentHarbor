import { useState } from "react";
import ReactDiffViewer, { DiffMethod } from "react-diff-viewer-continued";
import type { DiffEntry } from "../../lib/tauri";
import { basename } from "../../lib/platform";

interface DeployPreviewProps {
  diffs: DiffEntry[];
  strategies: Record<string, string>;
  onStrategyChange: (filePath: string, strategy: string) => void;
  onDeploy: () => void;
  onBack: () => void;
  loading: boolean;
}

type ViewMode = "split" | "unified" | "raw";

export function DeployPreview({
  diffs,
  strategies,
  onStrategyChange,
  onDeploy,
  onBack,
  loading,
}: DeployPreviewProps) {
  const [selectedFile, setSelectedFile] = useState<string | null>(
    diffs.length > 0 ? diffs[0].file_path : null
  );

  const selectedDiff = diffs.find((d) => d.file_path === selectedFile);

  return (
    <div className="flex flex-col h-full">
      <div className="flex flex-1 min-h-0">
        <div className="w-64 border-r border-border overflow-y-auto">
          <div className="p-4">
            <p className="text-xs text-text-muted uppercase mb-3">
              Files to Change ({diffs.length})
            </p>
            <div className="space-y-1">
              {diffs.map((diff) => (
                <FileItem
                  key={diff.file_path}
                  diff={diff}
                  selected={selectedFile === diff.file_path}
                  onClick={() => setSelectedFile(diff.file_path)}
                />
              ))}
            </div>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto">
          {selectedDiff ? (
            <EnhancedDiffView
              diff={selectedDiff}
              strategy={strategies[selectedDiff.file_path] || "merge"}
              onStrategyChange={(s) => onStrategyChange(selectedDiff.file_path, s)}
            />
          ) : (
            <div className="flex items-center justify-center h-full text-text-muted">
              No files to preview
            </div>
          )}
        </div>
      </div>

      <div className="flex items-center justify-between px-6 py-4 border-t border-border">
        <button
          onClick={onBack}
          className="px-4 py-2 text-sm text-text-muted hover:text-text-primary transition-colors"
        >
          ← Back
        </button>
        <div className="flex items-center gap-3">
          <span className="text-sm text-text-muted">
            {diffs.filter((d) => strategies[d.file_path] !== "skip").length} file
            {diffs.filter((d) => strategies[d.file_path] !== "skip").length !== 1 ? "s" : ""} will
            be modified
          </span>
          <button
            onClick={onDeploy}
            disabled={loading || diffs.length === 0}
            className="px-6 py-2 bg-accent-green text-white rounded-lg font-medium hover:bg-accent-green/80 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {loading ? "Deploying..." : "Deploy Now"}
          </button>
        </div>
      </div>
    </div>
  );
}

function FileItem({
  diff,
  selected,
  onClick,
}: {
  diff: DiffEntry;
  selected: boolean;
  onClick: () => void;
}) {
  const changeColors: Record<string, string> = {
    add: "bg-accent-green",
    modify: "bg-accent-blue",
    remove: "bg-accent-red",
  };

  const fileName = basename(diff.file_path) || diff.file_path;
  const isAgent = diff.file_path.includes("/agents/") && fileName.endsWith(".md");
  const isJson = fileName.endsWith(".json");

  return (
    <button
      onClick={onClick}
      className={`w-full flex items-center gap-2 px-3 py-2 rounded-lg text-left transition-colors ${
        selected
          ? "bg-accent-blue/20 text-text-primary"
          : "hover:bg-white/5 text-text-muted"
      }`}
    >
      <div className={`w-2 h-2 rounded-full ${changeColors[diff.change_type]}`} />
      <span className="text-sm truncate flex-1 font-mono">{fileName}</span>
      <div className="flex gap-1">
        {isAgent && (
          <span className="text-[9px] px-1 rounded bg-purple-500/20 text-purple-400">agent</span>
        )}
        {isJson && (
          <span className="text-[9px] px-1 rounded bg-blue-500/20 text-blue-400">json</span>
        )}
      </div>
    </button>
  );
}

function EnhancedDiffView({
  diff,
  strategy,
  onStrategyChange,
}: {
  diff: DiffEntry;
  strategy: string;
  onStrategyChange: (s: string) => void;
}) {
  const [viewMode, setViewMode] = useState<ViewMode>("split");

  const fileName = basename(diff.file_path) || "";
  const isAgent = diff.file_path.includes("/agents/") && fileName.endsWith(".md");

  const oldValue = diff.current_content || "";
  const newValue = diff.proposed_content || "";

  const darkStyles = {
    variables: {
      dark: {
        diffViewerBackground: "#0e0f13",
        diffViewerColor: "#e8e9ed",
        addedBackground: "rgba(34, 197, 94, 0.1)",
        addedColor: "#22c55e",
        removedBackground: "rgba(239, 68, 68, 0.1)",
        removedColor: "#ef4444",
        wordAddedBackground: "rgba(34, 197, 94, 0.3)",
        wordRemovedBackground: "rgba(239, 68, 68, 0.3)",
        addedGutterBackground: "rgba(34, 197, 94, 0.15)",
        removedGutterBackground: "rgba(239, 68, 68, 0.15)",
        gutterBackground: "#13141a",
        gutterBackgroundDark: "#0e0f13",
        highlightBackground: "rgba(59, 130, 246, 0.2)",
        highlightGutterBackground: "rgba(59, 130, 246, 0.3)",
        codeFoldGutterBackground: "#1a1b23",
        codeFoldBackground: "#1a1b23",
        emptyLineBackground: "#0e0f13",
        gutterColor: "#9394a1",
        addedGutterColor: "#22c55e",
        removedGutterColor: "#ef4444",
        codeFoldContentColor: "#9394a1",
        diffViewerTitleBackground: "#13141a",
        diffViewerTitleColor: "#e8e9ed",
        diffViewerTitleBorderColor: "#2a2b36",
      },
    },
    line: {
      padding: "4px 8px",
      fontSize: "12px",
      fontFamily: "JetBrains Mono, monospace",
    },
    gutter: {
      minWidth: "40px",
      padding: "0 8px",
      fontSize: "11px",
      fontFamily: "JetBrains Mono, monospace",
    },
    contentText: {
      fontFamily: "JetBrains Mono, monospace",
      fontSize: "12px",
    },
  };

  return (
    <div className="p-4 space-y-4">
      <div className="flex items-center justify-between">
        <div className="flex-1">
          <div className="flex items-center gap-2">
            <p className="font-mono text-sm text-text-primary">{diff.file_path}</p>
            {isAgent && (
              <span className="text-[10px] px-2 py-0.5 rounded-full bg-purple-500/20 text-purple-400 border border-purple-500/30">
                Agent Definition
              </span>
            )}
          </div>
          <p className="text-xs text-text-muted mt-1">
            {diff.change_type === "add" && "New file will be created"}
            {diff.change_type === "modify" && "File will be modified"}
            {diff.change_type === "remove" && "File will be removed"}
          </p>
        </div>

        <div className="flex items-center gap-3">
          <div className="flex bg-app-card rounded-lg border border-border overflow-hidden">
            <button
              onClick={() => setViewMode("split")}
              className={`px-3 py-1.5 text-xs transition-colors ${
                viewMode === "split"
                  ? "bg-accent-blue text-white"
                  : "text-text-muted hover:text-text-primary"
              }`}
            >
              Split
            </button>
            <button
              onClick={() => setViewMode("unified")}
              className={`px-3 py-1.5 text-xs transition-colors ${
                viewMode === "unified"
                  ? "bg-accent-blue text-white"
                  : "text-text-muted hover:text-text-primary"
              }`}
            >
              Unified
            </button>
            <button
              onClick={() => setViewMode("raw")}
              className={`px-3 py-1.5 text-xs transition-colors ${
                viewMode === "raw"
                  ? "bg-accent-blue text-white"
                  : "text-text-muted hover:text-text-primary"
              }`}
            >
              Raw
            </button>
          </div>

          <select
            value={strategy}
            onChange={(e) => onStrategyChange(e.target.value)}
            className="px-3 py-1.5 text-sm bg-app-card border border-border rounded-lg text-text-primary focus:outline-none focus:border-accent-blue"
          >
            <option value="merge">Merge</option>
            <option value="overwrite">Overwrite</option>
            <option value="skip">Skip</option>
          </select>
        </div>
      </div>

      {isAgent && diff.proposed_content && (
        <AgentPreviewBanner content={diff.proposed_content} />
      )}

      {viewMode === "raw" ? (
        <div className="rounded-lg border border-border overflow-hidden">
          <div className="bg-app-sidebar px-4 py-2 border-b border-border">
            <p className="text-xs text-text-muted uppercase">Final Content</p>
          </div>
          <pre className="p-4 bg-app-bg text-sm font-mono text-text-primary overflow-x-auto max-h-96 overflow-y-auto whitespace-pre-wrap">
            {newValue || "(empty)"}
          </pre>
        </div>
      ) : (
        <div className="rounded-lg border border-border overflow-hidden">
          <ReactDiffViewer
            oldValue={oldValue}
            newValue={newValue}
            splitView={viewMode === "split"}
            useDarkTheme={true}
            styles={darkStyles}
            compareMethod={DiffMethod.WORDS}
            showDiffOnly={false}
            leftTitle={diff.current_content ? "Current" : undefined}
            rightTitle={viewMode === "split" ? "Proposed" : undefined}
          />
        </div>
      )}
    </div>
  );
}

function AgentPreviewBanner({ content }: { content: string }) {
  const frontmatterMatch = content.match(/^---\n([\s\S]*?)\n---/);
  if (!frontmatterMatch) return null;

  const frontmatter = frontmatterMatch[1];
  const getName = (text: string) => {
    const match = text.match(/name:\s*(.+)/);
    return match ? match[1].trim() : null;
  };
  const getModel = (text: string) => {
    const match = text.match(/model:\s*(.+)/);
    return match ? match[1].trim() : null;
  };
  const getColor = (text: string) => {
    const match = text.match(/color:\s*(.+)/);
    return match ? match[1].trim() : null;
  };

  const name = getName(frontmatter);
  const model = getModel(frontmatter);
  const color = getColor(frontmatter);

  const promptStart = content.indexOf("---", 4);
  const promptContent = promptStart !== -1 ? content.slice(promptStart + 3).trim() : "";
  const promptPreview = promptContent.slice(0, 60) + (promptContent.length > 60 ? "..." : "");

  const colorMap: Record<string, string> = {
    red: "#ef4444",
    blue: "#3b82f6",
    green: "#22c55e",
    yellow: "#eab308",
    purple: "#9333ea",
    orange: "#f97316",
    pink: "#ec4899",
    cyan: "#06b6d4",
  };

  return (
    <div className="rounded-lg border-2 border-purple-500/30 bg-purple-500/5 p-4">
      <div className="flex items-center gap-3">
        <div
          className="w-10 h-10 rounded-lg flex items-center justify-center text-white font-bold"
          style={{ backgroundColor: colorMap[color || "blue"] || "#3b82f6" }}
        >
          {(name || "A")[0].toUpperCase()}
        </div>
        <div className="flex-1">
          <p className="font-medium text-text-primary">{name || "Agent"}</p>
          <p className="text-xs text-text-muted">
            {model && <span className="uppercase">{model}</span>}
          </p>
        </div>
      </div>
      {promptPreview && (
        <p className="mt-3 text-sm text-text-secondary italic">
          "{promptPreview}"
        </p>
      )}
    </div>
  );
}
