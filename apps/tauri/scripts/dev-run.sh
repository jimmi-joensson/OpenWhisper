#!/usr/bin/env bash
# One-command dev cycle for the Tauri shell on macOS.
#
# Why NOT `pnpm tauri dev`: that runs the bare cargo binary at
# `target/debug/openwhisper-tauri`, with no .app bundle and no
# Info.plist. TCC can't key Accessibility / Input Monitoring grants
# to a bare binary cleanly, so CGEventTap creation fails and the
# hotkey banner stays stuck even after granting in System Settings.
#
# Instead: `tauri build --debug` produces a real `OpenWhisper.app`
# at target/debug/bundle/macos/, with bundle id `com.openwhisper.app`
# and the signing identity TCC needs. We `open` that bundle.
#
# Trade-off: no HMR on Rust changes — every backend edit needs
# another `dev-run.sh` cycle. Frontend changes still hot-reload
# inside the bundled WebView.
#
# Ad-hoc codesigning still drifts each rebuild, so we always
# tccutil reset before launch — re-grant on first launch each cycle,
# matching the SwiftUI flow in `scripts/dev-run.sh`.
#
# Usage:
#   apps/tauri/scripts/dev-run.sh
set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
TAURI_DIR="$( cd "$SCRIPT_DIR/.." && pwd )"
REPO_ROOT="$( cd "$TAURI_DIR/../.." && pwd )"

BUNDLE_ID="com.openwhisper.app"
APP_PATH="$REPO_ROOT/target/debug/bundle/macos/OpenWhisper.app"

echo "==> Killing any running OpenWhisper (Tauri) instances"
pkill -f "OpenWhisper.app/Contents/MacOS/OpenWhisper" 2>/dev/null || true
pkill -f "target/debug/openwhisper-tauri" 2>/dev/null || true

echo "==> Resetting TCC grants for $BUNDLE_ID"
for SERVICE in Accessibility Microphone ListenEvent; do
    tccutil reset "$SERVICE" "$BUNDLE_ID" 2>/dev/null || true
done

# System Settings caches the Accessibility list and ignores tccutil's mutations
# until the app is restarted. Kicking it forces the next open to re-read TCC,
# so stale "OpenWhisper" entries from prior rebuilds disappear from the UI.
echo "==> Refreshing System Settings cache"
osascript -e 'tell application "System Settings" to quit' 2>/dev/null || true

echo "==> pnpm tauri build --debug (produces $APP_PATH)"
cd "$TAURI_DIR"
pnpm tauri build --debug

if [[ ! -d "$APP_PATH" ]]; then
    echo "error: built app not found at $APP_PATH" >&2
    exit 1
fi

echo "==> open $APP_PATH"
open "$APP_PATH"

cat <<EOF

Tauri dev run ready. Re-grant on first launch:
  1) Accessibility   → approve  (Right Cmd hotkey + paste)
  2) Microphone      → approve  (audio capture)

After grant, click Retry in the banner — the app will relaunch
once and the tap should install cleanly.
EOF
