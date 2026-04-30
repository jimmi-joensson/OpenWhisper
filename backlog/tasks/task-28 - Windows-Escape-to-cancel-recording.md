---
id: TASK-28
title: Windows Escape-to-cancel recording
status: Won't Do
assignee: []
created_date: '2026-04-24 18:45'
updated_date: '2026-04-30 16:32'
labels:
  - windows
  - input
dependencies: []
priority: medium
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Windows equivalent of Mac's global Escape-to-cancel-while-recording. On
Mac, Escape posted to the event tap fires `.openWhisperCancelDictation`;
DictationService decides whether to act based on phase.

Originally planned to ride a TASK-27 low-level hook, but TASK-27 is now
Won't Do (Windows keeps Ctrl+Space chord, no tap-not-hold port). So this
task owns its own minimal `WH_KEYBOARD_LL` hook — Escape-only, dedicated
thread with a private message pump, never swallows the key. Phase-gating
stays in the Rust core (Cancel is a no-op outside of Recording).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Escape pressed during recording cancels the session (discards audio, returns to idle, no paste)
- [x] #2 Escape pressed in any other phase passes through to the focused app (hook always calls CallNextHookEx)
- [x] #3 Hook scoped to Escape only — not a general keyboard-input service
- [x] #4 Hook runs on a dedicated thread with its own message pump so main-thread stalls don't drop events
- [x] #5 Gating lives in the Rust core (`Core.RequestCancel` is phase-aware), not in the hook callback
- [x] #6 Graceful fallback if SetWindowsHookEx fails (AV blocks it) — app keeps working, Escape-to-cancel just disabled
<!-- AC:END -->

## Implementation Plan

<!-- SECTION:PLAN:BEGIN -->
1. `apps/windows/OpenWhisper/Hotkey/EscapeHook.cs`: installs `SetWindowsHookExW(WH_KEYBOARD_LL, …)` on a background thread, runs a `GetMessage`/`TranslateMessage`/`DispatchMessage` pump, exits on `WM_QUIT`.
2. Hook callback: if nCode ≥ 0 and msg is WM_KEYDOWN/WM_SYSKEYDOWN and vkCode == VK_ESCAPE, fire `EscapePressed` event. Always return `CallNextHookEx` — never swallow.
3. MainWindow subscribes, marshals to dispatcher, calls `_service.Cancel()` which forwards to Rust `Core.RequestCancel()` (phase-gated on the Rust side).
4. Dispose: PostThreadMessage WM_QUIT, join thread (500 ms timeout), UnhookWindowsHookEx inside the thread as it exits.
<!-- SECTION:PLAN:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Closed during 2026-04-30 backlog review as Won't Do. WinUI 3 EscapeHook.cs superseded by Tauri Rust Escape hook (TASK-35 Phase 4). ACs were checked against the WinUI 3 implementation; that codebase now lives in archive/.
<!-- SECTION:FINAL_SUMMARY:END -->
