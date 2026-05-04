---
id: doc-11
title: 'Windows custom titlebar (Slack-style) + sidebar continuity — implementation plan'
type: plan
created_date: '2026-05-01 00:00'
---

# Windows custom titlebar (Slack-style) + sidebar continuity — implementation plan

> **For agentic workers:** Pick up the matching `TASK-68.N` subtask, mark it In Progress, work through the steps, append commit refs to the subtask notes, and check ACs as you go. Steps use checkbox (`- [ ]`) syntax for tracking.

**Backlog parent:** TASK-68
**Spec:** `backlog/docs/specs/2026-05-01-windows-custom-titlebar.md`
**Date:** 2026-05-01

**Goal.** Replace the OS-default Windows title bar with a single dark, app-drawn titlebar carrying min/max/close on the right. Restructure the layout so the sidebar runs from `y=0` and the titlebar inset sits over the content column only — kills the visible seam between the back-arrow strip and the sidebar on both platforms. Mac's traffic-light Overlay behavior is unchanged.

**Architecture.** Rust setup() drops Windows decorations behind `#[cfg(target_os = "windows")]`. React adds a `<WindowControls>` component (Windows-only render) that calls `getCurrentWindow().minimize()/toggleMaximize()/close()`. CSS grid restructures: sidebar column is full-height, titlebar is inset inside the content column. No state-machine changes; no Rust orchestration changes.

**Tech stack.** Tauri 2.10, `@tauri-apps/api/window`, lucide-react (already a dep). No new packages.

Each `### Task N:` heading maps 1:1 to a Backlog subtask `TASK-68.N`.

---

## File structure

| Action | File | Responsibility |
|---|---|---|
| Modify | `apps/tauri/src-tauri/src/lib.rs` | Drop decorations on Windows in `setup()` (cfg-gated) |
| Modify | `apps/tauri/src-tauri/capabilities/default.json` | Add `core:window:allow-minimize` / `allow-toggle-maximize` / `allow-close` / `allow-is-maximized` |
| Create | `apps/tauri/src/components/window-controls.tsx` | Min/Max/Close buttons; Windows-only render; isMaximized listener |
| Modify | `apps/tauri/src/App.tsx` | Layout restructure (sidebar from y=0, titlebar inset over content column) |
| Modify | `apps/tauri/src/App.css` | `.ow-app__shell` grid + `.ow-titlebar` inset styles + `.ow-window-controls` |
| Modify | `apps/tauri/tests/main-window.spec.ts` | Layout-shape assertions + Windows-platform branch test |
| Create | `apps/tauri/tests/window-controls.spec.ts` | WindowControls render/click test (Windows-platform forced via init script) |
| Modify | `.claude/skills/openwhisper-platform-gotchas/SKILL.md` | Document the `decorations: false` + `WS_THICKFRAME` retention finding |

---

### Task 1: Sidebar runs from y=0; titlebar inset over content column

**Goal.** Restructure the layout so the sidebar is the leftmost column from window top to bottom, and the titlebar lives **inside** the content column (not full-width above the grid). Fixes the seam between back-arrow and sidebar on both platforms. Mac gets a `38px` sidebar `padding-top` to clear traffic-lights (matches spec D5).

**Files.**
- Modify: `apps/tauri/src/App.tsx`
- Modify: `apps/tauri/src/App.css`
- Modify: `apps/tauri/tests/main-window.spec.ts` (one new layout-shape test)

**Steps.**

- [ ] **Step 1: Write the failing layout test.**

  In `main-window.spec.ts`, add inside `test.describe("sidebar nav")`:
  ```ts
  test("sidebar starts at window top; titlebar inset over content column", async ({ page }) => {
    await page.goto("/");
    const sidebarBox = await page.getByTestId("sidebar-item-home").boundingBox();
    // Sidebar's first item is within the top 60 px of the window — i.e. the
    // sidebar column starts at or near y=0.
    expect(sidebarBox && sidebarBox.y).toBeLessThan(60);

    // Titlebar (when settings is open) sits at x > 0 — inside content column,
    // not full-width.
    await page.getByTestId("sidebar-item-settings").click();
    const back = page.getByRole("button", { name: "Back to main" });
    const backBox = await back.boundingBox();
    expect(backBox && backBox.x).toBeGreaterThan(150); // sidebar is 180px wide
  });
  ```

