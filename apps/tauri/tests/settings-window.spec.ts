import {
  emitDeviceState,
  emitTick,
  expect,
  test,
  waitForDeviceStateListener,
  waitForTickListener,
} from "./fixtures/tauri-shim";

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

test.describe("settings — audio pane", () => {
  test("renders device picker, KV stats, and meter container", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    await expect(page.getByRole("heading", { name: "Audio" })).toBeVisible();
    await expect(
      page.getByRole("combobox", { name: "Microphone device" }),
    ).toBeVisible();
    // KV stats — floor and sample rate are constants, peak starts at "—".
    // Match inside the KV block specifically so the intro paragraph's
    // "16 kHz" mention doesn't double-resolve the locator.
    const kv = page.locator(".ow-audio__kv");
    await expect(kv.getByText("-55 dBFS")).toBeVisible();
    await expect(kv.getByText("16 kHz")).toBeVisible();
  });

  test("does not auto-start preview; Start test button toggles it", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    // Mounting the pane no longer triggers a preview — the user has to
    // explicitly press the button. Wait a little to make sure the mount
    // effect has had a chance to run.
    await page.waitForTimeout(200);
    expect(
      await page.evaluate(
        () =>
          (window as unknown as { __owAudioPreviewStarts?: number })
            .__owAudioPreviewStarts ?? 0,
      ),
    ).toBe(0);
    await page.getByRole("button", { name: "Start test" }).click();
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owAudioPreviewStarts?: number })
              .__owAudioPreviewStarts ?? 0,
        ),
      )
      .toBe(1);
    await page.getByRole("button", { name: "Stop test" }).click();
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owAudioPreviewStops?: number })
              .__owAudioPreviewStops ?? 0,
        ),
      )
      .toBeGreaterThan(0);
  });

  test("unmount stops an in-flight test", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    await page.getByRole("button", { name: "Start test" }).click();
    await expect(page.getByRole("button", { name: "Stop test" })).toBeVisible();
    // Navigate away — pane unmounts, stop is invoked even without an
    // explicit click.
    await page.getByRole("tab", { name: "General" }).click();
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owAudioPreviewStops?: number })
              .__owAudioPreviewStops ?? 0,
        ),
      )
      .toBeGreaterThan(0);
  });

  test("changing the device persists; restarts preview only if testing", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    const select = page.getByRole("combobox", { name: "Microphone device" });
    // Idle change — should persist but NOT start the preview.
    await select.selectOption("AirPods Pro");
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owAudioLastSet?: string })
              .__owAudioLastSet,
        ),
      )
      .toBe("AirPods Pro");
    expect(
      await page.evaluate(
        () =>
          (window as unknown as { __owAudioPreviewStarts?: number })
            .__owAudioPreviewStarts ?? 0,
      ),
    ).toBe(0);
    // Start a test, then change again — that path SHOULD bounce the stream.
    await page.getByRole("button", { name: "Start test" }).click();
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owAudioPreviewStarts?: number })
              .__owAudioPreviewStarts ?? 0,
        ),
      )
      .toBe(1);
    await select.selectOption("MacBook Pro Microphone");
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owAudioPreviewStarts?: number })
              .__owAudioPreviewStarts ?? 0,
        ),
      )
      .toBe(2);
  });

  test("device picker + Test button are disabled while recording", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    // Simulate a recording in flight via the dictation tick.
    await waitForTickListener(page);
    await emitTick(page, {
      phase: 2,
      status: "recording",
      is_recording: true,
      level: 0.4,
    });
    await expect(
      page.getByRole("combobox", { name: "Microphone device" }),
    ).toBeDisabled();
    await expect(page.getByRole("button", { name: "Start test" })).toBeDisabled();
    // Stop recording — controls re-enable.
    await emitTick(page, {
      phase: 0,
      status: "idle",
      is_recording: false,
      level: 0,
    });
    await expect(
      page.getByRole("combobox", { name: "Microphone device" }),
    ).toBeEnabled();
    await expect(page.getByRole("button", { name: "Start test" })).toBeEnabled();
  });

  test("System default option clears the saved device name", async ({
    page,
  }) => {
    await page.goto("/");
    await page.evaluate(() => {
      (window as unknown as { __owAudioDevice?: string | null }).__owAudioDevice =
        "AirPods Pro";
    });
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    const select = page.getByRole("combobox", { name: "Microphone device" });
    await expect(select).toHaveValue("AirPods Pro");
    await select.selectOption("");
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owAudioLastSet?: string | null })
              .__owAudioLastSet,
        ),
      )
      .toBeNull();
  });

  test("disconnected mic flips picker to System default without clearing saved pref", async ({
    page,
  }) => {
    await page.goto("/");
    await page.evaluate(() => {
      (window as unknown as { __owAudioDevice?: string | null }).__owAudioDevice =
        "AirPods Pro";
    });
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    const select = page.getByRole("combobox", { name: "Microphone device" });
    await expect(select).toHaveValue("AirPods Pro");
    // AirPods leaves the device list. Picker shows System default; saved
    // preference stays in core (no audio_set_device call).
    await waitForDeviceStateListener(page);
    const setCountBefore = await page.evaluate(
      () =>
        (window as unknown as { __owAudioSetCount?: number })
          .__owAudioSetCount ?? 0,
    );
    await emitDeviceState(page, {
      devices: [{ name: "MacBook Pro Microphone", is_default: true }],
      selected_name: "AirPods Pro",
      selected_present: false,
      default_name: "MacBook Pro Microphone",
    });
    await expect(select).toHaveValue("");
    await expect(select).not.toContainText("disconnected");
    const setCountAfter = await page.evaluate(
      () =>
        (window as unknown as { __owAudioSetCount?: number })
          .__owAudioSetCount ?? 0,
    );
    expect(setCountAfter).toBe(setCountBefore);
  });

  test("reconnecting the saved device auto-rebinds the picker", async ({
    page,
  }) => {
    await page.goto("/");
    await page.evaluate(() => {
      (window as unknown as { __owAudioDevice?: string | null }).__owAudioDevice =
        "AirPods Pro";
    });
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    const select = page.getByRole("combobox", { name: "Microphone device" });
    await waitForDeviceStateListener(page);
    // Disconnect → picker shows System default.
    await emitDeviceState(page, {
      devices: [{ name: "MacBook Pro Microphone", is_default: true }],
      selected_name: "AirPods Pro",
      selected_present: false,
      default_name: "MacBook Pro Microphone",
    });
    await expect(select).toHaveValue("");
    // Reconnect — saved preference auto-rebinds the picker, no click.
    await emitDeviceState(page, {
      devices: [
        { name: "MacBook Pro Microphone", is_default: true },
        { name: "AirPods Pro", is_default: false },
      ],
      selected_name: "AirPods Pro",
      selected_present: true,
      default_name: "MacBook Pro Microphone",
    });
    await expect(select).toHaveValue("AirPods Pro");
  });

  test("actively picking System default while disconnected clears saved pref", async ({
    page,
  }) => {
    await page.goto("/");
    await page.evaluate(() => {
      (window as unknown as { __owAudioDevice?: string | null }).__owAudioDevice =
        "AirPods Pro";
    });
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    const select = page.getByRole("combobox", { name: "Microphone device" });
    await waitForDeviceStateListener(page);
    // Mic disconnects, picker effectively shows System default.
    await emitDeviceState(page, {
      devices: [{ name: "MacBook Pro Microphone", is_default: true }],
      selected_name: "AirPods Pro",
      selected_present: false,
      default_name: "MacBook Pro Microphone",
    });
    // User explicitly chooses System default → that's an intent override
    // and should clear the saved preference, so reconnect doesn't snap
    // back to AirPods.
    await select.selectOption("");
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owAudioLastSet?: string | null })
              .__owAudioLastSet,
        ),
      )
      .toBeNull();
  });

  test("(default) tag follows the live host default", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    const select = page.getByRole("combobox", { name: "Microphone device" });
    // Initial fixture has MacBook Pro Microphone as default.
    await expect(select).toContainText("MacBook Pro Microphone (default)");
    // Bluetooth headset connects, host default flips. Tag should follow.
    await waitForDeviceStateListener(page);
    await emitDeviceState(page, {
      devices: [
        { name: "MacBook Pro Microphone", is_default: false },
        { name: "AirPods Pro", is_default: true },
      ],
      selected_name: null,
      selected_present: true,
      default_name: "AirPods Pro",
    });
    await expect(select).toContainText("AirPods Pro (default)");
    await expect(select).not.toContainText("MacBook Pro Microphone (default)");
  });

  test("level ticks update the peak readout while testing", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    // Peak starts at "—" because the test isn't running yet.
    const peakRow = page.locator(".ow-audio__kv-row").nth(1);
    await expect(peakRow).toContainText("—");
    await page.getByRole("button", { name: "Start test" }).click();
    await waitForTickListener(page);
    await emitTick(page, { phase: 0, status: "idle", level: 0.6 });
    // Peak readout updates from the rolling window — value is dB-converted
    // from 0.6 → roughly -4.4 dBFS, and we display one decimal.
    await expect(peakRow).toContainText("dBFS");
    await expect(peakRow).not.toContainText("—");
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
