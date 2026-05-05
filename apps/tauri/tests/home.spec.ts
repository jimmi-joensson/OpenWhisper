import {
  emitTick,
  expect,
  test,
  waitForHotkeyStatusListener,
  waitForPermissionsStatusListener,
  waitForTickListener,
} from "./fixtures/tauri-shim";

test.describe("home pane", () => {
  test("renders hero with live hotkey hint", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("heading", { name: "Ready when you are" })).toBeVisible();
    // Mac default toggle = RightCommand modifier-tap → "Right ⌘".
    await expect(page.getByTestId("home-hotkey-hint")).toContainText("Right ⌘");
    await expect(page.getByTestId("home-app-icon")).toBeVisible();
  });

  test("hotkey hint updates when binding changes", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByTestId("home-hotkey-hint")).toContainText("Right ⌘");

    // Simulate a rebind to Right Shift via the hotkey_captured event.
    await page.evaluate(() => {
      window.__owEmit("hotkey_captured", {
        target: "toggle",
        config: { kind: "modifier-tap", code: "RightShift", mods: [] },
      });
    });
    await expect(page.getByTestId("home-hotkey-hint")).toContainText("Right ⇧");
  });

  test("latest-transcript row appears after finalize and replaces on next finalize", async ({
    page,
  }) => {
    await page.goto("/");
    await waitForTickListener(page);

    // No row before any dictation.
    await expect(page.getByTestId("home-latest-row")).toHaveCount(0);

    // Drive transcribing → done with first transcript.
    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, {
      phase: 4,
      status: "idle",
      transcript: "first utterance",
      confidence: 0.9,
    });

    const row = page.getByTestId("home-latest-row");
    await expect(row).toBeVisible();
    await expect(row).toContainText("first utterance");

    // Second finalize replaces — only one row, new text.
    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, {
      phase: 4,
      status: "idle",
      transcript: "second utterance",
      confidence: 0.85,
    });
    await expect(page.getByTestId("home-latest-row")).toHaveCount(1);
    await expect(page.getByTestId("home-latest-row")).toContainText("second utterance");
    await expect(page.getByTestId("home-latest-row")).not.toContainText("first utterance");
  });

  test("hover reveals copy button; click writes to clipboard", async ({ page, context }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"]);
    await page.goto("/");
    await waitForTickListener(page);
    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, {
      phase: 4,
      status: "idle",
      transcript: "hello world",
      confidence: 0.9,
    });

    const row = page.getByTestId("home-latest-row");
    await expect(row).toBeVisible();
    const copyBtn = page.getByTestId("home-latest-copy");
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
    await emitTick(page, { phase: 3, status: "transcribing" });
    await emitTick(page, {
      phase: 4,
      status: "idle",
      transcript: "fresh utterance",
      confidence: 0.9,
    });
    await expect(page.getByTestId("home-latest-row")).toContainText(/just now|0s/i);
  });

  test("mic permission banner renders above the hero", async ({ page }) => {
    await page.goto("/");
    await waitForPermissionsStatusListener(page);

    // Push a denied snapshot via the permissions_status event the hook listens to.
    await page.evaluate(() =>
      window.__owEmit("permissions_status", {
        mic_ok: false,
        mic_state: "denied",
        error: "Microphone access denied.",
      }),
    );

    const banner = page.getByTestId("mic-banner");
    const hero = page.getByRole("heading", { name: "Ready when you are" });
    await expect(banner).toBeVisible();
    const bannerBox = await banner.boundingBox();
    const heroBox = await hero.boundingBox();
    expect(bannerBox && heroBox && bannerBox.y < heroBox.y).toBeTruthy();
  });
});

test.describe("hotkey banner", () => {
  test("hidden when status ok, visible with error when not, retry invokes hotkey_retry", async ({
    page,
  }) => {
    await page.goto("/");
    await waitForHotkeyStatusListener(page);

    // Default state: ok=true was last emit (from the wait probe). No banner.
    await expect(page.getByTestId("hotkey-banner")).toHaveCount(0);

    // Failure surfaces the banner with the exact error text.
    await page.evaluate(() =>
      window.__owEmit("hotkey_status", {
        ok: false,
        error: "AX denied — grant Accessibility, then click Restart.",
      }),
    );
    const banner = page.getByTestId("hotkey-banner");
    await expect(banner).toBeVisible();
    await expect(banner).toContainText("AX denied");
    await expect(banner.getByRole("button", { name: "Restart" })).toBeVisible();

    // Retry click invokes hotkey_retry exactly once.
    await banner.getByRole("button", { name: "Restart" }).click();
    const retryCount = await page.evaluate(
      () => (window as unknown as { __owHotkeyRetryCount?: number }).__owHotkeyRetryCount ?? 0,
    );
    expect(retryCount).toBe(1);

    // Recovery clears the banner.
    await page.evaluate(() =>
      window.__owEmit("hotkey_status", { ok: true, error: "" }),
    );
    await expect(page.getByTestId("hotkey-banner")).toHaveCount(0);
  });
});

test.describe("mic permission banner", () => {
  test("hidden when authorized, visible when denied, recovers when authorized again", async ({
    page,
  }) => {
    await page.goto("/");
    await waitForPermissionsStatusListener(page);

    // Default state: probe emitted ok=true. No banner.
    await expect(page.getByTestId("mic-banner")).toHaveCount(0);

    // Denial surfaces the banner with the System Settings copy.
    await page.evaluate(() =>
      window.__owEmit("permissions_status", {
        mic_ok: false,
        mic_state: "denied",
        error:
          "Microphone access denied. Grant it in System Settings → Privacy & Security → Microphone.",
      }),
    );
    const banner = page.getByTestId("mic-banner");
    await expect(banner).toBeVisible();
    await expect(banner).toContainText("Microphone access denied");
    // Mic banner now exposes an Open Settings shortcut (parity with the
    // automation banner). Click should invoke `open_microphone_settings`
    // — the focus-event handler will re-probe and clear the banner when
    // the user comes back from System Settings with grant flipped.
    await expect(
      banner.getByRole("button", { name: "Open Settings" }),
    ).toBeVisible();

    // Recovery clears the banner (e.g., user grants access then reopens).
    await page.evaluate(() =>
      window.__owEmit("permissions_status", {
        mic_ok: true,
        mic_state: "authorized",
        error: "",
      }),
    );
    await expect(page.getByTestId("mic-banner")).toHaveCount(0);
  });
});

test.describe("recognizer-load banner", () => {
  test("recognizer load error renders banner on Home", async ({ page }) => {
    await page.goto("/");
    await waitForTickListener(page);

    // Boot baseline: no banner.
    await expect(page.getByTestId("recognizer-banner")).toHaveCount(0);

    // PHASE_ERROR with "recognizer load" prefix → banner.
    await emitTick(page, {
      phase: 5,
      status: "idle",
      can_toggle: true,
      error_message:
        "recognizer load failed: failed to read model file model.int8.onnx",
    });
    const banner = page.getByTestId("recognizer-banner");
    await expect(banner).toBeVisible();
    await expect(banner).toContainText("recognizer load failed");
  });
});