- [ ] **Step 2: Run tests to verify it fails.**

  ```bash
  cd apps/tauri && pnpm exec playwright test main-window.spec.ts --grep "sidebar starts at window top"
  ```

  Expected: FAIL — current layout has titlebar at full width (back button at x≈14).

- [ ] **Step 3: Restructure App.tsx.**

  Move `<header>` (titlebar) inside the content column. Outer shape — note every draggable descendant repeats `data-tauri-drag-region` (drag.js does NOT walk ancestors per `openwhisper-platform-gotchas` "Window drag silently no-ops"):

  ```tsx
  <div className="ow-app">
    <div className="ow-app__shell">
      <SidebarNav ... />
      <div className="ow-app__column">
        <header className="ow-titlebar" data-tauri-drag-region>
          {route === "settings" && (
            <>
              <button
                className="ow-titlebar__back"
                onClick={goBack}
                aria-label="Back to main"
                data-tauri-drag-region="false"  /* opts OUT — onClick must fire */
              >←</button>
              <h1 className="ow-titlebar__title" data-tauri-drag-region>
                Settings
              </h1>
            </>
          )}
          <WindowControls platform={platform} />
        </header>
        <main className="ow-app__body">
          {/* route panes */}
        </main>
      </div>
    </div>
  </div>
  ```

  The `.ow-app__shell` grid stays `180px 1fr`. The new `.ow-app__column` is a flex-column inside the right cell. WindowControls component sets `data-tauri-drag-region="false"` on each button internally and `data-tauri-drag-region` on the wrapping cluster div (so the empty space between buttons stays draggable).

- [ ] **Step 4: App.css updates.**

  - `.ow-app__column`: `display: flex; flex-direction: column; min-height: 0; min-width: 0;`
  - `.ow-titlebar`: drop the `padding-left: 80px` (was reserving space for traffic-lights at window-top-left); titlebar is inset now.
  - `.ow-sidebar`: `padding-top: 14px` stays for non-Mac. On Mac, bump to `padding-top: 38px` (one number across spec/plan/AC, see spec D5) so traffic-lights clear the first item. Implementation: `body[data-platform="macos"] .ow-sidebar { padding-top: 38px; }` — set `data-platform` on `<body>` from `App.tsx` `useEffect` based on `detectPlatform()`.
  - `.ow-app__body`: drop `overflow-y: auto` from the outer `.ow-app__body` if it duplicates with the inner pane scroll. Verify the existing scroll spec still passes.

- [ ] **Step 5: Set body[data-platform] in App.tsx.**

  ```tsx
  useEffect(() => {
    document.body.setAttribute("data-platform", platform);
  }, [platform]);
  ```

- [ ] **Step 6: Run tests.**

  ```bash
  cd apps/tauri && pnpm tsc --noEmit && pnpm exec playwright test
  ```

  Expected: new test green; existing scroll spec still green; main-window sidebar-nav specs still green.

- [ ] **Step 7: Commit.**

  ```bash
  git add apps/tauri/src/App.tsx apps/tauri/src/App.css apps/tauri/tests/main-window.spec.ts
  git commit -m "Tauri: sidebar from y=0; titlebar inset over content column"
  ```

**Outcome ACs (Backlog).**
- Sidebar's first item renders within the top 60 px of the window.
- Settings titlebar back-arrow renders at `x > 150` (inside content column, not full-width).
- Mac sidebar has `padding-top: 38px` via `body[data-platform="macos"]` selector; non-Mac stays at `14px`.
- Existing main-window + scroll specs pass; one new layout-shape test added and passing.

---

### Task 2: Capabilities for window control IPCs

**Goal.** Add the four window-control capabilities the React WindowControls component will invoke. No code change — just permission additions. This must land before Task 3, otherwise IPC calls silently reject (per the macOS drag-region precedent in `openwhisper-platform-gotchas`).

**Files.**
- Modify: `apps/tauri/src-tauri/capabilities/default.json`

**Steps.**

