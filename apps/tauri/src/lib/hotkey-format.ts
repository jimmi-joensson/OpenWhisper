import type { HotkeyConfig } from "./use-global-hotkey";

export function configToChipKeys(cfg: HotkeyConfig | null): string[] {
  if (!cfg) return [];
  if (cfg.kind === "modifier-tap") {
    return [modifierLabel(cfg.code)];
  }
  return [...cfg.mods.map(modShortLabel), codeLabel(cfg.code)];
}

export function modifierLabel(code: string): string {
  switch (code) {
    case "RightCommand":
      return "Right ⌘";
    case "LeftCommand":
      return "Left ⌘";
    case "RightShift":
      return "Right ⇧";
    case "LeftShift":
      return "Left ⇧";
    case "RightOption":
      return "Right ⌥";
    case "LeftOption":
      return "Left ⌥";
    case "RightControl":
      return "Right ⌃";
    case "LeftControl":
      return "Left ⌃";
    default:
      return code;
  }
}

export function modShortLabel(name: string): string {
  switch (name) {
    case "Ctrl":
      return "Ctrl";
    case "Shift":
      return "Shift";
    case "Alt":
      return "Alt";
    case "Cmd":
      return "⌘";
    case "Win":
      return "Win";
    default:
      return name;
  }
}

export function codeLabel(code: string): string {
  switch (code) {
    case "ArrowLeft":
      return "←";
    case "ArrowRight":
      return "→";
    case "ArrowUp":
      return "↑";
    case "ArrowDown":
      return "↓";
    case "Return":
      return "Enter";
    case "Escape":
      return "Esc";
    case "ForwardDelete":
      return "Del";
    default:
      return code;
  }
}

export function formatHotkeyLabel(config: HotkeyConfig | null): string {
  const keys = configToChipKeys(config);
  if (keys.length === 0) return "—";
  return keys.join(" + ");
}
