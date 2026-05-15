import { useState, useEffect, useCallback, useMemo, useRef, Fragment } from "react";
import {
  analyzeModelRouting,
  type ModelRoutingAnalysis,
  type ModelRoutingMessage,
} from "../lib/tauri";

const PERIOD_OPTIONS = [7, 30, 90];

interface Routed {
  target: "haiku" | "sonnet" | "keep";
  savings: number;
}

function modelIsOpus(model: string | null | undefined): boolean {
  return !!model && model.toLowerCase().includes("opus");
}

function shortModel(model: string | null | undefined): string {
  if (!model) return "unknown";
  const m = model.toLowerCase();
  if (m.includes("haiku")) return "Haiku";
  if (m.includes("sonnet")) return "Sonnet";
  if (m.includes("opus")) return "Opus";
  return model;
}

function classify(msg: ModelRoutingMessage, aggressiveness: number): Routed {
  // Slider 0..100 maps to two input-token thresholds.
  const haikuMaxInput = 1000 + (aggressiveness / 100) * 7000; // 1k → 8k
  const sonnetMaxInput = 8000 + (aggressiveness / 100) * 56_000; // 8k → 64k
  const maxTools = aggressiveness > 50 ? 3 : 1;

  const haikuEligible =
    !msg.has_thinking &&
    msg.input_tokens <= haikuMaxInput &&
    msg.tool_count <= maxTools;

  if (haikuEligible) {
    return { target: "haiku", savings: msg.current_cost - msg.haiku_cost };
  }

  const sonnetEligible =
    !msg.has_thinking &&
    msg.input_tokens <= sonnetMaxInput &&
    modelIsOpus(msg.model);

  if (sonnetEligible) {
    return { target: "sonnet", savings: msg.current_cost - msg.sonnet_cost };
  }

  return { target: "keep", savings: 0 };
}

function formatUsd(n: number): string {
  if (n === 0) return "$0.00";
  if (Math.abs(n) < 0.01) return `$${n.toFixed(4)}`;
  if (Math.abs(n) < 1) return `$${n.toFixed(3)}`;
  return `$${n.toFixed(2)}`;
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return n.toString();
}

function rowKey(msg: ModelRoutingMessage): string {
  // Timestamps in the backend are RFC3339 strings sourced per assistant message.
  // Project + tokens disambiguate the rare case of two messages at the same instant.
  return `${msg.timestamp}|${msg.project ?? ""}|${msg.input_tokens}|${msg.output_tokens}`;
}

