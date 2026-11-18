#!/usr/bin/env node
// Keep the Cargo `target/` folder from growing without limit.
//
// Cargo never deletes old build output. Every dependency bump, profile,
// cross-compile triple and incremental session leaves files behind, and
// on the Mac dev box `target/` reached 48 GB before anyone noticed. This
// script trims it in three steps and stops as soon as the folder fits
// its budget:
//
//   1. Always: delete installers and bundles from older versions in every
//      `bundle/` folder (e.g. OpenWhisper_0.5.0_aarch64.dmg once the app
//      is on 0.6.1). The current version's bundles stay.
//   2. Over budget: delete the `incremental/` folders. They are the
//      fastest-growing part and only cost a slower next build.
//   3. Still over budget: delete everything except the `bundle/` folders,
//      so the current DMG / MSI / NSIS survive and the next build is a
//      full rebuild.
//
// Runs on its own as `pnpm prune:target`, after `pnpm release:mac`, and at
// most once a day at the start of `pnpm dev:tauri`.
//
// Options:
//   --dry-run          print what would be deleted, delete nothing
//   --budget=<GB>      size budget (default 30, or OPENWHISPER_TARGET_BUDGET_GB)
//
// Honors CARGO_TARGET_DIR. Don't run it while a cargo build is running in
// the same checkout.

const fs = require("node:fs");
const path = require("node:path");

const SCRIPT_DIR = __dirname;
const PKG = require(path.resolve(SCRIPT_DIR, "..", "package.json"));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "..", "..", "..");
const TARGET_DIR = process.env.CARGO_TARGET_DIR
    ? path.resolve(process.env.CARGO_TARGET_DIR)
    : path.join(REPO_ROOT, "target");
// Records the last daily check, so dev-run doesn't walk the folder on
// every iteration.
const STAMP = path.join(TARGET_DIR, ".openwhisper-prune-stamp");

const GB = 1024 ** 3;
const DEFAULT_BUDGET_GB = 30;
// Tauri names bundles `<Product>_<version>_<arch>...`, e.g.
// OpenWhisper_0.6.1_aarch64.dmg, OpenWhisper_0.6.1_x64-setup.exe.
const VERSION_IN_NAME = /_(\d+\.\d+\.\d+)[_-]/;
// `bundle/` and `incremental/` sit in `target/<profile>/` or, for cross
// builds, `target/<triple>/<profile>/`.
const SEARCH_DEPTH = 3;

const log = (msg) => console.log(`[prune-target] ${msg}`);
const gb = (bytes) =>
    bytes < GB ? `${Math.round(bytes / 1024 ** 2)} MB` : `${(bytes / GB).toFixed(1)} GB`;

function lstat(p) {
    try {
        return fs.lstatSync(p);
    } catch {
        return null;
    }
}

function readdir(dir) {
    try {
        return fs.readdirSync(dir, { withFileTypes: true });
    } catch {
        return [];
    }
}

// Space on disk, not apparent size. Windows has no block count, so fall
// back to the file size there.
function diskUsage(p) {
    const st = lstat(p);
    if (!st) return 0;
    const own = st.blocks != null ? st.blocks * 512 : st.size;
    if (!st.isDirectory()) return own;
    let total = own;
    for (const entry of readdir(p)) total += diskUsage(path.join(p, entry.name));
    return total;
}

function findDirs(root, name, depth) {
    const found = [];
    if (depth < 0) return found;
    for (const entry of readdir(root)) {
        if (!entry.isDirectory()) continue;
        const p = path.join(root, entry.name);
        if (entry.name === name) found.push(p);
        else found.push(...findDirs(p, name, depth - 1));
    }
    return found;
}

