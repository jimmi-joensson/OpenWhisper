#!/usr/bin/env node
// Post-`tauri build` re-sign for macOS bundles.
//
// Tauri runs `codesign --sign -` with `--options runtime` (hardened
// runtime) when `bundle.macOS.signingIdentity` is set. Hardened runtime
// + ad-hoc signature is incompatible with `CGEventTapCreate` on Sequoia
// 15: the tap returns nil even with Accessibility granted, the app
// never registers in System Settings → Input Monitoring, and the
// downstream mic prompt (gated on hotkey-install success) never fires.
//
// Without `signingIdentity` Tauri ships only the rustc linker-signed
// signature, which Sequoia + the quarantine bit applied on browser
// download flags as "damaged and can't be opened" with no bypass.
//
// So: let Tauri produce a properly sealed ad-hoc bundle (sealed
// resources, bound Info.plist), then re-sign here without
// `--options runtime`, and rebuild the DMG so the asset on disk
// matches the resigned `.app`.

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
const ICON = path.join(BUNDLE_DIR, "dmg", "icon.icns");

if (!fs.existsSync(APP)) {
  console.error(`[sign-mac] ${APP} not found — run \`pnpm tauri build\` first`);
  process.exit(1);
}

const run = (cmd) => execSync(cmd, { stdio: "inherit" });

console.log(`[sign-mac] re-signing ${APP} without hardened runtime`);
run(`codesign --force --deep --sign - "${APP}"`);

const flags = execSync(`codesign -dv --verbose=4 "${APP}" 2>&1 | grep -o 'flags=0x[0-9a-f]*([^)]*)'`)
  .toString()
  .trim();
console.log(`[sign-mac] ${flags}`);
if (flags.includes("runtime")) {
  console.error(`[sign-mac] hardened runtime still present — re-sign failed`);
  process.exit(1);
}

if (!fs.existsSync(DMG)) {
  console.log(`[sign-mac] no DMG at ${DMG} — skipping rebuild`);
  process.exit(0);
}

console.log(`[sign-mac] rebuilding DMG ${DMG}`);
const stage = "/tmp/openwhisper-dmgsrc";
run(`rm -rf "${stage}"`);
fs.mkdirSync(stage, { recursive: true });
run(`cp -R "${APP}" "${stage}/"`);
run(`ln -s /Applications "${stage}/Applications"`);
if (fs.existsSync(ICON)) {
  run(`cp "${ICON}" "${stage}/.VolumeIcon.icns"`);
  try {
    run(`SetFile -a C "${stage}"`);
  } catch {
    // SetFile is in Xcode CLT; non-fatal if missing
  }
}
run(`rm -f "${DMG}"`);
run(
  `hdiutil create -volname "OpenWhisper" -srcfolder "${stage}" -ov -format UDZO "${DMG}"`,
);
run(`rm -rf "${stage}"`);

console.log(`[sign-mac] done`);
