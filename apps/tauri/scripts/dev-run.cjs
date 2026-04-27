#!/usr/bin/env node
// Cross-platform entry for `pnpm dev:tauri`.
//
// macOS: shells out to dev-run.sh which does the TCC reset + bundled .app
// install dance (Accessibility / Mic grants need a stable bundle id, which
// `tauri dev` against the bare binary can't provide).
//
// Windows / Linux: no TCC equivalent, no bundle constraint — `tauri dev`
// is sufficient and works against the bare cargo binary. We chain
// `pnpm vendor:natives` first because tauri.conf.json's
// `bundle.resources` references vendor/WebView2Loader.dll +
// vendor/onnxruntime.dll, and tauri-build validates those paths exist
// during cargo build. Only `beforeBuildCommand` runs vendor:natives,
// not `beforeDevCommand`, so without this `tauri dev` panics in build.rs.
//
// Implementation note: Node 22+ blocks direct spawn of `.cmd`/`.bat` files
// without `shell: true` (security fix → EINVAL). Passing `args` *with*
// `shell: true` triggers the DEP0190 deprecation warning. The escape
// hatch is to build the full command line as a single string and pass
// it with `shell: true` and no `args` array — the OS shell parses it,
// no Node-side concatenation happens, no warnings, no EINVAL.
//
// Verbose flag: `pnpm dev:tauri --verbose` sets OPENWHISPER_VERBOSE=1
// in the spawned env. Rust reads it via
// openwhisper_core::verbose::enabled() and gates the `verbose_log!`
// macros that instrument drain/transcribe/inject timings. We do NOT
// accept `-v` as a shorthand because most CLIs (cargo, npm, node)
// use `-v` for `--version`; treating it as verbose here would surprise
// users. On macOS the env needs to ride along when dev-run.sh launches
// the .app bundle via `open` — handled there.

const { spawn } = require("node:child_process");
const fs = require("node:fs");
const path = require("node:path");

const isMac = process.platform === "darwin";

const args = process.argv.slice(2);
const verbose = args.includes("--verbose");

// Single source of truth for the verbose log path, used by both this
// script (Windows tee) and dev-run.sh (Mac redirect). Repo-local +
// gitignored so it's easy to `tail -f` from the repo root and survives
// terminal close. Truncated on each run so iteration N's logs don't
// trail iteration N-1's.
const TAURI_DIR = path.join(__dirname, "..");
const VERBOSE_LOG_PATH = path.join(TAURI_DIR, ".openwhisper-verbose.log");

// Applying tauri.dev.conf.json on Windows mirrors what dev-run.sh does on
// Mac: renames productName/identifier/main-window-title to "OpenWhisper Dev"
// so the dev build is visually distinct from a release install. Tauri's
// `--config` flag uses RFC 7396 JSON Merge Patch, so the overlay's
// `app.windows` array replaces (not deep-merges) the base array — both
// windows are redeclared in tauri.dev.conf.json for this reason.
const command = isMac
    ? `bash "${path.join(__dirname, "dev-run.sh")}"`
    : "pnpm vendor:natives && pnpm tauri dev --config src-tauri/tauri.dev.conf.json";

const env = { ...process.env };
let logStream = null;
if (verbose) {
    env.OPENWHISPER_VERBOSE = "1";
    // Pass the path through so dev-run.sh writes to the same file on Mac.
    env.OPENWHISPER_VERBOSE_LOG = VERBOSE_LOG_PATH;
    // Truncate once at start; both writers (this script's tee, and on
    // Mac the detached .app via bash `>>`) then append. Append mode
    // gives us line-atomic concurrent writes on POSIX/NTFS without an
    // explicit lock.
    fs.writeFileSync(VERBOSE_LOG_PATH, "");
    logStream = fs.createWriteStream(VERBOSE_LOG_PATH, { flags: "a" });
    console.log(
        `[dev-run] verbose mode ON — tee'd to ${VERBOSE_LOG_PATH}\n` +
            `         tail -f "${VERBOSE_LOG_PATH}" | grep '\\[ow\\.'`,
    );
}

// When verbose, we tee the spawned process's output to both the parent
// terminal and the log file. That requires `pipe` instead of `inherit`
// for stdout/stderr so Node can read the chunks. stdin stays inherit so
// Ctrl-C still works. Non-verbose path keeps full inherit (no overhead,
// preserves color/TTY detection).
const child = spawn(command, {
    stdio: verbose ? ["inherit", "pipe", "pipe"] : "inherit",
    cwd: TAURI_DIR,
    shell: true,
    env,
});

if (verbose) {
    child.stdout.on("data", (chunk) => {
        process.stdout.write(chunk);
        logStream.write(chunk);
    });
    child.stderr.on("data", (chunk) => {
        process.stderr.write(chunk);
        logStream.write(chunk);
    });
}

child.on("exit", (code, signal) => {
    if (logStream) logStream.end();
    if (signal) {
        process.kill(process.pid, signal);
    } else {
        process.exit(code ?? 0);
    }
});
