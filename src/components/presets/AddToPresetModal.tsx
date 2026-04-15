import { useState } from "react";
import { useFilteredCapabilities } from "../../stores/registryStore";
import { usePresetStore } from "../../stores/presetStore";
import type { CapabilityType } from "../../lib/types";

interface AddToPresetModalProps {
  presetId: string;
  existingCapabilityIds: string[];
  onClose: () => void;
}

export function AddToPresetModal({
  presetId,
  existingCapabilityIds,
  onClose,
}: AddToPresetModalProps) {
  const [search, setSearch] = useState("");
  const [typeFilter, setTypeFilter] = useState<CapabilityType | "all">("all");
  const [addedIds, setAddedIds] = useState<Set<string>>(new Set(existingCapabilityIds));

  const capabilities = useFilteredCapabilities();
  const { addCapabilityToPreset } = usePresetStore();

  const filteredCapabilities = capabilities.filter((cap) => {
    const matchesSearch =
      search === "" ||
      cap.name.toLowerCase().includes(search.toLowerCase()) ||
      cap.id.toLowerCase().includes(search.toLowerCase());
    const matchesType = typeFilter === "all" || cap.type === typeFilter;
    return matchesSearch && matchesType;
  });

  const handleAdd = async (capId: string) => {
    await addCapabilityToPreset(presetId, capId);
    setAddedIds((prev) => new Set(prev).add(capId));
  };

  const typeColors: Record<string, string> = {
    mcp: "#3b82f6",
    rule: "#22c55e",
    skill: "#f59e0b",
    hook: "#ec4899",
    plugin: "#8b5cf6",
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
      <div className="bg-app-sidebar border border-border rounded-xl w-full max-w-2xl max-h-[80vh] flex flex-col shadow-2xl">
        <div className="flex items-center justify-between px-6 py-4 border-b border-border">
          <h2 className="text-lg font-semibold text-text-primary">Add Capabilities</h2>
          <button
            onClick={onClose}
            className="w-8 h-8 flex items-center justify-center rounded-full hover:bg-white/10 text-text-muted hover:text-text-primary transition-colors"
          >
            ✕
          </button>
        </div>

        <div className="p-4 border-b border-border space-y-3">
          <input
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search capabilities..."
            className="w-full px-4 py-2 bg-app-card border border-border rounded-lg text-text-primary placeholder-text-muted focus:outline-none focus:border-accent-blue"
          />
          <div className="flex gap-2">
            {(["all", "mcp", "rule", "skill", "hook", "plugin"] as const).map((type) => (
              <button
                key={type}
                onClick={() => setTypeFilter(type)}
                className={`px-3 py-1 text-xs rounded-lg transition-colors ${
                  typeFilter === type
                    ? "bg-accent-blue text-white"
                    : "bg-white/5 text-text-muted hover:bg-white/10"
                }`}
              >
                {type === "all" ? "All" : type.toUpperCase()}
              </button>
            ))}
          </div>
        </div>

        <div className="flex-1 overflow-y-auto p-4 space-y-2">
          {filteredCapabilities.length === 0 ? (
            <p className="text-center text-text-muted py-8">No capabilities found</p>
          ) : (
            filteredCapabilities.map((cap) => {
              const isAdded = addedIds.has(cap.id);
              return (
                <div
                  key={cap.id}
                  className={`flex items-center gap-4 p-3 rounded-lg border transition-colors ${
                    isAdded
                      ? "border-accent-green/30 bg-accent-green/5 opacity-60"
                      : "border-border bg-app-card hover:bg-app-card-hover"
                  }`}
                >
                  <div
                    className="w-2 h-2 rounded-full"
                    style={{ backgroundColor: typeColors[cap.type] || "#666" }}
                  />
                  <div className="flex-1 min-w-0">
                    <p className="font-medium text-text-primary">{cap.name}</p>
                    <p className="text-xs font-mono text-text-muted">{cap.id}</p>
                  </div>
                  <span className="text-[10px] uppercase px-2 py-0.5 rounded bg-white/5 text-text-muted">
                    {cap.type}
                  </span>
                  {isAdded ? (
                    <span className="text-xs text-accent-green px-3 py-1">Added ✓</span>
                  ) : (
                    <button
                      onClick={() => handleAdd(cap.id)}
                      className="px-3 py-1 text-xs bg-accent-blue text-white rounded hover:bg-accent-blue/80 transition-colors"
                    >
                      Add
                    </button>
                  )}
                </div>
              );
            })
          )}
        </div>

        <div className="px-6 py-4 border-t border-border flex justify-end">
          <button
            onClick={onClose}
            className="px-6 py-2 bg-accent-blue text-white rounded-lg font-medium hover:bg-accent-blue/80 transition-colors"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
