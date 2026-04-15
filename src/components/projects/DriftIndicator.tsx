import { useState, useEffect } from "react";
import { detectDrift, type DriftInfo } from "../../lib/tauri";

interface DriftIndicatorProps {
  projectPath: string;
  onShowDrift?: () => void;
}

export function DriftIndicator({ projectPath, onShowDrift }: DriftIndicatorProps) {
  const [drift, setDrift] = useState<DriftInfo | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    checkDrift();
  }, [projectPath]);

  const checkDrift = async () => {
    setLoading(true);
    try {
      const info = await detectDrift(projectPath);
      setDrift(info);
    } catch (error) {
      console.error("Failed to check drift:", error);
    } finally {
      setLoading(false);
    }
  };

  if (loading || !drift || !drift.has_drift) {
    return null;
  }

  return (
    <button
      onClick={onShowDrift}
      className="flex items-center gap-1.5 px-2 py-1 rounded bg-accent-yellow/20 text-accent-yellow text-xs font-medium hover:bg-accent-yellow/30 transition-colors"
    >
      <span className="w-2 h-2 rounded-full bg-accent-yellow animate-pulse" />
      <span>Drift Detected ({drift.files.length} files)</span>
    </button>
  );
}
