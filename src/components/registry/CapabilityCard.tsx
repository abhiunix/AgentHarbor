import { useState, useRef, useEffect } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { UniversalCapability, CapabilityType, AdapterType } from "../../lib/types";
import { getCapabilityTypeLabel } from "../../lib/types";
import { getAdapterIconImg } from "../../lib/adapterPlugins";

interface CapabilityCardProps {
  capability: UniversalCapability;
  selected: boolean;
  onSelect: (id: string) => void;
  onDoubleClick: (capability: UniversalCapability) => void;
  onEdit?: (capability: UniversalCapability) => void;
  onDelete?: (id: string) => void;
  onFork?: (capability: UniversalCapability) => void;
  isNew?: boolean;
}

const typeColors: Record<CapabilityType, string> = {
  mcp: "#5b8af5",
  rule: "#34d399",
  skill: "#a78bfa",
  hook: "#fb923c",
  plugin: "#fbbf24",
  custom: "#2dd4bf",
};

const typeBgColors: Record<CapabilityType, string> = {
  mcp: "bg-accent-blue/10 text-accent-blue",
  rule: "bg-accent-green/10 text-accent-green",
  skill: "bg-accent-purple/10 text-accent-purple",
  hook: "bg-accent-orange/10 text-accent-orange",
  plugin: "bg-accent-yellow/10 text-accent-yellow",
  custom: "bg-teal-400/10 text-teal-400",
};

const adapters: AdapterType[] = ["claude-code", "cursor", "windsurf"];

const adapterLabels: Record<AdapterType, string> = {
  "claude-code": "CC",
  cursor: "Cu",
  windsurf: "Wi",
};

function ExpandableDescription({ text }: { text: string }) {
  const [expanded, setExpanded] = useState(false);
  const ref = useRef<HTMLParagraphElement>(null);
  const [clamped, setClamped] = useState(false);

  useEffect(() => {
    const el = ref.current;
    if (el) {
      setClamped(el.scrollHeight > el.clientHeight + 1);
    }
  }, [text]);

  return (
    <div className="mb-3">
      <p
        ref={ref}
        className={`text-xs text-text-secondary ${expanded ? "" : "line-clamp-2"}`}
      >
        {text}
      </p>
      {clamped && (
        <button
          onClick={(e) => {
            e.stopPropagation();
            setExpanded(!expanded);
          }}
          className="text-[10px] text-accent-blue hover:underline mt-0.5"
        >
          {expanded ? "Show less" : "Show more"}
        </button>
      )}
    </div>
  );
}

