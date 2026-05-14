import { useState, useEffect, type ReactNode } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { useLocation, useNavigate } from "react-router-dom";
import { useRegistryStore, useCapabilityCounts, useNewItemsCount } from "../../stores/registryStore";
import { useAgentCounts } from "../../stores/agentStore";
import { usePresetStore } from "../../stores/presetStore";
import { getEnabledAdapterPlugins, type AdapterPlugin } from "../../lib/adapterPlugins";
import type { CapabilityType } from "../../lib/types";
import logoIcon from "../../assets/icon.png";
import mcpIcon from "../../assets/mcp_logo.png";
import { ClaudeCodeSwitchModal } from "../common/ClaudeCodeSwitchModal";

// Half-white (Ollama) + half-orange (Claude) brain icon.
function SplitBrainIcon({ className = "w-4 h-4" }: { className?: string }) {
  const brainPath =
    "M9 3a3 3 0 0 0-3 3 2.5 2.5 0 0 0-2 4 2.5 2.5 0 0 0 0 4 2.5 2.5 0 0 0 2 4 3 3 0 0 0 3 3 3 3 0 0 0 3-3V6a3 3 0 0 0-3-3Zm6 0a3 3 0 0 1 3 3 2.5 2.5 0 0 1 2 4 2.5 2.5 0 0 1 0 4 2.5 2.5 0 0 1-2 4 3 3 0 0 1-3 3 3 3 0 0 1-3-3V6a3 3 0 0 1 3-3Z";
  return (
    <svg viewBox="0 0 24 24" className={className} xmlns="http://www.w3.org/2000/svg">
      <defs>
        <clipPath id="splitBrainClip">
          <path d={brainPath} />
        </clipPath>
      </defs>
      <g clipPath="url(#splitBrainClip)">
        <rect x="0" y="0" width="12" height="24" fill="#f5f5f5" />
        <rect x="12" y="0" width="12" height="24" fill="#DA7756" />
      </g>
      <path
        d={brainPath}
        fill="none"
        stroke="#0e0f13"
        strokeWidth="1"
        strokeLinejoin="round"
      />
      <line x1="12" y1="3.5" x2="12" y2="20.5" stroke="#0e0f13" strokeWidth="0.75" />
    </svg>
  );
}

interface NavItemProps {
  icon: string;
  iconImg?: string;
  iconNode?: ReactNode;
  label: string;
  count?: number;
  newCount?: number;
  active?: boolean;
  onClick: () => void;
}

function NavItem({ icon, iconImg, iconNode, label, count, newCount, active, onClick, testId }: NavItemProps & { testId?: string }) {
  return (
    <button
      onClick={onClick}
      className={`sidebar-item w-full text-left ${active ? "active" : ""}`}
      {...(testId ? { "data-testid": testId } : {})}
    >
      {iconNode ? (
        iconNode
      ) : iconImg ? (
        <img src={iconImg} alt="" className="w-4 h-4 object-contain" />
      ) : (
        <span className="text-base">{icon}</span>
      )}
      <span className="flex-1 flex items-center gap-1.5">
        {label}
        {newCount !== undefined && newCount > 0 && (
          <span className="text-[9px] font-bold px-1 py-0.5 rounded bg-accent-green/20 text-accent-green">
            {newCount} NEW
          </span>
        )}
      </span>
      {count !== undefined && (
        <span className="text-xs text-text-muted font-mono">{count}</span>
      )}
    </button>
  );
}

function SectionHeader({ title }: { title: string }) {
  return (
    <div className="px-3 pt-4 pb-1 text-[10px] font-semibold uppercase tracking-wider text-text-muted">
      {title}
    </div>
  );
}

// ── Collapsible adapter section ──────────────────────────────────────────────

const STORAGE_KEY_PREFIX = "sidebar-collapsed-";

function useCollapsedState(adapterId: string, defaultExpanded: boolean): [boolean, () => void] {
  const key = STORAGE_KEY_PREFIX + adapterId;
  const [expanded, setExpanded] = useState(() => {
    try {
      const stored = localStorage.getItem(key);
      if (stored !== null) return stored === "1";
    } catch { /* noop */ }
    return defaultExpanded;
  });

  useEffect(() => {
    try {
      localStorage.setItem(key, expanded ? "1" : "0");
    } catch { /* noop */ }
  }, [key, expanded]);

  return [expanded, () => setExpanded((prev) => !prev)];
}

