# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

<<<<<<< HEAD
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
=======
## Commands

```bash
npm install                       # install JS deps
npm run tauri dev                 # full dev: Vite on :1420 + Tauri window, Rust hot-restart
npm run build                     # tsc && vite build (frontend only)
npm run test:regression           # npm run build + cd src-tauri && cargo test  (the CI gate)
```

Targeted checks:

```bash
npx tsc --noEmit                                    # frontend type check (CI uses this)
cd src-tauri && cargo test <name>                   # single Rust test
cd src-tauri && cargo clippy --all-targets -- -D warnings   # lint Rust (CI runs this, continue-on-error)
cd src-tauri && SKIP_VERSION_BUMP=1 cargo build     # build check without bumping the patch version
```

**Version-bump side effect.** `npm run build` runs `scripts/bump-build.cjs` as a `prebuild` step and increments the patch in `package.json`, `src-tauri/tauri.conf.json`, and `src-tauri/Cargo.toml` in lockstep. Set `SKIP_VERSION_BUMP=1` whenever you don't want that (CI sets it on plain build checks; the signed-release flow also sets it — see `docs/build-and-release.md`).

Signed/notarized builds, release tagging, and the `latest.json` updater feed are documented in `docs/build-and-release.md`; do not run them speculatively.

## Architecture

Tauri 2 desktop app. **React 19 + TypeScript + Vite + Zustand** (`src/`) talks over Tauri IPC to a **Rust** backend (`src-tauri/src/`). Bundle target is macOS aarch64 + Windows x64; no Linux packaging.

### Two Tauri windows
`src/App.tsx` branches on `getCurrentWindow().label`:
- `main` — the full app (sidebar + routes under `BrowserRouter`).
- `tray-popover` — a separate Tauri window owned by `src-tauri/src/tray.rs`, renders `<TrayPopover />` only. It emits `navigate-to` events that the main window picks up via `NavigateListener` (also shows/focuses the main window).

### Adapter trait — the core abstraction
`src-tauri/src/adapters/traits.rs` defines `AgentAdapter` (`id`, `name`, `capabilities`, `detect`, `read_config`, `diff`, `deploy`, `remove`, `managed_paths`). One implementation per provider (`claude_code`, `cursor`, `windsurf`, `gemini`, `copilot`, `antigravity`, `vscode`, `codex`) lives next to it. `AdapterRegistry::new()` in `adapters/mod.rs` instantiates the full list — **adding a provider means adding it there**.

The deploy pipeline is: frontend calls `preview_deploy` → registry resolves capability/agent IDs → each selected adapter's `diff()` returns `ConfigDiffEntry`s (Add/Modify/Remove + current/proposed content) → user reviews in the wizard → `execute_deploy` runs `utils::backup` then writes via `utils::paths::atomic_write` and records a manifest. `utils::drift` later compares stored hashes against on-disk content to surface drift.

### Analytics is a parallel subsystem
`src-tauri/src/analytics/` has its own per-provider modules (`claude_v2`, `cursor_v2`, `codex`, `gemini`, `copilot`, plus several others), shared `http`, `token_store`, `cost_engine`, and a `commands` module that exposes the tray summary + per-provider analytics commands. The cost engine and rate-limit "LimitState ladder" are provider-specific — don't try to unify them with the deploy/adapter layer.

### Tauri command registration is centralized
Every IPC command must be wired in **two** places:
1. The Rust handler lives under `src-tauri/src/commands/<domain>.rs` (or `analytics/commands.rs`, or `tray.rs`).
2. It is registered in the giant `invoke_handler![…]` macro in `src-tauri/src/lib.rs`. If you add a new command and skip this, the frontend call silently fails with "command not found". The matching TypeScript wrapper goes in `src/lib/tauri.ts`.

### Registry sources (bundled + community + custom)
Capabilities and agents are loaded from three roots, in this order: the bundled `registry/` directory (resourced into the app via `tauri.conf.json`), a synced community clone under the app data dir, and per-user custom items. `src-tauri/src/registry/loader.rs` walks `capabilities/{mcps,rules,skills,hooks,plugins,customs}` and merges by ID (later sources win). The `updater` polls the configured GitHub repo on a background interval; `App.tsx` starts polling on launch when `settings.registry.auto_update` is on and listens for `registry-updated` events to reload stores.

### Frontend routing convention
The canonical pattern is `adapters/:adapterId/:featureId`, rendered by `src/pages/AdapterFeaturePage.tsx`. Older flat routes (`/global`, `/memory`, `/permissions`, `/usage`, `/extensions`, `/prompts`, `/transcripts`, `/plans`, `/ai-attribution`) are explicit `<Navigate>` redirects in `App.tsx` — when adding new feature pages, prefer the adapter route and only add a redirect if a legacy URL needs preserving.

### Cross-platform file handling
The codebase is sensitive to Windows quirks. When touching managed config files, use the helpers in `src-tauri/src/utils/paths.rs`:
- `atomic_write` (write tmp + rename; on Windows, remove-then-rename when destination is locked).
- `read_with_sharing` (retries with backoff on `PermissionDenied`).
- `normalize_line_endings` — always normalize to `\n` **before** hashing or diffing, otherwise Windows IDEs writing `\r\n` will produce false drift and noisy diffs.

Secrets (e.g. MCP `env` values) go through the OS keychain via the `keyring` crate (`utils/keychain.rs`) and are injected into MCP server configs at deploy time, never written to disk in plaintext.

### Privacy invariant
Outbound network calls are restricted to each provider's official API endpoints using the user's own OAuth tokens. No telemetry. Local provider files (Claude JSONLs, Gemini telemetry, Cursor SQLite) are read with shared-read opens and never copied off-disk.

## CI / Release

- `.github/workflows/ci.yml` — push/PR to `main`: `npx tsc --noEmit`, `cargo clippy` (warning-as-error, but `continue-on-error: true`), `cargo test`, `cargo build` with `SKIP_VERSION_BUMP=1`. Runs on macOS only.
- `.github/workflows/release.yml` — tag `v*`: full signed + notarized macOS build, publishes DMG + updater `.tar.gz` + `.sig` + `latest.json` to the GitHub Release. Requires the eight signing secrets listed in `docs/build-and-release.md`.
>>>>>>> 1330e7fb56d01125b5f8b0bd41f46df131fa77d9
