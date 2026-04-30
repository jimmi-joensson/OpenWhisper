---
id: TASK-12
title: Sign + notarize + DMG release pipeline
status: To Do
assignee: []
created_date: '2026-04-22 21:12'
updated_date: '2026-04-30 17:26'
labels:
  - macos
  - release
dependencies: []
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Reopened 2026-04-30 — Apple Developer enrollment now active. Move Mac Tauri release builds from ad-hoc to Developer ID Application + notarization, with hardened runtime kept (drops the `sign-mac.cjs` strip workaround). Scope this round: local `pnpm release:mac` produces a signed + notarized + stapled DMG that passes Gatekeeper on a fresh Mac. CI/GitHub Actions automation and Sparkle autoupdate split into follow-up tasks.

Background: ad-hoc + hardened runtime broke `CGEventTapCreate` on Sequoia 15, so `apps/tauri/scripts/sign-mac.cjs` re-signs without `--options runtime` and swaps the re-signed `.app` into the styled DMG. Notarization requires hardened runtime, so this workaround must go. Stable Developer ID identity also fixes TCC grant drift between Release builds.

Plan and step-by-step in conversation 2026-04-30; no plan doc until execution starts.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 `pnpm release:mac` produces a Developer ID-signed, notarized, stapled DMG; `xcrun stapler validate` passes; `spctl -a -t open --context context:primary-signature` passes
- [ ] #2 Fresh Mac (no prior install, downloaded DMG with quarantine bit) opens via double-click — no Gatekeeper "can't be opened" dialog
- [ ] #3 Hardened runtime present on the bundled `.app` (`codesign -dv` shows `flags=0x10000(runtime)`); CGEventTap installs and global hotkey works after Accessibility grant
- [ ] #4 `apps/tauri/src-tauri/Entitlements.plist` checked in with `com.apple.security.device.audio-input` plus any hardened-runtime relaxations actually needed by Wry/WebView2
- [ ] #5 `apps/tauri/scripts/sign-mac.cjs` either deleted (if Tauri's own signing path covers it) or replaced with a notarize-only step; `package.json` `release:mac` updated accordingly
- [ ] #6 `INSTALL.md` Gatekeeper-bypass section removed; troubleshooting entries about "permissions revoked on updates" and "app is damaged" dropped
- [ ] #7 `.claude/skills/openwhisper-releases/SKILL.md` updated — drop the strip-hardened-runtime warning, add notarytool + stapler validate to step 3
- [ ] #8 Memory `project_tcc_dev_pain.md` updated to note Release builds now have stable cdhash (Debug ad-hoc drift unchanged)
- [ ] #9 Follow-up tasks filed for: GitHub Actions release workflow with cert + notary secrets; Sparkle (or equivalent) autoupdate
<!-- AC:END -->
