# OpenCode Adapter — Provider Research

Answers to the research checklist in [creating-an-adapter.md](creating-an-adapter.md) for OpenCode (opencode.ai, sst/opencode). Verified against the sst/opencode repo (`dev`, v1.18.x), opencode.ai docs, and a live local install (v1.18.25, 2026-08). Facts cite source files; anything unverifiable is flagged.

## A. Deploy-adapter facts

### Id and name
- Adapter id: **`opencode`** (binary name, XDG dir name, npm scope `@opencode-ai/*`). Display name: **OpenCode**. No conflict with existing adapter ids.

### Config locations and precedence
Paths come from the `xdg-basedir` npm package (`packages/core/src/global.ts`) — platform-independent, so on **Windows everything lives under `%USERPROFILE%\.config\opencode` and `%USERPROFILE%\.local\share\opencode`** (not `%APPDATA%`), unless `XDG_*` is set.

| Surface | Path |
|---|---|
| Global config | `~/.config/opencode/opencode.json` or `.jsonc` (legacy `config.json`) |
| Project config | `opencode.json(c)` walking **up** from cwd to the git worktree root (every match merges) |
| Project dot-dir | `.opencode/` at any ancestor level + `~/.opencode`, each with optional `opencode.json(c)` and `agent(s)/`, `command(s)/`, `plugin(s)/`, `skill(s)/` |
| Data | `~/.local/share/opencode/` |

Merge order (later wins; deep merge, arrays concat — `config/config.ts`): remote `.well-known/opencode` → global → `OPENCODE_CONFIG` → project files up the tree → `.opencode/` dirs → `OPENCODE_CONFIG_CONTENT` env → managed config (`/Library/Application Support/opencode`, `/etc/opencode`, `%ProgramData%\opencode`).

Quirks to respect:
- opencode **auto-creates** the global config with `{"$schema": "https://opencode.ai/config.json"}` — preserve the `$schema` key.
- It writes `.gitignore`, `package.json`, `node_modules/` (auto-installed `@opencode-ai/plugin`) **into config dirs** — not user content, not drift.
- Configs are JSONC parsed with `jsonc-parser`; opencode's own writes are comment-preserving.

### Formats and keys per capability kind
All config is JSON/JSONC; agents/commands/skills are Markdown + YAML frontmatter; plugins are JS/TS.

- **MCP** — top-level `"mcp"` object, name → server. Local: `{"type":"local","command":["bun","x","server"],"environment":{...},"enabled":true}` — **`command` is one array (no separate `args`), env key is `environment` (not `env`)**. Remote: `{"type":"remote","url","headers","oauth"}`. `{env:VAR}` / `{file:path}` substitution supported. Registry MCP entries need translation.
- **Rules** — global: `~/.config/opencode/AGENTS.md` (falls back to `~/.claude/CLAUDE.md`); project: first matching of `AGENTS.md` > `CLAUDE.md` > `CONTEXT.md` walking up to the worktree root. Extra files via the `"instructions"` config array. User-owned prose ⇒ deploy through `utils/rule_block.rs` managed blocks.
- **Skills** — native: `{skill,skills}/**/SKILL.md` in config dirs; **also reads `.claude/skills/**` and `.agents/skills/**`** — a skill deployed for claude-code is already visible to OpenCode; avoid double-deploying. Frontmatter: `name` (must match dir), `description`, optional `license`/`compatibility`/`metadata`.
- **Commands** — `{command,commands}/**/*.md`; frontmatter `description`/`agent`/`model`/`subtask`; body is the template (`$ARGUMENTS`, `` !`shell` ``, `@file`).
- **Agents** — `{agent,agents}/**/*.md`; frontmatter `description` (required), `mode`, `model`, `tools`, `permission`, `disable`.
- **Hooks** — **no shell-hook config exists.** OpenCode's hook mechanism is JS/TS plugins (`{plugin,plugins}/*.{ts,js}` or the `"plugin"` npm array). Recommend marking the hooks capability unsupported.
- **Permissions** — top-level `"permission"` key: per-tool `allow|ask|deny`, bash pattern maps, per-agent overrides. Mergeable JSON, not hooks.

### Detection, merge semantics, clobber hazards
- **Detection**: OpenCode runs anywhere (machine-scoped like Codex). Project markers of intentional use: `.opencode/`, `opencode.json(c)`, `AGENTS.md`.
- **Wholesale per-item files**: agents, commands, skills, plugin files (remove = delete file). **Merged single file**: `opencode.json(c)` (mcp/instructions/permission/plugin keys) — key-level JSON merge like existing adapters. **Appended Markdown**: AGENTS.md via managed blocks.
- **JSONC hazard**: the user's global file may be `opencode.jsonc` with comments. `serde_json` will choke or strip comments. Also `.jsonc` merges **after** `.json` at global level, so writing a parallel `opencode.json` gets overridden by existing `.jsonc` keys. Options: text-level JSONC patching (what opencode itself does via jsonc-parser `modify`/`applyEdits`), or refuse/warn when only `.jsonc` exists.
- `mcp.<name>.environment` may hold secrets — same keychain-injection treatment as other adapters.

## B. Analytics-provider facts

### Local data — SQLite since v1.2.0
- **`~/.local/share/opencode/opencode.db`** (SQLite, WAL). Introduced v1.2.0 (2026-02-14) with a one-time migration from flat `storage/session/**.json` files. Reasonable to require ≥1.2.0.
- `session` table has **precomputed per-session aggregates**: `cost REAL, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write`, plus `directory`, `project_id`, `parent_id` (non-null = subagent), `agent`, `model`, ms-epoch timestamps. Totals are a plain SQL sum — no message parsing needed.
- `message.data` JSON blob: assistant messages carry `providerID`, `modelID`, `cost` (USD float), `tokens {total,input,output,reasoning,cache{read,write}}`, `time.created/completed`.
- The DB is held open by a running TUI — open **read-only** (`mode=ro`) with shared access; WAL means `-wal`/`-shm` are also read.
- Per-project filtering is trivial via `session.directory` / `project.worktree`.

### Auth
- `opencode auth login` writes **`~/.local/share/opencode/auth.json`** (0600, **plaintext** — docs say "encrypted" but the file is plaintext JSON, verified). Map of providerID → `{"type":"api","key"}` | `{"type":"oauth","refresh","access","expires"}` (Anthropic Pro/Max, Copilot device flow) | `{"type":"wellknown"}`.
- For AgentHarbor: `auth_type` **`local-file`** — no token-entry UI; connected = binary + auth.json present.

### Cost and pricing
- **OpenCode computes cost itself** per assistant message (`session.ts` `getUsage`): token counts × USD-per-million rates from the model catalog, incl. context-tier pricing and a Copilot special case; persisted in the DB. **AgentHarbor should read stored costs, not recompute** — no `cost_engine` wiring or `TODO(pricing)` needed.
- Catalog: `https://models.opencode.ai/api.json` (models.dev mirror), cached at `~/.cache/opencode/models.json`, 5-min TTL.
- **No cloud usage/limits API to poll** — analytics is purely local SQLite. No rate-limit ladder data found on disk for Anthropic OAuth (unverified that any exists).

## C. Practical notes
- v1.18.25 current (2026-08-28); near-daily patch releases. Config schema ("v1") stable; storage changed once (v1.2.0).
- Schema URLs: `https://opencode.ai/config.json`; separate `tui.json` for TUI config — leave alone.
- Capability mapping summary: mcp ✓, rules ✓ (managed blocks), skills ✓ (mind the `.claude/skills` overlap), commands/agents ✓ (one md per item), plugins ~ (JS files), hooks ✗.
