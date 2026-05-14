/**
 * Adapter Plugin Registry
 *
 * Each adapter declares its sidebar features here. To add a new adapter
 * (e.g. Gemini), just add an entry to the `adapterPlugins` array —
 * no changes needed in Sidebar, routing, or other adapters.
 */
import claudeCodeIcon from "../assets/claude_code_logo.png";
import cursorIcon from "../assets/cursor_logo.png";
import windsurfIcon from "../assets/windsurf_logo.svg";
import geminiIcon from "../assets/gemini_logo.svg";
import claudeDesktopIcon from "../assets/claude_desktop_logo.svg";
import kiroIcon from "../assets/kiro_logo.svg";
import antigravityIcon from "../assets/antigravity_logo.jpg";
import vscodeIcon from "../assets/vs_code_logo.png";
import copilotIcon from "../assets/codex-icon.svg";
import codexIcon from "../assets/codex-icon.svg";

export interface AdapterFeature {
  /** Unique feature id within this adapter */
  id: string;
  /** Display label in sidebar */
  label: string;
  /** Emoji or icon character */
  icon: string;
  /** Route path — will be prefixed with /adapters/:adapterId/ */
  route: string;
}

export interface AdapterPlugin {
  /** Unique adapter id (e.g. "claude-code") */
  id: string;
  /** Display name (e.g. "Claude Code") */
  name: string;
  /** Emoji or icon character for the adapter header */
  icon: string;
  /** Path to SVG/PNG logo image (imported asset) */
  iconImg?: string;
  /** Accent color (hex) */
  color: string;
  /** Ordered list of features in the sidebar */
  features: AdapterFeature[];
  /** Whether the adapter section is expanded by default */
  defaultExpanded: boolean;
  /** If true, shown as a non-interactive "Coming soon" entry */
  comingSoon?: boolean;
  /** If false, this adapter is hidden by default (user can re-enable in Settings) */
  enabledByDefault?: boolean;
}

export const adapterPlugins: AdapterPlugin[] = [
  {
    id: "claude-code",
    name: "Claude Code",
    icon: "⟡",
    iconImg: claudeCodeIcon,
    color: "#9333ea",
    defaultExpanded: true,
    features: [
      { id: "instructions", label: "Instructions", icon: "📝", route: "/adapters/claude-code/instructions" },
      { id: "memory", label: "Memory", icon: "🧠", route: "/adapters/claude-code/memory" },
      { id: "permissions", label: "Permissions & Control", icon: "🔒", route: "/adapters/claude-code/permissions" },
      { id: "analytics-v2", label: "Analytics", icon: "📊", route: "/adapters/claude-code/analytics-v2" },
      { id: "prompts", label: "Prompt History", icon: "💬", route: "/adapters/claude-code/prompts" },
      { id: "transcripts", label: "Transcripts", icon: "📜", route: "/adapters/claude-code/transcripts" },
      { id: "plans", label: "Plans & Todos", icon: "📋", route: "/adapters/claude-code/plans" },
    ],
  },
  {
    id: "cursor",
    name: "Cursor",
    icon: "⊞",
    iconImg: cursorIcon,
    color: "#3b82f6",
    defaultExpanded: false,
    features: [
      { id: "global-config", label: "Global Config", icon: "⊕", route: "/adapters/cursor/global-config" },
      { id: "rules", label: "Rules", icon: "☰", route: "/adapters/cursor/rules" },
      { id: "permissions", label: "Permissions", icon: "🔒", route: "/adapters/cursor/permissions" },
      { id: "hooks", label: "Hooks", icon: "⚡", route: "/adapters/cursor/hooks" },
      { id: "plans", label: "Plans", icon: "📋", route: "/adapters/cursor/plans" },
      { id: "analytics-v2", label: "Analytics", icon: "📊", route: "/adapters/cursor/analytics-v2" },
      { id: "transcripts", label: "Transcripts", icon: "📜", route: "/adapters/cursor/transcripts" },
    ],
  },
  {
    id: "windsurf",
    name: "Windsurf",
    icon: "≋",
    iconImg: windsurfIcon,
    color: "#22c55e",
    defaultExpanded: false,
    features: [
      { id: "global-config", label: "Global Config", icon: "⊕", route: "/adapters/windsurf/global-config" },
      { id: "rules", label: "Rules", icon: "☰", route: "/adapters/windsurf/rules" },
    ],
  },
  {
    id: "gemini",
    name: "Gemini CLI",
    icon: "✦",
    iconImg: geminiIcon,
    color: "#4285f4",
    defaultExpanded: false,
    features: [
      { id: "analytics", label: "Analytics", icon: "📊", route: "/adapters/gemini/analytics" },
      { id: "global-config", label: "Global Config", icon: "⊕", route: "/adapters/gemini/global-config" },
      { id: "memory", label: "Memory", icon: "🧠", route: "/adapters/gemini/memory" },
      { id: "hooks", label: "Hooks", icon: "⚡", route: "/adapters/gemini/hooks" },
      { id: "skills", label: "Skills", icon: "✦", route: "/adapters/gemini/skills" },
      { id: "agents", label: "Agents", icon: "⊛", route: "/adapters/gemini/agents" },
      { id: "extensions", label: "Extensions", icon: "🧩", route: "/adapters/gemini/extensions" },
    ],
  },
  {
    id: "claude-desktop",
    name: "Claude Desktop",
    icon: "◈",
    iconImg: claudeDesktopIcon,
    color: "#d97706",
    defaultExpanded: false,
    features: [
      { id: "global-config", label: "Global Config", icon: "⊕", route: "/adapters/claude-desktop/global-config" },
    ],
  },
  {
    id: "copilot",
    name: "GitHub Copilot",
    icon: "⊛",
    iconImg: copilotIcon,
    color: "#1f6feb",
    defaultExpanded: false,
    features: [],
    enabledByDefault: false,
  },
  {
    id: "antigravity",
    name: "Antigravity",
    icon: "◎",
    iconImg: antigravityIcon,
    color: "#a855f7",
    defaultExpanded: false,
    features: [],
    enabledByDefault: false,
  },
  {
    id: "vscode",
    name: "VS Code",
    icon: "⌨",
    iconImg: vscodeIcon,
    color: "#007acc",
    defaultExpanded: false,
    features: [],
    enabledByDefault: false,
  },
  {
    id: "codex",
    name: "Codex",
    icon: "\u229B",
    iconImg: codexIcon,
    color: "#10a37f",
    defaultExpanded: false,
    features: [
      { id: "global-config", label: "Global Config", icon: "\u2295", route: "/adapters/codex/global-config" },
      { id: "skills", label: "Skills", icon: "\u2726", route: "/adapters/codex/skills" },
      { id: "analytics", label: "Analytics", icon: "\uD83D\uDCCA", route: "/adapters/codex/analytics" },
    ],
  },
  // ── Coming soon ──
  {
    id: "kiro",
    name: "Kiro",
    icon: "K",
    iconImg: kiroIcon,
    color: "#ff6b00",
    defaultExpanded: false,
    features: [],
    comingSoon: true,
  },
];

