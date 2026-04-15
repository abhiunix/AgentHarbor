import { useEffect, useCallback, useRef } from "react";
import { useNavigate, useLocation } from "react-router-dom";
import { useRegistryStore } from "../stores/registryStore";
import { useAgentStore } from "../stores/agentStore";
import { syncRegistryNow } from "../lib/tauri";
import { modKey } from "../lib/platform";

export interface ShortcutHandlers {
  onOpenDeploy?: () => void;
  onOpenNewMenu?: () => void;
  onFocusSearch?: () => void;
  onToggleShortcutsHelp?: () => void;
}

export function useKeyboardShortcuts(handlers: ShortcutHandlers = {}) {
  const navigate = useNavigate();
  const location = useLocation();
  const handlersRef = useRef(handlers);
  handlersRef.current = handlers;

  const closeAllModals = useCallback(() => {
    useRegistryStore.getState().closeEditor();
    useRegistryStore.getState().closeDeployWizard();
    useAgentStore.getState().closeEditor();
  }, []);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      const isInputFocused = target.tagName === "INPUT" || 
                             target.tagName === "TEXTAREA" || 
                             target.isContentEditable;

      if (e.key === "Escape") {
        closeAllModals();
        if (isInputFocused) {
          (target as HTMLInputElement).blur();
        }
        return;
      }

      if (isInputFocused) {
        const key = e.key.toLowerCase();
        const allowedWhileFocused = ["k", "d", ",", "r", "/"];
        if (!((e.metaKey || e.ctrlKey) && allowedWhileFocused.includes(key))) {
          return;
        }
      }

      if (e.metaKey || e.ctrlKey) {
        switch (e.key.toLowerCase()) {
          case "k":
            e.preventDefault();
            handlersRef.current.onFocusSearch?.();
            break;

          case "d":
            e.preventDefault();
            useRegistryStore.getState().openDeployWizard();
            if (location.pathname !== "/" && location.pathname !== "/registry") {
              navigate("/");
            }
            break;

          case "n":
            e.preventDefault();
            handlersRef.current.onOpenNewMenu?.();
            break;

          case ",":
            e.preventDefault();
            navigate("/settings");
            break;

          case "r":
            e.preventDefault();
            syncRegistryNow({
              repo_url: "https://github.com/abhiunix/community-registry",
              branch: "main",
              polling_interval_minutes: 60,
              auto_update: true,
              github_pat: null,
            }).catch(console.error);
            break;

          case "/":
            e.preventDefault();
            handlersRef.current.onToggleShortcutsHelp?.();
            break;
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [navigate, location.pathname, closeAllModals]);
}

export const SHORTCUTS = [
  { keys: [modKey, "K"], description: "Focus search" },
  { keys: [modKey, "D"], description: "Open deploy wizard" },
  { keys: [modKey, "N"], description: "New capability/agent" },
  { keys: [modKey, "⇧", "N"], description: "New note (Private Notes)" },
  { keys: [modKey, "⇧", "F"], description: "New folder (Private Notes)" },
  { keys: [modKey, ","], description: "Open settings" },
  { keys: [modKey, "R"], description: "Sync registry" },
  { keys: ["Esc"], description: "Close panel/modal" },
  { keys: [modKey, "/"], description: "Show shortcuts help" },
];
