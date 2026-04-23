---
id: TASK-8
title: Text injection via Accessibility + CGEventPost
status: In Progress
assignee: []
created_date: '2026-04-22 21:11'
updated_date: '2026-04-23 06:41'
labels:
  - macos
  - integration
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Inject transcribed text into the focused app's text field. Prefer AX API (AXUIElementSetAttributeValue on kAXSelectedTextAttribute). Fallback to CGEventPost synthetic keystrokes for apps without AX text support.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Text appears in Notes, Safari, VSCode, Slack, Terminal
- [ ] #2 Handles Unicode + emoji correctly
- [ ] #3 Does not disturb undo history beyond one entry where possible
- [ ] #4 Prompts user for Accessibility permission with clear rationale
<!-- AC:END -->
