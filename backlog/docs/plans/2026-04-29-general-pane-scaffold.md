# Settings → General pane scaffold — implementation plan

**Backlog parent:** TASK-56
**Spec:** `backlog/docs/specs/2026-04-29-general-pane-scaffold.md`
**Date:** 2026-04-29

Each `### Task N:` heading maps 1:1 to a Backlog subtask `TASK-56.N`. Sequential — Task 1 installs the primitives the rest depend on.

**Dependency setup (done at plan-write time, not as a subtask):** TASK-54 gets `Dependencies: TASK-56` so the autostart wiring waits for the pane scaffold. TASK-55.6's plan markdown gets a one-line note pointing the executor at the new `GeneralPane` file.

---

### Task 1: Install shadcn primitives + align Switch tokens

**Goal.** Add `switch`, `toggle-group`, `field`, `separator` from the `@shadcn` registry, verify each conforms to the project's installed style (`radix-nova`), and override the Switch's checked-state background to the design's `--info` blue via a CSS-variable-based rule in `App.css`.

**Files.** `apps/tauri/components.json` (read-only confirm), `apps/tauri/src/components/ui/` (added by CLI), `apps/tauri/src/App.css`.

**Steps.**

1. From `apps/tauri/`, run:
   ```bash
   pnpm dlx shadcn@latest add switch toggle-group field separator
   ```
   Accept defaults — the CLI will write to `apps/tauri/src/components/ui/{switch,toggle-group,field,separator}.tsx` per `resolvedPaths.ui`.
2. Per the shadcn skill's "review added components" workflow step:
   - Read each new file. Verify imports use the project alias `@/lib/utils` (not raw paths).
   - Confirm no `"use client"` directives leaked in (project is `isRSC: false`).
   - Confirm icon imports, if any, use `lucide-react` (matches `iconLibrary: lucide`).
   - Spot-check that no Critical Rules violations are baked in (e.g. `data-icon` placement on icon-bearing variants).
3. **Record APIs that Task 2 consumes** — write the findings into a short "API notes" comment block at the top of Task 2's component file (or a comment in this plan, executor's choice). Specifically:
   - **Switch checked-state selector.** Read `apps/tauri/src/components/ui/switch.tsx`. Find the attribute the root element uses to identify itself (commonly `data-slot="switch"` in radix-nova; may differ). Record the verified selector — Task 1 step 4 below uses it; without verification the override is a silent no-op.
   - **Field row-orientation API.** Read `apps/tauri/src/components/ui/field.tsx`. Record whether `Field` accepts an `orientation` prop (and its accepted values), or whether horizontal layout is opt-in via a different prop / className. Task 2 references this verbatim instead of guessing.
   - **Section grouping.** Check whether `field.tsx` exports `FieldSet` + `FieldLegend`. If yes, Task 2 uses them for section headers (preferred — keeps the pane fully primitive-driven). If no, Task 2 falls back to the `<h3>` pattern in step 3 below.
4. Add the Switch checked-state override to `apps/tauri/src/App.css`. After the existing `@theme inline` block, using the **verified selector** from step 3:
   ```css
   /* Settings → General: design calls for info-blue Switch when on.
      Customization via CSS variable per shadcn customization.md, not per-instance className overrides.
      Selector confirmed against installed switch.tsx in step 3 above. */
   [data-slot="switch"][data-state="checked"] {
     background-color: var(--info);
   }
   ```
5. `pnpm tsc --noEmit` from `apps/tauri/` — clean.
6. `pnpm dev` boots without errors; existing panes (Audio, Shortcuts) render unchanged (smoke).

**Outcome ACs (Backlog).**

- Four shadcn components installed under `apps/tauri/src/components/ui/`: `switch.tsx`, `toggle-group.tsx`, `field.tsx`, `separator.tsx`.
- Each new file passes the shadcn skill's "review added components" checklist (correct alias, no leaked RSC directive, lucide icons if any).
- API notes recorded for Task 2: verified Switch `data-slot` selector, `Field` orientation API, and whether `FieldSet`/`FieldLegend` exports exist.
- App.css override uses the verified selector and paints any checked Switch in `var(--info)`.
- `pnpm tsc --noEmit` clean; existing panes render unchanged in dev.

