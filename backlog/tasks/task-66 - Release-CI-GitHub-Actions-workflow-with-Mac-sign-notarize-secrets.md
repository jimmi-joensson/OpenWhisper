---
id: TASK-66
title: 'Release CI: GitHub Actions workflow with Mac sign + notarize secrets'
status: To Do
assignee: []
created_date: '2026-05-01 06:10'
updated_date: '2026-05-04 08:03'
labels:
  - release
  - ci
  - macos
dependencies: []
priority: medium
ordinal: 28000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Replace the local two-machine release flow with a tag-driven GitHub Actions workflow. Mac side: import P12 cert into a temp keychain on the runner, run `pnpm release:mac`, fail closed on notarization errors. Windows side: builds MSI + NSIS as today. Both upload to a draft GH Release on tag push.

Secrets needed in repo settings:
- APPLE_DEVELOPER_ID_CERT_P12_BASE64 (cert + private key, .p12 export)
- APPLE_DEVELOPER_ID_CERT_PASSWORD (.p12 export password)
- APPLE_ID
- APPLE_TEAM_ID = 898R9M89GU
- APPLE_APP_SPECIFIC_PASSWORD

Open during this round and deferred (TASK-12 AC#9). Local pipeline works for v0.4.x via the openwhisper-releases skill.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 Tag push triggers a workflow that builds + signs + notarizes the Mac DMG and builds the Windows MSI + NSIS, attaching all three to a draft GH Release
- [ ] #2 P12 cert + password + Apple ID + team ID + app-specific password live in repo Actions secrets, not in code
- [ ] #3 Workflow fails the build if notarization returns Invalid/Rejected (no silent unsigned releases)
- [ ] #4 Local two-machine playbook in openwhisper-releases skill remains as a fallback documented for when CI is down
<!-- AC:END -->
