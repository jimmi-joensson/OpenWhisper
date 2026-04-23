#!/usr/bin/env bash
# Package OpenWhisper.app into a distributable DMG.
#
# Output: dist/OpenWhisper-<version>-arm64.dmg
#
# Version resolution (highest priority first):
#   1. $VERSION env var
#   2. First arg ($1)
#   3. Current git tag at HEAD (strips leading "v")
#   4. "dev-<shortsha>"
set -euo pipefail

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
REPO_ROOT="$( cd "$SCRIPT_DIR/.." && pwd )"

cd "$REPO_ROOT"

VERSION="${VERSION:-${1:-}}"
if [[ -z "$VERSION" ]]; then
    if git describe --exact-match --tags HEAD >/dev/null 2>&1; then
        VERSION="$(git describe --exact-match --tags HEAD | sed 's/^v//')"
    else
        VERSION="dev-$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
    fi
fi

echo "==> Packaging OpenWhisper $VERSION (arm64)"

# cargo may live in ~/.cargo/bin (not in non-interactive PATH).
if ! command -v cargo >/dev/null 2>&1 && [[ -f "$HOME/.cargo/env" ]]; then
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
fi

for tool in xcodegen xcodebuild cargo; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        echo "error: required tool '$tool' not found" >&2
        echo "hint: xcodegen via 'brew install xcodegen'; cargo via https://rustup.rs/" >&2
        exit 1
    fi
done

HAVE_CREATE_DMG=0
if command -v create-dmg >/dev/null 2>&1; then
    HAVE_CREATE_DMG=1
else
    echo "note: 'create-dmg' not installed — falling back to hdiutil (plainer DMG)"
    echo "      install nicer DMG layout with: brew install create-dmg"
fi

BUILD_DIR="$REPO_ROOT/build/release"
DIST_DIR="$REPO_ROOT/dist"
DMG_STAGE="$BUILD_DIR/dmg-stage"
rm -rf "$BUILD_DIR" "$DMG_STAGE"
mkdir -p "$BUILD_DIR" "$DIST_DIR" "$DMG_STAGE"

echo "==> Building Rust core (release)"
PROFILE=release "$SCRIPT_DIR/build-core.sh"

echo "==> Generating Xcode project"
pushd "$REPO_ROOT/apps/macos" >/dev/null
xcodegen generate --spec project.yml
popd >/dev/null

echo "==> xcodebuild archive (Release, arm64)"
ARCHIVE_PATH="$BUILD_DIR/OpenWhisper.xcarchive"
xcodebuild \
    -project "$REPO_ROOT/apps/macos/OpenWhisper.xcodeproj" \
    -scheme OpenWhisper \
    -configuration Release \
    -destination "generic/platform=macOS" \
    -archivePath "$ARCHIVE_PATH" \
    MARKETING_VERSION="$VERSION" \
    ARCHS=arm64 \
    ONLY_ACTIVE_ARCH=NO \
    archive

APP_SRC="$ARCHIVE_PATH/Products/Applications/OpenWhisper.app"
if [[ ! -d "$APP_SRC" ]]; then
    echo "error: built .app not found at $APP_SRC" >&2
    exit 1
fi

cp -R "$APP_SRC" "$DMG_STAGE/OpenWhisper.app"

# Drag-to-install shortcut. create-dmg adds one positioned via --app-drop-link, so
# skip the pre-create there (otherwise it clashes). For the hdiutil fallback this
# symlink is the only way to get a visible /Applications target in the DMG window.
if [[ "$HAVE_CREATE_DMG" != "1" ]]; then
    ln -s /Applications "$DMG_STAGE/Applications"
fi

echo "==> Verifying signature (ad-hoc expected)"
codesign --verify --deep --strict --verbose=2 "$DMG_STAGE/OpenWhisper.app" || true
codesign -dv "$DMG_STAGE/OpenWhisper.app" 2>&1 | head -5 || true

