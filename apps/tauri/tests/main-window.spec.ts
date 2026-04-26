import { emitTick, expect, test, waitForTickListener } from "./fixtures/tauri-shim";

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
  test("#root scrolls when content overflows the viewport", async ({ page }) => {
    await page.setViewportSize({ width: 600, height: 500 });
    await page.goto("/");
    await page.waitForSelector("text=OpenWhisper Dev");

    const probe = await page.evaluate(() => {
      const root = document.getElementById("root")!;
      return {
        clientH: root.clientHeight,
        scrollH: root.scrollHeight,
        overflowY: getComputedStyle(root).overflowY,
      };
    });

    expect(probe.overflowY).toBe("auto");
    expect(probe.scrollH).toBeGreaterThan(probe.clientH);

    const scrolled = await page.evaluate(() => {
      const root = document.getElementById("root")!;
      root.scrollTop = root.scrollHeight;
      return root.scrollTop;
    });
    expect(scrolled).toBeGreaterThan(0);

    // Bottom card should be visible after scroll-to-bottom.
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