---

### Task 2: Build the GeneralPane component

**Goal.** New file `apps/tauri/src/components/general-pane.tsx` that renders the three sections (Startup / Appearance / Updates) using the shadcn primitives from Task 1. Local state for the placeholder Switch and stub ToggleGroup; live `core_version` invoke for the Updates row.

**Files.** `apps/tauri/src/components/general-pane.tsx` (new).

**Steps.**

1. Create the file with `import { Switch } from "@/components/ui/switch"` and equivalents for `ToggleGroup` / `ToggleGroupItem`, `FieldGroup` / `Field` / `FieldLabel` / `FieldDescription`, `Separator`.
2. Import `invoke` from `@tauri-apps/api/core`. Use a `useEffect` to fetch `invoke<string>("core_version")` once on mount; store in `useState<string | null>`.
3. Section structure. **Use the API notes from Task 1 step 3** — substitute the verified `Field` orientation prop / className everywhere `<Field …horizontal…>` appears below. **If `FieldSet` + `FieldLegend` exist** (recorded as available in Task 1's notes), use them for each section header instead of the raw `<h3>` shown below — the sample falls back to `<h3>` only if those exports are absent in radix-nova.

   Preferred shape (when `FieldSet`/`FieldLegend` exist):
   ```tsx
   <FieldGroup>
     <FieldSet>
       <FieldLegend>Startup</FieldLegend>
       <Field {...horizontal-orientation-from-Task-1}>
         <FieldLabel htmlFor="launch-at-login">Launch at login</FieldLabel>
         <FieldDescription>
           OpenWhisper runs in the background, ready for your hotkey.
         </FieldDescription>
         <Switch
           id="launch-at-login"
           checked={launchAtLogin}
           onCheckedChange={setLaunchAtLogin}
         />
       </Field>
     </FieldSet>

     <Separator />

     <FieldSet>
       <FieldLegend>Appearance</FieldLegend>
       <Field {...horizontal-orientation-from-Task-1}>
         <FieldLabel>Theme</FieldLabel>
         <ToggleGroup type="single" value={theme} onValueChange={(v) => v && setTheme(v)}>
           <ToggleGroupItem value="system">System</ToggleGroupItem>
           <ToggleGroupItem value="light">Light</ToggleGroupItem>
           <ToggleGroupItem value="dark">Dark</ToggleGroupItem>
         </ToggleGroup>
       </Field>
     </FieldSet>

     <Separator />

     <FieldSet>
       <FieldLegend>Updates</FieldLegend>
       <Field {...horizontal-orientation-from-Task-1}>
         <FieldLabel>Current version</FieldLabel>
         <span className="font-mono text-sm">{version ?? "—"}</span>
       </Field>
     </FieldSet>
   </FieldGroup>
   ```

   Fallback (only when `FieldSet`/`FieldLegend` are not exported by `field.tsx`): replace each `<FieldSet>…<FieldLegend>X</FieldLegend>` pair with `<h3 className="text-xs font-mono uppercase tracking-wider text-muted-foreground">X</h3>` (layout-only `className`, semantic tokens — no raw colors).
4. Per shadcn styling rules:
   - **No `space-y-*`.** Use `<Separator />` between sections, plus `FieldGroup` for in-section spacing.
   - **No raw color overrides.** Section headers use `text-muted-foreground`, mono font via `font-mono` token, casing via `uppercase tracking-wider`.
   - **No manual `dark:` overrides.** The semantic tokens (`text-muted-foreground`) handle dark-mode automatically.
5. Default state: `launchAtLogin = true` (matches design default), `theme = "system"`.
6. `pnpm tsc --noEmit` clean.

**Outcome ACs (Backlog).**

- `general-pane.tsx` exports a `GeneralPane` component using only shadcn primitives + Tailwind layout classes.
- Three sections render: Startup with one Switch row, Appearance with one ToggleGroup row, Updates with the live current-version readout.
- No `space-y-*` / `space-x-*` classes; no raw color overrides on shadcn components; no `dark:` color modifiers.
- Local-state-only for placeholder rows; Updates row reads `core_version` via `invoke`.
- `pnpm tsc --noEmit` clean.

---

### Task 3: Wire GeneralPane into Settings.tsx

**Goal.** Replace the `PaneStub` for General with the real component, keeping all other panes untouched.

**Files.** `apps/tauri/src/Settings.tsx`.

**Steps.**

1. Add `import { GeneralPane } from "@/components/general-pane";` at the top of `Settings.tsx`.
2. At line 99, replace `{active === "general" && <PaneStub title="General" />}` with `{active === "general" && <GeneralPane />}`.
3. Leave `PaneStub` itself in the file (still used for the Models pane at line 101) — do not delete the helper.
4. `pnpm tsc --noEmit` clean.
5. `pnpm dev`: open Settings → General. Verify the three sections render with the design's section-header treatment and the Switch is info-blue when checked.

**Outcome ACs (Backlog).**

- `Settings.tsx` line 99 routes the General pane to the real `GeneralPane` component.
- `PaneStub` retained for the Models pane (line 101).
- Existing Audio + Shortcuts panes render unchanged.
- Manual smoke confirms the design treatment lands (info-blue Switch when on, mono section headers, three sections).

---

### Task 4: Playwright spec — section structure + landing pane

**Goal.** Cover the section structure and the landing-on-General default with Playwright. Existing tests for sidebar / landing remain green.

**Files.** `apps/tauri/tests/settings-window.spec.ts` (extend).

**Steps.**

1. Add to the existing `test.describe("settings view", ...)` block a new test "General pane renders Startup, Appearance, and Updates sections":
   ```ts
   test("General pane renders Startup, Appearance, and Updates sections", async ({ page }) => {
     await page.goto("/");
     await openSettings(page);
     await expect(page.getByText("Startup")).toBeVisible();
     await expect(page.getByText("Appearance")).toBeVisible();
     await expect(page.getByText("Updates")).toBeVisible();
     await expect(page.getByLabel("Launch at login")).toBeVisible();
     await expect(page.getByText("Theme")).toBeVisible();
     await expect(page.getByText("Current version")).toBeVisible();
   });
   ```
2. Add a test that the Theme ToggleGroup has the System option selected by default (use the `aria-pressed`/`data-state` attribute the radix `ToggleGroupItem` exposes).
3. Add a test that the Launch at login Switch starts checked (matches design default).
4. Verify the existing tests at the top of the file (`renders sidebar with all four panes`, `General is the landing pane`) still pass — the new pane should not break either.
5. `pnpm test:ui` green locally. If browsers missing: `pnpm exec playwright install chromium` then re-run.

**Outcome ACs (Backlog).**

- New test asserts the three section headers and three field labels render in the General pane.
- Theme ToggleGroup default-System assertion present.
- Launch-at-login Switch default-checked assertion present.
- Existing landing-on-General + sidebar tests still pass.
- `pnpm test:ui` green locally and on CI.

---

## Reviewer loop

After all 4 plan tasks have matching Backlog subtasks and TASK-54's dependency line is updated, dispatch the plan-document-reviewer agent with the standard plan-review criteria PLUS the verbatim Backlog-enforcement fragment from `.claude/skills/writing-backlog-plans/references/plan-reviewer-addendum.md` AND a shadcn-conformance check (no `space-*` classes in plan code samples, no raw color overrides, primitives are shadcn-installed not custom).

## Execution handoff

Sequential: 1 → 2 → 3 → 4. No parallelism; each task feeds the next. Status updates flow through `backlog task edit` per the cheatsheet. After landing, TASK-54 and TASK-55.6 each become row-wiring tasks against the now-real `GeneralPane`.

**TDD shape note.** Strict red-green-refactor doesn't apply: this is a UI scaffold using third-party primitives whose semantics aren't worth re-asserting. Task 4 lands the Playwright spec last, after the visual primitives are stable enough for assertions to be meaningful. Component-level unit tests are not added — the component composes shadcn primitives directly with no extracted logic to test in isolation.
