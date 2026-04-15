import { useState, useEffect } from "react";
import {
  listAgentMemory,
  clearAgentMemory,
  clearAllAgentMemory,
  type AgentMemory,
} from "../../lib/tauri";

interface AgentMemorySectionProps {
  projectPath: string;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
}

export function AgentMemorySection({ projectPath }: AgentMemorySectionProps) {
  const [memories, setMemories] = useState<AgentMemory[]>([]);
  const [loading, setLoading] = useState(true);
  const [clearing, setClearing] = useState<string | null>(null);

  useEffect(() => {
    loadMemory();
  }, [projectPath]);

  const loadMemory = async () => {
    setLoading(true);
    try {
      const data = await listAgentMemory(projectPath);
      setMemories(data);
    } catch (error) {
      console.error("Failed to load agent memory:", error);
    } finally {
      setLoading(false);
    }
  };

  const handleClear = async (memory: AgentMemory) => {
    if (!confirm(`Clear memory for agent "${memory.agent_name}"? This cannot be undone.`)) {
      return;
    }
    setClearing(memory.path);
    try {
      await clearAgentMemory(memory.path);
      loadMemory();
    } catch (error) {
      console.error("Failed to clear memory:", error);
    } finally {
      setClearing(null);
    }
  };

  const handleClearAll = async () => {
    if (!confirm("Clear ALL agent memory for this project? This cannot be undone.")) {
      return;
    }
    setClearing("all");
    try {
      await clearAllAgentMemory(projectPath);
      loadMemory();
    } catch (error) {
      console.error("Failed to clear all memory:", error);
    } finally {
      setClearing(null);
    }
  };

  const totalSize = memories.reduce((sum, m) => sum + m.size_bytes, 0);

  if (loading) {
    return (
      <section>
        <h4 className="text-xs font-semibold text-text-muted uppercase tracking-wider mb-2">
          Agent Memory
        </h4>
        <p className="text-sm text-text-muted">Loading...</p>
      </section>
    );
  }

  if (memories.length === 0) {
    return (
      <section>
        <h4 className="text-xs font-semibold text-text-muted uppercase tracking-wider mb-2">
          Agent Memory
        </h4>
        <p className="text-sm text-text-muted">No agent memory found</p>
      </section>
    );
  }

  return (
    <section>
      <div className="flex items-center justify-between mb-2">
        <h4 className="text-xs font-semibold text-text-muted uppercase tracking-wider">
          Agent Memory ({formatBytes(totalSize)})
        </h4>
        <button
          onClick={handleClearAll}
          disabled={clearing !== null}
          className="text-xs text-accent-red hover:underline disabled:opacity-50"
        >
          Clear All
        </button>
      </div>
      <div className="space-y-2">
        {memories.map((memory) => (
          <div
            key={memory.path}
            className="flex items-center justify-between p-2 rounded-lg bg-app-card"
          >
            <div className="flex-1 min-w-0">
              <p className="text-sm text-text-primary truncate font-mono">
                {memory.agent_name}
              </p>
              <p className="text-xs text-text-muted">
                {formatBytes(memory.size_bytes)} · {memory.file_count} file{memory.file_count !== 1 ? "s" : ""}
              </p>
            </div>
            <button
              onClick={() => handleClear(memory)}
              disabled={clearing !== null}
              className="ml-2 px-2 py-1 text-xs text-accent-red hover:bg-accent-red/10 rounded disabled:opacity-50"
            >
              {clearing === memory.path ? "..." : "Clear"}
            </button>
          </div>
        ))}
      </div>
    </section>
  );
}
