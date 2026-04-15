import { useIsDebugMode } from "../../hooks/useDebugMode";

interface DebugPathProps {
  /** The source path string to display (e.g. "~/.claude/settings.json") */
  path: string;
  /** Extra Tailwind classes */
  className?: string;
}

/**
 * Renders a monospace source-path label only when debug mode is enabled.
 * Usage: <DebugPath path="~/.claude/settings.json" />
 */
export function DebugPath({ path, className = "" }: DebugPathProps) {
  const debug = useIsDebugMode();
  if (!debug) return null;
  return (
    <p className={`text-text-muted text-xs font-mono mt-1 ${className}`}>
      {path}
    </p>
  );
}
