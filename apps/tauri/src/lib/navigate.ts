// `ow_navigate`: Rust (the tray, the pill, the answer window of Ask
// through `open_settings_window`) and a few components tell the main
// window which view to show. Mirrors src-tauri/src/navigate.rs.
//
// The payload is a view name as a plain string ("settings", "main",
// "diagnostics", "transcribe"), or an object that also names a
// settings pane and a row on it to bring into view:
//
//   { view: "settings", pane: "ask", focus: "web-search" }

import { SETTINGS_PANES, type SettingsPaneId } from "./settings-panes";

export const NAVIGATE_EVENT = "ow_navigate";

// Where an `ow_navigate` leads. `pane` and `focus` are null unless
// the view is Settings and the payload named them; `focus` is null
// without a pane.
export interface NavigateTarget {
  route: "home" | "settings" | "diagnostics" | "transcribe";
  pane: SettingsPaneId | null;
  // A row of that pane, as the part of its DOM id after the pane
  // (`settingsRowId`).
  focus: string | null;
}

// View names as they travel. "main" is Home: the tray has always
// called it that.
const ROUTES = new Map<string, NavigateTarget["route"]>([
  ["main", "home"],
  ["settings", "settings"],
  ["diagnostics", "diagnostics"],
  ["transcribe", "transcribe"],
]);

const ROW_NAME = /^[a-z0-9-]+$/;

function isPane(value: unknown): value is SettingsPaneId {
  return SETTINGS_PANES.some((pane) => pane.id === value);
}

// Read a payload of `ow_navigate`. null for a view this window does
// not have; a pane or a row it does not know is left out.
export function navigateTarget(payload: unknown): NavigateTarget | null {
  const named = typeof payload === "string" ? { view: payload } : payload;
  if (typeof named !== "object" || named === null) return null;
  const { view, pane, focus } = named as { view?: unknown; pane?: unknown; focus?: unknown };
  const route = typeof view === "string" ? ROUTES.get(view) : undefined;
  if (!route) return null;
  if (route !== "settings" || !isPane(pane)) return { route, pane: null, focus: null };
  return {
    route,
    pane,
    focus: typeof focus === "string" && ROW_NAME.test(focus) ? focus : null,
  };
}

// The DOM id a settings row carries so that navigation can bring it
// into view: `settingsRowId("ask", "web-search")` is the Web search
// row of the Ask pane.
export function settingsRowId(pane: SettingsPaneId, row: string): string {
  return `settings-${pane}-${row}`;
}
