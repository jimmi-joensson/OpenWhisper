#!/usr/bin/env node
// Post-`tauri build` re-sign for macOS bundles.
//
// Tauri runs `codesign --sign -` with `--options runtime` (hardened
// runtime) when `bundle.macOS.signingIdentity` is set. Hardened runtime
// + ad-hoc signature is incompatible with `CGEventTapCreate` on Sequoia
// 15: the tap returns nil even with Accessibility granted, so the
// global hotkey never installs and the boot mic prompt (gated on
// hotkey-install success) never fires.
//
// Without `signingIdentity` Tauri ships only the rustc linker-signed
// signature, which Sequoia + the quarantine bit applied on browser
// download flags as "damaged and can't be opened" with no bypass.
//
// So: let Tauri produce a properly sealed ad-hoc bundle and the styled
// DMG (background, icon positions, drop-to-Applications shortcut), then
// re-sign the `.app` here without `--options runtime`, and swap the
// re-signed `.app` into Tauri's DMG via UDRW conversion so the layout
// is preserved.

const fs = require("node:fs");
const path = require("node:path");
const { execSync } = require("node:child_process");

if (process.platform !== "darwin") process.exit(0);

const SCRIPT_DIR = __dirname;
const PKG = require(path.resolve(SCRIPT_DIR, "..", "package.json"));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "..", "..", "..");
const BUNDLE_DIR = path.join(REPO_ROOT, "target", "release", "bundle");
const APP = path.join(BUNDLE_DIR, "macos", "OpenWhisper.app");
const DMG = path.join(BUNDLE_DIR, "dmg", `OpenWhisper_${PKG.version}_aarch64.dmg`);

if (!fs.existsSync(APP)) {
  console.error(`[sign-mac] ${APP} not found — run \`pnpm tauri build\` first`);
  process.exit(1);
}

const run = (cmd) => execSync(cmd, { stdio: "inherit" });
const cap = (cmd) => execSync(cmd).toString().trim();

console.log(`[sign-mac] re-signing ${APP} without hardened runtime`);
run(`codesign --force --deep --sign - "${APP}"`);

const flagsLine = cap(
  `codesign -dv --verbose=4 "${APP}" 2>&1 | grep -o 'flags=0x[0-9a-f]*([^)]*)'`,
);
console.log(`[sign-mac] ${flagsLine}`);
if (flagsLine.includes("runtime")) {
  console.error(`[sign-mac] hardened runtime still present — re-sign failed`);
  process.exit(1);
}

if (!fs.existsSync(DMG)) {
  console.log(`[sign-mac] no DMG at ${DMG} — skipping`);
  process.exit(0);
}

// Swap the re-signed .app into Tauri's styled DMG without touching
// .DS_Store (icon positions), .VolumeIcon.icns, .background, etc.:
//   1. UDZO (compressed read-only) → UDRW (read-write)
//   2. mount, replace .app inside the volume, detach
//   3. UDRW → UDZO
const RW_DMG = `${DMG}.rw.dmg`;
const MOUNT = "/tmp/openwhisper-dmg-mount";

console.log(`[sign-mac] swapping re-signed .app into ${path.basename(DMG)}`);
run(`rm -f "${RW_DMG}"`);
run(`hdiutil convert "${DMG}" -format UDRW -o "${RW_DMG}" -quiet`);

run(`rm -rf "${MOUNT}" && mkdir -p "${MOUNT}"`);
run(`hdiutil attach "${RW_DMG}" -nobrowse -mountpoint "${MOUNT}" -quiet`);

try {
  const innerApp = path.join(MOUNT, "OpenWhisper.app");
  run(`rm -rf "${innerApp}"`);
  run(`cp -R "${APP}" "${innerApp}"`);
} finally {
  // Detach in any case so the dmg file isn't left mounted on failure.
  run(`hdiutil detach "${MOUNT}" -quiet`);
}

run(`rm -f "${DMG}"`);
run(`hdiutil convert "${RW_DMG}" -format UDZO -o "${DMG}" -quiet`);
run(`rm -f "${RW_DMG}"`);

console.log(`[sign-mac] done`);
