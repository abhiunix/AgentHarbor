import { create } from "zustand";
import {
  getBenchmarkRun,
  listBenchmarkDatasets,
  listBenchmarkModels,
  listBenchmarkProviders,
  listBenchmarkRuns,
  listReferenceBenchmarks,
  runBenchmarkSuite,
  saveBenchmarkDataset,
  updateBenchmarkManualReview,
  type BenchmarkDataset,
  type BenchmarkModel,
  type BenchmarkProvider,
  type BenchmarkRun,
  type BenchmarkRunRequest,
  type BenchmarkRunSummary,
  type ManualReview,
  type ReferenceBenchmark,
} from "../lib/tauri";

interface BenchmarkState {
  providers: BenchmarkProvider[];
  models: BenchmarkModel[];
  datasets: BenchmarkDataset[];
  references: ReferenceBenchmark[];
  runs: BenchmarkRunSummary[];
  currentRun: BenchmarkRun | null;
  loading: boolean;
  running: boolean;
  error: string | null;
}

interface BenchmarkActions {
  bootstrap: () => Promise<void>;
  refreshRuns: () => Promise<void>;
  loadRun: (runId: string) => Promise<void>;
  saveDataset: (dataset: BenchmarkDataset) => Promise<BenchmarkDataset | null>;
  runSuite: (request: BenchmarkRunRequest) => Promise<BenchmarkRun | null>;
  saveManualReview: (runId: string, itemId: string, manualReview: ManualReview) => Promise<void>;
  clearError: () => void;
}

export const useBenchmarkStore = create<BenchmarkState & BenchmarkActions>((set) => ({
  providers: [],
  models: [],
  datasets: [],
  references: [],
  runs: [],
  currentRun: null,
  loading: false,
  running: false,
  error: null,

  bootstrap: async () => {
    set({ loading: true, error: null });
    try {
      const [providers, models, datasets, references, runs] = await Promise.all([
        listBenchmarkProviders(),
        listBenchmarkModels(),
        listBenchmarkDatasets(),
        listReferenceBenchmarks(),
        listBenchmarkRuns(),
      ]);
      set({
        providers,
        models,
        datasets,
        references,
        runs,
        loading: false,
      });
    } catch (error) {
      set({
        loading: false,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  },

  refreshRuns: async () => {
    try {
      const runs = await listBenchmarkRuns();
      set({ runs });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    }
  },

  loadRun: async (runId) => {
    set({ loading: true, error: null });
    try {
      const run = await getBenchmarkRun(runId);
      set({ currentRun: run, loading: false });
    } catch (error) {
      set({
        loading: false,
        error: error instanceof Error ? error.message : String(error),
      });
    }
  },

  saveDataset: async (dataset) => {
    set({ loading: true, error: null });
    try {
      const saved = await saveBenchmarkDataset(dataset);
      const datasets = await listBenchmarkDatasets();
      set({ datasets, loading: false });
      return saved;
    } catch (error) {
      set({
        loading: false,
        error: error instanceof Error ? error.message : String(error),
      });
      return null;
    }
  },

  runSuite: async (request) => {
    set({ running: true, error: null });
    try {
      const run = await runBenchmarkSuite(request);
      const runs = await listBenchmarkRuns();
      set({
        currentRun: run,
        runs,
        running: false,
      });
      return run;
    } catch (error) {
      set({
        running: false,
        error: error instanceof Error ? error.message : String(error),
      });
      return null;
    }
  },

  saveManualReview: async (runId, itemId, manualReview) => {
    try {
      const run = await updateBenchmarkManualReview(runId, itemId, manualReview);
      set({ currentRun: run });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
    }
  },

  clearError: () => set({ error: null }),
}));
