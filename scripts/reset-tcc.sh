#!/usr/bin/env bash
# Reset OpenWhisper's TCC grants so the next launch asks fresh.
#
# Why: Debug builds use ad-hoc codesigning, so each rebuild gets a new
# signature and macOS TCC stops recognizing prior Accessibility grants.
# System Settings can still *show* the toggle on while the grant is
# silently stale. Running this clears the record cleanly.
set -euo pipefail

BUNDLE_ID="com.openwhisper.OpenWhisper"

tccutil reset Accessibility "$BUNDLE_ID" 2>/dev/null || true
tccutil reset Microphone    "$BUNDLE_ID" 2>/dev/null || true
tccutil reset ListenEvent   "$BUNDLE_ID" 2>/dev/null || true

killall OpenWhisper 2>/dev/null || true

cat <<EOF
Reset TCC for $BUNDLE_ID.

Next steps:
  1) Launch OpenWhisper
  2) Approve Accessibility + Microphone when prompted
  3) Quit and relaunch once for the Accessibility grant to take effect
EOF