DMG_NAME="OpenWhisper-${VERSION}-arm64.dmg"
DMG_PATH="$DIST_DIR/$DMG_NAME"
rm -f "$DMG_PATH"

# Generate DMG background: 560x380 canvas, arrow from app icon → Applications folder.
# Icon centers: app at (150, 180), Applications at (410, 180). Arrow sits in-between.
BG_PNG="$BUILD_DIR/dmg-background.png"
/usr/bin/swift - "$BG_PNG" <<'SWIFT_EOF'
import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers
import AppKit

let dst = URL(fileURLWithPath: CommandLine.arguments[1])
let W = 560, H = 380
let cs = CGColorSpace(name: CGColorSpace.sRGB)!
guard let ctx = CGContext(data: nil, width: W, height: H,
                          bitsPerComponent: 8, bytesPerRow: 0, space: cs,
                          bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { exit(2) }

// Light background
ctx.setFillColor(CGColor(red: 0.96, green: 0.96, blue: 0.97, alpha: 1.0))
ctx.fill(CGRect(x: 0, y: 0, width: W, height: H))

// Arrow coordinates. CG origin is bottom-left; create-dmg icon Y is from top.
// Icon center y = 180 (from top) → in CG = H - 180 = 200
let iconY_CG: CGFloat = 200
// Icons span ~64 px either side of their centers (icon-size 128).
// App icon right edge = 214; Applications folder left edge = 346.
// Keep a 16 px breathing room at each side so the arrow head is fully visible.
let startX: CGFloat = 230
let endX: CGFloat = 330
let stroke: CGFloat = 6
let headSize: CGFloat = 22

ctx.setStrokeColor(CGColor(red: 0.55, green: 0.55, blue: 0.58, alpha: 1.0))
ctx.setFillColor(CGColor(red: 0.55, green: 0.55, blue: 0.58, alpha: 1.0))
ctx.setLineWidth(stroke)
ctx.setLineCap(.round)

// Stem
ctx.move(to: CGPoint(x: startX, y: iconY_CG))
ctx.addLine(to: CGPoint(x: endX - headSize, y: iconY_CG))
ctx.strokePath()

// Head (filled triangle)
ctx.move(to: CGPoint(x: endX, y: iconY_CG))
ctx.addLine(to: CGPoint(x: endX - headSize, y: iconY_CG + headSize * 0.55))
ctx.addLine(to: CGPoint(x: endX - headSize, y: iconY_CG - headSize * 0.55))
ctx.closePath()
ctx.fillPath()

guard let img = ctx.makeImage(),
      let sink = CGImageDestinationCreateWithURL(dst as CFURL,
                                                 UTType.png.identifier as CFString,
                                                 1, nil) else { exit(2) }
CGImageDestinationAddImage(sink, img, nil)
CGImageDestinationFinalize(sink)
SWIFT_EOF

echo "==> Building DMG: $DMG_NAME"
if [[ "$HAVE_CREATE_DMG" == "1" ]]; then
    create-dmg \
        --volname "OpenWhisper $VERSION" \
        --background "$BG_PNG" \
        --window-size 560 380 \
        --icon-size 128 \
        --icon "OpenWhisper.app" 150 180 \
        --app-drop-link 410 180 \
        --no-internet-enable \
        "$DMG_PATH" \
        "$DMG_STAGE" \
    || {
        echo "warn: create-dmg failed, falling back to hdiutil"
        hdiutil create -volname "OpenWhisper $VERSION" \
            -srcfolder "$DMG_STAGE" -ov -format UDZO "$DMG_PATH"
    }
else
    hdiutil create -volname "OpenWhisper $VERSION" \
        -srcfolder "$DMG_STAGE" -ov -format UDZO "$DMG_PATH"
fi

SIZE="$(du -h "$DMG_PATH" | cut -f1)"
echo ""
echo "==> Done: $DMG_PATH ($SIZE)"
echo "    Version: $VERSION"
echo "    Signature: ad-hoc (Gatekeeper bypass required on install; see INSTALL.md)"
