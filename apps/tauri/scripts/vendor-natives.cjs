#!/usr/bin/env node
// Stage native shared libraries into apps/tauri/src-tauri/vendor/ so the
// Tauri bundler picks them up via `bundle.resources` and ships them next
// to the exe in MSI / NSIS installers (Windows). On Mac this is a no-op
// because the shipped Mac path uses FluidAudio (Swift bridge), not ort.
//
// Why pre-build vendoring instead of cargo build.rs:
// - Tauri's bundler reads tauri.conf.json before cargo's build scripts
//   run, so the resource files must already exist on disk.
// - We chain this from `beforeBuildCommand` in tauri.conf.json so it
//   runs as part of `pnpm tauri build` automatically.
//
// Sources:
//   WebView2Loader.dll → cargo registry, webview2-com-sys-0.38.2/x64/
//                        Pinned at the same crate version Cargo.lock has.
//   onnxruntime.dll    → ~/.cache/openwhisper/onnxruntime/, populated by
//                        `pnpm setup:ort` (auto-runs here if the cache
//                        is empty).

const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const { spawnSync } = require("node:child_process");

const SCRIPT_DIR = __dirname;
const SRC_TAURI = path.resolve(SCRIPT_DIR, "..", "src-tauri");
const VENDOR_DIR = path.join(SRC_TAURI, "vendor");

fs.mkdirSync(VENDOR_DIR, { recursive: true });

if (process.platform !== "win32") {
    // Non-Windows: tauri.conf.json's `bundle.resources` references both
    // DLLs by exact name, so the source files must exist or the bundler
    // bails out. Create 0-byte stubs — they end up in the Mac .app /
    // Linux .deb but are inert (recognizer uses FluidAudio on Mac, no
    // Linux MVP). Wasteful at <1 KB total, fine until we wire a true
    // platform-conditional resources config.
    const stubs = ["WebView2Loader.dll", "onnxruntime.dll"];
    for (const name of stubs) {
        const p = path.join(VENDOR_DIR, name);
        if (!fs.existsSync(p)) fs.writeFileSync(p, "");
    }
    console.log(
        `[vendor-natives] platform=${process.platform}: wrote 0-byte stubs (recognizer uses FluidAudio on Mac; Linux not in MVP)`,
    );
    process.exit(0);
}

function findWebView2Loader() {
    const homes = [
        process.env.CARGO_HOME,
        path.join(os.homedir(), ".cargo"),
        "C:/rust/.cargo",
    ].filter(Boolean);
    for (const home of homes) {
        const regSrc = path.join(home, "registry", "src");
        if (!fs.existsSync(regSrc)) continue;
        const indices = fs
            .readdirSync(regSrc)
            .filter((d) => d.startsWith("index.crates.io-"));
        for (const idx of indices) {
            const candidate = path.join(
                regSrc,
                idx,
                "webview2-com-sys-0.38.2",
                "x64",
                "WebView2Loader.dll",
            );
            if (fs.existsSync(candidate)) return candidate;
        }
    }
    throw new Error(
        "WebView2Loader.dll not found in cargo registry. Run `cargo fetch` " +
            "from the workspace root first so the webview2-com-sys-0.38.2 " +
            "source archive is unpacked.",
    );
}

function findOnnxRuntime() {
    const cached = path.join(
        os.homedir(),
        ".cache",
        "openwhisper",
        "onnxruntime",
        "onnxruntime.dll",
    );
    if (fs.existsSync(cached)) return cached;
    console.log(
        `[vendor-natives] onnxruntime.dll not in cache, running fetch-ort.cjs`,
    );
    const r = spawnSync(
        "node",
        [path.join(SCRIPT_DIR, "fetch-ort.cjs")],
        { stdio: "inherit" },
    );
    if (r.status !== 0) {
        throw new Error(`fetch-ort.cjs failed with status ${r.status}`);
    }
    if (!fs.existsSync(cached)) {
        throw new Error(
            `fetch-ort.cjs ran but ${cached} is still missing`,
        );
    }
    return cached;
}

const wv2Src = findWebView2Loader();
const ortSrc = findOnnxRuntime();

const wv2Dest = path.join(VENDOR_DIR, "WebView2Loader.dll");
const ortDest = path.join(VENDOR_DIR, "onnxruntime.dll");

fs.copyFileSync(wv2Src, wv2Dest);
fs.copyFileSync(ortSrc, ortDest);

console.log("[vendor-natives] staged:");
console.log(`  ${wv2Src}`);
console.log(`    → ${wv2Dest}`);
console.log(`  ${ortSrc}`);
console.log(`    → ${ortDest}`);
