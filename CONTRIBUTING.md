# Contributing to AgentHarbor

Bug fixes, provider analytics improvements, docs fixes, and UI polish are all welcome.

## Dev setup

```bash
npm install
npm run tauri dev          # Vite on :1420 + Tauri window, Rust hot-restart
```

Prerequisites: Node.js 18+, Rust (stable), and on macOS the Xcode command-line tools. See [docs/build-and-release.md](docs/build-and-release.md) for release builds — you never need signing keys for a contribution.

## Before you open a PR

Run the same checks CI runs:

```bash
npx tsc --noEmit                                            # frontend types
cd src-tauri && cargo test                                  # Rust tests
cd src-tauri && cargo clippy --all-targets -- -D warnings   # Rust lint
```

or the combined gate: `npm run test:regression`.

> **Note:** `npm run build` auto-bumps the patch version in three files. Set `SKIP_VERSION_BUMP=1` for plain build checks, and don't include version-bump diffs in your PR.

## PR checklist

- [ ] `npx tsc --noEmit` passes
- [ ] `cargo test` passes (`cd src-tauri`)
- [ ] `cargo clippy --all-targets -- -D warnings` is clean
- [ ] No version-bump changes (`package.json` / `tauri.conf.json` / `Cargo.toml`) unless the PR is a release
- [ ] New Tauri commands are registered in `src-tauri/src/lib.rs` (`invoke_handler!`) **and** wrapped in `src/lib/tauri.ts`
- [ ] File writes to managed configs go through `utils/paths.rs` helpers (`atomic_write`, `normalize_line_endings`)
- [ ] No secrets, tokens, or `.env` files in the diff
- [ ] Docs updated if behavior documented in `docs/` changed

## Commit conventions

Conventional commits, as in the existing history: `feat(scope): …`, `fix(scope): …`, `docs: …`, `chore: …`.

## Good places to start

- Issues labeled [`good first issue`](https://github.com/abhiunix/AgentHarbor/labels/good%20first%20issue)
- Docs under `docs/` — corrections and troubleshooting entries
- UI polish in `src/components/`

Capability/agent definitions (MCPs, rules, skills) are synced from the community registry repo configured in Settings → Registry, not from this repo's `registry/` directory — contribute those there.

## Architecture orientation

[CLAUDE.md](CLAUDE.md) is the fastest map of the codebase: adapter trait, deploy pipeline, analytics subsystem, and cross-platform file-handling rules.
