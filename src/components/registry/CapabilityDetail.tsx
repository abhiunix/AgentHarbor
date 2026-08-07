import { useMemo } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { UniversalCapability, CapabilityType } from "../../lib/types";
import { getCapabilityTypeLabel } from "../../lib/types";
import { useRegistryStore } from "../../stores/registryStore";

interface CapabilityDetailProps {
  capability: UniversalCapability;
  onClose: () => void;
  onDeploy: (capability: UniversalCapability) => void;
  onEdit?: (capability: UniversalCapability) => void;
  onFork?: (capability: UniversalCapability) => void;
  onCopyJson: (capability: UniversalCapability) => void;
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

export function CapabilityDetail({
  capability,
  onClose,
  onDeploy,
  onEdit,
  onFork,
  onCopyJson,
}: CapabilityDetailProps) {
  const isPrivate = capability.visibility === "private";
  const isDiscovered = capability.visibility === "discovered";
  const isPublic = capability.visibility === "public";
  const canFork = (isPublic || isDiscovered) && onFork;

  const allCapabilities = useRegistryStore((s) => s.capabilities);

  const similarCapabilities = useMemo(() => {
    const currentTags = new Set(capability.tags);
    return allCapabilities
      .filter((c) => c.id !== capability.id)
      .map((c) => {
        let score = 0;
        if (capability.category && c.category === capability.category) score += 2;
        for (const tag of c.tags) {
          if (currentTags.has(tag)) score += 1;
        }
        return { capability: c, score };
      })
      .filter((item) => item.score > 0)
      .sort((a, b) => b.score - a.score)
      .slice(0, 4)
      .map((item) => item.capability);
  }, [allCapabilities, capability.id, capability.tags, capability.category]);

  const renderTypeSpecificContent = () => {
    switch (capability.type) {
      case "mcp":
        return (
          <div className="space-y-4">
            <DetailSection title="Command">
              <code className="block p-3 bg-app-bg rounded-md font-mono text-sm text-text-primary overflow-x-auto">
                {capability.command}
              </code>
            </DetailSection>

            {capability.args && capability.args.length > 0 && (
              <DetailSection title="Arguments">
                <ul className="space-y-1">
                  {capability.args.map((arg, i) => (
                    <li key={i} className="font-mono text-sm text-text-secondary">
                      {arg}
                    </li>
                  ))}
                </ul>
              </DetailSection>
            )}

            {capability.env && Object.keys(capability.env).length > 0 && (
              <DetailSection title="Environment Variables">
                <div className="space-y-2">
                  {Object.entries(capability.env).map(([name, env]) => (
                    <div
                      key={name}
                      className="p-2 bg-app-bg rounded-md"
                    >
                      <p className="font-mono text-sm text-accent-cyan">
                        {name}
                      </p>
                      <p className="text-xs text-text-muted">{env.label}</p>
                      {env.required && (
                        <span className="text-[10px] text-accent-red">Required</span>
                      )}
                    </div>
                  ))}
                </div>
              </DetailSection>
            )}
          </div>
        );

      case "rule":
        return (
          <DetailSection title="Rule Content">
            <pre className="p-3 bg-app-bg rounded-md font-mono text-sm text-text-secondary overflow-x-auto whitespace-pre-wrap">
              {capability.content}
            </pre>
          </DetailSection>
        );

      case "skill":
        return (
          <div className="space-y-4">
            {capability.scope && (
              <DetailSection title="Scope">
                <code className="block p-3 bg-app-bg rounded-md font-mono text-sm text-text-primary">
                  {capability.scope}
                </code>
              </DetailSection>
            )}

            {capability.files && capability.files.length > 0 && (
              <DetailSection title="Files">
                <div className="space-y-2">
                  {capability.files.map((file) => (
                    <div key={file.path} className="p-2 bg-app-bg rounded-md">
                      <p className="font-mono text-sm text-accent-purple">
                        {file.path}
                      </p>
                      <pre className="text-xs text-text-muted mt-1 whitespace-pre-wrap">
                        {file.content.slice(0, 200)}
                        {file.content.length > 200 ? "..." : ""}
                      </pre>
                    </div>
                  ))}
                </div>
              </DetailSection>
            )}
          </div>
        );

      case "hook":
        // Show per-adapter JSON if adapter_configs is present
        if (capability.adapter_configs && Object.keys(capability.adapter_configs).length > 0) {
          return (
            <div className="space-y-4">
              {Object.entries(capability.adapter_configs).map(([adapterId, config]) => {
                const cfg = config as Record<string, unknown>;
                const deployPath = cfg.deploy_path as string | undefined;
                // Show config without deploy_path in the JSON preview
                const { deploy_path: _, ...displayConfig } = cfg;
                return (
                  <DetailSection key={adapterId} title={`${adapterId} Config`}>
                    {deployPath && (
                      <p className="text-xs text-text-muted font-mono mb-2">
                        Deploy to: {deployPath}
                      </p>
                    )}
                    <pre className="p-3 bg-app-bg rounded-md font-mono text-xs text-text-secondary overflow-x-auto whitespace-pre-wrap">
                      {JSON.stringify(displayConfig, null, 2)}
                    </pre>
                  </DetailSection>
                );
              })}
            </div>
          );
        }
        // Legacy display for hooks without adapter_configs
        return (
          <div className="space-y-4">
            <DetailSection title="Event">
              <span className="px-2 py-1 bg-accent-orange/10 text-accent-orange rounded-md font-mono text-sm">
                {capability.event}
              </span>
            </DetailSection>

            {capability.matcher && (
              <DetailSection title="Matcher Pattern">
                <code className="block p-3 bg-app-bg rounded-md font-mono text-sm text-text-primary">
                  {capability.matcher}
                </code>
              </DetailSection>
            )}

            <DetailSection title="Command">
              <code className="block p-3 bg-app-bg rounded-md font-mono text-sm text-text-primary">
                {capability.command}
              </code>
            </DetailSection>

            {capability.timeout_ms && (
              <DetailSection title="Timeout">
                <span className="text-sm text-text-primary">{capability.timeout_ms}ms</span>
              </DetailSection>
            )}
          </div>
        );

      case "plugin":
        return (
          <div className="space-y-4">
            <DetailSection title="Install Command">
              <code className="block p-3 bg-app-bg rounded-md font-mono text-sm text-text-primary overflow-x-auto">
                {capability.install_command}
              </code>
            </DetailSection>

            {capability.config && Object.keys(capability.config).length > 0 && (
              <DetailSection title="Configuration">
                <pre className="p-3 bg-app-bg rounded-md font-mono text-xs text-text-secondary overflow-x-auto">
                  {JSON.stringify(capability.config, null, 2)}
                </pre>
              </DetailSection>
            )}
          </div>
        );

      case "custom":
        return (
          <div className="space-y-4">
            {Object.entries(capability.adapter_configs).map(([adapterId, config]) => {
              const cfg = config as Record<string, unknown>;
              const deployPath = cfg.deploy_path as string | undefined;
              const content = cfg.content as string | undefined;
              return (
                <DetailSection key={adapterId} title={`${adapterId} Config`}>
                  {deployPath && (
                    <p className="text-xs text-text-muted font-mono mb-2">
                      Deploy to: {deployPath}
                    </p>
                  )}
                  {content && (
                    <pre className="p-3 bg-app-bg rounded-md font-mono text-xs text-text-secondary overflow-x-auto whitespace-pre-wrap">
                      {content}
                    </pre>
                  )}
                </DetailSection>
              );
            })}
          </div>
        );

      default:
        return null;
    }
  };

  return (
    <div
      className="fixed inset-y-0 right-0 w-[480px] bg-app-sidebar border-l border-border shadow-2xl z-50 flex flex-col"
      style={{ borderTopColor: typeColors[capability.type], borderTopWidth: "3px" }}
    >
      <div className="flex items-center justify-between p-4 border-b border-border">
        <div className="flex items-center gap-2">
          <span className={`text-xs font-semibold uppercase px-2 py-1 rounded ${typeBgColors[capability.type]}`}>
            {getCapabilityTypeLabel(capability.type)}
          </span>
          <span
            className={`text-xs px-2 py-1 rounded ${
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
            <span className="text-xs text-text-muted truncate max-w-[120px]" title={capability.source}>
              {capability.source}
            </span>
          )}
        </div>
        <button
          onClick={onClose}
          className="w-8 h-8 flex items-center justify-center rounded-md hover:bg-app-card-hover text-text-muted hover:text-text-primary transition-colors"
        >
          ✕
        </button>
      </div>

      <div className="flex-1 overflow-y-auto p-6 space-y-6">
        <div>
          <h2 className="text-xl font-semibold text-text-primary mb-1">
            {capability.name}
          </h2>
          <p className="font-mono text-sm text-text-muted">{capability.id}</p>
        </div>

        <p className="text-text-secondary">{capability.description}</p>

        <div className="grid grid-cols-2 gap-4">
          <DetailField label="Version" value={capability.version} />
          <DetailField label="Author" value={capability.author_github ? `@${capability.author_github}` : capability.author} />
          {capability.category && <DetailField label="Category" value={capability.category} />}
          {capability.license && <DetailField label="License" value={capability.license} />}
        </div>

        {(capability.stats?.github_stars || capability.source_info?.repo) && (
          <div className="flex items-center gap-3 p-3 bg-app-bg rounded-lg">
            {capability.stats?.github_stars != null && capability.stats.github_stars > 0 && (
              <span className="text-sm text-yellow-400 font-medium">
                ★ {capability.stats.github_stars.toLocaleString()}
              </span>
            )}
            {capability.source_info?.repo && (
              <span className="text-xs text-text-muted font-mono truncate flex-1">
                {capability.source_info.repo}
              </span>
            )}
            {capability.stats?.updated_at && (
              <span className="text-xs text-text-muted">
                Updated {capability.stats.updated_at}
              </span>
            )}
            {capability.source_info?.url && (
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  openUrl(capability.source_info!.url!);
                }}
                className="text-xs text-accent-blue hover:underline flex-shrink-0 cursor-pointer"
              >
                View Source ↗
              </button>
            )}
          </div>
        )}

        {capability.tags.length > 0 && (
          <div>
            <p className="text-xs text-text-muted uppercase mb-2">Tags</p>
            <div className="flex flex-wrap gap-2">
              {capability.tags.map((tag) => (
                <span
                  key={tag}
                  className="px-2 py-1 bg-app-bg text-text-secondary text-sm rounded-md"
                >
                  {tag}
                </span>
              ))}
            </div>
          </div>
        )}

        <div>
          <p className="text-xs text-text-muted uppercase mb-2">Adapter Compatibility</p>
          <div className="flex gap-2">
            {capability.compatible_agents.map((adapter) => (
              <span
                key={adapter}
                className="px-2 py-1 bg-accent-green/10 text-accent-green text-sm rounded-md"
              >
                {adapter}
              </span>
            ))}
          </div>
        </div>

        {renderTypeSpecificContent()}

        {similarCapabilities.length > 0 && (
          <DetailSection title="Similar">
            <div className="grid grid-cols-2 gap-2">
              {similarCapabilities.map((sim) => (
                <button
                  key={sim.id}
                  onClick={() => onDeploy(sim)}
                  className="text-left p-3 bg-app-bg rounded-lg hover:bg-app-card-hover transition-colors"
                >
                  <p className="text-sm font-medium text-text-primary truncate">{sim.name}</p>
                  <p className="text-xs text-text-secondary line-clamp-2 mt-1">{sim.description}</p>
                  {sim.stats?.github_stars != null && sim.stats.github_stars > 0 && (
                    <span className="text-[10px] text-yellow-400 mt-1.5 inline-block">
                      ★ {sim.stats.github_stars.toLocaleString()}
                    </span>
                  )}
                </button>
              ))}
            </div>
          </DetailSection>
        )}

        <DetailSection title="JSON Config">
          <pre className="p-4 bg-app-bg rounded-md font-mono text-xs text-text-secondary overflow-x-auto max-h-64">
            {JSON.stringify(capability, null, 2)}
          </pre>
        </DetailSection>
      </div>

      <div className="p-4 border-t border-border flex items-center gap-3">
        <button
          onClick={() => onDeploy(capability)}
          className="flex-1 h-10 rounded-md bg-accent-blue text-white text-sm font-medium hover:bg-accent-blue/90 transition-colors"
        >
          Deploy to Project
        </button>
        {canFork && (
          <button
            onClick={() => onFork(capability)}
            className="h-10 px-4 rounded-md bg-app-card border border-border text-sm text-text-primary hover:bg-app-card-hover transition-colors"
          >
            {isDiscovered ? "Import" : "Fork to Private"}
          </button>
        )}
        {isPrivate && onEdit && (
          <button
            onClick={() => onEdit(capability)}
            className="h-10 px-4 rounded-md bg-app-card border border-border text-sm text-text-primary hover:bg-app-card-hover transition-colors"
          >
            Edit
          </button>
        )}
        <button
          onClick={() => onCopyJson(capability)}
          className="h-10 px-4 rounded-md bg-app-card border border-border text-sm text-text-primary hover:bg-app-card-hover transition-colors"
        >
          Copy JSON
        </button>
      </div>
    </div>
  );
}

function DetailSection({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <p className="text-xs text-text-muted uppercase mb-2">{title}</p>
      {children}
    </div>
  );
}

function DetailField({ label, value }: { label: string; value: string }) {
  return (
    <div className="p-3 bg-app-bg rounded-md">
      <p className="text-xs text-text-muted uppercase mb-1">{label}</p>
      <p className="text-sm text-text-primary">{value}</p>
    </div>
  );
}
