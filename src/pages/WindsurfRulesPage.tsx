import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";
import { DebugPath } from "../components/common/DebugPath";

interface WindsurfRuleEntry {
  name: string;
  file_path: string;
  is_legacy: boolean;
  size_bytes: number;
  modified_at: string;
}

export function WindsurfRulesPage() {
  const [projectPath, setProjectPath] = useState<string | null>(null);

  // Legacy rules state
  const [legacyContent, setLegacyContent] = useState("");
  const [savedLegacyContent, setSavedLegacyContent] = useState("");
  const [legacyLoading, setLegacyLoading] = useState(false);
  const [legacySaving, setLegacySaving] = useState(false);
  const [legacyError, setLegacyError] = useState<string | null>(null);

  // Rules directory state
  const [rules, setRules] = useState<WindsurfRuleEntry[]>([]);
  const [rulesLoading, setRulesLoading] = useState(false);
  const [rulesError, setRulesError] = useState<string | null>(null);

  // Editor state for rules dir files
  const [selectedRule, setSelectedRule] = useState<string | null>(null);
  const [ruleContent, setRuleContent] = useState("");
  const [savedRuleContent, setSavedRuleContent] = useState("");
  const [ruleLoading, setRuleLoading] = useState(false);
  const [ruleSaving, setRuleSaving] = useState(false);
  const [ruleError, setRuleError] = useState<string | null>(null);

  // New rule state
  const [showNewRule, setShowNewRule] = useState(false);
  const [newRuleName, setNewRuleName] = useState("");

  const loadLegacy = useCallback(async () => {
    if (!projectPath) return;
    setLegacyLoading(true);
    setLegacyError(null);
    try {
      const text = await invoke<string>("read_windsurf_legacy_rules", { projectPath });
      setLegacyContent(text);
      setSavedLegacyContent(text);
    } catch (e) {
      setLegacyError(e instanceof Error ? e.message : String(e));
    } finally {
      setLegacyLoading(false);
    }
  }, [projectPath]);

  const loadRules = useCallback(async () => {
    if (!projectPath) return;
    setRulesLoading(true);
    setRulesError(null);
    try {
      const entries = await invoke<WindsurfRuleEntry[]>("list_windsurf_rules", { projectPath });
      setRules(entries);
    } catch (e) {
      setRulesError(e instanceof Error ? e.message : String(e));
    } finally {
      setRulesLoading(false);
    }
  }, [projectPath]);

  useEffect(() => {
    if (projectPath) {
      loadLegacy();
      loadRules();
    } else {
      setLegacyContent("");
      setSavedLegacyContent("");
      setRules([]);
      setSelectedRule(null);
    }
  }, [projectPath, loadLegacy, loadRules]);

  const handleSaveLegacy = async () => {
    if (!projectPath) return;
    setLegacySaving(true);
    setLegacyError(null);
    try {
      await invoke("write_windsurf_legacy_rules", { projectPath, content: legacyContent });
      setSavedLegacyContent(legacyContent);
    } catch (e) {
      setLegacyError(e instanceof Error ? e.message : String(e));
    } finally {
      setLegacySaving(false);
    }
  };

  const handleSelectRule = async (fileName: string) => {
    if (!projectPath) return;
    setSelectedRule(fileName);
    setRuleLoading(true);
    setRuleError(null);
    try {
      const text = await invoke<string>("read_windsurf_rules_file", { projectPath, fileName });
      setRuleContent(text);
      setSavedRuleContent(text);
    } catch (e) {
      setRuleError(e instanceof Error ? e.message : String(e));
    } finally {
      setRuleLoading(false);
    }
  };

  const handleSaveRule = async () => {
    if (!projectPath || !selectedRule) return;
    setRuleSaving(true);
    setRuleError(null);
    try {
      await invoke("write_windsurf_rules_file", {
        projectPath,
        fileName: selectedRule,
        content: ruleContent,
      });
      setSavedRuleContent(ruleContent);
      loadRules();
    } catch (e) {
      setRuleError(e instanceof Error ? e.message : String(e));
    } finally {
      setRuleSaving(false);
    }
  };

  const handleDeleteRule = async (fileName: string) => {
    if (!projectPath) return;
    setRulesError(null);
    try {
      await invoke("delete_windsurf_rule", { projectPath, fileName });
      if (selectedRule === fileName) {
        setSelectedRule(null);
        setRuleContent("");
        setSavedRuleContent("");
      }
      loadRules();
    } catch (e) {
      setRulesError(e instanceof Error ? e.message : String(e));
    }
  };

  const handleCreateRule = async () => {
    if (!projectPath || !newRuleName.trim()) return;
    const fileName = newRuleName.trim().endsWith(".md")
      ? newRuleName.trim()
      : newRuleName.trim() + ".md";
    setRulesError(null);
    try {
      await invoke("write_windsurf_rules_file", {
        projectPath,
        fileName,
        content: "",
      });
      setShowNewRule(false);
      setNewRuleName("");
      await loadRules();
      handleSelectRule(fileName);
    } catch (e) {
      setRulesError(e instanceof Error ? e.message : String(e));
    }
  };

  const legacyDirty = legacyContent !== savedLegacyContent;
  const ruleDirty = ruleContent !== savedRuleContent;

  if (!projectPath) {
    return (
      <div className="h-full flex flex-col">
        <div className="p-6 border-b border-border">
          <div className="flex items-center gap-2 mb-1">
            <span className="w-3 h-3 rounded-full" style={{ backgroundColor: "#22c55e" }} />
            <h1 className="text-2xl font-semibold text-text-primary">Windsurf — Rules</h1>
          </div>
          <DebugPath path=".windsurfrules · .windsurf/rules/" />
        </div>
        <div className="flex-1 flex flex-col items-center justify-center p-6 gap-4">
          <p className="text-text-muted text-sm">Select a project to manage Windsurf rules.</p>
          <ProjectScopeSelector value={projectPath} onChange={setProjectPath} />
        </div>
      </div>
    );
  }

  return (
    <div className="h-full flex flex-col">
      <div className="p-6 border-b border-border flex items-center justify-between flex-wrap gap-3">
        <div>
          <div className="flex items-center gap-2 mb-1">
            <span className="w-3 h-3 rounded-full" style={{ backgroundColor: "#22c55e" }} />
            <h1 className="text-2xl font-semibold text-text-primary">Windsurf — Rules</h1>
          </div>
          <DebugPath path=".windsurfrules · .windsurf/rules/" />
        </div>
        <ProjectScopeSelector value={projectPath} onChange={setProjectPath} />
      </div>

      <div className="flex-1 overflow-y-auto p-6 space-y-8">
        {/* Legacy Rules Section */}
        <section>
          <div className="flex items-center justify-between mb-3">
            <h2 className="text-lg font-semibold text-text-primary">
              Legacy Rules (.windsurfrules)
            </h2>
            <div className="flex items-center gap-2">
              {legacyDirty && (
                <span className="text-xs text-amber-400 font-medium">Unsaved changes</span>
              )}
              <button
                onClick={loadLegacy}
                disabled={legacyLoading}
                className="text-sm text-accent-blue hover:underline disabled:opacity-50"
              >
                Refresh
              </button>
              <button
                onClick={handleSaveLegacy}
                disabled={legacyLoading || legacySaving || !legacyDirty}
                className="px-4 py-2 rounded-lg bg-accent-blue text-white text-sm font-medium hover:bg-accent-blue/90 disabled:opacity-50"
              >
                {legacySaving ? "Saving..." : "Save"}
              </button>
            </div>
          </div>
          {legacyError && <p className="text-sm text-accent-red mb-2">{legacyError}</p>}
          {legacyLoading ? (
            <div className="h-32 flex items-center justify-center text-text-muted">Loading...</div>
          ) : (
            <textarea
              value={legacyContent}
              onChange={(e) => setLegacyContent(e.target.value)}
              className="w-full min-h-[160px] px-4 py-3 bg-app-card border border-border rounded-lg font-mono text-sm text-text-primary focus:outline-none focus:border-accent-blue resize-y"
              placeholder="# .windsurfrules&#10;&#10;Add legacy Windsurf rules here..."
            />
          )}
        </section>

        {/* Rules Directory Section */}
        <section>
          <div className="flex items-center justify-between mb-3">
            <h2 className="text-lg font-semibold text-text-primary">
              Rules Directory (.windsurf/rules/)
            </h2>
            <div className="flex items-center gap-2">
              <button
                onClick={loadRules}
                disabled={rulesLoading}
                className="text-sm text-accent-blue hover:underline disabled:opacity-50"
              >
                Refresh
              </button>
              <button
                onClick={() => setShowNewRule(true)}
                className="px-4 py-2 rounded-lg bg-green-600 text-white text-sm font-medium hover:bg-green-700"
              >
                + New Rule
              </button>
            </div>
          </div>

          {rulesError && <p className="text-sm text-accent-red mb-2">{rulesError}</p>}

          {/* New rule input */}
          {showNewRule && (
            <div className="flex items-center gap-2 mb-4 p-3 bg-app-card border border-border rounded-lg">
              <input
                type="text"
                value={newRuleName}
                onChange={(e) => setNewRuleName(e.target.value)}
                placeholder="rule-name.md"
                className="flex-1 px-3 py-2 bg-app-bg border border-border rounded-lg text-text-primary text-sm font-mono focus:outline-none focus:border-accent-blue"
                onKeyDown={(e) => e.key === "Enter" && handleCreateRule()}
              />
              <button
                onClick={handleCreateRule}
                disabled={!newRuleName.trim()}
                className="px-3 py-2 rounded-lg bg-green-600 text-white text-sm font-medium hover:bg-green-700 disabled:opacity-50"
              >
                Create
              </button>
              <button
                onClick={() => {
                  setShowNewRule(false);
                  setNewRuleName("");
                }}
                className="px-3 py-2 rounded-lg border border-border text-text-muted text-sm hover:text-text-primary"
              >
                Cancel
              </button>
            </div>
          )}

          {rulesLoading ? (
            <div className="h-32 flex items-center justify-center text-text-muted">Loading...</div>
          ) : rules.length === 0 ? (
            <div className="py-8 text-center text-text-muted text-sm">
              No rules found in .windsurf/rules/ directory.
            </div>
          ) : (
            <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3 mb-4">
              {rules.map((rule) => (
                <div
                  key={rule.name}
                  onClick={() => handleSelectRule(rule.name)}
                  className={`p-4 bg-app-card border rounded-lg cursor-pointer transition-colors hover:bg-card-hover ${
                    selectedRule === rule.name
                      ? "border-green-500"
                      : "border-border"
                  }`}
                >
                  <div className="flex items-start justify-between">
                    <h3 className="text-sm font-semibold text-text-primary truncate mr-2">
                      {rule.name}
                    </h3>
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        handleDeleteRule(rule.name);
                      }}
                      className="text-text-muted hover:text-red-400 text-xs flex-shrink-0"
                      title="Delete rule"
                    >
                      Delete
                    </button>
                  </div>
                  <div className="flex items-center gap-3 mt-2 text-xs text-text-muted">
                    <span>{formatBytes(rule.size_bytes)}</span>
                    <span>{formatDate(rule.modified_at)}</span>
                  </div>
                </div>
              ))}
            </div>
          )}

          {/* Rule editor */}
          {selectedRule && (
            <div className="mt-4">
              <div className="flex items-center justify-between mb-2">
                <h3 className="text-sm font-semibold text-text-primary font-mono">
                  {selectedRule}
                </h3>
                <div className="flex items-center gap-2">
                  {ruleDirty && (
                    <span className="text-xs text-amber-400 font-medium">Unsaved changes</span>
                  )}
                  <button
                    onClick={handleSaveRule}
                    disabled={ruleLoading || ruleSaving || !ruleDirty}
                    className="px-4 py-2 rounded-lg bg-accent-blue text-white text-sm font-medium hover:bg-accent-blue/90 disabled:opacity-50"
                  >
                    {ruleSaving ? "Saving..." : "Save"}
                  </button>
                </div>
              </div>
              {ruleError && <p className="text-sm text-accent-red mb-2">{ruleError}</p>}
              {ruleLoading ? (
                <div className="h-32 flex items-center justify-center text-text-muted">
                  Loading...
                </div>
              ) : (
                <textarea
                  value={ruleContent}
                  onChange={(e) => setRuleContent(e.target.value)}
                  className="w-full min-h-[200px] px-4 py-3 bg-app-card border border-border rounded-lg font-mono text-sm text-text-primary focus:outline-none focus:border-accent-blue resize-y"
                  placeholder="Enter rule content..."
                />
              )}
            </div>
          )}
        </section>
      </div>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDate(dateStr: string): string {
  try {
    const d = new Date(dateStr);
    return d.toLocaleDateString(undefined, { month: "short", day: "numeric", year: "numeric" });
  } catch {
    return dateStr;
  }
}
