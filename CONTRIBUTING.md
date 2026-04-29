# Contributing to OpenWhisper

Thanks for your interest in OpenWhisper. Contributions are welcome.

## Before you start

- **Read the README** for project goals, the stack, and how to build from
  source.
- **Look at `backlog/tasks/`** to see what work is already planned or in
  flight. OpenWhisper tracks work in-repo via
  [Backlog.md](https://github.com/MrLesk/Backlog.md), not via GitHub
  Issues. Run `backlog board` (after installing the CLI) for a kanban
  view.
- **For bug reports and feature requests from outside the project**, open
  a GitHub Issue. We'll triage and either convert it into a backlog task
  or close it with an explanation.

## Pull requests

When you open a pull request, a short legal boilerplate is pre-filled in
the PR description. **Leave that section intact.** It confirms that the
project may use, modify, distribute, and re-license your contributions
under whatever terms make sense in the future. Without it, we can't
merge.

The full text lives in
[`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md).

If you can't agree to the boilerplate (for example, your employer holds
the rights to your work), please reach out before opening the pull
request so we can figure out the right path.

## Workflow

1. Fork the repo and create a feature branch from `main`.
2. Build and run locally per the README. The `pnpm dev:tauri` flow on
   macOS is the recommended dev loop.
3. Make your change. Keep diffs focused — one logical change per pull
   request.
4. Add or update tests where they exist. The Playwright suite under
   `apps/tauri/tests/*.spec.ts` covers the React app and rebind UI; run
   `pnpm test:ui` from `apps/tauri/` before submitting.
5. Open a pull request against `main`. Reference any related backlog task
   in the PR description (e.g. "TASK-42"). Keep the legal boilerplate
   intact.

## Commit hygiene

- Conventional commit-style prefixes are not required, but a clear,
  imperative subject line is appreciated ("Fix mic-permission probe on
  Sonoma" rather than "various fixes").
- Avoid bundling unrelated changes. If you find a drive-by fix, prefer
  splitting it into its own pull request.

## Reporting security issues

For now, please open a private security advisory via GitHub
(`Security` → `Report a vulnerability`). A `SECURITY.md` with a more
formal process will land alongside the first signed release.

## Code of Conduct

Be respectful. Disagree on technical merits, not on people. Anyone
behaving in a way that makes the project less welcoming will be asked to
stop, and may be blocked from the project at the maintainer's discretion.
