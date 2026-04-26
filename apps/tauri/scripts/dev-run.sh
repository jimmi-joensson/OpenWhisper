#!/usr/bin/env bash
# One-command dev cycle for the Tauri shell on macOS.
#
# Tauri debug builds use ad-hoc codesigning. Each rebuild produces a new
# signature, and macOS TCC silently invalidates prior Accessibility /
# Microphone / Input Monitoring grants whenever the signing identity
# drifts. System Settings can still *show* the toggle on while the grant
# is dead. This script resets the TCC record cleanly, kills any running
# instance, then hands off to `pnpm tauri dev`.
#
# Mirrors `scripts/dev-run.sh` (the shipped SwiftUI flow). Different
# bundle id — Tauri uses `com.openwhisper.app` per `tauri.conf.json`.
#
# Usage:
#   apps/tauri/scripts/dev-run.sh
#   (or `pnpm dev:tauri` if added to package.json)
set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
TAURI_DIR="$( cd "$SCRIPT_DIR/.." && pwd )"

BUNDLE_ID="com.openwhisper.app"

echo "==> Killing any running OpenWhisper (Tauri) instances"
pkill -f "OpenWhisper.app/Contents/MacOS/OpenWhisper" 2>/dev/null || true
pkill -f "openwhisper-tauri" 2>/dev/null || true

echo "==> Resetting TCC grants for $BUNDLE_ID"
for SERVICE in Accessibility Microphone ListenEvent; do
    tccutil reset "$SERVICE" "$BUNDLE_ID" 2>/dev/null || true
done

echo "==> pnpm tauri dev"
cd "$TAURI_DIR"
exec pnpm tauri dev "$@"
