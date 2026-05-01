import {
  emitDeviceState,
  emitShowInFullscreenChanged,
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

  test("Launch at login Switch starts checked", async ({ page }) => {
    await page.goto("/");
    await openSettings(page);
    await expect(
      page.getByRole("switch", { name: "Launch at login" }),
    ).toBeChecked();
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

  test("sidebar Settings item opens settings", async ({ page }) => {
    await page.goto("/");
    await page.waitForSelector("text=OpenWhisper Dev");
    await expect(
      page.getByRole("heading", { name: "Settings" }),
    ).toBeHidden();
    await page.getByTestId("sidebar-item-settings").click();
    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
  });
});
