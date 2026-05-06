---
id: doc-38
title: Status footer bar — implementation plan
type: specification
created_date: '2026-05-06 05:10'
---

**Backlog parent:** TASK-86
**Spec:** backlog/docs/specs/doc-37 - Status-footer-bar-—-design.md

Four tasks, ordered by dependency. Tasks 1 and 2 can run in parallel (Rust surface vs. shadcn primitive). Task 3 depends on both. Task 4 depends on 3.

---

### Task 1: Add `recognizer_info` Tauri command + Recognizer trait method

Surface the engine name + origin from Rust so the React shell does not hardcode it.

**Files:**

- `core/src/recognizer/mod.rs` — extend `Recognizer` trait with `fn info(&self) -> RecognizerInfo` (new struct in same file). `RecognizerInfo { name: String, origin: String }`.
- `core/src/recognizer/fluidaudio.rs` — implement `info()` returning `("Parakeet", "on-device")`.
- `core/src/recognizer/ort_parakeet.rs` — implement `info()` returning `("Parakeet", "on-device")`.
- `core/src/recognizer/mod.rs` — add a free function `pub fn recognizer_info() -> Option<RecognizerInfo>` that mirrors the existing `recognizer_transcribe` accessor pattern: read the `ENGINE: OnceLock<Mutex<Box<dyn Recognizer>>>` (line 64), `lock()` the mutex, return `Some(guard.info())` if the engine has been initialized, otherwise `None`. Returning `Option` (instead of `Result<_, String>`) reflects that "engine not yet loaded" is the only failure mode and the React side wants to render a fallback, not surface an error.
- `apps/tauri/src-tauri/src/lib.rs` — add `#[tauri::command] fn recognizer_info() -> Option<core::recognizer::RecognizerInfo>` that calls the new core accessor directly. Register in `invoke_handler!` macro at line ~1181.
- `core/src/lib.rs` — re-export `RecognizerInfo` if not already in scope from `recognizer::mod`.

**Outcome ACs:**

- `RecognizerInfo` struct exists in `core::recognizer` with `name` + `origin` fields, both `String`, derives `Serialize` (for Tauri) + `Clone`.
- Both recognizer impls return concrete strings, no `unimplemented!()` paths.
- `recognizer_info()` core accessor returns `Some(RecognizerInfo)` when the engine is initialized and `None` before `recognizer_ensure_loaded` has run.
- `recognizer_info` Tauri command is callable from React via `invoke<RecognizerInfo | null>("recognizer_info")`.
- Unit tests exist covering each `info()` impl asserting non-empty `name` and `origin`, plus a test of the `recognizer_info()` accessor returning `None` before `ensure_loaded` and `Some` after.

**Verification:**

- `cargo test -p openwhisper-core recognizer::` runs green, including the new tests for both `info()` impls and the `recognizer_info()` accessor.
- Manual smoke during `pnpm tauri dev`: in DevTools console `await window.__TAURI__.core.invoke("recognizer_info")` returns `{ name: "Parakeet", origin: "on-device" }` after the recognizer has loaded, `null` if invoked before load.

---

### Task 2: Add shadcn `Kbd` component to apps/tauri

Project's `apps/tauri/src/components/ui/` does not currently include `kbd`. Add it via the shadcn CLI so it sits alongside the other primitives, then verify it composes with the project's tailwind config.

**Files:**

- `apps/tauri/src/components/ui/kbd.tsx` (new) — output of `pnpm dlx shadcn@latest add kbd` run from `apps/tauri/`.
- Verify `apps/tauri/src/App.css` already imports the shadcn theme tokens the new component needs (it should — `Button`, `Card`, etc. are already wired). If `Kbd` introduces a token missing in the `@theme` block, add it.

**Outcome ACs:**

- `kbd.tsx` is committed under `components/ui/` with the exports the project's other shadcn components follow.
- A `<Kbd>⌘,</Kbd>` rendered in a real consumer (throwaway placement in Home pane is fine, removed in Task 3) renders with a visible border + monospace font matching the existing `<kbd>{chord}</kbd>` styling in `home-pane.tsx:83`.
- TypeScript compiles cleanly with the new component imported and used; no regression in the existing Playwright suite.

**Verification:**

- `pnpm build` clean.
- `pnpm test:ui` green.
- Visual smoke during `pnpm tauri dev`.

---

### Task 3: Build the `<StatusFooter>` component and wire it into `App.tsx`

Compose the footer using `Kbd` (Task 2) + `recognizer_info` (Task 1). Lift the footer into `App.tsx` as a sibling after `ow-app__shell`.

> **Executor note:** before editing UI files in `apps/tauri/src/components/`, load the `openwhisper-ui-discipline` skill (which itself loads the `shadcn` skill). Verify `Kbd` is the canonical shadcn primitive for keycap rendering before composing the layout.

> **Status-text mapping lives in React.** The phase-to-phrase map ("Ready" / "Recording" / "Transcribing" / "Error") is intentionally a shell-side decision because it is user-facing copy, not orchestration logic. The Rust side already exposes the phase enum via `dictation.phase`; only the rendering of the corresponding word lives here. This is a deliberate carve-out from the general `openwhisper-orchestration-in-rust` rule and is documented here so a later reader does not "fix" it by pushing the strings into core.

**Files:**

