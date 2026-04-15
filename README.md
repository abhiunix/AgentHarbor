# AgentHarbor

A macOS desktop app for managing AI coding agent configurations across Claude Code, Cursor, Windsurf, Gemini CLI, and Codex.

## Features

- **Unified Analytics Dashboard** — Rate limits, token usage, cost tracking, and session stats across all providers
- **System Tray** — Real-time provider status with 4 tabs (Claude Code, Cursor, Codex, Gemini) and background refresh
- **Deploy Capabilities** — MCP servers, skills, rules, hooks, and agents to multiple IDEs simultaneously
- **Cost Engine** — Token deduplication and tiered pricing matching CodexBar's calculation logic
- **Gemini CLI Analytics** — Quota monitoring (Pro/Flash/Flash Lite), telemetry file parsing
- **Claude /stats Metrics** — Favorite model, streaks, active days, peak hour
- **Private Repo Support** — GITHUB_TOKEN from Secrets Manager for importing skills from private repos
- **macOS Keychain** — Secure credential storage with auto token refresh
- **Signed & Notarized** — Gatekeeper-approved distribution

## Supported Providers

| Provider | Analytics | Rate Limits | Cost Tracking | Deploy |
|----------|-----------|-------------|---------------|--------|
| Claude Code | Full | Session/Weekly/Sonnet | Tiered pricing | MCP, Skills, Rules, Hooks, Agents |
| Cursor | Full | Usage/Credits | Subscription tracking | MCP, Rules, Agents |
| Codex (OpenAI) | Full | Primary/Weekly | Per-model costs | MCP, Skills |
| Gemini CLI | Quota | Pro/Flash/Flash Lite | N/A (quota-based) | Skills, Hooks, Agents |
| Windsurf | Config | N/A | N/A | MCP, Rules |

## Tech Stack

| Layer | Technology |
|-------|------------|
| Framework | Tauri v2 |
| Backend | Rust |
| Frontend | React + TypeScript + Vite |
| Styling | Tailwind CSS v3 (dark theme) |
| State | Zustand |
| Build Target | macOS (universal binary) |

## Getting Started

### Prerequisites

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://rustup.rs/)
- [Tauri CLI](https://tauri.app/start/): `cargo install tauri-cli`

### Development

```bash
npm install
npm run tauri dev
```

### Production Build

```bash
npm run tauri build
```

The signed `.app` will be at `src-tauri/target/release/bundle/macos/AgentHarbor.app`.

## Project Structure

```
agentharbor/
├── src/                    # React frontend
│   ├── components/         # UI components (tray, registry, deploy, settings)
│   ├── pages/              # Route pages (analytics, config, etc.)
│   ├── stores/             # Zustand state stores
│   ├── lib/                # Utilities, Tauri bindings, types
│   └── hooks/              # Custom React hooks
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── analytics/      # Provider analytics (Claude, Cursor, Codex, Gemini)
│   │   ├── commands/       # Tauri IPC commands
│   │   ├── adapters/       # IDE adapter implementations
│   │   ├── registry/       # Capability registry loader & sync
│   │   ├── utils/          # File I/O, keychain, paths
│   │   ├── tray.rs         # System tray with popover
│   │   └── lib.rs          # App setup & command registration
│   └── tauri.conf.json     # Tauri configuration
└── registry/               # Bundled capability definitions
```

## License

MIT
