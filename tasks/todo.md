# AgentHarbor — Dan's session todo

Tracks the implementation backlog derived from `docs/dan_features.md` (canonical 10-item list) and `docs/dan_features-claude.md` (additions). This is the source of truth for "what to pick up next" between sessions. Mark items `[x]` when shipped (PR merged). Mark `[~]` if in flight.

## Current WIP — do not duplicate

- [~] **dan_features.md task #3** — normalized benchmark types in TS + Rust. `src-tauri/src/commands/mod.rs` has uncommitted `pub mod benchmarks;` declaration; `commands/benchmarks.rs` file pending. Coordinate before touching.

## dan_features.md canonical 10-item list

- [ ] **#1** Audit production-ready vs partial providers. → covered by **P0** below.
- [ ] **#2** Add Benchmark Lab to roadmap and navigation. Roadmap ✓ in docs; sidebar entry pending (above Notes in `src/components/layout/Sidebar.tsx`).
- [~] **#3** Normalized benchmark types in TS + Rust (WIP, see above). `models/benchmark.rs` + `src/lib/types.ts` mirror.
- [ ] **#4** Single-prompt × many-models MVP using stored API keys. Maps to **PR #7** below.
- [ ] **#5** Per-run metrics: tokens / cost / latency / context. Depends on **P1** dynamic catalog.
- [ ] **#6** Prompt variant comparison.
- [ ] **#7** Skill / plugin toggles for overhead and quality impact. Pairs with **P3** token-diff.
- [ ] **#8** Dataset import/export and saved run history.
- [ ] **#9** Rubric scoring and pairwise judge mode.
- [ ] **#10** E2E tests with mocked providers + deterministic fixtures.

## Additions from dan_features-claude.md

### Phase 0 — credibility + foundations

- [ ] **PR #1 / P0 / #1** — Surface hidden providers. Sidebar + provider modal + docs/features.md + README provider table. Mark experimental vs GA.
- [ ] **PR #2** — GitHub Actions regression workflow. `.github/workflows/regression.yml` runs `npm run test:regression` on push to main + PRs. (Not in canonical 10-item list; prerequisite for #10.)
- [ ] **PR #3 / P1** — Dynamic model catalog. New `src-tauri/src/analytics/model_catalog.rs`. Replace hardcoded `get_raw_pricing` (cost_engine.rs:40-82) and `src/data/model-costs.json`. Daily refresh from Anthropic / OpenAI / Google / OpenRouter + `simonw/llm-prices`. Cache at `~/.agentharbor/model-catalog.json` with etag + 24h staleness. Settings UI badge. (Unlocks canonical #5.)

### Phase 1 — benchmarking primitives (no UI yet)

> **Coordinate with WIP on canonical #3** before opening PR #4. The author of the uncommitted `pub mod benchmarks;` change owns the initial scaffold.

- [ ] **PR #4 / #3** — Benchmark data model: `src-tauri/src/models/benchmark.rs` + `src/lib/types.ts` mirror.
- [ ] **PR #4 / #3** — Tauri command stubs in `src-tauri/src/commands/benchmarks.rs` returning fixtures; wired into `lib.rs`.
- [ ] **PR #4 / #8** — Persistence: `~/.agentharbor/benchmarks/{run_id}/items/*.json` + `index.sqlite` (rusqlite).
- [ ] **PR #5 / P3** — Token-diff panel. Anthropic Token Count API for Claude, tiktoken for OpenAI, Gemini countTokens for Google. Callable from deploy preview. (Net-new; supports canonical #7.)
- [ ] **PR #6 / P7** — Ollama provider. `src-tauri/src/analytics/ollama.rs`. Auto-detect `localhost:11434`, list installed models, token counts via `/api/tokenize`. (Net-new local lane.)

### Phase 2 — Benchmark Lab MVP UI

- [ ] **PR #7 / #2 + #4** — `src/pages/BenchmarkLabPage.tsx` + `src/stores/benchmarkStore.ts` + `src/components/benchmarks/{DatasetBrowser,RunSetupPanel,LiveGrid,MetricsRail,CostEstimateModal,RunHistoryList}.tsx`. Sidebar entry above Notes.
- [ ] **PR #7 / #4** — Streaming run progress via Tauri event channel.
- [ ] **#9** Assertions module (exact / regex / JSON schema / contains / AST equality for code).
- [ ] **#8** Bundled dataset packs: 1× coding, 1× extraction, 1× writing. Place under `registry/benchmarks/`.

### Phase 3 — judges + human rating

- [ ] **#9** LLM-as-judge: pointwise rubric, balanced permutation, ≥2 judge models.
- [ ] `HumanRatingDrawer.tsx` keyboard-driven (`j`/`k`/`1`-`5`/`c`).
- [ ] **#9** Pairwise comparison mode.
- [ ] Spearman tracking per judge over time.

### Phase 4 — Inspect AI bridge + contamination overlay

- [ ] Subprocess + log streaming to Inspect AI.
- [ ] LiveBench / SWE-bench Live importers.
- [ ] Contamination overlay on bundled benchmarks (warn when model cutoff > benchmark release).

### Phase 5 — observability + governance

- [ ] PR — Prompt-caching observability panel (P2): cache hit ratio, savings, worst miss prompts, cache-key churn detector.
- [ ] PR — MCP tool latency profiler (P4).
- [ ] PR — Deploy regression detector (P5): post-deploy sentinel run + delta report.
- [ ] PR — Per-project cost budgets and alerts (P6).

### Phase 6 — image gen + local model variety

- [ ] Image-gen lane: VQAScore + T2I-CompBench++ fixtures.
- [ ] LM Studio (MLX) + llama.cpp providers alongside Ollama.

## Notes

- All PRs targeted at `abhiunix/AgentHarbor` upstream from `dan/<topic>` branches.
- Never merge from a session without user approval.
- E2E in CI must mock providers — never hit live APIs.
- When a PR ships, reference the `dan_features.md` task number (e.g., `#3 — benchmark types`) in the commit subject so both docs stay synced.
- Do not edit `docs/dan_features.md` (it is the canonical contract). Add to `docs/dan_features-claude.md` or open a new companion doc.
