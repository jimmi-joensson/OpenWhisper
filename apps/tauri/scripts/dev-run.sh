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

# Dev overlay (tauri.dev.conf.json) renames the bundle to "OpenWhisper Dev"
# with id "com.openwhisper.app.dev" so it's visually distinct from the
# release Tauri app in the Accessibility list.
DEV_CONFIG="$TAURI_DIR/src-tauri/tauri.dev.conf.json"
BUNDLE_ID="com.openwhisper.app.dev"
BUILT_APP_PATH="$REPO_ROOT/target/debug/bundle/macos/OpenWhisper Dev.app"
INSTALLED_APP_PATH="/Applications/OpenWhisper Dev.app"

echo "==> Quitting any running OpenWhisper Dev instances (release left alone)"
# Try graceful Quit first (lets the app clean up: unhook hotkey, save, etc).
osascript -e 'tell application "OpenWhisper Dev" to quit' 2>/dev/null || true

# Wait up to 3 s for graceful exit; force-kill anything still alive.
# Only target the Dev bundle path + the bare cargo dev binary. Release
# (com.openwhisper.app at /Applications/OpenWhisper.app) is independent and
# stays running — different bundle id means dev + release coexist.
for _ in 1 2 3 4 5 6; do
    if ! pgrep -f "OpenWhisper Dev.app/Contents/MacOS/|target/debug/openwhisper-tauri" >/dev/null 2>&1; then
        break
    fi
    sleep 0.5
done
pkill -9 -f "OpenWhisper Dev.app/Contents/MacOS/" 2>/dev/null || true
pkill -9 -f "target/debug/openwhisper-tauri" 2>/dev/null || true
sleep 0.3

# Reset only the Dev bundle's TCC entries. Each ad-hoc rebuild drifts the
# cdhash so the prior grant goes stale — wiping the entry keeps the
# Accessibility list tidy and the next launch prompts cleanly.
# DO NOT touch com.openwhisper.app — that's the release the user installed
# from the DMG; resetting it would invalidate their granted state and
# trigger an Accessibility prompt the next time they launch the release.
echo "==> Resetting TCC grants for OpenWhisper Dev only"
for SERVICE in Accessibility Microphone ListenEvent; do
    tccutil reset "$SERVICE" "com.openwhisper.app.dev" 2>/dev/null || true
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

echo "==> tauri build --debug --bundles app --config $DEV_CONFIG"
# `--bundles app` skips the DMG step (slow, fails on dev cdhash drift, and
# we don't need it for local launch).
pnpm tauri build --debug --bundles app --config "$DEV_CONFIG"

if [[ ! -d "$BUILT_APP_PATH" ]]; then
    echo "error: built app not found at $BUILT_APP_PATH" >&2
    exit 1
fi

# Strip hardened runtime that Tauri's signing step adds. Hardened runtime
# + ad-hoc breaks CGEventTapCreate on Sequoia 15: tap returns nil even
# with Accessibility granted, the app never auto-registers in Input
# Monitoring, and the boot mic prompt (gated on hotkey-install success)
# never fires. Re-sign with plain ad-hoc to keep sealed resources but
# drop the runtime flag.
echo "==> Re-signing $BUILT_APP_PATH without hardened runtime"
codesign --force --deep --sign - "$BUILT_APP_PATH"

# Install to /Applications so the launch path is stable + user-discoverable
# (Spotlight, Dock launcher, etc). TCC still re-prompts each cycle because
# ad-hoc cdhash drifts, but at least the path under
# /Applications/OpenWhisper Dev.app doesn't change between cycles.
echo "==> Replacing $INSTALLED_APP_PATH"
# Final guard: if anything is still holding the bundle, force-kill it.
# APFS lets us unlink files held by running processes, but `open` would
# then bring the still-running old instance to front instead of launching
# the freshly-copied bundle (same bundle id wins).
pkill -9 -f "$INSTALLED_APP_PATH/Contents/MacOS/" 2>/dev/null || true
sleep 0.2
rm -rf "$INSTALLED_APP_PATH"
cp -R "$BUILT_APP_PATH" "$INSTALLED_APP_PATH"

# `open` IPCs to LaunchServices, which spawns the app under its own
# context — env vars set in this shell do not propagate. When verbose
# mode is requested (OPENWHISPER_VERBOSE set by dev-run.cjs --verbose),
# launch the bundle's main executable directly so it inherits our env,
# and detach so this script can return. The log path is passed in via
# OPENWHISPER_VERBOSE_LOG so it stays in sync with the Windows path
# resolved in dev-run.cjs (defaults to .openwhisper-verbose.log next to
# this script). Otherwise fall through to `open` so behavior matches the
# pre-verbose flow exactly.
if [[ -n "${OPENWHISPER_VERBOSE:-}" ]]; then
    APP_BIN_NAME=$(/usr/libexec/PlistBuddy -c "Print CFBundleExecutable" \
        "$INSTALLED_APP_PATH/Contents/Info.plist")
    APP_BIN="$INSTALLED_APP_PATH/Contents/MacOS/$APP_BIN_NAME"
    LOG_PATH="${OPENWHISPER_VERBOSE_LOG:-$TAURI_DIR/.openwhisper-verbose.log}"
    echo "==> Launching $APP_BIN with OPENWHISPER_VERBOSE=1"
    echo "    Verbose logs: $LOG_PATH (tail -f and grep '\\[ow\\.')"
    "$APP_BIN" >>"$LOG_PATH" 2>&1 &
else
    echo "==> open $INSTALLED_APP_PATH"
    open "$INSTALLED_APP_PATH"
fi

cat <<EOF

Tauri dev run ready. App: OpenWhisper Dev (com.openwhisper.app.dev)

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
