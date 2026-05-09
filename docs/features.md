# AgentHarbor — Features

A visual tour of every major feature. Screenshots are ordered to match the in-app flow.

---

## Tray Analytics

AgentHarbor lives in your menu bar (macOS) or system tray (Windows). A single glance shows your active session burn, monthly spend, and rate-limit state for every AI provider — no browser, no CLI.
Clicking the tray icon opens a compact popover with per-provider cards. Each card shows the metric most relevant to that provider (session percentage, dollar spend, or quota bar).
<!-- SCREENSHOT: menu bar tray showing Claude Code session %, Cursor spend, Codex quota -->
> <img width="409" height="667" alt="image" src="https://github.com/user-attachments/assets/1e4ceefb-f941-4698-8b2a-210caa0db37f" />
> <img width="438" height="717" alt="image" src="https://github.com/user-attachments/assets/c10bc42e-f174-4ad2-9c93-d7a946b7af0d" />
> <img width="410" height="674" alt="image" src="https://github.com/user-attachments/assets/f1d446df-816b-4e34-bedb-d0cf2b59766a" />

---

## Provider Analytics Pages

Each provider has a dedicated full-screen analytics page with detailed usage breakdowns, cost history, and limit-state timeline.

<!-- SCREENSHOT: Claude Code analytics page — session timeline, Sonnet/Opus breakdown -->
> <img width="1727" height="994" alt="image" src="https://github.com/user-attachments/assets/1263f234-a338-422f-9829-056f0235f453" />
<img width="1723" height="904" alt="image" src="https://github.com/user-attachments/assets/0f415d6b-e8a9-4950-8fb5-76e6bcaa0712" />
<img width="1725" height="967" alt="image" src="https://github.com/user-attachments/assets/539ff4fa-ffdc-407d-827a-1decce9baac1" />

<!-- SCREENSHOT: Cursor analytics page — plan + on-demand spend breakdown -->
<img width="1724" height="868" alt="image" src="https://github.com/user-attachments/assets/5ae5eb03-5e33-4e3e-99d3-748ab10144ce" />


<!-- SCREENSHOT: Codex analytics page — primary/weekly windows, per-model cost -->
> <img width="1723" height="963" alt="image" src="https://github.com/user-attachments/assets/25fc7ead-19c5-4187-ad2e-5f8a9db2ca01" />


---

## Registry Browser

Browse, search, and filter the full library of bundled capabilities and community-contributed definitions — MCP servers, rules, skills, hooks, plugins, and custom files.

<!-- SCREENSHOT: registry page showing capability cards with type badges and adapter dots -->
> _Screenshot placeholder — capability registry browser_

Clicking a card opens the detail panel with full metadata, JSON preview, and adapter compatibility.

<!-- SCREENSHOT: capability detail slide-in panel -->
> _Screenshot placeholder — capability detail panel_

---

## Capability & Agent Editor

Create and edit private capabilities with structured forms — no raw JSON required for the common cases.

### MCP Server

Configure stdio and HTTP/SSE MCP servers with a JSON editor, environment variable manager, and live tool discovery.

<!-- SCREENSHOT: MCP capability editor with JSON textarea and env var rows -->
> _Screenshot placeholder — MCP server editor_

### Hook Editor

A two-tab structured form — one tab per adapter — with event dropdowns, matcher hints, hook type radios, timeout, and script file attachments.

<!-- SCREENSHOT: hook editor showing Claude Code tab with event dropdown and command field -->
> _Screenshot placeholder — Claude Code hook form_

<!-- SCREENSHOT: hook editor showing Cursor tab with beforeSubmitPrompt event and script files section -->
> _Screenshot placeholder — Cursor hook form with script files_

### Skill Editor

Define skill files with a SKILL.md entry point and optional helper scripts. Supports GitHub import via URL.

<!-- SCREENSHOT: skill editor with SKILL.md textarea and + Add File button -->
> _Screenshot placeholder — skill editor_

---

## Deploy Wizard

A four-step guided wizard deploys selected capabilities and agents to one or more IDE adapters simultaneously.

**Step 1 — Project:** Select a project folder or deploy to global config.

