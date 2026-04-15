#!/usr/bin/env bash
# Clear macOS quarantine so AgentDock.app from the DMG can be opened without "damaged" error.
# Run after mounting the DMG and copying the app, or point it at the .app path.
set -e
APP_PATH="${1:-}"
if [ -z "$APP_PATH" ]; then
  # Default: app inside Tauri's release bundle
  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  APP_PATH="$SCRIPT_DIR/../src-tauri/target/release/bundle/macos/AgentDock.app"
fi
if [ ! -d "$APP_PATH" ]; then
  echo "Usage: $0 [path/to/AgentDock.app]"
  echo "Example: $0 /Applications/AgentDock.app"
  echo "Or after mounting DMG: $0 /Volumes/AgentDock\\ 1.0.16/AgentDock.app"
  exit 1
fi
echo "Clearing quarantine on: $APP_PATH"
xattr -cr "$APP_PATH"
echo "Done. You can open the app now."
