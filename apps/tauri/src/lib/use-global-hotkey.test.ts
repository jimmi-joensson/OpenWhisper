import { describe, expect, it } from "vitest";
import {
  hotkeyFor,
  withHotkey,
  type HotkeyConfig,
  type HotkeySettings,
} from "./use-global-hotkey";

const toggle: HotkeyConfig = { kind: "chord", code: "Space", mods: ["Ctrl"] };
const cancel: HotkeyConfig = { kind: "chord", code: "Escape", mods: [] };
const cycle: HotkeyConfig = { kind: "chord", code: "M", mods: ["Ctrl", "Alt"] };

// As the backend sends it while the optional slot is unbound: the
// `cycle_mode` key is absent, not null.
const unbound: HotkeySettings = { toggle, cancel };

describe("hotkeyFor", () => {
  it("reads the dictation slots by their target", () => {
    expect(hotkeyFor(unbound, "toggle")).toBe(toggle);
    expect(hotkeyFor(unbound, "cancel")).toBe(cancel);
  });

  it("maps the cycle-mode target to the cycle_mode key", () => {
    expect(hotkeyFor({ ...unbound, cycle_mode: cycle }, "cycle-mode")).toBe(cycle);
  });

  it("reports an optional slot as null whether the key is absent or null", () => {
    expect(hotkeyFor(unbound, "cycle-mode")).toBeNull();
    expect(hotkeyFor({ ...unbound, cycle_mode: null }, "cycle-mode")).toBeNull();
  });
});

describe("the ask slot", () => {
  const ask: HotkeyConfig = { kind: "modifier-tap", code: "RightOption", mods: [] };

  it("reads the ask target from the ask key", () => {
    expect(hotkeyFor({ ...unbound, ask }, "ask")).toBe(ask);
  });

  it("is null when cleared (null, as the backend sends it) or absent", () => {
    expect(hotkeyFor({ ...unbound, ask: null }, "ask")).toBeNull();
    expect(hotkeyFor(unbound, "ask")).toBeNull();
  });

  it("is set and cleared without touching the other slots", () => {
    const withCycle = { ...unbound, cycle_mode: cycle };
    const bound = withHotkey(withCycle, "ask", ask);
    expect(bound).toEqual({ toggle, cancel, cycle_mode: cycle, ask });
    const cleared = withHotkey(bound, "ask", null);
    expect(hotkeyFor(cleared, "ask")).toBeNull();
    expect(hotkeyFor(cleared, "cycle-mode")).toBe(cycle);
    expect(cleared.toggle).toBe(toggle);
  });
});

describe("withHotkey", () => {
  it("replaces one slot and leaves the others", () => {
    const next = withHotkey(unbound, "cancel", cycle);
    expect(next).toEqual({ toggle, cancel: cycle });
    expect(unbound.cancel).toBe(cancel);
  });

  it("sets and unsets the optional slot", () => {
    const bound = withHotkey(unbound, "cycle-mode", cycle);
    expect(bound.cycle_mode).toBe(cycle);
    expect(hotkeyFor(withHotkey(bound, "cycle-mode", null), "cycle-mode")).toBeNull();
  });

  it("never leaves toggle or cancel without a binding", () => {
    expect(withHotkey(unbound, "toggle", null)).toEqual(unbound);
    expect(withHotkey(unbound, "cancel", null)).toEqual(unbound);
  });
});
