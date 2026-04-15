import { create } from "zustand";
import { readProjectUsageFiles } from "../lib/tauri";
import type { ProjectUsageRecord } from "../lib/tauri";

function sumUsage(records: ProjectUsageRecord[]): { input: number; output: number; cacheRead: number } {
  let input = 0;
  let output = 0;
  let cacheRead = 0;
  for (const r of records) {
    const u = r.usage;
    if (!u) continue;
    input += u.input_tokens ?? 0;
    output += u.output_tokens ?? 0;
    cacheRead += u.cache_read_input_tokens ?? 0;
  }
  return { input, output, cacheRead };
}

export type TimeRange = "5h" | "today" | "7d" | "week" | "30d" | "month" | "all";

interface UsageState {
  records: ProjectUsageRecord[];
  loading: boolean;
  error: string | null;
  lastFetched: number | null;
  timeRange: TimeRange;
  modelFilter: string;
  projectFilter: string | null;
}

function getTimeRangeStartMs(range: TimeRange): number {
  const now = Date.now();
  const d = new Date(now);
  switch (range) {
    case "5h":
      return now - 5 * 60 * 60 * 1000;
    case "today":
      d.setHours(0, 0, 0, 0);
      return d.getTime();
    case "7d":
      return now - 7 * 24 * 60 * 60 * 1000;
    case "week":
      d.setDate(d.getDate() - d.getDay());
      d.setHours(0, 0, 0, 0);
      return d.getTime();
    case "30d":
      return now - 30 * 24 * 60 * 60 * 1000;
    case "month":
      d.setDate(1);
      d.setHours(0, 0, 0, 0);
      return d.getTime();
    case "all":
      return 0;
    default:
      return now - 7 * 24 * 60 * 60 * 1000;
  }
}

interface UsageActions {
  refetch: () => Promise<void>;
  setTimeRange: (range: TimeRange) => void;
  setModelFilter: (model: string) => void;
  setProjectFilter: (project: string | null) => void;
}

export const useUsageStore = create<UsageState & UsageActions>((set) => ({
  records: [],
  loading: false,
  error: null,
  lastFetched: null,
  timeRange: "7d" as TimeRange,
  modelFilter: "",
  projectFilter: null,

  refetch: async () => {
    set({ loading: true, error: null });
    try {
      const records = await readProjectUsageFiles();
      set({ records, loading: false, lastFetched: Date.now() });
    } catch (e) {
      set({
        loading: false,
        error: e instanceof Error ? e.message : String(e),
      });
    }
  },

  setTimeRange: (timeRange) => set({ timeRange }),
  setModelFilter: (modelFilter) => set({ modelFilter }),
  setProjectFilter: (projectFilter) => set({ projectFilter }),
}));

export function useFilteredUsage(): {
  records: ProjectUsageRecord[];
  totals: { input: number; output: number; cacheRead: number };
  models: string[];
} {
  const records = useUsageStore((s) => s.records);
  const timeRange = useUsageStore((s) => s.timeRange);
  const modelFilter = useUsageStore((s) => s.modelFilter);
  const projectFilter = useUsageStore((s) => s.projectFilter);
  const since = getTimeRangeStartMs(timeRange);
  const filtered = records.filter((r) => {
    const t = new Date(r.timestamp).getTime();
    if (t < since) return false;
    if (modelFilter && r.model !== modelFilter) return false;
    if (projectFilter != null && r.project_path !== projectFilter) return false;
    return true;
  });
  const totals = sumUsage(filtered);
  const models = Array.from(new Set(records.map((r) => r.model).filter(Boolean))) as string[];
  return { records: filtered, totals, models };
}

export function useUsageTotalsToday(): { input: number; output: number; cacheRead: number } {
  const records = useUsageStore((s) => s.records);
  const todayStart = new Date();
  todayStart.setHours(0, 0, 0, 0);
  const since = todayStart.getTime();
  const todayRecords = records.filter((r) => new Date(r.timestamp).getTime() >= since);
  return sumUsage(todayRecords);
}
