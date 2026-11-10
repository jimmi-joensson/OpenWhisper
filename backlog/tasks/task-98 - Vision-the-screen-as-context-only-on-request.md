---
id: TASK-98
title: Vision - the screen as context, only on request
status: To Do
assignee: []
created_date: '2026-11-10T14:36:00Z'
updated_date: '2026-11-10T15:33:00Z'
labels:
  - vision
  - ask
  - core
  - cli
  - tauri
dependencies:
  - TASK-97
priority: medium
ordinal: 80000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Let an ask include a screenshot so the user can refer to what is on screen ('look at my screen', 'answer this message'). Whole screen, not only the focused window. Never in dictation mode, never in the background.

When a screenshot is taken is a setting (decided 2026-11-10, wording open):
- Off.
- Phrases: a user-editable list, managed like dictionary terms, in English and Danish. Matched locally on the transcript before anything leaves the machine.
- Smart: phrases, or the model says the request needs the screen.
- Always: every ask includes the screen.

Every capture is visible on the pill and marked on the history entry. The image is held in memory only and dropped after the request.

Needs: screen capture on macOS (Screen Recording permission) and Windows, image input on the backends (check that the Claude CLI accepts an image the way we call it; the OpenAI-compatible API does), the trigger setting and phrase list, and a list of apps that must never be captured (password managers, banking).

Open points: which display on a multi-monitor setup (proposal: the one with the focused window); how Smart asks for the screen (a first text-only call that can answer 'need screen', or a tool the model calls).
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 No screenshot is ever taken during dictation or without an ask in progress
- [ ] #2 Trigger setting with Off, Phrases, Smart and Always; default is Phrases; the phrase list is editable and ships with English and Danish defaults
- [ ] #3 Phrase matching runs locally on the transcript
- [ ] #4 Each capture is shown on the pill while it happens and recorded on the history entry
- [ ] #5 The image is never written to disk
- [ ] #6 Apps on the never-capture list block the capture and the answer says so
- [ ] #7 macOS permission flow is clear; Windows path verified on a real machine before leaving In Review
- [ ] #8 Headless parity: the CLI can run an ask with a screenshot
- [ ] #9 A password manager's windows never reach a screenshot: listed apps are left out of the image itself (not captured and then hidden), on macOS and Windows, with an eval that checks the pixels
- [ ] #10 When the focused app is on the never-capture list no screenshot is taken at all, and the answer says so
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Requirement from the user (2026-11-10): the tool must never read from a password manager; in a whole-screen capture its window has to be gone, like the phone setting that blacks an app out in screenshots.

Findings so far (web search, not yet tried in code):
- That phone-style protection is set by the app that owns the window. 1Password has no such setting on the desktop as far as found (open feature requests on its forum; its official release notes were not checked). On macOS it would not help anyway: since 15.4 a window that asks to be excluded is still captured by ScreenCaptureKit, and an Apple engineer states there is no public API to prevent capture.
- So the protection has to be on our side, where the screenshot is taken. macOS: ScreenCaptureKit can capture a display while excluding chosen applications (SCContentFilter display:excludingApplications:exceptingWindows:), so their windows are not in the image. Windows: no equivalent found for a whole-monitor capture; plan is to paint over the rectangles of the listed apps' windows before the image leaves the process (covers more than needed when a window is partly hidden, which is the safe side).
- Second layer on both: paint over the listed apps' window rectangles after capture as well, in case exclusion did not apply.
- Default list: 1Password, Bitwarden, KeePassXC, LastPass, Dashlane, Apple Passwords / Keychain Access; editable.
- Not covered by an app list, and to be said plainly in the settings: a password manager's browser extension popup (it is part of the browser window), a password shown on a web page or in a terminal.
History is left as it is: the user may dictate into a password manager's notes field, and that text is theirs to keep.
<!-- SECTION:NOTES:END -->
