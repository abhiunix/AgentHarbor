# Getting Started

This guide walks you from a fresh macOS install to a fully connected AgentHarbor with at least one project deployed.

## 1. Install

1. Download the latest DMG from the [GitHub Releases page](https://github.com/abhiunix/AgentHarbor/releases/latest).
   - Pick `AgentHarbor_<version>_aarch64.dmg` for Apple Silicon.
2. Open the DMG and drag **AgentHarbor.app** into **Applications**.
3. Launch it from Spotlight or `/Applications`.

The app is **signed** with a Developer ID certificate and **notarized** by Apple, so Gatekeeper allows it without `xattr -cr` workarounds. If you ever do see "AgentHarbor is damaged or can't be opened":

```bash
xattr -cr /Applications/AgentHarbor.app
```

(The repo also ships [`scripts/clear-quarantine-mac.sh`](../scripts/clear-quarantine-mac.sh) for the same purpose.)

### System requirements

- macOS 13 (Ventura) or newer.
- Apple Silicon. (Intel builds aren't published right now.)
- ~50 MB disk for the app, plus state under `~/Library/Application Support/com.agentharbor.app`.

## 2. First launch

On first launch you'll see four provider tabs: **Claude Code**, **Cursor**, **Codex**, and **Gemini CLI**. Each one is initially **Disconnected**; AgentHarbor never reads tokens silently — you connect explicitly.

### Connect Claude Code

AgentHarbor honours the same OAuth credentials that the `claude` CLI uses. Order of preference:

1. `~/.claude/.credentials.json` (the file `claude` itself writes after `claude login`).
2. AgentHarbor's stored token (set via the **Sign In** button in the app).
3. Stored refresh token, used automatically when the access token expires.

If you've already run `claude login` in your terminal, AgentHarbor will pick that up on the next refresh — no extra step needed. Otherwise click **Sign In** on the Claude Code tab.

### Connect Cursor

Click **Connect Cursor** and follow the prompts. AgentHarbor reads Cursor's local SQLite database to detect the session token, with manual paste as a fallback.

### Connect Codex (OpenAI)

Cleanly supports both ChatGPT-OAuth-based Codex and `OPENAI_API_KEY` flows. Pick whichever the **Connect** sheet offers based on the credentials it finds.

### Connect Gemini CLI

AgentHarbor reads `~/.gemini/oauth_creds.json` and refreshes Google OAuth tokens automatically. If you haven't run the CLI yet, install it and run any command once so the file exists.

## 3. Add your first project

1. Open the **Projects** tab in the sidebar.
2. Click **Add Project** and select a folder (anything with `.claude/`, `.cursor/`, `.windsurfrules`, or `.codex/` is auto-detected).
3. AgentHarbor lists the adapters it found (Claude Code, Cursor, Windsurf…) along with any deployed capabilities and agents.

## 4. Deploy your first capability

Open the **Library** tab, pick something simple like a single MCP server (e.g. **github-mcp**), and click **Deploy**. The wizard will:

1. Show a diff of the files it's about to write.
2. Create a backup of any existing config (so you can undo).
3. Write the new config atomically.
4. Stamp `<!-- AgentHarbor -->` markers in `CLAUDE.md` / `AGENTS.md` so it can detect drift later.

See [Deploying Capabilities & Agents](./deploying-capabilities.md) for the full flow, presets, and undo.

## 5. Optional — enable native notifications

Settings → **Analytics** → "Show limit-reached notifications". When enabled, AgentHarbor sends a native macOS notification on transitions like *Approaching → Reached*, *Healthy → ApiDisabled*, *RateLimited → Healthy*, etc. — once per transition, not every refresh.

## Where things live

| Path | What |
|---|---|
| `~/Library/Application Support/com.agentharbor.app/settings.json` | App settings (analytics toggles, registry config, deploy defaults) |
| `~/Library/Application Support/com.agentharbor.app/projects.json` | Tracked projects and recent paths |
| `~/Library/Application Support/com.agentharbor.app/presets.json` | Custom presets you created |
| `~/Library/Application Support/com.agentharbor.app/provider-tokens.json` | Encrypted-on-disk OAuth tokens for each provider |
| `~/Library/Application Support/com.agentharbor.app/backups/<hash>/<timestamp>/` | Per-project deploy backups |
| `~/Library/Application Support/com.agentharbor.app/limit-state.json` | Last seen `LimitState` per provider (for de-duping notifications) |
