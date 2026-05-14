# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

AgentHarbor is a native tray app (macOS + Windows) that manages configuration, analytics, and capability deployment across AI coding tools (Claude Code, Cursor, Codex, Gemini CLI, Windsurf, Copilot, Antigravity, VS Code). Tauri 2 shell — Rust backend in `src-tauri/`, React 19 + TypeScript + Vite + Tailwind frontend in `src/`.

## Commands

```bash
npm run tauri dev                          # dev with hot reload (Vite on :1420, Tauri window)
npm run build                              # tsc + vite build (frontend only). prebuild auto-bumps patch unless SKIP_VERSION_BUMP=1
npm run tauri build                        # full signed/notarized release bundle — see docs/build-and-release.md
npm run test:regression                    # frontend build + cargo test (full pre-PR gate)
npx tsc --noEmit                           # TS type-check only
cargo test --manifest-path src-tauri/Cargo.toml [name_of_test]   # run all / one Rust test
```

No lint script — `tsc --noEmit` and `cargo test` are the gates (CI also runs `cargo clippy`).

## Architecture

### IPC contract
- Every frontend → backend call goes through a typed wrapper in `src/lib/tauri.ts` that calls Tauri's `invoke`. **Never call `invoke` directly from a component.**
- Every backend command lives under `src-tauri/src/commands/<area>.rs` and **must** be added to the `tauri::generate_handler!` list in `src-tauri/src/lib.rs` — otherwise the frontend gets a runtime error, not a compile error.
- Commands return `Result<T, String>` (use `map_err(|e| e.to_string())`).

### Adapter abstraction
Each supported AI tool is an adapter under `src-tauri/src/adapters/` implementing `AgentAdapter` (see `adapters/traits.rs`). Adapters know that tool's on-disk layout (where MCP configs live, where rules go, what file format) and produce/apply `ConfigDiffEntry` lists. `AdapterRegistry::new()` in `adapters/mod.rs` is the canonical list — register new adapters there. `AdapterCapabilities` advertises which capability types (mcp / rules / skills / hooks / plugins / agents / custom) the adapter supports.

### Deploy pipeline
`commands/deploy.rs` → `preview_deploy` produces diffs per target adapter, UI renders them, `execute_deploy` writes them. Writes go through `utils/backup.rs` (snapshot before write) and `utils/fs.rs` atomic write helpers — **write to `.tmp`, then `std::fs::rename`, never write the destination directly**. `utils/manifest.rs` records what was deployed; `utils/drift.rs` compares current file contents to the manifest to flag external edits.

### Registry
- `UniversalCapability` (JSON) and `AgentDefinition` (Markdown + YAML frontmatter) are the two registry primitives, defined in `src-tauri/src/models/`.
- Loaded by `registry::loader::{load_capabilities, load_agents}` from a list of dirs. `get_bundled_registry_path()` resolves OS-specific resource locations (`.app/Contents/Resources/registry` on macOS, `resources/registry` on Windows) — do not hardcode paths.
- `registry::updater` polls a GitHub registry repo on a background interval and syncs into the community registry dir. Custom user capabilities live in their own dir (`commands/custom.rs`).
- The repo-root `registry/` directory is intentionally empty — bundled content is generated/fetched at build time.

### Analytics
`src-tauri/src/analytics/<provider>.rs` fetches usage from each provider's API and normalizes into the shared `AnalyticsData` type in `analytics/types.rs`. `cost_engine.rs` computes per-model dollar costs with token dedup across overlapping session windows. `tray.rs` reads cached analytics and updates the menu-bar title (`XX%` or `$N` depending on provider — see provider table in README). To add a provider, follow the 5-step recipe in `CONTRIBUTING.md` (analytics module → mod.rs → command → page → tray card).

### Frontend state
Zustand stores in `src/stores/` (`use*Store` pattern, one hook per store). Routes are React Router based, defined in `src/App.tsx`. UI palette is fixed dark: bg `#0e0f13`, cards `#1a1b23`, border `#2a2b36`, text `#e8e9ed` — Tailwind only, no CSS modules.

## Conventions worth not relearning

- **Atomic writes only** for any file under user config. The `.tmp` + rename pattern is enforced socially, not by a helper — easy to forget.
- **Path helpers** live in `src-tauri/src/utils/paths.rs`. Never hardcode `~/Library/...`, `%APPDATA%`, etc.
- **Secrets** (API keys, OAuth tokens) go through `utils/keychain.rs` → OS keychain. They get injected into MCP `env` blocks at deploy time, never serialized into capability JSON.
- Rust model structs derive `Serialize, Deserialize, Clone, Debug`.
- Boolean naming: `isLoading`, `hasError`, `canDeploy`. Functions start with a verb.
- Commit prefix is required: `feat | fix | chore | docs | refactor | test`. Subject ≤72 chars.

## Release flow

Push a `v<version>` tag → `.github/workflows/release.yml` runs the signed mac build, notarizes, and attaches DMG + `.tar.gz` + `.sig` + `latest.json` to the release. Eight repo secrets must be set (Apple cert + API key + Tauri updater key). See `docs/build-and-release.md` for the full signing-files checklist and local signed-build env vars.
