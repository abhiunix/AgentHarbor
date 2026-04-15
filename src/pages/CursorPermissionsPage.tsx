import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DebugPath } from "../components/common/DebugPath";

interface CursorPermissions {
  allow: string[];
  deny: string[];
}

export function CursorPermissionsPage() {
  const [allow, setAllow] = useState<string[]>([]);
  const [deny, setDeny] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);

  async function loadData() {
    setLoading(true);
    setError(null);
    try {
      const perms = await invoke<CursorPermissions>("get_cursor_permissions");
      setAllow([...perms.allow]);
      setDeny([...perms.deny]);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => {
    loadData();
  }, []);

  async function handleSave() {
    if (
      !window.confirm(
        "Save Cursor permissions? This directly affects IDE behavior."
      )
    )
      return;
    setSaving(true);
    setError(null);
    try {
      await invoke("update_cursor_permissions", {
        allow,
        deny,
      });
      await loadData();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  }

  if (loading) {
    return (
      <div className="p-6 flex items-center justify-center h-64">
        <p className="text-text-secondary">Loading permissions...</p>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col">
      <div className="px-6 pt-6 pb-4">
        <div className="flex items-center gap-3 mb-2">
          <span className="w-3 h-3 rounded-full bg-blue-500 flex-shrink-0" />
          <h1 className="text-2xl font-semibold text-text-primary">
            Cursor — Permissions
          </h1>
        </div>
        <DebugPath path="~/.cursor/cli-config.json" className="text-sm" />
        <p className="text-sm text-text-secondary mb-4">
          Manage allow/deny lists
        </p>

        {error && (
          <div className="px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400 mb-4">
            {error}
          </div>
        )}

        <div className="bg-amber-900/30 border border-amber-600/50 rounded-lg px-4 py-3 mb-4 flex items-start gap-3">
          <span className="text-amber-400 text-lg leading-none mt-0.5">
            &#x26A0;
          </span>
          <p className="text-amber-200 text-sm">
            Changes directly affect IDE behavior. Save with care.
          </p>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto px-6 pb-6">
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Allow List */}
          <div className="bg-app-card border border-border rounded-lg p-5">
            <h2 className="text-lg font-semibold text-text-primary mb-1">
              Allow List
            </h2>
            <p className="text-xs text-text-muted mb-4">
              Permitted tool patterns
            </p>
            <PermissionList
              items={allow}
              onRemove={(item) =>
                setAllow((prev) => prev.filter((i) => i !== item))
              }
              onAdd={(item) => setAllow((prev) => [...prev, item])}
            />
          </div>

          {/* Deny List */}
          <div className="bg-app-card border border-border rounded-lg p-5">
            <h2 className="text-lg font-semibold text-text-primary mb-1">
              Deny List
            </h2>
            <p className="text-xs text-text-muted mb-4">
              Blocked tool patterns
            </p>
            <PermissionList
              items={deny}
              onRemove={(item) =>
                setDeny((prev) => prev.filter((i) => i !== item))
              }
              onAdd={(item) => setDeny((prev) => [...prev, item])}
            />
          </div>
        </div>

        <div className="mt-6">
          <button
            onClick={handleSave}
            disabled={saving}
            className="px-6 py-2.5 bg-blue-600 hover:bg-blue-500 text-white text-sm font-medium rounded-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {saving ? "Saving..." : "Save Permissions"}
          </button>
        </div>
      </div>
    </div>
  );
}

function PermissionList({
  items,
  onRemove,
  onAdd,
}: {
  items: string[];
  onRemove: (item: string) => void;
  onAdd: (item: string) => void;
}) {
  const [newItem, setNewItem] = useState("");

  function handleAdd() {
    const trimmed = newItem.trim();
    if (trimmed && !items.includes(trimmed)) {
      onAdd(trimmed);
      setNewItem("");
    }
  }

  return (
    <div>
      <div className="space-y-1 mb-3">
        {items.length === 0 && (
          <p className="text-xs text-text-muted italic">None</p>
        )}
        {items.map((item) => (
          <div
            key={item}
            className="flex items-center justify-between bg-[#13141a] rounded px-3 py-1.5 text-sm text-text-primary group"
          >
            <span className="font-mono text-xs truncate mr-2">{item}</span>
            <button
              onClick={() => onRemove(item)}
              className="text-text-muted hover:text-red-400 transition-colors shrink-0"
            >
              &#x2715;
            </button>
          </div>
        ))}
      </div>
      <div className="flex gap-2">
        <input
          type="text"
          value={newItem}
          onChange={(e) => setNewItem(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleAdd()}
          placeholder="e.g. Bash(npm*)"
          className="flex-1 bg-[#13141a] border border-border rounded px-3 py-1.5 text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:border-blue-500"
        />
        <button
          onClick={handleAdd}
          disabled={!newItem.trim()}
          className="px-3 py-1.5 text-sm rounded bg-blue-600 hover:bg-blue-500 text-white disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
        >
          Add
        </button>
      </div>
    </div>
  );
}
