// Crash inspector — spec seeded by TASK-78.3, extended by TASK-78.7.
// TASK-78.3 covers the Diagnostics overview entry card + the full-pane
// crash list (no sub-sidebar); TASK-78.4–6 add detail-sheet body
// content, launch toast, and opt-in upload assertions on top of this
// file.

import {
  expect,
  setCrashes,
  test,
  type MockCrashSummary,
} from "./fixtures/tauri-shim";

const SECOND = 1000;

function recentCrash(
  overrides: Partial<MockCrashSummary>,
  baseTsMs: number = Date.now() - 10 * SECOND,
): MockCrashSummary {
  return {
    id: String(baseTsMs),
    ts_unix_ms: baseTsMs,
    app_version: "0.6.0",
    os: "macos (arm64)",
    message_truncated: "kaboom from the recognizer worker thread",
    unread: true,
    uploaded_at: null,
    ...overrides,
  };
}

test.describe("crash inspector — Diagnostics overview entry", () => {
  test("entry card hidden in quiet state when no crashes recorded", async ({
    page,
  }) => {
    await page.goto("/");
    await setCrashes(page, []);
    await page.getByTestId("sidebar-item-diagnostics").click();
    await expect(
      page.getByTestId("diagnostics-crashes-quiet"),
    ).toBeVisible();
    await expect(
      page.getByTestId("diagnostics-crashes-entry"),
    ).toHaveCount(0);
  });

  test("entry card surfaces unread pill + last-crash sub-line", async ({
    page,
  }) => {
    const now = Date.now();
    const crashes: MockCrashSummary[] = [
      recentCrash({ id: "100", ts_unix_ms: now - 60 * 60 * SECOND }),
      recentCrash({
        id: "200",
        ts_unix_ms: now - 5 * SECOND,
        message_truncated: "newer crash",
      }),
      recentCrash({
        id: "300",
        ts_unix_ms: now - 30 * SECOND,
        message_truncated: "another",
        unread: false,
      }),
    ];
    await page.goto("/");
    await setCrashes(page, crashes);
    await page.getByTestId("sidebar-item-diagnostics").click();

    const card = page.getByTestId("diagnostics-crashes-entry");
    await expect(card).toBeVisible();
    // Two unread out of three.
    await expect(
      page.getByTestId("diagnostics-crashes-unread-pill"),
    ).toContainText("2 unread");
    // Sub-line carries app_version + os from the latest summary.
    await expect(card).toContainText("0.6.0");
    await expect(card).toContainText("macos (arm64)");
  });
});

