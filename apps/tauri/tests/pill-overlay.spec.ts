import { pillTest as test, expect } from "./fixtures/tauri-shim";

// PillOverlay tests. main.tsx mounts PillOverlay (instead of App) when the
// window label is "pill", so the pillTest fixture is the only thing
// distinguishing this from the main-window suite. Shim's invoke stubs
// cover reposition_pill, set_pill_click_through, and show_main_window so
// the mount effects don't surface unhandled rejections.

interface PillStatePayload {
  status: "idle" | "recording" | "transcribing";
  levels: number[];
  willRefine?: boolean;
  refining?: boolean;
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

// The Refine mark (TASK-108): the pill says in a line above the
// capsule when a dictation goes through Refine, and says nothing when
// it does not. The capsule itself moves as for any dictation.
test.describe("pill overlay — Refine", () => {
  const label = (page: import("@playwright/test").Page) =>
    page.getByTestId("pill-mode-label");
  // Where the first dot is, in canvas pixels. It is the lowest point
  // of the sphere, whose spin leaves it in place.
  const firstDot = (page: import("@playwright/test").Page) =>
    page.locator(".pill-svg rect").first().evaluate((el) => {
      const m = /translate\(([-\d.]+) ([-\d.]+)\)/.exec(el.getAttribute("transform") ?? "");
      return m ? { x: Number(m[1]), y: Number(m[2]) } : null;
    });

  test("a plain dictation shows no Refine line, recording or transcribing", async ({ page }) => {
    await page.goto("/");
    await waitForPillMount(page);
    await emitPillState(page, { status: "recording", levels: RECORDING_LEVELS });
    await expectCapsuleRect(page, 105, 33);
    await expect(label(page)).toHaveAttribute("data-visible", "false");
    await emitPillState(page, { status: "transcribing", levels: ZERO_LEVELS });
    await expectCapsuleRect(page, 57, 33);
    await expect(label(page)).toHaveAttribute("data-visible", "false");
    await expect(label(page)).toHaveAttribute("data-refine", "false");
  });

  test("the Refine line stays up for the whole dictation: recording, transcribing, refining", async ({
    page,
  }) => {
    await page.goto("/");
    await waitForPillMount(page);
    // Recording: up at once, and still up long after a mode name
    // would have gone.
    await emitPillState(page, {
      status: "recording",
      levels: RECORDING_LEVELS,
      willRefine: true,
    });
    await expect(label(page)).toHaveAttribute("data-visible", "true");
    await expect(label(page)).toHaveAttribute("data-refine", "true");
    await expect(label(page)).toHaveText("Refine");
    await expect(label(page).locator("svg")).toBeVisible();
    await page.waitForTimeout(2000);
    await emitPillState(page, {
      status: "recording",
      levels: RECORDING_LEVELS,
      willRefine: true,
    });
    await expect(label(page)).toHaveAttribute("data-visible", "true");

    // Stopped, being transcribed: the same line, over the usual
    // sphere (its lowest dot stays where it is).
    await emitPillState(page, { status: "transcribing", levels: ZERO_LEVELS, willRefine: true });
    await expectCapsuleRect(page, 57, 33);
    await expect(label(page)).toHaveText("Refine");
    await expect(label(page)).toHaveAttribute("data-visible", "true");
    await expect.poll(async () => (await firstDot(page))?.y ?? 0).toBeGreaterThan(9);

    // With the model: it says so, and the sphere is still the sphere.
    await emitPillState(page, {
      status: "transcribing",
      levels: ZERO_LEVELS,
      willRefine: true,
      refining: true,
    });
    await expect(label(page)).toHaveText("Refining");
    await expect(label(page)).toHaveAttribute("data-visible", "true");
    await page.waitForTimeout(1800);
    await expect(label(page)).toHaveAttribute("data-visible", "true");
    expect((await firstDot(page))?.y ?? 0).toBeGreaterThan(9);

    // Delivered: back to idle, the line goes.
    await emitPillState(page, { status: "idle", levels: ZERO_LEVELS });
    await expect(label(page)).toHaveAttribute("data-visible", "false");
    await expectCapsuleRect(page, 38, 22);
  });

  test("a dictation marked only when it was stopped gets the line from then on", async ({
    page,
  }) => {
    await page.goto("/");
    await waitForPillMount(page);
    await emitPillState(page, { status: "recording", levels: RECORDING_LEVELS });
    await expectCapsuleRect(page, 105, 33);
    await expect(label(page)).toHaveAttribute("data-visible", "false");
    await emitPillState(page, { status: "transcribing", levels: ZERO_LEVELS, willRefine: true });
    await expect(label(page)).toHaveText("Refine");
    await expect(label(page)).toHaveAttribute("data-visible", "true");
  });

  test("a mode name shown meanwhile gives the place back to the Refine line", async ({ page }) => {
    await page.goto("/");
    await waitForPillMount(page);
    await emitPillState(page, {
      status: "recording",
      levels: RECORDING_LEVELS,
      willRefine: true,
    });
    await expect(label(page)).toHaveText("Refine");
    // The user switches mode while talking.
    await expect
      .poll(() =>
        page.evaluate(() => {
          window.__owEmit("modes_changed", {
            settings: { active: "formal", custom: [], app_defaults: [] },
            modes: [],
            switched_to: "Formal",
          });
          return document.querySelector('[data-testid="pill-mode-label"]')?.textContent;
        }),
      )
      .toBe("Formal");
    await expect(label(page)).toHaveAttribute("data-refine", "false");
    // A moment later the Refine line is back, with its mark.
    await expect(label(page)).toHaveText("Refine", { timeout: 4000 });
    await expect(label(page)).toHaveAttribute("data-refine", "true");
    await expect(label(page)).toHaveAttribute("data-visible", "true");
  });
});
