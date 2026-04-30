---
id: TASK-59
title: Bench-sherpa target isolation so MSI bundler stops sweeping CUDA DLLs
status: Won't Do
assignee: []
created_date: '2026-04-30 09:30'
updated_date: '2026-04-30 16:35'
labels:
  - windows
  - build
  - release
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
The Tauri 2 WiX bundler auto-sweeps every `.dll` sitting next to the main binary in `target/release/` into `main.wxs`, on top of `bundle.resources`. It dedupes against `bundle.resources` (so vendor copies of `onnxruntime.dll` and `WebView2Loader.dll` don't double up), but it does NOT distinguish DLLs that belong to the Tauri app from leftover DLLs from sibling Cargo targets in the same workspace.

Concrete fallout during v0.4.0 release: the Windows box had previously built `bench-sherpa`, which links a CUDA-enabled ONNX runtime and deposits ~2 GB of CUDA + cuDNN DLLs (`cublas64_*`, `cublasLt64_*`, `cudnn_engines_precompiled64_*`, `cufft64_*`, `nvrtc*`, etc.) into `target/release/`. On the next `pnpm tauri build`, those got swept into `main.wxs` and pushed into the WiX CAB, which has a 2 GB hard size limit. Linking failed at `light.exe` with `LGHT0306 An error (E_FAIL) was returned while finalizing a CAB file. This most commonly happens when creating a CAB file with more than 65535 files in it.` — a misleading error since `main.wxs` only had ~32 components; the real cause was the per-CAB size cap.

Workaround used for v0.4.0: manually `rm` the 18 stray DLLs from `target/release/` before re-running the bundler. Works but is fragile — anyone who builds bench-sherpa before cutting a release re-introduces the bug.

This task is the proper fix: keep bench-sherpa's outputs out of the Tauri app's bundle source dir.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Bench-sherpa builds (whether triggered by a developer running `cargo build -p bench-sherpa` or by a script) deposit their CUDA/cuDNN DLLs OUTSIDE `target/release/`. Options: a dedicated `CARGO_TARGET_DIR=target-bench` shim wrapper, a Cargo workspace `[profile.release-bench]` redirect, or moving bench-sherpa into a sub-crate with its own `target/`. Pick whichever the maintainer prefers.
- [ ] #2 `pnpm tauri build` on a clean checkout that has previously built bench-sherpa succeeds without manual cleanup. Reproduce by: `cargo build --release -p bench-sherpa && cd apps/tauri && pnpm tauri build` — must produce both MSI + NSIS setup without LGHT0306.
- [ ] #3 Post-fix: `target/release/wix/x64/main.wxs` after a fresh build references only `apps\tauri\src-tauri\vendor\WebView2Loader.dll`, `apps\tauri\src-tauri\vendor\onnxruntime.dll`, and `target\release\openwhisper-tauri.exe` for File Source paths — no CUDA/cuDNN/`cargs.dll` paths. Verify with `grep -oE 'Source="[^"]+"' target/release/wix/x64/main.wxs | sort -u`.
- [ ] #4 Documented in `apps/tauri/scripts/vendor-natives.cjs` (or a sibling pre-build hook) AND in the openwhisper-dev-workflow skill that bench-sherpa target isolation is the supported convention. The dev-workflow skill section "Tauri MSI bundler auto-sweeps stray DLLs from `target/release/`" should be updated to point at this task once it lands.
- [ ] #5 If a wrapper script approach is used: it lives at a discoverable path (`scripts/build-bench.sh` / `.cmd`) and the project README or backlog readme references it as the way to build bench targets.
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Background pointers:

- The bundler's auto-sweep behavior is in upstream `tauri-bundler/src/bundle/windows/msi/wix.rs` — it walks the binary's parent directory and adds every `.dll` it finds. EXEs are NOT swept (so `bench-sherpa.exe` itself is harmless; only its CUDA siblings are pulled in).
- The dedupe against `bundle.resources` is by destination path — `vendor/onnxruntime.dll → onnxruntime.dll` matches a `target/release/onnxruntime.dll` if both end up at the same flattened name, so the latter is skipped. CUDA DLLs have no entry in `bundle.resources`, so they're added unconditionally.
- WiX's CAB writer (`light.exe`) imposes a 2 GB hard cap per CAB; LGHT0306 surfaces this as the "65535 files" error, which is misleading. The MSI body itself can be larger if split across multiple CAB Media elements, but that's a workaround — the right fix is keeping the source set small.
- `pnpm tauri build` swallows the inner light.exe stderr; use `pnpm tauri build --verbose` to surface the real error (this is how v0.4.0's release agent diagnosed the issue).

Likely lowest-friction implementation: a `scripts/build-bench.cmd` (and `.sh`) wrapper that sets `CARGO_TARGET_DIR=%REPO%\target-bench` before invoking `cargo build`, plus a `.gitignore` entry for `target-bench/`. That avoids restructuring crates and keeps the workspace single-`target/` for everyday Rust development on the openwhisper-tauri / openwhisper-core side.

Avoid the temptation to fix this at WiX level (Media element split, multi-CAB) — that just lets the broken state ship a bigger MSI. The fix is keeping unrelated artifacts out of the bundler's source dir.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed during 2026-04-30 backlog review as Won't Do. Post-v0.4.0 priorities reset; bench-sherpa target-isolation work will be re-planned from current state if/when revisited. Workaround captured in the openwhisper-dev-workflow skill (DLL-sweep gotcha) — current build flow tolerates the failure mode by avoiding bench-sherpa builds in the same workspace before MSI packaging.
<!-- SECTION:FINAL_SUMMARY:END -->
