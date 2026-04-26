#!/usr/bin/env node
// Cross-platform entry for `pnpm dev:tauri`.
//
// macOS: shells out to dev-run.sh which does the TCC reset + bundled .app
// install dance (Accessibility / Mic grants need a stable bundle id, which
// `tauri dev` against the bare binary can't provide).
//
// Windows / Linux: no TCC equivalent, no bundle constraint — `tauri dev`
// is sufficient and works against the bare cargo binary.
//
// Implementation note: Node 22+ blocks direct spawn of `.cmd`/`.bat` files
// without `shell: true` (security fix → EINVAL). Passing `args` *with*
// `shell: true` triggers the DEP0190 deprecation warning. The escape
// hatch is to build the full command line as a single string and pass
// it with `shell: true` and no `args` array — the OS shell parses it,
// no Node-side concatenation happens, no warnings, no EINVAL.

const { spawn } = require("node:child_process");
const path = require("node:path");

const isMac = process.platform === "darwin";

const command = isMac
    ? `bash "${path.join(__dirname, "dev-run.sh")}"`
    : "pnpm tauri dev";

const child = spawn(command, {
    stdio: "inherit",
    cwd: path.join(__dirname, ".."),
    shell: true,
});

child.on("exit", (code, signal) => {
    if (signal) {
        process.kill(process.pid, signal);
    } else {
        process.exit(code ?? 0);
    }
});
