import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DebugPath } from "../components/common/DebugPath";

const CODEX_COLOR = "#10a37f";

interface CodexSkill {
  name: string;
  file_path: string;
  has_scripts: boolean;
  has_resources: boolean;
}

export function CodexSkillsPage() {
  const [skills, setSkills] = useState<CodexSkill[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [expandedSkill, setExpandedSkill] = useState<string | null>(null);
  const [skillContent, setSkillContent] = useState<string>("");
  const [contentLoading, setContentLoading] = useState(false);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    setExpandedSkill(null);
    setSkillContent("");
    try {
      const list = await invoke<CodexSkill[]>("list_codex_skills", {
        projectPath: null,
      });
      setSkills(list);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const handleExpand = async (skill: CodexSkill) => {
    if (expandedSkill === skill.file_path) {
      setExpandedSkill(null);
      setSkillContent("");
      return;
    }
    setExpandedSkill(skill.file_path);
    setContentLoading(true);
    try {
      const content = await invoke<string>("read_codex_skill_file", {
        filePath: skill.file_path + "/SKILL.md",
      });
      setSkillContent(content);
    } catch {
      setSkillContent("(Unable to read SKILL.md)");
    } finally {
      setContentLoading(false);
    }
  };

  return (
    <div className="h-full flex flex-col">
      <div className="p-6 border-b border-border">
        <div className="flex items-center justify-between flex-wrap gap-3">
          <div>
            <div className="flex items-center gap-2 mb-1">
              <span
                className="w-3 h-3 rounded-full"
                style={{ backgroundColor: CODEX_COLOR }}
              />
              <h1 className="text-2xl font-semibold text-text-primary">Codex — Skills</h1>
            </div>
            <p className="text-text-muted text-sm">Browse Codex skills and their resources</p>
            <DebugPath path="~/.codex/skills/" />
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

      <div className="flex-1 overflow-y-auto p-6">
        {error && (
          <div className="mb-4 px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400">
            {error}
          </div>
        )}

        {loading ? (
          <div className="h-64 flex items-center justify-center text-text-muted">Loading...</div>
        ) : skills.length === 0 ? (
          <div className="py-16 text-center text-text-muted text-sm">
            No skills found. Skills are stored in ~/.codex/skills/ directories.
          </div>
        ) : (
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
            {skills.map((skill) => (
              <div key={skill.file_path}>
                <div
                  onClick={() => handleExpand(skill)}
                  className={`p-4 bg-app-card border rounded-lg cursor-pointer transition-colors hover:bg-card-hover ${
                    expandedSkill === skill.file_path
                      ? "border-[#10a37f]"
                      : "border-border"
                  }`}
                >
                  <h3 className="text-sm font-semibold text-text-primary mb-2">{skill.name}</h3>
                  <div className="flex items-center gap-2">
                    {skill.has_scripts && (
                      <span className="px-1.5 py-0.5 text-[10px] font-medium rounded bg-purple-500/20 text-purple-400">
                        Scripts
                      </span>
                    )}
                    {skill.has_resources && (
                      <span className="px-1.5 py-0.5 text-[10px] font-medium rounded bg-green-500/20 text-green-400">
                        Resources
                      </span>
                    )}
                    {!skill.has_scripts && !skill.has_resources && (
                      <span className="text-xs text-text-muted">SKILL.md only</span>
                    )}
                  </div>
                </div>

                {expandedSkill === skill.file_path && (
                  <div className="mt-2 p-4 bg-app-card border border-border rounded-lg">
                    <h4 className="text-xs font-semibold text-text-muted mb-2">SKILL.md</h4>
                    {contentLoading ? (
                      <p className="text-sm text-text-muted">Loading...</p>
                    ) : (
                      <pre className="text-sm text-text-primary font-mono whitespace-pre-wrap max-h-[300px] overflow-y-auto">
                        {skillContent}
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
