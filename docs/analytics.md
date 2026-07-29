# Analytics & Tray

AgentHarbor's menu-bar icon, popover, and per-provider analytics pages all draw from the same in-memory cache that's refreshed every ~120 s in the background (faster when limits are tight). This page describes what each surface shows and why.

## Menu-bar icon

| Provider | Title content | Source |
|---|---|---|
| Claude Code (Pro/Max) | `XX%` of the active **Session (5h)**, falling back to **Weekly** when session is 0% | `/api/oauth/usage` |
| Claude Code (Enterprise) | `$N` total spend this cycle (capped or uncapped) | `/api/oauth/usage` `extra_usage` |
| Cursor | `$N` total spend = `plan_included_usd + plan_bonus_usd + on_demand_used_usd` | Cursor `/api/...` |
| Codex | `XX%` of the **Primary (5h)** WHAM window (Weekly only when Primary is 0%) | OpenAI `wham` |
| Gemini CLI | `XX%` of the highest-priority tier with quota remaining: **Pro → Flash → Flash Lite** | Cloud Code Assist |

The icon swaps to its red `*-active.png` variant and an `!` is appended to the title when the active provider's `LimitState` is **Reached**, **ApiDisabled**, **SubscriptionIssue**, **BillablePaused**, **RateLimited**, or **Unauthenticated**. Clicking another tab in the popover changes the active provider; the title updates immediately.

<img src="assets/tray-icon-macos.png" alt="macOS menu bar showing the AgentHarbor icon with the active session percentage" width="480">

## Tray popover

<img src="assets/tray-popover.png" alt="Tray popover with per-provider quota bars, spend, and session stats" width="480">

The header strip shows:

- **Connection dots** — green for connected providers, gray for disconnected, red for danger states.
- **Active limit summary** on the right: enterprise spend, "Limit / billing issue" pill, the next constrained window, or a "Show AgentHarbor" link when everything's healthy.

Per-provider cards include:

- **Rate-limit bars** — used % with reset time. Bars styled red when `LimitState` is *Reached*.
- **Credit / spend** — Claude Enterprise monthly spend with "Usage this cycle" %, Cursor included/bonus/on-demand split, Cursor team on-demand row with `· NN%` percentage.
- **Today / This Week** session stats from local logs (Claude Code projects, Gemini telemetry, etc.).
- **Limit banner** — friendly copy for `out_of_credits`, `trial_expired`, `payment_failed`, `Unauthenticated` (with a Reconnect button).

## `LimitState` ladder

`derive_claude_limit_state` (and equivalents for other providers) chooses the most actionable state, in this order:

1. **Unauthenticated** — at least one core OAuth call returned 401 *and* we have no fresh `/usage` or `/profile` data. Reconnect required.
2. **ApiDisabled** — `api_disabled_reason` set on the active org or its parent (e.g. `out_of_credits`, `trial_expired`). `out_of_credits` is only surfaced for **Enterprise** plans (which have a hard monthly spend cap); it is suppressed for Pro/Max/Team since those plans use time-windowed rate limits, not credit caps.
3. **BillablePaused** — `billable_usage_paused_until` set in the future.
4. **SubscriptionIssue** — `subscription_status` is anything other than `active` / `trialing`.
5. **RateLimited** — HTTP 429 returned by `/usage` when no other state applies. The retry-after seconds are humanised (e.g. `1h 23m`).
6. **Reached** — any rate-limit window at 100% used.
7. **Approaching** — any rate-limit window at ≥ 80% used.
8. **Healthy** — none of the above.

Cache TTL is **5 minutes** by default and **60 s** when in any "fast-refresh" state, but capped at **90 s** to limit how stale a previously-healthy snapshot can look after credentials break. On an HTTP 401 the entire Claude analytics cache is also flushed so the next read sees the live failure.

## Per-provider analytics pages

Open from the tray (**Show Full Analytics**) or sidebar.

- **Claude Code** — usage windows, Account & Billing tile row (Rate Limit Tier, Extra Usage, Usage this cycle, API-Equiv. Value, Billing Type), session stats, model-aware cost analysis.
- **Cursor** — included/bonus/on-demand spend, plan totals, hooks/permissions/transcripts/rules/plans pages.
- **Codex** — WHAM Primary/Weekly/Secondary windows, per-model API-equivalent cost, session counts.
- **Gemini CLI** — Pro/Flash/Flash Lite quota bars, project ID, telemetry-derived counters.

## Privacy

Local-only data (Claude project JSONLs, Gemini telemetry files) is read with file-share-friendly opens, never copied off-disk, and never sent anywhere. The only network calls AgentHarbor makes are to each provider's official endpoints with **your** OAuth tokens.
