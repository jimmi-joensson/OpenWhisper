#!/bin/bash
# Branch discipline — PreToolUse hook on Bash.
#
# Blocks `git commit` / `git push` / `git merge` when HEAD is on
# `main` or `master`. Pairs with the openwhisper-branch-discipline
# skill which explains the why.
#
# Hook contract:
# - stdin: JSON event from Claude Code (Bash PreToolUse).
# - exit 0  → allow the tool call.
# - exit 2  → deny; stderr is shown to Claude as the denial reason.

set -euo pipefail

# Read the event. Fail open on any parsing error — never block legitimate
# work because the hook itself broke.
INPUT=$(cat)

# Require jq. If absent, fail open (with a stderr breadcrumb so the
# missing dep is visible).
if ! command -v jq >/dev/null 2>&1; then
  echo "[branch-discipline] jq not found; skipping check" >&2
  exit 0
fi

CMD=$(echo "$INPUT" | jq -r '.tool_input.command // ""' 2>/dev/null || echo "")
if [ -z "$CMD" ]; then
  exit 0
fi

# Match `git commit`, `git push`, `git merge` as standalone subcommands.
# Word-boundaries matter — don't match `git commit-msg-helper` etc.
if ! echo "$CMD" | grep -qE '(^|[[:space:]&;|])git[[:space:]]+(commit|push|merge)([[:space:]]|$)'; then
  exit 0
fi

# Resolve repo + branch. Fail open outside a git repo or in detached HEAD.
REPO_ROOT=$(git rev-parse --show-toplevel 2>/dev/null || true)
if [ -z "$REPO_ROOT" ]; then
  exit 0
fi
BRANCH=$(git -C "$REPO_ROOT" rev-parse --abbrev-ref HEAD 2>/dev/null || echo "")
if [ -z "$BRANCH" ] || [ "$BRANCH" = "HEAD" ]; then
  exit 0
fi

case "$BRANCH" in
  main|master)
    cat >&2 <<EOF
Branch discipline: HEAD is '$BRANCH'.

The command "$CMD" is blocked because changes to main / master must go
through a feature branch + pull request.

Next step:
  git checkout -b <slug>     # task-<N>.<M>-<name> | session-YYYY-MM-DD-<name> | fix-<name>

Then retry. For parallel work, use a worktree instead of branch-switching:
  git worktree add ../OpenWhisper-<slug> -b <slug>

See .claude/skills/openwhisper-branch-discipline/SKILL.md for the full
rule + worktree usage.
EOF
    exit 2
    ;;
esac

exit 0
