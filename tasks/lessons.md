# Lessons learned — AgentHarbor

Project-specific corrections to incorporate. Append a new entry every time the user redirects approach. Each entry: the rule, **Why:**, **How to apply:**.

## Session 2026-05-15 (initial setup)

### Honor existing branch state before creating one

**Rule:** When `git checkout -b <branch>` fails with "already exists", do not `-B` or force-create. Check out the existing branch, diff against `main`, and inspect uncommitted edits before touching anything.

**Why:** Prior-session work may live on the branch unsquashed. Force-creating destroys it silently.

**How to apply:** Always start a session by running `git status` + `git log --oneline main..HEAD` + `git diff` on the target branch before adding new commits. Fold prior edits into the new commit only after confirming they belong to the same logical change.

### Doc-only sessions still get scaffolding

**Rule:** Even when the user says "doc only this session," still set up `CLAUDE.md`, `tasks/todo.md`, `tasks/lessons.md` if they don't exist. Future sessions need them.

**Why:** The global instructions require these files. Skipping them means the next session re-litigates conventions and re-discovers project quirks.

**How to apply:** Treat the scaffolding as part of "doc only" rather than a separate phase.

### Push to upstream needs explicit confirmation per session

**Rule:** "Push to upstream" approval was scoped to this session's `dan/benchmark-roadmap` branch. It does not authorize merges, force pushes, or pushes to `main`.

**Why:** Authorization is for the action and scope requested, not a blanket grant.

**How to apply:** Re-ask before any push that isn't a normal commit-and-push to the named `dan/` branch.
