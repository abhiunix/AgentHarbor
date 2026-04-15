import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";
import { DebugPath } from "../components/common/DebugPath";

interface CursorRule {
  name: string;
  description: string;
  globs: string;
  always_apply: boolean;
  file_path: string;
}

interface CursorRuleDetail extends CursorRule {
  content: string;
}

type Tab = "project" | "global";

export function CursorRulesPage() {
  const [activeTab, setActiveTab] = useState<Tab>("project");
  const [projectPath, setProjectPath] = useState<string | null>(null);

  return (
    <div className="h-full flex flex-col">
      <div className="px-6 pt-6 pb-0">
        <div className="flex items-center justify-between flex-wrap gap-3 mb-4">
          <div className="flex items-center gap-3">
            <span className="w-3 h-3 rounded-full bg-blue-500 flex-shrink-0" />
            <div>
              <h1 className="text-2xl font-semibold text-text-primary">
                Cursor — Rules
              </h1>
              <p className="text-sm text-text-secondary">
                Manage .cursor/rules/*.mdc files
              </p>
              <DebugPath path=".cursor/rules/*.mdc · ~/.cursor/rules/" />
            </div>
          </div>
          {activeTab === "project" && (
            <ProjectScopeSelector value={projectPath} onChange={setProjectPath} />
          )}
        </div>

        <div className="flex gap-1 border-b border-border">
          <button
            onClick={() => setActiveTab("project")}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
              activeTab === "project"
                ? "border-accent-blue text-text-primary"
                : "border-transparent text-text-secondary hover:text-text-primary"
            }`}
          >
            Project Rules
          </button>
          <button
            onClick={() => setActiveTab("global")}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
              activeTab === "global"
                ? "border-accent-blue text-text-primary"
                : "border-transparent text-text-secondary hover:text-text-primary"
            }`}
          >
            Global Rules
          </button>
        </div>
      </div>

      {activeTab === "project" ? (
        <RulesTab projectPath={projectPath} />
      ) : (
        <RulesTab projectPath={null} isGlobal />
      )}
    </div>
  );
}

function RulesTab({
  projectPath,
  isGlobal = false,
}: {
  projectPath: string | null;
  isGlobal?: boolean;
}) {
  const [rules, setRules] = useState<CursorRule[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [expandedRule, setExpandedRule] = useState<string | null>(null);
  const [ruleContent, setRuleContent] = useState<Record<string, string>>({});
  const [loadingContent, setLoadingContent] = useState<string | null>(null);
  const [showNewForm, setShowNewForm] = useState(false);

  const load = useCallback(async () => {
    if (!isGlobal && !projectPath) {
      setRules([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const result = isGlobal
        ? await invoke<CursorRule[]>("list_global_cursor_rules")
        : await invoke<CursorRule[]>("list_cursor_rules", { projectPath });
      setRules(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [projectPath, isGlobal]);

  useEffect(() => {
    load();
  }, [load]);

  const handleExpand = async (rule: CursorRule) => {
    const key = rule.name;
    if (expandedRule === key) {
      setExpandedRule(null);
      return;
    }
    setExpandedRule(key);
    if (!ruleContent[key]) {
      setLoadingContent(key);
      try {
        const detail = await invoke<CursorRuleDetail>("read_cursor_rule", {
          projectPath: isGlobal ? null : projectPath,
          ruleName: rule.name,
        });
        setRuleContent((prev) => ({ ...prev, [key]: detail.content }));
      } catch (e) {
        setRuleContent((prev) => ({
          ...prev,
          [key]: `Error loading rule: ${e}`,
        }));
      } finally {
        setLoadingContent(null);
      }
    }
  };

  const handleDelete = async (ruleName: string) => {
    if (!window.confirm(`Delete rule "${ruleName}"?`)) return;
    try {
      await invoke("delete_cursor_rule", {
        projectPath: isGlobal ? null : projectPath,
        ruleName,
      });
      setExpandedRule(null);
      await load();
    } catch (e) {
      setError(String(e));
    }
  };

  if (!isGlobal && !projectPath) {
    return (
      <div className="p-6 text-text-secondary text-sm">
        Select a project to view its Cursor rules.
      </div>
    );
  }

  if (loading) {
    return (
      <div className="p-6 text-text-secondary text-sm">Loading rules...</div>
    );
  }

  if (error) {
    return (
      <div className="p-6">
        <div className="px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400">
          {error}
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-6 space-y-3">
      <div className="flex items-center justify-between mb-2">
        <span className="text-sm text-text-secondary">
          {rules.length} rule{rules.length !== 1 ? "s" : ""}
        </span>
        <button
          onClick={() => setShowNewForm(true)}
          className="px-3 py-1.5 text-sm rounded bg-blue-600 hover:bg-blue-500 text-white transition-colors"
        >
          + New Rule
        </button>
      </div>

      {showNewForm && (
        <NewRuleForm
          projectPath={isGlobal ? null : projectPath}
          onClose={() => setShowNewForm(false)}
          onSaved={() => {
            setShowNewForm(false);
            load();
          }}
        />
      )}

      {rules.length === 0 && !showNewForm && (
        <div className="text-center py-12">
          <p className="text-text-secondary text-sm">No rules found.</p>
          <p className="text-text-muted text-xs mt-1">
            Click "+ New Rule" to create one.
          </p>
        </div>
      )}

      {rules.map((rule) => {
        const isExpanded = expandedRule === rule.name;
        const content = ruleContent[rule.name];
        const isLoadingThis = loadingContent === rule.name;

        return (
          <div
            key={rule.name}
            className="bg-app-card border border-border rounded-lg overflow-hidden"
          >
            <button
              onClick={() => handleExpand(rule)}
              className="w-full px-4 py-3 flex items-center gap-3 text-left hover:bg-app-card-hover transition-colors"
            >
              <span className="text-text-secondary text-sm flex-shrink-0">
                {isExpanded ? "\u25BC" : "\u25B6"}
              </span>
              <div className="flex-1 min-w-0">
                <div className="flex items-center gap-2">
                  <h3 className="text-sm font-medium text-text-primary truncate">
                    {rule.name}
                  </h3>
                  {rule.always_apply && (
                    <span className="px-1.5 py-0.5 text-[10px] font-medium rounded bg-green-500/20 text-green-400 flex-shrink-0">
                      Always Apply
                    </span>
                  )}
                </div>
                {rule.description && (
                  <p className="text-xs text-text-secondary truncate mt-0.5">
                    {rule.description}
                  </p>
                )}
                {rule.globs && (
                  <div className="flex gap-1 mt-1 flex-wrap">
                    {rule.globs.split(",").map((g) => (
                      <span
                        key={g.trim()}
                        className="px-1.5 py-0.5 text-[10px] rounded bg-blue-500/15 text-blue-400 font-mono"
                      >
                        {g.trim()}
                      </span>
                    ))}
                  </div>
                )}
              </div>
            </button>

            {isExpanded && (
              <div className="border-t border-border px-4 py-3">
                {isLoadingThis ? (
                  <p className="text-text-secondary text-sm">
                    Loading content...
                  </p>
                ) : content != null ? (
                  <>
                    <pre className="text-xs text-text-primary font-mono whitespace-pre-wrap break-words max-h-96 overflow-y-auto bg-[#13141a] rounded p-3 mb-3">
                      {content}
                    </pre>
                    <button
                      onClick={() => handleDelete(rule.name)}
                      className="px-3 py-1.5 text-sm rounded bg-red-600/20 text-red-400 hover:bg-red-600/30 transition-colors"
                    >
                      Delete Rule
                    </button>
                  </>
                ) : null}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

function NewRuleForm({
  projectPath,
  onClose,
  onSaved,
}: {
  projectPath: string | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [globs, setGlobs] = useState("");
  const [alwaysApply, setAlwaysApply] = useState(false);
  const [content, setContent] = useState("");
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSave = async () => {
    if (!name.trim()) return;
    setSaving(true);
    setError(null);
    try {
      await invoke("write_cursor_rule", {
        projectPath,
        ruleName: name.trim(),
        description,
        globs,
        alwaysApply,
        content,
      });
      onSaved();
    } catch (e) {
      setError(String(e));
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="bg-app-card border border-border rounded-lg p-4 space-y-3">
      <div className="flex items-center justify-between">
        <h3 className="text-sm font-semibold text-text-primary">New Rule</h3>
        <button
          onClick={onClose}
          className="text-text-muted hover:text-text-primary text-sm"
        >
          Cancel
        </button>
      </div>

      {error && (
        <div className="px-3 py-2 bg-red-500/10 border border-red-500/30 rounded text-sm text-red-400">
          {error}
        </div>
      )}

      <div>
        <label className="block text-xs text-text-secondary mb-1">Name</label>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="my-rule"
          className="w-full bg-[#13141a] border border-border rounded px-3 py-1.5 text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:border-blue-500"
        />
      </div>

      <div>
        <label className="block text-xs text-text-secondary mb-1">
          Description
        </label>
        <input
          type="text"
          value={description}
          onChange={(e) => setDescription(e.target.value)}
          placeholder="Brief description"
          className="w-full bg-[#13141a] border border-border rounded px-3 py-1.5 text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:border-blue-500"
        />
      </div>

      <div>
        <label className="block text-xs text-text-secondary mb-1">
          Globs (comma-separated)
        </label>
        <input
          type="text"
          value={globs}
          onChange={(e) => setGlobs(e.target.value)}
          placeholder="*.ts, *.tsx"
          className="w-full bg-[#13141a] border border-border rounded px-3 py-1.5 text-sm text-text-primary placeholder:text-text-muted focus:outline-none focus:border-blue-500"
        />
      </div>

      <label className="flex items-center gap-2 cursor-pointer">
        <div
          className={`relative w-10 h-5 rounded-full transition-colors ${
            alwaysApply ? "bg-green-500" : "bg-[#2a2b36]"
          }`}
          onClick={() => setAlwaysApply((v) => !v)}
        >
          <div
            className={`absolute top-0.5 w-4 h-4 rounded-full bg-white transition-transform ${
              alwaysApply ? "translate-x-5" : "translate-x-0.5"
            }`}
          />
        </div>
        <span className="text-sm text-text-primary">Always Apply</span>
      </label>

      <div>
        <label className="block text-xs text-text-secondary mb-1">
          Content
        </label>
        <textarea
          value={content}
          onChange={(e) => setContent(e.target.value)}
          rows={10}
          placeholder="Rule content..."
          className="w-full bg-[#13141a] border border-border rounded px-3 py-2 text-sm text-text-primary font-mono placeholder:text-text-muted focus:outline-none focus:border-blue-500 resize-y"
          spellCheck={false}
        />
      </div>

      <button
        onClick={handleSave}
        disabled={saving || !name.trim()}
        className="px-4 py-2 text-sm rounded bg-blue-600 hover:bg-blue-500 text-white transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
      >
        {saving ? "Saving..." : "Save Rule"}
      </button>
    </div>
  );
}
