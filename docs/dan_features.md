# Dan Features & Benchmarking Roadmap

This document captures a practical roadmap for expanding AgentHarbor into a stronger multi-provider analytics and benchmarking desktop app.

It is based on:

- the current AgentHarbor codebase and docs
- the provider/eval features already exposed by OpenAI, Anthropic, and Google
- existing prompt/model comparison patterns from tools like Promptfoo and ChainForge
- open benchmark families that are useful as reference signals rather than product truth

## Current Benchmark Lab workflow

The current Benchmark Lab direction should stay focused on private, runnable comparisons instead of generic leaderboards.

### Run a live benchmark

1. Open `Benchmark Lab`.
2. In `Credentials`, save an OpenAI and/or Anthropic benchmark API key.
3. Click `Refresh Models` after saving the key so Benchmark Lab loads the real models available to that key.
4. In `Targets`, switch the provider away from `Mock` and choose one of the fetched live models.
5. Add one or more cases and run the benchmark.

If a target is left on `Mock`, that row is synthetic by design.

### Quick launch from reference benchmarks

Benchmark Lab now has a reference-benchmark quick-launch flow:

1. Open `Reference Benchmarks`.
2. Pick a benchmark family such as `SWE-bench`, `BrowseComp`, or `T2I-CompBench++`.
3. Select multiple live models from the right-hand launch panel.
4. Click `Launch Benchmark`.

That turns the reference family into a runnable private comparison instead of leaving it as a paper link only.

### Result views

Benchmark Lab should expose separate result surfaces instead of one overloaded panel:

- `Setup`: run summary, totals, and launch guidance
- `Compare`: side-by-side bars for tokens, cost, latency, and judge score
- `Responses`: rendered model responses plus raw output and manual review
- `Gallery`: image-generation comparison view with side-by-side artifacts

### GitHub strategy comparison

Benchmark Lab should support a repo-driven comparison flow for context-reduction and token-saving approaches:

1. Paste a GitHub repository URL that claims prompt compression, context reduction, retrieval pruning, or token savings.
2. Let AgentHarbor fetch the repo docs and extract the relevant strategy context and headline claims.
3. Run two variants side by side:
   - baseline
   - strategy-augmented challenger
4. Compare output quality, token usage, cost, latency, and judge score in one place.

This keeps the product grounded in a workflow users actually care about:

- "Does this repo-backed strategy reduce context and cost for my prompt?"
- "Does it keep quality the same or better?"
- "Is the token reduction worth the added complexity?"

## Implemented direction

The Benchmark Lab implementation should keep moving in this order:

- live provider keys for runnable benchmarks
- live model discovery from saved provider keys
- private datasets and run history
- baseline vs challenger comparison
- GitHub-linked strategy extraction for context-reduction experiments
- benchmark packs and richer reference views after the core loop is stable

## Short-term TODO

- add Gemini and OpenRouter live model discovery, not just seeded fallback catalogs
- add cancellation and partial-run resume
- add dataset export from the UI
- add a richer comparison overview with per-case baseline vs challenger output panes
- add explicit token delta, cost delta, and quality delta summaries in run history
- add stronger first-run onboarding and a sample live benchmark recipe
- add a curated set of strategy demo repos and benchmark templates
- add image-specific judging and better visual diff scoring for image benchmarks

## Current repo findings

AgentHarbor already has most of the plumbing needed for a benchmark lab:

- a Tauri + React desktop shell with native secrets/token handling
- a unified analytics store in [`src/stores/analyticsStore.ts`](../src/stores/analyticsStore.ts)
- a unified analytics page in [`src/pages/UnifiedAnalyticsPage.tsx`](../src/pages/UnifiedAnalyticsPage.tsx)
- provider token capture in [`src/components/analytics/ProviderConnectModal.tsx`](../src/components/analytics/ProviderConnectModal.tsx)
- backend provider analytics dispatch in [`src-tauri/src/analytics/commands.rs`](../src-tauri/src/analytics/commands.rs)

