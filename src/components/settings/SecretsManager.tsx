import { useState, useEffect } from "react";
import {
  listSecrets,
  storeSecret,
  getSecret,
  deleteSecret,
  type SecretInfo,
} from "../../lib/tauri";
import { ConfirmDialog } from "../common/ConfirmDialog";
import { credentialStoreName } from "../../lib/platform";

interface SecretsManagerProps {
  isOpen: boolean;
  onClose: () => void;
}

export function SecretsManager({ isOpen, onClose }: SecretsManagerProps) {
  const [secrets, setSecrets] = useState<SecretInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [showAddForm, setShowAddForm] = useState(false);
  const [newKey, setNewKey] = useState("");
  const [newValue, setNewValue] = useState("");
  const [saving, setSaving] = useState(false);
  const [revealedKey, setRevealedKey] = useState<string | null>(null);
  const [revealedValue, setRevealedValue] = useState<string | null>(null);
  const [editingKey, setEditingKey] = useState<string | null>(null);
  const [editValue, setEditValue] = useState("");

  useEffect(() => {
    if (isOpen) {
      loadSecrets();
    }
  }, [isOpen]);

  const loadSecrets = async () => {
    setLoading(true);
    try {
      const data = await listSecrets();
      setSecrets(data);
    } catch (error) {
      console.error("Failed to load secrets:", error);
    } finally {
      setLoading(false);
    }
  };

  const handleAdd = async () => {
    if (!newKey.trim() || !newValue.trim()) return;
    setSaving(true);
    try {
      await storeSecret(newKey.trim(), newValue.trim());
      setNewKey("");
      setNewValue("");
      setShowAddForm(false);
      loadSecrets();
    } catch (error) {
      console.error("Failed to add secret:", error);
    } finally {
      setSaving(false);
    }
  };

  const handleReveal = async (key: string) => {
    if (revealedKey === key) {
      setRevealedKey(null);
      setRevealedValue(null);
      return;
    }
    try {
      const value = await getSecret(key);
      setRevealedKey(key);
      setRevealedValue(value);
    } catch (error) {
      console.error("Failed to reveal secret:", error);
    }
  };

  const handleEdit = async (key: string) => {
    const existing = secrets.find((s) => s.key === key);
    // Unset (e.g. a reserved secret with no value yet) — start with an empty
    // field instead of probing the keychain for a value that isn't there.
    if (existing && !existing.has_value) {
      setEditingKey(key);
      setEditValue("");
      return;
    }
    try {
      const value = await getSecret(key);
      setEditingKey(key);
      setEditValue(value || "");
    } catch (error) {
      console.error("Failed to get secret for edit:", error);
    }
  };

  const handleSaveEdit = async () => {
    if (!editingKey || !editValue.trim()) return;
    setSaving(true);
    try {
      await storeSecret(editingKey, editValue.trim());
      setEditingKey(null);
      setEditValue("");
      setRevealedKey(null);
      setRevealedValue(null);
      loadSecrets();
    } catch (error) {
      console.error("Failed to save secret:", error);
    } finally {
      setSaving(false);
    }
  };

  const [deleteConfirmKey, setDeleteConfirmKey] = useState<string | null>(null);

  const handleDeleteClick = (key: string) => {
    setDeleteConfirmKey(key);
  };

  const handleDeleteConfirm = async () => {
    if (!deleteConfirmKey) return;
    try {
      await deleteSecret(deleteConfirmKey);
      if (revealedKey === deleteConfirmKey) {
        setRevealedKey(null);
        setRevealedValue(null);
      }
      loadSecrets();
    } catch (error) {
      console.error("Failed to delete secret:", error);
    } finally {
      setDeleteConfirmKey(null);
    }
  };

  if (!isOpen) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
      <div className="bg-app-sidebar border border-border rounded-xl w-[600px] max-h-[80vh] flex flex-col">
        <div className="flex items-center justify-between px-6 py-4 border-b border-border">
          <div>
            <h2 className="text-lg font-semibold text-text-primary">Secrets Manager</h2>
            <p className="text-sm text-text-muted">
              Securely stored in {credentialStoreName}
            </p>
          </div>
          <button
            onClick={onClose}
            className="w-8 h-8 flex items-center justify-center rounded-full hover:bg-white/10 text-text-muted"
          >
            ✕
          </button>
        </div>

        <div className="flex-1 overflow-y-auto p-6">
          {loading ? (
            <p className="text-text-muted text-center py-8">Loading...</p>
          ) : (
            <div className="space-y-3">
              {secrets.length === 0 && !showAddForm ? (
                <div className="text-center py-8">
                  <p className="text-text-muted mb-4">No secrets stored</p>
                  <button
                    onClick={() => setShowAddForm(true)}
                    className="px-4 py-2 bg-accent-blue text-white rounded-lg text-sm"
                  >
                    Add Your First Secret
                  </button>
                </div>
              ) : (
                <>
                  {secrets.map((secret) => (
                    <div
                      key={secret.key}
                      className="p-4 bg-app-card border border-border rounded-lg"
                    >
                      <div className="flex items-start justify-between">
                        <div className="flex-1 min-w-0">
                          <div className="flex items-center gap-2 flex-wrap">
                            <p className="text-sm font-mono text-text-primary font-medium">
                              {secret.key}
                            </p>
                            {secret.reserved && (
                              <span
                                title="Required by AgentHarbor features — can be edited but not deleted"
                                className="inline-flex items-center gap-1 px-1.5 py-0.5 text-[10px] font-medium rounded-full bg-accent-blue/15 border border-accent-blue/30 text-accent-blue"
                              >
                                <svg className="w-2.5 h-2.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 11c0-1.105.895-2 2-2m-8 2V7a4 4 0 118 0m-9 4h10a1 1 0 011 1v7a1 1 0 01-1 1H6a1 1 0 01-1-1v-7a1 1 0 011-1z" />
                                </svg>
                                Reserved
                              </span>
                            )}
                            {!secret.has_value && (
                              <span className="inline-flex items-center px-1.5 py-0.5 text-[10px] font-medium rounded-full bg-white/5 border border-border text-text-muted">
                                Not set
                              </span>
                            )}
                          </div>
                          {revealedKey === secret.key && revealedValue !== null && (
                            <pre className="mt-2 text-xs font-mono bg-app-bg p-2 rounded text-text-secondary break-all whitespace-pre-wrap">
                              {revealedValue}
                            </pre>
                          )}
                          {editingKey === secret.key && (
                            <div className="mt-2 space-y-2">
                              <input
                                type="text"
                                value={editValue}
                                onChange={(e) => setEditValue(e.target.value)}
                                className="w-full px-3 py-2 bg-app-bg border border-border rounded text-sm text-text-primary font-mono focus:outline-none focus:border-accent-blue"
                              />
                              <div className="flex gap-2">
                                <button
                                  onClick={handleSaveEdit}
                                  disabled={saving}
                                  className="px-3 py-1 text-xs bg-accent-green/20 text-accent-green rounded hover:bg-accent-green/30"
                                >
                                  Save
                                </button>
                                <button
                                  onClick={() => setEditingKey(null)}
                                  className="px-3 py-1 text-xs bg-white/5 text-text-muted rounded hover:bg-white/10"
                                >
                                  Cancel
                                </button>
                              </div>
                            </div>
                          )}
                        </div>
                        {editingKey !== secret.key && (
                          <div className="flex items-center gap-1 ml-2">
                            {secret.has_value && (
                              <button
                                onClick={() => handleReveal(secret.key)}
                                className="px-2 py-1 text-xs text-text-muted hover:text-text-primary hover:bg-white/5 rounded"
                              >
                                {revealedKey === secret.key ? "Hide" : "Reveal"}
                              </button>
                            )}
                            <button
                              onClick={() => handleEdit(secret.key)}
                              className="px-2 py-1 text-xs text-accent-blue hover:bg-accent-blue/10 rounded"
                            >
                              {secret.has_value ? "Edit" : "Set"}
                            </button>
                            {!secret.reserved && (
                              <button
                                onClick={() => handleDeleteClick(secret.key)}
                                className="px-2 py-1 text-xs text-accent-red hover:bg-accent-red/10 rounded"
                              >
                                Delete
                              </button>
                            )}
                          </div>
                        )}
                      </div>
                    </div>
                  ))}

                  {showAddForm && (
                    <div className="p-4 bg-app-card border border-accent-blue rounded-lg space-y-3">
                      <input
                        type="text"
                        placeholder="Secret name (e.g., GITHUB_TOKEN)"
                        value={newKey}
                        onChange={(e) => setNewKey(e.target.value)}
                        className="w-full px-3 py-2 bg-app-bg border border-border rounded text-sm text-text-primary focus:outline-none focus:border-accent-blue"
                      />
                      <input
                        type="password"
                        placeholder="Secret value"
                        value={newValue}
                        onChange={(e) => setNewValue(e.target.value)}
                        className="w-full px-3 py-2 bg-app-bg border border-border rounded text-sm text-text-primary focus:outline-none focus:border-accent-blue"
                      />
                      <div className="flex gap-2">
                        <button
                          onClick={handleAdd}
                          disabled={saving || !newKey.trim() || !newValue.trim()}
                          className="px-4 py-2 text-sm bg-accent-blue text-white rounded-lg disabled:opacity-50"
                        >
                          {saving ? "Saving..." : "Add Secret"}
                        </button>
                        <button
                          onClick={() => {
                            setShowAddForm(false);
                            setNewKey("");
                            setNewValue("");
                          }}
                          className="px-4 py-2 text-sm bg-white/5 text-text-muted rounded-lg"
                        >
                          Cancel
                        </button>
                      </div>
                    </div>
                  )}
                </>
              )}
            </div>
          )}
        </div>

        <div className="px-6 py-4 border-t border-border flex justify-between items-center">
          <p className="text-xs text-text-muted">
            Secrets are stored securely in {credentialStoreName}
          </p>
          {secrets.length > 0 && !showAddForm && (
            <button
              onClick={() => setShowAddForm(true)}
              className="px-4 py-2 text-sm bg-accent-blue text-white rounded-lg"
            >
              Add Secret
            </button>
          )}
        </div>
      </div>

      <ConfirmDialog
        isOpen={!!deleteConfirmKey}
        title="Delete Secret"
        message={
          deleteConfirmKey
            ? `Are you sure you want to delete the secret "${deleteConfirmKey}"? This action cannot be undone.`
            : ""
        }
        onConfirm={handleDeleteConfirm}
        onCancel={() => setDeleteConfirmKey(null)}
      />
    </div>
  );
}
