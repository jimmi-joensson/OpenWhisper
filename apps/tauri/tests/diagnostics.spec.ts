// TASK-62.10 — Diagnostics pane covers MEMORY telemetry only.
// Performance counters (TASK-78.x) and Crashes (TASK-78.3) land as
// additional sections when their telemetry exists.
//
// MANUAL SMOKE — not CI-testable
//   The genuine cold-load-after-idle behaviour (Loaded → Unloaded →
//   first-dictation-after-6-min re-enters PHASE_LOADING_MODEL) cannot
//   be observed from the React side without a real recognizer load.
//   Verify on Mac AND Windows by:
//     1. `pnpm dev:tauri` (Mac) or `pnpm dev:tauri:win` (Windows).
//     2. Dictate once, then leave the app idle 6+ minutes.
//     3. Open Diagnostics → confirm Recognizer segment dropped from
//        the breakdown bar (registry shows Unloaded).
//     4. Dictate again — first transcription re-enters
//        PHASE_LOADING_MODEL briefly; second transcription is fast.
//     5. Toggle Settings → General → "Keep models warm" ON, repeat
//        step 2: Recognizer segment stays in the breakdown bar past
//        6 minutes.

import {
  emitModelStateChanged,
  expect,
  setMemoryStats,
  test,
  waitForModelStateChangedListener,
} from "./fixtures/tauri-shim";

const MB = 1024 * 1024;

test.describe("diagnostics pane", () => {
  test("sidebar entry opens pane with Memory card", async ({ page }) => {
    await page.goto("/");
    await setMemoryStats(page, {
      process: {
        rss_bytes: 256 * MB,
        peak_rss_bytes: 320 * MB,
        timestamp_unix_ms: 1_700_000_000_000,
      },
      models: [],
    });

    await expect(page.getByTestId("sidebar-item-diagnostics")).toBeVisible();
    await page.getByTestId("sidebar-item-diagnostics").click();

    await expect(
      page.getByRole("heading", { name: "Diagnostics" }),
    ).toBeVisible();
    await expect(page.getByText("Memory", { exact: true })).toBeVisible();
    await expect(page.getByText("OpenWhisper RSS")).toBeVisible();
  });

  test("RSS readout reflects telemetry_get_memory snapshot", async ({
    page,
  }) => {
    await page.goto("/");
    await setMemoryStats(page, {
      process: {
        rss_bytes: 612 * MB,
        peak_rss_bytes: 700 * MB,
        timestamp_unix_ms: 0,
      },
      models: [],
    });
    await page.getByTestId("sidebar-item-diagnostics").click();

    // Initial mount fetches synchronously. The OpenWhisper RSS readout
    // shows MB at 612 MB.
    const rssCell = page.getByTestId("diagnostics-readout-openwhisper-rss");
    await expect(rssCell).toHaveText("612");

    // Stash a higher snapshot + drive a refetch via the event channel —
    // same code path the 1-Hz poll uses. Avoids real-time waits.
    await waitForModelStateChangedListener(page);
    await setMemoryStats(page, {
      process: {
        rss_bytes: 800 * MB,
        peak_rss_bytes: 800 * MB,
        timestamp_unix_ms: 0,
      },
      models: [],
    });
    await emitModelStateChanged(page, {
      label: "recognizer",
      state: "Loaded",
    });

    await expect(rssCell).toHaveText("800");
  });

  test("breakdown bar adds a model segment on model-state-changed", async ({
    page,
  }) => {
    await page.goto("/");
    await setMemoryStats(page, {
      process: {
        rss_bytes: 280 * MB,
        peak_rss_bytes: 280 * MB,
        timestamp_unix_ms: 0,
      },
      models: [
        {
          label: "recognizer",
          state: "Unloaded",
          estimated_rss_bytes: 0,
        },
      ],
    });
    await page.getByTestId("sidebar-item-diagnostics").click();
    await waitForModelStateChangedListener(page);

    // No model segment yet — only the "Other" remainder is visible.
    await expect(page.getByTestId("diagnostics-segment-other")).toBeVisible();
    await expect(
      page.getByTestId("diagnostics-segment-model-recognizer"),
    ).toHaveCount(0);

    // Recognizer transitions to Loaded with a 612 MB delta. The pane
    // refetches on the event and the breakdown bar gains a model
    // segment + matching legend row.
    await setMemoryStats(page, {
      process: {
        rss_bytes: (280 + 612) * MB,
        peak_rss_bytes: (280 + 612) * MB,
        timestamp_unix_ms: 0,
      },
      models: [
        {
          label: "recognizer",
          state: "Loaded",
          estimated_rss_bytes: 612 * MB,
        },
      ],
    });
    await emitModelStateChanged(page, {
      label: "recognizer",
      state: "Loaded",
    });

    await expect(
      page.getByTestId("diagnostics-segment-model-recognizer"),
    ).toBeVisible();
    await expect(
      page
        .getByTestId("diagnostics-segment-model-recognizer")
        .getByText("Recognizer"),
    ).toBeVisible();
  });
});
