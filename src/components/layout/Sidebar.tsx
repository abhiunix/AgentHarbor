import { useState, useEffect } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { useRegistryStore, useCapabilityCounts, useNewItemsCount } from "../../stores/registryStore";
import { useAgentCounts } from "../../stores/agentStore";
import { usePresetStore } from "../../stores/presetStore";
import { getEnabledAdapterPlugins, type AdapterPlugin } from "../../lib/adapterPlugins";
import type { CapabilityType } from "../../lib/types";
import logoIcon from "../../assets/icon.png";
import mcpIcon from "../../assets/mcp_logo.png";

interface NavItemProps {
  icon: string;
  iconImg?: string;
  label: string;
  count?: number;
  newCount?: number;
  active?: boolean;
  onClick: () => void;
}

function NavItem({ icon, iconImg, label, count, newCount, active, onClick, testId }: NavItemProps & { testId?: string }) {
  return (
    <button
      onClick={onClick}
      className={`sidebar-item w-full text-left ${active ? "active" : ""}`}
      {...(testId ? { "data-testid": testId } : {})}
    >
      {iconImg ? (
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

function CollapsibleAdapterSection({
  plugin,
  currentPath,
  onNavigate,
}: {
  plugin: AdapterPlugin;
  currentPath: string;
  onNavigate: (route: string) => void;
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

        {/* ── Projects ────────────────────────────────── */}
        <SectionHeader title="Projects" />
        <nav className="px-2 space-y-0.5">
          <NavItem
            icon="◫"
            label="All Projects"
            active={location.pathname === "/projects"}
            onClick={() => navigate("/projects")}
          />
        </nav>

        {/* ── Private Notes ───────────────────────────── */}
        <SectionHeader title="Private Notes" />
        <nav className="px-2 space-y-0.5">
          <NavItem
            icon="📝"
            label="Private Notes"
            active={location.pathname === "/notes"}
            onClick={() => navigate("/notes")}
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
          Version {__APP_VERSION__}
        </div>
      </div>
    </aside>
  );
}
