# Dan Features (Claude edition) — Benchmark Lab & Beyond

A deeper companion to [`dan_features.md`](./dan_features.md). That doc sets the direction. This one drills into the actual codebase, the current (2025–2026) eval landscape, and a concrete short-duration backlog that fits the existing Tauri + Rust + React architecture.

Audience: contributors who will actually build this. Every claim is anchored to a file path, a paper, or a public benchmark.

Last research pass: 2026-05-15.

---

## 1. What AgentHarbor already has (audit findings)

The Rust backend is further along than the README suggests. A direct walk of `src-tauri/src/` produced this inventory:

### Provider surface — declared vs hidden

| Layer | Count | Where |
|---|---|---|
| Analytics modules in Rust | **16** | `src-tauri/src/analytics/{claude,claude_v2,claude_account,claude_desktop,cursor,cursor_v2,codex,gemini,copilot,openrouter,vertex_ai,kimi,zai,augment,amp,droid,kiro,jetbrains}.rs` |
| Deploy adapters in Rust | **8** | `src-tauri/src/adapters/{claude_code,cursor,windsurf,gemini,copilot,antigravity,vscode,codex}.rs` |
| Providers advertised as GA in README | **5** | Claude Code, Cursor, Codex, Gemini, Windsurf |
| Providers exposed in sidebar (`src/components/layout/Sidebar.tsx:51-234`) | **9** | Above 5 + Claude Desktop, Copilot, VSCode, AntiGravity, plus Codex partial |
| Orphan pages | 1+ | `UnifiedAnalyticsPage.tsx` (no sidebar entry; route present) |

Implication: AgentHarbor can claim multi-provider parity with OpenRouter and LiteLLM control planes *today* if it surfaces what is already wired. That is a one-PR win independent of any benchmark work.

### Tauri command surface

`src-tauri/src/lib.rs` registers **170+ commands**, grouped roughly:

- Registry & capabilities (~11): `get_all_capabilities`, `search_capabilities`, `save_agent`, `save_preset`, …
- Deploy / projects / drift (~18): `preview_deploy`, `execute_deploy`, `detect_drift`, `accept_drift`, `restore_drift`, …
- Analytics + provider tokens (~14): `get_all_provider_status`, `get_provider_analytics`, `save_provider_token`, `get_tray_summary`, `force_refresh_provider`, …
- Secrets + agent memory (~15)
- Per-adapter config / rules / hooks / skills (~50)
- Claude V2 analytics, Cursor V2 analytics, Codex skills, Gemini extensions, etc.

There are **no benchmark commands** today. Greenfield namespace.

### Capability data model

`src-tauri/src/models/capability.rs` already gives us typed building blocks for "config under test":

- `enum CapabilityType { Mcp, Rule, Skill, Hook, Plugin, Custom }`
- `struct McpServer { id, name, transport, command, args, env: HashMap<String, EnvVariable>, tool_list, … }`
- `struct Skill { id, files: Vec<SkillFile>, env, … }`
- `struct AgentDefinition { name, model, temperature, max_tokens, tools: ToolAccess, memory: MemoryScope }` (`src-tauri/src/models/agent.rs`)
- `enum MemoryScope { Global, ProjectLocal, ContextWindow }`

Benchmark variants ("skill on vs off", "two MCP stacks", "agent A vs agent B") fall out for free if we make a `BenchmarkConfig` reference these types by `CompositeId` rather than duplicating them.

### Secrets / token pipeline

`src-tauri/src/analytics/token_store.rs:65-92`:

- Tokens are stored in `~/.agentharbor/provider-tokens.json` (chmod 0600) with keychain as optional backup.
- An in-memory `TOKEN_CACHE` avoids repeated file reads and *avoids OS keychain prompts on every call*.
- `get_provider_token(provider_id, key_type)` is the single entry point.

This is gold for a benchmark runner: it can fan out 100 requests across providers without prompting the user once. The same store services analytics, deploy, and benchmark — no new secrets infra needed.

### Cost engine

`src-tauri/src/analytics/cost_engine.rs:40-82` hardcodes pricing for ~5 Claude SKUs plus a Sonnet-default fallback. The frontend mirror lives at `src/data/model-costs.json` (loaded by `src/lib/modelCosts.ts`). No model context-window catalog exists; the 200K Sonnet tier is implicit in the tiered-pricing logic.

This is the single biggest piece of technical debt the benchmark lab will exercise: pricing tables go stale fast (~weekly) and AgentHarbor will be uselessly wrong without a dynamic source. See §6 for the fix.

### Drift / backup utilities

`src-tauri/src/utils/drift.rs` already hashes deployed files (`compute_hash`, `DeployState`, `save_deploy_state`). The benchmark store can reuse this snapshot pattern for "config snapshot at run time" so historical runs stay reproducible even after the user changes their `.claude/` or `.cursor/` configs.

### Tests

`npm run test:regression` = `tsc && vite build && cargo test`. Rust unit tests exist for adapters and the cost engine (~13 tests). No frontend tests. No E2E framework. No CI workflows in `.github/`. We are starting clean on test infrastructure for benchmarks.