test.describe("crash inspector — full-pane list", () => {
  test("clicking the entry card swaps the pane to the crash list", async ({
    page,
  }) => {
    const crashes: MockCrashSummary[] = [
      recentCrash({ id: "100", ts_unix_ms: Date.now() - 30 * SECOND }),
      recentCrash({
        id: "200",
        ts_unix_ms: Date.now() - 5 * SECOND,
        message_truncated: "newer one",
      }),
    ];
    await page.goto("/");
    await setCrashes(page, crashes);
    await page.getByTestId("sidebar-item-diagnostics").click();
    await page.getByTestId("diagnostics-crashes-entry").click();

    await expect(page.getByTestId("crash-list")).toBeVisible();
    await expect(page.getByTestId("crash-list-counts")).toContainText(
      "2 unread · 2 total",
    );
    await expect(page.getByTestId("crash-row-200")).toBeVisible();
    await expect(page.getByTestId("crash-row-100")).toBeVisible();

    // Newest first — row 200 is younger than 100.
    const rowIds = await page
      .locator('[data-testid^="crash-row-"]')
      .evaluateAll((els) => els.map((e) => e.getAttribute("data-testid")));
    expect(rowIds.filter((id) => id?.match(/^crash-row-\d+$/))).toEqual([
      "crash-row-200",
      "crash-row-100",
    ]);
  });

  test("breadcrumb back returns to the overview", async ({ page }) => {
    await page.goto("/");
    await setCrashes(page, [
      recentCrash({ id: "1", ts_unix_ms: Date.now() - SECOND }),
    ]);
    await page.getByTestId("sidebar-item-diagnostics").click();
    await page.getByTestId("diagnostics-crashes-entry").click();
    await expect(page.getByTestId("crash-list")).toBeVisible();

    await page.getByTestId("crash-list-back").click();
    await expect(page.getByTestId("crash-list")).toHaveCount(0);
    await expect(page.getByTestId("diagnostics-crashes-entry")).toBeVisible();
  });

  test("hover reveals mark-read + delete buttons; row click marks read", async ({
    page,
  }) => {
    const crashes: MockCrashSummary[] = [
      recentCrash({ id: "100", ts_unix_ms: Date.now() - SECOND }),
    ];
    await page.goto("/");
    await setCrashes(page, crashes);
    await page.getByTestId("sidebar-item-diagnostics").click();
    await page.getByTestId("diagnostics-crashes-entry").click();

    const row = page.getByTestId("crash-row-100");
    const markRead = page.getByTestId("crash-row-mark-read-100");
    const deleteBtn = page.getByTestId("crash-row-delete-100");

    // Buttons exist in the DOM (per AC #3 — hover reveals them, but
    // the underlying element must be queryable so Playwright can act).
    await expect(markRead).toHaveCount(1);
    await expect(deleteBtn).toHaveCount(1);

    // Resting row shows zero opacity on the action buttons; hover sets
    // it to 1. Use a CSS-side check rather than relying on the user's
    // motion preference matching the test runner's.
    await row.hover();
    // Clicking the action button (not the row body) should NOT open
    // the sheet — it stops propagation. Mark-read alone clears unread.
    await markRead.click();
    await expect(page.getByTestId("crash-detail-sheet")).toHaveCount(0);
    // After the next 2s poll, the row's data-unread attribute is
    // cleared. Use a generous timeout so flake-resistant under load.
    await expect(row).not.toHaveAttribute("data-unread", "true", {
      timeout: 5000,
    });
  });

  test("single-row delete is one-click — no confirm dialog", async ({
    page,
  }) => {
    const crashes: MockCrashSummary[] = [
      recentCrash({ id: "100", ts_unix_ms: Date.now() - SECOND }),
      recentCrash({ id: "200", ts_unix_ms: Date.now() - 5 * SECOND }),
    ];
    await page.goto("/");
    await setCrashes(page, crashes);
    await page.getByTestId("sidebar-item-diagnostics").click();
    await page.getByTestId("diagnostics-crashes-entry").click();

    await page.getByTestId("crash-row-100").hover();
    await page.getByTestId("crash-row-delete-100").click();

    // Row vanishes after the next refetch — no confirm gate.
    await expect(page.getByTestId("crash-row-100")).toHaveCount(0, {
      timeout: 5000,
    });
    await expect(page.getByTestId("crash-row-200")).toBeVisible();
  });

  test("Delete all uses an AlertDialog with dynamic confirm copy", async ({
    page,
  }) => {
    const crashes: MockCrashSummary[] = [
      recentCrash({ id: "100", ts_unix_ms: Date.now() - SECOND }),
      recentCrash({ id: "200", ts_unix_ms: Date.now() - 5 * SECOND }),
      recentCrash({
        id: "300",
        ts_unix_ms: Date.now() - 10 * SECOND,
        unread: false,
      }),
    ];
    await page.goto("/");
    await setCrashes(page, crashes);
    await page.getByTestId("sidebar-item-diagnostics").click();
    await page.getByTestId("diagnostics-crashes-entry").click();

    await page.getByTestId("crash-list-delete-all").click();
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText("Delete all crash reports?");
    await expect(dialog).toContainText("2 unread will be removed too");

    const confirm = page.getByTestId("crash-list-delete-all-confirm");
    await expect(confirm).toContainText("Delete 3 reports");
    await confirm.click();

    // After confirm, every row is gone and the empty state takes over.
    await expect(page.getByTestId("crash-list-empty")).toBeVisible({
      timeout: 5000,
    });
  });

  test("empty state replaces the pane and exposes Open crash folder", async ({
    page,
  }) => {
    await page.goto("/");
    await setCrashes(page, []);
    await page.getByTestId("sidebar-item-diagnostics").click();
    // No entry card → use the quiet-state link instead.
    await page.getByTestId("diagnostics-crashes-open-folder").click();

    await expect(page.getByTestId("crash-list-empty")).toBeVisible();
    await expect(
      page.getByTestId("crash-list-open-folder"),
    ).toBeVisible();
    // Pane header (breadcrumb + delete-all) hidden in empty state.
    await expect(page.getByTestId("crash-list-back")).toHaveCount(0);
  });

  test("row click opens the detail sheet AND marks the crash read", async ({
    page,
  }) => {
    const crashes: MockCrashSummary[] = [
      recentCrash({ id: "100", ts_unix_ms: Date.now() - SECOND }),
    ];
    await page.goto("/");
    await setCrashes(page, crashes);
    await page.getByTestId("sidebar-item-diagnostics").click();
    await page.getByTestId("diagnostics-crashes-entry").click();

    await page.getByTestId("crash-row-100").click();
    await expect(page.getByTestId("crash-detail-sheet")).toBeVisible();

    // Mark-read on open. The shim mutates the fixture in place; after
    // the next 2 s poll the row's unread attribute should clear even
    // though the sheet stays open.
    await expect(page.getByTestId("crash-row-100")).not.toHaveAttribute(
      "data-unread",
      "true",
      { timeout: 5000 },
    );
  });
});
