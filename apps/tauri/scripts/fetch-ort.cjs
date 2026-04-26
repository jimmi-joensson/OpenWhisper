#!/usr/bin/env node
// Fetch the prebuilt ONNX Runtime shared library matching ort-sys
// 2.0.0-rc.10 (= ONNXRuntime 1.22.0) for the host OS, and place it where
// `core::recognizer::ort_lib::resolve()` looks first:
// `~/.cache/openwhisper/onnxruntime/<libname>`.
//
// Why a Node script instead of doing it in Rust on first launch:
// - The Rust path needs the lib *to load itself* before model download
//   etc. — circular if we try to bootstrap in-process. A separate setup
//   step keeps the runtime path simple (locate, init_from, done).
// - One install per host, not per build — keeps `pnpm tauri dev` cycles
//   fast.
// - Cross-platform (Mac dylib, Win DLL, Linux .so) needs zip + tar.gz
//   handling; Node's stdlib + a couple of one-off downloads is lighter
//   than dragging zip/tar crates into core just for setup.
//
// Usage: pnpm setup:ort
//        OPENWHISPER_ORT_VERSION=1.22.0 pnpm setup:ort  (override)

const fs = require("node:fs");
const path = require("node:path");
const os = require("node:os");
const https = require("node:https");
const { spawnSync } = require("node:child_process");

const ORT_VERSION = process.env.OPENWHISPER_ORT_VERSION || "1.22.0";

const PLATFORM = process.platform;
const ARCH = process.arch;

// Microsoft publishes prebuilts at:
//   https://github.com/microsoft/onnxruntime/releases/download/v<ver>/<asset>
function assetForHost() {
    if (PLATFORM === "win32" && ARCH === "x64") {
        return {
            asset: `onnxruntime-win-x64-${ORT_VERSION}.zip`,
            archive: "zip",
            inner: `onnxruntime-win-x64-${ORT_VERSION}/lib/onnxruntime.dll`,
            libName: "onnxruntime.dll",
        };
    }
    if (PLATFORM === "darwin") {
        const tag = ARCH === "arm64" ? "arm64" : "x86_64";
        return {
            asset: `onnxruntime-osx-${tag}-${ORT_VERSION}.tgz`,
            archive: "tgz",
            inner: `onnxruntime-osx-${tag}-${ORT_VERSION}/lib/libonnxruntime.${ORT_VERSION}.dylib`,
            libName: "libonnxruntime.dylib",
        };
    }
    if (PLATFORM === "linux" && ARCH === "x64") {
        return {
            asset: `onnxruntime-linux-x64-${ORT_VERSION}.tgz`,
            archive: "tgz",
            inner: `onnxruntime-linux-x64-${ORT_VERSION}/lib/libonnxruntime.so.${ORT_VERSION}`,
            libName: "libonnxruntime.so",
        };
    }
    throw new Error(`unsupported host: platform=${PLATFORM} arch=${ARCH}`);
}

function destDir() {
    return path.join(os.homedir(), ".cache", "openwhisper", "onnxruntime");
}

function download(url, dest) {
    return new Promise((resolve, reject) => {
        const file = fs.createWriteStream(dest);
        const get = (u) => {
            https
                .get(u, (res) => {
                    if (res.statusCode === 301 || res.statusCode === 302) {
                        res.resume();
                        return get(res.headers.location);
                    }
                    if (res.statusCode !== 200) {
                        reject(new Error(`HTTP ${res.statusCode} for ${u}`));
                        return;
                    }
                    res.pipe(file);
                    file.on("finish", () => file.close(resolve));
                })
                .on("error", reject);
        };
        get(url);
    });
}

function extractZip(archivePath, destDir) {
    if (PLATFORM === "win32") {
        // PowerShell ships with Windows; no extra deps needed.
        const cmd = `Expand-Archive -Force -LiteralPath '${archivePath}' -DestinationPath '${destDir}'`;
        const r = spawnSync("powershell", ["-NoProfile", "-Command", cmd], {
            stdio: "inherit",
        });
        if (r.status !== 0) throw new Error(`Expand-Archive exit ${r.status}`);
        return;
    }
    const r = spawnSync("unzip", ["-o", archivePath, "-d", destDir], { stdio: "inherit" });
    if (r.status !== 0) throw new Error(`unzip exit ${r.status}`);
}

function extractTgz(archivePath, destDir) {
    const r = spawnSync("tar", ["-xzf", archivePath, "-C", destDir], { stdio: "inherit" });
    if (r.status !== 0) throw new Error(`tar exit ${r.status}`);
}

async function main() {
    const { asset, archive, inner, libName } = assetForHost();
    const url = `https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/${asset}`;
    const dest = destDir();
    fs.mkdirSync(dest, { recursive: true });

    const finalLib = path.join(dest, libName);
    if (fs.existsSync(finalLib)) {
        console.log(`[setup:ort] already present: ${finalLib}`);
        return;
    }

    const archivePath = path.join(dest, asset);
    console.log(`[setup:ort] downloading ${url}`);
    await download(url, archivePath);

    const stageDir = path.join(dest, "stage");
    fs.mkdirSync(stageDir, { recursive: true });
    console.log(`[setup:ort] extracting ${archive} → ${stageDir}`);
    if (archive === "zip") extractZip(archivePath, stageDir);
    else extractTgz(archivePath, stageDir);

    const sourceLib = path.join(stageDir, inner);
    if (!fs.existsSync(sourceLib)) {
        throw new Error(`expected lib not found in archive: ${sourceLib}`);
    }
    fs.copyFileSync(sourceLib, finalLib);
    console.log(`[setup:ort] installed → ${finalLib}`);

    // Cleanup: remove the stage dir + the archive itself. Cache only
    // needs the final lib.
    fs.rmSync(stageDir, { recursive: true, force: true });
    fs.rmSync(archivePath, { force: true });
}

main().catch((e) => {
    console.error(`[setup:ort] failed: ${e.message}`);
    process.exit(1);
});
