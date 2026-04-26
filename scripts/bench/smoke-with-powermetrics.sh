#!/usr/bin/env bash
# One-shot: start powermetrics in background, fire the recognizer smoke,
# wait for sampler to finish, dump ANE/GPU/CPU lines.
#
# Run from repo root. Needs sudo (powermetrics is root-only).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

PM_LOG="/tmp/pm.log"
SAMPLES=40            # 40 × 250ms = 10 s window
INTERVAL_MS=250

echo "[bench] starting powermetrics ($SAMPLES samples × ${INTERVAL_MS}ms = $((SAMPLES * INTERVAL_MS / 1000))s window)…"
sudo powermetrics --samplers ane_power,gpu_power,cpu_power -i "$INTERVAL_MS" -n "$SAMPLES" > "$PM_LOG" 2>&1 &
PM_PID=$!
sleep 1

echo "[bench] firing smoke decode (2 passes for cold + warm)…"
SHERPA_ONNX_ARCHIVE_DIR=/tmp/sherpa-onnx-archives \
  "$HOME/.cargo/bin/cargo" run \
    -p openwhisper-core \
    --no-default-features --features recognizer \
    --release \
    --example recognizer_smoke \
    -- apps/macos/Resources/samples/smoke-test.wav

echo "[bench] waiting for powermetrics to finish…"
wait "$PM_PID"

echo "[bench] DONE — log at $PM_LOG ($(wc -l < $PM_LOG) lines)"
echo "---"
grep -iE "ANE Power|GPU Power|CPU Power|GPU H[Wz]|CPU H[Wz]" "$PM_LOG" | head -30 || true
