---
id: TASK-67
title: Sparkle autoupdate (or tauri-plugin-updater) for Mac releases
status: To Do
assignee: []
created_date: '2026-05-01 06:10'
labels:
  - release
  - macos
  - ux
dependencies: []
priority: low
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Mac users currently have to download a fresh DMG manually for each release. Wire Sparkle (or the tauri-plugin-updater equivalent that targets the same appcast format) so the app checks an appcast feed and offers in-app updates.

Prerequisites that landed in TASK-12 (both required for Sparkle to make sense):
- Stable Developer ID identity (Apple Team ID 898R9M89GU)
- Notarized + stapled DMGs

Without those, Sparkle would have to ship updates that re-trigger the Gatekeeper bypass on every install — nonsensical. With them, the auto-update path is invisible to the user.

Open during TASK-12 and deferred (AC#9). Re-evaluate priority after we have ~3 signed releases out and update friction becomes a real complaint. Windows equivalent uses the same plugin via NSIS/MSI delta updates — file as a Win-side task only if/when we cross that bridge.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 App polls an appcast feed (hosted on GitHub Pages or release notes) and surfaces an "Update available" banner without requiring re-grant of TCC permissions
- [ ] #2 EdDSA signing key for the appcast set up; private key in keychain, public key in tauri-plugin-updater config
- [ ] #3 Update workflow tested on Mac end-to-end: install N, ship N+1, update banner appears, click Install, app restarts on N+1, hotkey + grants survive
<!-- AC:END -->
