/**
 * Debate-run state shared across the whole app.
 *
 * Previously this lived in `DebatePage.tsx` local React state. Unmounting the
 * page (e.g. by navigating to another sidebar entry) tore everything down
 * mid-flight — listeners were detached, runState/turns garbage-collected, and
 * any `debate:*` events emitted from the still-running Rust worker thread
 * fell on the floor. The user could come back later only to see the final
 * record in history; they lost the live view and couldn't cancel.
 *
 * Now this Zustand store holds the run state for the app's lifetime. Tauri
 * event listeners are wired ONCE at app start (via `initDebateRunListeners`)
 * and write directly into the store. Any component — DebatePage, Sidebar,
 * future status bar — can subscribe.
 */

import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";
import type {
  DebateTurnStartEvent,
  DebateTokenEvent,
  DebateToolCallEvent,
  DebateTurnCompleteEvent,
  DebateCompleteEvent,
  DebateErrorEvent,
  DebateModel,
  DebateSpeaker,
  DebateTurnKind,
  ParsedTurn,
} from "../lib/tauri";

// ── Public types (moved here from DebatePage so any consumer can import) ────

export interface DebateToolCallState {
  tool: string;
  inputPreview: string;
  outputPreview: string;
  isError: boolean;
}

export interface DebateTurnState {
  index: number;
  speaker: DebateSpeaker;
  kind: DebateTurnKind;
  model: string;
  /** Streaming buffer — tokens are appended as `debate:token` events arrive. */
  text: string;
  /** Locked once `turn_complete` fires. */
  complete: boolean;
  parsed: ParsedTurn | null;
  parseError: string | null;
  inputTokens?: number;
  outputTokens?: number;
  toolCalls: DebateToolCallState[];
  /** Captured from `debate:turn_start` so the Inspect view can show them. */
  systemPrompt: string;
  userPrompt: string;
}

export interface DebateRunState {
  debateId: string;
  /** Max critique turns the user configured. */
  maxRounds: number;
  currentTurn: number;
  turns: DebateTurnState[];
  planPath: string | null;
  planContent: string;
  authorModel: DebateModel;
  reviewerModel: DebateModel;
}

export interface DebateResultState {
  debateId: string;
  planPath: string | null;
  planContent: string;
  finalPlan: string;
  caveats: string[];
  refinedPlanPath: string | null;
  turnsUsed: number;
  approved: boolean;
  totalInputTokens: number;
  totalOutputTokens: number;
  authorInputTokens: number;
  authorOutputTokens: number;
  reviewerInputTokens: number;
  reviewerOutputTokens: number;
  costAuthorUsd: number;
  costReviewerUsd: number;
  costTotalUsd: number;
  authorModelId: string;
  reviewerModelId: string;
}

export type DebateView = "list" | "running" | "result";

// ── Store ───────────────────────────────────────────────────────────────────

interface DebateRunStore {
  /** Matcher key for filtering incoming events. Set synchronously the moment
   * `start_debate` returns so the first event from the worker can never be
   * filtered out by a stale ref (the same race the previous `useRef` setup
   * was guarding against). */
  currentDebateId: string | null;
  runState: DebateRunState | null;
  resultState: DebateResultState | null;
  view: DebateView;
  runError: string | null;

  /** Called from `handleStartDebate` after the backend returns the debate id.
   * Atomically populates currentDebateId + runState + resets view to "running"
   * so subsequent turn_start events match without a race. */
  startRun: (initial: DebateRunState) => void;

  /** Optimistic UI reset for the Cancel button. The backend cancel command
   * is fired separately by the caller — we don't make the user wait. */
  cancelRun: () => void;

  /** Full reset (after the user clicks Done / Discard from the result view). */
  discard: () => void;

  /** Called by the listener; allows the page to react to non-cancellation
   * errors during a run (we don't auto-reset for these). */
  setRunError: (msg: string | null) => void;

  // Listener-driven mutations (called from initDebateRunListeners).
  _onTurnStart: (e: DebateTurnStartEvent) => void;
  _onToken: (e: DebateTokenEvent) => void;
  _onToolCall: (e: DebateToolCallEvent) => void;
  _onTurnComplete: (e: DebateTurnCompleteEvent) => void;
  _onComplete: (e: DebateCompleteEvent) => void;
  _onError: (e: DebateErrorEvent) => void;
}

