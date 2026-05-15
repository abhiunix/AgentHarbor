import { useState, useEffect, useCallback } from "react";
import { useParams, Navigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { getAdapterPlugin } from "../lib/adapterPlugins";
import {
  readClaudeDesktopConfig,
  writeClaudeDesktopConfig,
  getClaudeDesktopConfigPath,
  listGlobalAgentMemory,
  clearAgentMemory,
  clearAllGlobalMemory,
  type AgentMemory,
} from "../lib/tauri";
import {
  AdapterConfigSection,
  type AdapterGlobalConfig,
} from "../components/global/AdapterConfigSection";

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
            {config && adapterId !== "claude-code" && (
              <AdapterConfigSection config={config} onRefresh={loadConfig} />
            )}
            {adapterId === "claude-code" && <GlobalAgentMemorySection />}
          </>
        )}
      </div>
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
