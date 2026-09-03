# Backlog

Known items identified during work sessions; each entry has enough reference for an agent to pick up later.

- **Claude analytics cold start**: disk-persisted per-file aggregates keyed by (path, mtime, size) so app restarts skip re-parsing unchanged JSONL; see profiler report fixes #4/#5 (per-range mtime floors) in session notes and `claude_v2.rs` corpus cache added in 3b25f06.
- **OpenCode assets**: proper sidebar logo (currently reuses codex icon) and tray PNGs (`src-tauri/icons/providers/opencode*.png` + `include_bytes!` arms in `analytics/commands.rs`); plan Phase 8.5.
- **Kimi/DeepSeek menubar icons are colored logo resizes**: consider monochrome/template variants matching macOS menu bar style (`src-tauri/icons/providers/kimi*.png`, `deepseek*.png`).
- **`import_data` misses `"type": "agent"`**: `commands/importexport.rs` full-library import writes agent JSON without the type discriminator that `registry/loader.rs load_agent_file` requires; same bug class fixed in `save_agent`.
- **Fork author attribution inconsistent**: "Use this" silent fork keeps original author; editor fork path re-attributes to current user (`RegistryPage.handleUseCapability` vs `CapabilityEditor` save).
- **Drift for global-scope adapters**: files written outside the project root (codex, opencode global deploys) do not participate in drift tracking (`utils/drift.rs` is project-relative).
- **Gemini brew formula deprecated upstream** (disabled 2026-12-18): Gemini CLI install channel will need migration guidance; Antigravity (`~/.gemini/antigravity/` protobuf data) is the successor data source candidate.
- **`ai-code-tracking.db` has empty `conversation_summaries`/`ai_code_hashes`/`tracked_file_content` tables** on every machine checked during the Cursor Projects research (`~/.claude/plans/do-it-i-have-fizzy-rain.md`); schemas are read-ready in `commands/ai_tracking.rs` but there's no live data to validate the parsing against yet.
- **`conversation-search.db` FTS unused**: Cursor ships a full-text search index (`id == composerId`, needs `immutable=1` connection flag) alongside `state.vscdb` that nothing in the app reads; candidate for a chat-search feature on the new Cursor Projects page (`src-tauri/src/analytics/cursor_projects.rs`).
- **Codex removal scope ambiguity**: `AgentAdapter::remove` has no project/global scope argument, while `src-tauri/src/adapters/codex.rs` checks both locations for a matching capability.
- **Codex Windows config replacement recovery gap**: the Windows fallback in `src-tauri/src/commands/codex.rs::write_codex_config_file` removes the destination before its second rename attempt.
- **Codex platform capability parity**: MCP, agent, hook, and plugin capability flags remain disabled in `src-tauri/src/adapters/codex.rs`; Codex support differs from Claude Code.
- **Codex config write concurrency**: `src-tauri/src/commands/codex_parity.rs::write_config_edits` sends `expectedVersion: null`, and its file fallback performs an unlocked read-modify-write.
- **Codex App Server pagination startup overhead**: each model or permission-profile page in `src-tauri/src/commands/codex_parity.rs` starts a new one-shot App Server child with its own timeout.
