#!/usr/bin/env bash
# Wrapper so "npm run tauri build" on macOS clears quarantine on the built app before creating the DMG.
set -e
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [ "$1" = "build" ] && [ "$(uname)" = "Darwin" ]; then
  # Build without bundling so we can clear quarantine before creating the DMG
  npx tauri build --no-bundle
  npm run clear-quarantine
  # Now create DMG (and other bundles); the .app that goes in will already be cleared.
  # CI=true skips AppleScript/Finder steps in create-dmg that can fail (e.g. "Not enough arguments" or AppleEvent errors).
  CI=true npx tauri bundle
else
  exec npx tauri "$@"
fi