function remove(p, ctx) {
    // In a dry run nothing is deleted, so a later step can reach a path an
    // earlier step already counted.
    if (ctx.removed.some((r) => p === r.path || p.startsWith(r.path + path.sep))) return;
    let size = diskUsage(p);
    if (ctx.dryRun) {
        for (const r of ctx.removed) if (r.path.startsWith(p + path.sep)) size -= r.size;
        log(`would delete ${path.relative(TARGET_DIR, p)} (${gb(size)})`);
        ctx.removed.push({ path: p, size });
        ctx.freed += size;
        return;
    }
    try {
        fs.rmSync(p, { recursive: true, force: true });
        ctx.freed += size;
    } catch (err) {
        // A running dev .exe on Windows locks its own files. Skip those.
        log(`could not delete ${path.relative(TARGET_DIR, p)}: ${err.code || err.message}`);
    }
}

function pruneOldBundles(bundleDirs, ctx) {
    for (const bundle of bundleDirs) {
        // bundle/<format>/<file>, e.g. bundle/dmg/OpenWhisper_0.6.1_aarch64.dmg
        for (const format of readdir(bundle)) {
            if (!format.isDirectory()) continue;
            const formatDir = path.join(bundle, format.name);
            for (const entry of readdir(formatDir)) {
                const match = VERSION_IN_NAME.exec(entry.name);
                if (match && match[1] !== PKG.version) remove(path.join(formatDir, entry.name), ctx);
            }
        }
    }
}

// Delete everything under `dir` except the `keep` paths and their parents.
function removeAllExcept(dir, keep, ctx) {
    for (const entry of readdir(dir)) {
        const p = path.join(dir, entry.name);
        if (entry.name === "CACHEDIR.TAG" || p === STAMP || keep.includes(p)) continue;
        if (keep.some((k) => k.startsWith(p + path.sep))) removeAllExcept(p, keep, ctx);
        else remove(p, ctx);
    }
}

function prune({ budgetGb, dryRun = false, maxAgeHours } = {}) {
    if (!lstat(TARGET_DIR)) return;
    if (maxAgeHours != null) {
        const stamp = lstat(STAMP);
        if (stamp && Date.now() - stamp.mtimeMs < maxAgeHours * 3600 * 1000) return;
    }

    const budgetSource = budgetGb ?? process.env.OPENWHISPER_TARGET_BUDGET_GB ?? DEFAULT_BUDGET_GB;
    const budget = Number(budgetSource) * GB;
    if (!(budget > 0)) throw new Error(`invalid budget: ${budgetSource}`);

    const ctx = { dryRun, freed: 0, removed: [] };
    const bundleDirs = findDirs(TARGET_DIR, "bundle", SEARCH_DEPTH);
    const before = diskUsage(TARGET_DIR);

    pruneOldBundles(bundleDirs, ctx);
    if (before - ctx.freed > budget) {
        for (const dir of findDirs(TARGET_DIR, "incremental", SEARCH_DEPTH)) remove(dir, ctx);
    }
    if (before - ctx.freed > budget) {
        removeAllExcept(TARGET_DIR, bundleDirs, ctx);
    }

    const after = before - ctx.freed;
    const verb = dryRun ? "would free" : "freed";
    log(
        ctx.freed > 0
            ? `${verb} ${gb(ctx.freed)}, target is ${gb(after)} (budget ${gb(budget)})`
            : `target is ${gb(after)}, within the ${gb(budget)} budget`,
    );

    if (!dryRun) {
        try {
            fs.writeFileSync(STAMP, new Date().toISOString() + "\n");
        } catch {
            // Target folder gone or read-only: nothing to record.
        }
    }
}

module.exports = { prune };

if (require.main === module) {
    const args = process.argv.slice(2);
    const budgetArg = args.find((a) => a.startsWith("--budget="));
    try {
        prune({
            dryRun: args.includes("--dry-run"),
            budgetGb: budgetArg ? budgetArg.slice("--budget=".length) : undefined,
        });
    } catch (err) {
        console.error(`[prune-target] ${err.message}`);
        process.exit(1);
    }
}
