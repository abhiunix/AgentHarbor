import { useState, useEffect } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useUpdateStore } from "../../stores/updateStore";
import {
  getSettings,
  updateGeneralSettings,
  updateRegistrySettings,
  updateDeploySettings,
  updateAnalyticsSettings,
  syncRegistryNow,
  getSyncStatus,
  updateTrayVisibility,
  startRegistryPolling,
  stopRegistryPolling,
  type AppSettings as AppSettingsType,
  type GeneralSettings,
  type RegistrySettings,
  type DeploySettings,
  type AnalyticsSettings,
  type SyncState,
  type SyncConfig,
} from "../../lib/tauri";
import { useRegistryStore } from "../../stores/registryStore";
import { useAgentStore } from "../../stores/agentStore";
import { ImportExportModal } from "../common/ImportExportModal";
import { SecretsManager } from "./SecretsManager";
import {
  adapterPlugins,
  getEnabledAdapterIds,
  setEnabledAdapterIds,
} from "../../lib/adapterPlugins";
import { useDebugMode } from "../../hooks/useDebugMode";
import { credentialStoreName } from "../../lib/platform";

function SectionCard({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="bg-app-card border border-border rounded-lg overflow-hidden">
      <div className="px-4 py-3 border-b border-border">
        <h3 className="text-sm font-semibold text-text-primary uppercase tracking-wider">
          {title}
        </h3>
      </div>
      <div className="p-4 space-y-4">{children}</div>
    </div>
  );
}

function SettingRow({
  label,
  description,
  children,
}: {
  label: string;
  description?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-4">
      <div className="flex-1">
        <p className="text-sm text-text-primary">{label}</p>
        {description && (
          <p className="text-xs text-text-muted mt-0.5">{description}</p>
        )}
      </div>
      <div className="flex-shrink-0">{children}</div>
    </div>
  );
}

function Toggle({
  checked,
  onChange,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <button
      onClick={() => onChange(!checked)}
      className={`w-10 h-6 rounded-full relative transition-colors ${
        checked ? "bg-accent-blue" : "bg-border"
      }`}
    >
      <span
        className={`absolute top-1 w-4 h-4 rounded-full bg-white transition-transform ${
          checked ? "left-5" : "left-1"
        }`}
      />
    </button>
  );
}

function UpdatesCard() {
  const { latestVersion, isAvailable, lastChecked, isChecking, checkError, checkForUpdate, clearSnooze } =
    useUpdateStore();
  const [currentVersion, setCurrentVersion] = useState<string | null>(null);

  useEffect(() => {
    getVersion().then(setCurrentVersion).catch(() => {});
  }, []);

  return (
    <SectionCard title="Updates">
      <SettingRow
        label="Current version"
        description="AgentHarbor installed on this machine"
      >
        <span className="text-sm font-mono text-text-muted">
          {currentVersion ? `v${currentVersion}` : "—"}
        </span>
      </SettingRow>

      <SettingRow
        label="Status"
        description={
          lastChecked
            ? `Last checked ${lastChecked.toLocaleTimeString()}`
            : "Not yet checked"
        }
      >
        {isAvailable ? (
          <span className="text-sm font-medium text-accent-blue">
            v{latestVersion} available
          </span>
        ) : (
          <span className="text-sm text-accent-green">Up to date</span>
        )}
      </SettingRow>

      {checkError && (
        <p className="text-xs text-accent-red">{checkError}</p>
      )}

      <div className="flex items-center gap-2 pt-1">
        <button
          onClick={() => { clearSnooze(); checkForUpdate(); }}
          disabled={isChecking}
          className="px-4 py-2 text-sm bg-white/5 border border-border rounded-lg text-text-primary hover:bg-white/10 transition-colors disabled:opacity-50"
        >
          {isChecking ? "Checking…" : "Check Now"}
        </button>
      </div>
    </SectionCard>
  );
}

function DebugModeCard() {
  const [debugEnabled, setDebugEnabled] = useDebugMode();

  return (
    <SectionCard title="Developer">
      <SettingRow
        label="Debug Mode"
        description="Show source file paths on each page (e.g. ~/.claude/settings.json)"
      >
        <Toggle
          checked={debugEnabled}
          onChange={(checked) => setDebugEnabled(checked)}
        />
      </SettingRow>
    </SectionCard>
  );
}

