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

    // Sub-text exposes the split — the readout's sub-line is now the
    // sole carrier of the ANE attribution (the per-model breakdown bar
    // was retired in favour of a single design-faithful RSS Breakdown
    // bar; ANE weights are out-of-RSS by definition).
    await expect(
      page.getByText(/143 MB process \+ 461 MB ANE/),
    ).toBeVisible();
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

  test("RSS breakdown bar gains the Parakeet segment on a Windows-shape recognizer load", async ({
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

    // Cold: Parakeet segment hidden; the residual three (audio /
    // shell / caches) carry the full 280 MB process RSS.
    await expect(
      page.getByTestId("diagnostics-rss-segment-parakeet"),
    ).toHaveCount(0);
    await expect(
      page.getByTestId("diagnostics-rss-segment-shell"),
    ).toBeVisible();

    // Recognizer transitions to in-process Loaded with a 612 MB
    // claim (Windows shape). The pane refetches and the RSS
    // breakdown gains the Parakeet weights segment.
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
      page.getByTestId("diagnostics-rss-segment-parakeet"),
    ).toBeVisible();
  });

  test("RSS breakdown bar renders four canonical segments summing to ~100%", async ({
    page,
  }) => {
    // TASK-62.11 — V1 placeholder estimator splits RSS into Parakeet
    // weights / Audio buffers / App shell / Caches. With the
    // recognizer Loaded, all four segments are present; with no
    // recognizer, the Parakeet segment drops to zero and is hidden.
    await page.goto("/");
    await setMemoryStats(page, {
      system: NORMAL_SYSTEM,
      process: {
        rss_bytes: 1100 * MB,
        peak_rss_bytes: 1100 * MB,
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
    await page.getByTestId("sidebar-item-diagnostics").click();

    // Bar wrapper present.
    await expect(
      page.getByTestId("diagnostics-rss-breakdown"),
    ).toBeVisible();
    // Four canonical legend rows present, in order.
    await expect(
      page.getByTestId("diagnostics-rss-segment-parakeet"),
    ).toBeVisible();
    await expect(
      page.getByTestId("diagnostics-rss-segment-audio"),
    ).toBeVisible();
    await expect(
      page.getByTestId("diagnostics-rss-segment-shell"),
    ).toBeVisible();
    await expect(
      page.getByTestId("diagnostics-rss-segment-caches"),
    ).toBeVisible();

    // Percentages sum to within rounding tolerance of 100.
    const pctValues = await Promise.all(
      ["parakeet", "audio", "shell", "caches"].map(async (kind) => {
        const txt = await page
          .getByTestId(`diagnostics-rss-segment-${kind}-pct`)
          .innerText();
        return parseInt(txt.replace(/%$/, ""), 10);
      }),
    );
    const sum = pctValues.reduce((a, b) => a + b, 0);
    expect(sum).toBeGreaterThanOrEqual(99);
    expect(sum).toBeLessThanOrEqual(101);

    // Total readout uses honest units — GB at >= 1024 MB, MB below.
    // Bar covers process RSS + ANE; with the recognizer loaded
    // in-process at 612 MB inside an 1100 MB RSS the total stays
    // 1100 MB → 1.07 GB.
    await expect(
      page.getByTestId("diagnostics-rss-breakdown-total"),
    ).toContainText("GB total");
  });

  test("RSS breakdown hides Parakeet segment when recognizer is unloaded", async ({
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
      models: [],
    });
    await page.getByTestId("sidebar-item-diagnostics").click();

    await expect(
      page.getByTestId("diagnostics-rss-breakdown"),
    ).toBeVisible();
    await expect(
      page.getByTestId("diagnostics-rss-segment-parakeet"),
    ).toHaveCount(0);
    await expect(
      page.getByTestId("diagnostics-rss-segment-shell"),
    ).toBeVisible();
  });

  test("Memory breakdown attributes Parakeet to the ANE claim when recognizer is ANE-resident (Mac)", async ({
    page,
  }) => {
    // The bar represents OW memory as a whole (process RSS + ANE
    // claim), so on Mac the Parakeet segment is sourced from the
    // ANE/GPU claim rather than process RSS — matching the
    // "OpenWhisper Memory" total in the readout above (532 MB =
    // 72 MB process + 461 MB ANE in the canonical case). Earlier
    // shape limited the bar to in-process RSS, which read as
    // inconsistent against the readout total.
    await page.goto("/");
    await setMemoryStats(page, {
      system: NORMAL_SYSTEM,
      process: {
        rss_bytes: 110 * MB,
        peak_rss_bytes: 110 * MB,
        timestamp_unix_ms: 0,
      },
      models: [
        {
          label: "recognizer",
          state: "Loaded",
          estimated_rss_bytes: 0,
          claimed_bytes: 461 * MB,
          in_process: false,
        },
      ],
    });
    await page.getByTestId("sidebar-item-diagnostics").click();

    await expect(
      page.getByTestId("diagnostics-rss-breakdown"),
    ).toBeVisible();
    // Parakeet segment now visible — sourced from the ANE claim.
    await expect(
      page.getByTestId("diagnostics-rss-segment-parakeet"),
    ).toBeVisible();
    await expect(
      page.getByTestId("diagnostics-rss-segment-parakeet"),
    ).toContainText("Parakeet weights (ANE)");
    await expect(
      page.getByTestId("diagnostics-rss-segment-audio"),
    ).toBeVisible();
    await expect(
      page.getByTestId("diagnostics-rss-segment-shell"),
    ).toBeVisible();
    await expect(
      page.getByTestId("diagnostics-rss-segment-caches"),
    ).toBeVisible();
    const pctValues = await Promise.all(
      ["parakeet", "audio", "shell", "caches"].map(async (kind) => {
        const txt = await page
          .getByTestId(`diagnostics-rss-segment-${kind}-pct`)
          .innerText();
        return parseInt(txt.replace(/%$/, ""), 10);
      }),
    );
    const sum = pctValues.reduce((a, b) => a + b, 0);
    expect(sum).toBeGreaterThanOrEqual(99);
    expect(sum).toBeLessThanOrEqual(101);
  });
});
