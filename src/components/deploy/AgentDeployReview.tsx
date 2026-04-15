import type { AgentDefinition } from "../../lib/types";

interface AgentDeployReviewProps {
  agent: AgentDefinition;
  onContinue: () => void;
  onBack: () => void;
  loading: boolean;
}

export function AgentDeployReview({
  agent,
  onContinue,
  onBack,
  loading,
}: AgentDeployReviewProps) {
  const colorMap: Record<string, string> = {
    red: "#ef4444",
    blue: "#3b82f6",
    green: "#22c55e",
    yellow: "#eab308",
    purple: "#9333ea",
    orange: "#f97316",
    pink: "#ec4899",
    cyan: "#06b6d4",
  };

  const requiredCaps = agent.required_capabilities || [];
  const hasRequirements = requiredCaps.length > 0;

  return (
    <div className="p-6 space-y-6">
      <div
        className="rounded-lg border border-border overflow-hidden"
        style={{ borderTopWidth: "3px", borderTopColor: colorMap[agent.color] || "#3b82f6" }}
      >
        <div className="p-4 bg-app-card">
          <div className="flex items-start gap-3">
            <div
              className="w-3 h-3 rounded-full mt-1"
              style={{ backgroundColor: colorMap[agent.color] || "#3b82f6" }}
            />
            <div className="flex-1">
              <div className="flex items-center gap-2 mb-1">
                <h3 className="font-semibold text-text-primary">{agent.name}</h3>
                <span className="text-[10px] px-2 py-0.5 rounded bg-white/10 text-text-muted uppercase">
                  {agent.model}
                </span>
                <span className="text-[10px] px-2 py-0.5 rounded bg-white/10 text-text-muted uppercase">
                  {agent.memory}
                </span>
              </div>
              <p className="text-sm text-text-muted">{agent.description}</p>
              <p className="text-xs font-mono text-text-muted mt-2">{agent.id}</p>
            </div>
          </div>
        </div>
      </div>

      <div className="bg-app-card border border-border rounded-lg p-4">
        <p className="text-xs text-text-muted uppercase mb-2">Deploy Path</p>
        <p className="font-mono text-sm text-text-primary">
          agents/{agent.id.split("/").pop()}.md
        </p>
      </div>

      <div className="bg-accent-blue/10 border border-accent-blue/30 rounded-lg p-4">
        <p className="text-sm text-text-primary font-medium mb-1">📄 Agent File Contents</p>
        <p className="text-xs text-text-muted">
          YAML frontmatter (name, model, color, memory) + system prompt body
        </p>
      </div>

      <div>
        <p className="text-xs text-text-muted uppercase mb-3">Required Capabilities</p>
        {hasRequirements ? (
          <div className="space-y-2">
            {requiredCaps.map((capId) => (
              <div
                key={capId}
                className="flex items-center gap-3 p-3 rounded-lg bg-app-card border border-border"
              >
                <div className="w-4 h-4 rounded bg-accent-green flex items-center justify-center">
                  <span className="text-white text-xs">✓</span>
                </div>
                <span className="font-mono text-sm text-text-primary">{capId}</span>
                <span className="text-[10px] text-text-muted">(auto-included)</span>
              </div>
            ))}
          </div>
        ) : (
          <div className="p-4 rounded-lg bg-app-card border border-border text-center">
            <p className="text-sm text-text-muted">
              No additional capabilities needed.
            </p>
            <p className="text-xs text-text-muted mt-1">
              Only the agent <code className="font-mono">.md</code> file will be deployed.
            </p>
          </div>
        )}
      </div>

      <div className="flex items-center justify-between pt-4 border-t border-border">
        <button
          onClick={onBack}
          className="px-4 py-2 text-sm text-text-muted hover:text-text-primary transition-colors"
        >
          ← Back
        </button>
        <button
          onClick={onContinue}
          disabled={loading}
          className="px-6 py-2 bg-accent-purple text-white rounded-lg font-medium hover:bg-accent-purple/80 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {loading ? "Loading..." : "Preview Changes"}
        </button>
      </div>
    </div>
  );
}
