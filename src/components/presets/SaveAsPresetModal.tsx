import { useState } from "react";
import { usePresetStore } from "../../stores/presetStore";

interface SaveAsPresetModalProps {
  capabilityIds: string[];
  onClose: () => void;
  onSaved: () => void;
}

export function SaveAsPresetModal({
  capabilityIds,
  onClose,
  onSaved,
}: SaveAsPresetModalProps) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const { createPreset } = usePresetStore();

  const handleSave = async () => {
    if (!name.trim()) {
      setError("Name is required");
      return;
    }

    setSaving(true);
    setError(null);

    try {
      await createPreset(name.trim(), description.trim(), capabilityIds);
      onSaved();
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to save preset");
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-app-sidebar border border-border rounded-xl w-full max-w-md shadow-2xl">
        <div className="flex items-center justify-between px-6 py-4 border-b border-border">
          <h2 className="text-lg font-semibold text-text-primary">Save as Preset</h2>
          <button
            onClick={onClose}
            className="w-8 h-8 flex items-center justify-center rounded-full hover:bg-white/10 text-text-muted hover:text-text-primary transition-colors"
          >
            ✕
          </button>
        </div>

        <div className="p-6 space-y-4">
          {error && (
            <div className="p-3 bg-accent-red/10 border border-accent-red/30 rounded-lg text-accent-red text-sm">
              {error}
            </div>
          )}

          <div>
            <label className="block text-xs text-text-muted uppercase mb-2">
              Preset Name *
            </label>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="My Custom Preset"
              className="w-full px-4 py-2 bg-app-card border border-border rounded-lg text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-blue"
            />
          </div>

          <div>
            <label className="block text-xs text-text-muted uppercase mb-2">
              Description (optional)
            </label>
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              placeholder="What is this preset for?"
              rows={3}
              className="w-full px-4 py-2 bg-app-card border border-border rounded-lg text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-blue resize-none"
            />
          </div>

          <div className="p-3 bg-app-card border border-border rounded-lg">
            <p className="text-xs text-text-muted uppercase mb-1">Capabilities</p>
            <p className="text-sm text-text-primary">
              {capabilityIds.length} capabilit{capabilityIds.length === 1 ? "y" : "ies"} selected
            </p>
          </div>
        </div>

        <div className="flex items-center justify-end gap-3 px-6 py-4 border-t border-border">
          <button
            onClick={onClose}
            className="px-4 py-2 text-sm text-text-muted hover:text-text-primary transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleSave}
            disabled={saving || !name.trim()}
            className="px-6 py-2 bg-accent-blue text-white rounded-lg font-medium hover:bg-accent-blue/80 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {saving ? "Saving..." : "Save Preset"}
          </button>
        </div>
      </div>
    </div>
  );
}
