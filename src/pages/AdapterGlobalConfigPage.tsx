import { useState, useEffect, useCallback } from "react";
import { useParams, Navigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { getAdapterPlugin } from "../lib/adapterPlugins";
import {
  readClaudeSettings,
  writeClaudeSettings,
  readClaudeDesktopConfig,
  writeClaudeDesktopConfig,
  getClaudeDesktopConfigPath,
  readGlobalConfigRaw,
  writeGlobalConfigRaw,
  addGlobalMcpServer,
  removeGlobalMcpServer,
  listGlobalAgentMemory,
  clearAgentMemory,
  clearAllGlobalMemory,
  type AgentMemory,
} from "../lib/tauri";
import { ConfirmDialog } from "../components/common/ConfirmDialog";

// ── Types ────────────────────────────────────────────────────────────────────

interface AdapterGlobalConfig {
  id: string;
  name: string;
  color: string;
  globalPath: string;
  mcpServers: string[];
  hasConfig: boolean;
}

const ADAPTER_META: Record<string, { globalPath: string }> = {
  "claude-code": { globalPath: "~/.claude.json" },
  cursor: { globalPath: "~/.cursor/mcp.json" },
  windsurf: { globalPath: "~/.codeium/windsurf/mcp_config.json" },
  "claude-desktop": { globalPath: "Claude Desktop Config" },
};

// ── Main page ────────────────────────────────────────────────────────────────

export function AdapterGlobalConfigPage() {
  const { adapterId } = useParams<{ adapterId: string }>();
  const plugin = adapterId ? getAdapterPlugin(adapterId) : undefined;

  if (!plugin) return <Navigate to="/" replace />;

  return <AdapterGlobalConfigInner adapterId={plugin.id} plugin={plugin} />;
}

function AdapterGlobalConfigInner({
  adapterId,
  plugin,
}: {
  adapterId: string;
  plugin: ReturnType<typeof getAdapterPlugin> & {};
}) {
  const [config, setConfig] = useState<AdapterGlobalConfig | null>(null);
  const [loading, setLoading] = useState(true);

  const meta = ADAPTER_META[adapterId];

  const loadConfig = useCallback(async () => {
    setLoading(true);
    try {
      // Claude Desktop doesn't go through get_global_config
      if (adapterId === "claude-desktop") {
        const raw = await readClaudeDesktopConfig().catch(() => "{}");
        let mcpServers: string[] = [];
        try {
          const json = JSON.parse(raw || "{}");
          const mcp = json?.mcpServers;
          if (typeof mcp === "object" && mcp !== null) {
            mcpServers = Object.keys(mcp);
          }
        } catch { /* noop */ }
        setConfig({
          id: adapterId,
          name: plugin.name,
          color: plugin.color,
          globalPath: meta?.globalPath ?? "",
          mcpServers,
          hasConfig: mcpServers.length > 0,
        });
      } else {
        const result = await invoke<{ mcp_servers: string[]; has_config: boolean }>(
          "get_global_config",
          { adapterId }
        );
        setConfig({
          id: adapterId,
          name: plugin.name,
          color: plugin.color,
          globalPath: meta?.globalPath ?? "",
          mcpServers: result.mcp_servers,
          hasConfig: result.has_config,
        });
      }
    } catch {
      setConfig({
        id: adapterId,
        name: plugin.name,
        color: plugin.color,
        globalPath: meta?.globalPath ?? "",
        mcpServers: [],
        hasConfig: false,
      });
    } finally {
      setLoading(false);
    }
  }, [adapterId, plugin.name, plugin.color, meta?.globalPath]);

  useEffect(() => {
    loadConfig();
  }, [loadConfig]);

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="p-6 border-b border-border">
        <div className="flex items-center gap-3 mb-2">
          <span
            className="w-3 h-3 rounded-full"
            style={{ backgroundColor: plugin.color }}
          />
          <h1 className="text-2xl font-semibold text-text-primary">
            {plugin.name} — Global Config
          </h1>
        </div>
        <p className="text-text-muted">
          Manage global MCP servers and settings for {plugin.name}.
        </p>
      </div>

      {/* Warning */}
      <div className="mx-6 mt-4 p-4 bg-accent-orange/10 border border-accent-orange/30 rounded-lg">
        <div className="flex items-start gap-3">
          <span className="text-accent-orange text-lg">⚠️</span>
          <div>
            <p className="font-medium text-accent-orange">Changes affect all projects</p>
            <p className="text-sm text-text-muted mt-1">
              Global configuration applies to every project that uses {plugin.name}.
              Use project-level configuration for project-specific settings.
            </p>
          </div>
        </div>
      </div>

      {/* Body */}
      <div className="p-6 flex-1 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center py-12">
            <div className="animate-spin w-8 h-8 border-2 border-accent-blue border-t-transparent rounded-full" />
          </div>
        ) : (
          <>
            {adapterId === "claude-desktop" && (
              <ClaudeDesktopConfigSection className="mb-6" onRefresh={loadConfig} />
            )}
            {adapterId === "claude-code" && (
              <ClaudeSettingsSection className="mb-6" />
            )}
            {config && (
              <AdapterConfigSection config={config} onRefresh={loadConfig} />
            )}
            {adapterId === "claude-code" && <GlobalAgentMemorySection />}
          </>
        )}
      </div>
    </div>
  );
}

