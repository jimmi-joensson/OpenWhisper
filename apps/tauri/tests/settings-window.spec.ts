import {
  emitAskChanged,
  emitBtResumeDelayChanged,
  emitDeviceState,
  emitPauseAudioChanged,
  emitShowInFullscreenChanged,
  emitTick,
  expect,
  setMemoryStats,
  setPhysicalRamMb,
  test,
  waitForDeviceStateListener,
  waitForTickListener,
} from "./fixtures/tauri-shim";

const MB = 1024 * 1024;
const GB = 1024 * MB;

// Settings is now an in-window route inside App, not a separate window.
// All specs mount the main App tree and navigate via ⌘, or the
// `ow_navigate` event (the same surface tray Preferences… uses in prod).

async function openSettings(page: import("@playwright/test").Page) {
  await page.waitForSelector('[data-testid="sidebar-item-home"]');
  await page.evaluate(() => window.__owEmit("ow_navigate", "settings"));
  await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
}

test.describe("settings view", () => {
  test("renders sidebar with all five panes", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await expect(page.getByRole("tab", { name: "General" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Audio" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Models" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Shortcuts" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Dictionary" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Refine" })).toBeVisible();
    await expect(page.getByRole("tab", { name: "Modes" })).toBeVisible();
  });

  test("General is the landing pane", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await expect(page.getByRole("tab", { name: "General" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    // GeneralPane renders Launch at login as its first row — this fails
    // fast if the pane didn't mount.
    await expect(page.getByText("Launch at login")).toBeVisible();
  });

  test("General pane renders Startup, Appearance, and Updates sections", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await expect(page.getByRole("heading", { name: "Startup" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Appearance" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Updates" })).toBeVisible();
    await expect(page.getByText("Launch at login")).toBeVisible();
    await expect(page.getByText("Theme")).toBeVisible();
    await expect(page.getByText("Current version")).toBeVisible();
  });

  test("Theme ToggleGroup defaults to System", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await expect(
      page.getByRole("button", { name: "System" }),
    ).toHaveAttribute("aria-pressed", "true");
    await expect(
      page.getByRole("button", { name: "Light" }),
    ).toHaveAttribute("aria-pressed", "false");
    await expect(
      page.getByRole("button", { name: "Dark" }),
    ).toHaveAttribute("aria-pressed", "false");
  });

  test("Launch at login Switch starts unchecked when not registered", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await expect(
      page.getByRole("switch", { name: "Launch at login" }),
    ).not.toBeChecked();
  });

  test("Launch at login Switch hydrates from autostart isEnabled", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owAutostart?: boolean }).__owAutostart = true;
    });
    await page.goto("/");
    await openSettings(page);
    await expect(
      page.getByRole("switch", { name: "Launch at login" }),
    ).toBeChecked();
  });

  test("Toggling Launch at login on invokes autostart enable", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    const sw = page.getByRole("switch", { name: "Launch at login" });
    await expect(sw).not.toBeChecked();
    await sw.click();
    await expect(sw).toBeChecked();
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owAutostartLastSet?: boolean })
              .__owAutostartLastSet,
        ),
      )
      .toBe(true);
    expect(
      await page.evaluate(
        () =>
          (window as unknown as { __owAutostartEnableCount?: number })
            .__owAutostartEnableCount ?? 0,
      ),
    ).toBe(1);
  });

  test("Toggling Launch at login off invokes autostart disable", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owAutostart?: boolean }).__owAutostart = true;
    });
    await page.goto("/");
    await openSettings(page);
    const sw = page.getByRole("switch", { name: "Launch at login" });
    await expect(sw).toBeChecked();
    await sw.click();
    await expect(sw).not.toBeChecked();
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owAutostartLastSet?: boolean })
              .__owAutostartLastSet,
        ),
      )
      .toBe(false);
    expect(
      await page.evaluate(
        () =>
          (window as unknown as { __owAutostartDisableCount?: number })
            .__owAutostartDisableCount ?? 0,
      ),
    ).toBe(1);
  });

  test("Failed autostart enable reverts the Switch", async ({ page }) => {
    await page.addInitScript(() => {
      (
        window as unknown as { __owAutostartEnableShouldThrow?: boolean }
      ).__owAutostartEnableShouldThrow = true;
    });
    await page.goto("/");
    await openSettings(page);
    const sw = page.getByRole("switch", { name: "Launch at login" });
    await expect(sw).not.toBeChecked();
    await sw.click();
    // The optimistic flip lands first; the rejected promise then reverts
    // the state back to unchecked.
    await expect(sw).not.toBeChecked();
  });

  test("Show in fullscreen Switch reflects behavior_get on mount", async ({
    page,
  }) => {
    await page.goto("/");
    await page.evaluate(() => {
      (window as unknown as { __owShowInFullscreen?: boolean }).__owShowInFullscreen =
        true;
    });
    await openSettings(page);
    await expect(
      page.getByRole("switch", { name: "Show in fullscreen apps" }),
    ).toBeChecked();
  });

  test("Toggling the Show in fullscreen Switch invokes behavior_set with the new value", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    const sw = page.getByRole("switch", { name: "Show in fullscreen apps" });
    await expect(sw).not.toBeChecked();
    await sw.click();
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owShowInFullscreenLastSet?: boolean })
              .__owShowInFullscreenLastSet,
        ),
      )
      .toBe(true);
  });

  test("behavior_show_in_fullscreen_changed event updates the Show in fullscreen Switch", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    const sw = page.getByRole("switch", { name: "Show in fullscreen apps" });
    await expect(sw).not.toBeChecked();
    await emitShowInFullscreenChanged(page, true);
    await expect(sw).toBeChecked();
  });

  test("Pause audio Switch reflects behavior_get on mount", async ({ page }) => {
    await page.goto("/");
    await page.evaluate(() => {
      (window as unknown as { __owPauseAudio?: boolean }).__owPauseAudio = false;
    });
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    await expect(
      page.getByRole("switch", { name: "Pause audio during dictation" }),
    ).not.toBeChecked();
  });

  test("Toggling the Pause audio Switch invokes behavior_set with the new value", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    const sw = page.getByRole("switch", { name: "Pause audio during dictation" });
    // Default-on per the Rust-side default; the Switch starts checked.
    await expect(sw).toBeChecked();
    await sw.click();
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owPauseAudioLastSet?: boolean })
              .__owPauseAudioLastSet,
        ),
      )
      .toBe(false);
  });

  test("behavior_pause_audio_changed event updates the Pause audio Switch", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    const sw = page.getByRole("switch", { name: "Pause audio during dictation" });
    await expect(sw).toBeChecked();
    await emitPauseAudioChanged(page, false);
    await expect(sw).not.toBeChecked();
  });

  // BT resume delay slider is Windows-only — Mac uses adaptive
  // sample-rate polling for BT switchback. The four tests below all
  // fake `navigator.platform = "Win32"` via init script so they run
  // the Windows render path on a Mac CI box. The fifth test confirms
  // the slider is hidden on the default Mac platform.

  test("BT resume delay value label reflects behavior_get on mount", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, "platform", { value: "Win32" });
    });
    await page.goto("/");
    await page.evaluate(() => {
      (window as unknown as { __owBtResumeDelayMs?: number }).__owBtResumeDelayMs =
        2000;
    });
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    await expect(page.getByTestId("bt-resume-delay-value")).toHaveText("2.0s");
  });

  test("BT resume delay value label shows 'Off' when value is 0", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, "platform", { value: "Win32" });
    });
    await page.goto("/");
    await page.evaluate(() => {
      (window as unknown as { __owBtResumeDelayMs?: number }).__owBtResumeDelayMs = 0;
    });
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    await expect(page.getByTestId("bt-resume-delay-value")).toHaveText("Off");
  });

  test("behavior_bt_resume_delay_changed event updates the value label", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, "platform", { value: "Win32" });
    });
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    // Default is 5000 ms → always-decimal "5.0s" in the value label.
    await expect(page.getByTestId("bt-resume-delay-value")).toHaveText("5.0s");
    await emitBtResumeDelayChanged(page, 3000);
    await expect(page.getByTestId("bt-resume-delay-value")).toHaveText("3.0s");
  });

  test("BT resume delay slider is disabled when Pause audio is off", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      Object.defineProperty(navigator, "platform", { value: "Win32" });
    });
    await page.goto("/");
    await page.evaluate(() => {
      (window as unknown as { __owPauseAudio?: boolean }).__owPauseAudio = false;
    });
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    // base-ui renders a hidden `<input type="range">` inside the
    // Thumb that carries the actual disabled state. `getByRole`
    // skips disabled inputs from the accessibility tree under
    // Playwright's default rules, so we target the input via its
    // attribute selectors directly.
    await expect(
      page.locator(
        'input[type="range"][aria-label="Bluetooth resume delay"]',
      ),
    ).toBeDisabled();
  });

  test("BT resume delay slider is hidden on Mac", async ({ page }) => {
    // Playwright's Desktop Chrome preset reports navigator.platform =
    // "Win32" by default regardless of host OS, so we have to fake the
    // Mac platform explicitly here. On a real Mac runtime
    // navigator.platform = "MacIntel" (or similar), the SHOW_BT_RESUME_DELAY
    // module-level check evaluates false, and the Field never mounts.
    await page.addInitScript(() => {
      Object.defineProperty(navigator, "platform", { value: "MacIntel" });
    });
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    await expect(page.getByTestId("bt-resume-delay-value")).toHaveCount(0);
  });

  // Manual multi-monitor smoke (NOT covered here — these tests cover the
  // toggle UI half only):
  // 1. With Follow active screen ON, focus an app on display 1, then on
  //    display 2 — pill should jump bottom-center of display 2 within
  //    ~500 ms.
  // 2. Mid-recording switch: start recording on display 1, focus an app
  //    on display 2 — pill follows; level meter and SVG tween continue
  //    without flicker.
  // 3. With the toggle OFF, the pill stays on whichever monitor it last
  //    landed on through arbitrary focus changes.

  test("Follow active screen Switch defaults to ON", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await expect(
      page.getByRole("switch", { name: "Follow active screen" }),
    ).toBeChecked();
  });

  test("flipping Follow active screen invokes settings_set_pill_follow with false", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    const sw = page.getByRole("switch", { name: "Follow active screen" });
    await expect(sw).toBeChecked();
    await sw.click();
    await expect(sw).not.toBeChecked();
    const lastFollow = await page.evaluate(
      () =>
        (window as unknown as { __owPillLastFollow?: boolean })
          .__owPillLastFollow,
    );
    expect(lastFollow).toBe(false);
    const setCount = await page.evaluate(
      () =>
        (window as unknown as { __owPillSetCount?: number }).__owPillSetCount ??
        0,
    );
    expect(setCount).toBe(1);
  });

  test("Follow active screen hydrates from stored OFF value", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owPillFollow?: boolean }).__owPillFollow =
        false;
    });
    await page.goto("/");
    await openSettings(page);
    await expect(
      page.getByRole("switch", { name: "Follow active screen" }),
    ).not.toBeChecked();
  });

  test("Theme picker flips the dark/light class on <html>", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("button", { name: "Dark" }).click();
    expect(
      await page.evaluate(() =>
        document.documentElement.classList.contains("dark"),
      ),
    ).toBe(true);
    await page.getByRole("button", { name: "Light" }).click();
    expect(
      await page.evaluate(() =>
        document.documentElement.classList.contains("light"),
      ),
    ).toBe(true);
    expect(
      await page.evaluate(() =>
        document.documentElement.classList.contains("dark"),
      ),
    ).toBe(false);
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
    await expect(page.getByRole("tab", { name: "Dictionary" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await page.keyboard.press("ArrowDown");
    await expect(page.getByRole("tab", { name: "Refine" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await page.keyboard.press("ArrowDown");
    await expect(page.getByRole("tab", { name: "Modes" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await page.keyboard.press("ArrowDown");
    await expect(page.getByRole("tab", { name: "Ask", exact: true })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await page.keyboard.press("ArrowDown");
    await expect(page.getByRole("tab", { name: "General" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await page.keyboard.press("ArrowUp");
    await expect(page.getByRole("tab", { name: "Ask", exact: true })).toHaveAttribute(
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
    await expect(
      page.getByRole("heading", { name: "Ready when you are" }),
    ).toBeVisible();
  });
});

test.describe("settings — keep models warm", () => {
  test("toggle defaults OFF and flips to ON, persisting via settings_set_keep_models_warm", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);

    const sw = page.getByRole("switch", { name: "Keep models warm" });
    await expect(sw).toBeVisible();
    await expect(sw).not.toBeChecked();

    await sw.click();
    await expect(sw).toBeChecked();
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owKeepModelsWarmLast?: boolean })
              .__owKeepModelsWarmLast,
        ),
      )
      .toBe(true);
    expect(
      await page.evaluate(
        () =>
          (window as unknown as { __owKeepModelsWarmSetCount?: number })
            .__owKeepModelsWarmSetCount ?? 0,
      ),
    ).toBe(1);
  });

  test("toggle hydrates ON when settings_get_performance returns keep_models_warm=true", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owKeepModelsWarm?: boolean }).__owKeepModelsWarm =
        true;
    });
    await page.goto("/");
    await openSettings(page);

    await expect(
      page.getByRole("switch", { name: "Keep models warm" }),
    ).toBeChecked();
  });
});

