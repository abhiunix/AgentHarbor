# Creating a New Adapter

How to add a provider to AgentHarbor, and what you must research about the provider before writing code. There are two flavors, often shipped together:

1. **Deploy adapter** — implements the `AgentAdapter` trait so capabilities (MCPs, rules, skills, hooks…) can be deployed into the tool's config files.
2. **Analytics provider** — a module under `src-tauri/src/analytics/` that surfaces usage, cost, limits, and connection status (sidebar Analytics page + tray).

Reference commits: `2723314` (DeepSeek/Moonshot — balance-only API providers, 5 files, zero Rust) and `b6b2092` (Kimi — local-file analytics provider, 10 files including an 811-line Rust module).

## 0. Provider research checklist

Everything below is an external fact you must establish before coding. Get these wrong and the adapter mis-detects, produces phantom diffs, or silently reads nothing.

**For a deploy adapter:**
- The canonical short id the ecosystem uses (`claude-code`, `cursor`, `codex`). It becomes the join key across the entire app — registry `compatible_agents`, frontend plugin ids, backup manifests, tray ids — and cannot change later without breaking users' persisted settings.
- Which capability kinds the tool actually supports (MCP / rules / skills / hooks / plugins / subagents / custom files).
- How the tool marks a project as its own (dot-dir? marker file?), and whether it is project-scoped at all (Codex is machine-scoped — `~/.codex`).
- Exact config file paths, **formats** (JSON vs TOML vs Markdown vs `.mdc`), and key names (e.g. is the MCP map called `mcpServers`?).
- Merge semantics: do rules append into one user-owned Markdown file, or become one file per rule?
- How to reverse each write (delete a dir vs strip a managed block without clobbering user content).

**For an analytics provider:**
- Auth mechanism: API key, session token, OAuth device flow, CLI-managed credentials, or purely local files. This picks the `auth_type` string and the connect UX.
- API endpoints and real response bodies — capture one and commit a parse test. Formats vary in traps: DeepSeek returns money as decimal *strings* (`"110.00"`), Moonshot as floats.
- Local data layout if any (Kimi: `~/.kimi/config.toml`, per-project session dirs keyed by md5 of the path, `context.jsonl` / `wire.jsonl`).
- Pricing: **don't guess.** Both `deepseek.rs` and `moonshot.rs` ship with `TODO(pricing)` rather than wiring the cost engine on unverified rates.
- What a 401 looks like vs a transient failure — you must distinguish "reconnect needed" from "temporary error".

## 1. Deploy adapter (`AgentAdapter` trait)

### 1.1 The trait

`src-tauri/src/adapters/traits.rs:131` — `trait AgentAdapter: Send + Sync`:

