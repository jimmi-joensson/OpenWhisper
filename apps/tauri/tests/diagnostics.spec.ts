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
const GB = 1024 * MB;

const NORMAL_SYSTEM = {
  total_bytes: 24 * GB,
  used_bytes: 14 * GB,
  available_bytes: 10 * GB,
  swap_total_bytes: 4 * GB,
  swap_used_bytes: 0,
};

const CRITICAL_SYSTEM = {
  total_bytes: 24 * GB,
  used_bytes: 23 * GB, // 95.8% — flips pressure to critical
  available_bytes: GB,
  swap_total_bytes: 4 * GB,
  swap_used_bytes: 0,
};

test.describe("diagnostics pane", () => {
  test("sidebar entry opens pane with Memory card", async ({ page }) => {
    await page.goto("/");
    await setMemoryStats(page, {
      system: NORMAL_SYSTEM,
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
      page.getByRole("heading", { name: "Diagnostics", exact: true }),
    ).toBeVisible();
    await expect(page.getByText("Memory", { exact: true })).toBeVisible();
    await expect(page.getByText("System Memory Used")).toBeVisible();
    await expect(
      page.getByText("OpenWhisper Memory", { exact: true }),
    ).toBeVisible();
  });

  test("readouts reflect telemetry_get_memory snapshot", async ({ page }) => {
    await page.goto("/");
    await setMemoryStats(page, {
      system: NORMAL_SYSTEM,
      process: {
        rss_bytes: 612 * MB,
        peak_rss_bytes: 700 * MB,
        timestamp_unix_ms: 0,
      },
      models: [],
    });
    await page.getByTestId("sidebar-item-diagnostics").click();

    // System Memory Used renders 14 GB out of 24.
    const sysCell = page.getByTestId(
      "diagnostics-readout-system-memory-used",
    );
    await expect(sysCell).toHaveText("14.00");

    // OpenWhisper Memory with no ANE claim reads the same as RSS.
    const owCell = page.getByTestId(
      "diagnostics-readout-openwhisper-memory",
    );
    await expect(owCell).toHaveText("612");

    // Pressure pill reads "Normal" at 14/24 used.
    await expect(
      page.getByTestId("diagnostics-pressure-label"),
    ).toHaveText("Normal");

    // Stash a heavier snapshot + drive a refetch via the event channel —
    // same code path the 1-Hz poll uses. Avoids real-time waits.
    await waitForModelStateChangedListener(page);
    await setMemoryStats(page, {
      system: CRITICAL_SYSTEM,
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

    await expect(owCell).toHaveText("800");
    // 23/24 used — pressure flips Critical.
    await expect(
      page.getByTestId("diagnostics-pressure-label"),
    ).toHaveText("Critical");
    await expect(page.getByTestId("diagnostics-pressure")).toHaveAttribute(
      "data-level",
      "critical",
    );
  });

  test("Mac ANE recognizer adds out-of-process claim to total", async ({
    page,
  }) => {
    // Mac path: recognizer weights live in the ANE pool, not in
    // process RSS. The OpenWhisper Memory readout must reflect the
    // full claim — process RSS + ANE — so the user sees the actual
    // memory footprint without alt-tabbing to Activity Monitor.
    await page.goto("/");
    await setMemoryStats(page, {
      system: NORMAL_SYSTEM,
      process: {
        rss_bytes: 143 * MB,
        peak_rss_bytes: 200 * MB,
        timestamp_unix_ms: 0,
      },
      models: [
        {
          label: "recognizer",
          state: "Loaded",
          estimated_rss_bytes: 44 * MB,
          claimed_bytes: 461 * MB,
          in_process: false,
        },
      ],
    });
    await page.getByTestId("sidebar-item-diagnostics").click();

    // Total = 143 MB process + 461 MB ANE = 604 MB. Formatter snaps
    // to MB at < 1024 MB so the displayed number is 604.
    const owCell = page.getByTestId(
      "diagnostics-readout-openwhisper-memory",
    );
    await expect(owCell).toHaveText("604");

    // Sub-text exposes the split.
    await expect(
      page.getByText(/143 MB process \+ 461 MB ANE/),
    ).toBeVisible();

    // Breakdown bar carries an out-of-process segment for the
    // recognizer + the in-process Other residual.
    const externalSeg = page.getByTestId(
      "diagnostics-segment-model-recognizer",
    );
    await expect(externalSeg).toBeVisible();
    await expect(externalSeg).toHaveAttribute(
      "data-kind",
      "model-external",
    );
    await expect(externalSeg.getByText(/Recognizer \(ANE\)/)).toBeVisible();
  });

  test("memory poll keeps running after leaving the pane", async ({
    page,
  }) => {
    // Background-poll persistence guarantee — the ring buffer must
    // keep filling while the user is on Home / Settings so a model
    // load that happens off-pane is visible the moment they come
    // back. Verified by counting `telemetry_get_memory` invokes
    // while Diagnostics is NOT mounted: count must grow.
    await page.goto("/");
    await setMemoryStats(page, {
      system: NORMAL_SYSTEM,
      process: {
        rss_bytes: 143 * MB,
        peak_rss_bytes: 200 * MB,
        timestamp_unix_ms: 0,
      },
      models: [],
    });
    // Open Diagnostics first so we know the poll has booted.
    await page.getByTestId("sidebar-item-diagnostics").click();
    await expect(
      page.getByTestId("diagnostics-readout-openwhisper-memory"),
    ).toHaveText("143");

    const baseline = await page.evaluate(
      () =>
        (window as unknown as { __owTelemetryGetCount?: number })
          .__owTelemetryGetCount ?? 0,
    );

    // Leave the pane and idle long enough for at least one
    // additional 1 Hz poll to fire in the background.
    await page.getByTestId("sidebar-item-home").click();
    await page.waitForTimeout(1500);

    const afterLeaving = await page.evaluate(
      () =>
        (window as unknown as { __owTelemetryGetCount?: number })
          .__owTelemetryGetCount ?? 0,
    );
    // The exact delta depends on timer scheduling but at least one
    // poll must have fired off-pane. Strict-greater is the
    // load-bearing assertion.
    expect(afterLeaving).toBeGreaterThan(baseline);

    // Returning to Diagnostics shows the latest snapshot
    // immediately — no "graph spawning from scratch" gap.
    await setMemoryStats(page, {
      system: NORMAL_SYSTEM,
      process: {
        rss_bytes: 300 * MB,
        peak_rss_bytes: 320 * MB,
        timestamp_unix_ms: 0,
      },
      models: [],
    });
    await page.waitForTimeout(1100);
    await page.getByTestId("sidebar-item-diagnostics").click();
    await expect(
      page.getByTestId("diagnostics-readout-openwhisper-memory"),
    ).toHaveText("300");
  });

  test("memory poll pauses while window is hidden", async ({ page }) => {
    // Performance optimization — when the user can't see the chart
    // (window minimized, app in tray, screen locked) the 1 Hz IPC
    // is wasted. Verified by flipping `document.hidden` true,
    // counting invokes during the hidden window, then flipping
    // back and confirming the poll resumes.
    await page.goto("/");
    await setMemoryStats(page, {
      system: NORMAL_SYSTEM,
      process: {
        rss_bytes: 143 * MB,
        peak_rss_bytes: 200 * MB,
        timestamp_unix_ms: 0,
      },
      models: [],
    });
    await page.getByTestId("sidebar-item-diagnostics").click();
    await expect(
      page.getByTestId("diagnostics-readout-openwhisper-memory"),
    ).toHaveText("143");

    // Stub `document.hidden` and dispatch `visibilitychange`. The
    // store's handler tears down the interval. After 1500 ms the
    // invoke count must NOT have grown.
    await page.evaluate(() => {
      Object.defineProperty(document, "hidden", {
        configurable: true,
        get: () => true,
      });
      Object.defineProperty(document, "visibilityState", {
        configurable: true,
        get: () => "hidden",
      });
      document.dispatchEvent(new Event("visibilitychange"));
    });
    const baseline = await page.evaluate(
      () =>
        (window as unknown as { __owTelemetryGetCount?: number })
          .__owTelemetryGetCount ?? 0,
    );
    await page.waitForTimeout(1500);
    const afterHidden = await page.evaluate(
      () =>
        (window as unknown as { __owTelemetryGetCount?: number })
          .__owTelemetryGetCount ?? 0,
    );
    expect(afterHidden).toBe(baseline);

    // Flip back to visible — the store re-arms the interval and
    // immediately fetches once. Count must grow within 300 ms.
    await page.evaluate(() => {
      Object.defineProperty(document, "hidden", {
        configurable: true,
        get: () => false,
      });
      Object.defineProperty(document, "visibilityState", {
        configurable: true,
        get: () => "visible",
      });
      document.dispatchEvent(new Event("visibilitychange"));
    });
    await page.waitForTimeout(300);
    const afterVisible = await page.evaluate(
      () =>
        (window as unknown as { __owTelemetryGetCount?: number })
          .__owTelemetryGetCount ?? 0,
    );
    expect(afterVisible).toBeGreaterThan(afterHidden);
  });

  test("breakdown bar adds a model segment on model-state-changed", async ({
    page,
  }) => {
    await page.goto("/");
    await setMemoryStats(page, {
      system: NORMAL_SYSTEM,
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
          claimed_bytes: 0,
          in_process: true,
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

    // Recognizer transitions to Loaded with a 612 MB claim
    // (Windows-shape — in-process). The pane refetches on the
    // event and the breakdown bar gains an in-process model
    // segment + matching legend row.
    await setMemoryStats(page, {
      system: NORMAL_SYSTEM,
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
          claimed_bytes: 612 * MB,
          in_process: true,
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
      page.getByTestId("diagnostics-segment-model-recognizer"),
    ).toHaveAttribute("data-kind", "model");
    await expect(
      page
        .getByTestId("diagnostics-segment-model-recognizer")
        .getByText("Recognizer", { exact: true }),
    ).toBeVisible();
  });

  test("idle-released recognizer drops its breakdown segment even with stale rss-delta", async ({
    page,
  }) => {
    // Regression for a Windows-only visual bug: after the 5-min idle
    // unload fires, the registry sets `claimed_bytes = 0` but the
    // `estimated_rss_bytes` field on `ModelHandle` is the *last
    // observed* load delta and is never cleared. Without an explicit
    // state gate, `buildSegments` falls back to the stale rss-delta
    // and renders a full ~1.2 GB Recognizer segment even though the
    // weights are gone — RSS readout shows 88 MB while the breakdown
    // claims 1.2 GB. Visible contradiction.
    await page.goto("/");
    await setMemoryStats(page, {
      system: NORMAL_SYSTEM,
      process: {
        rss_bytes: (88 + 1180) * MB,
        peak_rss_bytes: (88 + 1180) * MB,
        timestamp_unix_ms: 0,
      },
      models: [
        {
          label: "recognizer",
          state: "Loaded",
          estimated_rss_bytes: 1180 * MB,
          claimed_bytes: 640 * MB,
          in_process: true,
        },
      ],
    });
    await page.getByTestId("sidebar-item-diagnostics").click();
    await waitForModelStateChangedListener(page);
    await expect(
      page.getByTestId("diagnostics-segment-model-recognizer"),
    ).toBeVisible();

    // Idle unload: RSS drops back to baseline, registry zeroes
    // `claimed_bytes`, but `estimated_rss_bytes` retains the historic
    // 1180 MB delta. The segment must disappear; "Other" must equal
    // the full process RSS.
    await setMemoryStats(page, {
      system: NORMAL_SYSTEM,
      process: {
        rss_bytes: 88 * MB,
        peak_rss_bytes: (88 + 1180) * MB,
        timestamp_unix_ms: 0,
      },
      models: [
        {
          label: "recognizer",
          state: "Unloaded",
          estimated_rss_bytes: 1180 * MB,
          claimed_bytes: 0,
          in_process: true,
        },
      ],
    });
    await emitModelStateChanged(page, {
      label: "recognizer",
      state: "Unloaded",
    });

    await expect(
      page.getByTestId("diagnostics-segment-model-recognizer"),
    ).toHaveCount(0);
    await expect(page.getByTestId("diagnostics-segment-other")).toBeVisible();
  });
});