export function CapabilityCard({
  capability,
  selected,
  onSelect,
  onDoubleClick,
  onEdit,
  onDelete,
  onFork,
  isNew = false,
}: CapabilityCardProps) {
  const isPrivate = capability.visibility === "private";
  const isDiscovered = capability.visibility === "discovered";
  const isPublic = capability.visibility === "public";
  const canFork = (isPublic || isDiscovered) && onFork;

  const handleClick = (e: React.MouseEvent) => {
    e.preventDefault();
    onSelect(capability.id);
  };

  const handleDoubleClick = (e: React.MouseEvent) => {
    e.preventDefault();
    onDoubleClick(capability);
  };

  return (
    <div
      onClick={handleClick}
      onDoubleClick={handleDoubleClick}
      className={`group relative bg-app-card border rounded-lg overflow-hidden cursor-pointer transition-all ${
        selected
          ? "border-accent-blue ring-1 ring-accent-blue/50"
          : "border-border hover:bg-app-card-hover"
      }`}
      style={{ borderTopColor: typeColors[capability.type], borderTopWidth: "2px" }}
    >
      <div className="absolute top-3 right-3 z-10">
        <input
          type="checkbox"
          checked={selected}
          onChange={() => onSelect(capability.id)}
          onClick={(e) => e.stopPropagation()}
          data-testid={`cap-checkbox-${capability.id}`}
          className="w-4 h-4 rounded border-border bg-app-input accent-accent-blue cursor-pointer"
        />
      </div>

      {(isPrivate || canFork) && (
        <div className="absolute top-3 right-10 z-10 opacity-0 group-hover:opacity-100 transition-opacity flex gap-1">
          {canFork && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onFork(capability);
              }}
              className="w-6 h-6 flex items-center justify-center rounded bg-app-bg/80 hover:bg-app-card-hover text-text-secondary hover:text-text-primary transition-colors"
              title={isDiscovered ? "Import as private" : "Fork to private"}
            >
              ⎘
            </button>
          )}
          {isPrivate && onEdit && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onEdit(capability);
              }}
              className="w-6 h-6 flex items-center justify-center rounded bg-app-bg/80 hover:bg-app-card-hover text-text-secondary hover:text-text-primary transition-colors"
              title="Edit"
            >
              ✎
            </button>
          )}
          {isPrivate && onDelete && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                onDelete(capability.id);
              }}
              className="w-6 h-6 flex items-center justify-center rounded bg-app-bg/80 hover:bg-accent-red/20 text-text-secondary hover:text-accent-red transition-colors"
              title="Delete"
            >
              ✕
            </button>
          )}
        </div>
      )}

      <div className="p-4">
        <div className="flex items-center gap-2 mb-2 pr-8">
          <span className={`text-[10px] font-semibold uppercase px-1.5 py-0.5 rounded ${typeBgColors[capability.type]}`}>
            {getCapabilityTypeLabel(capability.type)}
          </span>
          <span
            className={`text-[10px] px-1.5 py-0.5 rounded ${
              isPrivate
                ? "bg-accent-cyan/10 text-accent-cyan"
                : isDiscovered
                ? "bg-amber-500/20 text-amber-400"
                : "bg-text-muted/20 text-text-muted"
            }`}
          >
            {capability.visibility}
          </span>
          {isDiscovered && capability.source && (
            <span className="text-[9px] px-1.5 py-0.5 rounded bg-app-bg text-text-muted truncate max-w-[80px]" title={capability.source}>
              {capability.source}
            </span>
          )}
          {capability.type === "skill" && capability.managed && (
            <span
              className="text-[9px] px-1.5 py-0.5 rounded bg-accent-purple/15 text-accent-purple"
              title="Deployed by AgentHarbor"
            >
              managed
            </span>
          )}
          {isNew && (
            <span className="text-[9px] font-bold px-1.5 py-0.5 rounded bg-accent-green/20 text-accent-green animate-pulse">
              NEW
            </span>
          )}
        </div>

        <h3 className="text-sm font-medium text-text-primary mb-1 line-clamp-1">
          {capability.name}
        </h3>

        <p className="text-[11px] font-mono text-text-muted mb-2 truncate">{capability.id}</p>

        <ExpandableDescription text={capability.description} />

        <div className="flex items-center justify-between">
          <div className="flex flex-wrap items-center gap-1">
            {capability.category && (
              <span className="text-[10px] px-1.5 py-0.5 rounded bg-white/5 text-text-muted">
                {capability.category}
              </span>
            )}
            {capability.tags.slice(0, capability.category ? 2 : 3).map((tag) => (
              <span
                key={tag}
                className="text-[10px] px-1.5 py-0.5 rounded bg-app-bg text-text-muted"
              >
                {tag}
              </span>
            ))}
            {capability.tags.length > (capability.category ? 2 : 3) && (
              <span className="text-[10px] px-1.5 py-0.5 text-text-muted">
                +{capability.tags.length - (capability.category ? 2 : 3)}
              </span>
            )}
            {capability.stats?.github_stars != null && capability.stats.github_stars > 0 && (
              <span className="text-[10px] text-yellow-400 font-medium ml-1">
                ★ {capability.stats.github_stars >= 1000
                  ? `${(capability.stats.github_stars / 1000).toFixed(1)}k`
                  : capability.stats.github_stars}
              </span>
            )}
          </div>

          <div className="flex items-center gap-1">
            {capability.source_info?.url && (
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  openUrl(capability.source_info!.url!);
                }}
                className="w-5 h-5 flex items-center justify-center rounded-full bg-accent-blue/15 hover:bg-accent-blue/30 transition-colors cursor-pointer"
                title={capability.source_info.url}
              >
                <span className="text-[10px]">🔗</span>
              </button>
            )}
            {adapters.map((adapter) => {
              const supported = capability.compatible_agents.includes(adapter);
              const iconSrc = getAdapterIconImg(adapter);
              return (
                <div
                  key={adapter}
                  className={`w-5 h-5 flex items-center justify-center rounded-full ${
                    supported
                      ? "bg-accent-green/20"
                      : "bg-text-muted/10 opacity-30"
                  }`}
                  title={`${adapter}: ${supported ? "Supported" : "Not supported"}`}
                >
                  {iconSrc ? (
                    <img src={iconSrc} alt={adapterLabels[adapter]} className="w-3 h-3 object-contain" />
                  ) : (
                    <span className={`text-[8px] font-bold ${supported ? "text-accent-green" : "text-text-muted/50"}`}>
                      {adapterLabels[adapter]}
                    </span>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      </div>
    </div>
  );
}