---

## 1b. Alignment with `dan_features.md` and current WIP

`dan_features.md` already lays out the product direction and a 10-item next-implementation list. This doc does **not** propose a parallel track; it adds depth (research citations, concrete Rust types, command signatures, short-duration backlog) on top of the same plan.

### Current in-progress state (as of 2026-05-15)

- `src-tauri/src/commands/mod.rs` has an **uncommitted** `pub mod benchmarks;` declaration (see local `git status`). The matching `src-tauri/src/commands/benchmarks.rs` file has not been created yet — the Rust module skeleton is being scaffolded right now. **Do not touch this from another session** until the author commits the initial skeleton; coordinating writes across uncommitted changes is how merge conflicts and accidental reverts happen.
- No `BenchmarkLabPage.tsx`, `benchmarkStore.ts`, or `models/benchmark.rs` yet. The 10-item list in `dan_features.md` is the canonical task ordering; this doc proposes additions rather than reordering.

### Mapping `dan_features.md` § "Suggested next implementation tasks" → this doc

| `dan_features.md` task | This doc adds |
|---|---|
| 1. Audit production-ready vs partial providers | §1 audit table + §4 P0 (surface hidden providers) |
| 2. Add Benchmark Lab to roadmap and navigation | §3.5 UI layout; sidebar insertion above `Notes` |
| 3. Normalized benchmark types in TS + Rust | §3.1 concrete Rust struct signatures, mirrored TS via existing type-gen path |
| 4. Single-prompt × many-models MVP via stored keys | §3.2 Tauri commands (`start_benchmark_run`), §3.3 runner topology |
| 5. Per-run metrics: tokens / cost / latency / context | §3.1 `Usage` field on every `BenchmarkRunItem`; §4 P1 dynamic model catalog (so cost is correct) |
| 6. Prompt variant comparison | Falls out of `BenchmarkConfig.system_prompt` + multi-config columns |
| 7. Skill/plugin toggles for overhead and quality impact | `BenchmarkConfig.skills/rules/mcp_servers` reference existing `CompositeId`s + §4 P3 token-diff |
| 8. Dataset import/export + saved run history | §3.6 persistence (SQLite index + per-run dir); commands `import_benchmark_dataset`, `export_benchmark_run` |
| 9. Rubric scoring + pairwise judge | §3.4 scoring layers; §2 judge research (pointwise default, balanced permutation, ≥2 judges) |
| 10. E2E tests with mocked providers + deterministic fixtures | §7 test plan (mocked `Provider` trait, fixture datasets, `cargo insta` snapshots, `Clock` injection) |

### Additions this doc proposes beyond `dan_features.md`

These are net-new and do **not** appear in the original 10:

- CI regression workflow (`.github/workflows/regression.yml`) — independent prerequisite.
- **P1 — Dynamic model catalog** (replaces hardcoded pricing at `src-tauri/src/analytics/cost_engine.rs:40-82`). Implicit in `dan_features.md` Opportunity B but not in the numbered list. Without it, every cost number in the app rots within weeks.
- **P3 — Token-diff CLI / panel.** No web tool ships this. Strongest desktop wedge.
- **P4 — MCP tool latency profiler.**
- **P5 — Deploy regression detector.** Composition of P3 + a sentinel benchmark; depends on Lab MVP.
- **P6 — Per-project cost budgets and alerts.**
- **P7 — Ollama provider.** Local lane; free comparison column.
- **P8 — Contamination overlay.** Warns when a model's training cutoff is later than a benchmark's release date.
- Inspect AI subprocess bridge (Phase 4).
- 2025–2026 judge-bias mitigations baked into defaults.

### Convention: numbering authority

When this doc says "PR #N" or "Phase N", that is local to this doc. When it says "dan_features.md task #N", that is the canonical 10-item list. Anything that ships should reference the `dan_features.md` task number in the commit message so progress is visible on both docs.

---

## 2. State of the eval landscape (May 2026)

Highlights from a fresh research pass. URLs and arXiv ids are inlined for citation hygiene.

### Coding & agent benchmarks worth tracking

