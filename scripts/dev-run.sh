#!/usr/bin/env bash
# One-command dev cycle: build Rust core, build the macOS app, reset TCC,
# and launch. Ad-hoc signing means TCC grants drift on every rebuild, so
# we always reset — you'll re-grant Accessibility/Microphone/Input Monitoring
# on each run. Annoying but unavoidable until we move to Developer ID signing.
set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
REPO_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"
APP_PATH="$HOME/Library/Developer/Xcode/DerivedData/OpenWhisper-faelvxbavfasrsautnzfxmdjhmvy/Build/Products/Debug/OpenWhisper Dev.app"

PROFILE=release "$SCRIPT_DIR/build-core.sh"

echo "==> Building OpenWhisper Dev (xcodebuild)"
xcodebuild \
    -project "$REPO_ROOT/apps/macos/OpenWhisper.xcodeproj" \
    -scheme OpenWhisper \
    -configuration Debug \
    -destination 'platform=macOS' \
    build \
    | grep -E '(error:|warning:|BUILD SUCCEEDED|BUILD FAILED)' \
    || true

if [[ ! -d "$APP_PATH" ]]; then
    echo "error: built app not found at $APP_PATH" >&2
    exit 1
fi

echo "==> Killing any running OpenWhisper instances"
killall "OpenWhisper Dev" 2>/dev/null || true
killall OpenWhisper     2>/dev/null || true

echo "==> Resetting TCC grants"
"$SCRIPT_DIR/reset-tcc.sh" >/dev/null

echo "==> Launching $APP_PATH"
open "$APP_PATH"

cat <<EOF

Dev run ready. Re-grant on first launch:
  1) Accessibility  → approve → app will prompt for relaunch
  2) Microphone     → approve
  3) Input Monitoring (for Right Command hotkey) → approve
EOF
