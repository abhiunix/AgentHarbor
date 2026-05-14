# AgentHarbor — Dan's session todo

Tracks the implementation backlog derived from `docs/dan_features-claude.md`. This is the source of truth for "what to pick up next" between sessions. Mark items `[x]` when shipped (PR merged). Mark `[~]` if in flight.

## Phase 0 — credibility + foundations

- [ ] PR #1 — Surface hidden providers (P0). Sidebar + provider modal + docs/features.md + README provider table. Mark each experimental vs GA.
- [ ] PR #2 — GitHub Actions regression workflow. `.github/workflows/regression.yml` runs `npm run test:regression` on push to main + PRs.
- [ ] PR #3 — Dynamic model catalog (P1). New `src-tauri/src/analytics/model_catalog.rs`. Replace hardcoded `get_raw_pricing` (cost_engine.rs:40-82) and `src/data/model-costs.json`. Daily refresh from Anthropic / OpenAI / Google / OpenRouter + `simonw/llm-prices`. Cache at `~/.agentharbor/model-catalog.json` with etag + 24h staleness. Settings UI badge for "catalog updated N hours ago".

## Phase 1 — benchmarking primitives (no UI yet)

- [ ] PR #4 — Benchmark data model in Rust + TS mirror (`src-tauri/src/models/benchmark.rs`, `src/lib/types.ts`).
- [ ] PR #4 — Tauri command stubs in `src-tauri/src/commands/benchmarks.rs` returning fixtures; wired into `lib.rs`.
- [ ] PR #4 — Persistence: `~/.agentharbor/benchmarks/{run_id}/items/*.json` + `index.sqlite` (rusqlite).
- [ ] PR #5 — Token-diff panel (P3). Use Anthropic Token Count API for Claude, tiktoken for OpenAI, Gemini countTokens for Google. Callable from deploy preview.
- [ ] PR #6 — Ollama provider (P7). `src-tauri/src/analytics/ollama.rs`. Auto-detect `localhost:11434`, list installed models, token counts via `/api/tokenize`.

## Phase 2 — Benchmark Lab MVP UI

- [ ] PR #7 — `src/pages/BenchmarkLabPage.tsx` + `src/stores/benchmarkStore.ts` + `src/components/benchmarks/{DatasetBrowser,RunSetupPanel,LiveGrid,MetricsRail,CostEstimateModal,RunHistoryList}.tsx`. Sidebar entry above Notes.
- [ ] PR #7 — Streaming run progress via Tauri event channel.
- [ ] Assertions module (exact / regex / JSON schema / contains / AST equality for code).
- [ ] Bundled dataset packs: 1× coding, 1× extraction, 1× writing. Place under `registry/benchmarks/`.

## Phase 3 — judges + human rating

- [ ] LLM-as-judge: pointwise rubric, balanced permutation, ≥2 judge models.
- [ ] `HumanRatingDrawer.tsx` keyboard-driven (`j`/`k`/`1`-`5`/`c`).
- [ ] Pairwise comparison mode.
- [ ] Spearman tracking per judge over time.

## Phase 4 — Inspect AI bridge + contamination overlay

- [ ] Subprocess + log streaming to Inspect AI.
- [ ] LiveBench / SWE-bench Live importers.
- [ ] Contamination overlay on bundled benchmarks (warn when model cutoff > benchmark release).

## Phase 5 — observability + governance

- [ ] PR — Prompt-caching observability panel (P2): cache hit ratio, savings, worst miss prompts, cache-key churn detector.
- [ ] PR — MCP tool latency profiler (P4).
- [ ] PR — Deploy regression detector (P5): post-deploy sentinel run + delta report.
- [ ] PR — Per-project cost budgets and alerts (P6).

## Phase 6 — image gen + local model variety

- [ ] Image-gen lane: VQAScore + T2I-CompBench++ fixtures.
- [ ] LM Studio (MLX) + llama.cpp providers alongside Ollama.

## Notes

- All PRs targeted at `abhiunix/AgentHarbor` upstream from `dan/<topic>` branches.
- Never merge from a session without user approval.
- E2E in CI must mock providers — never hit live APIs.