// ── Claude Settings (~/.claude/settings.json) ────────────────────────────────

function ClaudeSettingsSection({ className = "" }: { className?: string }) {
  const [content, setContent] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const text = await readClaudeSettings();
      setContent(text);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try { await writeClaudeSettings(content); }
    catch (e) { setError(e instanceof Error ? e.message : String(e)); }
    finally { setSaving(false); }
  };

  return (
    <div className={`bg-app-card border border-border rounded-lg p-4 ${className}`}>
      <div className="flex items-center justify-between mb-3">
        <div>
          <h3 className="text-sm font-semibold text-text-primary">Claude Settings</h3>
          <p className="text-xs text-text-muted">~/.claude/settings.json</p>
        </div>
        <div className="flex gap-2">
          <button onClick={load} disabled={loading} className="text-xs text-accent-blue hover:underline disabled:opacity-50">
            Refresh
          </button>
          <button onClick={handleSave} disabled={loading || saving} className="px-3 py-1.5 rounded bg-accent-blue text-white text-xs font-medium hover:bg-accent-blue/90 disabled:opacity-50">
            {saving ? "Saving..." : "Save"}
          </button>
        </div>
      </div>
      {error && <p className="text-xs text-accent-red mb-2">{error}</p>}
      {loading ? (
        <div className="h-32 flex items-center justify-center text-text-muted text-sm">Loading...</div>
      ) : (
        <textarea
          value={content}
          onChange={(e) => setContent(e.target.value)}
          rows={8}
          className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg font-mono text-sm text-text-primary focus:outline-none focus:border-accent-blue resize-y"
        />
      )}
    </div>
  );
}

// ── Claude Desktop config ────────────────────────────────────────────────────

function ClaudeDesktopConfigSection({
  className = "",
  onRefresh,
}: {
  className?: string;
  onRefresh?: () => void;
}) {
  const [content, setContent] = useState("");
  const [path, setPath] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [text, configPath] = await Promise.all([
        readClaudeDesktopConfig(),
        getClaudeDesktopConfigPath(),
      ]);
      setContent(text);
      setPath(configPath);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      await writeClaudeDesktopConfig(content);
      onRefresh?.();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className={`bg-app-card border border-border rounded-lg p-4 ${className}`}>
      <div className="flex items-center justify-between mb-3">
        <div>
          <h3 className="text-sm font-semibold text-text-primary">Claude Desktop app config</h3>
          <p className="text-xs text-text-muted font-mono truncate max-w-md" title={path}>{path || "—"}</p>
        </div>
        <div className="flex gap-2">
          <button onClick={load} disabled={loading} className="text-xs text-accent-blue hover:underline disabled:opacity-50">Refresh</button>
          <button onClick={handleSave} disabled={loading || saving} className="px-3 py-1.5 rounded bg-accent-blue text-white text-xs font-medium hover:bg-accent-blue/90 disabled:opacity-50">
            {saving ? "Saving..." : "Save"}
          </button>
        </div>
      </div>
      {error && <p className="text-xs text-accent-red mb-2">{error}</p>}
      {loading ? (
        <div className="h-32 flex items-center justify-center text-text-muted text-sm">Loading...</div>
      ) : (
        <textarea
          value={content}
          onChange={(e) => setContent(e.target.value)}
          rows={8}
          className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg font-mono text-sm text-text-primary focus:outline-none focus:border-accent-blue resize-y"
        />
      )}
    </div>
  );
}

