#!/usr/bin/env bash
# Reset OpenWhisper's TCC grants so the next launch asks fresh.
#
# Why: Debug builds use ad-hoc codesigning, so each rebuild gets a new
# signature and macOS TCC stops recognizing prior Accessibility grants.
# System Settings can still *show* the toggle on while the grant is
# silently stale. Running this clears the record cleanly.
#
# Debug and Release are separate TCC entities (different bundle IDs), so
# we reset both every time — safer than guessing which one drifted.
set -euo pipefail

BUNDLE_IDS=(
    "com.openwhisper.OpenWhisper"      # Release
    "com.openwhisper.OpenWhisper.dev"  # Debug build, productName "OpenWhisper Dev"
)

for BUNDLE_ID in "${BUNDLE_IDS[@]}"; do
    for SERVICE in Accessibility Microphone ListenEvent; do
        tccutil reset "$SERVICE" "$BUNDLE_ID" 2>/dev/null || true
    done
done

killall OpenWhisper 2>/dev/null || true
killall "OpenWhisper Dev" 2>/dev/null || true

cat <<EOF
Reset TCC for:
  - ${BUNDLE_IDS[0]}
  - ${BUNDLE_IDS[1]}

Next steps:
  1) Launch OpenWhisper (or OpenWhisper Dev)
  2) Approve Accessibility + Microphone when prompted
  3) Quit and relaunch once for the Accessibility grant to take effect
EOF
