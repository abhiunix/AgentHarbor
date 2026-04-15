import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ProjectScopeSelector } from "../components/common/ProjectScopeSelector";
import { DebugPath } from "../components/common/DebugPath";

const GEMINI_COLOR = "#4285f4";

interface GeminiAgent {
  name: string;
  file_path: string;
  is_global: boolean;
  size_bytes: number;
}

type Tab = "project" | "global";

export function GeminiAgentsPage() {
  const [activeTab, setActiveTab] = useState<Tab>("project");
  const [projectPath, setProjectPath] = useState<string | null>(null);

  const [agents, setAgents] = useState<GeminiAgent[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [expandedAgent, setExpandedAgent] = useState<string | null>(null);
  const [agentContent, setAgentContent] = useState<string>("");
  const [contentLoading, setContentLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    setExpandedAgent(null);
    setAgentContent("");
    try {
      const path = activeTab === "project" ? projectPath : null;
      if (activeTab === "project" && !projectPath) {
        setAgents([]);
        setLoading(false);
        return;
      }
      const list = await invoke<GeminiAgent[]>("list_gemini_agents", {
        projectPath: path,
      });
      setAgents(list);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [activeTab, projectPath]);

  useEffect(() => {
    load();
  }, [load]);

  const handleExpand = async (agent: GeminiAgent) => {
    if (expandedAgent === agent.file_path) {
      setExpandedAgent(null);
      setAgentContent("");
      return;
    }
    setExpandedAgent(agent.file_path);
    setContentLoading(true);
    try {
      const content = await invoke<string>("read_gemini_agent", {
        filePath: agent.file_path,
      });
      setAgentContent(content);
    } catch {
      setAgentContent("(Unable to read agent file)");
    } finally {
      setContentLoading(false);
    }
  };

  const tabs: { id: Tab; label: string }[] = [
    { id: "project", label: "Project Agents" },
    { id: "global", label: "Global Agents" },
  ];

  return (
    <div className="h-full flex flex-col">
      <div className="p-6 border-b border-border">
        <div className="flex items-center justify-between flex-wrap gap-3">
          <div>
            <div className="flex items-center gap-2 mb-1">
              <span
                className="w-3 h-3 rounded-full"
                style={{ backgroundColor: GEMINI_COLOR }}
              />
              <h1 className="text-2xl font-semibold text-text-primary">Gemini CLI — Agents</h1>
            </div>
            <p className="text-text-muted text-sm">Browse Gemini sub-agents</p>
            <DebugPath path=".gemini/agents/ · ~/.gemini/agents/" />
          </div>
          <button
            onClick={load}
            disabled={loading}
            className="text-sm text-accent-blue hover:underline disabled:opacity-50"
          >
            Refresh
          </button>
        </div>
      </div>

      {/* Tabs */}
      <div className="px-6 pt-4 flex gap-1 border-b border-border">
        {tabs.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition-colors ${
              activeTab === tab.id
                ? "text-text-primary"
                : "border-transparent text-text-muted hover:text-text-primary"
            }`}
            style={
              activeTab === tab.id ? { borderBottomColor: GEMINI_COLOR } : undefined
            }
          >
            {tab.label}
          </button>
        ))}
      </div>

      {/* Project scope selector */}
      {activeTab === "project" && (
        <div className="px-6 pt-4">
          <ProjectScopeSelector
            value={projectPath}
            onChange={setProjectPath}
          />
        </div>
      )}

      <div className="flex-1 overflow-y-auto p-6">
        {error && (
          <div className="mb-4 px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400">
            {error}
          </div>
        )}

        {activeTab === "project" && !projectPath ? (
          <div className="py-16 text-center text-text-muted text-sm">
            Select a project to browse its Gemini agents.
          </div>
        ) : loading ? (
          <div className="h-64 flex items-center justify-center text-text-muted">Loading...</div>
        ) : agents.length === 0 ? (
          <div className="py-16 text-center text-text-muted text-sm">
            No agents found. Agents are stored as .md files in .gemini/agents/ directories.
          </div>
        ) : (
          <div className="space-y-3">
            {agents.map((agent) => (
              <div key={agent.file_path}>
                <div
                  onClick={() => handleExpand(agent)}
                  className={`p-4 bg-app-card border rounded-lg cursor-pointer transition-colors hover:bg-card-hover ${
                    expandedAgent === agent.file_path
                      ? "border-[#4285f4]"
                      : "border-border"
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-3">
                      <h3 className="text-sm font-semibold text-text-primary">{agent.name}</h3>
                      <span
                        className={`px-1.5 py-0.5 text-[10px] font-medium rounded ${
                          agent.is_global
                            ? "bg-blue-500/20 text-blue-400"
                            : "bg-green-500/20 text-green-400"
                        }`}
                      >
                        {agent.is_global ? "Global" : "Project"}
                      </span>
                    </div>
                    <span className="text-xs text-text-muted">{formatBytes(agent.size_bytes)}</span>
                  </div>
                  <p className="text-xs text-text-muted font-mono mt-1 truncate">
                    {agent.file_path}
                  </p>
                </div>

                {/* Expanded content */}
                {expandedAgent === agent.file_path && (
                  <div className="mt-2 p-4 bg-app-card border border-border rounded-lg">
                    <h4 className="text-xs font-semibold text-text-muted mb-2">Content</h4>
                    {contentLoading ? (
                      <p className="text-sm text-text-muted">Loading...</p>
                    ) : (
                      <pre className="text-sm text-text-primary font-mono whitespace-pre-wrap max-h-[400px] overflow-y-auto">
                        {agentContent}
                      </pre>
                    )}
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