// ── Generic adapter MCP config section ───────────────────────────────────────

type ConfigTab = "mcp" | "raw";

function AdapterConfigSection({
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

      {/* Add MCP server modal */}
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

// ── Global Agent Memory (Claude Code only) ───────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

function GlobalAgentMemorySection() {
  const [memories, setMemories] = useState<AgentMemory[]>([]);
  const [loading, setLoading] = useState(true);
  const [clearing, setClearing] = useState<string | null>(null);

  useEffect(() => { loadMemory(); }, []);

  const loadMemory = async () => {
    setLoading(true);
    try {
      const data = await listGlobalAgentMemory();
      setMemories(data);
    } catch (error) {
      console.error("Failed to load global agent memory:", error);
    } finally {
      setLoading(false);
    }
  };

  const handleClear = async (memory: AgentMemory) => {
    if (!confirm(`Clear memory for agent "${memory.agent_name}"? This cannot be undone.`)) return;
    setClearing(memory.path);
    try {
      await clearAgentMemory(memory.path);
      loadMemory();
    } catch (error) {
      console.error("Failed to clear memory:", error);
    } finally {
      setClearing(null);
    }
  };

  const handleClearAll = async () => {
    if (!confirm("Clear ALL global agent memory? This cannot be undone.")) return;
    setClearing("all");
    try {
      await clearAllGlobalMemory();
      loadMemory();
    } catch (error) {
      console.error("Failed to clear all memory:", error);
    } finally {
      setClearing(null);
    }
  };

  const totalSize = memories.reduce((sum, m) => sum + m.size_bytes, 0);

  return (
    <div className="mt-8 bg-app-card border border-border rounded-lg p-4">
      <div className="flex items-center justify-between mb-4">
        <div>
          <h3 className="text-sm font-semibold text-text-primary">Global Agent Memory</h3>
          <p className="text-xs text-text-muted mt-0.5">User-scoped memory stored in ~/.claude/agent-memory/</p>
        </div>
        {memories.length > 0 && (
          <button onClick={handleClearAll} disabled={clearing !== null} className="text-xs text-accent-red hover:underline disabled:opacity-50">
            Clear All ({formatBytes(totalSize)})
          </button>
        )}
      </div>
      {loading ? (
        <p className="text-sm text-text-muted">Loading...</p>
      ) : memories.length === 0 ? (
        <p className="text-sm text-text-muted">No global agent memory found</p>
      ) : (
        <div className="space-y-2">
          {memories.map((memory) => (
            <div key={memory.path} className="flex items-center justify-between p-3 rounded-lg bg-app-bg">
              <div className="flex-1 min-w-0">
                <p className="text-sm text-text-primary truncate font-mono">{memory.agent_name}</p>
                <p className="text-xs text-text-muted">
                  {formatBytes(memory.size_bytes)} · {memory.file_count} file{memory.file_count !== 1 ? "s" : ""}
                </p>
              </div>
              <button
                onClick={() => handleClear(memory)}
                disabled={clearing !== null}
                className="ml-2 px-2 py-1 text-xs text-accent-red hover:bg-accent-red/10 rounded disabled:opacity-50"
              >
                {clearing === memory.path ? "..." : "Clear"}
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