interface ExtraAdapterItem {
  id: string;
  label: string;
  iconNode?: ReactNode;
  icon?: string;
  onClick: () => void;
}

function CollapsibleAdapterSection({
  plugin,
  currentPath,
  onNavigate,
  extraItems,
}: {
  plugin: AdapterPlugin;
  currentPath: string;
  onNavigate: (route: string) => void;
  extraItems?: ExtraAdapterItem[];
}) {
  const [expanded, toggle] = useCollapsedState(plugin.id, plugin.defaultExpanded);

  const hasActiveChild = plugin.features.some((f) => currentPath === f.route);

  return (
    <div>
      <button
        onClick={toggle}
        className={`w-full flex items-center gap-2 px-3 py-1.5 text-sm rounded-lg transition-colors hover:bg-app-card-hover ${
          hasActiveChild && !expanded ? "text-accent-blue" : "text-text-primary"
        }`}
      >
        {/* Chevron */}
        <svg
          className={`w-3 h-3 text-text-muted transition-transform duration-200 ${expanded ? "rotate-90" : ""}`}
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          strokeWidth={2}
        >
          <path strokeLinecap="round" strokeLinejoin="round" d="M9 5l7 7-7 7" />
        </svg>

        {/* Adapter icon */}
        {plugin.iconImg ? (
          <img src={plugin.iconImg} alt="" className="w-4 h-4 object-contain shrink-0" />
        ) : (
          <span
            className="w-2 h-2 rounded-full shrink-0"
            style={{ backgroundColor: plugin.color }}
          />
        )}

        <span className="flex-1 text-left font-medium">{plugin.name}</span>

        {/* Feature count badge */}
        <span className="text-[10px] text-text-muted font-mono">
          {plugin.features.length}
        </span>
      </button>

      {/* Animated children */}
      <div
        className={`overflow-hidden transition-all duration-200 ${
          expanded ? "max-h-[500px] opacity-100" : "max-h-0 opacity-0"
        }`}
      >
        <nav className="pl-4 pr-2 pb-1 space-y-0.5">
          {plugin.features.map((feature) => (
            <NavItem
              key={feature.id}
              icon={feature.icon}
              label={feature.label}
              active={currentPath === feature.route}
              onClick={() => onNavigate(feature.route)}
            />
          ))}
          {extraItems?.map((item) => (
            <NavItem
              key={item.id}
              icon={item.icon ?? ""}
              iconNode={item.iconNode}
              label={item.label}
              onClick={item.onClick}
            />
          ))}
        </nav>
      </div>
    </div>
  );
}

// ── Main sidebar ─────────────────────────────────────────────────────────────