- [ ] **Step 1: Add the four allow-* lines.**

  In `default.json`, add to the `permissions` array:
  - `core:window:allow-minimize`
  - `core:window:allow-toggle-maximize`
  - `core:window:allow-close`
  - `core:window:allow-is-maximized`

  Verify against `~/.cargo/registry/src/index.crates.io-*/tauri-2.10.3/permissions/window/autogenerated/reference.md` that all four exist (they do; documented under "Permission Table").

- [ ] **Step 2: Type-check the capabilities file.**

  Tauri 2 validates capabilities at `cargo build`; a typo crashes the build. Trigger a quick check:
  ```bash
  cd apps/tauri && cargo check -p openwhisper-tauri 2>&1 | tail -20
  ```

  Expected: clean compile (warnings OK; errors not OK).

- [ ] **Step 3: Commit.**

  ```bash
  git add apps/tauri/src-tauri/capabilities/default.json
  git commit -m "Tauri: capabilities — allow window minimize/toggle-maximize/close/is-maximized"
  ```

**Outcome ACs (Backlog).**
- `default.json` lists all four window-control capabilities.
- `cargo check -p openwhisper-tauri` is clean (no errors).

---

### Task 3: WindowControls component (Windows-only render)

**Goal.** Render the min/max/close cluster on the right of the inset titlebar. Windows-only — Mac collapses to nothing (traffic-lights handle it). Maximize icon swaps to restore when window is maximized; listen to `tauri://resize` events to keep the icon in sync.

**Files.**
- Create: `apps/tauri/src/components/window-controls.tsx`
- Modify: `apps/tauri/src/App.tsx` (render WindowControls inside titlebar; pass platform)
- Modify: `apps/tauri/src/App.css` (add `.ow-window-controls` styles)
- Create: `apps/tauri/tests/window-controls.spec.ts`

**Steps.**

- [ ] **Step 1: Write the failing WindowControls spec.**

  ```ts
  // apps/tauri/tests/window-controls.spec.ts
  import { expect, test } from "./fixtures/tauri-shim";

  test.describe("window controls (Windows-only)", () => {
    test("renders min/max/close on Windows", async ({ page }) => {
      await page.addInitScript(() => {
        Object.defineProperty(navigator, "platform", { value: "Win32" });
      });
      await page.goto("/");
      await expect(page.getByTestId("window-control-minimize")).toBeVisible();
      await expect(page.getByTestId("window-control-maximize")).toBeVisible();
      await expect(page.getByTestId("window-control-close")).toBeVisible();
    });

    test("does not render on Mac", async ({ page }) => {
      // Default tauri-shim runs as Mac platform; no init override.
      await page.goto("/");
      await expect(page.getByTestId("window-control-close")).toHaveCount(0);
    });

    test("clicking minimize invokes window minimize IPC", async ({ page }) => {
      await page.addInitScript(() => {
        Object.defineProperty(navigator, "platform", { value: "Win32" });
      });
      await page.goto("/");
      await page.getByTestId("window-control-minimize").click();
      // tauri-shim records IPC calls on window.__owWindowCalls (extend shim if missing).
      const calls = await page.evaluate(
        () => (window as unknown as { __owWindowCalls?: string[] }).__owWindowCalls ?? [],
      );
      expect(calls).toContain("minimize");
    });
  });
  ```

  Extend `tests/fixtures/tauri-shim.ts` if it doesn't already shim `plugin:window|*` IPCs — record each invoked command name on `window.__owWindowCalls`.

- [ ] **Step 2: Verify failure.**

  ```bash
  cd apps/tauri && pnpm exec playwright test window-controls.spec.ts
  ```

  Expected: FAIL — testids don't exist yet.

