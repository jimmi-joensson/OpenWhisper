---
id: TASK-85.4
title: 'Plan Task 4: Mac bundle rename + notarization smoke'
status: To Do
assignee: []
created_date: '2026-05-04 16:35'
updated_date: '2026-05-04 16:40'
labels:
  - 85-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-85
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Update tauri.conf.json + tauri.dev.conf.json: productName, bundle.identifier (com.openwhisper.app → com.<new>.app), main window title. Signing identity stays (Team ID 898R9M89GU = trust anchor). Update version_reset.rs tccutil paths. Update notarize-mac.cjs / dev-run.sh hardcoded refs if any. Smoke notarization on 0.99.0-rename-smoke before tagging v1.0.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 tauri.conf.json + tauri.dev.conf.json: productName=<NEW_NAME>, bundle.identifier=com.<new>.app
- [ ] #2 Mac signing identity reference unchanged; Team ID 898R9M89GU preserved
- [ ] #3 0.99.0-rename-smoke build signs + notarizes successfully via pnpm release:mac && pnpm notarize:mac
- [ ] #4 Smoke install on test Mac: grant Accessibility + Mic, hotkey works, transcribe passes, ~/Library/Application Support/com.<new>.app/ created
- [ ] #5 version_reset.rs requires no edit (reads bundle id dynamically from app.config().identifier:95) — verified
- [ ] #6 Legacy-bundle-id TCC cleanup runs once on first launch under new bundle id (gated by Task 7 migration marker), clearing com.openwhisper.app rows from System Settings
- [ ] #7 notarize-mac.cjs DMG filename pattern flows from new productName; keychain profile renamed to <new>-notarytool (maintainer pre-stores credential before PR ships)
<!-- AC:END -->