export function Sidebar() {
  const location = useLocation();
  const navigate = useNavigate();
  const { filters, setTypeFilter } = useRegistryStore();
  const capabilityCounts = useCapabilityCounts();
  const agentCounts = useAgentCounts();
  const { presets } = usePresetStore();
  const newItemsCount = useNewItemsCount();
  const [appVersion, setAppVersion] = useState<string | null>(null);
  const [showCcSwitch, setShowCcSwitch] = useState(false);

  useEffect(() => {
    getVersion().then(setAppVersion).catch(() => {});
  }, []);

  const isRegistryView = location.pathname === "/" || location.pathname === "/registry";
  const isAgentsView = location.pathname === "/agents";

  const handleTypeClick = (type: CapabilityType | "all") => {
    navigate("/");
    setTypeFilter(type);
  };

  const typeItems: { icon: string; iconImg?: string; label: string; type: CapabilityType | "all" }[] = [
    { icon: "◈", label: "All", type: "all" },
    { icon: "", iconImg: mcpIcon, label: "MCPs", type: "mcp" },
    { icon: "✦", label: "Skills", type: "skill" },
    { icon: "⚡", label: "Hooks", type: "hook" },
    { icon: "☰", label: "Rules", type: "rule" },
  ];

  return (
    <aside className="w-sidebar min-w-sidebar bg-app-sidebar border-r border-border flex flex-col h-full">
      <div className="p-4 border-b border-border">
        <div className="flex items-center gap-2">
          <div className="w-7 h-7 rounded-lg flex items-center justify-center">
            <img src={logoIcon} alt="AgentHarbor" className="w-6 h-6" />
          </div>
          <span className="font-semibold text-text-primary">AgentHarbor</span>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto py-2">
        {/* ── Library ─────────────────────────────────── */}
        <div className="flex items-center justify-between px-3 pt-4 pb-1">
          <span className="text-[10px] font-semibold uppercase tracking-wider text-text-muted">
            Library
          </span>
          {newItemsCount > 0 && (
            <span className="text-[9px] font-bold px-1.5 py-0.5 rounded bg-accent-green/20 text-accent-green animate-pulse">
              {newItemsCount} NEW
            </span>
          )}
        </div>
        <nav className="px-2 space-y-0.5">
          {typeItems.map((item) => (
            <NavItem
              key={item.type}
              icon={item.icon}
              iconImg={item.iconImg}
              label={item.label}
              count={capabilityCounts[item.type]}
              active={isRegistryView && filters.type === item.type}
              onClick={() => handleTypeClick(item.type)}
              testId={item.type === "all" ? "sidebar-all" : undefined}
            />
          ))}
        </nav>

        <div className="px-2 mt-1 space-y-0.5">
          <NavItem
            icon="⊛"
            label="Agents"
            count={agentCounts.total}
            active={isAgentsView}
            onClick={() => navigate("/agents")}
          />
          <NavItem
            icon="⚙"
            label="Presets"
            count={presets.length}
            active={location.pathname === "/presets" || location.pathname.startsWith("/presets/")}
            onClick={() => navigate("/presets")}
          />
        </div>

        {/* ── Utilities (Projects + Notes + Debate) ───── */}
        <SectionHeader title="Utilities" />
        <nav className="px-2 space-y-0.5">
          <NavItem
            icon="◫"
            label="All Projects"
            active={location.pathname === "/projects"}
            onClick={() => navigate("/projects")}
          />
          <NavItem
            icon="📝"
            label="Private Notes"
            active={location.pathname === "/notes"}
            onClick={() => navigate("/notes")}
          />
          <NavItem
            icon="⚖"
            label="Debate"
            active={location.pathname === "/debate"}
            onClick={() => navigate("/debate")}
          />
        </nav>

        {/* ── Adapters (collapsible per-adapter) ──────── */}
        <SectionHeader title="Adapters" />
        <div className="px-2 space-y-0.5">
          {getEnabledAdapterPlugins().map((plugin) =>
            plugin.comingSoon ? (
              <div
                key={plugin.id}
                className="flex items-center gap-2 px-3 py-1.5 text-sm rounded-lg cursor-default"
              >
                {plugin.iconImg ? (
                  <img src={plugin.iconImg} alt="" className="w-4 h-4 object-contain shrink-0" />
                ) : (
                  <span
                    className="w-2 h-2 rounded-full shrink-0"
                    style={{ backgroundColor: plugin.color }}
                  />
                )}
                <span className="flex-1 text-left font-medium text-text-primary">{plugin.name}</span>
                <span className="text-[9px] font-bold px-1.5 py-0.5 rounded bg-accent-green/20 text-accent-green animate-pulse">
                  Coming Soon
                </span>
              </div>
            ) : (
              <CollapsibleAdapterSection
                key={plugin.id}
                plugin={plugin}
                currentPath={location.pathname}
                onNavigate={navigate}
                extraItems={
                  plugin.id === "claude-code"
                    ? [
                        {
                          id: "switch-model",
                          label: "Switch Model",
                          iconNode: <SplitBrainIcon />,
                          onClick: () => setShowCcSwitch(true),
                        },
                      ]
                    : undefined
                }
              />
            )
          )}
        </div>
      </div>

      {/* ── Footer ────────────────────────────────────── */}
      <div className="border-t border-border p-2">
        <NavItem
          icon="⚙"
          label="Settings"
          active={location.pathname === "/settings"}
          onClick={() => navigate("/settings")}
        />
        <div className="px-3 py-1.5 text-sm text-text-secondary">
          Version {appVersion ?? "…"}
        </div>
      </div>
      <ClaudeCodeSwitchModal
        open={showCcSwitch}
        onClose={() => setShowCcSwitch(false)}
      />
    </aside>
  );
}
