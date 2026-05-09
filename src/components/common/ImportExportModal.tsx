import { useState, useRef } from "react";
import {
  exportData,
  importData,
  validateImportData,
  type ExportData,
  type ImportResult,
} from "../../lib/tauri";
import { save } from "@tauri-apps/plugin-dialog";

interface ImportExportModalProps {
  mode: "export" | "import";
  onClose: () => void;
  onComplete?: () => void;
}

export function ImportExportModal({ mode, onClose, onComplete }: ImportExportModalProps) {
  const [step, setStep] = useState<"select" | "preview" | "result">("select");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  
  const [exportAll, setExportAll] = useState(true);
  
  const [importData_, setImportData] = useState<ExportData | null>(null);
  const [renameConflicts, setRenameConflicts] = useState(true);
  const [importResult, setImportResult] = useState<ImportResult | null>(null);
  
  const fileInputRef = useRef<HTMLInputElement>(null);

  const handleExport = async () => {
    setLoading(true);
    setError(null);

    try {
      const filePath = await save({
        defaultPath: `agentharbor-export-${new Date().toISOString().split("T")[0]}.json`,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });

      if (filePath) {
        await exportData([], [], [], filePath);
        setStep("result");
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleFileSelect = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;
    
    setLoading(true);
    setError(null);
    
    try {
      const text = await file.text();
      const data = await validateImportData(text);
      setImportData(data);
      setStep("preview");
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  const handleImport = async () => {
    if (!importData_) return;
    
    setLoading(true);
    setError(null);
    
    try {
      const result = await importData(importData_, renameConflicts);
      setImportResult(result);
      setStep("result");
      onComplete?.();
    } catch (err) {
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-app-card border border-border rounded-xl w-full max-w-lg shadow-2xl">
        <div className="px-6 py-4 border-b border-border flex items-center justify-between">
          <h2 className="text-lg font-semibold text-text-primary">
            {mode === "export" ? "Export Private Data" : "Import Private Data"}
          </h2>
          <button onClick={onClose} className="text-text-muted hover:text-text-primary">
            ✕
          </button>
        </div>

        <div className="p-6">
          {mode === "export" && step === "select" && (
            <div className="space-y-4">
              <p className="text-text-secondary text-sm">
                Export your custom capabilities, agents, and presets to a JSON file.
                Only private data is exported; public community data is excluded.
              </p>
              
              <div className="p-4 bg-app-bg rounded-lg">
                <label className="flex items-center gap-3 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={exportAll}
                    onChange={(e) => setExportAll(e.target.checked)}
                    className="w-4 h-4 rounded border-border accent-accent-blue"
                  />
                  <div>
                    <p className="text-sm text-text-primary font-medium">Export all items</p>
                    <p className="text-xs text-text-muted">
                      Includes all capabilities, agents, and presets
                    </p>
                  </div>
                </label>
              </div>

              {error && (
                <p className="text-sm text-accent-red">{error}</p>
              )}

              <button
                onClick={handleExport}
                disabled={loading}
                className="w-full py-3 bg-accent-blue text-white rounded-lg font-medium hover:bg-accent-blue/90 transition-colors disabled:opacity-50"
              >
                {loading ? "Exporting..." : "Export to File"}
              </button>
            </div>
          )}

          {mode === "export" && step === "result" && (
            <div className="text-center space-y-4">
              <div className="w-16 h-16 mx-auto rounded-full bg-accent-green/20 flex items-center justify-center">
                <span className="text-3xl">✓</span>
              </div>
              <p className="text-text-primary font-medium">Export Complete</p>
              <p className="text-text-secondary text-sm">
                Your data has been exported successfully.
              </p>
              <button
                onClick={onClose}
                className="px-6 py-2 bg-accent-blue text-white rounded-lg"
              >
                Done
              </button>
            </div>
          )}

          {mode === "import" && step === "select" && (
            <div className="space-y-4">
              <p className="text-text-secondary text-sm">
                Import capabilities, agents, and presets from a JSON file.
              </p>
              
              <input
                ref={fileInputRef}
                type="file"
                accept=".json"
                onChange={handleFileSelect}
                className="hidden"
              />
              
              <button
                onClick={() => fileInputRef.current?.click()}
                disabled={loading}
                className="w-full py-8 border-2 border-dashed border-border rounded-lg text-text-muted hover:border-accent-blue hover:text-accent-blue transition-colors"
              >
                {loading ? "Reading file..." : "Click to select a JSON file"}
              </button>

              {error && (
                <p className="text-sm text-accent-red">{error}</p>
              )}
            </div>
          )}

          {mode === "import" && step === "preview" && importData_ && (
            <div className="space-y-4">
              <p className="text-text-secondary text-sm">
                Review the items to import:
              </p>
              
              <div className="space-y-2">
                <div className="flex items-center justify-between p-3 bg-app-bg rounded-lg">
                  <span className="text-sm text-text-primary">Capabilities</span>
                  <span className="text-sm font-mono text-text-muted">
                    {importData_.capabilities.length}
                  </span>
                </div>
                <div className="flex items-center justify-between p-3 bg-app-bg rounded-lg">
                  <span className="text-sm text-text-primary">Agents</span>
                  <span className="text-sm font-mono text-text-muted">
                    {importData_.agents.length}
                  </span>
                </div>
                <div className="flex items-center justify-between p-3 bg-app-bg rounded-lg">
                  <span className="text-sm text-text-primary">Presets</span>
                  <span className="text-sm font-mono text-text-muted">
                    {importData_.presets.length}
                  </span>
                </div>
              </div>

              <label className="flex items-center gap-3 cursor-pointer p-3 bg-app-bg rounded-lg">
                <input
                  type="checkbox"
                  checked={renameConflicts}
                  onChange={(e) => setRenameConflicts(e.target.checked)}
                  className="w-4 h-4 rounded border-border accent-accent-blue"
                />
                <div>
                  <p className="text-sm text-text-primary">Rename conflicts</p>
                  <p className="text-xs text-text-muted">
                    Add "-imported" suffix to items that already exist
                  </p>
                </div>
              </label>

              {error && (
                <p className="text-sm text-accent-red">{error}</p>
              )}

              <div className="flex gap-3">
                <button
                  onClick={() => setStep("select")}
                  className="flex-1 py-2 border border-border rounded-lg text-text-primary hover:bg-app-card-hover"
                >
                  Back
                </button>
                <button
                  onClick={handleImport}
                  disabled={loading}
                  className="flex-1 py-2 bg-accent-blue text-white rounded-lg font-medium hover:bg-accent-blue/90 disabled:opacity-50"
                >
                  {loading ? "Importing..." : "Import"}
                </button>
              </div>
            </div>
          )}

          {mode === "import" && step === "result" && importResult && (
            <div className="space-y-4">
              <div className="text-center">
                <div className={`w-16 h-16 mx-auto rounded-full flex items-center justify-center ${
                  importResult.success ? "bg-accent-green/20" : "bg-accent-yellow/20"
                }`}>
                  <span className="text-3xl">{importResult.success ? "✓" : "!"}</span>
                </div>
                <p className="text-text-primary font-medium mt-3">
                  {importResult.success ? "Import Complete" : "Import Completed with Issues"}
                </p>
                <p className="text-text-secondary text-sm mt-1">
                  {importResult.message}
                </p>
              </div>

              {importResult.conflicts.length > 0 && (
                <div className="max-h-40 overflow-y-auto space-y-1">
                  {importResult.conflicts.map((conflict, idx) => (
                    <div
                      key={idx}
                      className="flex items-center gap-2 p-2 bg-app-bg rounded text-xs"
                    >
                      <span className="text-accent-yellow">⚠</span>
                      <span className="text-text-muted">{conflict.item_type}:</span>
                      <span className="text-text-secondary font-mono">{conflict.item_id}</span>
                      <span className="text-text-muted">— {conflict.message}</span>
                    </div>
                  ))}
                </div>
              )}

              <button
                onClick={onClose}
                className="w-full py-2 bg-accent-blue text-white rounded-lg"
              >
                Done
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
