---
id: TASK-78.6
title: 'Plan Task 6: Report on GitHub button (rescoped from opt-in upload)'
status: Done
assignee:
  - '@claude'
created_date: '2026-05-04 06:16'
updated_date: '2026-05-07 22:22'
labels:
  - 78-impl
milestone: m-1
dependencies: []
parent_task_id: TASK-78
ordinal: 39000
---

## Acceptance Criteria
<!-- AC:BEGIN -->
- [x] #1 Upload button is omitted (not disabled) when no endpoint is configured
- [x] #2 Upload AlertDialog renders the resolved endpoint in a mono code-card AND an Includes/Excludes block AND a 'Don't ask again for this endpoint' checkbox
- [x] #3 Checking 'don't ask again' persists in settings.upload_dialog_suppressed_endpoints and skips the dialog for that exact endpoint string on subsequent uploads
- [x] #4 After successful upload, the sheet footer shows 'Uploaded · just now' mono label in place of the Upload button (no re-upload affordance)
- [x] #5 state.json records uploaded_at on success; failed upload surfaces a toast, leaves the file and button alone
- [x] #6 Sheet footer renders a 'Report on GitHub' ghost button next to 'Open crash folder' (replacing the 78.4-era Upload placeholder)
- [ ] #7 Clicking the button opens https://github.com/jimmi-joensson/OpenWhisper/issues/new with prefilled title, body, and labels=bug,crash via the platform default browser
- [ ] #8 Body is the same redacted markdown report formatCrashAsMarkdown produces for the Copy GitHub-ready report flow — single source of truth, no separate formatter
- [ ] #9 Body is truncated to fit GitHub's URL length cap (~6 KB) with a trailing 'Truncated — use Copy GitHub-ready report for the full body' marker; full Copy flow stays available as the fallback
- [ ] #10 openwhisper crash-dump --github-url prints the URL to stdout for the latest crash or for --id <ID>; honours --json by emitting { url }
- [ ] #11 Vitest covers the URL builder (title + body composition, label list, encoding, truncation); Playwright asserts the button click invokes the opener with the expected URL
<!-- AC:END -->

## Implementation Notes

<!-- SECTION:NOTES:BEGIN -->
6e91288 Report on GitHub button + CLI parity (rescoped from opt-in upload). Rust + TS URL builders share an identical shape; --github-url printable from CLI.
<!-- SECTION:NOTES:END -->

## Final Summary

<!-- SECTION:FINAL_SUMMARY:BEGIN -->
Rescoped TASK-78.6 from configurable HTTPS upload to a one-click 'Report on GitHub' button. Sheet footer renders the button alongside Open crash folder; click invokes tauri-plugin-opener's open_url with a prefilled GitHub Issues URL at jimmi-joensson/OpenWhisper. Body is formatCrashAsMarkdown's redacted output, byte-budget-truncated at 6 KB with a 'Truncated — use Copy GitHub-ready report' marker; identity block always preserved. URL builder ported to Rust (core::crashes::build_github_issue_url + format_as_markdown) so the CLI's new --github-url flag prints the same URL the React side opens. form_urlencode mirrors URLSearchParams so both implementations emit byte-identical output. 8 new cargo tests + 7 Vitest tests + 1 Playwright case + CLI smoke green; 112/112 Playwright + 14/14 Vitest project-wide. Original opt-in-upload design preserved in git history; doc-22 + doc-23 rescoped in place. Commit 6e91288.
<!-- SECTION:FINAL_SUMMARY:END -->
