# Contributing to AgentHarbor

Thank you for your interest in contributing. This guide covers everything you need to get started.

---

## Ways to contribute

- **Bug reports** — open an issue with steps to reproduce, OS version, and app version
- **Registry entries** — add new MCP servers, rules, skills, hooks, or agents to the bundled registry
- **Provider analytics** — add or improve analytics support for a new AI provider
- **UI improvements** — fix visual issues, improve accessibility, add missing polish
- **Documentation** — fix inaccuracies, add missing details, improve clarity

---

## Prerequisites

| Tool | Version |
|---|---|
| Rust | 1.75+ (install via [rustup](https://rustup.rs)) |
| Node.js | 20+ |
| Xcode Command Line Tools | macOS only (`xcode-select --install`) |
| Tauri CLI | `cargo install tauri-cli` |

---

## Dev setup

```bash
# Clone the repo
git clone https://github.com/abhiunix/AgentHarbor.git
cd AgentHarbor

# Install frontend dependencies
npm install

# Start the dev server with hot reload
npm run tauri dev
```

The app opens as a native window. Frontend changes hot-reload; Rust changes require a recompile (automatic with `tauri dev`).

### Run checks

```bash
# TypeScript type check (no emit)
npx tsc --noEmit

# Rust tests
cargo test --manifest-path src-tauri/Cargo.toml

# Full regression suite
npm run test:regression
```

All three must pass before submitting a PR.

---

## Project layout

```
agentharbor/
├── src/                    React frontend
│   ├── components/         UI components (tray, registry, deploy, settings, analytics)
│   ├── pages/              Route pages
│   ├── stores/             Zustand state stores
│   ├── lib/                Tauri IPC wrappers and TypeScript types
│   └── hooks/              Custom React hooks
├── src-tauri/              Rust backend
│   ├── src/
│   │   ├── analytics/      Per-provider analytics fetchers and tray update logic
│   │   ├── adapters/       IDE adapter implementations (Claude Code, Cursor, Windsurf)
│   │   ├── commands/       Tauri IPC command handlers
│   │   ├── registry/       Registry loader and validator
│   │   ├── utils/          File I/O, keychain, drift detection, backup, manifests
│   │   ├── tray.rs         System tray setup and menu
│   │   └── lib.rs          App bootstrap and command registration
│   └── tauri.conf.json     App config (permissions, updater, bundle)
├── registry/               JSON capability and agent definitions (bundled at compile time)
│   ├── capabilities/
│   └── agents/
└── docs/                   User and contributor documentation
```

---

## Code standards

### Rust

- All model structs: `#[derive(Serialize, Deserialize, Clone, Debug)]`
- Atomic file writes: write to `.tmp`, then `std::fs::rename` — never write directly
- Error types: return `Result<T, String>` from Tauri commands; use `map_err(|e| e.to_string())`
- New Tauri commands must be registered in `src-tauri/src/lib.rs` under `tauri::generate_handler!`
- Write unit tests for all non-trivial logic (`#[cfg(test)]` module in the same file)

### TypeScript / React

- Tailwind CSS only — no CSS modules, no inline `style=` except for dynamic values
- Dark palette: app bg `#0e0f13`, cards `#1a1b23`, border `#2a2b36`, text primary `#e8e9ed`
- All Tauri IPC calls go through typed wrappers in `src/lib/tauri.ts` — never call `invoke` directly from components
- State lives in Zustand stores (`src/stores/`) — no prop drilling beyond two levels
- New stores export a single `use*Store` hook

### Naming

| Context | Convention |
|---|---|
| React components | `PascalCase.tsx` |
| Stores, hooks, utilities | `camelCase.ts` |
| Rust files | `snake_case.rs` |
| Boolean variables | `isLoading`, `hasError`, `canDeploy` |
| Functions | Start with a verb: `fetchData`, `handleClick`, `generateJson` |
| Constants | `UPPER_SNAKE_CASE` |

---

## Adding a registry capability

Registry entries live in `registry/capabilities/` as JSON files. Each file defines one `UniversalCapability`.

**Minimal MCP server example** (`registry/capabilities/my-mcp.json`):

```json
{
  "id": "community/my-mcp",
  "type": "mcp",
  "name": "My MCP Server",
  "description": "What this server does in one sentence.",
  "version": "1.0.0",
  "author": "community",
  "visibility": "public",
  "tags": ["category"],
  "adapter_configs": {
    "claude-code": {
      "files": [
        {
          "deploy_path": ".claude/settings.json",
          "content": "{\"mcpServers\":{\"my-mcp\":{\"command\":\"npx\",\"args\":[\"-y\",\"my-mcp-package\"]}}}"
        }
      ]
    }
  }
}
```

See `registry/CONTRIBUTING.md` for the full schema reference and all capability types (rule, skill, hook, plugin).

---

## Adding an agent definition

Agent definitions live in `registry/agents/` as Markdown files with YAML frontmatter.

```markdown
---
id: community/my-agent
name: My Agent
description: What this agent does.
model: claude-sonnet
memory: project
color: blue
tools:
  - Read
  - Edit
  - Bash
tags:
  - utility
---

You are a helpful agent that...
```

---

## Adding a provider analytics module

1. Create `src-tauri/src/analytics/<provider>.rs` — implement the fetch logic returning the shared `AnalyticsData` type.
2. Register it in `src-tauri/src/analytics/mod.rs`.
3. Add a Tauri command in `src-tauri/src/analytics/commands.rs`.
4. Add a frontend page in `src/pages/<Provider>AnalyticsPage.tsx` and wire up the route in `src/App.tsx`.
5. Add a tray card in `src/components/tray/TrayProviderCard.tsx`.

Look at `src-tauri/src/analytics/cursor.rs` as a reference implementation.

---

## Pull request checklist

Before opening a PR:

- [ ] `npx tsc --noEmit` passes with no errors
- [ ] `cargo test` passes with no failures
- [ ] New Rust logic has unit tests
- [ ] No hardcoded paths — use the path helpers in `src-tauri/src/utils/paths.rs`
- [ ] New Tauri commands are registered in `lib.rs`
- [ ] No plaintext secrets or API keys committed
- [ ] PR description explains *what* changed and *why*

---

## Commit message format

```
feat: add Gemini CLI analytics module
fix: correct cursor hook event mapping for PreToolUse
chore: bump version to 1.0.5
docs: add contributing guide
```

One of `feat`, `fix`, `chore`, `docs`, `refactor`, `test`. Keep the subject line under 72 characters.

---

## Reporting bugs

Open an issue and include:

- App version (shown in Settings → About)
- macOS / Windows version
- Steps to reproduce
- What you expected vs. what happened
- Console output from `View → Toggle Developer Tools` if relevant

---

## License

By contributing you agree that your changes will be licensed under the [MIT License](LICENSE).