- [ ] **Step 3: Create `window-controls.tsx`.**

  Use `getCurrentWindow().onResized(cb)` (window-scoped, returns the unlisten fn directly) — NOT the global `listen("tauri://resize", …)`, which doesn't fire reliably for synthetic resize events in Tauri 2.10. The maximize/restore glyphs are hand-rolled SVGs because lucide-react has no Win 11 "restore-down" overlapping-squares icon (`Copy` is a clipboard glyph, wrong shape).

  ```tsx
  import { useEffect, useState } from "react";
  import { Minus, X } from "lucide-react";
  import { getCurrentWindow } from "@tauri-apps/api/window";

  interface WindowControlsProps {
    platform: "macos" | "windows";
  }

  // Win 11 chrome maximize glyph — a single rounded square outline.
  function MaximizeGlyph() {
    return (
      <svg width="11" height="11" viewBox="0 0 11 11" aria-hidden="true">
        <rect x="0.75" y="0.75" width="9.5" height="9.5" rx="1" fill="none" stroke="currentColor" strokeWidth="1" />
      </svg>
    );
  }

  // Win 11 chrome restore-down glyph — two overlapping squares (front lower-left, back upper-right).
  function RestoreGlyph() {
    return (
      <svg width="11" height="11" viewBox="0 0 11 11" aria-hidden="true">
        <rect x="2.5" y="0.75" width="7.75" height="7.75" rx="1" fill="none" stroke="currentColor" strokeWidth="1" />
        <rect x="0.75" y="2.5" width="7.75" height="7.75" rx="1" fill="none" stroke="currentColor" strokeWidth="1" />
      </svg>
    );
  }

  export function WindowControls({ platform }: WindowControlsProps) {
    const [maximized, setMaximized] = useState(false);

    useEffect(() => {
      if (platform !== "windows") return;
      const win = getCurrentWindow();
      void win.isMaximized().then(setMaximized);
      const unlistenPromise = win.onResized(() => {
        void win.isMaximized().then(setMaximized);
      });
      return () => {
        void unlistenPromise.then((fn) => fn());
      };
    }, [platform]);

    if (platform !== "windows") return null;

    const win = getCurrentWindow();
    return (
      <div className="ow-window-controls" data-tauri-drag-region>
        <button
          type="button"
          className="ow-window-controls__btn"
          data-testid="window-control-minimize"
          aria-label="Minimize"
          data-tauri-drag-region="false"
          onClick={() => void win.minimize()}
        >
          <Minus size={14} aria-hidden="true" />
        </button>
        <button
          type="button"
          className="ow-window-controls__btn"
          data-testid="window-control-maximize"
          aria-label={maximized ? "Restore" : "Maximize"}
          data-tauri-drag-region="false"
          onClick={() => void win.toggleMaximize()}
        >
          {maximized ? <RestoreGlyph /> : <MaximizeGlyph />}
        </button>
        <button
          type="button"
          className="ow-window-controls__btn ow-window-controls__btn--close"
          data-testid="window-control-close"
          aria-label="Close"
          data-tauri-drag-region="false"
          onClick={() => void win.close()}
        >
          <X size={14} aria-hidden="true" />
        </button>
      </div>
    );
  }
  ```

  Cluster wrapper carries `data-tauri-drag-region` so empty horizontal space between buttons remains draggable; each button opts OUT individually so its `onClick` fires.

- [ ] **Step 4: Wire into App.tsx.**

  Inside the titlebar (now inset in the content column from Task 1), append `<WindowControls platform={platform} />` after the back-arrow + title block. The cluster auto-aligns right via flex.

- [ ] **Step 5: App.css styles.**

  ```css
  .ow-window-controls {
    margin-left: auto;
    display: flex;
    align-items: center;
    height: 100%;
  }

  .ow-window-controls__btn {
    width: 46px;
    height: 100%;
    display: grid;
    place-items: center;
    background: transparent;
    border: none;
    color: var(--muted-foreground);
    cursor: pointer;
    transition: background 80ms ease;
  }

  .ow-window-controls__btn:hover {
    background: color-mix(in oklch, var(--foreground) 10%, transparent);
    color: var(--foreground);
  }

  .ow-window-controls__btn--close:hover {
    background: #e81123; /* Win 11 close-hover red */
    color: white;
  }
  ```

  Update `.ow-titlebar` to `display: flex; align-items: center; height: 36px;` so the controls cluster sits flush right.

- [ ] **Step 6: Extend tauri-shim with window IPC mocks.**

  In `tests/fixtures/tauri-shim.ts`, in the IPC handler, intercept `plugin:window|minimize` / `toggle_maximize` / `close` / `is_maximized` and record them on `window.__owWindowCalls`. `is_maximized` returns `false` by default (or read from `__owMaximized`).

