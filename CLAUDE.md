# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

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

## Rules

Follow all rules in this section. Deployed via AgentHarbor.

<!-- AgentHarbor:rules:start -->
<!-- AgentHarbor:rule:c34734df-6eb4-4856-8ba0-7cf49c2d3824/bf7cd4d6a8d534fd -->
1. **Never Use  —**

Never use  `—` in any .md files or webpage, or any sentence. Instead if needed use alternate special characters may be like ; or something else

<!-- AgentHarbor:rule:7e98dc1f-24fc-4d96-8d18-6a2f308971d5/a6c693b77e484052 -->
2. **GiveTaskStatus&TestsToPerform**

Whenever you finish a task, provide a one-line summary of the completed tasks ✅ along with the relevant test cases the user should verify and observe.

<!-- AgentHarbor:rule:7e98dc1f-24fc-4d96-8d18-6a2f308971d5/e932239515c75b6f -->
3. **quick-answer-tldr**

When I ask you a question, then you have to first give me answer in TLDR format in easy explanation way. 
Then if needed you can give answers in full fledge explanation.

<!-- AgentHarbor:rule:7e98dc1f-24fc-4d96-8d18-6a2f308971d5/aa31a625544edba5 -->
4. **Explaination clarity**

Please consider that I am an SDE-1 developer and an SDE-1 network engineer, so I may not fully understand highly technical wording. So explain plainly + always add a glossary for plans, questions in chat or anywhere you think is necessary.

I am not familiar with jargons like ES Helper or eBPF. Please explain these concepts in a simple, beginner-friendly way. So the sentences should be in explanatory way and then in bracket it should come like (this process is called xyz) or (this is the work of ES helper in macos) etc

For any jargon or technical terms, please include a quick definition at the end of the document or chat.

<!-- AgentHarbor:rule:c34734df-6eb4-4856-8ba0-7cf49c2d3824/acb838f039b67051 -->
5. **validate-questions**

- If I ask a questions, or suggest some features then you must have to evaluate my request first then you have to shortly answer about the decisions impact, weather its a good decision or bad, if better approach is available then suggest that too with why.

<!-- AgentHarbor:rule:7e98dc1f-24fc-4d96-8d18-6a2f308971d5/a42f9c8169b09ea4 -->
6. **Ask-questions-before-plan**

When you are in doubt and you need more information from me, just ask me key questions right away before planning/implementing/answering.

<!-- AgentHarbor:rule:7e98dc1f-24fc-4d96-8d18-6a2f308971d5/4361888d0c1f0e38 -->
7. **maintain todo.md**

When we are working on something and you found out that there are some tasks like bug fixes then keep that in todo.md file so once we identify a problem we can keep in backlogs so we can recall by and work on that bug later if not now.
Keep references you/an agent knows whats the bug. dont write what to fix.
<!-- AgentHarbor:rules:end -->

<!-- AgentHarbor: Deployed Capabilities -->
- **Skill: macos-app-signing-c5d9fd60** (via Claude Code)
- **Skill: remotion-best-practices** (via Claude Code)
- **Skill: remotion-captions** (via Claude Code)
- **Skill: remotion-create** (via Claude Code)
- **Skill: remotion-docs** (via Claude Code)
- **Skill: remotion-interactivity** (via Claude Code)
- **Skill: remotion-maps** (via Claude Code)
- **Skill: remotion-markup** (via Claude Code)
- **Skill: remotion-multimedia** (via Claude Code)
- **Skill: remotion-render** (via Claude Code)
- **Skill: remotion-saas** (via Claude Code)
- **Skill: remotion-upgrade** (via Claude Code)
- **Hook: PostToolUse** (via Claude Code)
<!-- /AgentHarbor -->