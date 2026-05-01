import {
  emitTick,
  expect,
  test,
  waitForHotkeyStatusListener,
  waitForPermissionsStatusListener,
  waitForTickListener,
} from "./fixtures/tauri-shim";

test.describe("main window", () => {
  test("renders header + all four cards", async ({ page }) => {
    await page.goto("/");
    await page.waitForSelector("text=OpenWhisper Dev");

    await expect(page.getByText("Rust ↔ React FFI")).toBeVisible();
    await expect(page.getByText("Dictation debug")).toBeVisible();
    await expect(page.getByText("Dictation (mic → Rust core → Parakeet)")).toBeVisible();
    await expect(page.getByText("transcript", { exact: true })).toBeVisible();
  });

  test("FFI section shows mocked core_version", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByText("0.1.0-test")).toBeVisible();
  });

  test("debug Card reflects tick payload", async ({ page }) => {
    await page.goto("/");
    await waitForTickListener(page);
    await emitTick(page, {
      phase: 2,
      status: "recording",
      is_recording: true,
      level: 0.4321,
      can_toggle: true,
    });

    const debugCard = page
      .locator("div", { has: page.getByText("Dictation debug", { exact: true }) })
      .first();
    await expect(debugCard.getByText("2 (recording)")).toBeVisible();
    await expect(debugCard.getByText("0.4321")).toBeVisible();
  });
});

test.describe("scroll", () => {
  test(".ow-app__body scrolls when content overflows the viewport", async ({ page }) => {
    await page.setViewportSize({ width: 600, height: 500 });
    await page.goto("/");
    await page.waitForSelector("text=OpenWhisper Dev");

    // Window titlebar strip stays fixed; scroll happens inside
    // `.ow-app__body` so the strip never scrolls out of view.
    const probe = await page.evaluate(() => {
      const body = document.querySelector(".ow-app__body") as HTMLElement;
      return {
        clientH: body.clientHeight,
        scrollH: body.scrollHeight,
        overflowY: getComputedStyle(body).overflowY,
      };
    });

    expect(probe.overflowY).toBe("auto");
    expect(probe.scrollH).toBeGreaterThan(probe.clientH);

    const scrolled = await page.evaluate(() => {
      const body = document.querySelector(".ow-app__body") as HTMLElement;
      body.scrollTop = body.scrollHeight;
      return body.scrollTop;
    });
    expect(scrolled).toBeGreaterThan(0);

    await expect(page.getByText("transcript", { exact: true })).toBeInViewport();
  });

  test("transcript Card visible without scroll at default 720x820", async ({ page }) => {
    await page.setViewportSize({ width: 720, height: 820 });
    await page.goto("/");
    await page.waitForSelector("text=OpenWhisper Dev");
    await expect(page.getByText("transcript", { exact: true })).toBeInViewport();
  });
});

test.describe("phase transitions drive RecordButton", () => {
  test("idle → loading → recording → transcribing → idle", async ({ page }) => {
    await page.goto("/");
    await waitForTickListener(page);

    // idle: "Record"
    await emitTick(page, { phase: 0, status: "idle" });
    await expect(page.getByRole("button", { name: /^Record$/ })).toBeEnabled();

    // loading: "Loading…", disabled
    await emitTick(page, {
      phase: 1,
      status: "idle",
      can_toggle: false,
      status_message: "Loading model…",
    });
    await expect(page.getByRole("button", { name: /Loading/ })).toBeDisabled();

    // recording: "Stop & transcribe", enabled
    await emitTick(page, {
      phase: 2,
      status: "recording",
      is_recording: true,
      can_toggle: true,
    });
    await expect(page.getByRole("button", { name: /Stop & transcribe/ })).toBeEnabled();

    // transcribing: "Transcribing…", disabled
    await emitTick(page, {
      phase: 3,
      status: "transcribing",
      can_toggle: false,
    });
    await expect(page.getByRole("button", { name: /Transcribing/ })).toBeDisabled();

    // back to idle: "Record"
    await emitTick(page, { phase: 0, status: "idle", can_toggle: true });
    await expect(page.getByRole("button", { name: /^Record$/ })).toBeEnabled();
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
          "Microphone access denied. Grant it in System Settings → Privacy & Security → Microphone, then reopen OpenWhisper.",
      }),
    );
    const banner = page.getByTestId("mic-banner");
    await expect(banner).toBeVisible();
    await expect(banner).toContainText("Microphone access denied");
    // Mic banner is informational — no Retry button (recovery is via
    // System Settings, not an in-app button).
    await expect(banner.getByRole("button")).toHaveCount(0);

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
  test("appears on PHASE_ERROR with recognizer load prefix; transcribe-prefix errors stay in debug only", async ({
    page,
  }) => {
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

    // Per-utterance transcribe failure (different prefix) → debug KV only,
    // no banner. Confirms the prefix-based gating discriminates the two
    // error sources correctly.
    await emitTick(page, {
      phase: 5,
      status: "idle",
      can_toggle: true,
      error_message: "transcribe: empty audio buffer",
    });
    await expect(page.getByTestId("recognizer-banner")).toHaveCount(0);
  });
});

test.describe("sidebar nav", () => {
  test("clicking sidebar items switches the visible pane", async ({ page }) => {
    await page.goto("/");
    // Default route is Home.
    await expect(page.getByTestId("sidebar-item-home")).toHaveAttribute("aria-current", "page");

    // Click Diagnostics — old debug content should still be visible
    // (Task 1 leaves the Diagnostics body wired to MainWindowShell).
    await page.getByTestId("sidebar-item-diagnostics").click();
    await expect(page.getByTestId("sidebar-item-diagnostics")).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect(page.getByText("Rust ↔ React FFI")).toBeVisible();

    // Click Settings — existing settings shell renders.
    await page.getByTestId("sidebar-item-settings").click();
    await expect(page.getByTestId("sidebar-item-settings")).toHaveAttribute(
      "aria-current",
      "page",
    );
    await expect(page.getByRole("tab", { name: "General" })).toBeVisible();

    // Click Home — sidebar marks Home active. (Hero content lands in Task 4;
    // assert sidebar state only.)
    await page.getByTestId("sidebar-item-home").click();
    await expect(page.getByTestId("sidebar-item-home")).toHaveAttribute("aria-current", "page");
  });
});

test.describe("text selection", () => {
  test("chrome non-selectable, transcript + KV values selectable", async ({ page }) => {
    await page.goto("/");
    await waitForTickListener(page);
    await emitTick(page, {
      phase: 0,
      status: "idle",
      transcript: "selectable transcript text",
    });

    const headerSelect = await page
      .locator("h1", { hasText: "OpenWhisper Dev" })
      .evaluate((el) => getComputedStyle(el).userSelect);
    expect(headerSelect).toBe("none");

    const transcriptSelect = await page
      .getByText("selectable transcript text")
      .evaluate((el) => getComputedStyle(el).userSelect);
    expect(transcriptSelect).toBe("text");
  });
});
