import { useState, useEffect, useRef } from "react";
import { useNavigate } from "react-router-dom";

/**
 * A discoverability tip. `route` is optional: tips that name a concrete screen
 * become clickable and navigate there, the rest are informational.
 */
interface Tip {
  id: string;
  text: string;
  route?: string;
}

/**
 * Feature tips rotated in the header. Keep each one short enough to read at a
 * glance and phrased as something the user can *do*, not a feature name.
 */
const TIPS: Tip[] = [
  {
    id: "retention",
    text: "Keep your Claude Code chat history forever, instead of losing it after 30 days",
    route: "/adapters/claude-code/permissions#cleanup-period",
  },
  {
    id: "cost",
    text: "See what your usage would cost at pay-per-token API rates",
    route: "/adapters/claude-code/analytics-v2",
  },
  {
    id: "usage-chart",
    text: "Chart your token usage daily, weekly, or monthly",
    route: "/adapters/claude-code/analytics-v2",
  },
  {
    id: "transcripts",
    text: "Search every past conversation, grouped by project",
    route: "/adapters/claude-code/transcripts",
  },
  {
    id: "codex-projects",
    text: "Break down Codex sessions, tokens, and cost per project",
    route: "/adapters/codex/analytics",
  },
  {
    id: "deploy",
    text: "Deploy rules, MCPs, and skills to several agents at once",
  },
  {
    id: "prompts",
    text: "Re-run an old prompt straight from Prompt History",
    route: "/adapters/claude-code/prompts",
  },
  {
    id: "drift",
    text: "Spot config drift when a tool edits your config behind your back",
    route: "/projects",
  },
  {
    id: "memory",
    text: "Edit what each agent remembers about you, from one place",
    route: "/adapters/claude-code/memory",
  },
  {
    id: "presets",
    text: "Save a set of capabilities as a preset and reuse it later",
    route: "/presets",
  },
];

const ROTATE_MS = 8000;
const DISMISS_KEY = "agentharbor.headerTips.dismissed";

export function HeaderTips() {
  const navigate = useNavigate();
  const [index, setIndex] = useState(() => Math.floor(Math.random() * TIPS.length));
  const [visible, setVisible] = useState(true);
  const [dismissed, setDismissed] = useState(false);
  const paused = useRef(false);

  // Restore the dismissed preference. Wrapped because storage access throws in
  // some contexts (private windows, blocked site data).
  useEffect(() => {
    try {
      if (localStorage.getItem(DISMISS_KEY) === "1") setDismissed(true);
    } catch {
      /* keep tips visible if storage is unavailable */
    }
  }, []);

  // Rotate with a short fade so text never swaps abruptly mid-read.
  useEffect(() => {
    if (dismissed) return;
    const timer = setInterval(() => {
      if (paused.current) return;
      setVisible(false);
      setTimeout(() => {
        setIndex((i) => (i + 1) % TIPS.length);
        setVisible(true);
      }, 250);
    }, ROTATE_MS);
    return () => clearInterval(timer);
  }, [dismissed]);

  if (dismissed) return null;

  const tip = TIPS[index];
  const clickable = Boolean(tip.route);

  return (
    <div
      className="flex h-8 shrink-0 items-center gap-2.5 border-b border-border bg-app-card px-7"
      onMouseEnter={() => {
        paused.current = true;
      }}
      onMouseLeave={() => {
        paused.current = false;
      }}
    >
      <span className="shrink-0 text-[10px] font-semibold uppercase tracking-wider text-amber-400/70">
        Tip
      </span>
      <button
        type="button"
        disabled={!clickable}
        onClick={() => tip.route && navigate(tip.route)}
        title={clickable ? "Open this feature" : tip.text}
        className={`flex-1 min-w-0 truncate text-left text-[13px] font-medium tracking-[0.01em] text-amber-400 transition-opacity duration-200 ${
          visible ? "opacity-100" : "opacity-0"
        } ${clickable ? "hover:text-amber-300 hover:underline cursor-pointer" : "cursor-default"}`}
      >
        {tip.text}
      </button>
      <button
        type="button"
        aria-label="Hide tips"
        title="Hide tips"
        onClick={() => {
          setDismissed(true);
          try {
            localStorage.setItem(DISMISS_KEY, "1");
          } catch {
            /* dismissal simply will not persist */
          }
        }}
        className="shrink-0 px-1.5 text-sm leading-none text-text-secondary hover:text-text-primary"
      >
        ×
      </button>
    </div>
  );
}
