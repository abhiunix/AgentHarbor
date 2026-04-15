import { useState, useEffect } from "react";
import { useLocation, useNavigate } from "react-router-dom";
import { usePresetStore, usePresetById } from "../../stores/presetStore";
import { useRegistryStore } from "../../stores/registryStore";
import { AddToPresetModal } from "./AddToPresetModal";
import { DeployWizard } from "../deploy/DeployWizard";
import { ConfirmDialog } from "../common/ConfirmDialog";
import type { UniversalCapability } from "../../lib/types";
import type { Visibility } from "../../lib/types";

type PresetVisibilityFilter = "all" | Visibility;

const PRESET_VISIBILITY_OPTIONS: { value: PresetVisibilityFilter; label: string }[] = [
  { value: "all", label: "All" },
  { value: "public", label: "Public" },
  { value: "private", label: "Private" },
  { value: "discovered", label: "Discovered" },
];

export function PresetView() {
  const location = useLocation();
  const navigate = useNavigate();
  const id = location.pathname.replace(/^\/presets\//, "") || "";
  const preset = usePresetById(id || "");
  const { removeCapabilityFromPreset, deletePreset } = usePresetStore();
  const capabilities = useRegistryStore((s) => s.capabilities);
  const loadCapabilities = useRegistryStore((s) => s.loadCapabilities);

  const [presetVisibility, setPresetVisibility] = useState<PresetVisibilityFilter>("all");
  const [showAddModal, setShowAddModal] = useState(false);
  const [showDeploy, setShowDeploy] = useState(false);
  const [deletePresetConfirm, setDeletePresetConfirm] = useState(false);
  const [removeCapConfirm, setRemoveCapConfirm] = useState<{ capId: string; capName: string } | null>(null);

  useEffect(() => {
    if (capabilities.length === 0) loadCapabilities();
  }, [capabilities.length, loadCapabilities]);

  if (!preset) {
    return (
      <div className="p-6 text-center">
        <p className="text-text-muted">Preset not found</p>
        <button
          onClick={() => navigate("/")}
          className="mt-4 text-accent-blue hover:underline"
        >
          Go back to Registry
        </button>
      </div>
    );
  }

  const presetCapabilitiesRaw = capabilities.filter((c) =>
    preset.capability_ids.includes(c.id)
  );
  const presetCapabilities =
    presetVisibility === "all"
      ? presetCapabilitiesRaw
      : presetCapabilitiesRaw.filter((c) => c.visibility === presetVisibility);

  const handleRemoveCapabilityClick = (capId: string, capName: string) => {
    setRemoveCapConfirm({ capId, capName });
  };

  const handleRemoveCapabilityConfirm = async () => {
    if (!removeCapConfirm) return;
    await removeCapabilityFromPreset(preset.id, removeCapConfirm.capId);
    setRemoveCapConfirm(null);
  };

  const handleDeletePresetClick = () => {
    setDeletePresetConfirm(true);
  };

  const handleDeletePresetConfirm = async () => {
    await deletePreset(preset.id);
    setDeletePresetConfirm(false);
    navigate("/");
  };

  return (
    <div className="p-6">
      <div className="flex items-center justify-between mb-6">
        <div>
          <div className="flex items-center gap-3 mb-1">
            <h1 className="text-2xl font-bold text-text-primary">{preset.name}</h1>
            {preset.is_bundled && (
              <span className="text-[10px] px-2 py-0.5 rounded bg-accent-purple/20 text-accent-purple uppercase">
                Bundled
              </span>
            )}
          </div>
          <p className="text-text-muted">{preset.description}</p>
          <p className="text-xs text-text-muted mt-2">
            {preset.capability_ids.length} capabilit{preset.capability_ids.length === 1 ? "y" : "ies"}
          </p>
        </div>
        <div className="flex items-center gap-3">
          {!preset.is_bundled && (
            <>
              <button
                onClick={() => setShowAddModal(true)}
                className="px-4 py-2 text-sm border border-border rounded-lg text-text-primary hover:bg-white/5 transition-colors"
              >
                + Add Capabilities
              </button>
              <button
                onClick={handleDeletePresetClick}
                className="px-4 py-2 text-sm text-accent-red hover:bg-accent-red/10 rounded-lg transition-colors"
              >
                Delete
              </button>
            </>
          )}
          <button
            onClick={() => setShowDeploy(true)}
            className="px-6 py-2 bg-accent-blue text-white rounded-lg font-medium hover:bg-accent-blue/80 transition-colors"
          >
            Deploy Preset
          </button>
        </div>
      </div>

      {preset.tags.length > 0 && (
        <div className="flex gap-2 mb-6">
          {preset.tags.map((tag) => (
            <span
              key={tag}
              className="text-xs px-2 py-1 rounded bg-white/5 text-text-muted"
            >
              {tag}
            </span>
          ))}
        </div>
      )}

      <div className="flex items-center justify-between gap-4 mb-4">
        <div className="flex rounded-md overflow-hidden border border-border">
          {PRESET_VISIBILITY_OPTIONS.map((opt) => (
            <button
              key={opt.value}
              type="button"
              onClick={() => setPresetVisibility(opt.value)}
              className={`px-3 py-1.5 text-sm transition-colors ${
                presetVisibility === opt.value
                  ? "bg-accent-blue text-white"
                  : "bg-app-card text-text-secondary hover:text-text-primary"
              }`}
            >
              {opt.label}
            </button>
          ))}
        </div>
        <p className="text-sm text-text-muted">
          Showing {presetCapabilities.length} of {presetCapabilitiesRaw.length} capabilities
        </p>
      </div>

      <div className="space-y-3">
        {presetCapabilities.length === 0 ? (
          <div className="p-8 text-center border border-dashed border-border rounded-lg">
            <p className="text-text-muted mb-2">
              {presetCapabilitiesRaw.length === 0
                ? "No capabilities in this preset"
                : `No ${presetVisibility} capabilities in this preset`}
            </p>
            {!preset.is_bundled && presetCapabilitiesRaw.length === 0 && (
              <button
                onClick={() => setShowAddModal(true)}
                className="text-accent-blue hover:underline"
              >
                Add some capabilities
              </button>
            )}
            {presetCapabilitiesRaw.length > 0 && presetVisibility !== "all" && (
              <button
                type="button"
                onClick={() => setPresetVisibility("all")}
                className="text-accent-blue hover:underline"
              >
                Show all
              </button>
            )}
          </div>
        ) : (
          presetCapabilities.map((cap) => (
            <CapabilityRow
              key={cap.id}
              capability={cap}
              onRemove={!preset.is_bundled ? () => handleRemoveCapabilityClick(cap.id, cap.name) : undefined}
            />
          ))
        )}
      </div>

      {showAddModal && (
        <AddToPresetModal
          presetId={preset.id}
          existingCapabilityIds={preset.capability_ids}
          onClose={() => setShowAddModal(false)}
        />
      )}

      {showDeploy && (
        <DeployWizard
          isOpen={true}
          onClose={() => setShowDeploy(false)}
          initialCapabilityIds={preset.capability_ids}
        />
      )}

      <ConfirmDialog
        isOpen={deletePresetConfirm}
        title="Delete Preset"
        message={`Are you sure you want to delete the preset "${preset.name}"? This action cannot be undone.`}
        onConfirm={handleDeletePresetConfirm}
        onCancel={() => setDeletePresetConfirm(false)}
      />

      <ConfirmDialog
        isOpen={!!removeCapConfirm}
        title="Remove Capability"
        message={
          removeCapConfirm
            ? `Are you sure you want to remove "${removeCapConfirm.capName}" from this preset?`
            : ""
        }
        onConfirm={handleRemoveCapabilityConfirm}
        onCancel={() => setRemoveCapConfirm(null)}
      />
    </div>
  );
}

function CapabilityRow({
  capability,
  onRemove,
}: {
  capability: UniversalCapability;
  onRemove?: () => void;
}) {
  const typeColors: Record<string, string> = {
    mcp: "#3b82f6",
    rule: "#22c55e",
    skill: "#f59e0b",
    hook: "#ec4899",
    plugin: "#8b5cf6",
  };

  return (
    <div className="flex items-center gap-4 p-4 bg-app-card border border-border rounded-lg hover:bg-app-card-hover transition-colors">
      <div
        className="w-2 h-2 rounded-full"
        style={{ backgroundColor: typeColors[capability.type] || "#666" }}
      />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="font-medium text-text-primary">{capability.name}</span>
          <span className="text-[10px] uppercase px-2 py-0.5 rounded bg-white/5 text-text-muted">
            {capability.type}
          </span>
          <span className="text-[10px] uppercase px-2 py-0.5 rounded bg-white/5 text-text-muted">
            {capability.visibility}
          </span>
        </div>
        <p className="text-xs font-mono text-text-muted">{capability.id}</p>
      </div>
      {onRemove && (
        <button
          onClick={onRemove}
          className="px-3 py-1 text-xs text-accent-red hover:bg-accent-red/10 rounded transition-colors"
        >
          ✕ Remove
        </button>
      )}
    </div>
  );
}