- **SWE-bench family.** Verified is now widely treated as contaminated. OpenAI moved reporting to **SWE-bench Pro** (private split, Scale-hosted; frontier ~46% vs ~81% on Verified). **SWE-bench Multimodal** (Jan 2025, private test split, `sb-cli` submission) and **SWE-bench Live** (NeurIPS 2025 D&B, Microsoft — 50 fresh GitHub issues monthly) are the contamination-resistant ones. Repos: `swebench.com`, `github.com/microsoft/SWE-bench-Live`, `labs.scale.com/leaderboard/swe_bench_pro_public`.
- **Aider Polyglot** (`aider.chat/docs/leaderboards/`) — 225 Exercism problems × 6 languages, two-attempt with error feedback. Fully open and scriptable, ideal as an embedded smoke test.
- **LiveCodeBench** (`livecodebench.github.io`) — date-stamped problems filtered by model cutoff; contamination-free by construction.
- **Terminal-Bench 2.0** (`tbench.ai/leaderboard/terminal-bench/2.0`) — 89 shell tasks; directly maps onto Claude Code / Codex agent runs.
- **τ-bench / τ²-bench** (arXiv:2406.12045, arXiv:2506.07982) — multi-turn tool use with policy/Dec-POMDP twists. SOTA function-callers still <50%. MIT harness, embeddable.
- **GAIA2 + ARE** (arXiv:2509.17158, `huggingface.co/blog/gaia2`) — 1,120 multimodal assistant scenarios on Meta's open Agent Reasoning Environment. Closest analogue to AgentHarbor's day-to-day workloads.
- **BFCL V4** (`gorilla.cs.berkeley.edu/leaderboard.html`) — function-calling leaderboard now with web-search and memory tasks (ICML 2025 paper).
- **METR HCAST / Time-Horizon 1.1** (`metr.org/blog/2026-1-29-time-horizon-1-1/`) — long-horizon task lengths, doubling ~7 months.
- **ARC-AGI-2** (`arcprize.org/arc-agi/2`) — cost-per-task is a first-class metric, which is a useful pattern for AgentHarbor to copy in its own UI.

> **Berkeley RDI, 2026** (`rdi.berkeley.edu/blog/trustworthy-benchmarks-cont/`) showed eight of the above can be gamed to near-perfect scores without solving tasks. **Public leaderboards are directional only.** The strongest argument for shipping a private, project-local eval lab.

### Open-source eval frameworks (the build-on candidates)

| Framework | License | Why it matters for us |
|---|---|---|
| **Inspect AI** (UK AISI, `inspect.aisi.org.uk`) | MIT | Gold standard. Solver/Scorer primitives, sandboxed Docker, Agent Bridge for LangChain / OpenAI Agents SDK / Pydantic AI, 200+ pre-built tasks in `inspect_evals`. VS Code log viewer is reusable. |
| **DeepEval** | Apache-2.0 | Pytest-style assertions, CI/CD gating, 40+ metrics. |
| **lm-evaluation-harness** (EleutherAI) | MIT | Canonical academic harness behind Open LLM Leaderboard 2. |
| **RAGAS** | Apache-2.0 | RAG metrics (faithfulness, context precision/recall). |
| **Arize Phoenix** | Elastic v2 | Observability + eval, Apr 2025 prompt-versioning. |
| **Langfuse** | MIT (core) | OTEL-native tracing + prompt mgmt; self-hosts via Docker. |
| **Helicone** | Apache-2.0 | 100+ providers, proxy model, 50–80 ms overhead. |
| **OpenLLMetry** | Apache-2.0 | OpenTelemetry semantic conventions for LLMs. |
| **W&B Weave** | Apache-2.0 client | Eval + tracing, W&B integration. |

For a desktop Tauri app, the most pragmatic stack is **Inspect AI as the eval engine + OpenLLMetry/Langfuse for tracing + a DeepEval-style assertion DSL in Rust/TS for inline checks**. Inspect runs as a Python subprocess; the Rust backend can spawn it as a child process, stream logs through Tauri events, and persist results to the existing project store.

### Image-gen evaluation

- **VQAScore / GenAI-Bench** (Lin et al., ECCV 2024, arXiv:2404.01291, `linzhiqiu.github.io/papers/vqascore/`) — de-facto auto-metric in 2025; adopted by Google (Imagen 3/4), ByteDance Seed, NVIDIA. 1,600 compositional prompts + 15K human ratings.
- **T2I-CompBench++** (TPAMI Jan 2025, arXiv:2307.06350) — 8K prompts, 4 categories, detection-based scoring for 3D-spatial + numeracy.
- **GenEval** — object-detector-based; saturating.
- **GenColorBench** (arXiv:2510.20586, Oct 2025) — color fidelity.
- **T2I-CoreBench** (`t2i-corebench.github.io`) — "set the stage" narrative tests.

VQAScore is by far the most embed-friendly; the others either need humans or saturate fast.

### LLM-as-judge updates (2025–2026)

- arXiv:2406.07791 — position bias in pairwise judging; **balanced permutation** is now SOTA.
- arXiv:2504.14716 — pairwise flips on 35% of cases vs 9% for absolute scores. **Prefer pointwise rubrics**; reserve pairwise for human-in-loop calibration.
- arXiv:2506.22316 — scoring bias.
- arXiv:2604.23178 (2026) — multi-judge ensembles + CoT + calibrated rubrics improve human correlation.

Practical default for Benchmark Lab: pointwise rubric + balanced permutation + 3-judge ensemble across model families + Spearman against a small human-labeled gold set.

### Contamination-aware leaderboards to embed

- **LiveBench** (ICLR 2025, arXiv:2406.19314) — monthly question rotation.
- **MixEval / MixEval-Hard** (`mixeval.github.io`) — 0.96 ranking correlation with Chatbot Arena.
- **Arena-Hard / Arena-Hard-Auto** (arXiv:2406.11939) — auto-curated hard prompts.
- **Scale SEAL** (`scale.com/leaderboard`) — private, expert-graded.
- **SWE-Bench Pro** — replaces Verified.

