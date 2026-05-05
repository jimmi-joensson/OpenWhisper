---
id: doc-31
title: README + BUILD.md + architecture doc — implementation plan
type: plan
created_date: '2026-05-04 16:18'
---

# README + BUILD.md + architecture doc — implementation plan

**Backlog parent:** TASK-84
**Spec:** `backlog/docs/specs/doc-30 - README-BUILD.md-architecture-doc-—-design.md`
**Milestone:** m-1 — v1.0 public release readiness

## Ordering

```
3 (BUILD.md)            ──┐
4 (architecture.md)     ──┼──► 1 (README rewrite, references both)
                          │
2 (Badges)              ──┘──► CAN merge into 1's PR or land separately
                          
5 (Demo GIF)            ──── independent, can land last
```

**Soft dependency:** Task 1 (README rewrite) lands *after* Tasks 3 (BUILD.md) and 4 (architecture.md), because the README links to both. Task 2 (badges) is small enough to fold into Task 1's PR or ship separately. Task 5 (demo GIF) is independent — placeholder ships in Task 1, real GIF lands later.

## Task 1: README rewrite

Rewrite `README.md` at repo root. Target ~120-150 lines.

Sections (in order):

1. **Project name + tagline** — `# OpenWhisper` + one-sentence pitch. Current: "Open-source, local-first dictation for macOS." Update to cover Mac+Win: "Open-source, local-first dictation for macOS and Windows." Drop "macOS" exclusivity since Win works.
2. **Badge row** — populated by Task 2.
3. **Demo** — `![OpenWhisper in action](docs/assets/demo.gif)` reference. Until Task 5 captures the GIF, ship a placeholder line: `> Demo GIF coming with the v1.0 release. Until then, see the screenshots in [`docs/screenshots/`](docs/screenshots/).` (Or omit if no screenshots exist either — better to be honest about it.)
4. **Why** — preserve existing pitch verbatim where it's good (the "Existing dictation tools paywall good local transcription..." paragraph). Sharpen the "open alternative to Superwhisper" angle without naming Superwhisper as a target.
5. **What it does** — short bullet list of v1 features that are real on `main` today: hotkey-activated dictation, pill HUD overlay, Parakeet via FluidAudio (Mac) / sherpa-onnx (Win), settings UI, custom hotkey rebind, audio ducking, follows-active-screen pill, fullscreen-app override, launch-at-login. **Each bullet must be true today** — reviewer verifies against `main`.
6. **Status** — current shipping state. `Mac: signed + notarized DMG via Releases (TASK-12 done).` `Windows: builds from source via pnpm tauri build; signed MSI in TASK-66.` `Linux: planned post-v1.` Drop the stale "Tauri release pipeline is being rebuilt (see TASK-46)" claim — TASK-46 is done.
7. **Stack** — keep current section, update to reflect ort+sherpa swap (TASK-40 done; ort is the engine on Win, not sherpa-rs directly).
8. **Principles** — keep current 4 bullets verbatim (local-first/free-by-default, BYO keys, no dark patterns, correctable). They're well-written.
9. **Install** — short sentence + link to `INSTALL.md` for end-user install (Mac DMG today; Win MSI when TASK-66 ships). Drop the inline build steps — they move to BUILD.md.
10. **Building from source** — short sentence + link to `BUILD.md` (new, Task 3). README does NOT inline the build commands — that duplicates BUILD.md and rots.
11. **Architecture** — one sentence + link to `docs/contributing/architecture.md` (new, Task 4). "If you're proposing where new logic should live, start there."
12. **Contributing** — short sentence + link to `CONTRIBUTING.md`. Note that Backlog.md is the task system (`backlog board`).
13. **License** — keep current MIT + Parakeet CC-BY-4.0 attribution.

Drop the inline `cd apps/tauri / pnpm install / pnpm dev:tauri` block from the current README — it duplicates BUILD.md content. README links; BUILD.md tells.

**Outcomes:**
- `README.md` rewritten to ~120-150 lines, accurate as of `main` today.
- "No pre-built binaries yet" claim removed; current Mac DMG release flow named correctly.
- Install section links to `INSTALL.md`; building section links to `BUILD.md`; architecture section links to `docs/contributing/architecture.md`.
- Demo placeholder or real GIF (Task 5) embedded near the top.
- Tagline updated for Mac + Windows (drop Mac-only language).
- Every feature in the "What it does" section is verifiable on `main` today.

