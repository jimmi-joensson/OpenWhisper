import { describe, expect, it } from "vitest";
import { navigateTarget, settingsRowId } from "./navigate";

describe("navigateTarget", () => {
  it("reads the plain view names the tray has always sent", () => {
    expect(navigateTarget("settings")).toEqual({ route: "settings", pane: null, focus: null });
    expect(navigateTarget("main")).toEqual({ route: "home", pane: null, focus: null });
    expect(navigateTarget("diagnostics")).toEqual({ route: "diagnostics", pane: null, focus: null });
    expect(navigateTarget("transcribe")).toEqual({ route: "transcribe", pane: null, focus: null });
  });

  it("reads a settings pane and a row on it from the object form", () => {
    expect(navigateTarget({ view: "settings", pane: "ask", focus: "web-search" })).toEqual({
      route: "settings",
      pane: "ask",
      focus: "web-search",
    });
    expect(navigateTarget({ view: "settings", pane: "refine" })).toEqual({
      route: "settings",
      pane: "refine",
      focus: null,
    });
    expect(navigateTarget({ view: "settings" })).toEqual({
      route: "settings",
      pane: null,
      focus: null,
    });
  });

  it("leaves out a pane or a row it does not know", () => {
    // An unknown pane: Settings opens where it would have anyway,
    // and a row without its pane means nothing.
    expect(navigateTarget({ view: "settings", pane: "billing", focus: "web-search" })).toEqual({
      route: "settings",
      pane: null,
      focus: null,
    });
    // A row name is a plain word, never a selector or markup.
    expect(navigateTarget({ view: "settings", pane: "ask", focus: "a b" })?.focus).toBeNull();
    expect(navigateTarget({ view: "settings", pane: "ask", focus: 7 })?.focus).toBeNull();
    // A pane only counts for Settings.
    expect(navigateTarget({ view: "main", pane: "ask", focus: "web-search" })).toEqual({
      route: "home",
      pane: null,
      focus: null,
    });
  });

  it("is null for a view this window does not have", () => {
    expect(navigateTarget("history-of-everything")).toBeNull();
    expect(navigateTarget("constructor")).toBeNull();
    expect(navigateTarget({ pane: "ask" })).toBeNull();
    expect(navigateTarget(null)).toBeNull();
    expect(navigateTarget(undefined)).toBeNull();
    expect(navigateTarget(3)).toBeNull();
  });
});

describe("settingsRowId", () => {
  it("names the row after its pane", () => {
    expect(settingsRowId("ask", "web-search")).toBe("settings-ask-web-search");
  });
});
