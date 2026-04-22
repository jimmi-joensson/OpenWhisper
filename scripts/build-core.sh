#!/usr/bin/env bash
# Build openwhisper-core (Rust staticlib) and stage artifacts for the macOS app.
set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
REPO_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"

PROFILE="${PROFILE:-release}"
CARGO_FLAGS=()
if [[ "$PROFILE" == "release" ]]; then
    CARGO_FLAGS+=("--release")
fi

if ! command -v cargo >/dev/null 2>&1; then
    if [[ -f "$HOME/.cargo/env" ]]; then
        # shellcheck disable=SC1091
        source "$HOME/.cargo/env"
    else
        echo "error: cargo not found. Install Rust via https://rustup.rs/" >&2
        exit 1
    fi
fi

cd "$REPO_ROOT"

echo "==> Building openwhisper-core ($PROFILE)"
cargo build "${CARGO_FLAGS[@]}" -p openwhisper-core

SRC_LIB="$REPO_ROOT/target/$PROFILE/libopenwhisper_core.a"
DST_DIR="$REPO_ROOT/apps/macos/Vendor"
mkdir -p "$DST_DIR"
cp "$SRC_LIB" "$DST_DIR/libopenwhisper_core.a"
echo "==> Staged staticlib: $DST_DIR/libopenwhisper_core.a"

GENERATED="$REPO_ROOT/apps/macos/Generated"
if [[ -d "$GENERATED" ]]; then
    echo "==> swift-bridge artifacts under: $GENERATED"
fi
