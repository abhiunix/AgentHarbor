import { create } from "zustand";
import {
  getBenchmarkRun,
  listBenchmarkDatasets,
  listBenchmarkModels,
  listBenchmarkProviders,
  listBenchmarkRuns,
  listReferenceBenchmarks,
  refreshBenchmarkModels,
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
  runDetails: Record<string, BenchmarkRun>;
  currentRun: BenchmarkRun | null;
  loading: boolean;
  running: boolean;
  error: string | null;
}

interface BenchmarkActions {
  bootstrap: () => Promise<void>;
  refreshModels: (providerId?: string) => Promise<void>;
  refreshRuns: () => Promise<void>;
  hydrateRunDetails: (runIds?: string[]) => Promise<void>;
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
  runDetails: {},
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

  refreshModels: async (providerId) => {
    try {
      if (providerId) {
        const liveModels = await refreshBenchmarkModels(providerId);
        set((state) => {
          const nextModels = [
            ...state.models.filter((model) => model.provider_id !== providerId),
            ...liveModels,
          ].sort((left, right) => {
            const providerCompare = left.provider_id.localeCompare(right.provider_id);
            if (providerCompare !== 0) {
              return providerCompare;
            }
            return left.display_name.localeCompare(right.display_name);
          });
          return { models: nextModels };
        });
        return;
      }

      const models = await listBenchmarkModels();
      set({ models });
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) });
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

  hydrateRunDetails: async (runIds) => {
    const state = useBenchmarkStore.getState();
    const idsToLoad = (runIds ?? state.runs.map((run) => run.id)).filter((runId) => !state.runDetails[runId]);
    if (idsToLoad.length === 0) {
      return;
    }

    try {
      const runs = await Promise.all(idsToLoad.map((runId) => getBenchmarkRun(runId)));
      set((current) => ({
        runDetails: {
          ...current.runDetails,
          ...Object.fromEntries(runs.map((run) => [run.id, run])),
        },
      }));
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
