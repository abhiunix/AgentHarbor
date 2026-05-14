import { useState, useEffect, useCallback, useRef, useMemo } from "react";
import { open as openFileDialog } from "@tauri-apps/plugin-dialog";
import { readTextFile } from "@tauri-apps/plugin-fs";
import ReactDiffViewer, { DiffMethod } from "react-diff-viewer-continued";
import {
  readPlan,
  startDebate,
  cancelDebate,
  checkDebateCredentials,
  discoverProjectPlans,
  listDebates,
  getDebate,
  deleteDebate,
  deletePlanFile,
  listHiddenDebatePlans,
  hidePlanFromDebate,
  clearHiddenDebatePlans,
  type PlanEntry,
  type DebateCredentials,
  type DebateModel,
  type DebateToolCallRecord,
  type DebateSpeaker,
  type DebateTurnKind,
  type DebateSummary,
  type DebateRecord,
  type DebateTurn,
  type DebateRoundRecord,
  DEBATE_MODELS,
  DEFAULT_AUTHOR_MODEL_ID,
  DEFAULT_REVIEWER_MODEL_ID,
} from "../lib/tauri";
import {
  useDebateRunStore,
  type DebateRunState,
  type DebateResultState,
  type DebateTurnState,
  type DebateToolCallState,
} from "../stores/debateRunStore";
import { DebugPath } from "../components/common/DebugPath";
import { SecretsManager } from "../components/settings/SecretsManager";

// ──────────────────────────────────────────────────────────────────────────────
// Page
// ──────────────────────────────────────────────────────────────────────────────

