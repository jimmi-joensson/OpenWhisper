// Types and event names for Vision (TASK-98): an ask can include a
// picture of the screen. Mirror core/src/settings/vision.rs,
// core/src/vision/mod.rs and src-tauri/src/vision.rs.
//
// Only settings and states cross into the webview. The picture itself
// never does: no command takes or returns one.

// Mirrors `VisionTrigger` in core: when an ask includes the screen.
export type VisionTrigger = "off" | "phrases" | "smart" | "always";

// Mirrors `NeverCaptureApp` in core: an app that is never in a
// screenshot. `id` is the bundle id on macOS and the exe file name on
// Windows, in lower case.
export interface NeverCaptureApp {
  id: string;
  // May be "".
  name: string;
}

// Mirrors `VisionSettings` in core: the `vision` block of
// settings.json. A list that is null is the built-in one, so someone
// who never edited a list gets the defaults of the version they run.
export interface VisionSettings {
  trigger: VisionTrigger;
  phrases: string[] | null;
  never_capture: NeverCaptureApp[] | null;
}

// Mirrors `PermissionState` in core: whether the app may record the
// screen. "unknown" counts as not allowed.
export type VisionPermission = "granted" | "denied" | "unknown";

// What `vision_get` returns (`VisionState` in src-tauri/src/vision.rs).
export interface VisionState {
  settings: VisionSettings;
  // The lists in force: the person's, or the built-in ones.
  phrases: string[];
  never_capture: NeverCaptureApp[];
  // The lists Vision ships with on this platform.
  default_phrases: string[];
  default_never_capture: NeverCaptureApp[];
  // The list in force is the built-in one, entry for entry.
  phrases_are_default: boolean;
  never_capture_is_default: boolean;
  permission: VisionPermission;
  // Only macOS has a permission to give.
  platform: "macos" | "windows" | "other";
}

// Emitted after the Vision settings changed. Payload: `VisionSettings`.
export const VISION_CHANGED_EVENT = "vision_changed";

// The four choices of "When to include the screen", in the order they
// are offered, each with one line on what it does.
export const VISION_TRIGGERS: { value: VisionTrigger; label: string; description: string }[] = [
  {
    value: "off",
    label: "Off",
    description: "Never. No ask includes the screen.",
  },
  {
    value: "phrases",
    label: "When I say so",
    description:
      "Only when your question contains one of your phrases. The phrases are matched on this computer, before anything is sent.",
  },
  {
    value: "smart",
    label: "When needed",
    description:
      "When your question contains one of your phrases, or when the model says it needs to see the screen to answer. The question is then sent first, without the picture.",
  },
  {
    value: "always",
    label: "Always",
    description: "Every ask includes the screen.",
  },
];

// The trigger uses the phrase list.
export function triggerUsesPhrases(trigger: VisionTrigger): boolean {
  return trigger === "phrases" || trigger === "smart";
}

// What to call an app: its name, or its id when it has none.
export function appLabel(app: NeverCaptureApp): string {
  return app.name.trim() || app.id;
}

// The lists in force for a stored block, given the built-in lists.
// Used to show a change at once, before Rust has answered with what
// it stored.
export function withSettings(state: VisionState, settings: VisionSettings): VisionState {
  const phrases = settings.phrases ?? state.default_phrases;
  const neverCapture = settings.never_capture ?? state.default_never_capture;
  return {
    ...state,
    settings,
    phrases,
    never_capture: neverCapture,
    phrases_are_default: sameList(phrases, state.default_phrases),
    never_capture_is_default: sameList(
      neverCapture.map((a) => `${a.id}\n${a.name}`),
      state.default_never_capture.map((a) => `${a.id}\n${a.name}`),
    ),
  };
}

function sameList(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((value, i) => value === b[i]);
}