The repo also already contains backend analytics modules for more providers than the public docs currently advertise, including:

- OpenAI Codex
- Claude Code and Claude Desktop
- Cursor
- Gemini
- GitHub Copilot
- OpenRouter
- Vertex AI
- Kimi / Kimi K2
- z.ai
- Augment
- Amp
- Droid
- Kiro
- JetBrains

That makes the best near-term strategy clear:

1. expose and normalize the providers that already exist
2. add a benchmark runner on top of the existing provider/token infrastructure
3. treat public benchmark data as reference context, not as the only score users see

## Product direction

### 1. Benchmark Lab

Add a top-level Benchmark Lab where a user can run the same task across multiple models and compare:

- output quality
- latency
- input tokens
- output tokens
- cached tokens
- estimated cost
- context window usage
- tool calls
- pass/fail against structured assertions

This should support two core workflows:

- `one prompt -> many models`
- `many prompt or skill variants -> one or many models`

Recommended UX:

- left panel: dataset or single prompt editor
- top controls: provider/model picker, temperature, max tokens, tools on/off, skill/plugin toggles
- center grid: side-by-side results
- right rail: metrics, costs, context %, judge scores, and export

### 2. Prompt / Skill / Plugin comparison

The app should not stop at model comparison. A stronger AgentHarbor angle is configuration benchmarking:

- compare the same task with and without a skill
- compare two system prompts
- compare two instruction files
- compare two plugin stacks
- compare one model through direct API vs routed gateway providers

This fits the product much better than a generic benchmark viewer because AgentHarbor already manages capabilities, prompts, agent files, and project configs.

### 3. Context and cost observability

One of the most useful differentiators would be a normalized context and cost panel that shows:

- model context window
- actual prompt tokens
- actual completion tokens
- cache reads/writes when available
- estimated dollar cost
- percent of context consumed
- prompt overhead added by selected skills/plugins/instructions

This is especially important because users often care less about a model's abstract benchmark rank and more about:

- which model solves this task well
- how much context it burns
- how much it costs
- whether the added skill or plugin is worth the extra tokens

### 4. Built-in benchmark packs

Ship benchmark packs in three layers:

- `quick checks`: 5-20 prompts for fast model/prompt comparison
- `team packs`: saved private datasets for a project or org
- `reference packs`: curated public benchmark-inspired datasets

Suggested initial packs:

- coding: bug fix, refactor, test writing, repo Q&A
- structured extraction: JSON, CSV, schema validation
- research: web synthesis, citation quality, factual recall
- writing: summarization, rewrite, instruction following
- agentic/tool use: multi-step tasks with tool-call validation
- image generation: prompt fidelity, composition, text rendering

### 5. Public benchmark reference view

Public benchmarks should be presented as context, not as the final answer.

Good use:

- show benchmark families relevant to a task category
- explain what each benchmark actually measures
- let the user compare their private eval results against broad public trends

Bad use:

- pretend one global leaderboard determines the best model for all work
- mix unrelated benchmarks into one fake overall score

## Recommended architecture

### Frontend

Add:

- `src/pages/BenchmarkLabPage.tsx`
- `src/stores/benchmarkStore.ts`
- `src/components/benchmarks/*`

Recommended page sections:

- run setup
- dataset browser
- live comparison grid
- rubric/judge config
- run history
- export and share

Navigation:

- add a top-level sidebar item: `Benchmark Lab`
- optionally link it from the existing unified analytics page

### Backend

Add Rust commands under `src-tauri/src/commands/benchmarks.rs`:

- `list_benchmark_providers`
- `list_benchmark_models`
- `run_benchmark_case`
- `run_benchmark_suite`
- `score_benchmark_outputs`
- `save_benchmark_run`
- `list_benchmark_runs`
- `export_benchmark_run`

Keep provider execution behind a normalized interface so the UI does not care whether a result came from:

