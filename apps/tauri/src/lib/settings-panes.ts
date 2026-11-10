// Sidebar items mirror the design (project_recognizer_tauri / screens.jsx
// SettingsSidebarLayout). Order matters — General is the landing pane.
// Icons live in sidebar-nav.tsx (this is a pure data module).
export const SETTINGS_PANES = [
  { id: "general", label: "General" },
  { id: "audio", label: "Audio" },
  { id: "models", label: "Models" },
  { id: "shortcuts", label: "Shortcuts" },
  { id: "dictionary", label: "Dictionary" },
  { id: "refine", label: "Refine" },
  { id: "modes", label: "Modes" },
] as const;

export type SettingsPaneId = (typeof SETTINGS_PANES)[number]["id"];
