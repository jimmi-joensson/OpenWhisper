import { emitTick, expect, test, waitForTickListener } from "./fixtures/tauri-shim";

test.describe("diagnostics pane", () => {
  test("renders all four debug cards", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("sidebar-item-diagnostics").click();

    await expect(page.getByText("Rust ↔ React FFI")).toBeVisible();
    await expect(page.getByText("Dictation debug")).toBeVisible();
    await expect(page.getByText("Dictation (mic → Rust core → Parakeet)")).toBeVisible();
    await expect(page.getByText("transcript", { exact: true })).toBeVisible();
  });

  test("FFI section shows mocked core_version", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("sidebar-item-diagnostics").click();
    await expect(page.getByText("0.1.0-test")).toBeVisible();
  });

  test("debug Card reflects tick payload", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("sidebar-item-diagnostics").click();
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

  test("transcript + KV values selectable", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("sidebar-item-diagnostics").click();
    await waitForTickListener(page);
    await emitTick(page, {
      phase: 0,
      status: "idle",
      transcript: "selectable transcript text",
    });

    const transcriptSelect = await page
      .getByText("selectable transcript text")
      .evaluate((el) => getComputedStyle(el).userSelect);
    expect(transcriptSelect).toBe("text");
  });

  test("transcribe-prefix error appears in debug card last error row, no banner anywhere", async ({
    page,
  }) => {
    await page.goto("/");
    await page.getByTestId("sidebar-item-diagnostics").click();
    await waitForTickListener(page);

    await emitTick(page, {
      phase: 5,
      status: "idle",
      can_toggle: true,
      error_message: "transcribe: empty audio buffer",
    });

    // Debug card's "last error" KV row carries the message.
    const debugCard = page
      .locator("div", { has: page.getByText("Dictation debug", { exact: true }) })
      .first();
    await expect(debugCard.getByText("transcribe: empty audio buffer")).toBeVisible();

    // No recognizer banner on Diagnostics (banners are home-only).
    await expect(page.getByTestId("recognizer-banner")).toHaveCount(0);
  });
});

test.describe("phase transitions drive RecordButton", () => {
  test("idle → loading → recording → transcribing → idle", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("sidebar-item-diagnostics").click();
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