- [ ] **Step 7: Run tests.**

  ```bash
  cd apps/tauri && pnpm tsc --noEmit && pnpm exec playwright test
  ```

  Expected: all green including new spec; existing specs unchanged.

- [ ] **Step 8: Commit.**

  ```bash
  git add apps/tauri/src/components/window-controls.tsx apps/tauri/src/App.tsx apps/tauri/src/App.css apps/tauri/tests/window-controls.spec.ts apps/tauri/tests/fixtures/tauri-shim.ts
  git commit -m "Tauri: WindowControls component (min/max/close) on Windows"
  ```

**Outcome ACs (Backlog).**
- `<WindowControls>` renders min/max/close on `Win32` platform; renders nothing on Mac.
- Clicking minimize / maximize / close invokes the matching window IPC (verified via shim recording).
- Maximize glyph (single rounded square) swaps to restore glyph (two overlapping squares) when the window is maximized; subscription via `getCurrentWindow().onResized()` (NOT global `listen("tauri://resize")`).
- Close button hover is the Win 11 red (`#e81123`); other buttons hover at theme grey.

---

### Task 4: Drop Windows OS chrome via Rust setup()

**Goal.** Remove the OS title bar on Windows by calling `window.set_decorations(false)` in `lib.rs::setup()` behind `#[cfg(target_os = "windows")]`. Mac's `Overlay` mode untouched. Verify Aero-snap still works (Tauri keeps `WS_THICKFRAME`).

**Files.**
- Modify: `apps/tauri/src-tauri/src/lib.rs`

**Steps.**

- [ ] **Step 1: Locate the setup() function and the main-window handle.**

  ```bash
  cd apps/tauri/src-tauri && grep -n "setup\|get_webview_window" src/lib.rs | head -10
  ```

- [ ] **Step 2: Add the decorations call.**

  Inside `setup()` (or wherever the main window is first accessed), after the window handle is acquired:

  ```rust
  #[cfg(target_os = "windows")]
  {
      if let Some(main) = app.get_webview_window("main") {
          let _ = main.set_decorations(false);
      }
  }
  ```

  No-op on Mac (the cfg gate compiles to nothing).

- [ ] **Step 3: Build + smoke on Windows.**

  Build a release artifact, install, verify:
  - OS title bar is gone — single dark titlebar visible.
  - Min/max/close (rendered by Task 3) are functional.
  - `Win+←` snaps the window left half. `Win+↑` maximizes. `Win+↓` restores.
  - Double-click on the inset titlebar toggles maximize (Tauri drag.js handles this when `data-tauri-drag-region` is set).
  - Win 11 rounded corners visible (DWM default).

