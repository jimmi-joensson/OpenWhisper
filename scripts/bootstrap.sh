#!/usr/bin/env bash
# One-shot setup: build the Rust core, then generate the macOS Xcode project.
set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
REPO_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"

"$SCRIPT_DIR/build-core.sh"

if ! command -v xcodegen >/dev/null 2>&1; then
    echo "error: xcodegen not found. Install with: brew install xcodegen" >&2
    exit 1
fi

echo "==> Generating Xcode project"
cd "$REPO_ROOT/apps/macos"
xcodegen generate --spec project.yml

cat <<EOF

OpenWhisper bootstrap complete.

Next:
  open $REPO_ROOT/apps/macos/OpenWhisper.xcodeproj
  # or
  xcodebuild -project $REPO_ROOT/apps/macos/OpenWhisper.xcodeproj -scheme OpenWhisper build
EOF