test.describe("settings — shortcuts pane", () => {
  test("renders the four rows, Switch mode not set", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    await expect(page.locator(".ow-shortcuts__row")).toHaveCount(4);
    await expect(page.locator(".ow-shortcuts__row-title")).toHaveText([
      "Toggle dictation",
      "Cancel while recording",
      "Switch mode",
      /^Ask/,
    ]);
    await expect(
      page.getByRole("button", { name: "Rebind Toggle dictation" }),
    ).toContainText("Right ⌘");
    await expect(
      page.getByRole("button", { name: "Rebind Cancel while recording" }),
    ).toContainText("Esc");

    // The optional slot has no default: it says so, and there is
    // nothing to clear yet.
    const modeRow = page
      .locator(".ow-shortcuts__row")
      .filter({ hasText: "Switch mode" });
    await expect(modeRow).toContainText("Cycles through your Refine modes");
    await expect(modeRow).toContainText("Not set until you record a key");
    await expect(
      page.getByRole("button", { name: "Rebind Switch mode" }),
    ).toHaveText("Not set");
    await expect(modeRow.getByRole("button", { name: "Clear" })).toBeDisabled();
    await expect(
      modeRow.getByRole("button", { name: "Reset to default" }),
    ).toHaveCount(0);
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

  test("starting a capture on one row disables the others", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    const toggleChip = page.getByRole("button", {
      name: "Rebind Toggle dictation",
    });
    const cancelChip = page.getByRole("button", {
      name: "Rebind Cancel while recording",
    });
    const modeChip = page.getByRole("button", { name: "Rebind Switch mode" });

    await toggleChip.click();
    await expect(toggleChip).toHaveText("press keys…");
    await expect(cancelChip).toBeDisabled();
    await expect(modeChip).toBeDisabled();
    // The Ask row locks with them: its chip, and Clear next to Reset.
    await expect(page.getByRole("button", { name: "Rebind Ask" })).toBeDisabled();
    for (const clear of await page.getByRole("button", { name: "Clear" }).all()) {
      await expect(clear).toBeDisabled();
    }
    await page.getByRole("button", { name: "Cancel", exact: true }).click();

    // And from the optional row: the two dictation rows lock, chips
    // and reset controls alike.
    await modeChip.click();
    await expect(modeChip).toHaveText("press keys…");
    await expect(toggleChip).toBeDisabled();
    await expect(cancelChip).toBeDisabled();
    for (const reset of await page
      .getByRole("button", { name: "Reset to default" })
      .all()) {
      await expect(reset).toBeDisabled();
    }
  });

  test("recording a key for Switch mode saves it to the cycle-mode slot", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    const modeChip = page.getByRole("button", { name: "Rebind Switch mode" });
    await modeChip.click();
    await expect(modeChip).toHaveText("press keys…");
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owCaptureLastTarget?: string })
              .__owCaptureLastTarget,
        ),
      )
      .toBe("cycle-mode");

    // Real keys: the in-window capture path picks them up.
    await page.keyboard.press("Control+Alt+KeyM");

    await expect(modeChip).toContainText("Ctrl");
    await expect(modeChip).toContainText("Alt");
    await expect(modeChip).toContainText("M");
    await expect
      .poll(() =>
        page.evaluate(() => {
          const w = window as unknown as {
            __owHotkeyLastTarget?: string;
            __owHotkeyLastConfig?: unknown;
          };
          return { target: w.__owHotkeyLastTarget, config: w.__owHotkeyLastConfig };
        }),
      )
      .toEqual({
        target: "cycle-mode",
        config: { kind: "chord", code: "M", mods: ["Ctrl", "Alt"] },
      });

    // The dictation rows keep their bindings, and the new one can be
    // cleared now.
    await expect(
      page.getByRole("button", { name: "Rebind Toggle dictation" }),
    ).toContainText("Right ⌘");
    await expect(
      page.getByRole("button", { name: "Rebind Cancel while recording" }),
    ).toContainText("Esc");
    await expect(
      page
        .locator(".ow-shortcuts__row")
        .filter({ hasText: "Switch mode" })
        .getByRole("button", { name: "Clear" }),
    ).toBeEnabled();
  });

  test("a key captured outside the window lands on the Switch mode row", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    const modeChip = page.getByRole("button", { name: "Rebind Switch mode" });
    await modeChip.click();
    await expect(modeChip).toHaveText("press keys…");

    // The backend hook reports the slot by its kebab-case target.
    await page.evaluate(() =>
      window.__owEmit("hotkey_captured", {
        target: "cycle-mode",
        config: { kind: "modifier-tap", code: "RightOption", mods: [] },
      }),
    );

    await expect(modeChip).toContainText("Right ⌥");
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owHotkeyLastTarget?: string })
              .__owHotkeyLastTarget,
        ),
      )
      .toBe("cycle-mode");
  });

  test("Clear unsets the Switch mode shortcut", async ({ page }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owHotkeys?: unknown }).__owHotkeys = {
        toggle: { kind: "modifier-tap", code: "RightCommand", mods: [] },
        cancel: { kind: "chord", code: "Escape", mods: [] },
        cycle_mode: { kind: "chord", code: "M", mods: ["Ctrl", "Alt"] },
      };
    });
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    const modeChip = page.getByRole("button", { name: "Rebind Switch mode" });
    await expect(modeChip).toContainText("M");

    const clear = page
      .locator(".ow-shortcuts__row")
      .filter({ hasText: "Switch mode" })
      .getByRole("button", { name: "Clear" });
    await clear.click();

    await expect(modeChip).toHaveText("Not set");
    await expect(clear).toBeDisabled();
    await expect
      .poll(() =>
        page.evaluate(() => {
          const w = window as unknown as {
            __owHotkeyResetCount?: number;
            __owHotkeyLastTarget?: string;
            __owHotkeys?: Record<string, unknown>;
          };
          return {
            resets: w.__owHotkeyResetCount ?? 0,
            target: w.__owHotkeyLastTarget,
            stillSet: "cycle_mode" in (w.__owHotkeys ?? {}),
          };
        }),
      )
      .toEqual({ resets: 1, target: "cycle-mode", stillSet: false });
    // Clearing the optional slot leaves the dictation keys alone.
    await expect(
      page.getByRole("button", { name: "Rebind Toggle dictation" }),
    ).toContainText("Right ⌘");
  });

  // Playwright's Desktop Chrome reports navigator.platform = "Win32",
  // so the in-window keyboard fallback (Windows only) is live here.
  test("in-window fallback: the Switch mode key cycles once per press, and not after Clear", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owHotkeys?: unknown }).__owHotkeys = {
        toggle: { kind: "chord", code: "Space", mods: ["Ctrl"] },
        cancel: { kind: "chord", code: "Escape", mods: [] },
        cycle_mode: { kind: "chord", code: "M", mods: ["Ctrl", "Alt"] },
      };
    });
    const cycles = () =>
      page.evaluate(
        () =>
          (window as unknown as { __owModesCycleCount?: number })
            .__owModesCycleCount ?? 0,
      );
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    const modeChip = page.getByRole("button", { name: "Rebind Switch mode" });
    await expect(modeChip).toContainText("M");

    await page.keyboard.press("Control+Alt+KeyM");
    await expect.poll(cycles).toBe(1);
    // Other modifiers are another chord.
    await page.keyboard.press("Control+Shift+KeyM");
    await page.keyboard.press("Control+Alt+KeyM");
    await expect.poll(cycles).toBe(2);

    await page
      .locator(".ow-shortcuts__row")
      .filter({ hasText: "Switch mode" })
      .getByRole("button", { name: "Clear" })
      .click();
    await expect(modeChip).toHaveText("Not set");
    // Move focus off the now-disabled Clear button, so the press is
    // sure to reach the window listener and find nothing bound.
    await page.getByRole("heading", { name: "Shortcuts" }).click();
    await page.keyboard.press("Control+Alt+KeyM");
    // Let a stray invoke land before reading the count.
    await page.evaluate(() => new Promise((r) => setTimeout(r, 50)));
    expect(await cycles()).toBe(2);
  });

  test("a Switch mode key the backend could not install is reported on the pane", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owHotkeys?: unknown }).__owHotkeys = {
        toggle: { kind: "chord", code: "Space", mods: ["Ctrl"] },
        cancel: { kind: "chord", code: "Escape", mods: [] },
        cycle_mode: { kind: "chord", code: "Space", mods: ["Ctrl"] },
      };
    });
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    await expect(
      page.getByRole("button", { name: "Rebind Switch mode" }),
    ).toContainText("Space");
    await expect(page.locator(".ow-shortcuts__error")).toHaveCount(0);

    // `ok` stays true: dictation works, only the optional slot is out.
    await expect
      .poll(() =>
        page.evaluate(() =>
          window.__owEmit("hotkey_status", {
            ok: true,
            error: "",
            skipped: [
              {
                target: "cycle-mode",
                reason: "It uses the same keys as the toggle shortcut.",
              },
            ],
          }),
        ),
      )
      .toBeGreaterThan(0);
    await expect(page.locator(".ow-shortcuts__error")).toHaveText(
      "Switch mode is not active. It uses the same keys as the toggle shortcut.",
    );

    // The next install without a skipped slot takes the note away.
    await page.evaluate(() =>
      window.__owEmit("hotkey_status", { ok: true, error: "" }),
    );
    await expect(page.locator(".ow-shortcuts__error")).toHaveCount(0);
  });

  // ---- the Ask slot (TASK-97) ----

  const askRow = (page: import("@playwright/test").Page) =>
    page.locator(".ow-shortcuts__row").filter({ hasText: "ask a question out loud" });
  const askChip = (page: import("@playwright/test").Page) =>
    page.getByRole("button", { name: "Rebind Ask" });
  const seedAsk = (page: import("@playwright/test").Page, enabled: boolean) =>
    page.addInitScript((on) => {
      (window as unknown as { __owAskSettings?: unknown }).__owAskSettings = {
        enabled: on,
      };
    }, enabled);

  test("the Ask row shows the platform default and says what the key does", async ({
    page,
  }) => {
    await seedAsk(page, true);
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();

    const row = askRow(page);
    await expect(row.locator(".ow-shortcuts__row-title")).toHaveText("Ask");
    await expect(row).toContainText(
      "Press it, ask a question out loud, press again. The answer shows in a panel and is not typed anywhere.",
    );
    // The shim's default is the mac one, like its toggle default.
    await expect(askChip(page)).toContainText("Right ⌥");
    // Optional with a default: it can go back to it, and it can be
    // left without a key.
    await expect(row.getByRole("button", { name: "Reset to default" })).toBeEnabled();
    await expect(row.getByRole("button", { name: "Clear" })).toBeEnabled();
    // Ask is on: nothing says otherwise.
    await expect(page.getByTestId("shortcut-off-ask")).toHaveCount(0);
    await expect(page.getByTestId("shortcut-off-note-ask")).toHaveCount(0);
  });

  test("recording a key for Ask saves it to the ask slot", async ({ page }) => {
    await seedAsk(page, true);
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    const chip = askChip(page);
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
      .toBe("ask");

    // Real keys: the in-window capture path picks them up.
    await page.keyboard.press("Control+Shift+Space");

    await expect(chip).toContainText("Ctrl");
    await expect(chip).toContainText("Shift");
    await expect(chip).toContainText("Space");
    await expect
      .poll(() =>
        page.evaluate(() => {
          const w = window as unknown as {
            __owHotkeyLastTarget?: string;
            __owHotkeyLastConfig?: unknown;
          };
          return { target: w.__owHotkeyLastTarget, config: w.__owHotkeyLastConfig };
        }),
      )
      .toEqual({
        target: "ask",
        config: { kind: "chord", code: "Space", mods: ["Ctrl", "Shift"] },
      });
    // The dictation rows keep their bindings.
    await expect(
      page.getByRole("button", { name: "Rebind Toggle dictation" }),
    ).toContainText("Right ⌘");
  });

  test("Clear leaves Ask without a key and Reset to default brings it back", async ({
    page,
  }) => {
    await seedAsk(page, true);
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    const row = askRow(page);
    const chip = askChip(page);
    const clear = row.getByRole("button", { name: "Clear" });
    const reset = row.getByRole("button", { name: "Reset to default" });
    const slot = () =>
      page.evaluate(() => {
        const w = window as unknown as {
          __owHotkeys?: Record<string, unknown>;
          __owHotkeyClearCount?: number;
          __owHotkeyResetCount?: number;
          __owHotkeyLastTarget?: string;
        };
        return {
          ask: w.__owHotkeys?.ask ?? null,
          clears: w.__owHotkeyClearCount ?? 0,
          resets: w.__owHotkeyResetCount ?? 0,
          target: w.__owHotkeyLastTarget,
        };
      });
    await expect(chip).toContainText("Right ⌥");

    await clear.click();
    await expect(chip).toHaveText("Not set");
    await expect(clear).toBeDisabled();
    // Its own command: reset would have restored the default.
    await expect.poll(slot).toEqual({ ask: null, clears: 1, resets: 0, target: "ask" });

    await reset.click();
    await expect(chip).toContainText("Right ⌥");
    await expect(clear).toBeEnabled();
    await expect.poll(slot).toEqual({
      ask: { kind: "modifier-tap", code: "RightOption", mods: [] },
      clears: 1,
      resets: 1,
      target: "ask",
    });
    // Neither touched the other rows.
    await expect(
      page.getByRole("button", { name: "Rebind Toggle dictation" }),
    ).toContainText("Right ⌘");
    await expect(page.getByRole("button", { name: "Rebind Switch mode" })).toHaveText(
      "Not set",
    );
  });

  test("with Ask switched off the row says so, and follows the switch", async ({
    page,
  }) => {
    // Ask is off by default.
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();

    // The binding is kept and can still be changed; it is marked as
    // not active.
    await expect(askChip(page)).toContainText("Right ⌥");
    await expect(askChip(page)).toBeEnabled();
    await expect(page.getByTestId("shortcut-off-ask")).toHaveText("Off");
    await expect(page.getByTestId("shortcut-off-note-ask")).toHaveText(
      "Ask is switched off, so this key does nothing. The key is kept for when you switch Ask on.",
    );
    // Only the Ask row is marked.
    await expect(page.locator('[data-testid^="shortcut-off-"]')).toHaveCount(2);

    // Switched on elsewhere (the Ask pane, the CLI): the mark goes.
    await emitAskChanged(page, { enabled: true });
    await expect(page.getByTestId("shortcut-off-ask")).toHaveCount(0);
    await expect(page.getByTestId("shortcut-off-note-ask")).toHaveCount(0);
    await emitAskChanged(page, { enabled: false });
    await expect(page.getByTestId("shortcut-off-ask")).toHaveText("Off");
  });

  // Playwright's Desktop Chrome reports navigator.platform = "Win32",
  // so the in-window keyboard fallback (Windows only) is live here.
  test("in-window fallback: the Ask key toggles once per press, only while Ask is on", async ({
    page,
  }) => {
    await page.addInitScript(() => {
      (window as unknown as { __owHotkeys?: unknown }).__owHotkeys = {
        toggle: { kind: "chord", code: "Space", mods: ["Ctrl"] },
        cancel: { kind: "chord", code: "Escape", mods: [] },
        ask: { kind: "chord", code: "Space", mods: ["Ctrl", "Shift"] },
      };
    });
    await seedAsk(page, true);
    const toggles = () =>
      page.evaluate(
        () =>
          (
            (window as unknown as { __owAskCalls?: { cmd: string }[] }).__owAskCalls ?? []
          ).filter((c) => c.cmd === "ask_toggle").length,
      );
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    await expect(askChip(page)).toContainText("Space");
    await expect(page.getByTestId("shortcut-off-ask")).toHaveCount(0);

    await page.keyboard.press("Control+Shift+Space");
    await expect.poll(toggles).toBe(1);
    // The dictation chord is another chord.
    await page.keyboard.press("Control+Space");
    await page.keyboard.press("Control+Shift+Space");
    await expect.poll(toggles).toBe(2);

    // Ask switched off: the key is left alone.
    await emitAskChanged(page, { enabled: false });
    await expect(page.getByTestId("shortcut-off-ask")).toHaveText("Off");
    await page.getByRole("heading", { name: "Shortcuts" }).click();
    await page.keyboard.press("Control+Shift+Space");
    // Let a stray invoke land before reading the count.
    await page.evaluate(() => new Promise((r) => setTimeout(r, 50)));
    expect(await toggles()).toBe(2);
  });

  test("an Ask key the backend could not install is reported on the pane", async ({
    page,
  }) => {
    await seedAsk(page, true);
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Shortcuts" }).click();
    await expect(askChip(page)).toContainText("Right ⌥");

    // What Windows reports for a single-modifier binding.
    await expect
      .poll(() =>
        page.evaluate(() =>
          window.__owEmit("hotkey_status", {
            ok: true,
            error: "",
            skipped: [
              {
                target: "ask",
                reason:
                  "A single modifier key is unsupported on Windows. Record a key combination instead.",
              },
            ],
          }),
        ),
      )
      .toBeGreaterThan(0);
    await expect(page.locator(".ow-shortcuts__error")).toHaveText(
      "Ask is not active. A single modifier key is unsupported on Windows. Record a key combination instead.",
    );
  });
});