| Method | Contract |
|---|---|
| `id()` | Stable kebab-case string (see research checklist). |
| `name()` | Official display name. |
| `capabilities()` | `AdapterCapabilities` flags for the 7 kinds. Minimal example: `codex.rs:127` (skills only). |
| `detect(project_path)` | Cheap existence probe. Project-scoped idiom: `cursor.rs:559`; machine-scoped: `codex.rs:139`. |
| `read_config(project_path)` | What's currently installed, per kind (names only). |
| `diff(...)` | One `ConfigDiffEntry` per file that would change; `current_content` read from disk (`None` ⇒ Add). Pure preview — no writes. `options` currently carries `{"global": true}` and `claude_settings_target`. |
| `deploy(...)` | Must write exactly what `diff()` proposed — drift state is recorded from the *diff's* `proposed_content` (`commands/deploy.rs:326`), so divergence shows as immediate drift. |
| `remove(...)` | Takes `CompositeId`s. Derive on-disk artifact names with the **same helper** deploy used (`codex.rs` currently diverges: `:305` vs `:55` — don't copy that). |
| `managed_paths(project_path)` | Every config surface the adapter touches. |

### 1.2 Registration

`src-tauri/src/adapters/mod.rs` — three edits: `pub mod`, `pub use`, and `Arc::new(NewAdapter::new())` inside `AdapterRegistry::new()` (L31–44). That single list feeds `preview_deploy`/`execute_deploy` (`commands/deploy.rs:212,283`) and the `detect_adapters` command (`commands/project.rs:78`).

### 1.3 Mandatory utils

| Util | Why |
|---|---|
| `utils/paths.rs` `atomic_write_str` | tmp+rename; Windows remove-then-rename fallback when the IDE holds the file. Every adapter wraps it in a private `write_file_atomic`. |
| `utils/paths.rs` `normalize_line_endings` | **Before hashing or diffing**, always. Otherwise Windows `\r\n` produces permanent phantom drift. |
| `utils/paths.rs` `read_with_sharing` | Read-side retry on Windows `PermissionDenied`. |
| `utils/rule_block.rs` | The managed-block protocol for injecting/removing rules in user-owned Markdown. Don't hand-roll markers. |
| `utils/backup.rs` | Keyed by adapter id. Note: `execute_deploy` does *not* back up — the frontend calls `create_project_backup` first, gated on `settings.deploy.create_backups`. |
| `utils/drift.rs` | State at `<project>/.agentharbor/deploy-state.json`, relative paths only — files written outside the project root (global adapters) effectively don't participate in drift. |
| `utils/keychain.rs` | Secrets (MCP `env`) — service `com.agentharbor.app`, process-lifetime cache to avoid repeated macOS prompts on unsigned dev builds. |

### 1.4 Deploy-wizard visibility

`src/components/deploy/AdapterSelector.tsx:31` has a **hardcoded `ADAPTERS` array** (id, name, color, capability booleans) duplicating the Rust registry. A new deploy adapter is invisible in the wizard until added there. (It is already out of sync: 5 entries vs 8 registered adapters.) Also check the stale `AdapterType` union at `src/lib/types.ts:187`.

## 2. Analytics provider

### 2.1 Module archetypes

**Remote API provider** — template: `analytics/moonshot.rs` (165 lines). Structure: constants (id/name/URL) → serde response shapes → `resolve_token()` via `token_store::get_provider_token(PROVIDER_ID, "api-key")` → `check_connection()` → `fetch_<id>_analytics()` → tests parsing a real captured response body.

HTTP goes through `analytics/http.rs` (`build_client`, `authed_get`, `cookie_get`, …). Match `HttpCallError::Unsuccessful { status: 401, .. }` explicitly: 401 ⇒ mark disconnected; anything else ⇒ stay connected and surface the error (`moonshot.rs:89-101`).

**Local-file provider** — template: `analytics/kimi_v2.rs`. Patterns to copy:
- All filesystem work inside `async` commands wrapped in `tokio::task::spawn_blocking`.
- Separate caches for local data (300s TTL) and remote usage limits (60s TTL), merged at read time, so a token/API failure never blanks the local sections.
- A module doc comment listing the exact on-disk layout the module reads.

OAuth flavor: `analytics/kimi_auth.rs` (device/OAuth constants, sentinel error prefixes, credential fingerprint used to invalidate the usage cache on token swap).

### 2.2 Registration points (Rust)

| Where | What |
|---|---|
| `analytics/mod.rs` | `pub mod <provider>;` |
| `analytics/commands.rs:354` `get_all_provider_status()` | add `<provider>::check_connection()` |
| `analytics/commands.rs:379` `get_provider_analytics()` | add a match arm — unmatched ids return `Err("Unknown provider")`, an easy miss |
| `analytics/types.rs` `all_provider_info()` | one `ProviderInfo { id, name, auth_type, description, has_local_data, has_api }` literal. `auth_type` ∈ `auto-detect | token | api-key | device-flow | cli | local-file` |
| `lib.rs` | **two** edits: the `use analytics::<module>::{...}` import *and* the entry in `tauri::generate_handler![...]` (~L260). Skipping the macro entry fails silently at runtime with "command not found" — no compile error. |

Tokens: `analytics/token_store.rs` — primary storage is `provider-tokens.json` (0600) in app data; the keychain is a *write-only* backup. `get_provider_token` deliberately never reads the keychain (macOS prompt avoidance) — don't "fix" it.

### 2.3 Tray integration (6 locations)

| Location | Edit |
|---|---|
| `commands/config.rs:141` | `ALL_TRAY_PROVIDER_IDS: [&str; N]` — bump the hardcoded length (compile error if you forget, thankfully) |
| `analytics/commands.rs` `tray_provider_display_name()` | add the id (falls through to `"Unknown"`) |
| `analytics/commands.rs` `build_tray_summary()` | one `thread::spawn` handle + matching join block (providers fetch in parallel) |
| `src-tauri/icons/providers/` + `include_bytes!` constants in `analytics/commands.rs` | `<id>.png` + `<id>-active.png`. Missing icons degrade to no icon — easy to ship half-finished |
| `src/lib/tauri.ts:266` | the TS mirror of `ALL_TRAY_PROVIDER_IDS` (not compiler-checked) |
| `src/components/tray/TrayPopover.tsx:37` | `ALL_PROVIDER_TABS` entry (id, name, icon, deep-link route) |

`tray.rs` itself needs no per-provider edit. `TrayProviderCard.tsx` has a generic default branch (rate-limit bars + credits), so a new provider renders acceptably without a custom card.

## 3. Frontend wiring (both flavors)

- **`src/lib/adapterPlugins.ts`** — the single source of truth for the sidebar. Add the `AdapterPlugin` entry (id, name, logo from `src/assets/`, color, features with routes `/adapters/<id>/<featureId>`). If the adapter should be on by default for existing users, add its id to `NEW_DEFAULT_ADAPTERS` **and bump `NEW_DEFAULT_ADAPTERS_MIGRATION_KEY`** — appending without bumping the key does nothing for users who already ran the migration.
- **`src/pages/AdapterFeaturePage.tsx`** — lazy import + entry in `ADAPTER_FEATURE_COMPONENTS`. Declaring a feature in `adapterPlugins.ts` but not here renders "Feature X is not yet implemented". Balance-only API providers can reuse `ProviderBalancePage.tsx` with zero new page code.
- **`src/App.tsx`** — no change needed; the generic `adapters/:adapterId/:featureId` route covers new adapters. Only add a `<Navigate>` redirect to preserve a legacy flat URL.
- **`src/lib/tauri.ts`** — TS interfaces mirroring the Rust serde structs + `invoke` wrappers (camelCase args, Tauri converts to snake_case).
- **`src/components/analytics/ProviderConnectModal.tsx:8`** — `PROVIDER_TOKEN_CONFIG` entry. The `keyType` string must byte-match the `key_type` your Rust `resolve_token()` uses; a mismatch means tokens save fine but the provider stays "not configured". Skipped for `auto-detect`/`local-file`/`cli` auth types.
- Stores need nothing: `analyticsStore.ts` is fully generic; no store enumerates adapters.

## 4. Registry & `compatible_agents`

Capabilities carry `compatible_agents: Vec<String>` of adapter ids (loader accepts `adapters` / `compatible_agents` / `compatible_adapters` aliases). There is **no validation** that these strings match registered adapter ids — a typo is a silent no-op. Filtering happens only in the frontend (`AdapterSelector.tsx:76`): an explicit list wins; an **empty list means "compatible with any adapter whose type flags allow it"**, not "compatible with nothing". A new adapter id therefore needs no registry content changes to work — existing items become type-compatible automatically.

## 5. Gotchas checklist

1. `invoke_handler!` in `lib.rs` — silent runtime failure if skipped; requires both the `use` line and the macro entry.
2. `normalize_line_endings` before hashing/diffing, always.
3. `deploy()` must write exactly what `diff()` proposed (drift is recorded from the diff).
4. One name-derivation helper shared by `deploy()` and `remove()`.
5. `ALL_TRAY_PROVIDER_IDS`: bump the Rust array length; manually sync the TS mirror.
6. Bump `NEW_DEFAULT_ADAPTERS_MIGRATION_KEY` when adding a default-on adapter.
7. `AdapterSelector.tsx` hardcoded list — deploy adapters are invisible in the wizard until added.
8. Handle 401 explicitly to distinguish "reconnect" from transient errors.
9. `keyType` must match between `ProviderConnectModal.tsx` and Rust `resolve_token()`.
10. Money formats vary per provider — capture a real response and commit a parse test.
11. `spawn_blocking` for filesystem work in async commands; cache local and remote data separately.
12. Don't wire `cost_engine` on unverified pricing — leave a `TODO(pricing)`.
