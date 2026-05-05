---
id: TASK-85.7
title: 'Plan Task 7: Settings/data migration shim'
status: To Do
assignee: []
created_date: '2026-05-04 16:36'
labels:
  - 85-impl
dependencies: []
parent_task_id: TASK-85
milestone: m-1
priority: high
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Idempotent one-time migration on app boot. New core/src/settings/migration.rs: detect old ~/Library/Application Support/com.openwhisper.app/ (Mac) or %APPDATA%\openwhisper\ (Win), copy contents to new path, write atomic marker '.migrated-from-openwhisper'. Wire into lib.rs::setup() before settings store touches. File v1.1 follow-up to remove migration code after 60-day grace period.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [ ] #1 core/src/settings/migration.rs ships with migrate_legacy_settings_dir() that returns AlreadyMigrated / NoLegacyData / Migrated
- [ ] #2 Atomic marker write ensures double-launch doesn't double-migrate
- [ ] #3 Verbose log records migration event at INFO level
- [ ] #4 Unit test covers (a) no legacy dir → no-op, (b) legacy dir → copied, (c) marker exists → no-op
- [ ] #5 Mirror behavior on Windows for %APPDATA%
- [ ] #6 v1.1 follow-up Backlog task filed: remove migration code after 60-day grace period
<!-- AC:END -->
