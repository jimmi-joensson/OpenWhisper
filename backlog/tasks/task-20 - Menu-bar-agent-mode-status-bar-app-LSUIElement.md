---
id: TASK-20
title: Menu bar agent mode (status bar app + LSUIElement)
status: To Do
assignee: []
created_date: '2026-04-23 13:46'
labels:
  - macos
  - ui
  - lifecycle
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Make OpenWhisper live as a macOS menu bar extra so the app keeps running after the main window is closed. Dictation, hotkey, pill overlay, and permissions coordinator all stay active. User can re-open the main window, trigger dictation, and quit from a menu attached to a mic icon in the system menu bar.

Rationale: OpenWhisper is a hotkey-driven utility. A dock icon adds visual noise for a process that runs mostly in the background. Shipped competitors (Superwhisper, Whisper Transcription, Raycast) all default to menu-bar-only. This matches user expectation and avoids the 'close window = quit app' foot-gun.

Implementation outline:
- Resources/Info.plist: add LSUIElement = true. Hides dock icon and replaces the app menu. Window-close no longer terminates the app.
- New apps/macos/App/StatusBarController.swift: @MainActor class that owns an NSStatusItem, exposes an NSMenu, rebuilds menu on menuWillOpen (NSMenuDelegate) so current DictationService.phase is reflected. Menu items: Open OpenWhisper / Start or Stop Dictation (state-aware label) / Quit OpenWhisper. Icon: mic.fill SF Symbol, template-rendered so macOS auto-tints.
- OpenWhisperApp: add @State private var statusBar; wire it with the existing DictationService. Add a small MainWindowAccess singleton to bridge SwiftUI's openWindow environment action into the AppKit status-bar callback so 'Open OpenWhisper' re-opens the window after it's been closed.
- Reactive icon: use withObservationTracking to watch DictationService.phase and update the status-item image (e.g. mic.fill → waveform during recording) and the dictation menu item's title.
- First-launch hint: when Dock is hidden the user can miss that the app is running. Flash the status-item briefly or (v2) post a NSUserNotification 'OpenWhisper is in your menu bar'.

Non-goals (separate tasks):
- Settings/preferences window redesign (TASK-11)
- Mode indicator emoji for caveman modes (TASK-18 will add 🪶/🪨/🔥 layering on top of the mic icon)
- Launch-at-login (SMAppService) — part of TASK-11

Acceptance criteria:
- [ ] LSUIElement=true in Info.plist; dock icon not shown at launch
- [ ] mic icon appears in the system menu bar while app is running
- [ ] Closing the main window (red X) does NOT terminate the app; dictation hotkey still works
- [ ] Menu item 'Open OpenWhisper' re-opens the main window and brings app to front
- [ ] Menu item dynamically reflects dictation state (Start vs Stop vs Loading vs Transcribing)
- [ ] Menu item 'Quit OpenWhisper' terminates cleanly
- [ ] Pill overlay behaves unchanged regardless of main-window visibility
- [ ] Permissions coordinator still prompts on first run (TCC does not require dock presence)
<!-- SECTION:DESCRIPTION:END -->
