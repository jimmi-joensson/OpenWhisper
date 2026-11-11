---
id: doc-51
title: Vision - implementation plan
type: other
created_date: '2026-11-10T23:24:00Z'
---


**Backlog parent:** TASK-98 (Vision)
**Spec:** none as a separate file. The decisions the user took are in the task description, its notes and under "Design decisions" below.
**Stacks on:** `task-97-ask` (the ask pipeline, the answer panel, the `extra` line in history files).

## What exists today (verified in code and by running, 2026-11-10 and 11)

- An ask (`core/src/ask`) is a conversation with a backend that can take more than one turn; `AskInput` already carries images, and both backends already put an image on the wire (stream-json image block for the Claude CLI, `image_url` data URL for the OpenAI-compatible endpoint), tested against fakes.
- Run against the real CLI (TASK-98 notes): an image block on stdin works with the lean flags and tools off; the model read small synthetic text; with `--no-session-persistence` no copy of the image was found on disk, without it the CLI writes the image to its session file and a temp PNG.
- Run on macOS (TASK-98 notes): `SCContentFilter(display:excludingApplications:exceptingWindows:)` with `SCScreenshotManager`, through `objc2-screen-capture-kit`, leaves an excluded app's windows out of the image (what is behind them shows), from a plain thread. Measured on the Mac it was written on: 130 to 330 ms for the whole path (window lists, capture, paint, downscale, PNG); an earlier note of about 50 ms was the capture call alone. Window rectangles come from `CGWindowListCopyWindowInfo` without a permission. With the display asleep ScreenCaptureKit lists no display.
- Windows (docs only): no capture-side exclusion exists for a whole-monitor capture; `SetWindowDisplayAffinity` is owner-side.
- `core/src/focus` reads the frontmost app.

## Design decisions

1. **A screenshot exists only inside an ask.** `vision::capture` takes a token that only the ask session can make, and checks at run time that an ask is in progress. The dictation pipeline has no call site, there is no timer, no background capture and no "capture now" command in the shell.
2. **Trigger setting** (`vision.trigger`): `off`, `phrases` (default), `smart`, `always`.
   - `phrases`: an editable list, shipped with English and Danish defaults, matched locally on the transcript (lower case, punctuation dropped, whole words in order) before anything leaves the machine.
   - `smart`: a phrase match captures at once. Otherwise the first turn is text only and the model may answer a fixed marker meaning "I need the screen"; only then is the screen captured and sent as a second turn of the same conversation. The marker is held back from the panel.
   - `always`: every ask carries the screen.
