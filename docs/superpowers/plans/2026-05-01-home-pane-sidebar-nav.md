# Home pane + sidebar nav (v1) — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Backlog parent:** TASK-65
**Spec:** `docs/superpowers/specs/2026-05-01-home-pane-sidebar-nav.md`
**Date:** 2026-05-01

**Goal.** Replace the debug-style `MainWindowShell` with a clean Home pane + outer sidebar nav (Home / Settings / Diagnostics). Existing debug dashboard relocates to a Diagnostics pane; Settings is unchanged internally and only loses its titlebar gear entry point.

**Architecture.** Pure React refactor — no new Rust commands, no changes to the dictation phase machine. All new state (latest transcription, current hotkey label) is derived from data the shell already receives via `dictation_tick` and `settings_get_hotkeys` / `hotkey_captured`. Outer sidebar is a sibling layout container in `App.tsx`; Settings keeps its inner sub-sidebar untouched. View enum widens from `"main"|"settings"` to `"home"|"settings"|"diagnostics"`.

**Tech stack.** React 18, Tauri 2.x, lucide-react (already a dep), Playwright 1.x. No new packages.

Each `### Task N:` heading maps 1:1 to a Backlog subtask `TASK-65.N`. Tasks 1–2 set up the shell; Task 3 unblocks Task 4's hero copy; Tasks 4–6 build the Home pane; Task 7 closes the test loop.

---

## File structure

| Action | File | Responsibility |
|---|---|---|
| Modify | `apps/tauri/src/App.tsx` | View enum widens to 3 routes; titlebar gear removed; outer `<SidebarNav>` + body switch |
| Create | `apps/tauri/src/components/sidebar-nav.tsx` | Outer 3-item nav (Home / Settings / Diagnostics) with lucide icons |
| Create | `apps/tauri/src/components/home-pane.tsx` | Banners-on-top + hero + optional latest-transcript row |
| Create | `apps/tauri/src/components/diagnostics-pane.tsx` | Existing debug content, lifted out of MainWindowShell |
| Create | `apps/tauri/src/components/transcript-row.tsx` | Single row, hover copy button |
| Create | `apps/tauri/src/lib/use-current-hotkey.ts` | Loads `settings_get_hotkeys`, listens `hotkey_captured`, returns label for `toggle` |
| Create | `apps/tauri/src/lib/use-last-transcription.ts` | Derives `{text, timestamp, confidence} \| null` from dictation phase transitions |
| Create | `apps/tauri/src/lib/hotkey-format.ts` | Lifts `configToChipKeys` / `modifierLabel` / etc. out of `Settings.tsx` for reuse |
| Modify | `apps/tauri/src/Settings.tsx` | Imports the lifted formatters from `hotkey-format.ts` (drop local copies) |
| Delete | `apps/tauri/src/components/main-window-shell.tsx` | Body splits into HomePane + DiagnosticsPane; the wrapper goes away |
| Modify | `apps/tauri/src/App.css` | Outer sidebar layout, home pane styles, transcript row styles |
| Replace | `apps/tauri/tests/main-window.spec.ts` | App-shell-level tests only (sidebar routing, scroll, drag) |
| Create | `apps/tauri/tests/home.spec.ts` | Hero + banners + live hint + latest row + hover copy |
| Create | `apps/tauri/tests/diagnostics.spec.ts` | Lifted assertions from old main-window.spec.ts (debug Card payload, FFI section) |

---

### Task 1: Outer sidebar nav + view-enum widening

**Goal.** Land the routing scaffolding before any new pane content. View enum becomes `"home"|"settings"|"diagnostics"` (`"main"` renamed to `"home"`). Titlebar gear icon removed; new `<SidebarNav>` renders a left rail with three items. Settings continues to render as today (its inner sub-sidebar is untouched). Diagnostics renders the existing `<MainWindowShell>` verbatim for now — the actual extraction happens in Task 2.

**Files.**
- Modify: `apps/tauri/src/App.tsx`
- Create: `apps/tauri/src/components/sidebar-nav.tsx`
- Modify: `apps/tauri/src/App.css`
- Modify: `apps/tauri/tests/main-window.spec.ts` (one new test for sidebar routing)

**Steps.**

- [ ] **Step 1: Write the failing routing test.**

  In `apps/tauri/tests/main-window.spec.ts`, add a new `test.describe("sidebar nav")` block with one test:

  ```ts
  test.describe("sidebar nav", () => {
    test("clicking sidebar items switches the visible pane", async ({ page }) => {
      await page.goto("/");
      // Default route is Home.
      await expect(page.getByTestId("sidebar-item-home")).toHaveAttribute("aria-current", "page");

      // Click Diagnostics — old debug content should still be visible (Task 1 leaves it wired to MainWindowShell).
      await page.getByTestId("sidebar-item-diagnostics").click();
      await expect(page.getByTestId("sidebar-item-diagnostics")).toHaveAttribute("aria-current", "page");
      await expect(page.getByText("Rust ↔ React FFI")).toBeVisible();

      // Click Settings — existing settings shell renders.
      await page.getByTestId("sidebar-item-settings").click();
      await expect(page.getByTestId("sidebar-item-settings")).toHaveAttribute("aria-current", "page");
      await expect(page.getByRole("tab", { name: "General" })).toBeVisible();

      // Click Home — sidebar marks Home active. (Hero content lands in Task 4; assert sidebar state only.)
      await page.getByTestId("sidebar-item-home").click();
      await expect(page.getByTestId("sidebar-item-home")).toHaveAttribute("aria-current", "page");
    });
  });
  ```

