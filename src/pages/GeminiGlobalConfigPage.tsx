import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DebugPath } from "../components/common/DebugPath";

const GEMINI_COLOR = "#4285f4";

type Tab = "mcp" | "raw";

export function GeminiGlobalConfigPage() {
  const [activeTab, setActiveTab] = useState<Tab>("mcp");

  // MCP servers state
  const [servers, setServers] = useState<string[]>([]);
  const [mcpLoading, setMcpLoading] = useState(true);
  const [mcpError, setMcpError] = useState<string | null>(null);

  // Add modal state
  const [showAddModal, setShowAddModal] = useState(false);
  const [newServerName, setNewServerName] = useState("");
  const [newServerConfig, setNewServerConfig] = useState("{}");
  const [addError, setAddError] = useState<string | null>(null);

  // Raw settings state
  const [rawContent, setRawContent] = useState("");
  const [savedRawContent, setSavedRawContent] = useState("");
  const [rawLoading, setRawLoading] = useState(false);
  const [rawSaving, setRawSaving] = useState(false);
  const [rawError, setRawError] = useState<string | null>(null);

  const loadServers = useCallback(async () => {
    setMcpLoading(true);
    setMcpError(null);
    try {
      const list = await invoke<string[]>("get_gemini_mcp_servers");
      setServers(list);
    } catch (e) {
      setMcpError(e instanceof Error ? e.message : String(e));
    } finally {
      setMcpLoading(false);
    }
  }, []);

  const loadRaw = useCallback(async () => {
    setRawLoading(true);
    setRawError(null);
    try {
      const text = await invoke<string>("read_gemini_settings");
      setRawContent(text);
      setSavedRawContent(text);
    } catch (e) {
      setRawError(e instanceof Error ? e.message : String(e));
    } finally {
      setRawLoading(false);
    }
  }, []);

  useEffect(() => {
    loadServers();
    loadRaw();
  }, [loadServers, loadRaw]);

  const handleAddServer = async () => {
    if (!newServerName.trim()) return;
    setAddError(null);
    try {
      const configObj = JSON.parse(newServerConfig);
      await invoke("add_gemini_mcp_server", { name: newServerName.trim(), config: configObj });
      setShowAddModal(false);
      setNewServerName("");
      setNewServerConfig("{}");
      loadServers();
      loadRaw();
    } catch (e) {
      setAddError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleRemoveServer = async (name: string) => {
    setMcpError(null);
    try {
      await invoke("remove_gemini_mcp_server", { name });
      loadServers();
      loadRaw();
    } catch (e) {
      setMcpError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleSaveRaw = async () => {
    setRawSaving(true);
    setRawError(null);
    try {
      await invoke("write_gemini_settings", { content: rawContent });
      setSavedRawContent(rawContent);
      loadServers();
    } catch (e) {
      setRawError(e instanceof Error ? e.message : String(e));
    } finally {
      setRawSaving(false);
    }
  };

  const rawDirty = rawContent !== savedRawContent;

  const tabs: { id: Tab; label: string }[] = [
    { id: "mcp", label: "MCP Servers" },
    { id: "raw", label: "Raw Settings" },
  ];

  return (
    <div className="h-full flex flex-col">
      <div className="p-6 border-b border-border">
        <div className="flex items-center gap-2 mb-1">
          <span className="w-3 h-3 rounded-full" style={{ backgroundColor: GEMINI_COLOR }} />
          <h1 className="text-2xl font-semibold text-text-primary">Gemini CLI — Global Config</h1>
        </div>
        <DebugPath path="~/.gemini/settings.json" className="text-sm" />
      </div>

      {/* Warning banner */}
      <div className="mx-6 mt-4 px-4 py-3 bg-amber-500/10 border border-amber-500/30 rounded-lg text-sm text-amber-400">
        Changes here affect Gemini CLI globally across all projects.
      </div>

      {/* Tabs */}
      <div className="px-6 mt-4 flex gap-1 border-b border-border">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
              activeTab === tab.id
                ? "text-text-primary"
                : "border-transparent text-text-muted hover:text-text-primary"
            }`}
            style={
              activeTab === tab.id
                ? { borderBottomColor: GEMINI_COLOR }
                : undefined
            }
          >
            {tab.label}
          </button>
        ))}
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        {activeTab === "mcp" ? (
          <McpServersTab
            servers={servers}
            loading={mcpLoading}
            error={mcpError}
            onRemove={handleRemoveServer}
            onRefresh={loadServers}
            showAddModal={showAddModal}
            onShowAdd={() => setShowAddModal(true)}
            onCloseAdd={() => {
              setShowAddModal(false);
              setAddError(null);
              setNewServerName("");
              setNewServerConfig("{}");
            }}
            newServerName={newServerName}
            onNewNameChange={setNewServerName}
            newServerConfig={newServerConfig}
            onNewConfigChange={setNewServerConfig}
            addError={addError}
            onAdd={handleAddServer}
          />
        ) : (
          <RawSettingsTab
            content={rawContent}
            onChange={setRawContent}
            loading={rawLoading}
            saving={rawSaving}
            error={rawError}
            dirty={rawDirty}
            onSave={handleSaveRaw}
            onRefresh={loadRaw}
          />
        )}
      </div>
    </div>
  );
}

function McpServersTab({
  servers,
  loading,
  error,
  onRemove,
  onRefresh,
  showAddModal,
  onShowAdd,
  onCloseAdd,
  newServerName,
  onNewNameChange,
  newServerConfig,
  onNewConfigChange,
  addError,
  onAdd,
}: {
  servers: string[];
  loading: boolean;
  error: string | null;
  onRemove: (name: string) => void;
  onRefresh: () => void;
  showAddModal: boolean;
  onShowAdd: () => void;
  onCloseAdd: () => void;
  newServerName: string;
  onNewNameChange: (v: string) => void;
  newServerConfig: string;
  onNewConfigChange: (v: string) => void;
  addError: string | null;
  onAdd: () => void;
}) {
  return (
    <div>
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-lg font-semibold text-text-primary">MCP Servers</h2>
        <div className="flex items-center gap-2">
          <button
            onClick={onRefresh}
            disabled={loading}
            className="text-sm text-accent-blue hover:underline disabled:opacity-50"
          >
            Refresh
          </button>
          <button
            onClick={onShowAdd}
            className="px-4 py-2 rounded-lg text-white text-sm font-medium hover:opacity-90"
            style={{ backgroundColor: GEMINI_COLOR }}
          >
            + Add Server
          </button>
        </div>
      </div>

      {error && <p className="text-sm text-accent-red mb-3">{error}</p>}

      {/* Add modal */}
      {showAddModal && (
        <div className="mb-4 p-4 bg-app-card border border-border rounded-lg space-y-3">
          <h3 className="text-sm font-semibold text-text-primary">Add MCP Server</h3>
          <div>
            <label className="block text-xs text-text-muted mb-1">Server Name</label>
            <input
              type="text"
              value={newServerName}
              onChange={(e) => onNewNameChange(e.target.value)}
              placeholder="my-mcp-server"
              className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg text-text-primary text-sm font-mono focus:outline-none focus:border-accent-blue"
            />
          </div>
          <div>
            <label className="block text-xs text-text-muted mb-1">Config (JSON)</label>
            <textarea
              value={newServerConfig}
              onChange={(e) => onNewConfigChange(e.target.value)}
              rows={4}
              className="w-full px-3 py-2 bg-app-bg border border-border rounded-lg text-text-primary text-sm font-mono focus:outline-none focus:border-accent-blue resize-y"
              placeholder='{ "command": "npx", "args": [...] }'
            />
          </div>
          {addError && <p className="text-sm text-accent-red">{addError}</p>}
          <div className="flex items-center gap-2 justify-end">
            <button
              onClick={onCloseAdd}
              className="px-3 py-2 rounded-lg border border-border text-text-muted text-sm hover:text-text-primary"
            >
              Cancel
            </button>
            <button
              onClick={onAdd}
              disabled={!newServerName.trim()}
              className="px-4 py-2 rounded-lg text-white text-sm font-medium hover:opacity-90 disabled:opacity-50"
              style={{ backgroundColor: GEMINI_COLOR }}
            >
              Add
            </button>
          </div>
        </div>
      )}

      {loading ? (
        <div className="h-32 flex items-center justify-center text-text-muted">Loading...</div>
      ) : servers.length === 0 ? (
        <div className="py-12 text-center text-text-muted text-sm">
          No MCP servers configured. Click "+ Add Server" to get started.
        </div>
      ) : (
        <div className="space-y-2">
          {servers.map((name) => (
            <div
              key={name}
              className="flex items-center justify-between p-4 bg-app-card border border-border rounded-lg hover:bg-card-hover transition-colors"
            >
              <div className="flex items-center gap-3">
                <span
                  className="w-2 h-2 rounded-full"
                  style={{ backgroundColor: GEMINI_COLOR }}
                />
                <span className="text-sm font-mono text-text-primary">{name}</span>
              </div>
              <button
                onClick={() => onRemove(name)}
                className="text-xs text-text-muted hover:text-red-400 transition-colors"
              >
                Remove
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

function RawSettingsTab({
  content,
  onChange,
  loading,
  saving,
  error,
  dirty,
  onSave,
  onRefresh,
}: {
  content: string;
  onChange: (v: string) => void;
  loading: boolean;
  saving: boolean;
  error: string | null;
  dirty: boolean;
  onSave: () => void;
  onRefresh: () => void;
}) {
  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between mb-3">
        <h2 className="text-lg font-semibold text-text-primary">Raw Settings</h2>
        <div className="flex items-center gap-2">
          {dirty && (
            <span className="text-xs text-amber-400 font-medium">Unsaved changes</span>
          )}
          <button
            onClick={onRefresh}
            disabled={loading}
            className="text-sm text-accent-blue hover:underline disabled:opacity-50"
          >
            Refresh
          </button>
          <button
            onClick={onSave}
            disabled={loading || saving || !dirty}
            className="px-4 py-2 rounded-lg bg-accent-blue text-white text-sm font-medium hover:bg-accent-blue/90 disabled:opacity-50"
          >
            {saving ? "Saving..." : "Save"}
          </button>
        </div>
      </div>

      {error && <p className="text-sm text-accent-red mb-2">{error}</p>}

      {loading ? (
        <div className="h-64 flex items-center justify-center text-text-muted">Loading...</div>
      ) : (
        <textarea
          value={content}
          onChange={(e) => onChange(e.target.value)}
          className="w-full flex-1 min-h-[300px] px-4 py-3 bg-app-card border border-border rounded-lg font-mono text-sm text-text-primary focus:outline-none focus:border-accent-blue resize-y"
          placeholder="{ }"
        />
      )}
    </div>
  );
}
