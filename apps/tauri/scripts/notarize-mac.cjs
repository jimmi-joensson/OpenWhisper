#!/usr/bin/env node
// Submit the Tauri-built DMG to Apple notary, poll status until terminal,
// staple the ticket, validate, and run a final Gatekeeper assessment.
//
// Credentials come from a `notarytool` keychain profile created once via:
//   xcrun notarytool store-credentials "openwhisper-notarytool" \
//     --apple-id <apple-id-email> \
//     --team-id 898R9M89GU \
//     --password <app-specific-password>
//
// Why we don't use `notarytool submit --wait`: that command polls Apple's
// API in-process and crashes on a single network timeout (NSURLErrorDomain
// -1001), losing the submission UUID and forcing a manual resume. New
// developer accounts hit "In Progress" for 30+ min while the underlying
// HTTP poll flakes — too easy to lose a slot. Splitting submit + poll +
// retry survives transient network errors and prints heartbeats so a
// human watching the log can tell something is happening.

const fs = require("node:fs");
const path = require("node:path");
const { execSync, spawnSync } = require("node:child_process");

if (process.platform !== "darwin") process.exit(0);

const SCRIPT_DIR = __dirname;
const PKG = require(path.resolve(SCRIPT_DIR, "..", "package.json"));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "..", "..", "..");
const DMG = path.join(
  REPO_ROOT,
  "target",
  "release",
  "bundle",
  "dmg",
  `OpenWhisper_${PKG.version}_aarch64.dmg`,
);
const PROFILE = "openwhisper-notarytool";
const POLL_SECS = 30;
const POLL_BUDGET_MIN = 240; // 4 hours; new-account first submissions can run 60+ min
const RETRY_BACKOFF_SECS = 60;
const MAX_RETRIES = 5;

if (!fs.existsSync(DMG)) {
  console.error(`[notarize-mac] ${DMG} not found — run \`pnpm tauri build\` first`);
  process.exit(1);
}

const run = (cmd) => execSync(cmd, { stdio: "inherit" });
const cap = (cmd) => execSync(cmd).toString();
const sleep = (s) => new Promise((r) => setTimeout(r, s * 1000));

async function submit() {
  console.log(`[notarize-mac] submitting ${path.basename(DMG)}…`);
  const out = cap(
    `xcrun notarytool submit "${DMG}" --keychain-profile "${PROFILE}" --output-format json`,
  );
  const { id } = JSON.parse(out);
  if (!id) throw new Error(`no submission id in: ${out}`);
  console.log(`[notarize-mac] submission id: ${id}`);
  return id;
}

async function poll(id) {
  const start = Date.now();
  let retries = 0;
  while (true) {
    const elapsed = Math.floor((Date.now() - start) / 1000);
    if (elapsed > POLL_BUDGET_MIN * 60) {
      throw new Error(`polling budget (${POLL_BUDGET_MIN} min) exceeded; submission still in progress`);
    }
    const result = spawnSync(
      "xcrun",
      ["notarytool", "info", id, "--keychain-profile", PROFILE, "--output-format", "json"],
      { encoding: "utf8" },
    );
    if (result.status !== 0) {
      retries += 1;
      if (retries > MAX_RETRIES) {
        throw new Error(`notarytool info failed ${MAX_RETRIES} times in a row`);
      }
      console.log(`[notarize-mac] poll error (${retries}/${MAX_RETRIES}); backing off ${RETRY_BACKOFF_SECS}s`);
      await sleep(RETRY_BACKOFF_SECS);
      continue;
    }
    retries = 0;
    let info;
    try {
      info = JSON.parse(result.stdout);
    } catch {
      console.log(`[notarize-mac] unparseable info output; backing off ${RETRY_BACKOFF_SECS}s`);
      await sleep(RETRY_BACKOFF_SECS);
      continue;
    }
    const mins = (elapsed / 60).toFixed(1);
    console.log(`[notarize-mac] [${mins} min] status: ${info.status}`);
    if (info.status === "Accepted") return;
    if (info.status === "Invalid" || info.status === "Rejected") {
      console.error(`[notarize-mac] notarization ${info.status} — fetching log`);
      run(`xcrun notarytool log "${id}" --keychain-profile "${PROFILE}"`);
      throw new Error(`notarization ${info.status}`);
    }
    await sleep(POLL_SECS);
  }
}

(async () => {
  const id = await submit();
  await poll(id);

  console.log(`[notarize-mac] stapling ticket to DMG`);
  run(`xcrun stapler staple "${DMG}"`);

  console.log(`[notarize-mac] validating staple`);
  run(`xcrun stapler validate "${DMG}"`);

  console.log(`[notarize-mac] Gatekeeper assessment`);
  run(`spctl -a -t open --context context:primary-signature -vv "${DMG}"`);

  console.log(`[notarize-mac] done`);
})().catch((err) => {
  console.error(`[notarize-mac] ${err.message}`);
  process.exit(1);
});
