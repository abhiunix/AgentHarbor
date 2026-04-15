import { useState, useEffect, useCallback } from "react";
import {
  listClaudePlugins,
  listCursorExtensions,
  listCursorPlugins,
  toggleClaudePlugin,
} from "../lib/tauri";
import type { ClaudePlugin, CursorExtension, CursorPlugin } from "../lib/tauri";

type Tab = "claude" | "cursor-ext" | "cursor-plug";

export function ExtensionsPage() {
  const [activeTab, setActiveTab] = useState<Tab>("claude");
  const [search, setSearch] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [claudePlugins, setClaudePlugins] = useState<ClaudePlugin[]>([]);
  const [cursorExtensions, setCursorExtensions] = useState<CursorExtension[]>([]);
  const [cursorPlugins, setCursorPlugins] = useState<CursorPlugin[]>([]);

  const loadAll = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [plugins, extensions, cPlugins] = await Promise.all([
        listClaudePlugins(),
        listCursorExtensions(),
        listCursorPlugins(),
      ]);
      setClaudePlugins(plugins);
      setCursorExtensions(extensions);
      setCursorPlugins(cPlugins);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadAll();
  }, [loadAll]);

  const handleTogglePlugin = async (name: string, enabled: boolean) => {
    try {
      await toggleClaudePlugin(name, enabled);
      const updated = await listClaudePlugins();
      setClaudePlugins(updated);
    } catch (e) {
      setError(String(e));
    }
  };

  const lowerSearch = search.toLowerCase();

  const filteredClaude = claudePlugins.filter((p) =>
    p.name.toLowerCase().includes(lowerSearch)
  );
  const filteredCursorExt = cursorExtensions.filter((e) =>
    e.name.toLowerCase().includes(lowerSearch)
  );
  const filteredCursorPlug = cursorPlugins.filter((p) =>
    p.name.toLowerCase().includes(lowerSearch)
  );

  const tabs: { id: Tab; label: string; count: number }[] = [
    { id: "claude", label: "Claude Plugins", count: claudePlugins.length },
    { id: "cursor-ext", label: "Cursor Extensions", count: cursorExtensions.length },
    { id: "cursor-plug", label: "Cursor Plugins", count: cursorPlugins.length },
  ];

  return (
    <div className="h-full flex flex-col">
      <div className="px-6 pt-6 pb-0">
        <div className="flex items-center justify-between mb-4">
          <div>
            <h1 className="text-2xl font-semibold text-text-primary">Plugins & Extensions</h1>
            <p className="text-sm text-text-secondary mt-1">
              Browse installed plugins and extensions across your AI tools
            </p>
          </div>
          <button
            onClick={loadAll}
            disabled={loading}
            className="flex items-center gap-2 px-3 py-2 rounded-lg border border-border text-text-primary text-sm hover:bg-app-card-hover disabled:opacity-50"
          >
            <span className="text-base">↻</span> Refresh
          </button>
        </div>

        <div className="relative mb-4">
          <input
            type="text"
            placeholder="Search by name..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="w-full px-4 py-2 bg-app-card border border-border rounded-lg text-text-primary text-sm placeholder:text-text-secondary focus:outline-none focus:border-accent-blue"
          />
        </div>

        <div className="flex gap-1 border-b border-border">
          {tabs.map((tab) => (
            <button
              key={tab.id}
              onClick={() => setActiveTab(tab.id)}
              className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
                activeTab === tab.id
                  ? "border-accent-blue text-text-primary"
                  : "border-transparent text-text-secondary hover:text-text-primary"
              }`}
            >
              {tab.label}
              <span className="ml-1.5 text-xs text-text-secondary">({tab.count})</span>
            </button>
          ))}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto p-6">
        {error && (
          <div className="mb-4 px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400">
            {error}
          </div>
        )}

        {loading ? (
          <p className="text-text-secondary text-sm">Loading...</p>
        ) : activeTab === "claude" ? (
          <ClaudePluginGrid plugins={filteredClaude} onToggle={handleTogglePlugin} />
        ) : activeTab === "cursor-ext" ? (
          <CursorExtensionGrid extensions={filteredCursorExt} />
        ) : (
          <CursorPluginGrid plugins={filteredCursorPlug} />
        )}
      </div>
    </div>
  );
}

function ClaudePluginGrid({
  plugins,
  onToggle,
}: {
  plugins: ClaudePlugin[];
  onToggle: (name: string, enabled: boolean) => void;
}) {
  if (plugins.length === 0) {
    return <p className="text-text-secondary text-sm">No Claude plugins found.</p>;
  }

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      {plugins.map((plugin) => (
        <div
          key={plugin.name}
          className="bg-app-card border border-border rounded-lg p-4 hover:bg-app-card-hover transition-colors"
        >
          <div className="flex items-start justify-between mb-2">
            <h3 className="text-sm font-semibold text-text-primary truncate mr-2">
              {plugin.name}
            </h3>
            <button
              onClick={() => onToggle(plugin.name, !plugin.enabled)}
              className={`relative inline-flex h-5 w-9 flex-shrink-0 rounded-full transition-colors ${
                plugin.enabled ? "bg-accent-blue" : "bg-border"
              }`}
            >
              <span
                className={`inline-block h-4 w-4 rounded-full bg-white transition-transform mt-0.5 ${
                  plugin.enabled ? "translate-x-4 ml-0.5" : "translate-x-0.5"
                }`}
              />
            </button>
          </div>
          <div className="flex items-center gap-2 mb-2">
            <span className="text-xs text-text-secondary font-mono">v{plugin.version}</span>
            <ScopeBadge scope={plugin.scope} />
          </div>
          <p className="text-xs text-text-secondary">
            Installed {formatDate(plugin.installed_at)}
          </p>
        </div>
      ))}
    </div>
  );
}

function CursorExtensionGrid({ extensions }: { extensions: CursorExtension[] }) {
  if (extensions.length === 0) {
    return <p className="text-text-secondary text-sm">No Cursor extensions found.</p>;
  }

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      {extensions.map((ext) => (
        <div
          key={ext.id}
          className="bg-app-card border border-border rounded-lg p-4 hover:bg-app-card-hover transition-colors"
        >
          <h3 className="text-sm font-semibold text-text-primary truncate mb-2">
            {ext.name}
          </h3>
          <div className="flex items-center gap-2 mb-2">
            <span className="text-xs text-text-secondary font-mono">v{ext.version}</span>
            <span className="text-xs text-text-secondary">by {ext.publisher}</span>
          </div>
          <div className="flex items-center gap-2">
            <SourceBadge source={ext.source} />
            {ext.is_builtin && (
              <span className="px-1.5 py-0.5 text-[10px] font-medium rounded bg-amber-500/20 text-amber-400">
                Built-in
              </span>
            )}
          </div>
        </div>
      ))}
    </div>
  );
}

function CursorPluginGrid({ plugins }: { plugins: CursorPlugin[] }) {
  if (plugins.length === 0) {
    return <p className="text-text-secondary text-sm">No Cursor plugins found.</p>;
  }

  return (
    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
      {plugins.map((plugin) => (
        <div
          key={plugin.path}
          className="bg-app-card border border-border rounded-lg p-4 hover:bg-app-card-hover transition-colors"
        >
          <h3 className="text-sm font-semibold text-text-primary truncate mb-2">
            {plugin.name}
          </h3>
          <p className="text-xs text-text-secondary font-mono truncate" title={plugin.path}>
            {truncatePath(plugin.path)}
          </p>
        </div>
      ))}
    </div>
  );
}

function ScopeBadge({ scope }: { scope: string }) {
  const colors: Record<string, string> = {
    user: "bg-blue-500/20 text-blue-400",
    project: "bg-green-500/20 text-green-400",
    local: "bg-purple-500/20 text-purple-400",
  };
  const cls = colors[scope] ?? "bg-gray-500/20 text-gray-400";
  return (
    <span className={`px-1.5 py-0.5 text-[10px] font-medium rounded ${cls}`}>
      {scope}
    </span>
  );
}

function SourceBadge({ source }: { source: string }) {
  const colors: Record<string, string> = {
    marketplace: "bg-blue-500/20 text-blue-400",
    builtin: "bg-amber-500/20 text-amber-400",
  };
  const cls = colors[source] ?? "bg-gray-500/20 text-gray-400";
  return (
    <span className={`px-1.5 py-0.5 text-[10px] font-medium rounded ${cls}`}>
      {source}
    </span>
  );
}

function truncatePath(path: string, maxLen = 50): string {
  if (path.length <= maxLen) return path;
  const parts = path.split(/[/\\]/);
  if (parts.length <= 3) return "..." + path.slice(-maxLen);
  return ".../" + parts.slice(-3).join("/");
}

function formatDate(dateStr: string): string {
  try {
    const d = new Date(dateStr);
    return d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
  } catch {
    return dateStr;
  }
}