**Verification:** Reviewer reads each numbered claim and cross-checks against repo state — does the file/release/feature exist? Reviewer also reads the README cold and confirms the three downstream docs (INSTALL.md, BUILD.md, architecture.md) are linked.

**Rename touch-points** (for TASK-NEW-5): "OpenWhisper" appears 4-6 times in body; project URL `github.com/jimmi-joensson/OpenWhisper` once or twice; tagline; Stack section section header.

## Task 2: Badges

Add a row of badges to README directly below the title. Suggested set:

```markdown
[![CI](https://github.com/<org>/<repo>/actions/workflows/ci.yml/badge.svg)](https://github.com/<org>/<repo>/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform: macOS](https://img.shields.io/badge/macOS-15+-black?logo=apple)](README.md)
[![Platform: Windows](https://img.shields.io/badge/Windows-10/11-blue?logo=windows)](README.md)
```

Badges:

- **CI status** — links to TASK-82's `ci.yml` workflow. If TASK-82 hasn't shipped yet, the badge will render as "no status" (not broken). Acceptable.
- **License** — static MIT badge.
- **Platforms** — Mac + Windows pills. Use shields.io static badges (not dynamic — there's no platform CI matrix to query).

Don't add:
- Star count (looks try-hard).
- Coverage (no coverage tooling shipped).
- Discord/Slack (no community channels yet).
- Latest release version (would need GitHub API; use after a release lands and is stable).

**Outcomes:**
- README has a 4-badge row directly below the title.
- CI badge points at the workflow URL Task TASK-82.1 will create.
- License + platform badges render correctly on GitHub.
- No broken-image badges on render (shields.io static URLs are stable).

**Verification:** Open the README on GitHub after PR merge; visually confirm 4 badges render. CI badge may show "no status" until TASK-82 has runs against `main` — that's expected.

**Rename touch-points:** `<org>/<repo>` placeholders in CI badge URL.

## Task 3: BUILD.md (new, repo root)

Author `BUILD.md` at repo root. Target ~250-300 lines. Single canonical contributor onboarding doc.

Sections (in order):

1. **Prereqs** — install fnm (link to `https://github.com/Schniz/fnm`); install Rust via rustup; install pnpm globally via fnm-managed Node (`pnpm add -g pnpm` is fine, but document fnm install first); Mac builds need Xcode CLT (`xcode-select --install`); Windows builds need MSVC toolchain (Visual Studio Build Tools 2022 with "Desktop development with C++" workload). **Pin pnpm v10** to match the dev box (the dev-workflow skill says "pnpm only", but does not pin a major today — TASK-82 and TASK-84 standardize on v10; if the maintainer disagrees, update the skill in a follow-up).
2. **Clone** — `git clone https://github.com/<org>/<repo>.git`.
3. **First-time setup** — `cd apps/tauri && pnpm install && pnpm setup:ort`. Explain `setup:ort` provisions the ONNX Runtime dylib to `~/.cache/openwhisper/onnxruntime/`.
4. **Dev loop — Mac** — `pnpm dev:tauri` runs `scripts/dev-run.cjs`, which on Mac shells out to `bash scripts/dev-run.sh` (verified at `apps/tauri/scripts/dev-run.cjs:55-57`). Explain why this Mac-only path exists (TCC drift on `pnpm tauri dev` per the dev-workflow skill: ad-hoc cdhash changes break Accessibility/Mic permission grants on every Debug rebuild). Two-line summary; link the skill for agents.
5. **Dev loop — Windows** — `pnpm dev:tauri` runs the same `dev-run.cjs`, which on Windows takes the other branch: `pnpm vendor:natives && pnpm tauri dev --config src-tauri/tauri.dev.conf.json` (no `dev-run.sh` involved on Windows). Vendor-natives provisions `WebView2Loader.dll` and `onnxruntime.dll` next to the dev exe; Debug uses load-dynamic via `init_from`. No TCC dance needed.
6. **Production build** — `pnpm tauri build` produces `.app + .dmg` on Mac, `.msi` on Win. Note: Mac requires Developer ID + notarization (TASK-12 done; see `apps/tauri/scripts/notarize-mac.cjs`). Windows MSI is unsigned today (TASK-66).
7. **Tests** — `cargo test --workspace --exclude bench-sherpa --no-default-features --features tauri` for Rust; `pnpm test:ui` from `apps/tauri/` for Playwright. Note the `--exclude bench-sherpa` and `--no-default-features` per TASK-82's spec — bench-sherpa pulls CUDA, and the default `macos-shell` feature pulls swift-bridge which won't build on Win.
8. **Cross-platform gotchas** — short paragraph + link to `.claude/skills/openwhisper-platform-gotchas/SKILL.md`. The skill is the canonical record of every regression we've eaten; reading it before touching hotkeys/audio/text-injection saves an iteration. (BUILD.md doesn't duplicate the skill content — just points at it. Footer note: "agent users have this auto-loaded.")
9. **Task tracking — Backlog.md** — install via `pnpm add -g backlog.md`; key commands (`backlog board`, `backlog task list`, `backlog task <id> --plain`); pointer to `backlog/tasks/`. Reference CONTRIBUTING.md for PR workflow.
10. **Working with Claude Code (optional, agent-driven contributors)** — short note: this repo is set up to work well with Claude Code's skill system. Skills live under `.claude/skills/openwhisper-*/`. For human contributors, the skill files are still readable as plain markdown.
11. **Footer** — "If you're an agent, the canonical version of these conventions lives in `.claude/skills/openwhisper-dev-workflow/SKILL.md`. This BUILD.md is the human-facing translation; the skill is the source of truth."

