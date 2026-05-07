import { describe, expect, it } from "vitest";
import {
  breakdownEstimate,
  isRecognizerResident,
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

describe("isRecognizerResident", () => {
  it("returns false when no recognizer row is present", () => {
    expect(isRecognizerResident([])).toBe(false);
  });

  it("returns false when the recognizer is Unloaded", () => {
    expect(isRecognizerResident([recognizerRow("Unloaded")])).toBe(false);
  });

  it("returns true for Loaded / Active / Loading / Releasing", () => {
    expect(isRecognizerResident([recognizerRow("Loaded")])).toBe(true);
    expect(isRecognizerResident([recognizerRow("Active")])).toBe(true);
    expect(isRecognizerResident([recognizerRow("Loading")])).toBe(true);
    expect(isRecognizerResident([recognizerRow("Releasing")])).toBe(true);
  });
});

describe("breakdownEstimate", () => {
  it("attributes 612 MB to Parakeet weights when the recognizer is loaded", () => {
    const r = breakdownEstimate(1100, true);
    expect(r.parakeetMb).toBe(612);
    expect(r.audioBuffersMb).toBe(142);
    expect(r.cachesMb).toBe(84);
    expect(r.appShellMb).toBe(1100 - 612 - 142 - 84);
  });

  it("zeroes the Parakeet segment when the recognizer is not loaded", () => {
    const r = breakdownEstimate(700, false);
    expect(r.parakeetMb).toBe(0);
    expect(r.audioBuffersMb).toBe(142);
    expect(r.cachesMb).toBe(84);
    expect(r.appShellMb).toBe(700 - 142 - 84);
  });

  it("clamps the App shell segment to 100 MB on very low RSS", () => {
    const r = breakdownEstimate(120, false);
    expect(r.appShellMb).toBe(100);
  });

  it("keeps fixed baselines independent of recognizer state", () => {
    const loaded = breakdownEstimate(1500, true);
    const cold = breakdownEstimate(1500, false);
    expect(loaded.audioBuffersMb).toBe(cold.audioBuffersMb);
    expect(loaded.cachesMb).toBe(cold.cachesMb);
  });
});
