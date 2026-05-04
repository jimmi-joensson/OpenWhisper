---
id: TASK-85.5
title: 'Plan Task 5: Windows bundle rename'
status: To Do
assignee: []
created_date: '2026-05-04 16:36'
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
Update tauri.conf.json bundle.windows.wix.productName + verify upgradeCode is a stable UUID (not name-derived — would fork install paths on upgrade). Verify %APPDATA% path code uses dirs::data_dir() not hardcoded 'OpenWhisper'. Update vendor-natives.cjs if it hardcodes openwhisper-tauri.exe filename. MSI builds clean; Add/Remove Programs label correct.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 MSI builds via pnpm tauri build produce <NEW_NAME>-0.99.0-x64.msi
- [ ] #2 Add/Remove Programs shows new name on test Windows install
- [ ] #3 %APPDATA%\<new>\ created on first launch
- [ ] #4 vendor-natives.cjs works without referencing old exe name
- [ ] #5 tauri.conf.json carries an explicit bundle.windows.wix.upgradeCode UUID block (currently absent — added by this commit and immutable thereafter)
<!-- AC:END -->