- direct OpenAI API
- Anthropic API
- Gemini API
- OpenRouter
- Vertex AI
- a future local model runner

### Data model

Suggested entities:

- `BenchmarkDataset`
- `BenchmarkCase`
- `BenchmarkRun`
- `BenchmarkRunItem`
- `ModelConfig`
- `PromptVariant`
- `CapabilityVariant`
- `JudgeConfig`
- `BenchmarkScore`

Each run item should persist:

- raw request settings
- normalized response text
- structured output if present
- token/cost metadata
- latency
- error state
- judge results
- user rating

## Immediate feature opportunities from the current codebase

### Opportunity A: expose the hidden multi-provider analytics surface

The Rust backend already knows about many providers that the main product story does not fully expose yet. Before building a benchmark lab, AgentHarbor should make that support more visible and consistent.

Immediate tasks:

- add missing provider surfaces to docs
- audit which providers are fully working vs experimental
- expose the unified analytics page in routing/navigation if it is intentionally supported
- normalize provider naming, icons, and connection instructions

### Opportunity B: dynamic model catalog

Do not hardcode model tables for context windows, pricing, and capabilities where avoidable.

Instead:

- fetch provider model metadata where official APIs/docs support it
- cache a normalized model catalog locally
- let the benchmark lab use that catalog for constraints, pricing, and context calculations

This matters because model lists, aliases, pricing, and context windows move quickly.

### Opportunity C: benchmark the configuration, not only the model

AgentHarbor should compare:

- prompt A vs prompt B
- skill on vs off
- plugin set A vs plugin set B
- agent file variant A vs B

That makes the app more useful than a plain model playground.

## Recommended rollout phases

### Phase 0: docs and product cleanup

- document actual supported/experimental providers
- document unified analytics
- decide what is GA vs hidden vs experimental
- define benchmark JSON schema and storage layout

### Phase 1: Benchmark Lab MVP

- one prompt across many models
- manual run mode
- result grid
- latency/tokens/cost/context usage
- CSV/JSON export

### Phase 2: Prompt and capability experiments

- compare multiple prompt variants
- compare selected skills/plugins/instructions
- add run history and saved experiment presets

### Phase 3: rubric-based evaluation

- exact-match and schema checks
- regex and substring checks
- model-as-judge scoring
- pairwise comparison mode

### Phase 4: benchmark packs

- built-in coding, research, extraction, and writing packs
- import from JSON/YAML datasets
- project-local private benchmark sets

### Phase 5: image/media benchmarking

- Gemini and OpenAI image model comparisons
- image prompt fidelity scoring
- composition/text rendering checks
- gallery compare view

## Evaluation design principles

The app should follow a few hard rules:

- separate public benchmark scores from private benchmark scores
- keep raw outputs and scores inspectable
- never hide cost/context tradeoffs behind a single number
- normalize provider metrics, but preserve provider-native fields too
- support both human review and automated judges
- make datasets versioned and reproducible

## Research notes

### Provider eval capabilities