export function OptimizePage() {
  const [analysis, setAnalysis] = useState<ModelRoutingAnalysis | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [periodDays, setPeriodDays] = useState<number>(30);
  const [aggressiveness, setAggressiveness] = useState<number>(25); // start conservative
  const [bucket, setBucket] = useState<"haiku" | "sonnet" | "keep">("haiku");

  const requestIdRef = useRef(0);

  const load = useCallback(
    async (days: number) => {
      const myRequest = ++requestIdRef.current;
      setLoading(true);
      setError(null);
      try {
        const result = await analyzeModelRouting(days);
        if (requestIdRef.current !== myRequest) return; // a newer request has been issued — drop this one
        setAnalysis(result);
      } catch (e) {
        if (requestIdRef.current !== myRequest) return;
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        if (requestIdRef.current === myRequest) setLoading(false);
      }
    },
    []
  );

  useEffect(() => {
    load(periodDays);
  }, [load, periodDays]);

  const stats = useMemo(() => {
    if (!analysis) return null;
    let totalSavings = 0;
    let haikuCount = 0;
    let sonnetCount = 0;
    let keepCount = 0;
    const routed: { msg: ModelRoutingMessage; routed: Routed }[] = [];

    for (const msg of analysis.messages) {
      const r = classify(msg, aggressiveness);
      totalSavings += r.savings;
      if (r.target === "haiku") haikuCount++;
      else if (r.target === "sonnet") sonnetCount++;
      else keepCount++;
      routed.push({ msg, routed: r });
    }

    const days = analysis.period_days || 1;
    const monthlyProjection = (totalSavings / days) * 30;
    const projectedNewSpend = analysis.total_current_cost - totalSavings;
    const reductionPct =
      analysis.total_current_cost > 0
        ? (totalSavings / analysis.total_current_cost) * 100
        : 0;

    return {
      totalSavings,
      monthlyProjection,
      projectedNewSpend,
      reductionPct,
      haikuCount,
      sonnetCount,
      keepCount,
      routed,
    };
  }, [analysis, aggressiveness]);

  return (
    <div className="h-full overflow-y-auto">
      <div className="max-w-5xl mx-auto p-6">
        <div className="flex items-start justify-between mb-6">
          <div>
            <h1 className="text-2xl font-semibold text-text-primary mb-1">
              Token Optimizer
            </h1>
            <p className="text-sm text-text-secondary">
              Model routing recommendations from your local Claude usage logs.
              Haiku turns are excluded (already cheapest) — all numbers below
              describe your routable, non-Haiku traffic. Heuristic-only — no API
              calls, no $ spent on this page.
            </p>
          </div>
          <div className="flex items-center gap-2">
            <div className="flex rounded-md overflow-hidden border border-border">
              {PERIOD_OPTIONS.map((d) => (
                <button
                  key={d}
                  onClick={() => setPeriodDays(d)}
                  className={`px-3 py-1.5 text-xs transition-colors ${
                    periodDays === d
                      ? "bg-accent-blue text-white"
                      : "bg-app-card text-text-secondary hover:text-text-primary"
                  }`}
                >
                  {d}d
                </button>
              ))}
            </div>
            <button
              onClick={() => load(periodDays)}
              disabled={loading}
              className="px-3 py-1.5 bg-app-card border border-border text-text-primary text-xs font-medium rounded-md disabled:opacity-50 hover:border-border-light"
            >
              {loading ? "Loading…" : "Refresh"}
            </button>
          </div>
        </div>

        {error && (
          <div className="mb-4 p-3 bg-accent-red-dim border border-accent-red/30 rounded-md text-sm text-accent-red">
            {error}
          </div>
        )}

        {loading && !analysis && (
          <div className="space-y-3">
            <div className="h-32 bg-app-card border border-border rounded-lg animate-pulse" />
            <div className="h-20 bg-app-card border border-border rounded-lg animate-pulse" />
            <div className="h-64 bg-app-card border border-border rounded-lg animate-pulse" />
          </div>
        )}

        {analysis && stats && (
          <>
            {/* Hero savings */}
            <div className="mb-6 p-6 bg-app-card border border-border rounded-lg">
              <div className="flex items-baseline justify-between flex-wrap gap-3">
                <div>
                  <div className="text-xs uppercase tracking-wider text-text-muted mb-1">
                    Estimated monthly savings
                  </div>
                  <div className="text-4xl font-semibold text-accent-green">
                    {formatUsd(stats.monthlyProjection)}
                  </div>
                  <div className="text-xs text-text-muted mt-1">
                    {stats.reductionPct.toFixed(0)}% reduction vs. routable
                    (non-Haiku) spend over the same window
                  </div>
                </div>
                <div className="text-right text-xs text-text-muted leading-5">
                  <div>
                    Routable spend ({analysis.period_days}d):{" "}
                    <span className="text-text-primary font-mono">
                      {formatUsd(analysis.total_current_cost)}
                    </span>
                  </div>
                  <div>
                    Projected (routable):{" "}
                    <span className="text-text-primary font-mono">
                      {formatUsd(stats.projectedNewSpend)}
                    </span>
                  </div>
                  <div>
                    Non-Haiku messages analyzed:{" "}
                    <span className="text-text-primary font-mono">
                      {analysis.total_messages.toLocaleString()}
                    </span>
                  </div>
                </div>
              </div>
            </div>

            {/* Slider */}
            <div className="mb-6 p-4 bg-app-card border border-border rounded-lg">
              <div className="flex items-center justify-between mb-3">
                <div>
                  <div className="text-sm font-medium text-text-primary">
                    Aggressiveness
                  </div>
                  <div className="text-xs text-text-muted">
                    Drag to see how thresholds change the savings curve.
                  </div>
                </div>
                <div className="text-xs font-mono text-text-secondary">
                  {aggressiveness < 35
                    ? "Conservative"
                    : aggressiveness > 65
                    ? "Aggressive"
                    : "Balanced"}{" "}
                  · {aggressiveness}
                </div>
              </div>
              <input
                type="range"
                min={0}
                max={100}
                step={1}
                value={aggressiveness}
                onChange={(e) => setAggressiveness(Number(e.target.value))}
                className="w-full accent-accent-blue"
              />
              <div className="flex justify-between text-[10px] text-text-muted mt-1 font-mono">
                <span>Haiku cutoff: ≤{formatTokens(1000 + (aggressiveness / 100) * 7000)} input</span>
                <span>Sonnet cutoff: ≤{formatTokens(8000 + (aggressiveness / 100) * 56000)} input</span>
                <span>Tools allowed: ≤{aggressiveness > 50 ? 3 : 1}</span>
              </div>
            </div>

            {/* Bucket distribution — click to filter the table below */}
            <div className="mb-6 grid grid-cols-3 gap-3">
              <BucketCard
                label="Route to Haiku"
                count={stats.haikuCount}
                total={analysis.messages.length}
                color="bg-accent-green"
                active={bucket === "haiku"}
                onClick={() => setBucket("haiku")}
              />
              <BucketCard
                label="Route to Sonnet"
                count={stats.sonnetCount}
                total={analysis.messages.length}
                color="bg-accent-blue"
                active={bucket === "sonnet"}
                onClick={() => setBucket("sonnet")}
              />
              <BucketCard
                label="Keep current"
                count={stats.keepCount}
                total={analysis.messages.length}
                color="bg-app-card-hover"
                active={bucket === "keep"}
                onClick={() => setBucket("keep")}
              />
            </div>

            {/* Bucket-filtered message table */}
            <BucketTable bucket={bucket} routed={stats.routed} />
          </>
        )}
      </div>
    </div>
  );
}

