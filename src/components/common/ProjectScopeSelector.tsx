import { useState } from "react";
import { selectProjectFolder } from "../../lib/tauri";

export interface ProjectScopeSelectorProps {
  value: string | null;
  onChange: (projectPath: string | null) => void;
  className?: string;
}

function truncatePath(path: string, maxLen: number): string {
  if (path.length <= maxLen) return path;
  return "…" + path.slice(-maxLen + 1);
}

export function ProjectScopeSelector({
  value,
  onChange,
  className = "",
}: ProjectScopeSelectorProps) {
  const [browseLoading, setBrowseLoading] = useState(false);

  const handleBrowse = async () => {
    setBrowseLoading(true);
    try {
      const path = await selectProjectFolder();
      if (path) onChange(path);
    } finally {
      setBrowseLoading(false);
    }
  };

  const handleClear = () => {
    onChange(null);
  };

  return (
    <div className={`flex items-center gap-2 flex-wrap ${className}`}>
      <span className="text-sm text-text-muted whitespace-nowrap">Scope:</span>
      {value == null ? (
        <>
          <span className="text-sm text-text-secondary">Global (All Projects)</span>
          <button
            type="button"
            onClick={handleBrowse}
            disabled={browseLoading}
            className="px-3 py-2 bg-app-card border border-border rounded-lg text-text-primary text-sm hover:bg-card-hover focus:outline-none focus:border-accent-blue disabled:opacity-50"
          >
            {browseLoading ? "Opening…" : "Browse project"}
          </button>
        </>
      ) : (
        <>
          <span
            className="text-sm font-mono text-text-secondary truncate max-w-[280px]"
            title={value}
          >
            {truncatePath(value, 42)}
          </span>
          <button
            type="button"
            onClick={handleClear}
            className="text-sm text-text-muted hover:text-text-primary focus:outline-none"
          >
            Clear
          </button>
          <button
            type="button"
            onClick={handleBrowse}
            disabled={browseLoading}
            className="px-3 py-2 bg-app-card border border-border rounded-lg text-text-primary text-sm hover:bg-card-hover focus:outline-none focus:border-accent-blue disabled:opacity-50"
          >
            {browseLoading ? "Opening…" : "Browse project"}
          </button>
        </>
      )}
    </div>
  );
}