- [ ] **Step 2: Run the test to verify it fails.**

  ```bash
  cd apps/tauri && pnpm exec playwright test main-window.spec.ts --grep "sidebar nav"
  ```

  Expected: FAIL — `sidebar-item-home` testid does not exist yet.

- [ ] **Step 3: Create `sidebar-nav.tsx`.**

  ```tsx
  import { Home, Settings as SettingsIcon, Activity } from "lucide-react";

  export type Route = "home" | "settings" | "diagnostics";

  interface SidebarNavProps {
    active: Route;
    onSelect: (route: Route) => void;
  }

  const ITEMS: ReadonlyArray<{ id: Route; label: string; Icon: typeof Home }> = [
    { id: "home",        label: "Home",        Icon: Home },
    { id: "settings",    label: "Settings",    Icon: SettingsIcon },
    { id: "diagnostics", label: "Diagnostics", Icon: Activity },
  ];

  export function SidebarNav({ active, onSelect }: SidebarNavProps) {
    return (
      <nav className="ow-sidebar" aria-label="Primary">
        {ITEMS.map(({ id, label, Icon }) => (
          <button
            key={id}
            type="button"
            data-testid={`sidebar-item-${id}`}
            className={
              "ow-sidebar__item" +
              (active === id ? " ow-sidebar__item--active" : "")
            }
            aria-current={active === id ? "page" : undefined}
            onClick={() => onSelect(id)}
          >
            <Icon size={16} aria-hidden="true" />
            <span>{label}</span>
          </button>
        ))}
      </nav>
    );
  }
  ```

