#!/usr/bin/env bash
# Reclaim disk from regenerable build caches.
#
# Everything removed here is rebuilt automatically by the next `cargo build`
# or `npm run tauri dev`; no source, git history, or config is touched.
# Run with --deep to also drop the whole cargo target dir (slowest rebuild).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if pgrep -f "target/debug/agentharbor" >/dev/null 2>&1; then
  echo "The dev app is running. Stop it first (its binary lives in target/)." >&2
  exit 1
fi

before=$(du -sk . | awk '{print $1}')

# Incremental compile cache: the largest and most disposable artifact.
rm -rf src-tauri/target/debug/incremental

# Duplicate target dirs inside agent worktrees.
for wt in .claude/worktrees/*/; do
  [ -d "$wt/src-tauri/target" ] && rm -rf "$wt/src-tauri/target"
done

# Vite build output.
rm -rf dist

if [ "${1:-}" = "--deep" ]; then
  rm -rf src-tauri/target
  echo "Removed the full cargo target dir; the next build recompiles from scratch."
fi

after=$(du -sk . | awk '{print $1}')
printf 'Freed %s GB (%s GB -> %s GB)\n' \
  "$(( (before-after)/1024/1024 ))" "$(( before/1024/1024 ))" "$(( after/1024/1024 ))"
