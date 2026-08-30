# Backlog

Known items identified during work sessions; each entry has enough reference for an agent to pick up later.

- **Claude analytics cold start**: disk-persisted per-file aggregates keyed by (path, mtime, size) so app restarts skip re-parsing unchanged JSONL; see profiler report fixes #4/#5 (per-range mtime floors) in session notes and `claude_v2.rs` corpus cache added in 3b25f06.
- **OpenCode assets**: proper sidebar logo (currently reuses codex icon) and tray PNGs (`src-tauri/icons/providers/opencode*.png` + `include_bytes!` arms in `analytics/commands.rs`); plan Phase 8.5.
- **Kimi/DeepSeek menubar icons are colored logo resizes**: consider monochrome/template variants matching macOS menu bar style (`src-tauri/icons/providers/kimi*.png`, `deepseek*.png`).
- **`import_data` misses `"type": "agent"`**: `commands/importexport.rs` full-library import writes agent JSON without the type discriminator that `registry/loader.rs load_agent_file` requires; same bug class fixed in `save_agent`.
- **Fork author attribution inconsistent**: "Use this" silent fork keeps original author; editor fork path re-attributes to current user (`RegistryPage.handleUseCapability` vs `CapabilityEditor` save).
- **Drift for global-scope adapters**: files written outside the project root (codex, opencode global deploys) do not participate in drift tracking (`utils/drift.rs` is project-relative).
- **Gemini brew formula deprecated upstream** (disabled 2026-12-18): Gemini CLI install channel will need migration guidance; Antigravity (`~/.gemini/antigravity/` protobuf data) is the successor data source candidate.