<!-- SCREENSHOT: project selector step with recent projects list -->
> _Screenshot placeholder — project selector_

**Step 2 — Select:** Choose adapters and pick which capabilities and agents to include.

<!-- SCREENSHOT: adapter selector with checkboxes for Claude Code, Cursor, Windsurf -->
> _Screenshot placeholder — adapter selector_

**Step 3 — Preview:** Review syntax-highlighted diffs (split / unified / raw) for every file that will be written. Override the per-file strategy (Replace / Merge / Append).

<!-- SCREENSHOT: deploy preview with split diff view and strategy dropdown -->
> _Screenshot placeholder — deploy diff preview_

**Step 4 — Success:** Deployment result with per-adapter status, file count, and one-click open in Finder, Cursor, or VS Code.

<!-- SCREENSHOT: deployment complete screen showing Cursor success, open-in buttons -->
> _Screenshot placeholder — deployment success screen_

---

## Undo Deploy

Every deployment creates a timestamped backup before writing any files. The success screen shows an **Undo Deploy** button that restores the exact pre-deploy snapshot in one click.

<!-- SCREENSHOT: deploy success screen with Undo Deploy button highlighted -->
> _Screenshot placeholder — undo deploy button_

---

## Presets

Bundle any set of capabilities into a named preset for one-click deploy. Ships with `Full-Stack Web` and `Data Science` examples. Add to a preset directly from the registry or create one from a selection.

<!-- SCREENSHOT: preset detail view showing capability list with + Add Capabilities button -->
> _Screenshot placeholder — preset editor_

---

## Drift Detection

When an external tool or teammate modifies a managed file, AgentHarbor shows a drift badge on the project. Opening the review panel shows a side-by-side diff with **Accept** (keep the external change) or **Restore** (rewrite from the last deploy snapshot).

<!-- SCREENSHOT: drift indicator badge on project card, and drift review panel with side-by-side diff -->
> _Screenshot placeholder — drift detection review_

---

## Agent Browser & Editor

Browse sub-agent definitions (`.md` files with YAML frontmatter), inspect their system prompt, model, memory scope, and required capabilities. Create or edit private agents with a structured form including color swatch, tool checkboxes, and a system prompt textarea.

<!-- SCREENSHOT: agent card grid with model/memory badges and color bars -->
> _Screenshot placeholder — agent browser_

<!-- SCREENSHOT: agent editor modal with name, color picker, tools, and system prompt -->
> _Screenshot placeholder — agent editor_

---

## Secrets Manager

Sensitive values (API keys, webhook URLs) are stored in the macOS Keychain — never on disk in plaintext. The Secrets Manager lets you add, reveal, edit, and delete secrets, and they are automatically injected into MCP `env` blocks at deploy time.

<!-- SCREENSHOT: secrets manager modal with masked key list and reveal button -->
> _Screenshot placeholder — secrets manager_

---

## Agent Memory Management

View and clear the persistent memory files that Claude Code and other agents write to disk — per-project and global — directly from the project detail panel.

<!-- SCREENSHOT: agent memory section in project detail showing file list and Clear buttons -->
> _Screenshot placeholder — agent memory manager_

---

## Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Cmd K` | Focus search |
| `Cmd D` | Open deploy wizard |
| `Cmd N` | New capability / agent |
| `Cmd ,` | Settings |
| `Cmd R` | Sync registry |
| `Cmd /` | Shortcuts help overlay |
| `Esc` | Close modal |

<!-- SCREENSHOT: shortcuts help overlay -->
> _Screenshot placeholder — keyboard shortcuts overlay_

---

## Menu Bar Mode

Enable **Keep running on close** in Settings to hide the main window without quitting. The tray icon remains active for quick access to deploy, sync, and analytics — zero desktop footprint.

<!-- SCREENSHOT: settings panel showing Menu Bar Mode toggle -->
> _Screenshot placeholder — menu bar mode setting_

---

## Auto-Update

AgentHarbor checks for new releases every 4 hours. When an update is available, an in-app banner appears with a one-click install button and a 24-hour snooze option. No manual downloads needed.

<!-- SCREENSHOT: update banner at the top of the app with Install and Snooze buttons -->
> _Screenshot placeholder — auto-update banner_