### Local model inference (2026 numbers)

Empirical comparison (arXiv:2511.05502):

| Hardware | Model | Tokens/sec |
|---|---|---|
| M4 Max | Llama 3.3 8B Q4_K_M | Ollama ~55, LM Studio MLX ~57 |
| M2 Pro | Llama 3.1 8B Q4_K_M | llama.cpp 38–48, MLX 45–58 |
| RTX 4090 | Qwen3 14B Q5_K_M | Ollama 85, LM Studio 84 |

MLC-LLM wins on long-context (paged KV cache). For AgentHarbor: **Ollama is the default** (ubiquitous, OpenAI-compatible API, big model catalog), with LM Studio (MLX) recommended for Apple Silicon power users and llama.cpp as a fallback.

### Token / cost analyzers

- **tiktoken** — exact for OpenAI only.
- **Anthropic Token Count API** — only ground-truth source post-Claude-3 (free).
- **AgentOps `tokencost`** — multi-provider counts + cost via Anthropic's count API.
- **LiteLLM** — unified token counter; gaps tracked at `BerriAI/litellm#312`.
- **simonw/llm-prices** (`llm-prices.com`) — community price table, machine-readable.
- **Artificial Analysis** (`artificialanalysis.ai/models`) — now includes **Cache Hit Price**.

There is no widely adopted "token diff between configs" tool. Clear product gap — see §4 feature P3.

### Provider gateways

- **LiteLLM** (MIT, self-host) — most embeddable; we could ship the LiteLLM proxy in-process for unified routing.
- **OpenRouter** — single API, 300+ models, transparent markup.
- **Portkey** — strongest telemetry/replay model; worth emulating.
- **Vercel AI Gateway** / **Cloudflare AI Gateway** — closed but free-tier friendly.

---

## 3. Benchmark Lab — concrete architecture

Building on the `dan_features.md` MVP, refined against the audit and research above.

### 3.1 Data model (Rust + mirrored TS)

```rust
// src-tauri/src/models/benchmark.rs (new)

pub struct BenchmarkDataset {
    pub id: CompositeId,
    pub name: String,
    pub description: String,
    pub category: BenchCategory,         // Coding | ToolUse | Extraction | Writing | Image | Custom
    pub cases: Vec<BenchmarkCase>,
    pub source: DatasetSource,           // Bundled | LocalFile | RegistrySync | Inspect(task_id)
    pub version: String,
    pub created_at: DateTime<Utc>,
    pub provenance: Provenance,          // hash of files, git sha when synced
}

pub struct BenchmarkCase {
    pub id: String,
    pub prompt: PromptSpec,
    pub expected: Vec<Assertion>,         // structural; judges live separately
    pub tools: Vec<ToolRef>,              // MCP server refs by CompositeId
    pub max_turns: Option<u32>,
    pub timeout_ms: u64,
    pub tags: Vec<String>,
}

pub struct BenchmarkConfig {
    pub model: ModelRef,                  // provider + model id + version
    pub system_prompt: Option<String>,
    pub skills: Vec<CompositeId>,         // refs into existing Skill registry
    pub rules: Vec<CompositeId>,
    pub mcp_servers: Vec<CompositeId>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub agent: Option<CompositeId>,       // optional AgentDefinition ref
    pub config_hash: String,              // sha256 of resolved config (drift-aware)
}

pub struct BenchmarkRun {
    pub id: Uuid,
    pub dataset_id: CompositeId,
    pub dataset_version: String,
    pub configs: Vec<BenchmarkConfig>,    // each column in the comparison grid
    pub items: Vec<BenchmarkRunItem>,     // dataset.cases.len() × configs.len()
    pub judge: Option<JudgeConfig>,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: RunStatus,                // Pending | Running | Completed | Cancelled | Failed
    pub env_snapshot: EnvSnapshot,        // OS, AgentHarbor version, adapter versions, model catalog rev
}

pub struct BenchmarkRunItem {
    pub case_id: String,
    pub config_idx: usize,
    pub raw_request: serde_json::Value,
    pub raw_response: serde_json::Value,
    pub normalized_text: String,
    pub structured_output: Option<serde_json::Value>,
    pub usage: Usage,                     // prompt/completion/cache tokens, ms, $ estimate
    pub tool_calls: Vec<ToolCall>,
    pub assertion_results: Vec<AssertionResult>,
    pub judge_results: Vec<JudgeResult>,
    pub user_rating: Option<UserRating>,
    pub error: Option<RunError>,
}
```

Every item is fully inspectable. Raw responses are stored, not just scores. Scoring is layered (assertions → judge → human rating) so each layer can be re-run without re-spending tokens.

### 3.2 Tauri commands