export const useDebateRunStore = create<DebateRunStore>((set, get) => ({
  currentDebateId: null,
  runState: null,
  resultState: null,
  view: "list",
  runError: null,

  startRun: (initial) =>
    set({
      currentDebateId: initial.debateId,
      runState: initial,
      resultState: null,
      runError: null,
      view: "running",
    }),

  cancelRun: () =>
    set({
      currentDebateId: null,
      runState: null,
      runError: null,
      view: "list",
    }),

  discard: () =>
    set({
      currentDebateId: null,
      runState: null,
      resultState: null,
      runError: null,
      view: "list",
    }),

  setRunError: (runError) => set({ runError }),

  _onTurnStart: (e) => {
    if (get().currentDebateId !== e.debate_id) return;
    set((s) => {
      if (!s.runState || s.runState.debateId !== e.debate_id) return s;
      return {
        runState: {
          ...s.runState,
          currentTurn: e.index,
          turns: [
            ...s.runState.turns,
            {
              index: e.index,
              speaker: e.speaker,
              kind: e.kind,
              model: e.model,
              text: "",
              complete: false,
              parsed: null,
              parseError: null,
              toolCalls: [],
              systemPrompt: e.system_prompt ?? "",
              userPrompt: e.user_prompt ?? "",
            },
          ],
        },
      };
    });
  },

  _onToken: (e) => {
    if (get().currentDebateId !== e.debate_id) return;
    set((s) => {
      if (!s.runState || s.runState.debateId !== e.debate_id) return s;
      const idx = s.runState.turns.findIndex((t) => t.index === e.index);
      if (idx === -1) return s;
      const turns = s.runState.turns.slice();
      if (turns[idx].complete) return s;
      turns[idx] = { ...turns[idx], text: turns[idx].text + e.text };
      return { runState: { ...s.runState, turns } };
    });
  },

  _onToolCall: (e) => {
    if (get().currentDebateId !== e.debate_id) return;
    set((s) => {
      if (!s.runState || s.runState.debateId !== e.debate_id) return s;
      const idx = s.runState.turns.findIndex((t) => t.index === e.index);
      if (idx === -1) return s;
      const turns = s.runState.turns.slice();
      turns[idx] = {
        ...turns[idx],
        toolCalls: [
          ...turns[idx].toolCalls,
          {
            tool: e.tool,
            inputPreview: e.input_preview,
            outputPreview: e.output_preview,
            isError: e.is_error,
          },
        ],
      };
      return { runState: { ...s.runState, turns } };
    });
  },

  _onTurnComplete: (e) => {
    if (get().currentDebateId !== e.debate_id) return;
    set((s) => {
      if (!s.runState || s.runState.debateId !== e.debate_id) return s;
      const idx = s.runState.turns.findIndex((t) => t.index === e.index);
      if (idx === -1) return s;
      const turns = s.runState.turns.slice();
      turns[idx] = {
        ...turns[idx],
        text: e.raw_text || turns[idx].text,
        complete: true,
        parsed: e.parsed,
        parseError: e.parse_error,
        inputTokens: e.input_tokens,
        outputTokens: e.output_tokens,
      };
      return { runState: { ...s.runState, turns } };
    });
  },

  _onComplete: (e) => {
    if (get().currentDebateId !== e.debate_id) return;
    const snapshot = get().runState;
    if (!snapshot) return;
    set({
      resultState: {
        debateId: snapshot.debateId,
        planPath: snapshot.planPath,
        planContent: snapshot.planContent,
        finalPlan: e.final_plan,
        caveats: e.caveats,
        refinedPlanPath: e.refined_plan_path || null,
        turnsUsed: e.turns_used,
        approved: e.approved,
        totalInputTokens: e.total_input_tokens,
        totalOutputTokens: e.total_output_tokens,
        authorInputTokens: e.author_input_tokens,
        authorOutputTokens: e.author_output_tokens,
        reviewerInputTokens: e.reviewer_input_tokens,
        reviewerOutputTokens: e.reviewer_output_tokens,
        costAuthorUsd: e.cost_author_usd,
        costReviewerUsd: e.cost_reviewer_usd,
        costTotalUsd: e.cost_total_usd,
        authorModelId: snapshot.authorModel.id,
        reviewerModelId: snapshot.reviewerModel.id,
      },
      view: "result",
    });
  },

  _onError: (e) => {
    if (get().currentDebateId !== e.debate_id) return;
    if (e.message === "cancelled") {
      set({
        currentDebateId: null,
        runState: null,
        runError: null,
        view: "list",
      });
    } else {
      set({ runError: e.message });
    }
  },
}));

// ── Listener init ───────────────────────────────────────────────────────────

let _initStarted = false;

/**
 * Attach `debate:*` event listeners to the Tauri event bus. Idempotent — safe
 * to call multiple times. Call ONCE at app startup (App.tsx). Listeners stay
 * attached for the app's lifetime so debates running while the user is on
 * another page still update the store.
 */
export async function initDebateRunListeners(): Promise<void> {
  if (_initStarted) return;
  _initStarted = true;
  const store = useDebateRunStore.getState();
  await Promise.all([
    listen<DebateTurnStartEvent>("debate:turn_start", (e) =>
      useDebateRunStore.getState()._onTurnStart(e.payload)
    ),
    listen<DebateTokenEvent>("debate:token", (e) =>
      useDebateRunStore.getState()._onToken(e.payload)
    ),
    listen<DebateToolCallEvent>("debate:tool_call", (e) =>
      useDebateRunStore.getState()._onToolCall(e.payload)
    ),
    listen<DebateTurnCompleteEvent>("debate:turn_complete", (e) =>
      useDebateRunStore.getState()._onTurnComplete(e.payload)
    ),
    listen<DebateCompleteEvent>("debate:complete", (e) =>
      useDebateRunStore.getState()._onComplete(e.payload)
    ),
    listen<DebateErrorEvent>("debate:error", (e) =>
      useDebateRunStore.getState()._onError(e.payload)
    ),
  ]);
  // Silence unused-binding lint in case future refactors drop the destructure.
  void store;
}
