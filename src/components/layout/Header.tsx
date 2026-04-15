import { useState, useRef, useEffect, useCallback, RefObject } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useRegistryStore } from "../../stores/registryStore";
import { useAgentStore } from "../../stores/agentStore";
import { modKey } from "../../lib/platform";

const openAgentEditor = () => useAgentStore.getState().openEditor();
const openCapabilityEditor = () => useRegistryStore.getState().openEditor();
const openDeployWizard = () => useRegistryStore.getState().openDeployWizard();

function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value);

  useEffect(() => {
    const handler = setTimeout(() => {
      setDebouncedValue(value);
    }, delay);

    return () => {
      clearTimeout(handler);
    };
  }, [value, delay]);

  return debouncedValue;
}

interface HeaderProps {
  searchInputRef?: RefObject<HTMLInputElement>;
  showNewMenu?: boolean;
  setShowNewMenu?: (show: boolean) => void;
}

export function Header({ searchInputRef: externalSearchRef, showNewMenu: externalShowNewMenu, setShowNewMenu: externalSetShowNewMenu }: HeaderProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const [searchValue, setSearchValue] = useState("");
  const [internalShowNewMenu, setInternalShowNewMenu] = useState(false);
  const internalSearchRef = useRef<HTMLInputElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  const searchInputRef = externalSearchRef || internalSearchRef;
  const showNewMenu = externalShowNewMenu !== undefined ? externalShowNewMenu : internalShowNewMenu;
  const setShowNewMenu = externalSetShowNewMenu || setInternalShowNewMenu;

  const { setSearch: setRegistrySearch } = useRegistryStore();
  const { setSearch: setAgentSearch } = useAgentStore();

  const isAgentsView = location.pathname === "/agents";
  const debouncedSearch = useDebounce(searchValue, 300);

  useEffect(() => {
    if (isAgentsView) {
      setAgentSearch(debouncedSearch);
    } else {
      setRegistrySearch(debouncedSearch);
    }
  }, [debouncedSearch, isAgentsView, setAgentSearch, setRegistrySearch]);

  useEffect(() => {
    const handleClickOutside = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setShowNewMenu(false);
      }
    };

    document.addEventListener("mousedown", handleClickOutside);
    return () => document.removeEventListener("mousedown", handleClickOutside);
  }, [setShowNewMenu]);

  const handleDeploy = useCallback(() => {
    openDeployWizard();
    if (location.pathname !== "/" && location.pathname !== "/registry") {
      navigate("/");
    }
  }, [navigate, location.pathname]);

  const getTitle = () => {
    if (location.pathname === "/agents") return "Agents";
    if (location.pathname === "/projects") return "Projects";
    if (location.pathname === "/settings") return "Settings";
    if (location.pathname === "/presets" || location.pathname === "/presets/") return "Presets";
    if (location.pathname.startsWith("/presets/")) return "Preset";
    if (location.pathname === "/notes") return "Private Notes";
    return "Registry";
  };

  return (
    <header className="h-16 px-7 flex items-center gap-4 border-b border-border bg-app-bg">
      <h1 className="text-lg font-semibold text-text-primary">{getTitle()}</h1>

      <div className="flex-1 flex items-center gap-3 max-w-md">
        <div className="relative flex-1">
          <input
            ref={searchInputRef}
            type="text"
            placeholder={isAgentsView ? "Search agents..." : "Search capabilities..."}
            value={searchValue}
            onChange={(e) => setSearchValue(e.target.value)}
            data-testid="header-search"
            className="w-full h-9 pl-9 pr-12 rounded-md bg-app-input border border-border text-sm text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-blue transition-colors"
          />
          <svg
            className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-text-muted"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z"
            />
          </svg>
          <kbd className="absolute right-3 top-1/2 -translate-y-1/2 px-1.5 py-0.5 text-[10px] font-mono text-text-muted bg-app-bg border border-border rounded">
            {modKey}K
          </kbd>
        </div>
      </div>

      <div className="flex items-center gap-2">
        <div className="relative" ref={menuRef}>
          <button
            onClick={() => setShowNewMenu(!showNewMenu)}
            data-testid="header-new"
            className="h-9 px-3 flex items-center gap-1.5 rounded-md bg-app-card border border-border text-sm text-text-primary hover:bg-app-card-hover transition-colors"
          >
            <span className="text-lg">+</span>
            <span>New</span>
            <svg
              className={`w-4 h-4 transition-transform ${showNewMenu ? "rotate-180" : ""}`}
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M19 9l-7 7-7-7"
              />
            </svg>
          </button>

          {showNewMenu && (
            <div className="absolute right-0 mt-2 w-48 py-1 rounded-lg bg-app-modal border border-border shadow-modal z-50">
              <button
                onClick={() => {
                  setShowNewMenu(false);
                  openCapabilityEditor();
                  if (location.pathname !== "/" && location.pathname !== "/registry") {
                    navigate("/");
                  }
                }}
                className="w-full px-4 py-2 text-left text-sm text-text-primary hover:bg-white/5 transition-colors"
              >
                New Capability
              </button>
              <button
                onClick={() => {
                  setShowNewMenu(false);
                  openAgentEditor();
                  if (location.pathname !== "/agents") {
                    navigate("/agents");
                  }
                }}
                className="w-full px-4 py-2 text-left text-sm text-text-primary hover:bg-white/5 transition-colors"
              >
                New Agent
              </button>
            </div>
          )}
        </div>

        <button
          onClick={handleDeploy}
          data-testid="header-deploy"
          className="h-9 px-4 flex items-center gap-2 rounded-md bg-accent-blue text-white text-sm font-medium hover:bg-accent-blue/90 transition-colors"
        >
          <span>▸</span>
          <span>Deploy</span>
        </button>
      </div>
    </header>
  );
}