- OpenAI explicitly distinguishes between public industry benchmarks and application-specific evals, and its eval docs point to evaluation against external models and continuous improvement workflows. See OpenAI model docs and eval guidance: [Compare models](https://developers.openai.com/api/docs/models/compare), [Evaluation best practices](https://developers.openai.com/api/docs/guides/evaluation-best-practices), and [Evaluate agent workflows](https://developers.openai.com/api/docs/guides/agent-evals).
- Anthropic's evaluation tool already supports side-by-side prompt comparison, quality grading, and prompt versioning. See [Anthropic Evaluation Tool](https://platform.claude.com/docs/en/test-and-evaluate/eval-tool).
- Anthropic's current public model overview is useful for context window and pricing normalization. See [Anthropic models overview](https://platform.claude.com/docs/en/about-claude/models/overview).
- Google Vertex AI's Gen AI evaluation service supports adaptive rubrics, pairwise metrics, and evaluation of third-party models. See [Vertex AI evaluation overview](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/models/evaluation-overview) and [evaluation API reference](https://docs.cloud.google.com/vertex-ai/generative-ai/docs/model-reference/evaluation).
- Google's Gemini docs now span text, image, video, speech, and agent models, which makes Gemini especially relevant for a future media benchmark lane. See [Gemini models](https://ai.google.dev/gemini-api/docs/models).

### Related product patterns

- Promptfoo uses matrix views for evaluating outputs across many prompts and models. See [Promptfoo intro](https://www.promptfoo.dev/docs/intro/).
- ChainForge focuses on prompt comparison and evaluator-driven prompt scoring/plotting. See [ChainForge compare prompts](https://www.chainforge.ai/docs/compare_prompts/).

### Context-reduction and token-saving references

These are the most relevant references for the GitHub strategy-comparison feature:

- Microsoft's [LLMLingua](https://github.com/microsoft/LLMLingua) and the linked LLMLingua / LongLLMLingua / LLMLingua-2 papers are the clearest mainstream examples of prompt compression with explicit cost and long-context framing.
- [Characterizing Prompt Compression Methods for Long Context Inference](https://arxiv.org/abs/2407.08892) is useful because it compares compression methods instead of treating any one method as universal truth.
- [Perception Compressor](https://arxiv.org/abs/2409.19272) is a good training-free reference for retrieval plus selective compression in long-context settings.
- GitHub's [context-compression topic](https://github.com/topics/context-compression) is a practical discovery source for newer repo-level tools that claim token reduction or repository-aware context pruning.
- Example repo claims from that topic include repository-aware or coding-agent-oriented tools such as `context-editor-agent`, `CogniLayer`, `engram`, and `claude-rolling-context`, which are useful as benchmark inputs even when their claims need to be validated on private workloads.

### Public benchmark references worth integrating carefully

- OpenAI's GDPval focuses on economically valuable real-world tasks rather than narrow academic tests. See [GDPval](https://openai.com/index/gdpval/).
- OpenAI's BrowseComp is a browsing-agent benchmark with 1,266 hard web tasks. See [BrowseComp](https://openai.com/index/browsecomp/).
- HELM is useful as a model for a living benchmark that evolves over time instead of pretending to be final. See [HELM paper](https://friedeggs.github.io/files/helm.pdf).
- T2I-CompBench++ is a useful reference for text-to-image evaluation, especially compositional fidelity. See [T2I-CompBench++](https://arxiv.org/abs/2307.06350).

## Risks and constraints

- provider APIs differ in token accounting and cost reporting
- model-as-judge scoring can introduce bias
- public benchmark contamination makes some leaderboard claims weak
- live benchmark runs can become expensive quickly
- image generation comparisons need separate storage, rendering, and judge flows
- some providers expose richer usage metadata than others, so normalization must be lossy but transparent

## Suggested next implementation tasks

1. Audit which existing providers are production-ready vs partial.
2. Add `Benchmark Lab` to the product roadmap and navigation design.
3. Create normalized benchmark types in TypeScript and Rust.
4. Build a single-prompt, multi-model MVP using stored API keys.
5. Add per-run metrics for tokens, cost, latency, and context usage.
6. Add prompt variant comparison.
7. Add skill/plugin toggles to measure overhead and quality impact.
8. Add dataset import/export and saved run history.
9. Add rubric scoring and pairwise judge mode.
10. Add E2E tests with mocked providers and deterministic fixtures.

## E2E test plan

For E2E, avoid live provider calls in CI.

Use:

- mocked provider adapters
- deterministic fixture datasets
- snapshotable benchmark results
- fixture-based token/cost metadata

Core E2E flows:

- connect provider key
- run one prompt against two or more mocked models
- compare prompt A vs prompt B
- compare skill off vs on
- verify metrics table, export, history, and error handling
- verify cost/context calculations render correctly

This will let the product ship a stable benchmark experience before layering on live-provider complexity.