- `apps/tauri/src/components/status-footer.tsx` (new) — three-section layout with `flex` row, sidebar-width left region, `flex-1` middle, fixed-width right. Use semantic Tailwind tokens (`bg-background`, `text-muted-foreground`, `border-t border-border`).
- `apps/tauri/src/lib/use-recognizer-info.ts` (new) — small hook: `useEffect` invokes `recognizer_info` once at mount, stores `{ name, origin } | null` in state. Returns the value.
- `apps/tauri/src/App.tsx` — wrap shell + footer in a `flex flex-col h-screen` parent; render `<StatusFooter>` after `ow-app__shell`. Pass `dictation.phase`, the recognizer info, and a `onOpenSettings={() => setRoute("settings")}` callback.
- `apps/tauri/src/App.css` — add `.ow-app__footer` rules for height (`32px`), border-top, and the internal grid (sidebar-width + 1fr). Read `--ow-sidebar-width` from the existing sidebar definition rather than duplicating the constant.
- Status-dot color mapping: small map from `dictation.phase` enum (imported from `lib/dictation.ts`) to `bg-emerald-500` / `bg-red-500` / `bg-amber-500`.
- Status-text mapping: same source, returns `"Ready" | "Recording" | "Transcribing" | "Error"`.

**Layout shape:**

```tsx
<footer className="ow-app__footer flex items-center border-t border-border bg-background text-sm">
  <div className="ow-app__footer-left flex items-center gap-2 px-4 hover:bg-accent/40 cursor-pointer" onClick={onOpenSettings}>
    <Kbd>{platform === "macos" ? "⌘," : "Ctrl+,"}</Kbd>
    <span className="text-muted-foreground uppercase tracking-wide">Settings</span>
  </div>
  <div className="flex-1 flex items-center gap-2 px-4">
    <span className={cn("size-2 rounded-full", dotColor)} />
    <span>{phaseLabel}</span>
    {info && (
      <>
        <span className="text-muted-foreground">·</span>
        <span>{info.name}</span>
        <span className="text-muted-foreground">·</span>
        <span className="text-muted-foreground">{info.origin}</span>
      </>
    )}
  </div>
  <div className="flex items-center gap-2 px-4">
    <span className="text-muted-foreground">Hotkey</span>
    <Kbd>{hotkeyLabel || "—"}</Kbd>
  </div>
</footer>
```

(Final implementation may differ in classNames; this shape captures the structure.)

**Outcome ACs:**

- `StatusFooter` renders in all three routes (Home, Settings, Diagnostics). The `.ow-app__footer` element measures exactly 32 px tall in every route — verified by the Playwright spec in Task 4 reading `boundingBox().height` after each route swap.
- Settings-hint click region navigates to `route="settings"`; on Windows the keycap shows `Ctrl+,` instead of `⌘,`.
- Status dot color follows `dictation.phase` (verified by manually triggering record / stop and watching the color flip).
- Engine name + origin appear after `recognizer_info` resolves; before that, only the dot + phrase show.
- Hotkey region updates within one render after a Settings rebind (no stale value).
- Footer is never covered by the pill overlay (z-index ordering OK — footer is in the main window, pill is its own window).

**Verification:**

- `pnpm tauri dev`, exercise: launch (empty state) → see `● Ready · Parakeet · on-device`; press hotkey → dot turns red, label "Recording"; release → "Transcribing" (briefly amber) → back to "Ready"; click `⌘, SETTINGS` → routes to Settings; rebind hotkey in Settings → return to Home, verify keycap on the right matches.
- `pnpm build` clean.

---

### Task 4: Playwright coverage for the footer

Add a spec exercising the empty state, the Settings-hint click, and the post-rebind hotkey update. Per CLAUDE.md, Playwright is mandatory verification for any rebind-UI-touching change — and Task 3 reads from `useCurrentHotkey`, which the rebind UI updates.

**Files:**

- `apps/tauri/tests/status-footer.spec.ts` (new) — three test cases:
  1. **Empty state**: launch app, assert footer contains `Settings` text + a kbd showing `⌘,` (or `Ctrl+,` on Windows runner), `Ready` text, and a kbd in the right region with non-empty content.
  2. **Settings click**: click the left footer region, assert URL/route reflects Settings (whatever signal the existing `app.spec.ts` uses for route).
  3. **Hotkey rebind reflection**: open Settings → Hotkeys, rebind toggle to a new chord, return to Home, assert the right-side kbd's text matches the new chord.

**Outcome ACs:**

- Spec file added with the three cases above; the new spec is discovered by Playwright's existing config and reports as passing in the project's `pnpm test:ui` run.
- A fourth assertion in case 1 (or a new case) asserts `.ow-app__footer` `boundingBox().height === 32` on Home and Settings routes, locking the layout-stability AC from Task 3.
- Test 3 fails if `StatusFooter` is not re-reading from `useCurrentHotkey` (regression guard against a future refactor that caches the hotkey in component state).

**Verification:**

- `pnpm test:ui` is actually executed and reports green, including the new spec. Per CLAUDE.md, reading the test file and inferring assertions are still satisfied is NOT acceptable for rebind-UI-touching changes.

---

## Cross-task notes

- **Parallelism:** Tasks 1 and 2 are independent. Task 3 needs both. Task 4 needs Task 3.
- **No deferred decisions.** Color tokens, layout structure, status enum mapping, click target shape — all decided here. Empty state copy ("Hotkey —") decided. Engine-not-yet-loaded fallback (collapse middle region) decided.
- **Out-of-scope reminders:** No status-dot animation, no engine swap UI, no hotkey rebind from footer, no stats anywhere. Stats is a separate ticket TBD.