export function DebatePage() {
  // Project — the directory whose plans we're debating. Persisted across
  // app launches in localStorage. When unset, no plans are auto-loaded and
  // the user gets a CTA to pick one.
  const [projectDir, setProjectDirState] = useState<string | null>(() => {
    try {
      return localStorage.getItem("debate.projectDir") || null;
    } catch {
      return null;
    }
  });
  const setProjectDir = useCallback((next: string | null) => {
    setProjectDirState(next);
    try {
      if (next) localStorage.setItem("debate.projectDir", next);
      else localStorage.removeItem("debate.projectDir");
    } catch {
      /* ignore — private mode, etc. */
    }
  }, []);

  // Plans + custom-loaded files (custom always prepended)
  const [plans, setPlans] = useState<PlanEntry[]>([]);
  const [loadingPlans, setLoadingPlans] = useState(false);
  const [plansError, setPlansError] = useState<string | null>(null);

  // Per-plan content cache (for expand + copy)
  const [planContent, setPlanContent] = useState<Record<string, string>>({});
  const [loadingContent, setLoadingContent] = useState<string | null>(null);
  const [expandedPlan, setExpandedPlan] = useState<string | null>(null);
  const [copiedKey, setCopiedKey] = useState<string | null>(null);

  // Credentials
  const [creds, setCreds] = useState<DebateCredentials | null>(null);
  const [secretsOpen, setSecretsOpen] = useState(false);

  // Configurator
  const defaultAuthorModel =
    DEBATE_MODELS.find((m) => m.id === DEFAULT_AUTHOR_MODEL_ID) ?? DEBATE_MODELS[0];
  const defaultReviewerModel =
    DEBATE_MODELS.find((m) => m.id === DEFAULT_REVIEWER_MODEL_ID) ??
    DEBATE_MODELS[DEBATE_MODELS.length - 1];
  const [configuratorFor, setConfiguratorFor] = useState<PlanEntry | null>(null);
  const [authorModel, setAuthorModel] = useState<DebateModel>(defaultAuthorModel);
  const [reviewerModel, setReviewerModel] = useState<DebateModel>(defaultReviewerModel);
  const [iterations, setIterations] = useState<string>("3");
  const [starting, setStarting] = useState(false);
  const [startError, setStartError] = useState<string | null>(null);

  // View state — sourced from the app-scoped Zustand store so debates keep
  // running (and the user can return to view live progress / cancel) when
  // they navigate away from this page mid-flight. Listeners are wired ONCE
  // at app startup via `initDebateRunListeners`; they write into the store.
  const view = useDebateRunStore((s) => s.view);
  const runState = useDebateRunStore((s) => s.runState);
  const resultState = useDebateRunStore((s) => s.resultState);
  const runError = useDebateRunStore((s) => s.runError);

  // Persisted debate history
  const [debates, setDebates] = useState<DebateSummary[]>([]);
  /** Which plan_path the user is currently viewing history for (null = closed). */
  const [historyPlanPath, setHistoryPlanPath] = useState<string | null>(null);
  /** Plan the user has asked to delete; pending confirmation. */
  const [deletePending, setDeletePending] = useState<PlanEntry | null>(null);
  /** Paths the user has hidden from this page (persisted in app data dir). */
  const [hiddenPlanPaths, setHiddenPlanPaths] = useState<Set<string>>(new Set());
  /** A fetched DebateRecord to display in the read-only detail view. */
  const [historyDetail, setHistoryDetail] = useState<DebateRecord | null>(null);

  const debatedPlanPaths = useMemo(
    () => new Set(debates.map((d) => d.plan_path).filter(Boolean)),
    [debates]
  );

  const refreshDebates = useCallback(async () => {
    try {
      const list = await listDebates();
      setDebates(list);
    } catch (e) {
      console.error("Failed to load debate history", e);
      setDebates([]);
    }
  }, []);

  const refreshHiddenPlans = useCallback(async () => {
    try {
      const paths = await listHiddenDebatePlans();
      setHiddenPlanPaths(new Set(paths));
    } catch (e) {
      console.error("Failed to load hidden-plan list", e);
      setHiddenPlanPaths(new Set());
    }
  }, []);

  const refreshCreds = useCallback(async () => {
    try {
      const c = await checkDebateCredentials();
      setCreds(c);
    } catch (e) {
      console.error("Failed to check debate credentials", e);
      setCreds({ anthropic: false, openai: false });
    }
  }, []);

  /** Load plans for the currently-selected project. Plans from
   * `<projectDir>/.claude/plans/` get `source: "claude"`, ones from
   * `<projectDir>/.cursor/plans/` get `source: "cursor"`. Device-picked
   * customs (source === "custom") in current state are preserved across
   * project switches so users don't lose ad-hoc loads. */
  const loadPlanList = useCallback(async () => {
    if (!projectDir) {
      // No project selected — only show whatever custom (device-picked) plans
      // are in state.
      setPlans((prev) => prev.filter((p) => p.source === "custom"));
      setLoadingPlans(false);
      return;
    }
    setLoadingPlans(true);
    setPlansError(null);
    try {
      const discovered = await discoverProjectPlans(projectDir);
      const projectEntries: PlanEntry[] = discovered.map((d) => ({
        name: d.name,
        source: d.source === "claude" ? "claude" : "cursor",
        file_path: d.file_path,
        overview: "",
        modified_at: d.modified_at,
      }));
      setPlans((prev) => {
        const customs = prev.filter((p) => p.source === "custom");
        return [...customs, ...projectEntries];
      });
    } catch (e) {
      setPlansError(String(e));
    } finally {
      setLoadingPlans(false);
    }
  }, [projectDir]);

  const pickProject = useCallback(async () => {
    try {
      const selected = await openFileDialog({ multiple: false, directory: true });
      if (!selected || typeof selected !== "string") return;
      setProjectDir(selected);
      setPlansError(null);
    } catch (e) {
      console.error("Failed to pick project folder", e);
    }
  }, [setProjectDir]);

  const clearProject = useCallback(() => {
    setProjectDir(null);
    setPlansError(null);
  }, [setProjectDir]);

  useEffect(() => {
    refreshCreds();
    refreshDebates();
    refreshHiddenPlans();
  }, [refreshCreds, refreshDebates, refreshHiddenPlans]);

  // Re-load whenever the project changes.
  useEffect(() => {
    loadPlanList();
  }, [loadPlanList]);

  // ── Plan list actions ─────────────────────────────────────────────────────

  const handleExpand = useCallback(
    async (plan: PlanEntry) => {
      const key = plan.file_path;
      if (expandedPlan === key) {
        setExpandedPlan(null);
        return;
      }
      setExpandedPlan(key);
      if (!planContent[key]) {
        setLoadingContent(key);
        try {
          const content = await readPlan(plan.file_path);
          setPlanContent((prev) => ({ ...prev, [key]: content }));
        } catch (e) {
          setPlanContent((prev) => ({
            ...prev,
            [key]: `Error loading plan: ${e}`,
          }));
        } finally {
          setLoadingContent(null);
        }
      }
    },
    [expandedPlan, planContent]
  );

  const handleCopy = useCallback(
    async (plan: PlanEntry) => {
      const key = plan.file_path;
      try {
        let content = planContent[key];
        if (!content) {
          content =
            plan.source === "custom"
              ? await readTextFile(plan.file_path)
              : await readPlan(plan.file_path);
          setPlanContent((prev) => ({ ...prev, [key]: content }));
        }
        await navigator.clipboard.writeText(content);
        setCopiedKey(key);
        window.setTimeout(() => {
          setCopiedKey((curr) => (curr === key ? null : curr));
        }, 1500);
      } catch (e) {
        console.error("Failed to copy plan", e);
      }
    },
    [planContent]
  );

  const handleChooseFromDevice = useCallback(async () => {
    try {
      const selected = await openFileDialog({
        multiple: false,
        directory: false,
        filters: [{ name: "Markdown", extensions: ["md"] }],
      });
      if (!selected || typeof selected !== "string") return;
      const absPath = selected;
      const baseName = absPath.split(/[\\/]/).pop() ?? absPath;
      const content = await readTextFile(absPath);
      const entry: PlanEntry = {
        name: baseName,
        source: "custom",
        file_path: absPath,
        overview: "",
        modified_at: new Date().toISOString(),
      };
      setPlanContent((prev) => ({ ...prev, [absPath]: content }));
      setPlans((prev) => {
        // De-dupe if already in list
        const filtered = prev.filter((p) => p.file_path !== absPath);
        return [entry, ...filtered];
      });
    } catch (e) {
      console.error("Failed to load file from device", e);
    }
  }, []);

  /** Project-folder scan. macOS Finder hides dotfolders in the file picker,
   * so we let the user pick a regular folder, then the backend walks
   * `.claude/plans/` and `.cursor/plans/` for `.md` files. */
  /** Open the configurator modal IMMEDIATELY (synchronous state update),
   * and load the plan content in the background. Previously this awaited
   * `readPlan` first, which made the button feel unresponsive on the first
   * click (the modal didn't appear until the IPC round-trip finished).
   * The Configurator now disables its Start button while content is still
   * loading, so the user can't fire `handleStartDebate` before it's ready. */
  const openConfigurator = useCallback(
    (plan: PlanEntry) => {
      setStartError(null);
      setAuthorModel(defaultAuthorModel);
      setReviewerModel(defaultReviewerModel);
      setIterations("3");
      setConfiguratorFor(plan);

      const key = plan.file_path;
      if (planContent[key] !== undefined) return; // already cached
      setLoadingContent(key);
      void (async () => {
        try {
          const content =
            plan.source === "custom"
              ? await readTextFile(plan.file_path)
              : await readPlan(plan.file_path);
          setPlanContent((prev) => ({ ...prev, [key]: content }));
        } catch (e) {
          console.error("Failed to load plan content for configurator", e);
          setStartError(`Failed to load plan content: ${e}`);
        } finally {
          setLoadingContent(null);
        }
      })();
    },
    [planContent, defaultAuthorModel, defaultReviewerModel]
  );

  // ── Start debate ──────────────────────────────────────────────────────────

  const handleStartDebate = useCallback(async () => {
    if (!configuratorFor) return;
    const plan = configuratorFor;
    const content = planContent[plan.file_path];
    if (content === undefined) return;

    const trimmed = iterations.trim();
    if (trimmed === "") {
      setStartError("Enter an iteration count between 1 and 10.");
      return;
    }
    const parsed = Number(trimmed);
    if (
      !Number.isFinite(parsed) ||
      !Number.isInteger(parsed) ||
      parsed < 1 ||
      parsed > 10
    ) {
      setStartError("Iterations must be a whole number between 1 and 10.");
      return;
    }

    setStarting(true);
    setStartError(null);
    try {
      // Only pass project_dir for plans that actually belong to the selected
      // project. Device-picked customs may live outside it.
      const planProjectDir =
        projectDir && plan.file_path.startsWith(projectDir + "/")
          ? projectDir
          : null;
      const debateId = await startDebate({
        planContent: content,
        planPath: plan.file_path,
        projectDir: planProjectDir,
        authorProvider: authorModel.provider,
        authorModel: authorModel.id,
        reviewerProvider: reviewerModel.provider,
        reviewerModel: reviewerModel.id,
        maxRounds: parsed,
      });
      // Commit the run to the store SYNCHRONOUSLY before React renders so the
      // very first turn_start event can't be filtered out by a stale matcher.
      // `startRun` atomically sets currentDebateId + runState + view: "running".
      useDebateRunStore.getState().startRun({
        debateId,
        maxRounds: parsed,
        currentTurn: 0,
        turns: [],
        planPath: plan.file_path,
        planContent: content,
        authorModel,
        reviewerModel,
      });
      setConfiguratorFor(null);
    } catch (e) {
      const msg = String(e);
      if (msg.includes("missing_credentials")) {
        await refreshCreds();
        setStartError(
          "Required API keys are missing. Add them in the Secrets Manager."
        );
      } else {
        setStartError(msg);
      }
    } finally {
      setStarting(false);
    }
  }, [configuratorFor, planContent, authorModel, reviewerModel, iterations, projectDir, refreshCreds]);

  // ── Listen to debate events ───────────────────────────────────────────────

  // When the store transitions into "result" view (i.e. a debate just
  // completed), re-sync our local view of the persisted artefacts: the
  // history list picks up the new record, and the plan list picks up the
  // backend-written `_v<N>.md` sibling. The listener that produced the
  // `view === "result"` transition lives at app scope (initDebateRunListeners),
  // so this page-local effect is what replaces the inline refresh calls that
  // used to live inside the `debate:complete` handler.
  useEffect(() => {
    if (view === "result") {
      loadPlanList();
      refreshDebates();
    }
  }, [view, loadPlanList, refreshDebates]);

  // ── Cancel ────────────────────────────────────────────────────────────────

  const handleCancelDebate = useCallback(async () => {
    if (!runState) return;
    const debateId = runState.debateId;
    // Optimistically return to the list — the backend cancel may take a moment
    // (the worker only polls between SSE lines), so don't make the user wait.
    // `cancelRun` wipes currentDebateId synchronously so any in-flight events
    // for this debate are filtered out by the store's matcher.
    useDebateRunStore.getState().cancelRun();
    try {
      await cancelDebate(debateId);
    } catch (e) {
      console.error("Failed to cancel debate", e);
    }
  }, [runState]);

  // ── Result actions ────────────────────────────────────────────────────────

  const handleDiscard = useCallback(() => {
    useDebateRunStore.getState().discard();
    // Refresh plan list (file mtimes may have changed)
    loadPlanList();
  }, [loadPlanList]);

  // ── History panel handlers ────────────────────────────────────────────────

  const openHistory = useCallback((planPath: string) => {
    setHistoryDetail(null);
    setHistoryPlanPath(planPath);
  }, []);

  const closeHistory = useCallback(() => {
    setHistoryDetail(null);
    setHistoryPlanPath(null);
  }, []);

  const viewDebateDetail = useCallback(async (id: string) => {
    try {
      const rec = await getDebate(id);
      setHistoryDetail(rec);
    } catch (e) {
      console.error("Failed to load debate detail", e);
    }
  }, []);

  const handleDeleteDebate = useCallback(
    async (id: string) => {
      try {
        await deleteDebate(id);
        await refreshDebates();
        if (historyDetail?.id === id) setHistoryDetail(null);
      } catch (e) {
        console.error("Failed to delete debate", e);
      }
    },
    [historyDetail, refreshDebates]
  );

  const handleDeletePlanRequest = useCallback((plan: PlanEntry) => {
    setDeletePending(plan);
  }, []);

  const removePlanFromLocalState = useCallback((filePath: string) => {
    setPlans((prev) => prev.filter((p) => p.file_path !== filePath));
    setPlanContent((prev) => {
      const { [filePath]: _, ...rest } = prev;
      return rest;
    });
    setExpandedPlan((curr) => (curr === filePath ? null : curr));
  }, []);

  /** "Delete from disk" — physically removes the .md file. */
  const handleConfirmDeleteFromDisk = useCallback(async () => {
    const plan = deletePending;
    if (!plan) return;
    try {
      await deletePlanFile(plan.file_path);
      removePlanFromLocalState(plan.file_path);
      // Re-fetch registry plans so the Claude/Cursor groups stay in sync if
      // the deleted file was registry-managed. Customs (device/scan) live
      // only in component state, so the local splice above is enough for them.
      await loadPlanList();
    } catch (e) {
      console.error("Failed to delete plan", e);
      setPlansError(`Failed to delete: ${e}`);
    } finally {
      setDeletePending(null);
    }
  }, [deletePending, loadPlanList, removePlanFromLocalState]);

  /** "Delete from debate page" — keeps the file on disk, just hides from
   * this view. Persistent across app restarts. */
  const handleConfirmHideFromPage = useCallback(async () => {
    const plan = deletePending;
    if (!plan) return;
    try {
      await hidePlanFromDebate(plan.file_path);
      setHiddenPlanPaths((prev) => {
        const next = new Set(prev);
        next.add(plan.file_path);
        return next;
      });
      removePlanFromLocalState(plan.file_path);
    } catch (e) {
      console.error("Failed to hide plan", e);
      setPlansError(`Failed to hide: ${e}`);
    } finally {
      setDeletePending(null);
    }
  }, [deletePending, removePlanFromLocalState]);

  /** "Show all" — wipes the persistent hidden-plans list so previously
   * removed-from-Debate-page plans reappear on the next render. The
   * project's plans are re-listed so any new files also show up. */
  const handleClearHidden = useCallback(async () => {
    try {
      await clearHiddenDebatePlans();
      setHiddenPlanPaths(new Set());
      await loadPlanList();
    } catch (e) {
      console.error("Failed to clear hidden plans", e);
      setPlansError(`Failed to clear hidden plans: ${e}`);
    }
  }, [loadPlanList]);

  // ── Banner / credential state ─────────────────────────────────────────────

  const credsReady = creds?.anthropic === true && creds?.openai === true;
  const selectionCredsReady = (() => {
    if (!creds) return false;
    const needAnthropic =
      authorModel.provider === "anthropic" || reviewerModel.provider === "anthropic";
    const needOpenAI =
      authorModel.provider === "openai" || reviewerModel.provider === "openai";
    return (!needAnthropic || creds.anthropic) && (!needOpenAI || creds.openai);
  })();

  return (
    <div className="h-full flex flex-col">
      <div className="px-6 pt-6 pb-4">
        <div className="flex items-center justify-between flex-wrap gap-3 mb-4">
          <div>
            <h1 className="text-2xl font-semibold text-text-primary mb-1">AI Debate</h1>
            <DebugPath path="~/.claude/plans/ · ~/.cursor/plans/" className="text-sm" />
          </div>
          <CredentialsBar
            creds={creds}
            onOpenSecrets={() => setSecretsOpen(true)}
          />
        </div>

        {creds && !credsReady && (
          <MissingCredsBanner
            creds={creds}
            onOpenSecrets={() => setSecretsOpen(true)}
          />
        )}
      </div>

      {view === "list" && (
        <PlanListView
          plans={plans.filter((p) => !hiddenPlanPaths.has(p.file_path))}
          loading={loadingPlans}
          error={plansError}
          expandedPlan={expandedPlan}
          planContent={planContent}
          loadingContent={loadingContent}
          copiedKey={copiedKey}
          credsReady={!!credsReady}
          debatedPlanPaths={debatedPlanPaths}
          debates={debates}
          projectDir={projectDir}
          hiddenCount={hiddenPlanPaths.size}
          onPickProject={pickProject}
          onClearProject={clearProject}
          onClearHidden={handleClearHidden}
          onExpand={handleExpand}
          onCopy={handleCopy}
          onDebate={openConfigurator}
          onShowHistory={openHistory}
          onDelete={handleDeletePlanRequest}
          onChooseFromDevice={handleChooseFromDevice}
        />
      )}

      {view === "running" && runState && (
        <DebateRunner
          state={runState}
          onCancel={handleCancelDebate}
          error={runError}
          onDismissError={() => {
            // Wipe both the run and the error banner, returning to the list.
            // `cancelRun` already clears runError + view; calling setRunError
            // afterward is harmless but explicit.
            useDebateRunStore.getState().cancelRun();
            useDebateRunStore.getState().setRunError(null);
          }}
        />
      )}

      {view === "result" && resultState && (
        <ResultView state={resultState} onDiscard={handleDiscard} />
      )}

      {configuratorFor && (
        <Configurator
          plan={configuratorFor}
          authorModel={authorModel}
          reviewerModel={reviewerModel}
          iterations={iterations}
          starting={starting}
          contentLoading={loadingContent === configuratorFor.file_path}
          error={startError}
          credsReady={selectionCredsReady}
          onAuthorModel={setAuthorModel}
          onReviewerModel={setReviewerModel}
          onIterations={setIterations}
          onClose={() => setConfiguratorFor(null)}
          onStart={handleStartDebate}
          onOpenSecrets={() => setSecretsOpen(true)}
        />
      )}

      <SecretsManager
        isOpen={secretsOpen}
        onClose={() => {
          setSecretsOpen(false);
          refreshCreds();
        }}
      />

      {historyPlanPath !== null && historyDetail === null && (
        <HistoryPanel
          planPath={historyPlanPath}
          debates={debates.filter((d) => d.plan_path === historyPlanPath)}
          onClose={closeHistory}
          onView={viewDebateDetail}
          onDelete={handleDeleteDebate}
        />
      )}

      {historyDetail && (
        <DebateDetailView
          record={historyDetail}
          onBack={() => setHistoryDetail(null)}
          onClose={closeHistory}
        />
      )}

      {deletePending && (
        <DeletePlanDialog
          plan={deletePending}
          onCancel={() => setDeletePending(null)}
          onHideFromPage={handleConfirmHideFromPage}
          onDeleteFromDisk={handleConfirmDeleteFromDisk}
        />
      )}
    </div>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Credentials UI
// ──────────────────────────────────────────────────────────────────────────────

function CredentialsBar({
  creds,
  onOpenSecrets,
}: {
  creds: DebateCredentials | null;
  onOpenSecrets: () => void;
}) {
  if (!creds) return null;
  const allGood = creds.anthropic && creds.openai;
  if (!allGood) {
    return (
      <button
        type="button"
        onClick={onOpenSecrets}
        className="text-xs text-text-secondary underline hover:text-text-primary transition-colors"
      >
        Open Secrets Manager
      </button>
    );
  }
  return (
    <div className="flex items-center gap-2">
      <Bubble label="ANTHROPIC_API_KEY" ok={creds.anthropic} />
      <Bubble label="OPENAI_API_KEY" ok={creds.openai} />
    </div>
  );
}

function Bubble({ label, ok }: { label: string; ok: boolean }) {
  return (
    <span
      className={`inline-flex items-center gap-1 px-2 py-0.5 text-[10px] font-medium rounded-full border ${
        ok
          ? "bg-green-500/15 border-green-500/40 text-green-400"
          : "bg-red-500/15 border-red-500/40 text-red-400"
      }`}
    >
      <span aria-hidden>✓</span>
      <span className="font-mono">{label}</span>
    </span>
  );
}

function MissingCredsBanner({
  creds,
  onOpenSecrets,
}: {
  creds: DebateCredentials;
  onOpenSecrets: () => void;
}) {
  const missing: string[] = [];
  if (!creds.anthropic) missing.push("ANTHROPIC_API_KEY");
  if (!creds.openai) missing.push("OPENAI_API_KEY");

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onOpenSecrets}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onOpenSecrets();
        }
      }}
      className="cursor-pointer flex items-start gap-3 px-4 py-3 bg-amber-500/10 border border-amber-500/30 rounded-lg hover:bg-amber-500/15 transition-colors"
    >
      <div className="flex-1 text-sm text-amber-200">
        To run a debate, add the missing API key in Secrets —{" "}
        {missing.map((key, i) => (
          <span key={key}>
            <strong className="font-mono text-amber-100">{key}</strong>
            {i < missing.length - 1 ? " and/or " : ""}
          </span>
        ))}
        .
      </div>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onOpenSecrets();
        }}
        className="flex-shrink-0 px-3 py-1.5 text-xs font-medium rounded bg-amber-500/20 border border-amber-500/40 text-amber-100 hover:bg-amber-500/30 transition-colors"
      >
        Open Secrets Manager
      </button>
    </div>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Plan List View
