# OpenWhisper — Claude working notes

## Knowledge management

Prefer **our own project skills** (under `apps/tauri/.claude/skills/`, e.g. `platform-gotchas/`) over machine-local memory for any knowledge that should travel with the repo or across machines. Never write into third-party skills (e.g. `playwright-cli`) — those may be regenerated from upstream and lose local edits; for tool-specific knowledge create a new skill or reference file of our own that links across to the third-party skill.

Machine-local memory is reserved for knowledge that is **specific to the machine or user account** and would be wrong (or simply useless) on another box. Examples that belong in memory: this machine's username triggering a specific encoding bug, the corporate account having no admin credentials, this dev box being an RDP session with a virtual mic, the local shell needing a `~/.bashrc` shim. Examples that do NOT belong in memory: project values, architecture rules, build conventions, model quirks, task-tracking conventions — those go in skills or `docs/`. If a memory entry would still be true on a fresh checkout on a different machine, it's in the wrong place.

## Verifying changes

When changes touch the React app, the rebind UI, or any flow covered by `apps/tauri/tests/*.spec.ts`, run the Playwright suite and confirm it passes — don't read the test file and infer that the existing assertions are still satisfied. If browsers are missing locally, install once with `pnpm exec playwright install chromium` then `pnpm exec playwright test`.