```
src-tauri/src/commands/benchmarks.rs (new)
  list_benchmark_datasets        // bundled + user-imported + registry-synced
  get_benchmark_dataset(id)
  import_benchmark_dataset(path | url)
  list_benchmark_providers       // pulls from existing token_store + model catalog
  list_benchmark_models(provider)
  estimate_benchmark_cost(dataset_id, configs)   // dry-run, no API calls
  start_benchmark_run(dataset_id, configs, judge)
  cancel_benchmark_run(run_id)
  get_benchmark_run(run_id)      // includes streaming progress
  list_benchmark_runs(filter)
  rerun_failed_items(run_id)
  rerun_with_new_judge(run_id, judge)
  rerun_for_drift(run_id)        // re-execute against current config state, diff cost+quality
  export_benchmark_run(run_id, format)   // JSON, CSV, Markdown report
  delete_benchmark_run(run_id)
```

Streaming uses existing Tauri event channel (`tauri::Manager::emit_all`). Each `BenchmarkRunItem` completion emits a `benchmark://progress` event; the frontend appends incrementally rather than polling.

### 3.3 Runner topology

```
Frontend (Zustand benchmarkStore)
      │
      ▼   Tauri command
Rust dispatcher  ──►  Provider router (reuse src-tauri/src/analytics adapters)
      │                    │
      │                    ├─► Anthropic / OpenAI / Google / OpenRouter / Vertex
      │                    └─► Local: Ollama / LM Studio / llama.cpp
      │
      ├─►  Judge worker pool (separate from generation pool, different rate limits)
      ├─►  Inspect AI bridge (optional, spawned per Inspect task)
      └─►  Storage: ~/.agentharbor/benchmarks/{run_id}/items/*.json
                    + SQLite index for fast list/filter
```

All API calls go through the Rust backend, reusing `token_store` and `cost_engine`. Renderer never sees keys — matches the user's "Rust backend" preference and the existing security posture.

### 3.4 Scoring layers (in order)

1. **Structural assertions** (exact-match, regex, JSON schema, contains, AST equality for code) — free, run inline in Rust.
2. **Programmatic graders** (compile + run for code tasks, sandboxed via existing `utils/process.rs` or a new container shim; pytest-style for Inspect tasks).
3. **LLM-as-judge** — pointwise rubric, balanced permutation, ≥2 judge models, position-bias-aware. Default judges: `claude-opus-4-7` + `gemini-2-pro` + `gpt-5.2` (any 2 the user has tokens for).
4. **Human review UI** — diff-style side-by-side, keyboard-driven (`j`/`k` navigate, `1`–`5` rate, `c` comment). Compact for quick passes through 100+ items.

Spearman vs human ratings is reported per judge over time so the user can tell when a judge model drifts.

### 3.5 UI layout

```
src/pages/BenchmarkLabPage.tsx (new)
src/stores/benchmarkStore.ts (new)
src/components/benchmarks/
  DatasetBrowser.tsx
  RunSetupPanel.tsx        // configs as columns, drag-in skills/MCPs/rules
  LiveGrid.tsx             // streaming results; rows=cases, cols=configs
  MetricsRail.tsx          // tokens / $ / latency / context % per column
  JudgePanel.tsx
  RunHistoryList.tsx
  CostEstimateModal.tsx
  TokenDiffViewer.tsx      // see §4 P3
  HumanRatingDrawer.tsx
```

Sidebar: insert `Benchmark Lab` above `Notes` in `src/components/layout/Sidebar.tsx`.

### 3.6 Persistence

- Runs: `~/.agentharbor/benchmarks/{run_id}/` (one dir per run; manifest + item JSONs).
- Index: `~/.agentharbor/benchmarks/index.sqlite` (rusqlite; existing dep candidate). Schema: `runs(id, dataset_id, started_at, status, cost_usd, config_hashes)`, `items(run_id, case_id, config_idx, score, latency_ms, tokens, $)`.
- Datasets bundled in `registry/benchmarks/` and synced via the existing git-based registry loader (`src-tauri/src/registry/loader.rs`).

### 3.7 What we explicitly do not do in MVP

- No multi-tenant cloud. Local only.
- No human-evaluator gig marketplace.
- No automated "your model is best" headline — only category-specific rollups with explicit caveats.
- No live calls in CI — all tests mock providers (see §7).

---

## 4. Short-duration features that ship before the Lab

Ordered by ROI / effort. Each is its own PR. None require waiting on the full Lab.

### P0 — Surface the hidden providers (≤1 day)

The Rust backend already knows 16 providers; the README advertises 5. Expose Augment, Amp, OpenRouter, Vertex AI, Kimi, z.ai, JetBrains, Droid, Kiro in:

- `src/components/layout/Sidebar.tsx` adapters section (gated by a `experimental: true` flag where appropriate)
- `src/components/analytics/ProviderConnectModal.tsx` provider picker
- `docs/features.md` and the README provider table

Mark experimental vs GA explicitly. Closing this gap is the cheapest credibility boost AgentHarbor can ship.

### P1 — Dynamic model catalog (≤2 days)

