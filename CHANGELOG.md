# Changelog

All notable changes to AgentHarbor are documented here.

## [1.0.118] - 2026-05-04

### Analytics & tray

- **Claude Code:** Limit-state ladder with friendly messages; HTTP 401/`Unauthenticated` reconnect flow; prefer `~/.claude/.credentials.json` over the in-app vault so analytics match Claude CLI; avoid stale `/account` cache masking auth failures; shorter analytics cache staleness cap.
- **Cursor:** Menu bar shows total spend (included + bonus + on-demand); Team On-Demand line shows spend percentage in the popover.
- **Codex:** Menu bar / tray title uses **Primary (5h)** usage % instead of the tighter Weekly window.
- **Gemini:** Menu bar and popover header pick **Pro → Flash → Flash Lite** by remaining quota (first tier with headroom).
- **Claude analytics page:** “Usage this cycle” tile on Account & Billing (monthly spend vs cap).

### Settings

- Analytics toggles for internal usage buckets and limit notifications (with notification permission on launch when enabled).

### Other

- Structured HTTP errors (including 429 + `Retry-After`); `LimitStateBanner` component for tray and Claude analytics.
