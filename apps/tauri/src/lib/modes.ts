// Types for modes (TASK-96). Mirror core/src/settings/modes.rs,
// core/src/modes.rs and src-tauri/src/modes.rs. A mode is a name plus
// instructions the Refine step follows, optionally with its own model
// and effort. Every rule about modes (ids, what a delete does, which
// mode runs) lives in core; this file only names the shapes.

import type { RefineEffort } from "./refine";

// Id of the built-in Dictation mode: today's cleanup, whose
// instructions are the Refine instructions.
export const BUILTIN_MODE_ID = "dictation";

// One mode the user made, as stored.
export interface Mode {
  id: string;
  name: string;
  instructions: string;
  // null = the model in the Refine settings.
  model: string | null;
  // null = the effort in the Refine settings.
  effort: RefineEffort | null;
}

// "In this app, use that mode."
export interface AppDefault {
  app_id: string;
  // May be empty; then the id is shown.
  app_name: string;
  // Id of the mode.
  mode: string;
}

// The `modes` block of settings.json.
export interface ModesSettings {
  // Id of the selected mode.
  active: string;
  custom: Mode[];
  app_defaults: AppDefault[];
}

// A mode as it is listed: a custom one, or the built-in one put
// together from the Refine settings.
export interface ModeView extends Mode {
  builtin: boolean;
}

// An app that can be given a default mode.
export interface ModeApp {
  id: string;
  name: string;
}

// What every change returns: the stored block and the list of modes,
// the built-in one first.
export interface ModesNow {
  settings: ModesSettings;
  modes: ModeView[];
}

// What `modes_get` returns.
export interface ModesState extends ModesNow {
  // Apps seen in the history, then apps that already have a default.
  apps: ModeApp[];
}

// Payload of the `modes_changed` event, sent to every window after a
// change, whoever made it (a command, the hotkey, the tray menu).
export interface ModesChanged extends ModesNow {
  // Name of the mode now selected when the user switched mode; null
  // for every other change.
  switched_to: string | null;
}

export const MODES_CHANGED_EVENT = "modes_changed";

// What the mode dialog collects.
export interface ModeDraft {
  name: string;
  instructions: string;
  model: string | null;
  effort: RefineEffort | null;
}
