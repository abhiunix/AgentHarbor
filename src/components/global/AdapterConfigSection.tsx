import { useCallback, useEffect, useState } from "react";
import {
  readGlobalConfigRaw,
  writeGlobalConfigRaw,
  addGlobalMcpServer,
  removeGlobalMcpServer,
} from "../../lib/tauri";
import { ConfirmDialog } from "../common/ConfirmDialog";

export interface AdapterGlobalConfig {
  id: string;
  name: string;
  color: string;
  globalPath: string;
  mcpServers: string[];
  hasConfig: boolean;
}

type ConfigTab = "mcp" | "raw";

export function AdapterConfigSection({
  config,
  onRefresh,
}: {
  config: AdapterGlobalConfig;
  onRefresh: () => void;
}) {
  const [tab, setTab] = useState<ConfigTab>("mcp");
  const [rawJson, setRawJson] = useState("");
  const [rawLoading, setRawLoading] = useState(false);
  const [rawSaving, setRawSaving] = useState(false);
  const [rawError, setRawError] = useState<string | null>(null);
  const [addModalOpen, setAddModalOpen] = useState(false);
  const [addName, setAddName] = useState("");
  const [addConfig, setAddConfig] = useState("{}");
  const [removeConfirm, setRemoveConfirm] = useState<string | null>(null);

  const loadRaw = useCallback(async () => {
    setRawLoading(true);
    setRawError(null);
    try {
      const text = await readGlobalConfigRaw(config.id);
      setRawJson(text);
    } catch (e) {
      setRawError(e instanceof Error ? e.message : String(e));
    } finally {
      setRawLoading(false);
    }
  }, [config.id]);

  useEffect(() => {
    if (tab === "raw") loadRaw();
  }, [tab, loadRaw]);

  const handleSaveRaw = async () => {
    setRawSaving(true);
    setRawError(null);
    try {
      await writeGlobalConfigRaw(config.id, rawJson);
      onRefresh();
    } catch (e) {
      setRawError(e instanceof Error ? e.message : String(e));
    } finally {
      setRawSaving(false);
    }
  };

  const handleAddServer = async () => {
    const name = addName.trim();
    if (!name) return;
    let configObj: Record<string, unknown>;
    try {
      configObj = JSON.parse(addConfig.trim() || "{}") as Record<string, unknown>;
    } catch (e) {
      setRawError("Invalid JSON: " + (e instanceof Error ? e.message : String(e)));
      return;
    }
    setRawError(null);
    try {
      await addGlobalMcpServer(config.id, name, configObj);
      setAddModalOpen(false);
      setAddName("");
      setAddConfig("{}");
      onRefresh();
      if (tab === "raw") loadRaw();
    } catch (e) {
      setRawError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleRemoveServerConfirm = async () => {
    if (!removeConfirm) return;
    try {
      await removeGlobalMcpServer(config.id, removeConfirm);
      onRefresh();
      if (tab === "raw") loadRaw();
    } catch (e) {
      setRawError(e instanceof Error ? e.message : String(e));
    } finally {
      setRemoveConfirm(null);
    }
  };

  return (
    <div className="space-y-6">
      <div className="bg-app-card border border-border rounded-lg p-4">
        <div className="flex items-center justify-between mb-4">
          <div>
            <p className="text-xs text-text-muted uppercase">Global Config Path</p>
            <p className="font-mono text-sm text-text-primary mt-1">{config.globalPath}</p>
          </div>
          <div className="flex items-center gap-2">
            {config.hasConfig ? (
              <span className="text-xs px-2 py-1 rounded bg-accent-green/20 text-accent-green">Configured</span>
            ) : (
              <span className="text-xs px-2 py-1 rounded bg-white/10 text-text-muted">Not configured</span>
            )}
          </div>
        </div>
      </div>

      <div className="flex gap-1 p-1 bg-app-bg rounded-lg w-fit">
        <button
          type="button"
          onClick={() => setTab("mcp")}
          className={`px-3 py-2 rounded text-sm font-medium transition-colors ${
            tab === "mcp" ? "bg-accent-blue text-white" : "text-text-secondary hover:text-text-primary"
          }`}
        >
          MCP Servers
        </button>
        <button
          type="button"
          onClick={() => setTab("raw")}
          className={`px-3 py-2 rounded text-sm font-medium transition-colors ${
            tab === "raw" ? "bg-accent-blue text-white" : "text-text-secondary hover:text-text-primary"
          }`}
        >
          Raw JSON
        </button>
      </div>

      {tab === "mcp" && (
        <div>
          <div className="flex items-center justify-between mb-3">
            <p className="text-xs text-text-muted uppercase">MCP Servers ({config.mcpServers.length})</p>
            <div className="flex gap-2">
              <button onClick={onRefresh} className="text-xs text-accent-blue hover:text-accent-blue/80 transition-colors">Refresh</button>
              <button onClick={() => { setRawError(null); setAddModalOpen(true); }} className="text-xs text-accent-blue hover:underline">Add server</button>
            </div>
          </div>
          {config.mcpServers.length > 0 ? (
            <div className="space-y-2">
              {config.mcpServers.map((server) => (
                <div key={server} className="flex items-center gap-3 p-3 bg-app-card border border-border rounded-lg">
                  <div className="w-2 h-2 rounded-full bg-accent-blue" />
                  <span className="font-mono text-sm text-text-primary flex-1">{server}</span>
                  <span className="text-[10px] uppercase px-2 py-0.5 rounded bg-white/5 text-text-muted">mcp</span>
                  <button onClick={() => setRemoveConfirm(server)} className="text-xs text-accent-red hover:underline">Remove</button>
                </div>
              ))}
            </div>
          ) : (
            <div className="p-8 bg-app-card border border-border rounded-lg text-center">
              <p className="text-text-muted">No global MCP servers configured</p>
              <p className="text-xs text-text-muted mt-2">Add a server below or edit Raw JSON.</p>
            </div>
          )}
        </div>
      )}

      {tab === "raw" && (
        <div>
          <div className="flex items-center justify-between mb-2">
            <p className="text-xs text-text-muted uppercase">Raw config</p>
            <div className="flex gap-2">
              <button onClick={loadRaw} disabled={rawLoading} className="text-xs text-accent-blue hover:underline disabled:opacity-50">Refresh</button>
              <button onClick={handleSaveRaw} disabled={rawLoading || rawSaving} className="px-3 py-1.5 rounded bg-accent-blue text-white text-xs font-medium disabled:opacity-50">
                {rawSaving ? "Saving..." : "Save"}
              </button>
            </div>
          </div>
          {rawError && <p className="text-xs text-accent-red mb-2">{rawError}</p>}
          {rawLoading ? (
            <div className="h-48 flex items-center justify-center text-text-muted text-sm">Loading...</div>
          ) : (
            <textarea
              value={rawJson}
              onChange={(e) => setRawJson(e.target.value)}
              rows={14}
              className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg font-mono text-sm text-text-primary focus:outline-none focus:border-accent-blue resize-y"
            />
          )}
        </div>
      )}

      {addModalOpen && (
        <>
          <div className="fixed inset-0 bg-black/40 z-40" onClick={() => setAddModalOpen(false)} />
          <div className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 z-50 w-full max-w-md bg-app-card border border-border rounded-xl p-6 shadow-xl">
            <h3 className="text-lg font-semibold text-text-primary mb-4">Add MCP server</h3>
            {rawError && <p className="text-xs text-accent-red mb-3">{rawError}</p>}
            <div className="space-y-3">
              <div>
                <label className="block text-sm text-text-secondary mb-1">Server name (key)</label>
                <input
                  type="text"
                  value={addName}
                  onChange={(e) => setAddName(e.target.value)}
                  placeholder="my-server"
                  className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg text-text-primary font-mono text-sm"
                />
              </div>
              <div>
                <label className="block text-sm text-text-secondary mb-1">Config JSON</label>
                <textarea
                  value={addConfig}
                  onChange={(e) => setAddConfig(e.target.value)}
                  rows={6}
                  placeholder='{"command":"npx","args":["-y","@modelcontextprotocol/server-example"]}'
                  className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg font-mono text-sm text-text-primary resize-y"
                />
              </div>
            </div>
            <div className="flex justify-end gap-2 mt-4">
              <button onClick={() => setAddModalOpen(false)} className="px-3 py-2 rounded border border-border text-text-primary text-sm hover:bg-app-card-hover">Cancel</button>
              <button onClick={handleAddServer} disabled={!addName.trim()} className="px-3 py-2 rounded bg-accent-blue text-white text-sm font-medium hover:bg-accent-blue/90 disabled:opacity-50">Add</button>
            </div>
          </div>
        </>
      )}

      <ConfirmDialog
        isOpen={!!removeConfirm}
        title="Remove MCP Server"
        message={removeConfirm ? `Are you sure you want to remove the MCP server "${removeConfirm}"? This action cannot be undone.` : ""}
        onConfirm={handleRemoveServerConfirm}
        onCancel={() => setRemoveConfirm(null)}
      />
    </div>
  );
}
