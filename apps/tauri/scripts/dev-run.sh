#!/usr/bin/env bash
# One-command dev cycle for the Tauri shell on macOS, with frontend HMR.
#
# Why NOT `pnpm tauri dev`: that runs the bare cargo binary at
# `target/debug/openwhisper-tauri`, with no .app bundle and no
# Info.plist. TCC can't key Accessibility / Input Monitoring grants
# to a bare binary, so CGEventTap creation fails and the hotkey
# banner stays stuck even after granting in System Settings.
#
# Instead: build a real .app bundle whose WebView loads from
# Vite's dev server (frontendDist = http://localhost:1420 in
# tauri.dev.conf.json). The bundle has a stable id
# (com.openwhisper.app.dev) so TCC can grant Accessibility +
# Microphone, AND the WebView hot-reloads on src/** changes
# without rebuilding the .app.
#
# Backend (Rust) changes still need a full rerun of this script —
# Tauri 2 has no built-in "rebuild + reload binary into running
# .app" path, and ad-hoc codesigning drifts each rebuild's cdhash
# so the AX prompt + grant cycle re-runs each time.
#
# Usage:
#   apps/tauri/scripts/dev-run.sh
#
# Frontend HMR: edit anything in src/ — Vite pushes the change to
# the running .app. No rebuild.
# Rust HMR: re-run this script.
set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
TAURI_DIR="$( cd "$SCRIPT_DIR/.." && pwd )"
REPO_ROOT="$( cd "$TAURI_DIR/../.." && pwd )"

# Dev overlay (tauri.dev.conf.json) renames the bundle to "OpenWhisper Dev
# Tauri" with id "com.openwhisper.app.dev" so it's visually distinct from
# the SwiftUI shipped app + its debug variant in the Accessibility list.
DEV_CONFIG="$TAURI_DIR/src-tauri/tauri.dev.conf.json"
BUNDLE_ID="com.openwhisper.app.dev"
APP_PATH="$REPO_ROOT/target/debug/bundle/macos/OpenWhisper Dev Tauri.app"

echo "==> Killing any running OpenWhisper (Tauri) instances"
pkill -f "OpenWhisper Dev Tauri.app/Contents/MacOS/" 2>/dev/null || true
pkill -f "OpenWhisper.app/Contents/MacOS/" 2>/dev/null || true
pkill -f "target/debug/openwhisper-tauri" 2>/dev/null || true

# Reset every OpenWhisper variant we know of, every cycle. Ad-hoc rebuilds
# drift their cdhash and leave stale entries; clearing them keeps the
# Accessibility / Microphone / Input Monitoring lists tidy.
echo "==> Resetting TCC grants for all OpenWhisper variants"
for VARIANT_BID in \
    "com.openwhisper.app.dev"     `# Tauri dev (this script)` \
    "com.openwhisper.app"         `# Tauri release` \
    "com.openwhisper.OpenWhisper" `# SwiftUI release` \
    "com.openwhisper.OpenWhisper.dev" `# SwiftUI debug`; do
    for SERVICE in Accessibility Microphone ListenEvent; do
        tccutil reset "$SERVICE" "$VARIANT_BID" 2>/dev/null || true
    done
done

# System Settings caches the Accessibility list and ignores tccutil's mutations
# until the app is restarted. Kicking it forces the next open to re-read TCC,
# so stale entries from prior rebuilds disappear from the UI.
echo "==> Refreshing System Settings cache"
osascript -e 'tell application "System Settings" to quit' 2>/dev/null || true

cd "$TAURI_DIR"

# Free port 1420 if a previous run left Vite hanging.
PREV_VITE_PID="$(lsof -t -iTCP:1420 -sTCP:LISTEN 2>/dev/null || true)"
if [[ -n "$PREV_VITE_PID" ]]; then
    echo "==> Killing prior Vite on :1420 (pid $PREV_VITE_PID)"
    kill "$PREV_VITE_PID" 2>/dev/null || true
    sleep 0.5
fi

echo "==> Starting Vite dev server (background)"
pnpm dev > /tmp/openwhisper-vite.log 2>&1 &
VITE_PID=$!

# Stop Vite when this script exits — but leave the .app running.
# (User can quit the .app via tray; rerun this script to rebuild.)
trap 'kill $VITE_PID 2>/dev/null || true' EXIT INT TERM

echo "==> Waiting for Vite at http://localhost:1420"
for i in $(seq 1 60); do
    if curl -fsS --max-time 1 http://localhost:1420 > /dev/null 2>&1; then
        echo "    Vite ready"
        break
    fi
    if ! kill -0 $VITE_PID 2>/dev/null; then
        echo "error: Vite died during startup. /tmp/openwhisper-vite.log:" >&2
        tail -20 /tmp/openwhisper-vite.log >&2
        exit 1
    fi
    sleep 0.5
done

echo "==> tauri build --debug --config $DEV_CONFIG"
pnpm tauri build --debug --config "$DEV_CONFIG"

if [[ ! -d "$APP_PATH" ]]; then
    echo "error: built app not found at $APP_PATH" >&2
    exit 1
fi

echo "==> open $APP_PATH"
open "$APP_PATH"

cat <<EOF

Tauri dev run ready. App: OpenWhisper Dev Tauri (com.openwhisper.app.dev)

Re-grant on first launch:
  1) Accessibility   → approve  (Right Cmd hotkey + paste)
  2) Microphone      → approve  (audio capture)

After grant, click Retry in the banner — the app relaunches once
and the tap installs cleanly.

Frontend HMR: edit src/** — Vite pushes the change to the
running .app via http://localhost:1420.
Rust changes: re-run apps/tauri/scripts/dev-run.sh.

Vite log: /tmp/openwhisper-vite.log
Press Ctrl-C in this terminal to stop Vite (the .app keeps
running; quit it from the tray when you're done).
EOF

# Keep Vite in the foreground so the user can Ctrl-C to stop it.
wait $VITE_PID