Replace `src-tauri/src/analytics/cost_engine.rs:40-82` and `src/data/model-costs.json` with a daily-refreshed catalog:

- Sources (in priority order): Anthropic `/v1/models`, OpenAI `/v1/models`, Google `models.list`, OpenRouter `/api/v1/models`, `simonw/llm-prices` JSON dump, `artificialanalysis.ai` (manual pin if no public API).
- Normalize to a single `ModelCatalogEntry { id, provider, family, context_window, input_price, output_price, cache_hit_price, cache_write_price, release_date, deprecation_date?, capabilities: [text, vision, audio, tools, prompt_cache] }`.
- Cache at `~/.agentharbor/model-catalog.json` with `etag` + 24h staleness.
- Show "catalog updated 4 hours ago" badge in settings.

Without this, every other cost number in the app rots within weeks.

### P2 — Prompt-caching observability panel (≤2 days)

Anthropic prompt cache + OpenAI cached-input pricing are now distinct line items. Surface per-project:

- Cache hit ratio (last 7d / 30d)
- Cache savings (USD, vs no-cache baseline)
- Worst cache-miss prompts (top 10 by wasted spend)
- Cache-key churn detector — flag system prompts that change too often to cache

Source: existing `claude_v2.rs` and `cursor_v2.rs` analytics already surface usage rows; just add cache-aware aggregation.

### P3 — Token-diff CLI / panel (≤2 days)

Tool nobody ships yet: given two config snapshots (system prompts, skill set, MCP defs, rules), produce a per-provider-tokenizer diff:

```
$ harbor token-diff configs/before.json configs/after.json --model claude-sonnet-4-7
  Skill "code-reviewer/SKILL.md"        +1,247 tokens   ($0.00374/run × 850 runs/day = $3.18/day)
  MCP "filesystem".tool_list             +488 tokens   ($0.00146/run × 850 runs/day = $1.24/day)
  System prompt (Cursor 0.46→0.47)        −62 tokens   (−$0.00019/run)
  Net per request: +1,673 tokens         (+$5.02/day at current usage)
```

Implementation: re-use existing `EnvVariable`/`Skill`/`McpServer` types; tokenize via Anthropic Token Count API (free) for Claude, tiktoken for OpenAI, Gemini `countTokens` for Google. Surface in a `TokenDiffViewer.tsx` modal callable from the deploy preview and from the benchmark lab.

This is **the** product wedge against web competitors — no SaaS sees the user's installed configs.

### P4 — MCP tool latency profiler (≤2 days)

For each connected MCP server, record p50/p95 latency and token cost per tool call across the user's day-to-day agent runs. Flag the "fat tool" stealing wall-clock or tokens. Implementation: hook into existing transcript ingestion (`src-tauri/src/analytics/claude_v2.rs` already parses message logs).

### P5 — "What changed since last deploy" cost-regression detector (≤3 days)

After any deploy (existing `record_deployment` command fires), enqueue a small sentinel benchmark — 5–10 saved cases — and report delta vs the previous deploy's run. Three lines per metric: latency, tokens, judge score. Optional auto-rollback hook.

Reuses Benchmark Lab primitives once they exist; until then, ship a stub that just runs token-diff (P3) automatically.

### P6 — Cost budgets and alerts per project (≤1 day)

`projects` already exist (`src/stores/projectStore.ts`). Add a `monthly_budget_usd` field, tally against existing usage data, fire a system notification at 80% / 100%. Trivial; satisfies common enterprise ask.

### P7 — Local model lane via Ollama (≤2 days)

Add `Ollama` as a provider in `src-tauri/src/analytics/ollama.rs` (new). No tokens needed; auto-detect `http://localhost:11434`. List installed models, surface in benchmark provider picker, count tokens via `/api/tokenize`. Free comparison lane for every benchmark run.

### P8 — Contamination overlay on bundled benchmarks (≤1 day)

When a user selects a public benchmark in the Lab, look up the benchmark release date and warn if their model's training cutoff is later. Wire LiveBench / SWE-bench Live metadata into the catalog.

---

## 5. Differentiators vs web platforms

Five things only a Tauri desktop app can do credibly. These should drive marketing copy as much as engineering priorities.

1. **Filesystem-aware context.** AgentHarbor can read the user's real `~/.claude/`, `.cursorrules`, MCP configs, installed skills. It can answer "this MCP server adds 12.4K tokens to every Claude request and costs you $3.18/day." Promptfoo Cloud, LangSmith, and Braintrust cannot.
2. **Local secret handling.** Keys never leave the keychain. Compliance-friendly (SOC 2 / HIPAA shops who refuse third-party LLM proxies).
3. **Offline / air-gapped runs.** Ollama or MLX local lane with zero network. Gov + enterprise sweet spot.
4. **Drift-aware re-runs.** After a `claude` or `cursor` CLI upgrade, the bundled system prompt may change byte-for-byte. AgentHarbor can detect (`utils/drift.rs` already hashes files) and auto re-run a sentinel benchmark, flagging cost/quality regressions. No SaaS sees the upgraded binary.
5. **Cross-agent runtime matrix.** Same task fanned out across Claude Code *as a real CLI invocation*, Cursor agents, Codex, Gemini CLI, and local models — wall-clock + cost + token usage + diff quality side-by-side. Promptfoo fans out prompts; it cannot fan out *agent runtimes* with real shells.

