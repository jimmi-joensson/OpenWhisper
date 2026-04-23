---
id: TASK-8
title: Text injection via Accessibility + CGEventPost
status: Done
assignee: []
created_date: '2026-04-22 21:11'
updated_date: '2026-04-23 18:16'
labels:
  - macos
  - integration
dependencies: []
priority: high
ordinal: 2000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Inject transcribed text into the focused app's text field. Prefer AX API (AXUIElementSetAttributeValue on kAXSelectedTextAttribute). Fallback to CGEventPost synthetic keystrokes for apps without AX text support.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Text appears in Notes, Safari, VSCode, Slack, Terminal
- [x] #2 Handles Unicode + emoji correctly
- [x] #3 Does not disturb undo history beyond one entry where possible
- [x] #4 Prompts user for Accessibility permission with clear rationale
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
2026-04-23 runtime verified end-to-end. User dictated a multi-sentence message into Claude Code's chat input via Right Command toggle; transcript pasted at cursor without touching focus. Pasteboard-save-restore preserves prior clipboard. Implementation chose Cmd+V over AX kAXSelectedTextAttribute because the latter silently fails in Electron/Chromium/Terminal and we wanted universal behavior.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Full dictation loop works: Right Command → record (with live meter) → Right Command → Parakeet transcribe on ANE → Cmd+V paste into focused app. User confirmed by dictating a message directly into the Claude Code chat prompt.
<!-- SECTION:FINAL_SUMMARY:END -->
