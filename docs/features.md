# Features

A visual tour of AgentHarbor's major features. Each section links to the guide that covers it in depth.

## Live tray analytics

The menu-bar icon shows the most relevant metric for the active provider — session percentage for Claude Code and Codex, spend for Cursor and Claude Enterprise, quota tier for Gemini CLI — refreshed every ~60 s.

<img src="assets/tray-icon-macos.png" alt="macOS menu bar showing the AgentHarbor icon with the active session percentage" width="480">

Clicking the icon opens the popover: connection dots, per-provider rate-limit bars with reset times, spend breakdowns, and today/this-week session stats.

<img src="assets/tray-popover.png" alt="Tray popover with per-provider quota bars, spend, and session stats" width="480">

Details: [Analytics & Tray](./analytics.md).

## Limit-state ladder & notifications

Every provider is mapped onto a single `LimitState` ladder — `Unauthenticated → ApiDisabled → BillablePaused → SubscriptionIssue → RateLimited → Reached → Approaching → Healthy` — and native notifications fire once per transition. The tray icon swaps to its red variant with a `!` in any non-healthy state.

<img src="assets/limit-notification.png" alt="Native macOS notification for a limit-state transition" width="480">

## Per-provider analytics pages

Full pages for each provider: usage windows, account & billing tiles, model-aware cost analysis for Claude; included/bonus/on-demand spend for Cursor; WHAM windows and per-model cost for Codex; tier quota bars for Gemini CLI.

<img src="assets/analytics-claude.png" alt="Claude Code analytics page with usage windows and cost analysis" width="720">

## Deploy wizard

Deploy an MCP server, rule, skill, hook, or sub-agent to any combination of adapters. Step 3 shows a live per-adapter diff with Split/Unified/Raw views and a per-file `Replace / Merge / Append` strategy; a backup is created before every write.

<img src="assets/deploy-wizard-diff.png" alt="Deploy wizard preview step showing a split diff with per-file strategy" width="720">

## Undo Deploy

The success screen lists every file written and offers one-click undo, restoring the pre-deploy snapshot from the backup store.

<img src="assets/deploy-success-undo.png" alt="Deploy success screen with the Undo Deploy button" width="720">

Details: [Deploying Capabilities & Agents](./deploying-capabilities.md).

## Drift detection

AgentHarbor hashes every managed file. When a teammate or another tool edits one, the project shows a **Drift** badge and the review modal offers side-by-side Accept / Restore per file.

<img src="assets/drift-review.png" alt="Drift review modal with a side-by-side diff and Accept/Restore actions" width="720">

## Presets

Bundle any set of capabilities for one-click deploy; presets are editable inline and reusable across projects.

<img src="assets/presets.png" alt="Presets view with bundled and custom presets" width="720">

## Secrets manager

Sensitive env vars (e.g. `GITHUB_TOKEN` for an MCP server) are stored in the macOS Keychain and injected into MCP `env` blocks at deploy time — never written to disk in plaintext.

<img src="assets/secrets-manager.png" alt="Secrets settings backed by the macOS Keychain" width="720">

## Auto-update

The Tauri updater checks GitHub Releases every 4 hours; an in-app banner offers one-click install with an optional 24 h snooze.