test.describe("settings — audio pane", () => {
  // The Microphone picker is a shadcn / BaseUI Select, not a native
  // <select>, so Playwright's `selectOption` / `toHaveValue` don't apply.
  // These helpers click the trigger, then a portaled option by its
  // accessible name. Matchers can be a substring or a regex; matching
  // is case-insensitive.
  function micTrigger(page: import("@playwright/test").Page) {
    return page.getByRole("combobox", { name: "Microphone device" });
  }

  async function pickMicOption(
    page: import("@playwright/test").Page,
    matcher: string | RegExp,
  ) {
    await micTrigger(page).click();
    // String matchers default to exact — needed because the default
    // option's accessible name concatenates the platform prefix with the
    // resolved device label ("Windows Default MacBook Pro Microphone"),
    // which would otherwise collide with the explicit "MacBook Pro
    // Microphone" device row on a substring match.
    const exact = typeof matcher === "string";
    await page.getByRole("option", { name: matcher, exact }).click();
  }

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
    // Idle change — should persist but NOT start the preview. We persist
    // the cpal device id; the visible option label is "AirPods Pro".
    await pickMicOption(page, "AirPods Pro");
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owAudioLastSet?: string })
              .__owAudioLastSet,
        ),
      )
      .toBe("airpods-pro");
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
    await pickMicOption(page, "MacBook Pro Microphone");
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

  test("default option clears the saved device id", async ({ page }) => {
    await fakePlatform(page, "MacIntel");
    await page.goto("/");
    await page.evaluate(() => {
      (window as unknown as { __owAudioDevice?: string | null }).__owAudioDevice =
        "airpods-pro";
    });
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    const select = micTrigger(page);
    // Trigger label reflects the persisted selection's items[].label.
    await expect(select).toContainText("AirPods Pro");
    // Pick the default row — its accessible name is the platform prefix
    // followed by the resolved device label, both inside the two-line
    // option layout.
    await pickMicOption(page, /macOS Default/);
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

  test("disconnected mic flips picker to default option without clearing saved pref", async ({
    page,
  }) => {
    await fakePlatform(page, "MacIntel");
    await page.goto("/");
    await page.evaluate(() => {
      (window as unknown as { __owAudioDevice?: string | null }).__owAudioDevice =
        "airpods-pro";
    });
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    const select = micTrigger(page);
    await expect(select).toContainText("AirPods Pro");
    // AirPods leaves the device list. Picker shows the platform default;
    // saved preference stays in core (no audio_set_device call).
    await waitForDeviceStateListener(page);
    const setCountBefore = await page.evaluate(
      () =>
        (window as unknown as { __owAudioSetCount?: number })
          .__owAudioSetCount ?? 0,
    );
    await emitDeviceState(page, {
      devices: [
        {
          id: "default-mic",
          label: "MacBook Pro Microphone",
          is_default: true,
        },
      ],
      selected_id: "airpods-pro",
      selected_present: false,
      default_label: "MacBook Pro Microphone",
    });
    await expect(select).toContainText("macOS Default");
    await expect(select).not.toContainText("AirPods Pro");
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
    await fakePlatform(page, "MacIntel");
    await page.goto("/");
    await page.evaluate(() => {
      (window as unknown as { __owAudioDevice?: string | null }).__owAudioDevice =
        "airpods-pro";
    });
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    const select = micTrigger(page);
    await waitForDeviceStateListener(page);
    // Disconnect → picker shows the platform default.
    await emitDeviceState(page, {
      devices: [
        {
          id: "default-mic",
          label: "MacBook Pro Microphone",
          is_default: true,
        },
      ],
      selected_id: "airpods-pro",
      selected_present: false,
      default_label: "MacBook Pro Microphone",
    });
    await expect(select).toContainText("macOS Default");
    // Reconnect — saved preference auto-rebinds the picker, no click.
    await emitDeviceState(page, {
      devices: [
        {
          id: "default-mic",
          label: "MacBook Pro Microphone",
          is_default: true,
        },
        { id: "airpods-pro", label: "AirPods Pro", is_default: false },
      ],
      selected_id: "airpods-pro",
      selected_present: true,
      default_label: "MacBook Pro Microphone",
    });
    await expect(select).toContainText("AirPods Pro");
    await expect(select).not.toContainText("macOS Default");
  });

  test("actively picking default option while disconnected clears saved pref", async ({
    page,
  }) => {
    await fakePlatform(page, "MacIntel");
    await page.goto("/");
    await page.evaluate(() => {
      (window as unknown as { __owAudioDevice?: string | null }).__owAudioDevice =
        "airpods-pro";
    });
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    await waitForDeviceStateListener(page);
    // Mic disconnects, picker effectively shows the platform default.
    await emitDeviceState(page, {
      devices: [
        {
          id: "default-mic",
          label: "MacBook Pro Microphone",
          is_default: true,
        },
      ],
      selected_id: "airpods-pro",
      selected_present: false,
      default_label: "MacBook Pro Microphone",
    });
    // User explicitly picks the default option → that's an intent
    // override and should clear the saved preference, so reconnect
    // doesn't snap back to AirPods.
    await pickMicOption(page, /macOS Default/);
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

  // Override `navigator.platform` so the platform-aware "<Platform> Default"
  // prefix is deterministic regardless of the host the suite runs on.
  // Mirrors the existing `navigator.platform` regex check in App.tsx /
  // use-global-hotkey.ts — we read it once at module load so the override
  // must fire before the SPA boots (init script, not a runtime mutation).
  async function fakePlatform(
    page: import("@playwright/test").Page,
    value: string,
  ) {
    await page.addInitScript((v) => {
      Object.defineProperty(navigator, "platform", {
        value: v,
        configurable: true,
      });
    }, value);
  }

  test("default option reveals the live default device label and follows host changes", async ({
    page,
  }) => {
    await fakePlatform(page, "MacIntel");
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    const select = page.getByRole("combobox", { name: "Microphone device" });
    // Initial fixture: MacBook Pro Microphone is the host default, so the
    // default row reads "macOS Default (MacBook Pro Microphone)".
    await expect(select).toContainText(
      "macOS Default (MacBook Pro Microphone)",
    );
    // The default device row shows just its bare label — no "(default)"
    // suffix, mirroring Discord's picker.
    await expect(select).not.toContainText("MacBook Pro Microphone (default)");
    // Bluetooth headset connects, host default flips. The default line
    // follows.
    await waitForDeviceStateListener(page);
    await emitDeviceState(page, {
      devices: [
        {
          id: "default-mic",
          label: "MacBook Pro Microphone",
          is_default: false,
        },
        { id: "airpods-pro", label: "AirPods Pro", is_default: true },
      ],
      selected_id: null,
      selected_present: true,
      default_label: "AirPods Pro",
    });
    await expect(select).toContainText("macOS Default (AirPods Pro)");
    await expect(select).not.toContainText(
      "macOS Default (MacBook Pro Microphone)",
    );
  });

  test("default option uses the Windows-specific prefix on Windows", async ({
    page,
  }) => {
    await fakePlatform(page, "Win32");
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Audio" }).click();
    const select = page.getByRole("combobox", { name: "Microphone device" });
    await expect(select).toContainText(
      "Windows Default (MacBook Pro Microphone)",
    );
    await expect(select).not.toContainText(
      "macOS Default (MacBook Pro Microphone)",
    );
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

test.describe("settings — models pane", () => {
  test("storage panel renders disk total + install count", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Models" }).click();

    const panel = page.getByTestId("settings-models-storage");
    await expect(panel).toBeVisible();
    await expect(
      page.getByTestId("settings-models-storage-total"),
    ).toHaveText("460 MB");
    await expect(
      page.getByTestId("settings-models-storage-count"),
    ).toContainText("on disk · 1 model installed");
    // Path + reveal button are intentionally not surfaced in v1 —
    // bundled Parakeet weights live in app resources, not the
    // user-data models folder.
    await expect(
      page.getByTestId("settings-models-storage-path"),
    ).toHaveCount(0);
    await expect(
      page.getByTestId("settings-models-storage-reveal"),
    ).toHaveCount(0);
  });

  test("budget bar renders at the top of the pane with physical readout", async ({
    page,
  }) => {
    await page.goto("/");
    await setPhysicalRamMb(page, 24576);
    await setMemoryStats(page, {
      system: {
        total_bytes: 24 * GB,
        used_bytes: 14 * GB,
        available_bytes: 10 * GB,
        swap_total_bytes: 4 * GB,
        swap_used_bytes: 0,
      },
      process: {
        rss_bytes: 1100 * MB,
        peak_rss_bytes: 1100 * MB,
        timestamp_unix_ms: 0,
      },
      models: [],
    });
    await openSettings(page);
    await page.getByRole("tab", { name: "Models" }).click();

    const bar = page.getByTestId("settings-models-budget-bar");
    await expect(bar).toBeVisible();
    await expect(
      page.getByTestId("settings-models-budget-physical"),
    ).toContainText("of 24.00 GB physical");
    // Parakeet is enabled-by-default → its segment renders.
    await expect(
      page.getByTestId("settings-models-budget-seg-parakeet-multilang"),
    ).toBeVisible();
  });

  test("footer caveat sits below the budget bar and links to Diagnostics", async ({
    page,
  }) => {
    await page.goto("/");
    await openSettings(page);
    await page.getByRole("tab", { name: "Models" }).click();

    const caveat = page.getByTestId("settings-models-budget-caveat");
    const link = page.getByTestId("settings-models-caveat-link");
    await expect(caveat).toBeVisible();
    await expect(link).toBeVisible();
    await expect(link).toContainText("Diagnostics → Memory");

    // Caveat sits between the budget bar and the model list in DOM order.
    const order = await page.evaluate(() => {
      const list = [
        "settings-models-budget-bar",
        "settings-models-budget-caveat",
        "settings-models-list",
      ];
      return list.map((id) => {
        const el = document.querySelector(`[data-testid="${id}"]`);
        return el ? Array.prototype.indexOf.call(el.parentElement!.children, el) : -1;
      });
    });
    expect(order[0]).toBeLessThan(order[1]);
    expect(order[1]).toBeLessThan(order[2]);

    // Click navigates to Diagnostics route via ow_navigate.
    await link.click();
    await expect(
      page.getByRole("heading", { name: "Diagnostics", exact: true }),
    ).toBeVisible();
  });
});

test.describe("main window — settings entry point", () => {
  test("⌘, switches to settings view", async ({ page }) => {
    await page.goto("/");
    await page.waitForSelector('[data-testid="sidebar-item-home"]');
    await page.keyboard.press("Meta+,");
    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  });

  test("ow_navigate event swaps the view", async ({ page }) => {
    await page.goto("/");
    await page.waitForSelector('[data-testid="sidebar-item-home"]');
    await page.evaluate(() => window.__owEmit("ow_navigate", "settings"));
    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
    await page.evaluate(() => window.__owEmit("ow_navigate", "main"));
    await expect(
      page.getByRole("heading", { name: "Settings" }),
    ).toBeHidden();
  });

  test("sidebar Settings item opens settings", async ({ page }) => {
    await page.goto("/");
    await page.waitForSelector('[data-testid="sidebar-item-home"]');
    await expect(
      page.getByRole("heading", { name: "Settings" }),
    ).toBeHidden();
    await page.getByTestId("sidebar-item-settings").click();
    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  });
});
