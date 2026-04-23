---
id: TASK-1
title: Scaffold macOS SwiftUI app shell
status: Done
assignee: []
created_date: '2026-04-22 21:11'
updated_date: '2026-04-23 18:16'
labels:
  - macos
  - setup
dependencies: []
priority: high
ordinal: 6000
---

## Description

<!-- SECTION:DESCRIPTION:BEGIN -->
Create Xcode project for OpenWhisper macOS. SwiftUI + AppKit hybrid. Min target macOS 14 (for latest CoreML + Accessibility APIs). Set up signing team, bundle ID, entitlements plist.
<!-- SECTION:DESCRIPTION:END -->

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Xcode project builds and launches empty app
- [x] #2 Entitlements include microphone access + Accessibility usage description in Info.plist
- [x] #3 App icon placeholder in place
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
Xcode project generated via xcodegen from apps/macos/project.yml. Build succeeds (Debug, arm64, macOS 14 target). Info.plist carries NSMicrophoneUsageDescription + NSAppleEventsUsageDescription. Empty AppIcon.appiconset in place as placeholder. Generated .xcodeproj is gitignored — reproduce with scripts/bootstrap.sh.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Scaffolded via xcodegen + SwiftUI @main app. Links Rust staticlib through swift-bridge bridging header. BUILD SUCCEEDED verified by xcodebuild; Rust FFI symbols (hello_from_rust, core_version) confirmed present in OpenWhisper.debug.dylib via nm.
<!-- SECTION:FINAL_SUMMARY:END -->
