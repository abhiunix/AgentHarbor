import modelCostsJson from "../data/model-costs.json";

export interface ModelCostRates {
  input_per_million: number;
  output_per_million: number;
  cache_read_per_million: number;
}

export interface ModelCostEntry {
  name: string;
  input_per_million: number;
  output_per_million: number;
  cache_read_per_million: number;
  deprecated?: boolean;
}

export interface ModelCostsConfig {
  description?: string;
  models: Record<string, ModelCostEntry>;
  aliases?: Record<string, string>;
  default_fallback: ModelCostRates;
}

const config = modelCostsJson as ModelCostsConfig;

function resolveModelId(modelId: string): string | null {
  const key = modelId.trim();
  if (config.models[key]) return key;
  if (config.aliases && config.aliases[key] && config.models[config.aliases[key]]) {
    return config.aliases[key];
  }
  return null;
}

function getRatesForModel(modelId: string | undefined): ModelCostRates {
  if (!modelId) return config.default_fallback;
  const resolved = resolveModelId(modelId);
  if (resolved) {
    const m = config.models[resolved];
    return {
      input_per_million: m.input_per_million,
      output_per_million: m.output_per_million,
      cache_read_per_million: m.cache_read_per_million,
    };
  }
  const lower = modelId.toLowerCase();
  for (const [k, m] of Object.entries(config.models)) {
    if (lower.includes(k.toLowerCase()) || k.toLowerCase().includes(lower)) {
      return {
        input_per_million: m.input_per_million,
        output_per_million: m.output_per_million,
        cache_read_per_million: m.cache_read_per_million,
      };
    }
  }
  return config.default_fallback;
}

export function computeCost(
  inputTokens: number,
  outputTokens: number,
  cacheReadTokens: number,
  modelId: string | undefined
): number {
  const c = computeCostBreakdown(inputTokens, outputTokens, cacheReadTokens, modelId);
  return c.inputCost + c.outputCost + c.cacheCost;
}

export function computeCostBreakdown(
  inputTokens: number,
  outputTokens: number,
  cacheReadTokens: number,
  modelId: string | undefined
): { inputCost: number; outputCost: number; cacheCost: number } {
  const r = getRatesForModel(modelId);
  return {
    inputCost: (inputTokens / 1_000_000) * r.input_per_million,
    outputCost: (outputTokens / 1_000_000) * r.output_per_million,
    cacheCost: (cacheReadTokens / 1_000_000) * r.cache_read_per_million,
  };
}

export function getModelCostsConfig(): ModelCostsConfig {
  return config;
}