**Outcomes:**
- `BUILD.md` committed at repo root.
- A new contributor cloning the repo can follow the doc top-to-bottom and land at a working dev environment without referring to other files (except links).
- Doc cites the dev-workflow skill at footer as source of truth and documents the human/agent split.
- Cross-platform gotchas section links to the platform-gotchas skill.
- Tests section uses the exact `cargo` flags from TASK-82 plan (`--exclude bench-sherpa`, `--no-default-features --features tauri`).
- TCC drift trap on Mac is explained with a one-liner pointer to `scripts/dev-run.sh` rationale.

**Verification:** Reviewer follows the doc on a clean Mac checkout (or a clean Windows checkout if available) and reaches a `pnpm dev:tauri` running app without diverging into other docs. Reviewer also confirms the test command matches TASK-82.2's AC #1.

**Rename touch-points:** "OpenWhisper" appears in prereqs, dev-loop section, the ort cache path (`~/.cache/openwhisper/onnxruntime/`), and footer.

## Task 4: docs/contributing/architecture.md (new)

Author `docs/contributing/architecture.md`. Target ~200 lines.

Sections (in order):

1. **The rule** — one paragraph stating the orchestration-in-rust axiom: state machines, phase transitions, gating logic, status strings live in `core/`, not the platform shell. Direct prose translation of the skill's first paragraph.
2. **Why** — copy the skill's "Why" section into prose: solo dev + multi-shell = inevitable drift. Cite the April 2026 commit (`5b30e02`) where the dictation state machine moved into Rust.
3. **What lives where** — two columns or two lists, mirroring the skill's "What stays in core" / "What stays in the shell" sections. Examples:
   - Core: dictation state machine; transcript filter pipeline; recognizer trait + Mac/Win impls; settings schema; diagnostics readout.
   - Shell: NSPanel ops + tauri-nspanel; tray menu; hotkey hook (`apps/tauri/src-tauri/src/hotkey/mac.rs|windows.rs`); fullscreen detection; TCC reset; React UI components; Tauri command wrappers.
4. **ASCII diagram** — single picture showing core ↔ Tauri shell ↔ React frontend, with arrows for state-snapshot polling (~20 Hz) and event push-back from the shell. Author fresh for this doc — TASK-81's spec (doc-24) has a library-API diagram that's structurally similar but framed around the cli/lib/Tauri split, not the orchestration-vs-glue split. Adapt the *style* (ASCII boxes + arrows, no Mermaid/SVG); don't copy the boxes.
5. **Recognizer trait** — short paragraph describing the `Recognizer` trait at `core/src/recognizer/mod.rs:55` (originally specced as `SpeechRecognizer` under TASK-22, renamed to `Recognizer` during TASK-33/TASK-40 — the codebase truth is `Recognizer`). Two impls: `FluidAudioBridge` on Mac, `OrtParakeet` (sherpa-onnx via ort) on Windows. Cite the file so the reader can jump to the trait definition.
6. **Tauri command wrappers** — explain that `#[tauri::command]` functions are *thin delegations* into `core::` functions (the post-TASK-81.10 cleanup pass enforces this). A command body that does business logic is a smell.
7. **When you find drift** — paraphrase the skill's "How to apply": when you see `if phase === 'recording'` in React, push it down. Same rule applies to `INotifyPropertyChanged`-style setters in any future shell.
8. **Footer** — "If you're an agent, the canonical version of this rule lives in `.claude/skills/openwhisper-orchestration-in-rust/SKILL.md`."

