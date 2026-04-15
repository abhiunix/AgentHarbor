import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { usePresetStore } from "../../stores/presetStore";
import type { Preset } from "../../lib/types";
import logoIcon from "../../assets/icon.png";

type PresetVisibilityFilter = "all" | "public" | "private";

const FILTER_OPTIONS: { value: PresetVisibilityFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "public", label: "Public" },
  { value: "private", label: "Private" },
];

function filterPresets(presets: Preset[], filter: PresetVisibilityFilter): Preset[] {
  if (filter === "all") return presets;
  if (filter === "public") return presets.filter((p) => p.is_bundled);
  return presets.filter((p) => !p.is_bundled);
}

interface PresetCardProps {
  preset: Preset;
  onClick: () => void;
}

function PresetCard({ preset, onClick }: PresetCardProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="group relative w-full text-left bg-app-card border border-border rounded-lg overflow-hidden hover:bg-app-card-hover transition-all"
    >
      <div className="p-4">
        <div className="flex items-start justify-between gap-2 mb-2">
          <div className="min-w-0">
            <p className="font-medium text-text-primary truncate">{preset.name}</p>
            <p className="text-xs font-mono text-text-muted truncate">{preset.id}</p>
          </div>
          <span
            className={`shrink-0 text-[10px] font-medium px-1.5 py-0.5 rounded ${
              preset.is_bundled ? "bg-accent-green/10 text-accent-green" : "bg-accent-orange/10 text-accent-orange"
            }`}
          >
            {preset.is_bundled ? "public" : "private"}
          </span>
        </div>
        {preset.description && (
          <p className="text-sm text-text-secondary line-clamp-2 mb-2">{preset.description}</p>
        )}
        <p className="text-xs text-text-muted">
          {preset.capability_ids.length} {preset.capability_ids.length === 1 ? "capability" : "capabilities"}
        </p>
      </div>
    </button>
  );
}

export function PresetList() {
  const navigate = useNavigate();
  const { presets, loading, error } = usePresetStore();
  const [visibilityFilter, setVisibilityFilter] = useState<PresetVisibilityFilter>("all");

  const filteredPresets = filterPresets(presets, visibilityFilter);

  if (loading) {
    return (
      <div className="p-6">
        <div className="mb-4 p-4 bg-accent-blue/10 border border-accent-blue/20 rounded-lg animate-pulse h-16" />
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {[...Array(6)].map((_, i) => (
            <div
              key={i}
              className="bg-app-card border border-border rounded-lg h-40 animate-pulse"
            />
          ))}
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-6 text-center">
        <p className="text-accent-red mb-2">Failed to load presets</p>
        <p className="text-sm text-text-secondary">{error}</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="px-6 py-4">
        <div className="p-4 bg-accent-blue/10 border border-accent-blue/20 rounded-lg mb-4">
          <p className="text-sm text-text-primary">
            <span className="font-medium">Presets</span> are named groups of capabilities for one-click deploy.
          </p>
        </div>

        <div className="flex items-center justify-between gap-4 flex-wrap">
          <div className="flex items-center gap-1 p-0.5 bg-app-bg rounded-lg border border-border">
            {FILTER_OPTIONS.map((opt) => (
              <button
                key={opt.value}
                type="button"
                onClick={() => setVisibilityFilter(opt.value)}
                className={`px-3 py-1.5 text-xs font-medium rounded-md transition-colors ${
                  visibilityFilter === opt.value
                    ? "bg-app-card-hover text-text-primary"
                    : "text-text-muted hover:text-text-secondary"
                }`}
              >
                {opt.label}
              </button>
            ))}
          </div>
          <p className="text-sm text-text-muted">
            Showing {filteredPresets.length} of {presets.length} presets
          </p>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-6 pb-6">
        {filteredPresets.length === 0 ? (
          <div className="text-center py-12">
            <div className="w-16 h-16 mx-auto mb-4 opacity-30">
              <img src={logoIcon} alt="AgentHarbor" className="w-full h-full" />
            </div>
            <p className="text-text-muted text-lg mb-2">No presets found</p>
            <p className="text-text-secondary text-sm">
              {visibilityFilter === "all" ? "Create a preset from the Registry or adjust the filter." : "No presets match this filter."}
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {filteredPresets.map((preset) => (
              <PresetCard
                key={preset.id}
                preset={preset}
                onClick={() => navigate(`/presets/${preset.id}`)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
