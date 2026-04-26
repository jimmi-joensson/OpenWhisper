#!/usr/bin/env node
// Cross-platform entry for `pnpm dev:tauri`.
//
// macOS: shells out to dev-run.sh which does the TCC reset + bundled .app
// install dance (Accessibility / Mic grants need a stable bundle id, which
// `tauri dev` against the bare binary can't provide).
//
// Windows / Linux: no TCC equivalent, no bundle constraint — `tauri dev`
// is sufficient and works against the bare cargo binary.

const { spawn } = require("node:child_process");
const path = require("node:path");

const isMac = process.platform === "darwin";

const cmd = isMac ? "bash" : "pnpm";
const args = isMac
    ? [path.join(__dirname, "dev-run.sh")]
    : ["tauri", "dev"];

const child = spawn(cmd, args, {
    stdio: "inherit",
    cwd: path.join(__dirname, ".."),
    shell: process.platform === "win32", // pnpm.cmd resolution on Windows
});

child.on("exit", (code, signal) => {
    if (signal) {
        process.kill(process.pid, signal);
    } else {
        process.exit(code ?? 0);
    }
});
