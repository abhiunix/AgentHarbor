# AgentHarbor

A macOS desktop app for managing AI coding-agent configurations across **Claude Code**, **Cursor**, **Codex (OpenAI)**, **Gemini CLI**, and **Windsurf** — with live analytics, deploy/undo workflow, presets, drift detection, and limit-state notifications.

Built with Tauri v2 (Rust + React).

[![GitHub release](https://img.shields.io/github/v/release/abhiunix/AgentHarbor?label=latest&color=blue)](https://github.com/abhiunix/AgentHarbor/releases/latest)
[![Total Downloads](https://img.shields.io/github/downloads/abhiunix/AgentHarbor/total?label=downloads&color=green)](https://github.com/abhiunix/AgentHarbor/releases)
[![Platform](https://img.shields.io/badge/platform-macOS%2013%2B-lightgrey)](https://github.com/abhiunix/AgentHarbor/releases/latest)
[![License](https://img.shields.io/github/license/abhiunix/AgentHarbor)](LICENSE)

## Installation

> macOS 13+, Apple Silicon. Signed with Apple Developer ID and notarized.

1. **Download the latest DMG:** [**AgentHarbor — latest release**](https://github.com/abhiunix/AgentHarbor/releases/latest)
   - Pick `AgentHarbor_<version>_aarch64.dmg`.
2. Open the DMG and drag **AgentHarbor.app** into **Applications**.
3. Launch from Spotlight or `/Applications`.

If macOS ever flags it as "damaged" (only happens when the quarantine attribute survives an unusual download path):

```bash
xattr -cr /Applications/AgentHarbor.app
```

The full first-run walkthrough lives in [`docs/getting-started.md`](docs/getting-started.md).

## Features

- **Tray analytics** — connection dots, per-provider rate limits, monthly spend, and a smart menu-bar metric:
  - Claude Pro/Max → active **Session (5h)** %
  - Claude Enterprise → total **$ spend** (capped or uncapped)
  - Cursor → total **$ spend** = included + bonus + on-demand
  - Codex → **Primary (5h)** %
  - Gemini → **Pro → Flash → Flash Lite** (first tier with quota)
- **Limit-state ladder** — `Unauthenticated`, `ApiDisabled` (`out_of_credits`, `trial_expired`, …), `BillablePaused`, `SubscriptionIssue`, `RateLimited` (with friendly retry-after copy), `Reached`, `Approaching`, `Healthy` — surfaced in the tray, the analytics page, and as native notifications.
- **Deploy wizard** — multi-adapter, syntax-highlighted diffs, per-file `Replace`/`Merge`/`Append` strategy, automatic backups, and an **Undo Deploy** button.
- **Presets** — bundle capabilities for one-click deploys; bundled `Full-Stack Web` / `Data Science` examples plus your own.
- **Drift detection** — file hashes per managed file; side-by-side diff with **Accept** or **Restore** when something changed externally.
- **Cost engine** — per-model API-equivalent costs for Claude (Opus/Sonnet/Haiku) and Codex (GPT-5/4/3.5), plus token-deduplicated session totals.
- **Native macOS** — Keychain integration, system tray with click-through popover, optional menu-bar mode, native notifications.
- **Auto-update** — Tauri updater verifies signatures from `.signing/.TAURI_SIGNING_PRIVATE_KEY`; users get a prompt when new releases land.

## Supported providers

| Provider | Analytics | Rate limits / spend | Deploy targets |
|---|---|---|---|
| Claude Code | Full (Pro/Max/Enterprise) | Session 5h, Weekly, Sonnet/Opus, monthly $ | MCP, skills, rules, hooks, agents |
| Cursor | Full | Plan included + bonus + on-demand $, team OD | MCP, rules, agents |
| Codex (OpenAI) | Full | Primary 5h, Weekly 7d, per-model $ | MCP, skills |
| Gemini CLI | Quota | Pro / Flash / Flash Lite | Skills, hooks, agents |
| Windsurf | Config | – | MCP, rules |

## Documentation

- [Getting Started](docs/getting-started.md)
- [Analytics & Tray](docs/analytics.md)
- [Deploying Capabilities & Agents](docs/deploying-capabilities.md)
- [Build & Release](docs/build-and-release.md)
- [Troubleshooting](docs/troubleshooting.md)
- [Regression Checklist](docs/regression-checklist.md)
- [Changelog](CHANGELOG.md)

## Tech stack

| Layer | Technology |
|---|---|
| Framework | Tauri v2 |
| Backend | Rust (`reqwest`, `serde`, `keyring`, `chrono`) |
| Frontend | React + TypeScript + Vite |
| Styling | Tailwind CSS v3 (dark theme) |
| State | Zustand |
| Build target | macOS aarch64 |

## Development

```bash
npm install
npm run tauri dev          # dev with hot reload
npm run test:regression    # tsc + vite build + cargo test
```

For signed/notarized release builds, see [`docs/build-and-release.md`](docs/build-and-release.md).

## Project layout

```
agentharbor/
├── src/                    React frontend
│   ├── components/         UI (tray, registry, deploy, settings, analytics)
│   ├── pages/              Route pages
│   ├── stores/             Zustand state
│   ├── lib/                Tauri IPC, types, utilities
│   └── hooks/              Custom hooks
├── src-tauri/              Rust backend
│   ├── src/
│   │   ├── analytics/      Per-provider analytics + tray commands
│   │   ├── adapters/       Adapter implementations (Claude/Cursor/Windsurf/…)
│   │   ├── commands/       Tauri IPC commands
│   │   ├── registry/       Capability registry loader & validator
│   │   ├── utils/          File I/O, keychain, paths, drift, manifest, backup
│   │   ├── tray.rs         System tray
│   │   └── lib.rs          App setup & command registration
│   └── tauri.conf.json
├── registry/               Bundled capability/agent definitions
├── docs/                   User & contributor docs
├── scripts/                bump-build, clear-quarantine, tauri-wrapper
└── public/                 Static assets served by Vite
```

## License

MIT