function BucketTable({
  bucket,
  routed,
}: {
  bucket: "haiku" | "sonnet" | "keep";
  routed: { msg: ModelRoutingMessage; routed: Routed }[];
}) {
  const [expandedKey, setExpandedKey] = useState<string | null>(null);
  const filtered = useMemo(
    () => routed.filter((r) => r.routed.target === bucket),
    [bucket, routed]
  );
  useEffect(() => {
    setExpandedKey(null);
  }, [bucket]);

  const sorted = useMemo(() => {
    if (bucket === "keep") {
      // For "keep" bucket, sort by current cost descending so the most expensive
      // un-routed prompts surface first — those are the ones a future heuristic might catch.
      return [...filtered].sort(
        (a, b) => b.msg.current_cost - a.msg.current_cost
      );
    }
    return [...filtered].sort((a, b) => b.routed.savings - a.routed.savings);
  }, [filtered, bucket]);

  // If the currently-expanded message dropped out of this bucket (e.g. user
  // moved the aggressiveness slider), collapse it so the detail panel can't
  // outlive its row.
  useEffect(() => {
    if (expandedKey && !sorted.some((r) => rowKey(r.msg) === expandedKey)) {
      setExpandedKey(null);
    }
  }, [sorted, expandedKey]);

  const labels: Record<typeof bucket, { title: string; hint: string }> = {
    haiku: {
      title: "Messages that would route to Haiku",
      hint: "Small input, no thinking, few tool calls. These run fine on Haiku at ~⅕ the cost.",
    },
    sonnet: {
      title: "Messages that would route to Sonnet",
      hint: "Mid-size Opus prompts without thinking — Sonnet handles these at ~⅓ the cost.",
    },
    keep: {
      title: "Messages staying on current model",
      hint: "Used thinking, long input, or many tools. Sorted by current spend so you can spot high-cost outliers.",
    },
  };

  if (filtered.length === 0) {
    return (
      <div className="text-center py-12 text-text-muted text-sm">
        No messages in this bucket at the current aggressiveness.
      </div>
    );
  }

  return (
    <div className="bg-app-card border border-border rounded-lg overflow-hidden">
      <div className="px-4 py-3 border-b border-border">
        <div className="text-sm font-medium text-text-primary">
          {labels[bucket].title}
        </div>
        <div className="text-xs text-text-muted mt-0.5">
          {labels[bucket].hint} · showing {Math.min(sorted.length, 50)} of{" "}
          {filtered.length}
        </div>
      </div>
      <div className="overflow-x-auto">
        <table className="w-full text-xs">
          <thead>
            <tr className="text-left text-text-muted border-b border-border">
              <th className="px-4 py-2 font-medium w-6"></th>
              <th className="px-4 py-2 font-medium">When</th>
              <th className="px-4 py-2 font-medium">Prompt</th>
              <th className="px-4 py-2 font-medium">Current</th>
              <th className="px-4 py-2 font-medium text-right">Input</th>
              <th className="px-4 py-2 font-medium">Signals</th>
              <th className="px-4 py-2 font-medium text-right">Cost now</th>
              <th className="px-4 py-2 font-medium text-right">
                {bucket === "keep" ? "Why kept" : "Saves"}
              </th>
            </tr>
          </thead>
          <tbody>
            {sorted.slice(0, 50).map((c) => {
              const key = rowKey(c.msg);
              const isOpen = expandedKey === key;
              return (
                <Fragment key={key}>
                  <tr
                    className="border-b border-border/50 hover:bg-app-card-hover/30 cursor-pointer"
                    onClick={() => setExpandedKey(isOpen ? null : key)}
                  >
                    <td className="px-2 py-2 text-text-muted text-center select-none">
                      {isOpen ? "▾" : "▸"}
                    </td>
                    <td className="px-4 py-2 text-text-muted font-mono whitespace-nowrap">
                      {new Date(c.msg.timestamp).toLocaleDateString()}{" "}
                      <span className="text-text-muted/60">
                        {new Date(c.msg.timestamp).toLocaleTimeString([], {
                          hour: "2-digit",
                          minute: "2-digit",
                        })}
                      </span>
                    </td>
                    <td className="px-4 py-2 text-text-secondary max-w-[360px]">
                      <div className="truncate">
                        {c.msg.prompt_preview ?? (
                          <span className="italic text-text-muted">
                            (no prompt text — likely a tool-result message)
                          </span>
                        )}
                      </div>
                    </td>
                    <td className="px-4 py-2 text-text-secondary">
                      {shortModel(c.msg.model)}
                    </td>
                    <td className="px-4 py-2 text-right font-mono text-text-secondary">
                      {formatTokens(c.msg.input_tokens)}
                    </td>
                    <td className="px-4 py-2">
                      <SignalChips msg={c.msg} />
                    </td>
                    <td className="px-4 py-2 text-right font-mono text-text-secondary">
                      {formatUsd(c.msg.current_cost)}
                    </td>
                    <td className="px-4 py-2 text-right font-mono">
                      {bucket === "keep" ? (
                        <span className="text-text-muted text-[10px]">
                          {keepReason(c.msg)}
                        </span>
                      ) : (
                        <span className="text-accent-green">
                          {formatUsd(c.routed.savings)}
                        </span>
                      )}
                    </td>
                  </tr>
                  {isOpen && (
                    <tr className="border-b border-border/50 bg-app-bg/40">
                      <td colSpan={8} className="px-6 py-3">
                        {bucket !== "keep" && (() => {
                          const r = routeRationale(c.msg, bucket);
                          return (
                            <div className="mb-4 p-3 bg-accent-green-dim border border-accent-green/30 rounded-md">
                              <div className="text-[10px] uppercase tracking-wider text-accent-green font-semibold mb-1">
                                Why route to {bucket === "haiku" ? "Haiku" : "Sonnet"}
                              </div>
                              <div className="text-sm text-text-primary mb-2">
                                {r.headline}
                              </div>
                              <ul className="text-xs text-text-secondary space-y-1 list-disc list-inside mb-2">
                                {r.reasons.map((reason, idx) => (
                                  <li key={idx}>{reason}</li>
                                ))}
                              </ul>
                              {r.caveats.length > 0 && (
                                <>
                                  <div className="text-[10px] uppercase tracking-wider text-accent-yellow font-semibold mb-1 mt-2">
                                    Caveats
                                  </div>
                                  <ul className="text-xs text-text-secondary space-y-1 list-disc list-inside">
                                    {r.caveats.map((cav, idx) => (
                                      <li key={idx}>{cav}</li>
                                    ))}
                                  </ul>
                                </>
                              )}
                            </div>
                          );
                        })()}
                        {bucket === "keep" && (
                          <div className="mb-4 p-3 bg-app-card-hover border border-border rounded-md">
                            <div className="text-[10px] uppercase tracking-wider text-text-muted font-semibold mb-1">
                              Why keep on {shortModel(c.msg.model)}
                            </div>
                            <div className="text-sm text-text-secondary">
                              {keepReason(c.msg)}. At the current aggressiveness
                              setting, this message doesn't fit the cheaper-model
                              profile — drag the slider right to be more
                              aggressive, but verify a few of these first.
                            </div>
                          </div>
                        )}

                        <div className="text-[10px] uppercase tracking-wider text-text-muted mb-1">
                          Prompt
                        </div>
                        <div className="text-sm text-text-primary whitespace-pre-wrap mb-3">
                          {c.msg.prompt_preview ?? (
                            <span className="italic text-text-muted">
                              No user-prompt text was recorded for this message
                              (likely a tool result or system continuation).
                            </span>
                          )}
                        </div>
                        <div className="grid grid-cols-2 md:grid-cols-4 gap-x-6 gap-y-1 text-[11px] font-mono text-text-muted">
                          <div>
                            Project:{" "}
                            <span className="text-text-secondary">
                              {c.msg.project ?? "—"}
                            </span>
                          </div>
                          <div>
                            Output tokens:{" "}
                            <span className="text-text-secondary">
                              {formatTokens(c.msg.output_tokens)}
                            </span>
                          </div>
                          <div>
                            Cache read:{" "}
                            <span className="text-text-secondary">
                              {formatTokens(c.msg.cache_read)}
                            </span>
                          </div>
                          <div>
                            Cache write:{" "}
                            <span className="text-text-secondary">
                              {formatTokens(c.msg.cache_write)}
                            </span>
                          </div>
                          <div>
                            Haiku cost:{" "}
                            <span className="text-text-secondary">
                              {formatUsd(c.msg.haiku_cost)}
                            </span>
                          </div>
                          <div>
                            Sonnet cost:{" "}
                            <span className="text-text-secondary">
                              {formatUsd(c.msg.sonnet_cost)}
                            </span>
                          </div>
                          <div>
                            Tools:{" "}
                            <span className="text-text-secondary">
                              {c.msg.tool_count}
                            </span>
                          </div>
                          <div>
                            Thinking:{" "}
                            <span className="text-text-secondary">
                              {c.msg.has_thinking ? "yes" : "no"}
                            </span>
                          </div>
                        </div>
                      </td>
                    </tr>
                  )}
                </Fragment>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}

function SignalChips({ msg }: { msg: ModelRoutingMessage }) {
  return (
    <div className="flex items-center gap-1 flex-wrap">
      {msg.has_thinking && (
        <span className="text-[9px] font-mono uppercase px-1.5 py-0.5 rounded bg-accent-purple-dim text-accent-purple">
          thinking
        </span>
      )}
      <span
        className={`text-[9px] font-mono uppercase px-1.5 py-0.5 rounded ${
          msg.tool_count === 0
            ? "bg-app-card-hover text-text-muted"
            : "bg-accent-orange-dim text-accent-orange"
        }`}
      >
        {msg.tool_count} tool{msg.tool_count === 1 ? "" : "s"}
      </span>
      {msg.cache_read > 0 && (
        <span className="text-[9px] font-mono uppercase px-1.5 py-0.5 rounded bg-accent-cyan-dim text-accent-cyan">
          cached
        </span>
      )}
    </div>
  );
}

interface RouteRationale {
  headline: string;
  reasons: string[];
  caveats: string[];
}

function routeRationale(
  msg: ModelRoutingMessage,
  target: "haiku" | "sonnet"
): RouteRationale {
  const reasons: string[] = [];
  const caveats: string[] = [];
  const totalTokens =
    msg.input_tokens + msg.cache_read + msg.cache_write;
  const inputLabel =
    msg.input_tokens < 1000
      ? "Very small input"
      : msg.input_tokens < 4000
      ? "Small input"
      : "Moderate input";

  reasons.push(
    `${inputLabel} (~${formatTokens(msg.input_tokens)} tokens). The model didn't need Opus's larger context to handle this.`
  );

  if (!msg.has_thinking) {
    reasons.push(
      "No extended thinking was used — Opus's reasoning advantage wasn't engaged here. The cheaper model would have produced the same output path."
    );
  }

  if (msg.tool_count === 0) {
    reasons.push(
      "Zero tool calls — this was a single-shot response. Opus's multi-step planning isn't being used."
    );
  } else if (msg.tool_count === 1) {
    reasons.push(
      "Only 1 tool call — straightforward task, no complex chaining."
    );
  } else if (msg.tool_count <= 3) {
    reasons.push(
      `${msg.tool_count} tool calls — light orchestration, well within Sonnet's capability.`
    );
  }

  if (msg.cache_read > 0 && msg.cache_read > msg.input_tokens) {
    reasons.push(
      `Heavy cache reuse (${formatTokens(msg.cache_read)} cached vs ${formatTokens(msg.input_tokens)} fresh input) — this looks like a repetitive task pattern; cheaper model is plenty.`
    );
  }

  if (msg.output_tokens > 4000) {
    caveats.push(
      `Output was ${formatTokens(msg.output_tokens)} tokens — verify the response quality didn't depend on Opus.`
    );
  }
  if (msg.input_tokens > 4000) {
    caveats.push(
      "Input is on the larger side — spot-check a couple of these to confirm Haiku handles them cleanly before bulk-switching."
    );
  }

  const savings =
    target === "haiku"
      ? msg.current_cost - msg.haiku_cost
      : msg.current_cost - msg.sonnet_cost;
  const targetName = target === "haiku" ? "Haiku" : "Sonnet";
  const factor = msg.current_cost > 0 ? msg.current_cost / Math.max(target === "haiku" ? msg.haiku_cost : msg.sonnet_cost, 1e-9) : 1;
  const headline = `Saves ${formatUsd(savings)} on this single message — ${shortModel(msg.model)} cost ${formatUsd(msg.current_cost)}, ${targetName} would cost ${formatUsd(target === "haiku" ? msg.haiku_cost : msg.sonnet_cost)} (${factor.toFixed(1)}× cheaper). Re-priced from the same ${formatTokens(totalTokens)} total tokens.`;

  return { headline, reasons, caveats };
}

function keepReason(msg: ModelRoutingMessage): string {
  const reasons: string[] = [];
  if (msg.has_thinking) reasons.push("thinking");
  if (msg.input_tokens > 64_000) reasons.push("very large input");
  else if (msg.input_tokens > 8000) reasons.push("large input");
  if (msg.tool_count > 3) reasons.push(`${msg.tool_count} tools`);
  if (reasons.length === 0) return "above slider threshold";
  return reasons.join(" + ");
}

function BucketCard({
  label,
  count,
  total,
  color,
  active,
  onClick,
}: {
  label: string;
  count: number;
  total: number;
  color: string;
  active: boolean;
  onClick: () => void;
}) {
  const pct = total > 0 ? (count / total) * 100 : 0;
  return (
    <button
      type="button"
      onClick={onClick}
      className={`text-left p-3 bg-app-card border rounded-lg transition-colors ${
        active
          ? "border-accent-blue ring-1 ring-accent-blue/40"
          : "border-border hover:border-border-light"
      }`}
    >
      <div className="text-xs text-text-muted mb-1">{label}</div>
      <div className="text-xl font-semibold text-text-primary">{count}</div>
      <div className="mt-2 h-1.5 bg-app-bg rounded-full overflow-hidden">
        <div className={`h-full ${color}`} style={{ width: `${pct}%` }} />
      </div>
      <div className="mt-1 text-[10px] font-mono text-text-muted">
        {pct.toFixed(0)}%
      </div>
    </button>
  );
}
