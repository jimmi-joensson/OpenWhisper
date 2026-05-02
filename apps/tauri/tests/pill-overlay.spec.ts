import { pillTest as test, expect } from "./fixtures/tauri-shim";

// PillOverlay tests. main.tsx mounts PillOverlay (instead of App) when the
// window label is "pill", so the pillTest fixture is the only thing
// distinguishing this from the main-window suite. Shim's invoke stubs
// cover reposition_pill, set_pill_click_through, and show_main_window so
// the mount effects don't surface unhandled rejections.

interface PillStatePayload {
  status: "idle" | "recording" | "transcribing";
  levels: number[];
}

const ZERO_LEVELS = Array.from({ length: 12 }, () => 0);
const RECORDING_LEVELS = Array.from({ length: 12 }, (_, i) => 0.4 + i * 0.02);

async function emitPillState(
  page: import("@playwright/test").Page,
  payload: PillStatePayload,
) {
  return page.evaluate((p) => window.__owEmit("pill_state", p), payload);
}

async function readCapsuleRect(page: import("@playwright/test").Page) {
  return page.locator(".pill-capsule").boundingBox();
}

async function waitForPillMount(page: import("@playwright/test").Page) {
  await page.locator(".pill-capsule").waitFor({ state: "visible" });
}

// Poll for a target visual rect within a generous timeout. Replaces fixed
// waitForTimeout — under parallel test load RAF can drop frames and fixed
// settle windows go flaky. Tolerance ±1.5 px absorbs sub-pixel rounding +
// the rare frame where the spring is at, e.g., scale 1.99 instead of 2.0
// (still at the spring's snap threshold).
async function expectCapsuleRect(
  page: import("@playwright/test").Page,
  width: number,
  height: number,
  timeout = 4000,
) {
  const close = (a: number, b: number) => Math.abs(a - b) <= 1.5;
  await expect
    .poll(
      async () => {
        const r = await readCapsuleRect(page);
        return r != null && close(r.width, width) && close(r.height, height);
      },
      { timeout, intervals: [50, 100, 200] },
    )
    .toBe(true);
}

test.describe("pill overlay — visual dimensions", () => {
  test("idle capsule renders at 38x22", async ({ page }) => {
    await page.goto("/");
    await waitForPillMount(page);
    await expectCapsuleRect(page, 38, 22);
  });

  test("recording capsule renders at 105x33 (1.5x scale)", async ({ page }) => {
    await page.goto("/");
    await waitForPillMount(page);
    await emitPillState(page, { status: "recording", levels: RECORDING_LEVELS });
    // Layout width 70 × scale 1.5 = 105; layout height 22 × scale 1.5 = 33.
    await expectCapsuleRect(page, 105, 33);
  });

  test("transcribing capsule renders at 57x33 (1.5x scale)", async ({ page }) => {
    await page.goto("/");
    await waitForPillMount(page);
    await emitPillState(page, { status: "transcribing", levels: ZERO_LEVELS });
    // Layout width 38 × scale 1.5 = 57; layout height 22 × scale 1.5 = 33.
    await expectCapsuleRect(page, 57, 33);
  });
});

test.describe("pill overlay — reduced motion", () => {
  test("idle->recording reaches target rect quickly", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/");
    await waitForPillMount(page);
    await emitPillState(page, { status: "recording", levels: RECORDING_LEVELS });
    // Reduced-motion branches snap width + scale on the next RAF tick.
    // 500ms timeout is conservative under parallel load; the snap itself
    // resolves in a few frames.
    await expectCapsuleRect(page, 105, 33, 500);
  });

  test("transcribing->idle snaps without sphere implode tween", async ({
    page,
  }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto("/");
    await waitForPillMount(page);
    await emitPillState(page, { status: "transcribing", levels: ZERO_LEVELS });
    await expectCapsuleRect(page, 57, 33, 500);
    await emitPillState(page, { status: "idle", levels: ZERO_LEVELS });
    await expectCapsuleRect(page, 38, 22, 500);
  });
});

test.describe("pill overlay — click-through gating", () => {
  test("idle calls set_pill_click_through(passthrough: false)", async ({
    page,
  }) => {
    await page.goto("/");
    await waitForPillMount(page);
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owPillPassthrough?: boolean })
              .__owPillPassthrough,
        ),
      )
      .toBe(false);
  });

  test("recording flips passthrough to true", async ({ page }) => {
    await page.goto("/");
    await waitForPillMount(page);
    await emitPillState(page, { status: "recording", levels: RECORDING_LEVELS });
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owPillPassthrough?: boolean })
              .__owPillPassthrough,
        ),
      )
      .toBe(true);
  });

  test("returning to idle flips passthrough back to false", async ({ page }) => {
    await page.goto("/");
    await waitForPillMount(page);
    await emitPillState(page, { status: "recording", levels: RECORDING_LEVELS });
    // Wait for the recording target rect to land before flipping back so
    // the click-through invoke ordering is stable.
    await expectCapsuleRect(page, 105, 33);
    await emitPillState(page, { status: "idle", levels: ZERO_LEVELS });
    await expect
      .poll(() =>
        page.evaluate(
          () =>
            (window as unknown as { __owPillPassthrough?: boolean })
              .__owPillPassthrough,
        ),
      )
      .toBe(false);
  });
});
