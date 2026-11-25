import { describe, expect, it } from "vitest";
import { configToChipKeys, formatHotkeyLabel } from "./hotkey-format";

describe("configToChipKeys", () => {
  it("shows a modifier-tap as its one sided key", () => {
    expect(
      configToChipKeys({ kind: "modifier-tap", code: "RightCommand", mods: [] }),
    ).toEqual(["Right ⌘"]);
  });

  it("shows a modifier-combo as every key by its sided name", () => {
    const combo = {
      kind: "modifier-combo" as const,
      code: "RightCommand",
      mods: ["RightOption"],
    };
    expect(configToChipKeys(combo)).toEqual(["Right ⌘", "Right ⌥"]);
    expect(formatHotkeyLabel(combo)).toBe("Right ⌘ + Right ⌥");
  });

  it("shows a chord as its modifiers, then its key", () => {
    expect(
      configToChipKeys({ kind: "chord", code: "Space", mods: ["Ctrl", "Shift"] }),
    ).toEqual(["Ctrl", "Shift", "Space"]);
  });

  it("shows nothing for a slot without a key", () => {
    expect(configToChipKeys(null)).toEqual([]);
  });
});