**Outcomes:**
- `docs/contributing/architecture.md` committed.
- A contributor proposing a feature can read the doc and answer "where does this logic go?" without needing skill-loader access.
- ASCII diagram renders identically in GitHub web view, IDE preview, and `cat`.
- The `What lives where` table covers at minimum: state machine, transcript pipeline, recognizer trait, hotkey hook, NSPanel ops, tray menu, settings schema, fullscreen detection, TCC reset.
- Footer cites the orchestration-in-rust skill as source of truth.

**Verification:** Reviewer reads BOTH `.claude/skills/openwhisper-orchestration-in-rust/SKILL.md` and the new `architecture.md` and confirms the rules are identical — only the framing/length differs. No fabricated rules, no missing axioms.

**Rename touch-points:** "OpenWhisper" appears in body 2-3 times; cite paths under `core/src/` and `apps/tauri/src-tauri/src/` if the rename moves directories.

## Task 5: Demo GIF capture + embed

**Optional ship.** If visual freeze isn't done, ship a placeholder in Task 1 and pick this up after the milestone closes. Current `main` is reasonably stable: pill HUD has TASK-70 (active scale) done, TASK-79 (emitter stutter) done. Should be capturable.

Steps:

1. **Set up scene** — clean macOS desktop, a text editor with cursor in a known position, no notifications, no clock-changing artifacts.
2. **Record** — use macOS screen recording (Cmd+Shift+5 → record selected area) at 30 fps, ~5-7 seconds. Capture: hotkey press → pill appears → speak ~3 seconds ("hello, this is OpenWhisper transcribing locally") → hotkey press → pill disappears → text appears in editor.
3. **Trim** — use `ffmpeg` or QuickTime to trim leading/trailing dead frames.
4. **Convert to GIF + optimize** — `ffmpeg -i demo.mov -vf "fps=15,scale=720:-1:flags=lanczos,palettegen" palette.png` then `ffmpeg -i demo.mov -i palette.png -filter_complex "fps=15,scale=720:-1:flags=lanczos[v];[v][1:v]paletteuse" demo.gif`. Target output: <500 KB.
5. **Commit** to `docs/assets/demo.gif`.
6. **Update README** to replace the placeholder line with `![OpenWhisper recording and transcribing locally](docs/assets/demo.gif)`.

**Outcomes:**
- `docs/assets/demo.gif` committed, ≤500 KB, 15 fps, 720px wide.
- README's demo placeholder replaced with a real `![alt](path)` reference.
- GIF shows hotkey press → pill HUD → speech → text injection — the full dictation loop in <7 seconds.
- Alt text is accurate for screen readers ("OpenWhisper recording and transcribing locally", not "demo gif").

**Verification:** Open README on GitHub; confirm GIF auto-plays. Confirm file size <500 KB. Spot-check on mobile (some iOS browsers refuse GIFs >2 MB).

**Rename touch-points:** GIF filename if it includes the project name (it doesn't — using `demo.gif`); alt text in README references the project name.

## Cross-task verification checklist

Before marking TASK-84 done:

- [ ] All 5 subtasks `Done` in Backlog (Task 5 may be deferred — that's OK as long as Task 1 ships with a placeholder).
- [ ] `README.md` rewritten; every claim verifiable against `main`.
- [ ] `BUILD.md` exists at repo root with full prereq → first PR walkthrough.
- [ ] `docs/contributing/architecture.md` exists with the orchestration-in-rust rule externalized.
- [ ] README links to `INSTALL.md` (existing), `BUILD.md` (new), `docs/contributing/architecture.md` (new), `CONTRIBUTING.md` (existing), `LICENSE` (existing).
- [ ] No stale "TASK-46 in progress" claim or similar in README.
- [ ] Badge row renders correctly on GitHub.
- [ ] If demo GIF is shipped, it's <500 KB and shows the full record→transcribe→paste loop.
- [ ] Rename touch-points list per file is compiled and handed to TASK-NEW-5.
