// DevTools panel — TanStack-style floating trigger + sheet hosting
// pill-state override and crash-simulation controls.
//
// The panel is gated behind `import.meta.env.DEV` at the React call
// site; Playwright runs against the Vite dev server, so it's always
// rendered in this test environment.

import { expect, test } from "./fixtures/tauri-shim";

test.describe("dev tools panel", () => {
  test("trigger opens the sheet and exposes pill + simulate-crash sections", async ({
    page,
  }) => {
    await page.goto("/");

    const trigger = page.getByTestId("devtools-trigger");
    await expect(trigger).toBeVisible();
    await trigger.click();

    const sheet = page.getByTestId("devtools-sheet");
    await expect(sheet).toBeVisible();
    await expect(sheet.getByText("Pill state")).toBeVisible();
    await expect(sheet.getByText("Simulate crash")).toBeVisible();
  });

  test("trigger persists across routes", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByTestId("devtools-trigger")).toBeVisible();

    await page.getByTestId("sidebar-item-diagnostics").click();
    await expect(page.getByTestId("devtools-trigger")).toBeVisible();

    await page.getByTestId("sidebar-item-settings").click();
    await expect(page.getByTestId("devtools-trigger")).toBeVisible();
  });

  test("Simulate crash invokes crashes_debug_trigger_panic", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByTestId("devtools-trigger").click();

    await page.getByTestId("devtools-simulate-crash").click();

    const calls = await page.evaluate(
      () =>
        (window as unknown as { __owCrashesDebugTriggerCount?: number })
          .__owCrashesDebugTriggerCount ?? 0,
    );
    expect(calls).toBeGreaterThanOrEqual(1);

    // Inline feedback surfaces (the shim returns ok, so the success
    // branch fires).
    await expect(
      page.getByTestId("devtools-simulate-crash-feedback"),
    ).toBeVisible();
  });

  test("pill status picker enables manual mode on first click", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByTestId("devtools-trigger").click();

    // Manual override defaults OFF — picking a status flips it ON.
    await page.getByTestId("devtools-pill-status-recording").click();

    // The Switch reflects manual mode. base-ui Switch surfaces
    // `aria-checked` on the root.
    await expect(page.getByTestId("devtools-pill-manual")).toHaveAttribute(
      "aria-checked",
      "true",
    );
  });
});