function AdapterVisibilityCard() {
  const [enabledIds, setEnabled] = useState<string[]>(() => getEnabledAdapterIds());

  const handleToggle = (adapterId: string, checked: boolean) => {
    const next = checked
      ? [...enabledIds, adapterId]
      : enabledIds.filter((id) => id !== adapterId);
    setEnabled(next);
    setEnabledAdapterIds(next);
  };

  return (
    <SectionCard title="Adapters">
      <p className="text-xs text-text-muted -mt-2 mb-2">
        Choose which AI IDE adapters appear in the sidebar.
      </p>
      {adapterPlugins.map((plugin) => (
        <SettingRow
          key={plugin.id}
          label={plugin.name}
          description={`Show ${plugin.name} section in sidebar`}
        >
          <Toggle
            checked={enabledIds.includes(plugin.id)}
            onChange={(checked) => handleToggle(plugin.id, checked)}
          />
        </SettingRow>
      ))}
    </SectionCard>
  );
}

export function AppSettings() {
  const [settings, setSettings] = useState<AppSettingsType | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [syncState, setSyncState] = useState<SyncState | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [syncMessage, setSyncMessage] = useState<string | null>(null);
  const [githubPat, setGithubPat] = useState("");
  const [importExportMode, setImportExportMode] = useState<"export" | "import" | null>(null);
  const [showSecretsManager, setShowSecretsManager] = useState(false);
  
  const loadCapabilities = useRegistryStore((s) => s.loadCapabilities);
  const loadAgents = useAgentStore((s) => s.loadAgents);

  useEffect(() => {
    loadSettings();
    loadSyncStatus();
  }, []);

  const loadSettings = async () => {
    try {
      const data = await getSettings();
      setSettings(data);
    } catch (error) {
      console.error("Failed to load settings:", error);
    } finally {
      setLoading(false);
    }
  };
  
  const loadSyncStatus = async () => {
    try {
      const status = await getSyncStatus();
      setSyncState(status);
    } catch (error) {
      console.error("Failed to load sync status:", error);
    }
  };
  
  const handleSyncNow = async () => {
    if (!settings) return;
    setSyncing(true);
    setSyncMessage(null);
    
    try {
      const config: SyncConfig = {
        repo_url: settings.registry.github_repo.trim().replace(/[,;]+$/, ''),
        branch: settings.registry.github_branch.trim(),
        polling_interval_minutes: settings.registry.poll_interval_minutes,
        auto_update: settings.registry.auto_update,
        github_pat: githubPat || null,
      };
      
      const result = await syncRegistryNow(config);
      setSyncMessage(result.message);
      
      if (result.success) {
        await loadSyncStatus();
        loadCapabilities();
        loadAgents();
        
        handleRegistryChange({ last_sync: new Date().toISOString() });
      }
    } catch (error) {
      setSyncMessage(error instanceof Error ? error.message : "Sync failed");
    } finally {
      setSyncing(false);
    }
  };

  const handleGeneralChange = async (updates: Partial<GeneralSettings>) => {
    if (!settings) return;
    setSaving(true);
    try {
      const newGeneral = { ...settings.general, ...updates };
      const updated = await updateGeneralSettings(newGeneral);
      setSettings(updated);
    } catch (error) {
      console.error("Failed to save general settings:", error);
    } finally {
      setSaving(false);
    }
  };

  const buildSyncConfig = (reg: RegistrySettings): SyncConfig => ({
    repo_url: reg.github_repo.trim().replace(/[,;]+$/, ""),
    branch: reg.github_branch.trim(),
    polling_interval_minutes: Math.max(1, reg.poll_interval_minutes),
    auto_update: reg.auto_update,
    github_pat: githubPat || null,
  });

  const handleRegistryChange = async (updates: Partial<RegistrySettings>) => {
    if (!settings) return;
    setSaving(true);
    try {
      const newRegistry = { ...settings.registry, ...updates };
      const updated = await updateRegistrySettings(newRegistry);
      setSettings(updated);
      if (!updated.registry.auto_update) {
        await stopRegistryPolling();
      } else if (updated.registry.github_repo.trim()) {
        await stopRegistryPolling();
        await startRegistryPolling(buildSyncConfig(updated.registry));
      }
    } catch (error) {
      console.error("Failed to save registry settings:", error);
    } finally {
      setSaving(false);
    }
  };

  const handleDeployChange = async (updates: Partial<DeploySettings>) => {
    if (!settings) return;
    setSaving(true);
    try {
      const newDeploy = { ...settings.deploy, ...updates };
      const updated = await updateDeploySettings(newDeploy);
      setSettings(updated);
    } catch (error) {
      console.error("Failed to save deploy settings:", error);
    } finally {
      setSaving(false);
    }
  };

  const handleAnalyticsChange = async (updates: Partial<AnalyticsSettings>) => {
    if (!settings) return;
    setSaving(true);
    try {
      const newAnalytics = { ...settings.analytics, ...updates };
      const updated = await updateAnalyticsSettings(newAnalytics);
      setSettings(updated);
      // Also update the cache TTL in the backend
      const { invoke } = await import("@tauri-apps/api/core");
      await invoke("set_cursor_v2_cache_ttl", { seconds: newAnalytics.refresh_interval_minutes * 60 });
    } catch (error) {
      console.error("Failed to save analytics settings:", error);
    } finally {
      setSaving(false);
    }
  };

  if (loading) {
    return (
      <div className="p-6 text-center">
        <p className="text-text-muted">Loading settings...</p>
      </div>
    );
  }

  if (!settings) {
    return (
      <div className="p-6 text-center">
        <p className="text-accent-red">Failed to load settings</p>
      </div>
    );
  }

  return (
    <div className="p-6 max-w-3xl mx-auto space-y-6">
      <div className="flex items-center justify-between mb-6">
        <div>
          <h1 className="text-2xl font-bold text-text-primary">Settings</h1>
          <p className="text-text-muted text-sm">Configure AgentHarbor preferences</p>
        </div>
        {saving && (
          <span className="text-xs text-accent-blue">Saving...</span>
        )}
      </div>

      <SectionCard title="General">
        <SettingRow
          label="Username"
          description="Used as the author namespace for private capabilities and agents"
        >
          <input
            type="text"
            value={settings.general.username}
            onChange={(e) => handleGeneralChange({ username: e.target.value })}
            onBlur={() => handleGeneralChange({})}
            className="w-40 px-3 py-1.5 bg-app-bg border border-border rounded text-sm text-text-primary focus:outline-none focus:border-accent-blue"
          />
        </SettingRow>
        <SettingRow
          label="Launch at Login"
          description="Start AgentHarbor when you log in"
        >
          <Toggle
            checked={settings.general.launch_at_login}
            onChange={(checked) => handleGeneralChange({ launch_at_login: checked })}
          />
        </SettingRow>
        <SettingRow
          label="Show in Menu Bar"
          description="Display AgentHarbor icon in the system tray"
        >
          <Toggle
            checked={settings.general.show_in_menu_bar}
            onChange={async (checked) => {
              await handleGeneralChange({ show_in_menu_bar: checked });
              await updateTrayVisibility(checked);
            }}
          />
        </SettingRow>
        <SettingRow
          label="Keep Running on Close"
          description="Hide window instead of quitting when closing (requires system tray)"
        >
          <Toggle
            checked={settings.general.keep_running_on_close}
            onChange={(checked) => handleGeneralChange({ keep_running_on_close: checked })}
          />
        </SettingRow>
      </SectionCard>

      <SectionCard title="Registry Sync">
        <SettingRow
          label="GitHub Repository"
          description="Community registry source"
        >
          <input
            type="text"
            value={settings.registry.github_repo}
            onChange={(e) => handleRegistryChange({ github_repo: e.target.value })}
            placeholder="https://github.com/owner/repo"
            className="w-full min-w-[20rem] max-w-xl px-3 py-1.5 bg-app-bg border border-border rounded text-sm text-text-primary font-mono focus:outline-none focus:border-accent-blue"
          />
        </SettingRow>
        <SettingRow
          label="Branch"
          description="Git branch to sync from"
        >
          <input
            type="text"
            value={settings.registry.github_branch}
            onChange={(e) => handleRegistryChange({ github_branch: e.target.value })}
            placeholder="main"
            className="w-32 px-3 py-1.5 bg-app-bg border border-border rounded text-sm text-text-primary focus:outline-none focus:border-accent-blue"
          />
        </SettingRow>
        <SettingRow
          label="Poll Interval"
          description="How often to check for updates (minutes)"
        >
          <input
            type="number"
            min={1}
            max={1440}
            value={settings.registry.poll_interval_minutes}
            onChange={(e) => handleRegistryChange({ poll_interval_minutes: parseInt(e.target.value) || 60 })}
            className="w-20 px-3 py-1.5 bg-app-bg border border-border rounded text-sm text-text-primary text-center focus:outline-none focus:border-accent-blue"
          />
        </SettingRow>
        <SettingRow
          label="Auto-update"
          description="Automatically sync with community registry"
        >
          <Toggle
            checked={settings.registry.auto_update}
            onChange={(checked) => handleRegistryChange({ auto_update: checked })}
          />
        </SettingRow>
        <SettingRow
          label="GitHub PAT (Optional)"
          description="Personal access token for private repos or higher rate limits"
        >
          <input
            type="password"
            value={githubPat}
            onChange={(e) => setGithubPat(e.target.value)}
            placeholder="ghp_..."
            className="w-48 px-3 py-1.5 bg-app-bg border border-border rounded text-sm text-text-primary font-mono focus:outline-none focus:border-accent-blue"
          />
        </SettingRow>
        
        <div className="pt-2 border-t border-border mt-4">
          <div className="flex items-center justify-between">
            <div className="space-y-1">
              <div className="flex items-center gap-2">
                <span className={`w-2 h-2 rounded-full ${
                  syncState?.is_syncing ? "bg-accent-yellow animate-pulse" :
                  syncState?.last_error ? "bg-accent-red" :
                  syncState?.last_sync_time ? "bg-accent-green" : "bg-text-muted"
                }`} />
                <span className="text-sm text-text-secondary">
                  {syncState?.is_syncing ? "Syncing..." :
                   syncState?.last_error ? "Error" :
                   syncState?.last_sync_time ? "Connected" : "Not synced"}
                </span>
              </div>
              {syncState?.last_sync_time && (
                <p className="text-xs text-text-muted">
                  Last sync: {new Date(syncState.last_sync_time).toLocaleString()}
                </p>
              )}
              {syncState && !syncState.last_error && (syncState.capabilities_count > 0 || syncState.agents_count > 0) && (
                <p className="text-xs text-text-muted">
                  {syncState.capabilities_count} capabilities, {syncState.agents_count} agents from community
                </p>
              )}
              {syncState?.last_error && (
                <p className="text-xs text-accent-red">{syncState.last_error}</p>
              )}
              {syncMessage && (
                <p className={`text-xs ${syncMessage.includes("fail") || syncMessage.includes("error") ? "text-accent-red" : "text-accent-green"}`}>
                  {syncMessage}
                </p>
              )}
            </div>
            <button
              onClick={handleSyncNow}
              disabled={syncing}
              className={`px-4 py-2 text-sm rounded-lg transition-colors ${
                syncing
                  ? "bg-white/5 border border-border text-text-muted cursor-not-allowed"
                  : "bg-accent-blue text-white hover:bg-accent-blue/90"
              }`}
            >
              {syncing ? (
                <span className="flex items-center gap-2">
                  <svg className="animate-spin h-4 w-4" viewBox="0 0 24 24">
                    <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" fill="none" />
                    <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4zm2 5.291A7.962 7.962 0 014 12H0c0 3.042 1.135 5.824 3 7.938l3-2.647z" />
                  </svg>
                  Syncing...
                </span>
              ) : (
                "Sync Now"
              )}
            </button>
          </div>
        </div>
      </SectionCard>

      <SectionCard title="Deploy">
        <SettingRow
          label="Default Strategy"
          description="How to handle conflicts when deploying"
        >
          <select
            value={settings.deploy.default_strategy}
            onChange={(e) => handleDeployChange({ default_strategy: e.target.value })}
            className="px-3 py-1.5 bg-app-bg border border-border rounded text-sm text-text-primary focus:outline-none focus:border-accent-blue"
          >
            <option value="merge">Merge</option>
            <option value="overwrite">Overwrite</option>
            <option value="skip">Skip</option>
          </select>
        </SettingRow>
        <SettingRow
          label="Create Backups"
          description="Backup existing files before overwriting"
        >
          <Toggle
            checked={settings.deploy.create_backups}
            onChange={(checked) => handleDeployChange({ create_backups: checked })}
          />
        </SettingRow>
      </SectionCard>

      <SectionCard title="Analytics">
        <SettingRow
          label="Refresh Interval"
          description="How often to refresh analytics data (minutes)"
        >
          <input
            type="number"
            min={1}
            max={60}
            value={settings.analytics?.refresh_interval_minutes ?? 5}
            onChange={(e) => {
              const val = Math.max(1, Math.min(60, parseInt(e.target.value) || 5));
              handleAnalyticsChange({ refresh_interval_minutes: val });
            }}
            className="w-20 px-3 py-1.5 bg-app-bg border border-border rounded text-sm text-text-primary text-center focus:outline-none focus:border-accent-blue"
          />
        </SettingRow>
        <SettingRow
          label="Claude experimental features"
          description="Show Anthropic experimental usage windows (omelette, tangelo, …) on Claude analytics"
        >
          <Toggle
            checked={settings.analytics?.claude_experimental_features ?? false}
            onChange={(checked) =>
              handleAnalyticsChange({ claude_experimental_features: checked })
            }
          />
        </SettingRow>
        <SettingRow
          label="Limit notifications"
          description="macOS notifications when you approach or hit usage limits (Claude, Codex, …)"
        >
          <Toggle
            checked={settings.analytics?.limit_notifications_enabled ?? true}
            onChange={(checked) =>
              handleAnalyticsChange({ limit_notifications_enabled: checked })
            }
          />
        </SettingRow>
      </SectionCard>

      <AdapterVisibilityCard />

      <SectionCard title="Secrets">
        <SettingRow
          label="Stored Secrets"
          description={`API keys and tokens stored in ${credentialStoreName}`}
        >
          <span className="text-sm text-text-muted">
            {settings.secrets.count} secret{settings.secrets.count !== 1 ? "s" : ""}
          </span>
        </SettingRow>
        <div className="pt-2">
          <button
            onClick={() => setShowSecretsManager(true)}
            className="px-4 py-2 text-sm bg-white/5 border border-border rounded-lg text-text-primary hover:bg-white/10 transition-colors"
          >
            Manage Secrets
          </button>
        </div>
      </SectionCard>

      <SectionCard title="Data Management">
        <SettingRow
          label="Export Private Data"
          description="Export your custom capabilities, agents, and presets to a JSON file (excludes public community data)"
        >
          <button
            onClick={() => setImportExportMode("export")}
            className="px-4 py-2 text-sm bg-white/5 border border-border rounded-lg text-text-primary hover:bg-white/10 transition-colors"
          >
            Export
          </button>
        </SettingRow>
        <SettingRow
          label="Import Private Data"
          description="Import custom capabilities, agents, and presets from a JSON file"
        >
          <button
            onClick={() => setImportExportMode("import")}
            className="px-4 py-2 text-sm bg-white/5 border border-border rounded-lg text-text-primary hover:bg-white/10 transition-colors"
          >
            Import
          </button>
        </SettingRow>
      </SectionCard>

      <DebugModeCard />

      <UpdatesCard />

      {importExportMode && (
        <ImportExportModal
          mode={importExportMode}
          onClose={() => setImportExportMode(null)}
          onComplete={() => {
            loadCapabilities();
            loadAgents();
          }}
        />
      )}

      <SecretsManager
        isOpen={showSecretsManager}
        onClose={() => {
          setShowSecretsManager(false);
          loadSettings();
        }}
      />
    </div>
  );
}
