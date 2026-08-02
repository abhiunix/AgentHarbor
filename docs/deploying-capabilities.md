# Deploying Capabilities & Agents

AgentHarbor treats every IDE config piece (MCP server, rule, skill, hook, plugin, sub-agent) as a **universal capability** that can be deployed to one or more **adapters** (Claude Code, Cursor, Windsurf, Codex, Gemini). This page covers how to use the deploy wizard end-to-end.

## Concepts

| Term | Meaning |
|---|---|
| Capability | A single deployable thing: an MCP server, a rule, a skill, a hook, or a plugin |
| Agent | An AI sub-agent, deployed as a `agents/*.md` file (shared across Claude Code and Cursor) |
| Adapter | A target IDE — Claude Code, Cursor, Windsurf, Codex, or Gemini CLI |
| Composite ID | `author/name`, e.g. `community/github-mcp` |
| Preset | A named bundle of capabilities for one-click deploy |

## Deploy wizard

Open it from the **Deploy** button in the header, the **Deploy** action on a capability/agent card, or the **Deploy preset** button on a preset.

The wizard has 4 steps:

1. **Project** — pick a folder. AgentHarbor detects which adapters are usable (`.claude/`, `.cursor/`, `.windsurfrules`, `.codex/`, etc.).
2. **Adapter selection** — multi-select the adapters to deploy to. Capability cards show which adapters they support; agents support every adapter that uses `agents/*.md`.
3. **Preview** — a live diff per adapter:
   - **Split / Unified / Raw** view toggle.
   - Per-file strategy dropdown (`Replace` / `Merge` for JSON / `Append`).
   - Backup banner ("A backup of these files will be created before writing").
4. **Success** — list of files written, "Open in Finder" links, and an **Undo Deploy** button (see Backups below).

<img src="assets/deploy-wizard-diff.png" alt="Deploy wizard preview step showing a split diff with per-file strategy" width="720">

### What gets written

| Adapter | Path |
|---|---|
| Claude Code | `<project>/.claude/settings.json`, `<project>/CLAUDE.md`, `<project>/.claude/skills/*.md`, shared `<project>/agents/*.md` |
| Cursor | `<project>/.cursor/mcp.json`, `<project>/.cursorrules`, shared `<project>/agents/*.md` |
| Windsurf | `<project>/.windsurf/mcp_config.json`, `<project>/.windsurfrules` (no agents support) |

Global-scope deploys go to `~/.claude/`, `~/.cursor/`, `~/.codeium/windsurf/` respectively. Use the **Global Config** view in the sidebar to manage those.

### Manifest comments

After every deploy AgentHarbor writes (or updates) a `<!-- AgentHarbor: Deployed Capabilities -->` block in `CLAUDE.md` and `AGENTS.md`. This is what powers drift detection.

## Presets

Presets live in `~/Library/Application Support/com.agentharbor.app/presets.json`.

- **Save as preset** — from the multi-select bar in Library or from any capability detail.
- **Edit preset** — `+ Add Capabilities` and `✕ Remove` inline.
- **Deploy preset** — fires the deploy wizard with the preset's capabilities pre-selected.

## Backups & undo

Every deploy writes a manifest under:

```
~/Library/Application Support/com.agentharbor.app/backups/<projectHash>/<timestamp>/
  manifest.json
  <relative paths of pre-deploy file copies>
```

<img src="assets/deploy-success-undo.png" alt="Deploy success screen with the Undo Deploy button" width="720">

The success screen has an **Undo Deploy** button that restores from the most recent backup. From the project detail view you can also list, restore, or delete older backups. A launch-time cleanup keeps the per-project backup count bounded.

## Drift detection

Once deployed, AgentHarbor stores hashes of every managed file. On project open it compares those hashes against the current files and shows a **Drift** badge if anything was changed externally (a teammate edited `CLAUDE.md`, you tweaked `.cursorrules` directly, etc.).

<img src="assets/drift-review.png" alt="Drift review modal with a side-by-side diff and Accept/Restore actions" width="720">

The **Drift Review** modal shows side-by-side diffs and offers two actions per file:

- **Accept drift** — record the new content as the new baseline.
- **Restore** — overwrite the file with the original deployed content.

## Custom capabilities

Use **+ New** in the header to author your own MCP, rule, skill, hook, plugin, or agent. Custom items live in the local registry only (not synced to community). They're editable and deletable from their detail view.

## Removing capabilities

From the project detail view, click any deployed capability and choose **Remove**. AgentHarbor reverses the deploy:

- JSON keys it added are deleted (other keys are untouched).
- Markdown blocks bounded by its manifest comment are removed.
- Standalone files (skills, agent `.md` files) are deleted.

A backup is created before removal, so this is also undoable.
