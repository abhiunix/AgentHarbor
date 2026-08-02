import { BrowserRouter, Routes, Route, Navigate, useNavigate } from "react-router-dom";
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { AppLayout } from "./components/layout/AppLayout";
import { TestBridge } from "./components/common/TestBridge";
import { TrayPopover } from "./components/tray/TrayPopover";
import { RegistryPage } from "./pages/RegistryPage";
import { AgentsPage } from "./pages/AgentsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { ProjectsPage } from "./pages/ProjectsPage";
import { PresetPage } from "./pages/PresetPage";
import { NotesPage } from "./pages/NotesPage";
import { DebatePage } from "./pages/DebatePage";
import { RecommendationsPage } from "./pages/RecommendationsPage";
import { OptimizePage } from "./pages/OptimizePage";
import { AdapterFeaturePage } from "./pages/AdapterFeaturePage";
import { useRegistryStore } from "./stores/registryStore";
import { useAgentStore } from "./stores/agentStore";
import { usePresetStore } from "./stores/presetStore";
import { syncRegistryNow, getSettings, startRegistryPolling, getSyncStatus, pruneTranscriptBackups } from "./lib/tauri";
import { startUpdatePolling, stopUpdatePolling } from "./stores/updateStore";
import { initDebateRunListeners } from "./stores/debateRunStore";
import type { SyncConfig } from "./lib/tauri";
import {
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";

function AppInitializer({ children }: { children: React.ReactNode }) {
  const loadCapabilities = useRegistryStore((s) => s.loadCapabilities);
  const loadAgents = useAgentStore((s) => s.loadAgents);
  const loadPresets = usePresetStore((s) => s.loadPresets);

  useEffect(() => {
    loadCapabilities();
    loadAgents();
    loadPresets();
  }, [loadCapabilities, loadAgents, loadPresets]);

  useEffect(() => {
    startUpdatePolling();
    return () => stopUpdatePolling();
  }, []);

  // Delete transcript secret-scrub backups older than 7 days (fire-and-forget).
  useEffect(() => {
    pruneTranscriptBackups().catch(() => {});
  }, []);

  // Attach debate:* listeners ONCE for the app's lifetime so an in-flight debate
  // keeps updating the store even when the user navigates away from DebatePage.
  useEffect(() => {
    initDebateRunListeners().catch((e) => {
      console.error("Failed to init debate-run listeners", e);
    });
  }, []);

  // Ask once for notification permission when limit alerts are enabled (macOS).
  useEffect(() => {
    (async () => {
      try {
        const settings = await getSettings();
        if (settings.analytics?.limit_notifications_enabled === false) return;
        let granted = await isPermissionGranted();
        if (!granted) {
          const result = await requestPermission();
          granted = result === "granted";
        }
        if (!granted) {
          console.info("Notification permission not granted — limit alerts may be silent.");
        }
      } catch {
        /* ignore */
      }
    })();
  }, []);

  // Start background registry polling on app load when auto-update is enabled
  useEffect(() => {
    (async () => {
      try {
        const settings = await getSettings();
        const repoUrl = settings.registry.github_repo.trim().replace(/[,;]+$/, "");
        if (settings.registry.auto_update && repoUrl) {
          const config: SyncConfig = {
            repo_url: repoUrl,
            branch: settings.registry.github_branch.trim(),
            polling_interval_minutes: Math.max(1, settings.registry.poll_interval_minutes),
            auto_update: true,
            github_pat: null,
          };
          await startRegistryPolling(config);
        }
      } catch (e) {
        console.error("Failed to start registry polling:", e);
      }
    })();
  }, []);

  // Listen for registry-updated (manual Sync Now) and poll sync status so UI updates after background sync
  useEffect(() => {
    let cancelled = false;
    let unlistenFn: (() => void) | null = null;
    let lastSyncTime: string | null = null;

    const unlistenUpdated = listen("registry-updated", () => {
      if (!cancelled) {
        loadCapabilities();
        loadAgents();
      }
    });
    unlistenUpdated.then((fn) => {
      if (cancelled) { fn(); } else { unlistenFn = fn; }
    });

    const intervalId = setInterval(async () => {
      if (cancelled) return;
      try {
        const status = await getSyncStatus();
        if (status.last_sync_time && status.last_sync_time !== lastSyncTime) {
          lastSyncTime = status.last_sync_time;
          loadCapabilities();
          loadAgents();
        }
      } catch {
        // ignore
      }
    }, 45_000);
    return () => {
      cancelled = true;
      if (unlistenFn) unlistenFn();
      clearInterval(intervalId);
    };
  }, [loadCapabilities, loadAgents]);

  useEffect(() => {
    let cancelled = false;
    let unlistenDeployFn: (() => void) | null = null;
    let unlistenSyncFn: (() => void) | null = null;

    const unlistenDeploy = listen("open-deploy-wizard", () => {
      if (!cancelled) useRegistryStore.getState().openDeployWizard();
    });
    unlistenDeploy.then((fn) => {
      if (cancelled) { fn(); } else { unlistenDeployFn = fn; }
    });

    const unlistenSync = listen("sync-registry", async () => {
      if (cancelled) return;
      try {
        const settings = await getSettings();
        await syncRegistryNow({
          repo_url: settings.registry.github_repo,
          branch: settings.registry.github_branch,
          polling_interval_minutes: settings.registry.poll_interval_minutes,
          auto_update: settings.registry.auto_update,
          github_pat: null,
        });
        loadCapabilities();
        loadAgents();
      } catch (error) {
        console.error("Sync from tray failed:", error);
      }
    });
    unlistenSync.then((fn) => {
      if (cancelled) { fn(); } else { unlistenSyncFn = fn; }
    });

    return () => {
      cancelled = true;
      if (unlistenDeployFn) unlistenDeployFn();
      if (unlistenSyncFn) unlistenSyncFn();
    };
  }, [loadCapabilities, loadAgents]);

  return <>{children}</>;
}

/** Listens for "navigate-to" events from the tray popover and navigates accordingly. */
function NavigateListener() {
  const navigate = useNavigate();

  useEffect(() => {
    let cancelled = false;
    let unlistenFn: (() => void) | null = null;

    const unlistenPromise = listen<string>("navigate-to", async (event) => {
      if (!cancelled && event.payload) {
        navigate(event.payload);
        // Restores the macOS activation policy too, which a plain show() can't do
        await invoke("show_main_window").catch(() => {});
        getCurrentWindow().setFocus().catch(() => {});
      }
    });
    unlistenPromise.then((fn) => {
      if (cancelled) { fn(); } else { unlistenFn = fn; }
    });

    return () => {
      cancelled = true;
      if (unlistenFn) unlistenFn();
    };
  }, [navigate]);

  return null;
}

function App() {
  const [windowLabel, setWindowLabel] = useState<string | null>(null);

  useEffect(() => {
    try {
      const label = getCurrentWindow().label;
      setWindowLabel(label);
    } catch {
      setWindowLabel("main");
    }
  }, []);

  // Waiting for window label detection
  if (windowLabel === null) return null;

  // Tray popover renders its own minimal UI
  if (windowLabel === "tray-popover") {
    return <TrayPopover />;
  }

  return (
    <BrowserRouter>
      <TestBridge />
      <NavigateListener />
      <AppInitializer>
        <Routes>
          <Route path="/" element={<AppLayout />}>
            <Route index element={<RegistryPage />} />
            <Route path="registry" element={<RegistryPage />} />
            <Route path="agents" element={<AgentsPage />} />
            <Route path="presets/*" element={<PresetPage />} />
            <Route path="projects" element={<ProjectsPage />} />
            <Route path="recommendations" element={<RecommendationsPage />} />
            <Route path="optimize" element={<OptimizePage />} />
            <Route path="notes" element={<NotesPage />} />
            <Route path="debate" element={<DebatePage />} />

            {/* ── Adapter feature routes ────────────────── */}
            <Route
              path="adapters/:adapterId/:featureId"
              element={<AdapterFeaturePage />}
            />

            {/* ── Legacy redirects (old flat routes → new adapter routes) ── */}
            <Route path="global" element={<Navigate to="/adapters/claude-code/global-config" replace />} />
            <Route path="memory" element={<Navigate to="/adapters/claude-code/instructions" replace />} />
            <Route path="permissions" element={<Navigate to="/adapters/claude-code/permissions" replace />} />
            <Route path="extensions" element={<Navigate to="/adapters/gemini/extensions" replace />} />
            <Route path="usage" element={<Navigate to="/adapters/claude-code/usage" replace />} />
            <Route path="ai-attribution" element={<Navigate to="/adapters/cursor/attribution" replace />} />
            <Route path="prompts" element={<Navigate to="/adapters/claude-code/prompts" replace />} />
            <Route path="transcripts" element={<Navigate to="/adapters/claude-code/transcripts" replace />} />
            <Route path="plans" element={<Navigate to="/adapters/claude-code/plans" replace />} />

            <Route path="settings" element={<SettingsPage />} />
          </Route>
        </Routes>
      </AppInitializer>
    </BrowserRouter>
  );
}

export default App;