// ──────────────────────────────────────────────────────────────────────────────

type PlanGroupKey = "claude" | "cursor" | "custom";

function PlanListView({
  plans,
  loading,
  error,
  expandedPlan,
  planContent,
  loadingContent,
  copiedKey,
  credsReady,
  debatedPlanPaths,
  debates,
  projectDir,
  hiddenCount,
  onPickProject,
  onClearProject,
  onClearHidden,
  onExpand,
  onCopy,
  onDebate,
  onShowHistory,
  onDelete,
  onChooseFromDevice,
}: {
  plans: PlanEntry[];
  loading: boolean;
  error: string | null;
  expandedPlan: string | null;
  planContent: Record<string, string>;
  loadingContent: string | null;
  copiedKey: string | null;
  credsReady: boolean;
  debatedPlanPaths: Set<string>;
  /** All persisted debate summaries — used to slice per-plan history. */
  debates: DebateSummary[];
  projectDir: string | null;
  /** Number of plans currently in the persistent hidden-from-Debate list. */
  hiddenCount: number;
  onPickProject: () => void;
  onClearProject: () => void;
  onClearHidden: () => void;
  onExpand: (p: PlanEntry) => void;
  onCopy: (p: PlanEntry) => void;
  onDebate: (p: PlanEntry) => void;
  onShowHistory: (planPath: string) => void;
  onDelete: (p: PlanEntry) => void;
  onChooseFromDevice: () => void;
}) {
  const [groupsExpanded, setGroupsExpanded] = useState<
    Record<PlanGroupKey, boolean>
  >({ claude: true, cursor: true, custom: true });

  const toggleGroup = (key: PlanGroupKey) => {
    setGroupsExpanded((prev) => ({ ...prev, [key]: !prev[key] }));
  };

  const claudePlans = plans.filter((p) => p.source === "claude");
  const cursorPlans = plans.filter((p) => p.source === "cursor");
  const customPlans = plans.filter(
    (p) => p.source !== "claude" && p.source !== "cursor"
  );

  const groups: { key: PlanGroupKey; label: string; items: PlanEntry[] }[] = [
    { key: "claude", label: ".claude/plans", items: claudePlans },
    { key: "cursor", label: ".cursor/plans", items: cursorPlans },
    { key: "custom", label: "Custom (device-picked)", items: customPlans },
  ];

  // Empty-state: no project selected and no custom plans loaded.
  if (!projectDir && customPlans.length === 0 && !loading) {
    return (
      <div className="flex-1 overflow-y-auto px-6 pb-6 space-y-3">
        <div className="bg-app-card border border-border rounded-lg p-8 flex flex-col items-center text-center gap-3">
          <p className="text-base font-medium text-text-primary">
            Pick a project to debate its plans
          </p>
          <p className="text-sm text-text-secondary max-w-md">
            AgentHarbor will list any plans inside the project's{" "}
            <code className="text-text-primary">.claude/plans/</code> and{" "}
            <code className="text-text-primary">.cursor/plans/</code> folders.
            Models in the debate get the project path in their context, so they
            can ground critiques in real code rather than guesses.
          </p>
          <div className="flex items-center gap-2 mt-1">
            <button
              type="button"
              onClick={onPickProject}
              className="px-4 py-2 text-sm font-medium rounded bg-accent-blue text-white hover:bg-accent-blue/90 transition-colors"
            >
              Pick a project folder
            </button>
            <button
              type="button"
              onClick={onChooseFromDevice}
              className="px-3 py-2 text-sm rounded border border-border text-text-secondary hover:text-text-primary"
            >
              Or load a single .md
            </button>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto px-6 pb-6 space-y-3">
      <div className="bg-app-card border border-border rounded-lg px-4 py-3 flex items-center justify-between gap-3 flex-wrap">
        <div className="min-w-0">
          <p className="text-[11px] uppercase tracking-wide text-text-secondary">
            Project
          </p>
          {projectDir ? (
            <p className="text-sm font-mono text-text-primary truncate">
              {projectDir}
            </p>
          ) : (
            <p className="text-sm text-text-secondary">
              No project selected — only ad-hoc plans below.
            </p>
          )}
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={onPickProject}
            className="px-3 py-1.5 text-xs font-medium rounded bg-app-bg border border-border text-text-primary hover:bg-app-card-hover transition-colors"
          >
            {projectDir ? "Change project" : "Pick project"}
          </button>
          {projectDir && (
            <button
              type="button"
              onClick={onClearProject}
              className="px-3 py-1.5 text-xs rounded border border-border text-text-secondary hover:text-text-primary"
            >
              Clear
            </button>
          )}
          <button
            type="button"
            onClick={onChooseFromDevice}
            className="px-3 py-1.5 text-xs font-medium rounded bg-app-bg border border-border text-text-primary hover:bg-app-card-hover transition-colors"
            title="Load a single .md file from anywhere — bypasses the project"
          >
            Load .md
          </button>
        </div>
      </div>
      <div className="flex items-center gap-2 flex-wrap">
        <p className="text-xs text-text-secondary">
          {plans.length} plan{plans.length === 1 ? "" : "s"} available
        </p>
        {hiddenCount > 0 && (
          <>
            <span className="text-xs text-text-secondary">·</span>
            <span
              className="text-xs px-1.5 py-0.5 rounded bg-amber-500/10 border border-amber-500/30 text-amber-300"
              title="Plans you removed from this page via the delete dialog. The files are still on disk."
            >
              {hiddenCount} hidden
            </span>
            <button
              type="button"
              onClick={onClearHidden}
              className="text-xs px-2 py-0.5 rounded border border-border text-text-secondary hover:text-text-primary"
              title="Restore all hidden plans to the list"
            >
              Show all
            </button>
          </>
        )}
      </div>

      {loading ? (
        <p className="text-text-secondary text-sm">Loading plans…</p>
      ) : error ? (
        <div className="px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400">
          {error}
        </div>
      ) : (
        groups.map((group) => {
          const isOpen = groupsExpanded[group.key];
          return (
            <div
              key={group.key}
              className="bg-app-card border border-border rounded-lg overflow-hidden"
            >
              <div
                className="w-full px-4 py-3 flex items-center gap-3 hover:bg-app-card-hover transition-colors cursor-pointer"
                onClick={() => toggleGroup(group.key)}
                role="button"
                tabIndex={0}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    toggleGroup(group.key);
                  }
                }}
              >
                <span className="text-text-secondary text-sm flex-shrink-0">
                  {isOpen ? "▼" : "▶"}
                </span>
                <span className="text-sm font-semibold text-text-primary">
                  {group.label}
                </span>
                <span className="text-xs text-text-secondary">
                  ({group.items.length})
                </span>
              </div>
              {isOpen && (
                <div className="border-t border-border p-2 space-y-2">
                  {group.items.length === 0 ? (
                    <p className="text-text-secondary text-sm px-2 py-1">
                      No plans in this group.
                    </p>
                  ) : (
                    group.items.map((plan) => (
                      <PlanRow
                        key={plan.file_path}
                        plan={plan}
                        isExpanded={expandedPlan === plan.file_path}
                        content={planContent[plan.file_path]}
                        isLoadingContent={loadingContent === plan.file_path}
                        isCopied={copiedKey === plan.file_path}
                        credsReady={credsReady}
                        hasHistory={debatedPlanPaths.has(plan.file_path)}
                        planDebates={debates
                          .filter((d) => d.plan_path === plan.file_path)
                          .slice()
                          .sort((a, b) =>
                            a.created_at.localeCompare(b.created_at)
                          )}
                        onExpand={() => onExpand(plan)}
                        onCopy={() => onCopy(plan)}
                        onDebate={() => onDebate(plan)}
                        onShowHistory={() => onShowHistory(plan.file_path)}
                        onDelete={() => onDelete(plan)}
                      />
                    ))
                  )}
                </div>
              )}
            </div>
          );
        })
      )}
    </div>
  );
}