---

## 6. Evaluation principles (hard rules)

Borrowed from `dan_features.md` and sharpened with 2026 research:

1. Separate **public** benchmark scores from **private** benchmark scores. Never average them.
2. Keep raw outputs and per-judge scores inspectable. No black-box headline numbers.
3. Never hide cost or context trade-offs behind a single quality number. Always show the cost-quality scatter.
4. Normalize provider usage metrics but preserve provider-native fields verbatim alongside.
5. Pointwise rubrics by default; pairwise only with balanced permutation and ≥2 judge models.
6. Datasets are versioned and content-hashed; runs reference dataset versions, not mutable names.
7. Treat public leaderboards as directional only (per RDI 2026). Surface contamination warnings inline.
8. Show the model catalog freshness on every run — stale pricing = useless cost numbers.
9. Local models are first-class; they get the same UI columns as hosted models.

---

## 7. Test plan

E2E in CI must never hit live providers. Use:

- **Mocked provider adapters** behind the existing `analytics::Provider` trait — return canned `Usage` + canned text.
- **Fixture datasets** in `registry/benchmarks/fixtures/` (5 cases each, deterministic).
- **Snapshot benchmarks** — `cargo insta` or `expect-test` for run-item JSON.
- **Time control** — inject `Clock` into runner for deterministic timestamps.

Required E2E flows:

1. Connect provider token → assertion: stored, retrievable, never logged.
2. Run one prompt × two mocked models → assertion: 2 items, correct usage tallies.
3. Compare prompt-A vs prompt-B (same model) → assertion: token diff matches expectation.
4. Compare skill-off vs skill-on → assertion: config_hash differs, tokens differ, judge invoked.
5. Cancel mid-run → assertion: partial items persisted, status=Cancelled.
6. Re-run failed items only → assertion: only failed items get new attempts.
7. Cost-regression after deploy → assertion: delta report rendered, notification fired.
8. Export run → JSON / CSV / Markdown report round-trips through the importer.

Add GitHub Actions workflow `.github/workflows/regression.yml` running `npm run test:regression` on push to main + PRs. Currently there is no CI at all — this is independent and should land before the Lab work.

---

## 8. Rollout phases

| Phase | Deliverable | Effort | Depends on |
|---|---|---|---|
| 0 | P0 — expose hidden providers | 1 day | — |
| 0 | P1 — dynamic model catalog | 2 days | — |
| 0 | CI workflow (regression) | 0.5 day | — |
| 1 | Benchmark data model + Tauri commands (no UI) | 3 days | P1 |
| 1 | P7 — Ollama provider | 2 days | P1 |
| 1 | P3 — token-diff panel (standalone) | 2 days | P1 |
| 2 | Benchmark Lab MVP UI: dataset browser, run setup, live grid, metrics rail | 5 days | Phase 1 |
| 2 | Assertions (exact/regex/schema/contains/AST) | 2 days | Phase 1 |
| 2 | Bundled dataset packs (coding × 1, extraction × 1, writing × 1) | 1 day | Phase 1 |
| 3 | LLM-as-judge (pointwise, balanced permutation, ≥2 judges) | 3 days | Phase 2 |
| 3 | Human rating drawer (keyboard-driven) | 2 days | Phase 2 |
| 3 | Pairwise comparison mode | 2 days | Phase 3 judge |
| 4 | Inspect AI bridge (subprocess + log streaming) | 3 days | Phase 2 |
| 4 | LiveBench / SWE-bench Live importers | 2 days | Phase 4 inspect |
| 4 | P8 — contamination overlay | 1 day | Phase 4 importers |
| 5 | P2 — prompt-caching observability | 2 days | P1 |
| 5 | P4 — MCP tool latency profiler | 2 days | — |
| 5 | P5 — deploy regression detector | 3 days | Phase 2 + P3 |
| 5 | P6 — project cost budgets | 1 day | — |
| 6 | Image-gen lane (VQAScore + T2I-CompBench++ fixtures) | 5 days | Phase 3 |
| 6 | Local MLX / llama.cpp providers (alongside P7 Ollama) | 3 days | P7 |

Total to a usable, opinionated Benchmark Lab + 8 short-duration shipped features: ~30 engineering days. Phases 0–2 alone (~14 days) are enough for an internal release.

---

## 9. Risks and unknowns

