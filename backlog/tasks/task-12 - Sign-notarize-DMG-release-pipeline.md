---
id: TASK-12
title: Sign + notarize + DMG release pipeline
status: Done
assignee: []
created_date: '2026-04-22 21:12'
updated_date: '2026-05-01 06:10'
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
- [x] #1 `pnpm release:mac` produces a Developer ID-signed, notarized, stapled DMG; `xcrun stapler validate` passes; `spctl -a -t open --context context:primary-signature` passes
- [x] #2 Fresh Mac (no prior install, downloaded DMG with quarantine bit) opens via double-click — no Gatekeeper "can't be opened" dialog
- [x] #3 Hardened runtime present on the bundled `.app` (`codesign -dv` shows `flags=0x10000(runtime)`); CGEventTap installs and global hotkey works after Accessibility grant
- [x] #4 `apps/tauri/src-tauri/Entitlements.plist` checked in with `com.apple.security.device.audio-input` plus any hardened-runtime relaxations actually needed by Wry/WebView2
- [x] #5 `apps/tauri/scripts/sign-mac.cjs` either deleted (if Tauri's own signing path covers it) or replaced with a notarize-only step; `package.json` `release:mac` updated accordingly
- [x] #6 `INSTALL.md` Gatekeeper-bypass section removed; troubleshooting entries about "permissions revoked on updates" and "app is damaged" dropped
- [x] #7 `.claude/skills/openwhisper-releases/SKILL.md` updated — drop the strip-hardened-runtime warning, add notarytool + stapler validate to step 3
- [x] #8 Memory `project_tcc_dev_pain.md` updated to note Release builds now have stable cdhash (Debug ad-hoc drift unchanged)
- [x] #9 Follow-up tasks filed for: GitHub Actions release workflow with cert + notary secrets; Sparkle (or equivalent) autoupdate
<!-- AC:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Mac Release builds now signed with Developer ID Application: Jimmi Joensson (898R9M89GU) + Apple-notarized + stapled. Hardened runtime kept (`flags=0x10000(runtime)`); CGEventTap survives the transition with audio-input + JIT entitlements. The strip-hardened-runtime sign-mac.cjs workaround retired. Quarantine-bit test confirms Gatekeeper accepts the DMG as `Notarized Developer ID` even when xattr-marked as a Safari download. INSTALL.md, openwhisper-releases skill, openwhisper-platform-gotchas (cdhash drift entry), and project_tcc_dev_pain memory updated. Follow-ups split out: TASK-66 (GH Actions release workflow with secrets), TASK-67 (Sparkle/tauri-plugin-updater autoupdate). v0.4.0 release on origin remains ad-hoc; first user-facing notarized release will be v0.5.0.
<!-- SECTION:FINAL_SUMMARY:END -->