- [ ] **Step 4: Verify Mac.**

  Build + run on Mac. Behavior should be identical to before:
  - Traffic-lights at top-left of sidebar (sidebar's `38px` padding-top from Task 1 keeps first item clear).
  - Drag region works on the inset titlebar.
  - WindowControls slot is empty.

- [ ] **Step 5: Commit.**

  ```bash
  git add apps/tauri/src-tauri/src/lib.rs
  git commit -m "Tauri: drop Windows OS decorations in setup() (cfg-gated)"
  ```

**Outcome ACs (Backlog).**
- `lib.rs::setup()` calls `set_decorations(false)` only on Windows.
- Smoke on Win 11: no OS title bar; Aero-snap works; rounded corners present.
- Smoke on Mac: behavior unchanged from pre-task — traffic-lights, drag, sidebar continuity all still right.

---

### Task 5: Visual polish — sidebar/titlebar bg unified, min-width safety

**Goal.** Match the sidebar and titlebar background colors so the layout reads as one continuous dark panel. Remove residual gaps. Verify min-width window doesn't overflow WindowControls onto content.

**Files.**
- Modify: `apps/tauri/src/App.css`

**Steps.**

- [ ] **Step 1: Audit current colors.**

  ```bash
  grep -n "background.*var\|rgb(0 0 0" apps/tauri/src/App.css | head -10
  ```

  Sidebar uses `rgb(0 0 0 / 0.15)` over `var(--background)`. Titlebar uses bare `var(--background)`. Decision: bump titlebar to the same `rgb(0 0 0 / 0.15)` overlay so they read identically.

- [ ] **Step 2: Update `.ow-titlebar` background.**

  ```css
  .ow-titlebar {
    background: rgb(0 0 0 / 0.15);
    border-bottom: 1px solid var(--border);
    /* ... */
  }
  ```

  Keep `border-bottom` so there's still a divider between titlebar and pane content.

- [ ] **Step 3: Min-width sanity at 600 px.**

  Ensure WindowControls cluster (3×46 = 138 px) + back-arrow + Settings title fit in the inset titlebar at the 600 px viewport used by the existing scroll test. Body (`width - sidebar`) at 600 = 420 px content column. Titlebar items: back (24) + title (~80) + WindowControls (138) = 242. Fits.

- [ ] **Step 4: Run the full Playwright suite.**

  ```bash
  cd apps/tauri && pnpm tsc --noEmit && pnpm exec playwright test
  ```

  Expected: still all green (visual-only change, no testid impact).

- [ ] **Step 5: Commit.**

  ```bash
  git add apps/tauri/src/App.css
  git commit -m "Tauri: unify sidebar + titlebar background (one continuous dark panel)"
  ```

**Outcome ACs (Backlog).**
- Sidebar and titlebar share the same computed background color (rgb overlay over `--background`).
- Existing scroll spec at 600×500 still passes (no overflow regression).

---

### Task 6: Update openwhisper-platform-gotchas with the Windows decorations finding

**Goal.** Document `decorations: false` + `WS_THICKFRAME` retention + the WindowControls IPC capability gotcha. Future-Claude needs to know: don't bypass capabilities; don't strip `WS_THICKFRAME`; do not use `titleBarStyle: Transparent` on Mac.

**Files.**
- Modify: `.claude/skills/openwhisper-platform-gotchas/SKILL.md`

**Steps.**

- [ ] **Step 1: Append a new entry under `## Windows`.**

  Title: "Custom titlebar requires `set_decorations(false)` AND four window IPCs in capabilities".

  Content:
  - **Symptom:** OS title bar still drawn / WindowControls clicks silently no-op.
  - **Root cause:** `decorations: false` only removes the OS chrome; without `core:window:allow-minimize|toggle-maximize|close|is-maximized` in capabilities, the IPC calls reject silently (same pattern as the Mac drag-region capability bug).
  - **Fix in tree:** `apps/tauri/src-tauri/capabilities/default.json` lists all four. `apps/tauri/src-tauri/src/lib.rs::setup()` calls `set_decorations(false)` behind `#[cfg(target_os = "windows")]`.
  - **Mac impact:** None — the cfg gate compiles to nothing on Mac. Mac stays on `Overlay`.
  - **Don't-do:**
    - Don't move `decorations: false` into `tauri.conf.json` — `app.windows[]` replace-not-merge in `tauri.dev.conf.json` means platform-conditional config has to be duplicated four times.
    - Don't strip `WS_THICKFRAME` manually thinking "decorations: false should kill it" — Tauri keeps it for Aero-snap; stripping breaks `Win+←/→`.
    - Don't switch Mac to `titleBarStyle: "Transparent"` to avoid the cfg gate — see existing macOS entry in this file (loses focus-loss blur).

- [ ] **Step 2: Commit.**

  ```bash
  git add .claude/skills/openwhisper-platform-gotchas/SKILL.md
  git commit -m "Skill: openwhisper-platform-gotchas — Windows custom titlebar capability gotcha"
  ```

**Outcome ACs (Backlog).**
- New entry in `openwhisper-platform-gotchas` under `## Windows` covering decorations + capabilities + WS_THICKFRAME.
- Don't-do section names the three anti-patterns (per-platform conf split, manual WS_THICKFRAME strip, Mac Transparent fallback).

---

## Reviewer + handoff

After Task 6 lands locally, dispatch the plan-document-reviewer subagent (with the Backlog enforcement addendum from `.claude/skills/writing-backlog-plans/references/plan-reviewer-addendum.md`) for a final pass. If green, hand the plan off via subagent-driven-development.
