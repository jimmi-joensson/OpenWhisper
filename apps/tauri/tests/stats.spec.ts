import {
  emitSettingsStatsChanged,
  emitStatsChanged,
  expect,
  setStatsSummary,
  setUserWpm,
  test,
} from "./fixtures/tauri-shim";

test.describe("stats strip — Home", () => {
  test("empty state renders 0 / 0 / 0 / —", async ({ page }) => {
    await setStatsSummary(page, {
      words_today: 0,
      words_week: 0,
      words_all_time: 0,
      seconds_total: 0,
    });
    await page.goto("/");

    const strip = page.getByTestId("stats-strip");
    await expect(strip).toBeVisible();
    await expect(page.getByTestId("stats-cell-0").locator(".ow-stats-strip__value")).toHaveText("0");
    await expect(page.getByTestId("stats-cell-1").locator(".ow-stats-strip__value")).toHaveText("0");
    await expect(page.getByTestId("stats-cell-2").locator(".ow-stats-strip__value")).toHaveText("0");
    await expect(page.getByTestId("stats-cell-3").locator(".ow-stats-strip__value")).toHaveText("—");
    // Empty subcaption stays plain "vs. typing" (no wpm clause yet).
    await expect(page.getByTestId("stats-cell-3").locator(".ow-stats-strip__hint")).toHaveText("vs. typing");
  });

  test("strip refreshes when stats_changed fires after a dictation", async ({ page }) => {
    await setStatsSummary(page, {
      words_today: 0,
      words_week: 0,
      words_all_time: 0,
      seconds_total: 0,
    });
    await page.goto("/");
    await expect(page.getByTestId("stats-cell-0").locator(".ow-stats-strip__value")).toHaveText("0");

    // Simulate a dictation landing — swap the fixture and emit
    // stats_changed; the hook re-fetches and the strip re-renders.
    await setStatsSummary(page, {
      words_today: 16,
      words_week: 16,
      words_all_time: 16,
      // 16 words at 40 wpm = 24 s of typing; 7.4 s of speaking →
      // ~17 s saved, formatted as "0 min" by the round-to-min
      // formatter (under 1 min rounds to 0). Asserting the integer
      // word counts is the load-bearing part — the format function
      // is covered by Rust-side timing math, not this spec.
      seconds_total: 7.4,
    });
    await emitStatsChanged(page);

    await expect(page.getByTestId("stats-cell-0").locator(".ow-stats-strip__value")).toHaveText("16");
    await expect(page.getByTestId("stats-cell-1").locator(".ow-stats-strip__value")).toHaveText("16");
    await expect(page.getByTestId("stats-cell-2").locator(".ow-stats-strip__value")).toHaveText("16");
    // Populated subcaption now contains the live wpm clause.
    await expect(page.getByTestId("stats-cell-3").locator(".ow-stats-strip__hint")).toContainText("vs. typing at 40 wpm");
  });

  test("WPM change updates Time Saved subcaption immediately", async ({ page }) => {
    // setStatsSummary stashes a window var, but page.goto wipes it via
    // addInitScript firing on navigation. Goto first (default → 0
    // rows), then push the populated fixture + stats_changed so the
    // hook picks up a non-empty state.
    await page.goto("/");
    await setStatsSummary(page, {
      words_today: 100,
      words_week: 100,
      words_all_time: 100,
      seconds_total: 30,
    });
    await emitStatsChanged(page);
    await expect(page.getByTestId("stats-cell-3").locator(".ow-stats-strip__hint")).toContainText("vs. typing at 40 wpm");

    // Bump WPM to 80 via the settings_stats_changed event — useUserWpm
    // re-renders, StatsStrip recomputes Time Saved + the subcaption
    // text without remounting.
    await emitSettingsStatsChanged(page, 80);
    await expect(page.getByTestId("stats-cell-3").locator(".ow-stats-strip__hint")).toContainText("vs. typing at 80 wpm");
  });
});

test.describe("stats — Settings → General", () => {
  test("Reset stats button opens AlertDialog and confirms via stats_reset", async ({ page }) => {
    await setStatsSummary(page, {
      words_today: 16,
      words_week: 16,
      words_all_time: 16,
      seconds_total: 7.4,
    });
    await page.goto("/");

    // Open Settings via the sidebar item.
    await page.getByRole("button", { name: "Settings" }).click();
    // General pane is the default landing pane.
    const trigger = page.getByTestId("stats-reset-trigger");
    await expect(trigger).toBeVisible();
    await trigger.click();

    // AlertDialog title must be present (a11y requirement + spec copy).
    await expect(page.getByRole("heading", { name: "Reset all stats?" })).toBeVisible();

    // Confirm — invokes stats_reset.
    await page.getByTestId("stats-reset-confirm").click();
    const resetCount = await page.evaluate(
      () => (window as unknown as { __owStatsResetCount?: number }).__owStatsResetCount ?? 0,
    );
    expect(resetCount).toBe(1);
  });

  test("Typing speed input persists via settings_set_user_wpm and clamps out-of-range", async ({ page }) => {
    await setUserWpm(page, 40);
    await page.goto("/");
    await page.getByRole("button", { name: "Settings" }).click();

    const input = page.getByLabel("Typing speed in words per minute");
    await expect(input).toHaveValue("40");

    // In-range value goes through unchanged.
    await input.fill("75");
    await input.blur();
    const lastInRange = await page.evaluate(
      () => (window as unknown as { __owUserWpmLast?: number }).__owUserWpmLast,
    );
    expect(lastInRange).toBe(75);

    // Above-max clamps to 300 (the shim mirrors the Rust clamp).
    await input.fill("9999");
    await input.blur();
    const lastClamped = await page.evaluate(
      () => (window as unknown as { __owUserWpmLast?: number }).__owUserWpmLast,
    );
    expect(lastClamped).toBe(300);
  });
});