- **Provider terms-of-service.** Some providers prohibit using their model to evaluate another provider's output. The judge selector must surface this; default to same-provider judges when in doubt.
- **Cost runaway.** A 10-config × 100-case run with judges can hit $50+. The `estimate_benchmark_cost` dry-run is non-negotiable; default to *require confirmation* over $5.
- **Tokenizer drift.** Anthropic count API is the only ground truth for Claude; OpenAI tiktoken stays accurate for OpenAI; Gemini `countTokens` for Google. Don't hand-roll approximations — they will be wrong.
- **Sandboxing.** Programmatic graders that run model-generated code need a real sandbox. macOS sandbox-exec works locally; Windows is harder. Defer to Phase 2 if a clean sandbox isn't ready.
- **Inspect AI versioning.** Inspect's API has changed across 2025; pin a tested version and check the changelog every release.
- **Public benchmark licenses.** SWE-bench Pro is private; we link, we do not redistribute. Bundled fixtures must be cleared individually (MIT / Apache / CC-BY at minimum).

---

## 10. References

### Provider eval docs

- OpenAI — [Compare models](https://developers.openai.com/api/docs/models/compare), [Evaluation best practices](https://developers.openai.com/api/docs/guides/evaluation-best-practices), [Evaluate agent workflows](https://developers.openai.com/api/docs/guides/agent-evals).
- Anthropic — [Evaluation Tool](https://platform.claude.com/docs/en/test-and-evaluate/eval-tool), [Models overview](https://platform.claude.com/docs/en/about-claude/models/overview).
- Google Vertex AI — [Evaluation overview](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/models/evaluation-overview), [Gemini models](https://ai.google.dev/gemini-api/docs/models).

### Benchmarks

- SWE-bench: `swebench.com`, `github.com/microsoft/SWE-bench-Live`, `labs.scale.com/leaderboard/swe_bench_pro_public`
- Aider Polyglot: `aider.chat/docs/leaderboards/`
- LiveCodeBench: `livecodebench.github.io`
- Terminal-Bench 2.0: `tbench.ai/leaderboard/terminal-bench/2.0`
- τ-bench / τ²-bench: arXiv:2406.12045, arXiv:2506.07982
- GAIA / GAIA2: `huggingface.co/blog/gaia2`, arXiv:2509.17158
- BFCL V4: `gorilla.cs.berkeley.edu/leaderboard.html`
- METR Time-Horizon 1.1: `metr.org/blog/2026-1-29-time-horizon-1-1/`
- ARC-AGI-2: `arcprize.org/arc-agi/2`
- LiveBench: arXiv:2406.19314
- MixEval: `mixeval.github.io`
- Arena-Hard-Auto: arXiv:2406.11939
- Scale SEAL: `scale.com/leaderboard`
- RDI trustworthy benchmarks 2026: `rdi.berkeley.edu/blog/trustworthy-benchmarks-cont/`

### Eval frameworks

- Inspect AI: `inspect.aisi.org.uk`, `github.com/UKGovernmentBEIS/inspect_ai`, `github.com/UKGovernmentBEIS/inspect_evals`
- DeepEval: `deepeval.com`
- lm-evaluation-harness: `github.com/EleutherAI/lm-evaluation-harness`
- Langfuse: `langfuse.com`
- Phoenix: `arize.com/phoenix`
- Helicone: `helicone.ai`
- OpenLLMetry: `github.com/traceloop/openllmetry`
- Promptfoo: `promptfoo.dev`
- ChainForge: `chainforge.ai`

### Image-gen

- VQAScore: arXiv:2404.01291, `linzhiqiu.github.io/papers/vqascore/`
- T2I-CompBench++: arXiv:2307.06350, `karine-h.github.io/T2I-CompBench/`
- GenColorBench: arXiv:2510.20586

### Judge research

- Position bias pairwise: arXiv:2406.07791
- Pairwise vs pointwise: arXiv:2504.14716
- Scoring bias: arXiv:2506.22316
- Bias mitigation (2026): arXiv:2604.23178
- Judge survey: arXiv:2411.15594

### Token / pricing

- Artificial Analysis: `artificialanalysis.ai/models`
- simonw/llm-prices: `github.com/simonw/llm-prices`
- AgentOps tokencost: `pypi.org/project/tokencost/`

### Local inference

- MLX vs Ollama vs llama.cpp benchmark: arXiv:2511.05502

---

## 11. Immediate next steps

The next PRs, in order, that an engineer can pick up without further design discussion:

1. PR #1 — surface hidden providers + experimental flag (P0). Touches sidebar, provider modal, docs.
2. PR #2 — GitHub Actions regression workflow. Wires `npm run test:regression` on PR + main.
3. PR #3 — dynamic model catalog (P1). New `src-tauri/src/analytics/model_catalog.rs`, delete the hardcoded `get_raw_pricing` and `model-costs.json`, settings UI for refresh.
4. PR #4 — `BenchmarkDataset` / `BenchmarkRun` Rust types + Tauri command stubs returning fixtures. No UI.
5. PR #5 — token-diff panel (P3). Standalone, callable from deploy preview today.
6. PR #6 — Ollama provider adapter (P7). Local lane goes live.
7. PR #7 — Benchmark Lab MVP UI: dataset browser, run setup, live grid, metrics rail. Live against PR #4.

Each PR ships independently and adds value on its own. The Lab does not block the short-duration wins, and the short-duration wins do not block the Lab.
