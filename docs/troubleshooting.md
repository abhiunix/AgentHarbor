# Troubleshooting

## "AgentHarbor is damaged and can't be opened"

This means macOS attached the quarantine attribute when downloading the DMG (most common on first install or when copying through some file managers). Clear it:

```bash
xattr -cr /Applications/AgentHarbor.app
```

Or run [`scripts/clear-quarantine-mac.sh`](../scripts/clear-quarantine-mac.sh) and pass the path. If you're seeing this on a freshly downloaded **signed and notarized** release, please file an issue — it should not happen.

## Claude Code says "Reconnect" or shows `Unauthenticated`

The OAuth access token in `~/.claude/.credentials.json` (or AgentHarbor's vault) is no longer accepted by Anthropic. Possible reasons:

- The Claude Code CLI rotated tokens without updating ours (rare, but happens after major CLI upgrades).
- The token expired and the refresh token is also stale.
- You revoked the OAuth grant from your Anthropic account settings.

Fix:

```bash
claude login            # writes a fresh ~/.claude/.credentials.json
```

Then click **Refresh** in the AgentHarbor tray, or wait ~60 s. AgentHarbor reads that file first, ahead of its own vault, so it always matches what `claude` itself uses.

## Tray shows "rate limited" but the CLI says "out of credits"

Anthropic returns HTTP 429 *and* `api_disabled_reason: "out_of_credits"` for billing-related blocks. AgentHarbor's `LimitState` ladder prefers the more specific **ApiDisabled** state — but the matching `/api/oauth/account` row may itself be 429-throttled, in which case we fall back to the last known good snapshot (up to 1 hour old).

If you still see the generic "rate limited" copy after a few minutes, try:

1. Disconnect and reconnect Claude Code in AgentHarbor.
2. Make sure the active token actually works: `claude -p "ping"` from a terminal.
3. If Anthropic returned a stale `out_of_credits` even though usage is clearly under cap, AgentHarbor will detect that and ignore it — but you may need to wait one refresh cycle.

## Codex tray shows the wrong percentage

Codex exposes both **Primary (5h)** and **Weekly (7d)** WHAM windows. The menu-bar title prefers Primary unless it's 0% used, in which case it falls back to Weekly. If you expected Primary's percentage but see a different number, confirm that the popover bar agrees — the same logic drives both surfaces.

## Gemini quota looks empty

The Cloud Code Assist API requires a project ID. AgentHarbor reads it from `loadCodeAssist`. If your Gemini OAuth doesn't have a `cloudai_companion_project` field (older logins), do `gemini --version` once after a recent CLI upgrade so the file is rewritten.

## Notarization failed during local build

`tauri build` reports `failed to bundle project`. Common causes:

- **`Developer ID Application: <name> (TEAMID): no identity found`** — your keychain has a different signing identity name than the one in `APPLE_SIGNING_IDENTITY`. Run:

  ```bash
  security find-identity -v -p codesigning
  ```

  and use whatever name appears (e.g. `Developer ID Application: <YOUR_NAME> (<YOUR_TEAM_ID>)`).

- **`Apple notarization service: HTTP 401`** — the `.p8` API key is for a different team or has been revoked. Regenerate from `appstoreconnect.apple.com/access/integrations/api`.

- **DMG creation hangs / AppleScript errors** — the `bundle_dmg.sh` step occasionally fails on macOS Sequoia. The `.app` will still be fully signed and notarized. Re-create the DMG manually:

  ```bash
  hdiutil create -volname "AgentHarbor" \
    -srcfolder src-tauri/target/release/bundle/macos/AgentHarbor.app \
    -ov -format UDZO \
    src-tauri/target/release/bundle/dmg/AgentHarbor.dmg

  codesign --force --sign "Developer ID Application: <name> (<TEAMID>)" \
    src-tauri/target/release/bundle/dmg/AgentHarbor.dmg
  ```

See [Build & Release](./build-and-release.md) for the full env-var rundown.

## "1 MCP server failed" in Claude Code after deploy

That message comes from the Claude CLI itself, not AgentHarbor. Common reasons:

- The MCP server's `command` isn't on `PATH` for the shell that launched `claude`.
- The MCP needs an env var (e.g. `GITHUB_TOKEN`) you haven't supplied. Open Settings → **Secrets** and add it; AgentHarbor merges secrets into `env` blocks at deploy time.

Run `claude mcp` (or click the `/mcp` link in Claude Code) for the per-server error log.

## Settings or projects file got corrupted

If the app refuses to launch after an update, look for malformed JSON in:

```
~/Library/Application Support/com.agentharbor.app/{settings,projects,presets}.json
```

Move the offender aside and restart — AgentHarbor will write a fresh default. Your tracked projects survive in macOS recents either way.

## Reset everything

```bash
rm -rf ~/Library/Application\ Support/com.agentharbor.app
```

Removes settings, presets, tokens, backups, and limit-state cache. Project files on disk (`.claude/`, `.cursor/`, …) are untouched.
