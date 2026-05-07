import { describe, expect, it } from "vitest";
import {
  breakdownEstimate,
  isRecognizerInProcessResident,
  type ModelMemoryRow,
} from "./use-memory-stats";

function recognizerRow(
  state: ModelMemoryRow["state"],
  overrides: Partial<ModelMemoryRow> = {},
): ModelMemoryRow {
  return {
    label: "recognizer",
    state,
    estimated_rss_bytes: 0,
    claimed_bytes: 0,
    in_process: true,
    ...overrides,
  };
}

describe("isRecognizerInProcessResident", () => {
  it("returns false when no recognizer row is present", () => {
    expect(isRecognizerInProcessResident([])).toBe(false);
  });

  it("returns false when the recognizer is Unloaded", () => {
    expect(isRecognizerInProcessResident([recognizerRow("Unloaded")])).toBe(
      false,
    );
  });

  it("returns true for in-process Loaded / Active / Loading / Releasing", () => {
    expect(isRecognizerInProcessResident([recognizerRow("Loaded")])).toBe(true);
    expect(isRecognizerInProcessResident([recognizerRow("Active")])).toBe(true);
    expect(isRecognizerInProcessResident([recognizerRow("Loading")])).toBe(
      true,
    );
    expect(isRecognizerInProcessResident([recognizerRow("Releasing")])).toBe(
      true,
    );
  });

  it("returns false when the recognizer is loaded but ANE-resident (Mac)", () => {
    expect(
      isRecognizerInProcessResident([
        recognizerRow("Loaded", { in_process: false }),
      ]),
    ).toBe(false);
    expect(
      isRecognizerInProcessResident([
        recognizerRow("Active", { in_process: false }),
      ]),
    ).toBe(false);
  });
});

describe("breakdownEstimate", () => {
  it("attributes 612 MB to Parakeet weights when in-process resident (Windows shape)", () => {
    const r = breakdownEstimate(1100, true);
    expect(r.parakeetMb).toBe(612);
    // Non-Parakeet residual = 488 MB; ratios 28% / 16% / remainder.
    expect(r.audioBuffersMb).toBe(Math.round(488 * 0.28));
    expect(r.cachesMb).toBe(Math.round(488 * 0.16));
    const sum =
      r.parakeetMb + r.audioBuffersMb + r.cachesMb + r.appShellMb;
    expect(sum).toBe(1100);
  });

  it("zeroes the Parakeet segment when the recognizer is ANE-resident (Mac shape)", () => {
    // 110 MB process RSS — typical Mac shape with Parakeet on the ANE.
    const r = breakdownEstimate(110, false);
    expect(r.parakeetMb).toBe(0);
    const sum =
      r.parakeetMb + r.audioBuffersMb + r.cachesMb + r.appShellMb;
    expect(sum).toBe(110);
    // App shell stays the largest non-Parakeet segment.
    expect(r.appShellMb).toBeGreaterThan(r.audioBuffersMb);
    expect(r.appShellMb).toBeGreaterThan(r.cachesMb);
  });

  it("scales audio + caches proportionally so segments always fit RSS", () => {
    // Cold start, no recognizer, modest RSS.
    const r = breakdownEstimate(280, false);
    expect(r.parakeetMb).toBe(0);
    const sum =
      r.parakeetMb + r.audioBuffersMb + r.cachesMb + r.appShellMb;
    expect(sum).toBe(280);
  });

  it("never produces a negative app-shell segment on tiny RSS", () => {
    const r = breakdownEstimate(40, false);
    expect(r.appShellMb).toBeGreaterThanOrEqual(0);
    const sum =
      r.parakeetMb + r.audioBuffersMb + r.cachesMb + r.appShellMb;
    expect(sum).toBe(40);
  });
});