function PlanRow({
  plan,
  isExpanded,
  content,
  isLoadingContent,
  isCopied,
  credsReady,
  hasHistory,
  planDebates,
  onExpand,
  onCopy,
  onDebate,
  onShowHistory,
  onDelete,
}: {
  plan: PlanEntry;
  isExpanded: boolean;
  content: string | undefined;
  isLoadingContent: boolean;
  isCopied: boolean;
  credsReady: boolean;
  hasHistory: boolean;
  /** Persisted debates for THIS plan, oldest-first so the chronological
   * `debate_v1`, `debate_v2`, … numbering is stable. */
  planDebates: DebateSummary[];
  onExpand: () => void;
  onCopy: () => void;
  onDebate: () => void;
  onShowHistory: () => void;
  onDelete: () => void;
}) {
  // The original-content panel is collapsed by default — saves scroll real
  // estate when the user just wants to see the per-plan debate history.
  const [originalOpen, setOriginalOpen] = useState(false);
  return (
    <div className="bg-app-card border border-border rounded-lg overflow-hidden">
      <div
        className="w-full px-4 py-3 flex items-center gap-3 hover:bg-app-card-hover transition-colors cursor-pointer"
        onClick={onExpand}
        role="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            onExpand();
          }
        }}
      >
        <span className="text-text-secondary text-sm flex-shrink-0">
          {isExpanded ? "▼" : "▶"}
        </span>
        <div className="flex-1 min-w-0 flex items-center gap-2">
          <h3 className="text-sm font-medium text-text-primary truncate">
            {plan.name}
          </h3>
          {hasHistory && (
            <button
              type="button"
              onClick={(e) => {
                e.stopPropagation();
                onShowHistory();
              }}
              title="View AI debate history for this plan"
              className="bg-green-500/15 border border-green-500/40 text-green-300 text-[10px] px-1.5 py-0.5 rounded font-medium hover:bg-green-500/25 transition-colors flex-shrink-0"
            >
              Debated
            </button>
          )}
        </div>
        <div className="relative flex-shrink-0">
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              onCopy();
            }}
            title="Copy plan content"
            className="px-2 py-1 text-text-secondary hover:text-text-primary rounded hover:bg-app-card-hover transition-colors"
          >
            <span aria-hidden>⧉</span>
          </button>
          {isCopied && (
            <span className="absolute -top-7 left-1/2 -translate-x-1/2 px-2 py-0.5 text-[10px] rounded bg-green-500/20 border border-green-500/40 text-green-400 whitespace-nowrap">
              Copied
            </span>
          )}
        </div>
        <span className="text-xs text-text-secondary flex-shrink-0">
          {formatDate(plan.modified_at)}
        </span>
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onDebate();
          }}
          disabled={!credsReady}
          className={`flex-shrink-0 px-3 py-1.5 text-xs font-medium rounded transition-colors ${
            credsReady
              ? "bg-accent-blue text-white hover:bg-accent-blue/90"
              : "bg-app-card border border-border text-text-secondary cursor-not-allowed"
          }`}
          title={credsReady ? "Start AI debate" : "Add API keys to enable"}
        >
          AI Debate
        </button>
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation();
            onDelete();
          }}
          title="Delete this plan file"
          className="flex-shrink-0 px-3 py-1.5 text-xs font-medium rounded border border-red-500/40 text-red-300 hover:bg-red-500/10 transition-colors"
        >
          Delete
        </button>
      </div>

      {isExpanded && (
        <div className="border-t border-border">
          <div
            className="px-4 py-2 flex items-center gap-2 hover:bg-app-card-hover transition-colors cursor-pointer"
            onClick={() => setOriginalOpen((v) => !v)}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                setOriginalOpen((v) => !v);
              }
            }}
          >
            <span className="text-text-secondary text-xs flex-shrink-0">
              {originalOpen ? "▼" : "▶"}
            </span>
            <span className="text-[11px] uppercase tracking-wide text-text-secondary">
              Original content
            </span>
          </div>
          {originalOpen && (
            <div className="px-4 py-3 border-t border-border">
              {isLoadingContent ? (
                <p className="text-text-secondary text-sm">Loading content...</p>
              ) : content ? (
                <pre className="text-xs text-text-primary font-mono whitespace-pre-wrap break-words max-h-96 overflow-y-auto">
                  {content}
                </pre>
              ) : (
                <p className="text-text-secondary text-sm">No content.</p>
              )}
            </div>
          )}
          <div className="border-t border-border px-4 py-3 space-y-2">
            <p className="text-[10px] uppercase tracking-wide text-text-secondary">
              AI Debates ({planDebates.length})
            </p>
            {planDebates.length === 0 ? (
              <p className="text-xs text-text-secondary">
                No AI debates yet. Click <span className="text-text-primary">AI Debate</span> to start one — the rounds will persist here.
              </p>
            ) : (
              <div className="space-y-2">
                {planDebates.map((d, i) => (
                  <PlanDebateBlock key={d.id} versionIndex={i} summary={d} />
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

/** One persisted debate, collapsible. Header shows `debate_v<N>` + date,
 * total cost, and APPROVED / ROUND LIMIT badge. Expanding fetches the full
 * record once and renders each turn through `TurnCard` (or, for legacy pre-v2
 * records, `LegacyRoundCard`) so users see exactly the same transcript they
 * saw during streaming. */
function PlanDebateBlock({
  versionIndex,
  summary,
}: {
  versionIndex: number;
  summary: DebateSummary;
}) {
  const [open, setOpen] = useState(false);
  const [record, setRecord] = useState<DebateRecord | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleToggle = useCallback(async () => {
    const next = !open;
    setOpen(next);
    if (next && !record && !loading) {
      setLoading(true);
      setError(null);
      try {
        const full = await getDebate(summary.id);
        setRecord(full);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    }
  }, [open, record, loading, summary.id]);

  const label = `debate_v${versionIndex + 1}`;

  return (
    <div className="bg-app-bg border border-border rounded-md overflow-hidden">
      <button
        type="button"
        onClick={handleToggle}
        className="w-full px-3 py-2 flex items-center gap-3 text-left hover:bg-app-card-hover transition-colors"
      >
        <span className="text-text-secondary text-xs flex-shrink-0">
          {open ? "▼" : "▶"}
        </span>
        <span className="text-sm font-mono text-text-primary flex-shrink-0">
          {label}
        </span>
        <span className="flex-1 text-xs text-text-secondary truncate">
          {formatDate(summary.created_at)} · {summaryCountLabel(summary)} ·{" "}
          {modelDisplayName(summary.author_model)} vs{" "}
          {modelDisplayName(summary.reviewer_model)}
        </span>
        <span className="text-[10px] px-1.5 py-0.5 rounded bg-app-card border border-border text-text-secondary font-mono flex-shrink-0">
          {formatUSD(summary.cost_total_usd)}
        </span>
        {summary.approved ? (
          <span className="text-[10px] px-1.5 py-0.5 rounded bg-green-500/15 border border-green-500/40 text-green-400 font-medium flex-shrink-0">
            APPROVED
          </span>
        ) : (
          <span className="text-[10px] px-1.5 py-0.5 rounded bg-amber-500/15 border border-amber-500/40 text-amber-300 font-medium flex-shrink-0">
            ROUND LIMIT
          </span>
        )}
      </button>
      {open && (
        <div className="border-t border-border px-3 py-3 space-y-3">
          {(summary.author_input_tokens > 0 ||
            summary.author_output_tokens > 0 ||
            summary.reviewer_input_tokens > 0 ||
            summary.reviewer_output_tokens > 0) && (
            <div className="bg-app-card border border-border rounded p-2">
              <TokenBreakdown
                authorIn={summary.author_input_tokens}
                authorOut={summary.author_output_tokens}
                reviewerIn={summary.reviewer_input_tokens}
                reviewerOut={summary.reviewer_output_tokens}
                costAuthorUsd={summary.cost_author_usd}
                costReviewerUsd={summary.cost_reviewer_usd}
                costTotalUsd={summary.cost_total_usd}
              />
            </div>
          )}
          {loading && (
            <p className="text-xs text-text-secondary">Loading turns...</p>
          )}
          {error && (
            <p className="text-xs text-red-400">Failed to load: {error}</p>
          )}
          {record && <RecordTranscript record={record} />}
        </div>
      )}
    </div>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Configurator
// ──────────────────────────────────────────────────────────────────────────────

function Configurator({
  plan,
  authorModel,
  reviewerModel,
  iterations,
  starting,
  contentLoading,
  error,
  credsReady,
  onAuthorModel,
  onReviewerModel,
  onIterations,
  onClose,
  onStart,
  onOpenSecrets,
}: {
  plan: PlanEntry;
  authorModel: DebateModel;
  reviewerModel: DebateModel;
  iterations: string;
  starting: boolean;
  /** True while we're reading the plan file via IPC. Gates the Start button
   * so a click before the read finishes can't no-op silently. */
  contentLoading: boolean;
  error: string | null;
  credsReady: boolean;
  onAuthorModel: (m: DebateModel) => void;
  onReviewerModel: (m: DebateModel) => void;
  onIterations: (s: string) => void;
  onClose: () => void;
  onStart: () => void;
  onOpenSecrets: () => void;
}) {
  const anthropicOptions = DEBATE_MODELS.filter((m) => m.provider === "anthropic");
  const openaiOptions = DEBATE_MODELS.filter((m) => m.provider === "openai");

  const renderSelect = (
    current: DebateModel,
    onChange: (m: DebateModel) => void
  ) => (
    <select
      value={current.id}
      onChange={(e) => {
        const next = DEBATE_MODELS.find((m) => m.id === e.target.value);
        if (next) onChange(next);
      }}
      className="w-full px-3 py-2 text-sm bg-app-bg border border-border rounded text-text-primary focus:outline-none focus:border-accent-blue"
    >
      <optgroup label="Anthropic">
        {anthropicOptions.map((m) => (
          <option key={m.id} value={m.id}>
            {m.label}
          </option>
        ))}
      </optgroup>
      <optgroup label="OpenAI">
        {openaiOptions.map((m) => (
          <option key={m.id} value={m.id}>
            {m.label}
          </option>
        ))}
      </optgroup>
    </select>
  );

  return (
    <>
      <div
        className="fixed inset-0 bg-black/40 z-[60]"
        onClick={onClose}
        aria-hidden
      />
      <div
        className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 z-[61] w-full max-w-md rounded-xl border border-border bg-app-card shadow-2xl"
        role="dialog"
        aria-modal="true"
      >
        <div className="p-6 space-y-4">
          <div>
            <h2 className="text-lg font-semibold text-text-primary mb-1">
              Configure AI Debate
            </h2>
            <p className="text-xs text-text-secondary font-mono truncate">{plan.name}</p>
          </div>

          <div>
            <label className="block text-xs font-medium text-text-secondary mb-2">
              Author model
            </label>
            {renderSelect(authorModel, onAuthorModel)}
          </div>

          <div>
            <label className="block text-xs font-medium text-text-secondary mb-2">
              Reviewer model
            </label>
            {renderSelect(reviewerModel, onReviewerModel)}
            {authorModel.id === reviewerModel.id && (
              <p className="text-[11px] text-text-secondary mt-2">
                Both sides will use{" "}
                <span className="font-medium text-text-primary">{authorModel.label}</span>{" "}
                — the same model debating itself.
              </p>
            )}
          </div>

          <div>
            <label className="block text-xs font-medium text-text-secondary mb-2">
              Iterations (1–10)
            </label>
            <input
              type="number"
              min={1}
              max={10}
              value={iterations}
              onChange={(e) => onIterations(e.target.value)}
              className="w-24 px-3 py-1.5 text-sm bg-app-bg border border-border rounded text-text-primary focus:outline-none focus:border-accent-blue"
            />
          </div>

          {error && (
            <div className="px-3 py-2 bg-red-500/10 border border-red-500/30 rounded text-xs text-red-400 space-y-1">
              <div>{error}</div>
              {error.includes("missing") && (
                <button
                  type="button"
                  onClick={onOpenSecrets}
                  className="underline text-amber-300 hover:text-amber-200"
                >
                  Open Secrets Manager
                </button>
              )}
            </div>
          )}

          {!credsReady && !error && (
            <div className="px-3 py-2 bg-amber-500/10 border border-amber-500/30 rounded text-xs text-amber-200">
              Both API keys are required.{" "}
              <button
                type="button"
                onClick={onOpenSecrets}
                className="underline hover:text-amber-100"
              >
                Open Secrets Manager
              </button>
            </div>
          )}

          <div className="flex justify-end gap-3 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="h-9 px-4 rounded-md bg-app-bg border border-border text-text-primary hover:bg-app-card-hover transition-colors text-sm"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={onStart}
              disabled={!credsReady || starting || contentLoading}
              className={`h-9 px-4 rounded-md text-sm font-medium transition-colors ${
                !credsReady || starting || contentLoading
                  ? "bg-accent-blue/50 text-white/70 cursor-not-allowed"
                  : "bg-accent-blue text-white hover:bg-accent-blue/90"
              }`}
            >
              {contentLoading
                ? "Loading plan…"
                : starting
                ? "Starting…"
                : "Start AI Debate"}
            </button>
          </div>
        </div>
      </div>
    </>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Debate Runner
// ──────────────────────────────────────────────────────────────────────────────

function DebateRunner({
  state,
  onCancel,
  error,
  onDismissError,
}: {
  state: DebateRunState;
  onCancel: () => void;
  error: string | null;
  onDismissError: () => void;
}) {
  // Sum tokens by speaker across all turns.
  const authorIn = state.turns.reduce(
    (s, t) => (t.speaker === "author" ? s + (t.inputTokens ?? 0) : s),
    0
  );
  const authorOut = state.turns.reduce(
    (s, t) => (t.speaker === "author" ? s + (t.outputTokens ?? 0) : s),
    0
  );
  const reviewerIn = state.turns.reduce(
    (s, t) => (t.speaker === "reviewer" ? s + (t.inputTokens ?? 0) : s),
    0
  );
  const reviewerOut = state.turns.reduce(
    (s, t) => (t.speaker === "reviewer" ? s + (t.outputTokens ?? 0) : s),
    0
  );
  const costAuthor = cost_for_client(
    state.authorModel.id,
    authorIn,
    authorOut
  );
  const costReviewer = cost_for_client(
    state.reviewerModel.id,
    reviewerIn,
    reviewerOut
  );
  const hasAnyTokens =
    authorIn > 0 || authorOut > 0 || reviewerIn > 0 || reviewerOut > 0;

  return (
    <div className="flex-1 overflow-y-auto px-6 pb-6 space-y-4">
      <div className="bg-app-card border border-border rounded-lg p-4 space-y-3">
        <div className="flex items-center justify-between gap-3">
          <div>
            <p className="text-sm font-medium text-text-primary">
              Turn {state.currentTurn || 0}
            </p>
            <p className="text-xs text-text-secondary">
              Author: <span className="text-text-primary">{state.authorModel.label}</span>
              {"  ·  "}Reviewer:{" "}
              <span className="text-text-primary">{state.reviewerModel.label}</span>
            </p>
            {hasAnyTokens && (
              <div className="mt-2">
                <TokenBreakdown
                  authorIn={authorIn}
                  authorOut={authorOut}
                  reviewerIn={reviewerIn}
                  reviewerOut={reviewerOut}
                  costAuthorUsd={costAuthor}
                  costReviewerUsd={costReviewer}
                  costTotalUsd={costAuthor + costReviewer}
                />
              </div>
            )}
          </div>
          <button
            type="button"
            onClick={onCancel}
            className="px-3 py-1.5 text-xs font-medium rounded bg-red-500/15 border border-red-500/40 text-red-300 hover:bg-red-500/25 transition-colors"
          >
            Cancel
          </button>
        </div>
      </div>

      {error && (
        <div className="px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400 flex items-center justify-between gap-3">
          <span>AI Debate error: {error}</span>
          <button
            type="button"
            onClick={onDismissError}
            className="px-2 py-1 text-xs rounded border border-red-500/40 hover:bg-red-500/20"
          >
            Back to list
          </button>
        </div>
      )}

      <div className="space-y-3">
        {state.turns.map((turn, idx) => {
          const isNewest = idx === state.turns.length - 1;
          return (
            <TurnCard
              key={turn.index}
              turn={turn}
              defaultOpen={isNewest}
            />
          );
        })}
        {state.turns.length === 0 && !error && (
          <p className="text-text-secondary text-sm">Waiting for first turn…</p>
        )}
      </div>
    </div>
  );
}

function kindLabel(kind: DebateTurnKind): string {
  switch (kind) {
    case "opening":
      return "Opening";
    case "critique":
      return "Critique";
    case "response":
      return "Response";
    case "finalize":
      return "Finalize";
  }
}

function speakerLabel(speaker: DebateSpeaker): string {
  return speaker === "reviewer" ? "Reviewer" : "Author";
}

/** One turn in the v2 turn-based debate engine. Header shows the position +
 * speaker + kind; body renders a typed view (opening / critique / response /
 * finalize) based on the parsed payload, falling back to raw text while the
 * turn is still streaming or when the model output failed to parse. */
function TurnCard({
  turn,
  defaultOpen,
}: {
  turn: DebateTurnState;
  defaultOpen: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const [inspect, setInspect] = useState(false);
  const bodyRef = useRef<HTMLDivElement | null>(null);

  // Auto-collapse prior turns, auto-expand newest as it arrives.
  useEffect(() => {
    setOpen(defaultOpen);
  }, [defaultOpen]);

  // Auto-scroll to bottom while streaming.
  useEffect(() => {
    if (!open || turn.complete) return;
    const el = bodyRef.current;
    if (el) {
      el.scrollTop = el.scrollHeight;
    }
  }, [turn.text, open, turn.complete]);

  const header = `Turn ${turn.index} · ${speakerLabel(turn.speaker)} (${kindLabel(turn.kind)}) · ${modelDisplayName(turn.model)}`;

  // Distinct green border for the finalize turn — flags it as the saved version.
  const finalized = turn.complete && turn.kind === "finalize";
  const cardBorder = finalized ? "border-green-500/40" : "border-border";

  // Critique verdict influences the header badge when present.
  const critiqueVerdict =
    turn.complete && turn.kind === "critique" && turn.parsed?.kind === "critique"
      ? turn.parsed.verdict
      : null;

  return (
    <div className={`bg-app-card border ${cardBorder} rounded-lg overflow-hidden`}>
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="w-full px-4 py-3 flex items-center gap-3 text-left hover:bg-app-card-hover transition-colors"
      >
        <span className="text-text-secondary text-sm flex-shrink-0">
          {open ? "▼" : "▶"}
        </span>
        <span className="flex-1 text-sm font-medium text-text-primary truncate">
          {header}
        </span>
        {!turn.complete && (
          <span className="text-[10px] px-1.5 py-0.5 rounded bg-blue-500/15 border border-blue-500/30 text-blue-300">
            streaming
          </span>
        )}
        {turn.complete &&
          (turn.inputTokens !== undefined || turn.outputTokens !== undefined) && (
            <span
              className="text-[10px] px-1.5 py-0.5 rounded bg-app-bg border border-border text-text-secondary font-mono"
              title="Tokens used by this turn (input → output)"
            >
              {(turn.inputTokens ?? 0).toLocaleString()}↑ /{" "}
              {(turn.outputTokens ?? 0).toLocaleString()}↓
            </span>
          )}
        {critiqueVerdict && <CritiqueVerdictBadge verdict={critiqueVerdict} />}
        {open && (
          <span
            role="button"
            tabIndex={0}
            onClick={(e) => {
              e.stopPropagation();
              setInspect((v) => !v);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.stopPropagation();
                e.preventDefault();
                setInspect((v) => !v);
              }
            }}
            title="Show the system + user prompts and full tool responses for this turn"
            className={`text-[10px] px-1.5 py-0.5 rounded border font-medium cursor-pointer ${
              inspect
                ? "bg-accent-blue/20 border-accent-blue/50 text-accent-blue"
                : "bg-app-bg border-border text-text-secondary hover:text-text-primary"
            }`}
          >
            Inspect
          </span>
        )}
      </button>
      {open && (
        <div className="border-t border-border">
          {turn.toolCalls.length > 0 && (
            <ToolCallList calls={turn.toolCalls} />
          )}
          {inspect && <InspectPanel turn={turn} />}
          <div
            ref={bodyRef}
            className="text-xs text-text-primary max-h-[28rem] overflow-y-auto px-4 py-3 space-y-2"
          >
            {/* Parse-error banner — flag fallback rendering. */}
            {turn.complete && turn.parsed === null && turn.parseError && (
              <div className="px-3 py-2 bg-amber-500/10 border border-amber-500/30 rounded text-[11px] text-amber-200 leading-snug">
                {speakerLabel(turn.speaker)} failed to produce structured output:{" "}
                <span className="font-mono text-amber-100">{turn.parseError}</span>
              </div>
            )}
            <TurnBody turn={turn} />
          </div>
        </div>
      )}
    </div>
  );
}

/** Render the body of a turn. While streaming or when parsing failed, falls
 * back to a raw text view. Once a `parsed` payload is available, renders the
 * typed sections for that kind. */
function TurnBody({ turn }: { turn: DebateTurnState }) {
  // While streaming, or when parsing failed entirely, show raw text.
  if (!turn.complete || turn.parsed === null) {
    if (turn.text) {
      return (
        <div className="space-y-0.5 font-mono">
          {turn.text.split("\n").map((line, i) => (
            <div
              key={i}
              className="whitespace-pre-wrap break-words leading-relaxed"
            >
              {line || " "}
            </div>
          ))}
        </div>
      );
    }
    return (
      <div className="text-text-secondary font-mono">
        {turn.complete ? "(empty)" : "..."}
      </div>
    );
  }

  const parsed = turn.parsed;
  switch (parsed.kind) {
    case "opening":
      return <PreBlock content={parsed.plan} />;
    case "critique":
      return (
        <div className="space-y-3">
          <Section label="Issues">
            {parsed.issues.length === 0 ? (
              <p className="text-text-secondary">(none)</p>
            ) : (
              <ol className="space-y-1 list-decimal pl-5">
                {parsed.issues.map((issue, i) => (
                  <li key={i} className="whitespace-pre-wrap break-words">
                    {issue}
                  </li>
                ))}
              </ol>
            )}
          </Section>
          <div className="text-[11px] flex items-center gap-2">
            <span className="text-text-secondary">Verdict:</span>
            <CritiqueVerdictBadge verdict={parsed.verdict} />
          </div>
        </div>
      );
    case "response":
      return (
        <div className="space-y-3">
          <Section label="Accepted">
            {parsed.accepted.length === 0 ? (
              <p className="text-text-secondary">(none)</p>
            ) : (
              <ul className="space-y-1 list-disc pl-5">
                {parsed.accepted.map((item, i) => (
                  <li key={i} className="whitespace-pre-wrap break-words">
                    {item}
                  </li>
                ))}
              </ul>
            )}
          </Section>
          <Section label="Rejected">
            {parsed.rebutted.length === 0 ? (
              <p className="text-text-secondary">(none)</p>
            ) : (
              <ul className="space-y-1 list-disc pl-5">
                {parsed.rebutted.map((item, i) => (
                  <li key={i} className="whitespace-pre-wrap break-words">
                    {item}
                  </li>
                ))}
              </ul>
            )}
          </Section>
          <Section label="Refined plan">
            {parsed.refined_plan ? (
              <PreBlock content={parsed.refined_plan} />
            ) : (
              <p className="text-text-secondary">(none)</p>
            )}
          </Section>
        </div>
      );
    case "finalize":
      return (
        <div className="space-y-3">
          <Section label="Final plan">
            <PreBlock content={parsed.plan} />
          </Section>
          <Section label="Caveats">
            {parsed.caveats.length === 0 ? (
              <p className="text-text-secondary">(none)</p>
            ) : (
              <ul className="space-y-1 list-disc pl-5">
                {parsed.caveats.map((c, i) => (
                  <li key={i} className="whitespace-pre-wrap break-words">
                    {c}
                  </li>
                ))}
              </ul>
            )}
          </Section>
        </div>
      );
  }
}

function Section({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div>
      <p className="text-[10px] uppercase tracking-wide text-text-secondary mb-1">
        {label}
      </p>
      <div className="text-xs text-text-primary leading-relaxed">{children}</div>
    </div>
  );
}

function PreBlock({ content }: { content: string }) {
  if (!content) {
    return <p className="text-text-secondary">(empty)</p>;
  }
  return (
    <pre className="whitespace-pre-wrap break-words text-xs text-text-primary font-mono leading-relaxed">
      {content}
    </pre>
  );
}

/** Compact tool-call summary. Shows a single line "Ran command tool1, tool2"
 * with adjacent same-tool runs collapsed (e.g. cat, cat, grep → cat ×2, grep).
 * The verbose input/output payloads are intentionally hidden so the user sees
 * the debate text, not the model's scratch work. */
function ToolCallList({ calls }: { calls: DebateToolCallState[] }) {
  const anyError = calls.some((c) => c.isError);
  // Collapse consecutive runs of the same tool: [cat, cat, grep, cat] →
  // "cat ×2, grep, cat".
  const compact: string[] = [];
  let i = 0;
  while (i < calls.length) {
    let j = i + 1;
    while (j < calls.length && calls[j].tool === calls[i].tool) j++;
    const count = j - i;
    compact.push(count > 1 ? `${calls[i].tool} ×${count}` : calls[i].tool);
    i = j;
  }
  return (
    <div className="px-4 py-2 border-b border-border bg-app-bg/60">
      <p
        className={`text-[11px] font-mono leading-snug ${
          anyError ? "text-red-300" : "text-text-secondary"
        }`}
      >
        <span className="text-text-primary">Ran command</span> {compact.join(", ")}
      </p>
    </div>
  );
}

/** Debug view for a round: shows the exact system + user prompts that were
 * sent to the model, and the full tool call exchanges (input + output) that
 * were fed back into the conversation between turns. Toggle via "Inspect" in
 * the round header. */
function InspectPanel({ turn }: { turn: DebateTurnState }) {
  return (
    <div className="px-4 py-3 border-b border-border bg-app-bg/40 space-y-3">
      <InspectBlock
        label="System prompt"
        content={turn.systemPrompt || "(not recorded)"}
      />
      <InspectBlock
        label="User prompt (first turn)"
        content={turn.userPrompt || "(not recorded)"}
      />
      {turn.toolCalls.length > 0 && (
        <div>
          <p className="text-[10px] uppercase tracking-wide text-text-secondary mb-1">
            Tool exchanges fed back to the model
          </p>
          <div className="space-y-2">
            {turn.toolCalls.map((c, i) => (
              <div
                key={i}
                className="rounded border border-border bg-app-card overflow-hidden"
              >
                <div
                  className={`px-3 py-1.5 text-[11px] font-mono border-b border-border ${
                    c.isError ? "text-red-300" : "text-text-primary"
                  }`}
                >
                  → {c.tool}({c.inputPreview})
                </div>
                <pre
                  className={`px-3 py-1.5 text-[11px] font-mono whitespace-pre-wrap break-words max-h-48 overflow-y-auto ${
                    c.isError ? "text-red-300/80" : "text-text-secondary"
                  }`}
                >
                  {c.outputPreview || "(empty)"}
                </pre>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function InspectBlock({ label, content }: { label: string; content: string }) {
  const [copied, setCopied] = useState(false);
  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(content);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      /* ignore */
    }
  };
  return (
    <div>
      <div className="flex items-center justify-between mb-1">
        <p className="text-[10px] uppercase tracking-wide text-text-secondary">
          {label}
        </p>
        <button
          type="button"
          onClick={handleCopy}
          className="text-[10px] px-1.5 py-0.5 rounded border border-border text-text-secondary hover:text-text-primary"
        >
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      <pre className="text-[11px] text-text-primary font-mono whitespace-pre-wrap break-words max-h-48 overflow-y-auto px-3 py-2 rounded bg-app-card border border-border">
        {content}
      </pre>
    </div>
  );
}

/** Render a persisted debate's transcript. Prefers the v2 `turns` array;
 * falls back to legacy `rounds` only when `turns` is empty (or absent). */
function RecordTranscript({ record }: { record: DebateRecord }) {
  if (record.turns && record.turns.length > 0) {
    return (
      <div className="space-y-3">
        {record.turns.map((t) => (
          <TurnCard
            key={t.index}
            turn={turnRecordToState(t)}
            defaultOpen={false}
          />
        ))}
      </div>
    );
  }
  // ── Legacy code path: pre-v2 debates persisted with `rounds` ────────────
  const rounds = record.rounds ?? [];
  if (rounds.length === 0) {
    return (
      <p className="text-xs text-text-secondary">(transcript not recorded)</p>
    );
  }
  return (
    <div className="space-y-3">
      {rounds.map((r) => (
        <LegacyRoundCard key={`${r.round}-${r.role}`} round={r} />
      ))}
    </div>
  );
}

/** Convert a persisted [`DebateTurn`] into the `DebateTurnState` shape
 * [`TurnCard`] consumes. Marks the turn complete and surfaces the persisted
 * `parsed` payload verbatim. */
function turnRecordToState(t: DebateTurn): DebateTurnState {
  return {
    index: t.index,
    speaker: t.speaker,
    kind: t.kind,
    model: t.model,
    text: t.raw_text,
    complete: true,
    parsed: t.parsed,
    parseError: t.parse_error,
    inputTokens: t.input_tokens,
    outputTokens: t.output_tokens,
    toolCalls: recordToolCallsForUi(t.tool_calls),
    systemPrompt: t.system_prompt,
    userPrompt: t.user_prompt,
  };
}

/** Minimal renderer for one persisted legacy round. Pre-v2 records don't have
 * structured `parsed` payloads — we show the raw `full_text` and a verdict tag
 * if one exists. */
function LegacyRoundCard({ round }: { round: DebateRoundRecord }) {
  const [open, setOpen] = useState(false);
  const isApproved = round.verdict === "APPROVED";
  const isRevise = round.verdict === "REVISE";
  const header = `Round ${round.round} · ${
    round.role === "reviewer" ? "Reviewer" : "Author"
  } (${modelDisplayName(round.model)})`;
  const tools = recordToolCallsForUi(round.tool_calls);
  return (
    <div className="bg-app-card border border-border rounded-lg overflow-hidden">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="w-full px-4 py-3 flex items-center gap-3 text-left hover:bg-app-card-hover transition-colors"
      >
        <span className="text-text-secondary text-sm flex-shrink-0">
          {open ? "▼" : "▶"}
        </span>
        <span className="flex-1 text-sm font-medium text-text-primary truncate">
          {header}
        </span>
        <span
          className="text-[10px] px-1.5 py-0.5 rounded bg-app-bg border border-border text-text-secondary font-mono"
          title="Tokens (input → output)"
        >
          {round.input_tokens.toLocaleString()}↑ /{" "}
          {round.output_tokens.toLocaleString()}↓
        </span>
        {isApproved && (
          <span className="text-[10px] px-1.5 py-0.5 rounded bg-green-500/15 border border-green-500/40 text-green-400 font-medium">
            APPROVED
          </span>
        )}
        {isRevise && (
          <span className="text-[10px] px-1.5 py-0.5 rounded bg-amber-500/15 border border-amber-500/40 text-amber-300 font-medium">
            REVISE
          </span>
        )}
      </button>
      {open && (
        <div className="border-t border-border">
          {tools.length > 0 && <ToolCallList calls={tools} />}
          <div className="text-xs text-text-primary font-mono max-h-[28rem] overflow-y-auto px-4 py-3 space-y-0.5">
            {round.full_text ? (
              round.full_text.split("\n").map((line, i) => (
                <div
                  key={i}
                  className="whitespace-pre-wrap break-words leading-relaxed"
                >
                  {line || " "}
                </div>
              ))
            ) : (
              <div className="text-text-secondary">(empty)</div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

/** Badge for a reviewer critique's verdict. The new engine uses
 * `APPROVE` / `REQUEST_CHANGES` (vs. the old `APPROVED` / `REVISE`). */
function CritiqueVerdictBadge({
  verdict,
}: {
  verdict: "APPROVE" | "REQUEST_CHANGES";
}) {
  if (verdict === "APPROVE") {
    return (
      <span className="inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded bg-green-500/15 border border-green-500/40 text-green-400 font-medium">
        <span aria-hidden>✓</span>
        APPROVE
      </span>
    );
  }
  return (
    <span className="text-[10px] px-1.5 py-0.5 rounded bg-amber-500/15 border border-amber-500/40 text-amber-300 font-medium">
      REQUEST_CHANGES
    </span>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Result View
// ──────────────────────────────────────────────────────────────────────────────

function ResultView({
  state,
  onDiscard,
}: {
  state: DebateResultState;
  onDiscard: () => void;
}) {
  const [actionError, setActionError] = useState<string | null>(null);
  const [copiedRefined, setCopiedRefined] = useState(false);

  const handleCopyRefined = useCallback(async () => {
    try {
      await navigator.clipboard.writeText(state.finalPlan);
      setCopiedRefined(true);
      window.setTimeout(() => setCopiedRefined(false), 1500);
    } catch (e) {
      setActionError(String(e));
    }
  }, [state.finalPlan]);

  const savedFileName = state.refinedPlanPath
    ? state.refinedPlanPath.split(/[\\/]/).pop()
    : null;

  return (
    <div className="flex-1 overflow-y-auto px-6 pb-6 space-y-4">
      <div className="bg-app-card border border-border rounded-lg p-4 space-y-3">
        <div>
          <p className="text-sm font-medium text-text-primary">
            AI Debate complete{" "}
            {state.approved ? (
              <span className="text-[10px] ml-2 px-1.5 py-0.5 rounded bg-green-500/15 border border-green-500/40 text-green-400">
                APPROVED
              </span>
            ) : (
              <span className="text-[10px] ml-2 px-1.5 py-0.5 rounded bg-amber-500/15 border border-amber-500/40 text-amber-300">
                ROUND LIMIT
              </span>
            )}
          </p>
          <p className="text-xs text-text-secondary mt-1">
            {state.turnsUsed} turn{state.turnsUsed === 1 ? "" : "s"} used.
          </p>
          {(state.authorInputTokens > 0 ||
            state.authorOutputTokens > 0 ||
            state.reviewerInputTokens > 0 ||
            state.reviewerOutputTokens > 0) && (
            <div className="mt-2">
              <TokenBreakdown
                authorIn={state.authorInputTokens}
                authorOut={state.authorOutputTokens}
                reviewerIn={state.reviewerInputTokens}
                reviewerOut={state.reviewerOutputTokens}
                costAuthorUsd={state.costAuthorUsd}
                costReviewerUsd={state.costReviewerUsd}
                costTotalUsd={state.costTotalUsd}
              />
            </div>
          )}
        </div>
      </div>

      {state.refinedPlanPath && (
        <div className="bg-green-500/5 border border-green-500/30 rounded-lg px-4 py-3 flex items-center justify-between gap-3 flex-wrap">
          <div className="min-w-0">
            <p className="text-xs text-text-secondary">Saved as</p>
            <p className="text-sm font-mono text-green-300 truncate" title={state.refinedPlanPath}>
              {savedFileName}
            </p>
          </div>
          <p className="text-[11px] text-text-secondary font-mono truncate max-w-xs" title={state.refinedPlanPath}>
            {state.refinedPlanPath}
          </p>
        </div>
      )}

      {state.caveats.length > 0 && (
        <div className="bg-amber-500/5 border border-amber-500/30 rounded-lg px-4 py-3">
          <p className="text-[10px] uppercase tracking-wide text-amber-300 mb-2">
            Reviewer caveats
          </p>
          <ul className="space-y-1 list-disc pl-5 text-xs text-amber-100">
            {state.caveats.map((c, i) => (
              <li key={i} className="whitespace-pre-wrap break-words">
                {c}
              </li>
            ))}
          </ul>
        </div>
      )}

      {actionError && (
        <div className="px-4 py-3 bg-red-500/10 border border-red-500/30 rounded-lg text-sm text-red-400">
          {actionError}
        </div>
      )}

      <div className="bg-app-card border border-border rounded-lg overflow-hidden">
        <div className="px-4 py-2 border-b border-border flex items-center justify-between">
          <p className="text-xs font-medium text-text-primary">Refined plan</p>
          <div className="relative flex items-center gap-2">
            <button
              type="button"
              onClick={handleCopyRefined}
              className="px-2.5 py-1 text-xs rounded bg-app-bg border border-border text-text-primary hover:bg-app-card-hover transition-colors"
            >
              Copy
            </button>
            {copiedRefined && (
              <span className="absolute -top-7 right-0 px-2 py-0.5 text-[10px] rounded bg-green-500/20 border border-green-500/40 text-green-400 whitespace-nowrap">
                Copied
              </span>
            )}
          </div>
        </div>
        <pre className="text-xs text-text-primary font-mono whitespace-pre-wrap break-words max-h-[60vh] overflow-y-auto px-4 py-3">
          {state.finalPlan}
        </pre>
      </div>

      <div className="flex items-center justify-end">
        <button
          type="button"
          onClick={onDiscard}
          className="px-3 py-1.5 text-sm rounded bg-app-card border border-border text-text-primary hover:bg-app-card-hover transition-colors"
        >
          Done
        </button>
      </div>
    </div>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Token & Cost — shared bits
// ──────────────────────────────────────────────────────────────────────────────

/** Mirrors the Rust `cost_for(model, in, out)` price table in
 * src-tauri/src/commands/debate.rs. Used for the live runner's running total —
 * the persisted/completed cost numbers come from the backend payload directly.
 * Prices are USD per 1,000,000 tokens (rough estimates — adjust together). */
function cost_for_client(modelId: string, inputTokens: number, outputTokens: number): number {
  const TABLE: Record<string, [number, number]> = {
    "claude-opus-4-7": [15.0, 75.0],
    "claude-sonnet-4-6": [3.0, 15.0],
    "claude-haiku-4-5-20251001": [0.8, 4.0],
    "gpt-5": [1.25, 10.0],
    "gpt-5-mini": [0.25, 2.0],
    "gpt-4o": [2.5, 10.0],
    "gpt-4o-mini": [0.15, 0.6],
  };
  const [inPerM, outPerM] = TABLE[modelId] ?? [0, 0];
  return (inputTokens / 1_000_000) * inPerM + (outputTokens / 1_000_000) * outPerM;
}

function formatUSD(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return "$0.00";
  if (n >= 1) return `$${n.toFixed(2)}`;
  if (n >= 0.01) return `$${n.toFixed(4)}`;
  return "$<0.0001";
}

function TokenBreakdown({
  authorIn,
  authorOut,
  reviewerIn,
  reviewerOut,
  costAuthorUsd,
  costReviewerUsd,
  costTotalUsd,
}: {
  authorIn: number;
  authorOut: number;
  reviewerIn: number;
  reviewerOut: number;
  costAuthorUsd: number;
  costReviewerUsd: number;
  costTotalUsd: number;
}) {
  const totalIn = authorIn + reviewerIn;
  const totalOut = authorOut + reviewerOut;
  const row = (
    label: string,
    inT: number,
    outT: number,
    usd: number,
    isTotal = false
  ) => (
    <div
      className={`grid grid-cols-[5rem_1fr_auto] items-center gap-3 font-mono text-xs ${
        isTotal ? "border-t border-border pt-1 mt-1" : ""
      }`}
    >
      <span className="text-text-secondary">{label}</span>
      <span>
        <span className="text-text-primary">{inT.toLocaleString()}</span>
        <span className="text-text-secondary"> in · </span>
        <span className="text-text-primary">{outT.toLocaleString()}</span>
        <span className="text-text-secondary"> out</span>
      </span>
      <span className="text-text-primary">{formatUSD(usd)}</span>
    </div>
  );
  return (
    <div className="text-xs leading-relaxed">
      {row("Author", authorIn, authorOut, costAuthorUsd)}
      {row("Reviewer", reviewerIn, reviewerOut, costReviewerUsd)}
      {row("Total", totalIn, totalOut, costTotalUsd, true)}
    </div>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// History — list panel + read-only detail viewer
// ──────────────────────────────────────────────────────────────────────────────

function HistoryPanel({
  planPath,
  debates,
  onClose,
  onView,
  onDelete,
}: {
  planPath: string;
  debates: DebateSummary[];
  onClose: () => void;
  onView: (id: string) => void;
  onDelete: (id: string) => void;
}) {
  const planName = planPath.split(/[\\/]/).pop() || planPath;
  return (
    <>
      <div
        className="fixed inset-0 bg-black/40 z-[60]"
        onClick={onClose}
        aria-hidden
      />
      <div
        className="fixed right-0 top-0 bottom-0 z-[61] w-full max-w-md bg-app-card border-l border-border shadow-2xl flex flex-col"
        role="dialog"
        aria-modal="true"
      >
        <div className="px-5 py-4 border-b border-border flex items-center justify-between">
          <div className="min-w-0">
            <p className="text-xs text-text-secondary">AI Debate history</p>
            <p className="text-sm font-medium text-text-primary truncate">{planName}</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="px-2 py-1 text-xs rounded border border-border text-text-secondary hover:text-text-primary"
          >
            Close
          </button>
        </div>
        <div className="flex-1 overflow-y-auto p-4 space-y-3">
          {debates.length === 0 ? (
            <p className="text-text-secondary text-sm">No AI debates yet for this plan.</p>
          ) : (
            debates.map((d) => (
              <div
                key={d.id}
                className="bg-app-bg border border-border rounded-lg p-3 space-y-2"
              >
                <div className="flex items-center justify-between gap-2 flex-wrap">
                  <p className="text-xs text-text-secondary">{formatDate(d.created_at)}</p>
                  {d.approved ? (
                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-green-500/15 border border-green-500/40 text-green-400">
                      APPROVED
                    </span>
                  ) : (
                    <span className="text-[10px] px-1.5 py-0.5 rounded bg-amber-500/15 border border-amber-500/40 text-amber-300">
                      ROUND LIMIT
                    </span>
                  )}
                </div>
                <p className="text-xs text-text-primary">
                  <span className="text-text-secondary">Author:</span>{" "}
                  {modelDisplayName(d.author_model)}
                  {"  ·  "}
                  <span className="text-text-secondary">Reviewer:</span>{" "}
                  {modelDisplayName(d.reviewer_model)}
                </p>
                <p className="text-xs text-text-secondary font-mono">
                  {summaryCountLabel(d)} ·{" "}
                  <span className="text-text-primary">{formatUSD(d.cost_total_usd)}</span>
                </p>
                <div className="flex items-center justify-end gap-2 pt-1">
                  <button
                    type="button"
                    onClick={() => onDelete(d.id)}
                    className="px-2 py-1 text-xs rounded border border-red-500/40 text-red-300 hover:bg-red-500/10"
                  >
                    Delete
                  </button>
                  <button
                    type="button"
                    onClick={() => onView(d.id)}
                    className="px-3 py-1 text-xs font-medium rounded bg-accent-blue text-white hover:bg-accent-blue/90"
                  >
                    View
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>
    </>
  );
}

function DebateDetailView({
  record,
  onBack,
  onClose,
}: {
  record: DebateRecord;
  onBack: () => void;
  onClose: () => void;
}) {
  const [splitView, setSplitView] = useState(true);
  const darkStyles = useMemo(
    () => ({
      variables: {
        dark: {
          diffViewerBackground: "#0e0f13",
          diffViewerColor: "#e8e9ed",
          addedBackground: "rgba(34, 197, 94, 0.1)",
          addedColor: "#22c55e",
          removedBackground: "rgba(239, 68, 68, 0.1)",
          removedColor: "#ef4444",
          wordAddedBackground: "rgba(34, 197, 94, 0.3)",
          wordRemovedBackground: "rgba(239, 68, 68, 0.3)",
          addedGutterBackground: "rgba(34, 197, 94, 0.15)",
          removedGutterBackground: "rgba(239, 68, 68, 0.15)",
          gutterBackground: "#13141a",
          gutterBackgroundDark: "#0e0f13",
          gutterColor: "#9394a1",
          addedGutterColor: "#22c55e",
          removedGutterColor: "#ef4444",
          codeFoldGutterBackground: "#1a1b23",
          codeFoldBackground: "#1a1b23",
          emptyLineBackground: "#0e0f13",
          codeFoldContentColor: "#9394a1",
          diffViewerTitleBackground: "#13141a",
          diffViewerTitleColor: "#e8e9ed",
          diffViewerTitleBorderColor: "#2a2b36",
        },
      },
      line: {
        padding: "4px 8px",
        fontSize: "12px",
        fontFamily: "JetBrains Mono, monospace",
      },
    }),
    []
  );

  return (
    <>
      <div className="fixed inset-0 bg-black/40 z-[60]" onClick={onClose} aria-hidden />
      <div
        className="fixed inset-x-4 top-4 bottom-4 z-[61] bg-app-card border border-border rounded-xl shadow-2xl flex flex-col overflow-hidden"
        role="dialog"
        aria-modal="true"
      >
        <div className="px-5 py-4 border-b border-border flex items-center justify-between gap-3 flex-wrap">
          <div className="min-w-0">
            <p className="text-xs text-text-secondary">{formatDate(record.created_at)}</p>
            <p className="text-sm font-medium text-text-primary truncate">
              {record.plan_name || "Unsaved plan"}
            </p>
            <p className="text-xs text-text-secondary mt-1">
              Author: <span className="text-text-primary">{modelDisplayName(record.author_model)}</span>
              {"  ·  "}Reviewer:{" "}
              <span className="text-text-primary">{modelDisplayName(record.reviewer_model)}</span>
            </p>
          </div>
          <div className="flex items-center gap-2">
            {record.approved ? (
              <span className="text-[10px] px-1.5 py-0.5 rounded bg-green-500/15 border border-green-500/40 text-green-400">
                APPROVED
              </span>
            ) : (
              <span className="text-[10px] px-1.5 py-0.5 rounded bg-amber-500/15 border border-amber-500/40 text-amber-300">
                ROUND LIMIT
              </span>
            )}
            <button
              type="button"
              onClick={onBack}
              className="px-3 py-1 text-xs rounded border border-border text-text-secondary hover:text-text-primary"
            >
              ← Back
            </button>
            <button
              type="button"
              onClick={onClose}
              className="px-3 py-1 text-xs rounded border border-border text-text-secondary hover:text-text-primary"
            >
              Close
            </button>
          </div>
        </div>

        <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
          {(record.author_input_tokens > 0 ||
            record.author_output_tokens > 0 ||
            record.reviewer_input_tokens > 0 ||
            record.reviewer_output_tokens > 0) && (
            <div className="bg-app-bg border border-border rounded-lg p-3">
              <TokenBreakdown
                authorIn={record.author_input_tokens}
                authorOut={record.author_output_tokens}
                reviewerIn={record.reviewer_input_tokens}
                reviewerOut={record.reviewer_output_tokens}
                costAuthorUsd={record.cost_author_usd}
                costReviewerUsd={record.cost_reviewer_usd}
                costTotalUsd={record.cost_total_usd}
              />
            </div>
          )}

          <RecordTranscript record={record} />

          {record.caveats && record.caveats.length > 0 && (
            <div className="bg-amber-500/5 border border-amber-500/30 rounded-lg px-4 py-3">
              <p className="text-[10px] uppercase tracking-wide text-amber-300 mb-2">
                Reviewer caveats
              </p>
              <ul className="space-y-1 list-disc pl-5 text-xs text-amber-100">
                {record.caveats.map((c, i) => (
                  <li key={i} className="whitespace-pre-wrap break-words">
                    {c}
                  </li>
                ))}
              </ul>
            </div>
          )}

          <div className="bg-app-card border border-border rounded-lg overflow-hidden">
            <div className="px-4 py-2 border-b border-border flex items-center justify-between">
              <p className="text-xs font-medium text-text-primary">Original ↔ Refined</p>
              <div className="flex bg-app-bg rounded border border-border overflow-hidden">
                <button
                  type="button"
                  onClick={() => setSplitView(true)}
                  className={`px-2.5 py-1 text-xs ${
                    splitView
                      ? "bg-accent-blue text-white"
                      : "text-text-secondary hover:text-text-primary"
                  }`}
                >
                  Split
                </button>
                <button
                  type="button"
                  onClick={() => setSplitView(false)}
                  className={`px-2.5 py-1 text-xs ${
                    !splitView
                      ? "bg-accent-blue text-white"
                      : "text-text-secondary hover:text-text-primary"
                  }`}
                >
                  Unified
                </button>
              </div>
            </div>
            <ReactDiffViewer
              oldValue={record.original_plan}
              newValue={record.final_plan}
              splitView={splitView}
              useDarkTheme
              styles={darkStyles}
              compareMethod={DiffMethod.WORDS}
            />
          </div>
        </div>
      </div>
    </>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Delete-plan dialog (3 options: hide / delete-from-disk / cancel)
// ──────────────────────────────────────────────────────────────────────────────

function DeletePlanDialog({
  plan,
  onCancel,
  onHideFromPage,
  onDeleteFromDisk,
}: {
  plan: PlanEntry;
  onCancel: () => void;
  onHideFromPage: () => void;
  onDeleteFromDisk: () => void;
}) {
  return (
    <>
      <div
        className="fixed inset-0 bg-black/40 z-[70]"
        onClick={onCancel}
        aria-hidden
      />
      <div
        className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 z-[71] w-full max-w-md rounded-xl border border-border bg-app-card shadow-2xl"
        role="dialog"
        aria-modal="true"
      >
        <div className="p-5 space-y-4">
          <div>
            <h2 className="text-base font-semibold text-text-primary mb-1">
              Remove this plan?
            </h2>
            <p className="text-xs text-text-secondary font-mono break-all">
              {plan.file_path}
            </p>
          </div>

          <div className="space-y-2">
            <button
              type="button"
              onClick={onHideFromPage}
              className="w-full text-left px-3 py-2.5 rounded border border-border bg-app-bg hover:border-accent-blue hover:bg-accent-blue/10 transition-colors"
            >
              <p className="text-sm font-medium text-text-primary">
                Remove from Debate page
              </p>
              <p className="text-xs text-text-secondary mt-0.5">
                Keeps the file on disk. This plan won't show up here again
                unless you clear the hide list.
              </p>
            </button>

            <button
              type="button"
              onClick={onDeleteFromDisk}
              className="w-full text-left px-3 py-2.5 rounded border border-red-500/40 bg-red-500/5 hover:bg-red-500/15 transition-colors"
            >
              <p className="text-sm font-medium text-red-300">
                Delete from disk
              </p>
              <p className="text-xs text-text-secondary mt-0.5">
                Permanently deletes the file. This cannot be undone.
              </p>
            </button>
          </div>

          <div className="flex justify-end pt-1">
            <button
              type="button"
              onClick={onCancel}
              className="px-3 py-1.5 text-sm rounded border border-border text-text-secondary hover:text-text-primary"
            >
              Cancel
            </button>
          </div>
        </div>
      </div>
    </>
  );
}

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

/** Convert the persisted `DebateToolCallRecord[]` shape into the UI shape
 * `TurnCard` consumes. Persisted records carry the full `input` JSON and
 * truncated `output` — we surface them as previews directly. */
function recordToolCallsForUi(
  records: DebateToolCallRecord[] | null | undefined
): DebateToolCallState[] {
  if (!records || records.length === 0) return [];
  return records.map((r) => ({
    tool: r.tool,
    inputPreview: r.input,
    outputPreview: r.output,
    isError: r.is_error,
  }));
}

/** Cosmetic label for a persisted debate's iteration count. Prefers the v2
 * `turns_used` field; falls back to legacy `rounds_used` for older records. */
function summaryCountLabel(summary: DebateSummary): string {
  const turns = summary.turns_used ?? 0;
  if (turns > 0) {
    return `${turns} turn${turns === 1 ? "" : "s"}`;
  }
  const rounds = summary.rounds_used;
  return `${rounds} round${rounds === 1 ? "" : "s"}`;
}

function modelDisplayName(modelId: string): string {
  const known = DEBATE_MODELS.find((m) => m.id === modelId);
  if (known) return known.label;
  const lower = modelId.toLowerCase();
  if (lower.includes("claude") || lower.includes("anthropic")) return modelId;
  if (lower.includes("gpt") || lower.includes("openai")) return modelId;
  return modelId;
}

function formatDate(dateStr: string): string {
  try {
    const d = new Date(dateStr);
    return d.toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  } catch {
    return dateStr;
  }
}
