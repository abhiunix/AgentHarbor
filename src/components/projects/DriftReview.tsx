import { useState, useEffect } from "react";
import {
  detectDrift,
  acceptDrift,
  restoreDrift,
  getDriftDiff,
  type DriftInfo,
  type DriftFile,
  type DriftDiff,
} from "../../lib/tauri";

interface DriftReviewProps {
  projectPath: string;
  onClose: () => void;
  onResolved: () => void;
}

export function DriftReview({ projectPath, onClose, onResolved }: DriftReviewProps) {
  const [drift, setDrift] = useState<DriftInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [selectedFile, setSelectedFile] = useState<DriftFile | null>(null);
  const [fileDiff, setFileDiff] = useState<DriftDiff | null>(null);
  const [actionInProgress, setActionInProgress] = useState(false);

  useEffect(() => {
    loadDrift();
  }, [projectPath]);

  const loadDrift = async () => {
    setLoading(true);
    try {
      const info = await detectDrift(projectPath);
      setDrift(info);
      if (info.files.length > 0) {
        setSelectedFile(info.files[0]);
      }
    } catch (error) {
      console.error("Failed to load drift:", error);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (selectedFile) {
      loadFileDiff(selectedFile.relative_path);
    }
  }, [selectedFile]);

  const loadFileDiff = async (filePath: string) => {
    try {
      const diff = await getDriftDiff(projectPath, filePath);
      setFileDiff(diff);
    } catch (error) {
      console.error("Failed to load file diff:", error);
    }
  };

  const handleAccept = async () => {
    setActionInProgress(true);
    try {
      await acceptDrift(projectPath);
      onResolved();
    } catch (error) {
      console.error("Failed to accept drift:", error);
    } finally {
      setActionInProgress(false);
    }
  };

  const handleRestore = async () => {
    if (!confirm("This will overwrite current files with the last deployed versions. Continue?")) {
      return;
    }
    setActionInProgress(true);
    try {
      await restoreDrift(projectPath);
      onResolved();
    } catch (error) {
      console.error("Failed to restore drift:", error);
    } finally {
      setActionInProgress(false);
    }
  };

  if (loading) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
        <div className="bg-app-sidebar border border-border rounded-xl p-8">
          <p className="text-text-muted">Checking for drift...</p>
        </div>
      </div>
    );
  }

  if (!drift || !drift.has_drift) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
        <div className="bg-app-sidebar border border-border rounded-xl p-8 text-center">
          <p className="text-text-primary mb-4">No drift detected</p>
          <button
            onClick={onClose}
            className="px-4 py-2 bg-accent-blue text-white rounded-lg text-sm"
          >
            Close
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="bg-app-sidebar border border-border rounded-xl w-[900px] max-h-[80vh] flex flex-col">
        <div className="flex items-center justify-between px-6 py-4 border-b border-border">
          <div>
            <h2 className="text-lg font-semibold text-text-primary">Drift Detected</h2>
            <p className="text-sm text-text-muted">
              {drift.files.length} file{drift.files.length !== 1 ? "s" : ""} have changed since last deploy
            </p>
          </div>
          <button
            onClick={onClose}
            className="w-8 h-8 flex items-center justify-center rounded-full hover:bg-white/10 text-text-muted"
          >
            ✕
          </button>
        </div>

        <div className="flex flex-1 min-h-0">
          <div className="w-64 border-r border-border overflow-y-auto">
            <div className="p-2">
              {drift.files.map((file) => (
                <button
                  key={file.path}
                  onClick={() => setSelectedFile(file)}
                  className={`w-full text-left px-3 py-2 rounded-lg text-sm ${
                    selectedFile?.path === file.path
                      ? "bg-accent-blue/20 text-accent-blue"
                      : "text-text-secondary hover:bg-white/5"
                  }`}
                >
                  <div className="flex items-center gap-2">
                    <span
                      className={`text-xs px-1.5 py-0.5 rounded font-medium ${
                        file.change_type === "deleted"
                          ? "bg-accent-red/20 text-accent-red"
                          : "bg-accent-yellow/20 text-accent-yellow"
                      }`}
                    >
                      {file.change_type}
                    </span>
                    <span className="truncate font-mono text-xs">{file.relative_path}</span>
                  </div>
                </button>
              ))}
            </div>
          </div>

          <div className="flex-1 overflow-y-auto p-4">
            {selectedFile && fileDiff ? (
              <div className="space-y-4">
                <h3 className="text-sm font-medium text-text-primary font-mono">
                  {selectedFile.relative_path}
                </h3>
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <h4 className="text-xs font-semibold text-text-muted uppercase mb-2">
                      Expected (Last Deploy)
                    </h4>
                    <pre className="text-xs font-mono bg-app-bg border border-border rounded-lg p-3 overflow-auto max-h-96 text-text-secondary">
                      {fileDiff.expected || "(empty)"}
                    </pre>
                  </div>
                  <div>
                    <h4 className="text-xs font-semibold text-text-muted uppercase mb-2">
                      Current (On Disk)
                    </h4>
                    <pre className="text-xs font-mono bg-app-bg border border-border rounded-lg p-3 overflow-auto max-h-96 text-text-secondary">
                      {fileDiff.current || "(deleted)"}
                    </pre>
                  </div>
                </div>
              </div>
            ) : (
              <div className="h-full flex items-center justify-center text-text-muted">
                Select a file to view changes
              </div>
            )}
          </div>
        </div>

        <div className="px-6 py-4 border-t border-border flex items-center justify-between">
          <p className="text-xs text-text-muted">
            Accept current = update deploy state to match disk. Restore = revert files to last deploy.
          </p>
          <div className="flex gap-2">
            <button
              onClick={handleAccept}
              disabled={actionInProgress}
              className="px-4 py-2 text-sm bg-accent-green/20 text-accent-green rounded-lg hover:bg-accent-green/30 disabled:opacity-50"
            >
              Accept Current
            </button>
            <button
              onClick={handleRestore}
              disabled={actionInProgress}
              className="px-4 py-2 text-sm bg-accent-red/20 text-accent-red rounded-lg hover:bg-accent-red/30 disabled:opacity-50"
            >
              Restore Last Deploy
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
