import { expect, test } from "./fixtures/tauri-shim";

// Settings is now an in-window route inside App, not a separate window.
// All specs mount the main App tree and navigate via ⌘, or the
// `ow_navigate` event (the same surface tray Preferences… uses in prod).

async function openSettings(page: import("@playwright/test").Page) {
  await page.waitForSelector("text=OpenWhisper Dev");
  await page.evaluate(() => window.__owEmit("ow_navigate", "settings"));
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
}

test.describe("settings view", () => {
  test("renders sidebar with all four panes", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await expect(page.getByRole("tab", { name: "General" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Audio" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Models" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Shortcuts" })).toBeVisible();
  });

  test("General is the landing pane", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await expect(page.getByRole("tab", { name: "General" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await expect(page.getByRole("heading", { name: "General" })).toBeVisible();
  });

  test("clicking a sidebar item activates the pane", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    await expect(page.getByRole("tab", { name: "Shortcuts" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await expect(page.getByRole("heading", { name: "Shortcuts" })).toBeVisible();
  });

  test("ArrowDown / ArrowUp cycle through panes", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    const general = page.getByRole("tab", { name: "General" });
    await general.focus();
    await page.keyboard.press("ArrowDown");
    await expect(page.getByRole("tab", { name: "Audio" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await page.keyboard.press("ArrowDown");
    await page.keyboard.press("ArrowDown");
    await expect(page.getByRole("tab", { name: "Shortcuts" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await page.keyboard.press("ArrowDown");
    await expect(page.getByRole("tab", { name: "General" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await page.keyboard.press("ArrowUp");
    await expect(page.getByRole("tab", { name: "Shortcuts" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  test("back arrow returns to main view", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("button", { name: "Back to main" }).click();
    await expect(
      page.getByRole("heading", { name: "Settings" }),
    ).toBeHidden();
    await expect(page.getByText("OpenWhisper Dev")).toBeVisible();
  });
});

test.describe("settings — shortcuts pane", () => {
  test("renders both rebindable rows", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    await expect(
      page.getByRole("button", { name: "Rebind Toggle dictation" }),
    ).toContainText("Right ⌘");
    await expect(
      page.getByRole("button", { name: "Rebind Cancel while recording" }),
    ).toContainText("Esc");
  });

  test("clicking the toggle chip starts capture for the toggle slot", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    const chip = page.getByRole("button", { name: "Rebind Toggle dictation" });
    await chip.click();
    await expect(chip).toHaveText("press keys…");
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owCaptureLastTarget?: string })
              .__owCaptureLastTarget,
        ),
      )
      .toBe("toggle");
  });

  test("clicking the cancel chip starts capture for the cancel slot", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    const chip = page.getByRole("button", {
      name: "Rebind Cancel while recording",
    });
    await chip.click();
    await expect(chip).toHaveText("press keys…");
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owCaptureLastTarget?: string })
              .__owCaptureLastTarget,
        ),
      )
      .toBe("cancel");
  });

  test("hotkey_captured updates only the targeted slot", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    const cancelChip = page.getByRole("button", {
      name: "Rebind Cancel while recording",
    });
    await cancelChip.click();
    await expect(cancelChip).toHaveText("press keys…");

    await page.evaluate(() =>
      window.__owEmit("hotkey_captured", {
        target: "cancel",
        config: { kind: "chord", code: "Q", mods: ["Ctrl"] },
      }),
    );

    await expect(cancelChip).toContainText("Ctrl");
    await expect(cancelChip).toContainText("Q");
    // Toggle row stays on its default.
    await expect(
      page.getByRole("button", { name: "Rebind Toggle dictation" }),
    ).toContainText("Right ⌘");
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owHotkeyLastTarget?: string })
              .__owHotkeyLastTarget,
        ),
      )
      .toBe("cancel");
  });

  test("Cancel exits capture mode without saving", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    const chip = page.getByRole("button", { name: "Rebind Toggle dictation" });
    await chip.click();
    await expect(chip).toHaveText("press keys…");

    await page.getByRole("button", { name: "Cancel", exact: true }).click();
    await expect(chip).toContainText("Right ⌘");
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owCaptureCancelCount?: number })
              .__owCaptureCancelCount ?? 0,
        ),
      )
      .toBeGreaterThan(0);
  });

  test("Reset to default invokes settings_reset_hotkey for the right slot", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    const cancelRow = page
      .locator(".ow-shortcuts__row")
      .filter({ hasText: "Cancel while recording" });
    await cancelRow.getByRole("button", { name: "Reset to default" }).click();
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owHotkeyLastTarget?: string })
              .__owHotkeyLastTarget,
        ),
      )
      .toBe("cancel");
  });

  test("starting one capture disables the other row", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    const toggleChip = page.getByRole("button", {
      name: "Rebind Toggle dictation",
    });
    const cancelChip = page.getByRole("button", {
      name: "Rebind Cancel while recording",
    });
    await toggleChip.click();
    await expect(toggleChip).toHaveText("press keys…");
    await expect(cancelChip).toBeDisabled();
  });
});

test.describe("main window — settings entry point", () => {
  test("⌘, switches to settings view", async ({ page }) => {
    await page.goto("/");
    await page.waitForSelector("text=OpenWhisper Dev");
    await page.keyboard.press("Meta+,");
    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  });

  test("ow_navigate event swaps the view", async ({ page }) => {
    await page.goto("/");
    await page.waitForSelector("text=OpenWhisper Dev");
    await page.evaluate(() => window.__owEmit("ow_navigate", "settings"));
    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
    await page.evaluate(() => window.__owEmit("ow_navigate", "main"));
    await expect(
      page.getByRole("heading", { name: "Settings" }),
    ).toBeHidden();
  });
});
