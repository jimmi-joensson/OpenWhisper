// Sidebar items mirror the design (project_recognizer_tauri / screens.jsx
// SettingsSidebarLayout). Order matters — General is the landing pane.
export const SETTINGS_PANES = [
  { id: "general", label: "General", icon: "⚙" },
  { id: "audio", label: "Audio", icon: "🎙" },
  { id: "models", label: "Models", icon: "◆" },
  { id: "shortcuts", label: "Shortcuts", icon: "⌘" },
] as const;

export type SettingsPaneId = (typeof SETTINGS_PANES)[number]["id"];
