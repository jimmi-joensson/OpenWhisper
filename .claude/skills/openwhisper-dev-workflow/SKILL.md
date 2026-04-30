---
name: openwhisper-dev-workflow
description: Build, dev-loop, and tooling conventions for OpenWhisper. READ before scripting dev commands, proposing build changes, picking a package manager, or shipping a Windows release bundle.
---

# Dev workflow conventions

## Node toolchain — fnm + pnpm, never raw npm

Use **fnm** (Fast Node Manager) for Node version management — not nvm, Volta, nodist, or direct Node installers. Use **pnpm** for all package installs — not npm or yarn, even for global tools.

**Why:** fnm is fast and cross-platform (Rust-based, identical on macOS + Windows). pnpm has a content-addressable store that keeps disk usage down across many projects. This is a cross-project standard, not OpenWhisper-specific.

**How to apply:**
- For project-local deps: `pnpm install` / `pnpm add` / `pnpm run`.
- For global CLIs (Backlog.md, etc.): `pnpm add -g <pkg>` — never `npm i -g`.
- Don't propose `npm run` or `npm i` in scripts or docs — use `pnpm run` / `pnpm add`.

## Build Rust core in release even during dev

Default any `scripts/dev-run.*` to build `openwhisper-core` with `--release`. The shell (Swift/Tauri) stays Debug for breakpoints.

**Why:** Debug Rust is 50–120× slower than Release on DSP-heavy paths. Concrete case: `audio_drain_samples()` blocked 900–2066 ms for 16–38 s of audio in Debug vs **17 ms in Release**. The rubato `SincFixedIn` resampler (`sinc_len=128`, `oversampling_factor=64`) is a debug performance cliff — `opt-level=0` kills it. Release-core + debug-shell rebuilds are incremental and cached: first cold ~7 s, subsequent ~1 s.

**How to apply:** When writing a `scripts/dev-run.*` for any platform, default the core build to `cargo build --release -p openwhisper-core`. If the user reports "app feels slow," first check the linked `.lib`/`.dll` isn't a debug-profile artifact — compare `target/release` vs `target/debug` mtimes before investigating code. Only flip to debug-core when genuinely stepping through Rust.

## Tauri release bundle must vendor native DLLs (Windows)

For Windows release builds (`pnpm tauri build`), the bundle is **non-functional** without the pre-build vendoring step at `apps/tauri/scripts/vendor-natives.cjs`:

- **WebView2Loader.dll** — GNU-toolchain Tauri builds don't statically link the loader (MSVC builds do). Without it next to the exe, the installed app fails to launch with "WebView2Loader.dll not found".
- **onnxruntime.dll** — load-dynamic ort needs the lib resolvable at runtime. The resolver order is: env override → next to exe → cache. For end-user installs, "next to exe" is the only one that works.

**How it works:** `vendor-natives.cjs` is chained from `tauri.conf.json::beforeBuildCommand`. It pulls WebView2Loader.dll from the cargo registry's webview2-com-sys source archive, onnxruntime.dll from `~/.cache/openwhisper/onnxruntime/` (auto-runs `fetch-ort.cjs` if the cache is empty). Both are copied to `apps/tauri/src-tauri/vendor/` and referenced via the `bundle.resources` object-form map so they flatten next to the exe in the MSI/NSIS layout.

**How to apply:** Don't propose simplifying away the vendor step or the object-form resource map. If the WebView2Loader source path or the ort version changes, update the script — don't reach for `download-binaries` (which doesn't ship for `x86_64-pc-windows-gnu`) or manual DLL copying (works on one machine, breaks for end users).

## Tauri MSI bundler auto-sweeps stray DLLs from `target/release/`

The Tauri 2 WiX bundler auto-sweeps every `.dll` sitting next to the main binary in `target/release/` and adds it to `main.wxs` as a separate `<Component>`/`<File>`, on top of whatever is in the explicit `bundle.resources` map. EXEs are NOT swept — only DLLs. The bundler dedupes a swept DLL against `bundle.resources` (so the vendor copies of `onnxruntime.dll` and `WebView2Loader.dll` don't double up), but it does NOT distinguish between DLLs that belong to the Tauri app and stale DLLs left over from sibling targets in the same workspace.

**Why this bites OpenWhisper:** The repo has a `bench-sherpa` binary that links a CUDA-enabled ONNX runtime. Building it deposits ~2 GB of CUDA + cuDNN DLLs (`cublas64_*`, `cublasLt64_*`, `cudnn_engines_precompiled64_*`, `cufft64_*`, etc.) into `target/release/`. On the next `pnpm tauri build`, the WiX bundler sweeps all of them into the MSI. Linking then fails at WiX `light.exe` with:

> `light.exe : error LGHT0306 : An error (E_FAIL) was returned while finalizing a CAB file. This most commonly happens when creating a CAB file with more than 65535 files in it.`

The error text is misleading — the actual cause is the **2 GB hard size limit of a single MSI CAB**, not the file count. Confirm by running `pnpm tauri build --verbose` (the bare `pnpm tauri build` swallows light's stderr) and grepping `target/release/wix/x64/main.wxs` for `Source=` to see exactly which paths got pulled in.

**How to apply:**
- Before a release build, ensure `target/release/` contains only the openwhisper-tauri build outputs (`openwhisper-tauri.exe`, `onnxruntime.dll`, `WebView2Loader.dll`) plus standard Cargo metadata. Specifically: no `cargs.dll`, no `cu*64_*.dll`, no `cudnn_*.dll`, no `nvrtc*.dll`, no `nvJitLink_*.dll`.
- If you need bench-sherpa for development, build it in a separate target dir (`CARGO_TARGET_DIR=target-bench cargo build -p bench-sherpa`) so its CUDA siblings never land next to the Tauri exe.
- Don't try to fix this at WiX level (Media element split, multi-cab) — the right fix is keeping the source dir clean. Track follow-up under TASK-59 (Bench-sherpa target isolation so MSI bundler stops sweeping CUDA DLLs).
- Don't be fooled by the LGHT0306 "65535 files" wording. With our `bundle.resources` map, a healthy `main.wxs` has fewer than 50 components.

## Task tracking — Backlog.md CLI

Tasks live in `backlog/` at the repo root, managed by the **Backlog.md** CLI (npm global, but install with `pnpm add -g backlog.md`).

Useful commands:
- `backlog board` — kanban view of current state
- `backlog task list` — flat task list
- `backlog task create` — new task

Directory layout:
- `backlog/tasks/` — active tasks (`task-N - Title.md`)
- `backlog/decisions/` — architecture decision records
- `backlog/drafts/` — pre-task scoping notes
- `backlog/config.yml` — CLI config

**How to apply:** Don't suggest GitHub Issues, Linear, Jira, or an ad-hoc `TODO.md`. When a conversation surfaces new work, file it as a backlog task. When closing work, update the matching task file's status (`To Do` / `In Progress` / `Done`) and any acceptance-criteria checkboxes.
