#!/usr/bin/env bash
# Clear macOS quarantine so AgentHarbor.app from the DMG can be opened without
# a "damaged" or "untrusted developer" error. Run after copying the app out of
# the mounted DMG, or pass an explicit path.
set -e
APP_PATH="${1:-}"
if [ -z "$APP_PATH" ]; then
  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
  APP_PATH="$SCRIPT_DIR/../src-tauri/target/release/bundle/macos/AgentHarbor.app"
fi
if [ ! -d "$APP_PATH" ]; then
  echo "Usage: $0 [path/to/AgentHarbor.app]"
  echo "Example: $0 /Applications/AgentHarbor.app"
  echo "After mounting DMG: $0 \"/Volumes/AgentHarbor 1.0.118/AgentHarbor.app\""
  exit 1
fi
echo "Clearing quarantine on: $APP_PATH"
xattr -cr "$APP_PATH"
echo "Done. You can open the app now."