/** Look up an adapter plugin by id */
export function getAdapterPlugin(id: string): AdapterPlugin | undefined {
  return adapterPlugins.find((p) => p.id === id);
}

/** Get adapter icon image path by id (returns undefined if no image) */
export function getAdapterIconImg(id: string): string | undefined {
  return adapterPlugins.find((p) => p.id === id)?.iconImg;
}

/** Get adapter display name by id */
export function getAdapterName(id: string): string {
  return adapterPlugins.find((p) => p.id === id)?.name ?? id;
}

/** Get all adapter ids */
export function getAdapterIds(): string[] {
  return adapterPlugins.map((p) => p.id);
}

// ── Enabled adapters (persisted in localStorage) ─────────────────────────────

const ENABLED_ADAPTERS_KEY = "agentharbor-enabled-adapters";

/** Get the default enabled adapter IDs (everything except those marked enabledByDefault: false). */
function getDefaultEnabledAdapterIds(): string[] {
  return adapterPlugins.filter((p) => p.enabledByDefault !== false).map((p) => p.id);
}

/** Get the set of enabled adapter IDs from localStorage. Falls back to plugin defaults. */
export function getEnabledAdapterIds(): string[] {
  try {
    const stored = localStorage.getItem(ENABLED_ADAPTERS_KEY);
    if (stored) {
      const parsed = JSON.parse(stored) as string[];
      if (Array.isArray(parsed)) return parsed;
    }
  } catch { /* noop */ }
  return getDefaultEnabledAdapterIds();
}

/** Save the set of enabled adapter IDs to localStorage. */
export function setEnabledAdapterIds(ids: string[]): void {
  try {
    localStorage.setItem(ENABLED_ADAPTERS_KEY, JSON.stringify(ids));
  } catch { /* noop */ }
}

/** Get only the enabled adapter plugins. Coming-soon adapters are always included. */
export function getEnabledAdapterPlugins(): AdapterPlugin[] {
  const enabled = new Set(getEnabledAdapterIds());
  return adapterPlugins.filter((p) => p.comingSoon || enabled.has(p.id));
}