- [ ] **Step 4: Update `App.tsx`.**

  - Rename `View` to `Route`; widen to `"home" | "settings" | "diagnostics"`. Initial state `"home"`.
  - Update the `ow_navigate` event listener — accept `"main"|"settings"`; map `"main"` → `"home"` (preserve tray-menu compatibility without changing Rust).
  - Replace the titlebar gear button with a single grouped layout: `.ow-app__shell` wraps `<SidebarNav active={route} onSelect={setRoute} />` and the existing `.ow-app__body`.
  - Remove the `data-testid="open-settings-button"` gear branch from the titlebar entirely. The titlebar keeps the back-arrow only on the `settings` route (preserving the existing affordance for users who arrived via tray's Preferences…); on `home` and `diagnostics` it shows nothing but the drag region.
  - Body switch:
    ```tsx
    {route === "settings" && <SettingsShell />}
    {route === "diagnostics" && <MainWindowShell {...allProps} />}
    {route === "home" && <MainWindowShell {...allProps} />}  {/* placeholder until Task 4 */}
    ```
    Yes — Home renders MainWindowShell verbatim in Task 1. The Home pane content lands in Task 4. This intermediate state lets the routing test pass without forcing a big-bang refactor.
  - Keep the Cmd/Ctrl+, shortcut wired to `setRoute("settings")`.

- [ ] **Step 5: Update `App.css` — outer sidebar.**

  After the `.ow-app__body` rule:

  ```css
  .ow-app__shell {
    display: grid;
    grid-template-columns: 180px 1fr;
    flex: 1;
    min-height: 0;
  }

  .ow-sidebar {
    background: rgb(0 0 0 / 0.15);
    border-right: 1px solid var(--border);
    padding: 14px 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
  }

  .ow-sidebar__item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 8px 12px;
    border-radius: 6px;
    background: transparent;
    border: 1px solid transparent;
    color: var(--foreground);
    font-family: var(--font-sys);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
  }

  .ow-sidebar__item:hover {
    background: color-mix(in oklch, var(--foreground) 5%, transparent);
  }

  .ow-sidebar__item:focus-visible {
    outline: none;
    border-color: var(--info-border);
    box-shadow: 0 0 0 2px var(--info-bg);
  }

  .ow-sidebar__item--active,
  .ow-sidebar__item--active:hover {
    background: color-mix(in oklch, var(--info) 30%, transparent);
    font-weight: 500;
  }
  ```

  Update `.ow-app__body` to `min-height: 0; overflow-y: auto;` so it scrolls inside its grid cell rather than the page.

- [ ] **Step 6: Smoke-fix the existing tests.**

  The existing `main-window.spec.ts` tests reference `data-testid="open-settings-button"` indirectly via `text=OpenWhisper Dev` waits etc. Read each failing test; the only concrete dependency on the gear button itself is in `tests/settings-window.spec.ts`. Update that file:

  - Replace any `getByTestId("open-settings-button").click()` with `page.getByTestId("sidebar-item-settings").click()`.

  Existing main-window assertions ("renders header + all four cards", "FFI section shows mocked core_version", "debug Card reflects tick payload") still pass because Diagnostics renders the same `MainWindowShell` and the default route is now Home — but Home also currently renders `MainWindowShell` (the Task 1 placeholder), so the cards stay visible on `/` for now. Task 4 will move them off Home for good and the pane-scoped assertions migrate to `diagnostics.spec.ts` in Task 7.

- [ ] **Step 7: Run the suite — green.**

  ```bash
  cd apps/tauri && pnpm tsc --noEmit && pnpm exec playwright test
  ```

  Expected: all green, including the new sidebar test.

- [ ] **Step 8: Commit.**

  ```bash
  git add apps/tauri/src/App.tsx apps/tauri/src/components/sidebar-nav.tsx apps/tauri/src/App.css apps/tauri/tests/main-window.spec.ts apps/tauri/tests/settings-window.spec.ts
  git commit -m "Tauri: outer sidebar nav + Route enum (Home/Settings/Diagnostics)"
  ```

**Outcome ACs (Backlog).**
- New `<SidebarNav>` component renders Home / Settings / Diagnostics with lucide icons.
- View enum widens to `"home" | "settings" | "diagnostics"`; default is `"home"`.
- Titlebar gear button removed; sidebar is the sole entry to Settings (tray Preferences… still works because ow_navigate `"settings"` payload is preserved).
- New Playwright test "clicking sidebar items switches the visible pane" passes.
- Existing main-window + settings-window specs stay green.
- `pnpm tsc --noEmit` clean.

---

### Task 2: Extract DiagnosticsPane

**Goal.** Lift the debug content out of `MainWindowShell` into a new `<DiagnosticsPane>` component. The Home placeholder in Task 1 no longer renders the debug body; only the Diagnostics route does. Banners (mic / hotkey / recognizer-load) stay rendered on **both** routes for now — they migrate to Home-only in Task 4.

**Files.**
- Create: `apps/tauri/src/components/diagnostics-pane.tsx`
- Modify: `apps/tauri/src/App.tsx` (route → DiagnosticsPane vs. nothing)
- Modify: `apps/tauri/src/components/main-window-shell.tsx` (becomes a thin re-export until deleted in Task 4)

**Steps.**

- [ ] **Step 1: Create `diagnostics-pane.tsx` by copying `main-window-shell.tsx` verbatim.**

  Rename the exported function to `DiagnosticsPane`, keep the same props interface (export `MainWindowShellProps` as `DiagnosticsPaneProps`). Drop the `<h1>{title}</h1>` line — Diagnostics doesn't need a centered app-name title; the route is identified by the active sidebar item. Keep everything else: banners, the four Sections, transcript box, RecordButton, ModelLoadProgress.

- [ ] **Step 2: Update `App.tsx`.**

  Import `DiagnosticsPane` and switch the body:

  ```tsx
  {route === "settings" && <SettingsShell />}
  {route === "diagnostics" && <DiagnosticsPane {...diagnosticsProps} />}
  {route === "home" && (
    <div data-testid="home-placeholder" style={{ padding: 40, color: "var(--muted-foreground)" }}>
      Home pane — coming in Task 4.
    </div>
  )}
  ```

  The `diagnosticsProps` object is the same prop bag previously passed to `MainWindowShell`. The DEV-only `<DevPillControls>` continues to render only on `home` (it's a development affordance for the home route, not Diagnostics).

- [ ] **Step 3: Update existing main-window.spec.ts assertions for the new default route.**

  The default route is now `home` and Home renders the placeholder. Any existing assertions that look for "Rust ↔ React FFI", "Dictation debug", etc. on `/` must first navigate to Diagnostics:

  ```ts
  // Inside each impacted test, after page.goto("/"):
  await page.getByTestId("sidebar-item-diagnostics").click();
  ```

  Don't move them yet — they migrate to `diagnostics.spec.ts` in Task 7.

- [ ] **Step 4: Convert `main-window-shell.tsx` into a re-export.**

  ```tsx
  export { DiagnosticsPane as MainWindowShell, type DiagnosticsPaneProps as MainWindowShellProps } from "./diagnostics-pane";
  export type { Platform } from "./diagnostics-pane";
  ```

  This keeps any straggling imports working until Task 4 deletes the file.

- [ ] **Step 5: Run tests + tsc.**

  ```bash
  cd apps/tauri && pnpm tsc --noEmit && pnpm exec playwright test
  ```

  Expected: all green. The placeholder text "Home pane — coming in Task 4." should appear at `/`; debug cards reachable via Diagnostics sidebar item.

- [ ] **Step 6: Commit.**

  ```bash
  git add apps/tauri/src/components/diagnostics-pane.tsx apps/tauri/src/components/main-window-shell.tsx apps/tauri/src/App.tsx apps/tauri/tests/main-window.spec.ts
  git commit -m "Tauri: extract DiagnosticsPane; Home gets placeholder until Task 4"
  ```

**Outcome ACs (Backlog).**
- `DiagnosticsPane` component renders the previous debug dashboard (FFI / Dictation debug / Dictation mic→Parakeet / transcript / RecordButton).
- Diagnostics route shows the dashboard; Home route shows a placeholder (replaced in Task 4).
- `MainWindowShell` is now a re-export shim (deleted in Task 4).
- Playwright suite green; existing dashboard assertions navigate via Diagnostics sidebar.

---

### Task 3: `useCurrentHotkey` hook + `hotkey-format.ts`

**Goal.** Expose the user's current toggle binding to React as a formatted label string ("Right ⌘", "Ctrl + Space"), updating in real time when the user rebinds in Settings → Shortcuts. Also lift the chord-formatting helpers out of `Settings.tsx` into a shared module so HomePane and ShortcutsPane format identically.

**Files.**
- Create: `apps/tauri/src/lib/hotkey-format.ts` (lifts `configToChipKeys`, `modifierLabel`, `modShortLabel`, `codeLabel`)
- Create: `apps/tauri/src/lib/use-current-hotkey.ts`
- Modify: `apps/tauri/src/Settings.tsx` (drop the local helper copies; import from `hotkey-format.ts`)

**Steps.**

- [ ] **Step 1: Write the failing hook test.**

  Hook tests aren't part of the existing Playwright surface, but the format module is pure. Add a tiny unit test via Vitest if it's wired up; otherwise the integration assertion in Task 4 covers correctness. Check first:

  ```bash
  cd apps/tauri && grep -l "vitest" package.json
  ```

  If Vitest is not configured, skip the unit test and rely on the home.spec.ts test in Task 4 (which asserts the formatted hint on screen). Note in the commit message that format tests fold into the home spec.

- [ ] **Step 2: Create `hotkey-format.ts`.**

  Copy `configToChipKeys`, `modifierLabel`, `modShortLabel`, `codeLabel` from `Settings.tsx` lines ~765–832 into the new file. Export each. Add one new export:

  ```ts
  import type { HotkeyConfig } from "./use-global-hotkey";

  export function formatHotkeyLabel(config: HotkeyConfig | null): string {
    const keys = configToChipKeys(config);
    if (keys.length === 0) return "—";
    return keys.join(" + ");
  }
  ```

  `formatHotkeyLabel` is what HomePane consumes; `configToChipKeys` stays exported for ShortcutsPane.

- [ ] **Step 3: Create `use-current-hotkey.ts`.**

  ```ts
  import { useEffect, useState } from "react";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    type HotkeyCapturedPayload,
    type HotkeyConfig,
    type HotkeySettings,
    type HotkeyTarget,
  } from "./use-global-hotkey";

  export function useCurrentHotkey(target: HotkeyTarget = "toggle"): HotkeyConfig | null {
    const [config, setConfig] = useState<HotkeyConfig | null>(null);

    useEffect(() => {
      let alive = true;
      void invoke<HotkeySettings>("settings_get_hotkeys").then((s) => {
        if (alive) setConfig(s[target]);
      });

      let unlisten: UnlistenFn | undefined;
      void listen<HotkeyCapturedPayload>("hotkey_captured", (evt) => {
        if (evt.payload.target === target) setConfig(evt.payload.config);
      }).then((fn) => {
        unlisten = fn;
      });

      return () => {
        alive = false;
        unlisten?.();
      };
    }, [target]);

    return config;
  }
  ```

- [ ] **Step 4: Wire `Settings.tsx` to import from the new module.**

  Delete the local copies of `configToChipKeys`, `modifierLabel`, `modShortLabel`, `codeLabel` (lines ~765–832). Replace with:

  ```ts
  import { configToChipKeys } from "./lib/hotkey-format";
  ```

  `HotkeyChip` continues to use `configToChipKeys` as before.

- [ ] **Step 5: Run tsc + Playwright.**

  ```bash
  cd apps/tauri && pnpm tsc --noEmit && pnpm exec playwright test
  ```

  Expected: green. The Settings → Shortcuts pane behavior is unchanged; this is a pure refactor.

- [ ] **Step 6: Commit.**

  ```bash
  git add apps/tauri/src/lib/hotkey-format.ts apps/tauri/src/lib/use-current-hotkey.ts apps/tauri/src/Settings.tsx
  git commit -m "Tauri: lift hotkey-format helpers; add useCurrentHotkey hook"
  ```

**Outcome ACs (Backlog).**
- `hotkey-format.ts` exports `configToChipKeys`, `modifierLabel`, `modShortLabel`, `codeLabel`, `formatHotkeyLabel`.
- `useCurrentHotkey("toggle")` returns the live `HotkeyConfig`, updating on `hotkey_captured` events.
- `Settings.tsx` uses the lifted helpers (no local duplicate).
- Existing settings-window spec passes — Shortcut chip rendering unchanged.

---

### Task 4: HomePane skeleton (banners + hero)

**Goal.** Replace the Task 2 placeholder with a real Home pane: banners at the top, hero (icon + headline + live hotkey hint) below. No transcript row yet — that lands in Task 6. Banners migrate from `DiagnosticsPane` to HomePane: only Home shows them; Diagnostics drops them.

**Files.**
- Create: `apps/tauri/src/components/home-pane.tsx`
- Modify: `apps/tauri/src/components/diagnostics-pane.tsx` (drop banner JSX + props)
- Modify: `apps/tauri/src/App.tsx` (route → HomePane; pass banner props to HomePane only)
- Modify: `apps/tauri/src/App.css` (add `.ow-home` styles)
- Create: `apps/tauri/tests/home.spec.ts`

**Steps.**

- [ ] **Step 1: Write the failing home spec.**

  ```ts
  // apps/tauri/tests/home.spec.ts
  import { expect, test } from "./fixtures/tauri-shim";

  test.describe("home pane", () => {
    test("renders hero with live hotkey hint", async ({ page }) => {
      await page.goto("/");
      await expect(page.getByRole("heading", { name: "Ready when you are" })).toBeVisible();
      // Mac default: RightCommand modifier-tap → "Right ⌘".
      await expect(page.getByTestId("home-hotkey-hint")).toContainText("Right ⌘");
      await expect(page.getByTestId("home-app-icon")).toBeVisible();
    });

    test("hotkey hint updates when binding changes", async ({ page }) => {
      await page.goto("/");
      await expect(page.getByTestId("home-hotkey-hint")).toContainText("Right ⌘");

      // Simulate a rebind to Right ⇧ via the hotkey_captured event.
      await page.evaluate(() => {
        const detail = {
          target: "toggle",
          config: { kind: "modifier-tap", code: "RightShift", mods: [] },
        };
        // tauri event bus surrogate exposed by tauri-shim.ts
        (window as unknown as { __owEmit: (name: string, payload: unknown) => void }).__owEmit(
          "hotkey_captured",
          detail,
        );
      });
      await expect(page.getByTestId("home-hotkey-hint")).toContainText("Right ⇧");
    });

    test("mic permission banner renders above the hero", async ({ page }) => {
      // Stash a denied permissions snapshot before the page loads, then assert order.
      await page.addInitScript(() => {
        (window as unknown as { __owPermissions?: unknown }).__owPermissions = {
          mic_ok: false,
          accessibility_ok: true,
          error: "Microphone access denied.",
        };
      });
      await page.goto("/");
      const banner = page.getByTestId("mic-banner");
      const hero = page.getByRole("heading", { name: "Ready when you are" });
      await expect(banner).toBeVisible();
      const bannerBox = await banner.boundingBox();
      const heroBox = await hero.boundingBox();
      expect(bannerBox && heroBox && bannerBox.y < heroBox.y).toBeTruthy();
    });
  });
  ```

  If the shim doesn't already expose `__owEmit` for `hotkey_captured`, extend `tauri-shim.ts` to surface it — see existing `emitTick` for the pattern.

- [ ] **Step 2: Run the tests to verify they fail.**

  ```bash
  cd apps/tauri && pnpm exec playwright test home.spec.ts
  ```

  Expected: FAIL — HomePane doesn't exist yet.

- [ ] **Step 3: Create `home-pane.tsx`.**

  ```tsx
  import iconSrc from "../assets/icon-128.png";  // copied from src-tauri/icons/128x128.png in Step 4
  import { HealthBanner } from "./health-banner";
  import { useCurrentHotkey } from "../lib/use-current-hotkey";
  import { formatHotkeyLabel } from "../lib/hotkey-format";

  interface HomePaneProps {
    platform: "macos" | "windows";
    hotkeyError?: string | null;
    onHotkeyRetry?: () => void;
    micError?: string | null;
    recognizerError?: string | null;
    onRecognizerRetry?: () => void;
  }

  export function HomePane({
    platform,
    hotkeyError,
    onHotkeyRetry,
    micError,
    recognizerError,
    onRecognizerRetry,
  }: HomePaneProps) {
    const toggleConfig = useCurrentHotkey("toggle");
    const chord = formatHotkeyLabel(toggleConfig);

    return (
      <div className="ow-home">
        {hotkeyError && (
          <div data-testid="hotkey-banner" className="ow-home__banner">
            <HealthBanner message={hotkeyError} onRetry={onHotkeyRetry} retryLabel="Restart" />
          </div>
        )}
        {micError && (
          <div data-testid="mic-banner" className="ow-home__banner">
            <HealthBanner message={micError} />
          </div>
        )}
        {recognizerError && (
          <div data-testid="recognizer-banner" className="ow-home__banner">
            <HealthBanner
              message={recognizerError}
              onRetry={onRecognizerRetry}
              retryLabel="Retry"
            />
          </div>
        )}

        <section className="ow-home__hero">
          <img
            src={iconSrc}
            alt=""
            data-testid="home-app-icon"
            className="ow-home__icon"
            width={64}
            height={64}
          />
          <h1 className="ow-home__headline">Ready when you are</h1>
          <p className="ow-home__hint" data-testid="home-hotkey-hint">
            Press <kbd>{chord}</kbd> anywhere — speak — press again to stop.
          </p>
        </section>
      </div>
    );
  }
  ```

- [ ] **Step 4: Resolve the icon import path.**

  Two viable options — pick one based on what the existing Vite config supports:

  - **Option A (preferred):** Place a copy at `apps/tauri/src/assets/icon-128.png` (copy from `apps/tauri/src-tauri/icons/128x128.png`). Import via the standard React asset path: `import iconSrc from "../assets/icon-128.png";`. This keeps the Vite asset pipeline straightforward and avoids cross-tree imports. Note: the file is a copy, not a fork — write a one-line README in `apps/tauri/src/assets/` noting the source and that future icon changes must update both.
  - **Option B (only if Vite supports the alias):** `import iconSrc from "@root/src-tauri/icons/128x128.png?url";` after adding a `@root` alias in `vite.config.ts` pointing at the package root. More elegant but adds a config knob.

  Default: Option A.

- [ ] **Step 5: Update `DiagnosticsPane`.**

  Drop the banner JSX (lines that render `hotkey-banner` / `mic-banner` / `recognizer-banner` testids) and remove `hotkeyError` / `onHotkeyRetry` / `micError` / `recognizerError` / `onRecognizerRetry` from its props interface. Banners now live on Home only.

- [ ] **Step 6: Update `App.tsx`.**

  ```tsx
  {route === "home" && (
    <HomePane
      platform={platform}
      hotkeyError={hotkey.status && !hotkey.status.ok ? hotkey.status.error : null}
      onHotkeyRetry={() => void hotkey.retry()}
      micError={permissions.status && !permissions.status.mic_ok ? permissions.status.error : null}
      recognizerError={recognizerError}
    />
  )}
  ```

  The DEV `<DevPillControls>` keeps rendering alongside HomePane.

- [ ] **Step 7: Add `.ow-home` styles to `App.css`.**

  ```css
  .ow-home {
    padding: 24px 28px 32px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-width: 580px;
    margin: 0 auto;
    width: 100%;
  }

  .ow-home__banner { width: 100%; }

  .ow-home__hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    padding: 48px 16px 24px;
    gap: 12px;
  }

  .ow-home__icon {
    image-rendering: -webkit-optimize-contrast;
    /* No drop-shadow — bundle PNG already ships with its own shadow band. */
  }

  .ow-home__headline {
    font-size: 22px;
    font-weight: 600;
    letter-spacing: -0.01em;
    margin: 0;
  }

  .ow-home__hint {
    margin: 0;
    color: var(--muted-foreground);
    font-size: 13px;
  }

  .ow-home__hint kbd {
    font-family: var(--font-mono);
    font-size: 12px;
    padding: 1px 6px;
    border-radius: 4px;
    background: rgb(255 255 255 / 0.08);
    border: 1px solid var(--border);
  }
  ```

- [ ] **Step 8: Run tests.**

  ```bash
  cd apps/tauri && pnpm tsc --noEmit && pnpm exec playwright test
  ```

  Expected: home.spec.ts green; existing main-window assertions still pass via the Diagnostics navigation hop added in Task 2 step 3; settings-window spec untouched.

- [ ] **Step 9: Commit.**

  ```bash
  git add apps/tauri/src/components/home-pane.tsx apps/tauri/src/components/diagnostics-pane.tsx apps/tauri/src/App.tsx apps/tauri/src/App.css apps/tauri/src/assets/icon-128.png apps/tauri/src/assets/README.md apps/tauri/tests/home.spec.ts apps/tauri/tests/fixtures/tauri-shim.ts
  git commit -m "Tauri: HomePane skeleton (banners + hero with live hotkey hint)"
  ```

**Outcome ACs (Backlog).**
- `<HomePane>` renders banners (when set) above a centered hero (icon + "Ready when you are" + chord-bearing hint).
- Hotkey hint reads from `useCurrentHotkey("toggle")` and re-renders on `hotkey_captured` events.
- Banners no longer render on Diagnostics — only on Home.
- `home.spec.ts` (3 tests) green; default Mac binding renders "Right ⌘" in the hint.

---

### Task 5: `useLastTranscription` hook

**Goal.** Derive `{text, timestamp, confidence} | null` from the existing `dictation_tick` stream. The value flips to a non-null snapshot when the dictation lifecycle finalizes a non-empty transcript; subsequent finalized transcripts replace it. No persistence — restart resets to `null`.

**Definition of "finalize":** the canonical signal is `phase` transitioning from `PHASE_TRANSCRIBING` (3) to `PHASE_DONE` (4) or `PHASE_IDLE` (0) **with a non-empty `transcript`**. Use a `useRef<number>` to hold the previous phase between renders so the transition is detectable from `useEffect` without re-subscribing.

**Files.**
- Create: `apps/tauri/src/lib/use-last-transcription.ts`

**Steps.**

- [ ] **Step 1: Create `use-last-transcription.ts`.**

  Pure hook landing — no Playwright assertion this task. The row component lands in Task 6, where the integration tests live (row appears, row replaces, hover copy, relative time). Wiring the hook in isolation here keeps the implementation surface small and lets Task 6's tests assert hook + row + HomePane integration as one unit.

  ```ts
  import { useEffect, useRef, useState } from "react";
  import { listen } from "@tauri-apps/api/event";
  import {
    DICTATION_TICK_EVENT,
    PHASE_DONE,
    PHASE_IDLE,
    PHASE_TRANSCRIBING,
    type DictationTick,
  } from "./dictation";

  export interface LatestTranscription {
    text: string;
    timestamp: number;  // Date.now() at finalize
    confidence: number;
  }

  export function useLastTranscription(): LatestTranscription | null {
    const [latest, setLatest] = useState<LatestTranscription | null>(null);
    const prevPhaseRef = useRef<number>(PHASE_IDLE);

    useEffect(() => {
      const unlisten = listen<DictationTick>(DICTATION_TICK_EVENT, (event) => {
        const t = event.payload;
        const prev = prevPhaseRef.current;
        const finalizing =
          (prev === PHASE_TRANSCRIBING && (t.phase === PHASE_DONE || t.phase === PHASE_IDLE)) ||
          // PHASE_DONE arriving directly (skipped TRANSCRIBING for short utterances) also counts.
          (t.phase === PHASE_DONE && prev !== PHASE_DONE);
        if (finalizing && t.transcript.trim().length > 0) {
          setLatest({
            text: t.transcript,
            timestamp: Date.now(),
            confidence: t.confidence,
          });
        }
        prevPhaseRef.current = t.phase;
      });
      return () => {
        void unlisten.then((fn) => fn());
      };
    }, []);

    return latest;
  }
  ```

- [ ] **Step 3: Run tsc.**

  ```bash
  cd apps/tauri && pnpm tsc --noEmit
  ```

  Expected: clean. No Playwright run needed yet — UI exercise lands in Task 6.

- [ ] **Step 4: Commit.**

  ```bash
  git add apps/tauri/src/lib/use-last-transcription.ts
  git commit -m "Tauri: useLastTranscription hook (derives latest finalized utterance)"
  ```

**Outcome ACs (Backlog).**
- Hook subscribes to `dictation_tick` and tracks the previous phase via a ref.
- On phase transition into `PHASE_DONE` (or `PHASE_IDLE` from `PHASE_TRANSCRIBING`) with non-empty trimmed transcript, the hook updates state to `{text, timestamp: Date.now(), confidence}`.
- Subsequent finalizations replace state; no list, no growth.
- `pnpm tsc --noEmit` clean.

---

### Task 6: TranscriptRow + wire into HomePane

**Goal.** Single-row component shown below the hero when `useLastTranscription()` returns non-null. Hover reveals a copy-to-clipboard button. No row-body click action, no re-insert.

**Files.**
- Create: `apps/tauri/src/components/transcript-row.tsx`
- Modify: `apps/tauri/src/components/home-pane.tsx` (call hook, render row)
- Modify: `apps/tauri/src/App.css` (add `.ow-transcript-row` styles)
- Modify: `apps/tauri/tests/home.spec.ts` (add the row + copy tests deferred from Task 5)

**Steps.**

- [ ] **Step 1: Write the failing tests in `home.spec.ts`.**

  Add (alongside the tests deferred from Task 5):

  ```ts
  test("hover reveals copy button; click writes to clipboard", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.goto("/");
    await waitForTickListener(page);
    await emitTick(page, { phase: 4, status: "idle", transcript: "hello world", confidence: 0.9 });

    const row = page.getByTestId("home-latest-row");
    await expect(row).toBeVisible();
    const copyBtn = page.getByTestId("home-latest-copy");
    // The button is in the DOM but not yet "visible" via opacity until hover.
    await expect(copyBtn).toHaveCSS("opacity", "0");

    await row.hover();
    await expect(copyBtn).toHaveCSS("opacity", "1");
    await copyBtn.click();

    const clip = await page.evaluate(() => navigator.clipboard.readText());
    expect(clip).toBe("hello world");
  });

  test("relative time renders for fresh transcript", async ({ page }) => {
    await page.goto("/");
    await waitForTickListener(page);
    await emitTick(page, { phase: 4, status: "idle", transcript: "fresh utterance" });
    await expect(page.getByTestId("home-latest-row")).toContainText(/just now|0s/i);
  });
  ```

- [ ] **Step 2: Run tests to verify they fail.**

  Expected: FAIL — `home-latest-row` testid missing, copy button missing.

- [ ] **Step 3: Create `transcript-row.tsx`.**

  ```tsx
  import { useState } from "react";
  import { Copy, Check } from "lucide-react";

  interface TranscriptRowProps {
    text: string;
    timestamp: number;
  }

  export function TranscriptRow({ text, timestamp }: TranscriptRowProps) {
    const [copied, setCopied] = useState(false);

    const onCopy = async () => {
      try {
        await navigator.clipboard.writeText(text);
        setCopied(true);
        setTimeout(() => setCopied(false), 1200);
      } catch {
        // ignore — clipboard may be unavailable in some contexts; UI stays as-is.
      }
    };

    return (
      <div className="ow-transcript-row" data-testid="home-latest-row">
        <div className="ow-transcript-row__body">
          <p className="ow-transcript-row__text ow-selectable">{text}</p>
          <span className="ow-transcript-row__time">{formatRelativeTime(timestamp)}</span>
        </div>
        <button
          type="button"
          className="ow-transcript-row__copy"
          data-testid="home-latest-copy"
          onClick={() => void onCopy()}
          aria-label={copied ? "Copied" : "Copy transcript"}
        >
          {copied ? <Check size={14} aria-hidden="true" /> : <Copy size={14} aria-hidden="true" />}
        </button>
      </div>
    );
  }

  function formatRelativeTime(timestamp: number): string {
    const seconds = Math.max(0, Math.floor((Date.now() - timestamp) / 1000));
    if (seconds < 5) return "just now";
    if (seconds < 60) return `${seconds}s ago`;
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h ago`;
    return new Date(timestamp).toLocaleDateString();
  }
  ```

  **Note on relative time:** since this row is replaced (not persisted), the longest realistic age is the duration of the current session. For v1 we don't auto-update the label every tick — it's "just now" on render and only re-renders when the row replaces. That's a known limitation; if it surprises users, follow-up adds a 1-minute interval refresh.

- [ ] **Step 4: Wire into `HomePane`.**

  ```tsx
  import { useLastTranscription } from "../lib/use-last-transcription";
  import { TranscriptRow } from "./transcript-row";

  export function HomePane(props: HomePaneProps) {
    const latest = useLastTranscription();
    // ... existing hero ...
    return (
      <div className="ow-home">
        {/* banners */}
        <section className="ow-home__hero">{/* unchanged */}</section>
        {latest && <TranscriptRow text={latest.text} timestamp={latest.timestamp} />}
      </div>
    );
  }
  ```

- [ ] **Step 5: Add `.ow-transcript-row` styles to `App.css`.**

  ```css
  .ow-transcript-row {
    position: relative;
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 12px 14px;
    background: var(--card);
    border: 1px solid var(--border);
    border-radius: 8px;
  }

  .ow-transcript-row__body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .ow-transcript-row__text {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: var(--foreground);
    word-break: break-word;
  }

  .ow-transcript-row__time {
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--muted-foreground);
  }

  .ow-transcript-row__copy {
    flex-shrink: 0;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--muted-foreground);
    cursor: pointer;
    opacity: 0;
    transition: opacity 120ms ease, background-color 120ms ease;
  }

  .ow-transcript-row:hover .ow-transcript-row__copy,
  .ow-transcript-row__copy:focus-visible {
    opacity: 1;
  }

  .ow-transcript-row__copy:hover {
    background: color-mix(in oklch, var(--foreground) 8%, transparent);
    color: var(--foreground);
  }
  ```

- [ ] **Step 6: Run tests.**

  ```bash
  cd apps/tauri && pnpm tsc --noEmit && pnpm exec playwright test
  ```

  Expected: all green, including the deferred tests from Task 5.

- [ ] **Step 7: Commit.**

  ```bash
  git add apps/tauri/src/components/transcript-row.tsx apps/tauri/src/components/home-pane.tsx apps/tauri/src/App.css apps/tauri/tests/home.spec.ts
  git commit -m "Tauri: HomePane latest-transcript row with hover copy"
  ```

**Outcome ACs (Backlog).**
- `<TranscriptRow>` renders text + relative time + copy button.
- Copy button is `opacity: 0` until row is hovered or button is focus-visible; click writes the text to the clipboard and briefly shows a check icon.
- HomePane renders the row beneath the hero only when `useLastTranscription()` returns non-null.
- `home.spec.ts` covers: row appears after finalization, row replaces (not appends), hover reveals copy button, copy writes to clipboard, "just now" relative time renders for fresh transcripts.

---

### Task 7: Test refactor — clean split + delete `MainWindowShell`

**Goal.** Migrate the dashboard-specific assertions out of `main-window.spec.ts` into a new `diagnostics.spec.ts`, leaving `main-window.spec.ts` to assert app-shell-level behaviors only (sidebar nav, scroll, drag region). Delete the `main-window-shell.tsx` re-export shim now that nothing imports it.

**Files.**
- Modify: `apps/tauri/tests/main-window.spec.ts` (slim down to shell tests)
- Create: `apps/tauri/tests/diagnostics.spec.ts` (lifted dashboard assertions)
- Delete: `apps/tauri/src/components/main-window-shell.tsx` (re-export shim)
- Modify: any remaining import of `main-window-shell` (search-and-replace)

**Steps.**

- [ ] **Step 1: Lift dashboard tests to `diagnostics.spec.ts`.**

  Move these `test()` blocks from `main-window.spec.ts` to a new `apps/tauri/tests/diagnostics.spec.ts`:

  - "renders header + all four cards" → rename to "renders all four debug cards"
  - "FFI section shows mocked core_version"
  - "debug Card reflects tick payload"
  - "phase transitions drive RecordButton" (entire describe block)
  - "hidden when status ok, visible with error when not, retry invokes hotkey_retry" — but this banner now lives on Home, so this test moves to **`home.spec.ts`** instead. Move the assertion verbatim; just adjust the navigation step (no `sidebar-item-diagnostics` click).
  - "hidden when authorized, visible when denied, recovers when authorized again" — same: lives on Home now → home.spec.ts.
  - "appears on PHASE_ERROR with recognizer load prefix; transcribe-prefix errors stay in debug only" — Home pane shows the recognizer banner (load prefix); the transcribe-prefix half asserts the "last error" KV in the debug card on Diagnostics. **Split** into a Home-side and Diagnostics-side test:
    - Home: "recognizer load error renders banner on Home"
    - Diagnostics: "transcribe-prefix error appears in debug card last error row, no banner anywhere"
  - "chrome non-selectable, transcript + KV values selectable" — chrome part stays in main-window.spec.ts; transcript + KV selectable parts move to diagnostics.spec.ts (those elements now live there only).

  Each lifted test gets `await page.getByTestId("sidebar-item-diagnostics").click()` after `page.goto("/")`.

- [ ] **Step 2: Slim `main-window.spec.ts`.**

  Keep only:
  - The new "sidebar nav" describe block (from Task 1).
  - The two `scroll` tests (".ow-app__body scrolls when content overflows the viewport", "transcript Card visible without scroll at default 720x820"). For the latter, **navigate to Diagnostics first** (`await page.getByTestId("sidebar-item-diagnostics").click()`) and keep the assertion target on the dashboard transcript box. The Home pane has no transcript row by default (only after a finalized dictation), so asserting it would require an `emitTick` lead-in that doesn't belong in a scroll-geometry test.
  - The chrome non-selectable assertion (titlebar + sidebar are non-selectable; pane content is selectable in pane-specific specs).

- [ ] **Step 3: Delete the re-export shim.**

  ```bash
  cd apps/tauri && rm src/components/main-window-shell.tsx
  grep -rn "main-window-shell" src/ tests/ || true
  ```

  Replace any stragglers with imports from `./components/diagnostics-pane` (DiagnosticsPane) or `./components/home-pane` (HomePane) as appropriate. The `Platform` type re-exports from DiagnosticsPane; if any importer wants `Platform` independently, lift it to `lib/platform.ts` (small enum, no dependencies).

- [ ] **Step 4: Run the full suite.**

  ```bash
  cd apps/tauri && pnpm tsc --noEmit && pnpm exec playwright test
  ```

  Expected: all green. main-window.spec.ts now ~40 lines; home.spec.ts ~150; diagnostics.spec.ts ~150; settings-window.spec.ts unchanged.

- [ ] **Step 5: Verify route smoke in `pnpm dev`.**

  Per the openwhisper-platform-gotchas skill and CLAUDE.md, type-checking ≠ feature-checking for UI changes. Boot the dev shell and click each sidebar item; confirm:
  - Home shows hero + (after a real dictation cycle) a transcript row.
  - Settings shows the existing General/Audio/Models/Shortcuts sub-sidebar.
  - Diagnostics shows the FFI / Dictation debug / mic→Parakeet / transcript cards + RecordButton.
  - Cmd/Ctrl+, jumps from any route to Settings.
  - Tray "Preferences…" still opens Settings (ow_navigate "settings" path).

- [ ] **Step 6: Commit.**

  ```bash
  git add apps/tauri/tests/main-window.spec.ts apps/tauri/tests/home.spec.ts apps/tauri/tests/diagnostics.spec.ts apps/tauri/src/components/main-window-shell.tsx
  git commit -m "Tauri: split tests by route; delete MainWindowShell shim"
  ```

**Outcome ACs (Backlog).**
- `main-window.spec.ts` retains only app-shell assertions (sidebar routing, scroll, drag).
- `diagnostics.spec.ts` has the four-card / RecordButton / debug-card / KV-selectable assertions, each prefixed with a Diagnostics navigation click.
- `home.spec.ts` covers banners + hero + live hint + latest row + hover copy + relative time.
- `MainWindowShell` re-export shim deleted; no remaining imports.
- Dev-shell smoke confirms all three routes render and Cmd/Ctrl+, + tray Preferences… still work.
- `pnpm test:ui` green; `pnpm tsc --noEmit` clean.

---

## Reviewer + handoff

After Task 7 lands locally, dispatch the plan-document-reviewer subagent (with the Backlog enforcement addendum from `.claude/skills/writing-backlog-plans/references/plan-reviewer-addendum.md`) for a final pass. If green, hand the plan off via subagent-driven-development.
