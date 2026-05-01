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

## GitHub comments — edit or replace, don't stack

When updating a GitHub issue or PR comment thread on the **same topic** in close succession (same session, or within ~24 h), prefer editing the prior comment, or deleting it and posting a single replacement, over leaving a chain of consecutive comments by the same author.

**Why:** Stacked back-to-back comments from one author dilute signal and look like noise on the timeline. If the only thing that changed is "I learned more / shipped the fix," the reader wants the *current* state, not a diff log of intermediate updates. Concrete case: posting "Update — infra landed, will close after first user-facing release" and then 6 minutes later closing the issue with "Done — re-uploaded the DMG" produced two consecutive notifications and a redundant intermediate comment. The right shape was a single closing comment with the final state.

**How to apply:**
- Before composing a new comment, check whether you have a recent prior comment on the same thread (`gh api /repos/<owner>/<repo>/issues/<num>/comments`).
- If a prior comment is now **superseded** (a status changed, the answer evolved), edit it via `gh api -X PATCH /repos/<owner>/<repo>/issues/comments/<id> -f body=…` or delete it (`gh api -X DELETE …`) and post a single replacement.
- If the prior comment is **still valid context** and the new comment adds genuinely new information (new question answered, distinct phase of work), a new comment is fine.
- "Same topic" means the same conversational point — a reply to a follow-up question is a new topic; an "actually, here's the final state" follow-up to your own update is the same topic.
- Exception: when the user explicitly asks for a new comment ("post a follow-up saying X"), respect that.
- This applies symmetrically to PR review comments — don't post "actually, ignore that" right after a review comment; edit or delete the original.

## Issue close-out comments — short, addressed, linked

When closing a user-reported GitHub issue, the comment shape is: **@mention the reporter, link to the shipped artifact, one line of relevant change, thank them.** Don't include implementation forensics (SHA256, codesign output, verification commands) — those belong in release notes / `INSTALL.md` / a security advisory, not in a close-out comment.

**Why:** The reporter is a human who wants to know "is this fixed for me, and where do I get it?" — not "how do I forensically prove it." Close-out comments without an `@<reporter>` don't trigger an inbox notification on their side, so silently closing the issue lets the fix sit there unnoticed. Concrete case: closing #2 today my draft was 9 lines of hash + `spctl` block; the published version was 5 lines, `@mkrautz`-mentioned, with `[Release 0.4.0](https://…/releases/tag/v0.4.0)` as a clickable link. The shorter shape is what the reporter wanted.

**Template:**

> Done @<reporter> — <one-line summary of what shipped> in [<release name>](<link to release / PR / docs>).
>
> <one optional sentence on user-visible behavior change>
>
> Thanks again for the issue!

**How to apply:**
- Always `@<reporter>` on close-out, even when the reporter is a regular collaborator. The notification matters more than the etiquette.
- Always include a markdown link to the concrete artifact (release tag, asset URL, merged PR, updated doc) — not a bare reference like "v0.4.0".
- Cut SHA256s, `spctl`/`codesign -dv` output, environment dumps, and any "here's how to verify" code block. If verification matters, link to the place where verification *lives* (e.g. `INSTALL.md`'s troubleshooting section).
- Aim for ≤ 6 lines including blank lines. Longer means you're explaining the implementation; shorten it.
- This applies to **close-out** comments specifically. Mid-thread debugging comments and PR review comments have different shapes.

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
