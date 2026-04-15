import { create } from "zustand";
import {
  getAiCommitScores,
  getAiCodeSummary,
  getAiFileTypeBreakdown,
  getConversationSummaries,
  getAiTrackingModelBreakdown,
} from "../lib/tauri";
import type {
  ScoredCommit,
  AiCodeSummary,
  FileTypeBreakdown,
  ConversationSummary,
} from "../lib/tauri";

interface AiTrackingState {
  commits: ScoredCommit[];
  summary: AiCodeSummary | null;
  fileTypes: FileTypeBreakdown[];
  conversations: ConversationSummary[];
  modelBreakdown: Record<string, number>;
  loading: boolean;
  error: string | null;
}

interface AiTrackingActions {
  fetchAll: () => Promise<void>;
}

export const useAiTrackingStore = create<AiTrackingState & AiTrackingActions>((set) => ({
  commits: [],
  summary: null,
  fileTypes: [],
  conversations: [],
  modelBreakdown: {},
  loading: false,
  error: null,

  fetchAll: async () => {
    set({ loading: true, error: null });
    try {
      const [commits, summary, fileTypes, conversations, modelBreakdown] = await Promise.all([
        getAiCommitScores(200, 0),
        getAiCodeSummary(),
        getAiFileTypeBreakdown(),
        getConversationSummaries(100, 0),
        getAiTrackingModelBreakdown(),
      ]);
      set({ commits, summary, fileTypes, conversations, modelBreakdown, loading: false });
    } catch (e) {
      set({ loading: false, error: e instanceof Error ? e.message : String(e) });
    }
  },
}));