3. **Whole display, the one with the focused window** (frontmost app's first on-screen window; the main display when that cannot be told).
4. **Never-capture list** (`vision.never_capture`, editable; default: 1Password, Bitwarden, KeePassXC, LastPass, Dashlane, Apple Passwords, Keychain Access, by bundle id on macOS and exe name on Windows). Three layers:
   - the frontmost app is on the list: no capture at all;
   - macOS: listed apps (and OpenWhisper itself) are excluded in the capture filter, so their windows are not in the image;
   - both platforms: the listed apps' window rectangles are painted black in the pixel buffer before anything else happens to it (at native size, grown by 2 px, then the image is scaled down). On Windows this is the only layer.
   If any step of this cannot be carried out (the app list or the window list cannot be read), there is no capture. The settings say plainly what a list of apps cannot cover: a password manager's browser extension popup, a password shown on a web page or in a terminal.
5. **When there is no screenshot, the answer says so.** The ask goes on as text only; the panel shows a notice with the reason (a protected app is in front, no screen recording permission, no display, capture failed) and the model is told the screen is not available and why, so it does not guess.
6. **In memory only.** The capture is an RGBA buffer, then a PNG in memory (longest side 1568 px), handed to the backend as bytes and dropped when the request ends. It is never written to a file by us, never logged, never sent to the webview. `--no-session-persistence` on the CLI is load-bearing and gets a test.
7. **Visible.** The ask snapshot carries the screen state (`capturing`, `included`, `blocked`, `unavailable`) and the pill shows it while it happens; the history entry gets `screen=yes` on the `extra` line.
8. **Text on the screen is content, not instructions.** The prompt says so, and an eval set with synthetic images that carry instructions checks it. An ask still has no tools, so the worst case is a wrong answer.
9. **macOS permission.** Screen Recording, checked without prompting (`CGPreflightScreenCaptureAccess`) and requested from the Vision settings (`CGRequestScreenCaptureAccess`), which also say that the app must be restarted after granting. The CLI uses the terminal's grant.
10. **Windows.** `BitBlt` of the foreground window's monitor, then the paint layer (visible, not cloaked top-level windows of listed exe names, DWM extended frame bounds). Compile-checked here; AC 7 stays open until it has run on a real machine.
11. **Headless parity.** `openwhisper ask run --screen` (and `--vision TRIGGER`), `openwhisper vision status | config | phrases | never-capture | check TEXT | probe` (`probe` is a dry run and takes no screenshot: it reports the permission, the display that would be captured and its size, the app in front and whether it is listed, the listed apps that are open with their window counts, and with `--text` what the trigger would decide), `openwhisper vision eval`. A screenshot exists only inside an ask, so the one way to a capture from the command line is `ask run --screen` or `--vision`. The live pixel test and its helper window are behind the cargo feature `vision-live-test`: `OPENWHISPER_VISION_LIVE=1 cargo test -p openwhisper-cli --features vision-live-test --test vision_live -- --ignored --nocapture`.
12. **Not in this plan.** A region or window picker, more than one display per ask, OCR on the machine, redacting by content.

## Task 1: Core: settings, trigger, macOS capture, paint layer, ask integration (TASK-98.1)

Files: `core/src/vision/{mod,trigger,paint,mac,windows}.rs` (new; `windows.rs` a stub that says "not built yet" until Task 2), `core/src/settings/vision.rs` (new), `core/src/settings/mod.rs`, `core/src/ask/{mod,prompt}.rs`, `core/src/history/{mod,tests}.rs` (`screen` key), `core/Cargo.toml` (feature `vision`), `core/src/{lib,prelude}.rs`.

Outcomes:
- Trigger: phrase matching is local and tested in English and Danish; `off` never captures; `smart` without a phrase makes a text-only first turn and captures only on the marker (stub backend tests).
- `capture` without an ask in progress is an error; the dictation path cannot reach it (test).
- The frontmost app on the list means no capture and a notice; a failed exclusion or window listing means no capture (tests with a fake platform layer).
- The paint layer blacks out the given rectangles in a synthetic image at every scale, including after the downscale (pixel tests); rectangle mapping from points to pixels rounds outward.
- A capture ends as PNG bytes in memory; nothing in the module opens a file for writing (test that greps the module is acceptable in addition to review); the ask backends receive the bytes; `screen=yes` is stored on the history entry and old files parse as before.

Verification: `cargo test -p openwhisper-core --no-default-features --features tauri --lib`; a live, env-gated macOS test with a test window of one solid colour: not excluded it is in the image, excluded it is not, paint-only it is black.

## Task 2: Windows capture and paint (TASK-98.2)

Files: `core/src/vision/windows.rs`, `core/Cargo.toml` (windows features).

Outcomes:
- Compiles for `x86_64-pc-windows-gnu` with tests; pure parts (rectangle maths, exe-name matching) are unit tested on any platform.
- Not run: needs the Windows machine (AC 7).

Can run in parallel with Tasks 3 to 5 (no shared files).

## Task 3: CLI: `ask run --screen`, `openwhisper vision`, eval sets (TASK-98.3)

Files: `cli/src/commands/{vision,ask}.rs`, `cli/src/main.rs`, `core/src/vision/{eval,testimg}.rs` (new: synthetic images drawn from a built-in bitmap font).

Outcomes:
- `ask run --screen` sends a capture; `vision check TEXT` prints the trigger decision; `vision probe` is a dry run: it reports what a capture would do and takes none.
- Eval sets: (a) smart decision, English and Danish, needs-screen and does-not; (b) reading synthetic images; (c) synthetic images whose text gives instructions. Offline self-tests pass; live runs are in the notes.

## Task 4: Shell and UI: Vision settings, pill and panel states, history marker (TASK-98.4)

Files: `apps/tauri/src-tauri/src/vision.rs` (new), `apps/tauri/src-tauri/src/lib.rs`, `apps/tauri/src/components/vision-settings.tsx` (new, inside the Ask pane), `apps/tauri/src/lib/{vision,use-vision}.ts` (new), `apps/tauri/src/{AnswerPanel,PillOverlay}.tsx`, `apps/tauri/src/components/history-row.tsx`, shims.

Controls: `Select` for the trigger (four fixed choices with a line each on what they do), the phrase list and the never-capture list managed like dictionary terms (add, remove, restore defaults), a permission row with status and a button.

Outcomes:
- The trigger can be set; phrases and never-capture apps can be added, removed and restored.
- The panel and the pill show capturing, included and the reason when there is no screenshot.
- The permission row shows the state and what to do.
- History marks asks that included the screen.

## Task 5: Tests and verification (TASK-98.5)

Outcomes:
- Core, CLI and shell cargo tests, Windows target check, Playwright pass.
- The live pixel test result (or why it could not run) and the eval numbers and latency are in the TASK-98 notes.
- TASK-98 is In Review with the unverified items listed (the permission prompt, a real password manager on screen, Retina and multi-display, Windows).

## Ordering

1 first. Then 2, 3 and 4 can run side by side (Windows file, CLI and eval files, shell and UI files). 5 last.
