---
id: doc-27
title: PR-gate CI workflow — implementation plan
type: plan
created_date: '2026-05-04 15:43'
---

# PR-gate CI workflow — implementation plan

**Backlog parent:** TASK-82
**Spec:** `backlog/docs/specs/doc-26 - PR-gate-CI-workflow-—-design.md`
**Milestone:** m-1 — v1.0 public release readiness

## Ordering

```
1 ── 2 ── 4
     │
     3 ── 4
          │
          5 (manual maintainer step, can land any time)
```

- Task 1 (workflow skeleton) lands first — every other task edits the same file.
- Tasks 2 and 3 can run in parallel after Task 1 (different jobs in the same file; merge conflicts are easy to resolve).
- Task 4 (caching) lands once both gates are working un-cached, so cache misses are observable as runtime improvements.
- Task 5 (branch-protection doc) is paperwork; can land any time but is most useful after Tasks 1–4 are stable so the doc names the actual job IDs.

**External dependency:** TASK-81.4 (CLI scaffold) and TASK-81.9 (CLI smoke test) should land before Task 2 ships, so `cargo test --workspace` has the smoke to run. If TASK-81.9 isn't ready, Task 2 still ships — `cargo test` will pass with whatever tests exist — and TASK-81.9's smoke gets gated automatically once it lands.

## Task 1: `.github/workflows/ci.yml` skeleton + triggers

Land the workflow file with three jobs, all currently stub no-ops, plus the trigger and concurrency boilerplate:

```yaml
name: CI

on:
  pull_request:
    branches: [main]
  push:
    branches: [main]
  workflow_dispatch:

concurrency:
  group: ci-${{ github.ref }}
  cancel-in-progress: true

jobs:
  rust-gate-mac:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - run: echo "rust-gate-mac stub"
  rust-gate-win:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
      - run: echo "rust-gate-win stub"
  frontend-gate-mac:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - run: echo "frontend-gate-mac stub"
```

Land in a PR; verify the three jobs run and post status checks. (This PR will spend ~3 min weighted per run; cheap.)

**Outcomes:**
- `.github/workflows/ci.yml` committed.
- A PR opening triggers three GitHub status checks named `rust-gate-mac`, `rust-gate-win`, `frontend-gate-mac`.
- All three pass on the stub.
- `concurrency` group cancels stale runs when new commits land on the same PR branch.

**Verification:** Open the PR that lands this commit; visually confirm the three status checks appear and pass; check the Actions tab shows the workflow name "CI".

## Task 2: Rust gate — `cargo fmt --check`, `clippy`, `cargo test`

Fill in `rust-gate-mac` and `rust-gate-win` with the actual Rust gates. Both jobs share the same step list — extract via a YAML anchor or just duplicate (5 steps × 2 jobs = trivial copy).

Steps per job (in order):

1. `actions/checkout@v4`
2. `pnpm/action-setup@v4` with `version: 10` (matches the dev box; lockfile is v9-compatible so v10 is the safer pin).
3. `actions/setup-node@v4` with `node-version: 20`, `cache: 'pnpm'`, `cache-dependency-path: apps/tauri/pnpm-lock.yaml`.
4. `dtolnay/rust-toolchain@stable` with `components: rustfmt, clippy`.
5. **ort provisioning** — `pnpm --dir apps/tauri setup:ort` (the script lives at `apps/tauri/scripts/fetch-ort.cjs`; the only `package.json` in the repo is `apps/tauri/package.json`, so running pnpm from the workspace root would fail with "no project found"). Provisions `~/.cache/openwhisper/onnxruntime/` with the ort dylib. **Mac and Windows both** — sherpa-onnx via ort runs on both via the load-dynamic path.
6. `cargo fmt --all --check`
7. `cargo clippy --workspace --all-targets --exclude bench-sherpa --no-default-features --features tauri -- -D warnings`
8. `cargo test --workspace --exclude bench-sherpa --no-default-features --features tauri`

Why those flag choices:

- **`--exclude bench-sherpa`** — `Cargo.toml` lists `scripts/bench/bench-sherpa` as a workspace member; it links a CUDA-enabled ONNX runtime. Per the spec's Non-goals and the `openwhisper-dev-workflow` skill, bench-sherpa never builds in CI. Without `--exclude` the workspace test would either fail for missing CUDA libs or balloon the `target/` cache.
- **`--no-default-features --features tauri`** — `openwhisper-core`'s default features include `macos-shell`, which depends on `swift-bridge` / `swift-bridge-build`. On `windows-latest` there is no Swift toolchain and the build would fail. `--no-default-features` strips `macos-shell`; `--features tauri` adds it back without dragging Swift-bridge in. Mac is fine either way; using the same flag set on both runners keeps the YAML symmetric.
- **`--all-targets`** on clippy makes lints run against tests + examples too, not just the lib targets.

Notes for the YAML:
- On Windows, `cargo` and `clippy` may need `linker = "lld-link"` if MSVC linker drama appears; default MSVC should work for the workspace as-is. If linker errors emerge, document in branch-protection.md as a known workaround.

**Outcomes:**
- `rust-gate-mac` runs `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --exclude bench-sherpa --no-default-features --features tauri -- -D warnings`, and `cargo test --workspace --exclude bench-sherpa --no-default-features --features tauri` — all green on a clean checkout of `main`.
- `rust-gate-win` runs the identical flag set and passes — `swift-bridge` is excluded by `--no-default-features`, bench-sherpa's CUDA libs never get pulled.
- Introducing a `cargo fmt` violation locally and pushing it to a PR turns the corresponding job red.
- Introducing a `clippy::collapsible_if` warning locally and pushing it turns the job red.
- TASK-81.9's CLI smoke test (once landed) runs as part of `cargo test --workspace` on both runners.
- pnpm is invoked via `pnpm --dir apps/tauri setup:ort` so it finds the only `package.json` in the repo.

**Verification:** Push a deliberate `cargo fmt` violation to a throwaway branch; PR; confirm `rust-gate-mac` and `rust-gate-win` go red. Revert; confirm green. Repeat for a clippy violation.

## Task 3: Frontend gate — `pnpm install`, typecheck, Playwright

Fill in `frontend-gate-mac` with the frontend gates:

Steps (in order):

1. `actions/checkout@v4`
2. `pnpm/action-setup@v4` with `version: 10` (match the dev box)
3. `actions/setup-node@v4` with `node-version: 20`, `cache: 'pnpm'`, `cache-dependency-path: apps/tauri/pnpm-lock.yaml`
4. `defaults: run: working-directory: apps/tauri` at the job level (so all subsequent steps run from the right dir)
5. `pnpm install --frozen-lockfile`
6. `pnpm exec playwright install chromium --with-deps` (install only chromium to keep cache lean)
7. `pnpm exec tsc --noEmit` (typecheck)
8. `pnpm test:ui` (Playwright)

Notes:
- `--frozen-lockfile` will fail CI if `pnpm-lock.yaml` is out of sync with `package.json`. That's intentional — fail loud rather than silently bumping deps.
- The `playwright install --with-deps` step pulls system libs on Linux; on macOS no extra deps needed but the flag is a no-op.
- If `pnpm test:ui` flakes on shared macOS runners, bump `playwright.config.ts` `expect.timeout` and `use.actionTimeout` rather than retry-loops.

**Outcomes:**
- `frontend-gate-mac` runs `tsc --noEmit` and `playwright test` from `apps/tauri/` on every PR.
- A `tsc` error in any `.ts` / `.tsx` under `apps/tauri/src/` turns the job red.
- A failing Playwright spec under `apps/tauri/tests/*.spec.ts` turns the job red.
- `pnpm-lock.yaml` drift (committing `package.json` without regenerated lockfile) turns the job red.

**Verification:** Introduce a deliberate `tsc` error (e.g. `const x: number = "string"`); confirm red. Revert. Modify a Playwright spec to assert the wrong text; confirm red. Revert.

## Task 4: Cache strategy

Add caching steps to all three jobs to bring warm-cache runtime to ~5 min/job. Cache layers:

- **cargo registry + `target/`** — use `Swatinem/rust-cache@v2` (the de-facto Rust CI cache action). It handles cargo registry, git db, and target eviction logic correctly without us hand-rolling key derivation. Pass `workspaces: ". -> target"` and `cache-on-failure: true`. Add to both `rust-gate-mac` and `rust-gate-win` *after* `dtolnay/rust-toolchain@stable` and *before* the cargo invocations.
- **pnpm store** — handled by `actions/setup-node@v4` with `cache: 'pnpm'` + `cache-dependency-path: apps/tauri/pnpm-lock.yaml`. No separate cache step.
- **`~/.cache/openwhisper/onnxruntime/`** — `actions/cache@v4` keyed on the literal `1.22.0` string from `apps/tauri/scripts/fetch-ort.cjs` (the `OPENWHISPER_ORT_VERSION` env var). Cache the directory. Restore step runs *before* `pnpm --dir apps/tauri setup:ort` so a hit makes setup a no-op.
- **Playwright browsers** — `actions/cache@v4` keyed on `apps/tauri/pnpm-lock.yaml` hash + literal `chromium`. Path: `~/Library/Caches/ms-playwright` (macOS — the only Playwright runner). Restore step runs before `playwright install`.

For each non-Swatinem cache, set:
- `key`: deterministic, content-hash + version-constant where appropriate (so a version bump auto-busts the cache).
- `restore-keys`: a fallback prefix so a partial hit is still useful when the primary key shifts.

**Outcomes:**
- Warm-cache CI run (no source changes that bust cache) finishes in ~15 min wall (sum of three jobs running in parallel).
- Cold-cache run finishes in ~40 min wall.
- Bumping `OPENWHISPER_ORT_VERSION` in `fetch-ort.cjs` invalidates the ort cache automatically (because the version is in the cache key).
- Bumping a dep in `Cargo.toml` invalidates the cargo registry + target cache automatically.

**Verification:** Trigger a fresh PR; confirm in the Actions UI that each cache step shows "Cache restored from key: ..." (not "Cache not found"). Total wall time should drop noticeably between the first run on a new branch and the second.

## Task 5: Branch-protection doc — `docs/maintainer/branch-protection.md`

Author a doc that tells the maintainer which boxes to tick in GitHub Settings → Branches → Branch protection rule for `main`, once the repo is public-flipped (TASK-NEW-5). The doc is procedural; no code.

Cover:

1. Rule pattern: `main`.
2. Require a pull request before merging.
   - Require approvals: 1.
   - Dismiss stale approvals when new commits are pushed: yes.
3. Require status checks to pass before merging.
   - Required checks: `rust-gate-mac`, `rust-gate-win`, `frontend-gate-mac`.
   - Require branches to be up to date before merging: yes.
4. Require conversation resolution before merging: yes.
5. Restrict who can push to matching branches: leave blank for v1 (single maintainer).
6. Allow force pushes: no.
7. Allow deletions: no.

Plus a short troubleshooting section:

- "I made a typo in `package.json` and CI keeps failing on `--frozen-lockfile`" → run `pnpm install` locally, commit the regenerated `pnpm-lock.yaml`.
- "ort cache returned a partial restore" → bump `OPENWHISPER_ORT_VERSION` env in the workflow file as a forced miss, or manually clear the cache via the Actions UI.
- "Cold-cache CI runs after a few weeks of inactivity" → GitHub's per-repo cache cap is **10 GB total** across all caches, evicted by LRU. Caches older than 7 days also auto-expire. Nothing to do but accept the occasional cold run.

**Outcomes:**
- `docs/maintainer/branch-protection.md` committed.
- The doc names the three required status checks exactly as they appear in Tasks 2 and 3.
- Maintainer can follow the doc top-to-bottom without referencing this plan.
- Troubleshooting section addresses the three CI failure modes most likely to surprise a contributor.

**Verification:** Reviewer reads the doc and confirms a non-author maintainer could apply the settings without questions.

## Cross-task verification checklist

Before marking TASK-82 done:

- [ ] All 5 subtasks `Done` in Backlog.
- [ ] `.github/workflows/ci.yml` exists and passes on a clean `main` checkout.
- [ ] Three GitHub status checks (`rust-gate-mac`, `rust-gate-win`, `frontend-gate-mac`) appear on every PR to `main`.
- [ ] Each gate has been demonstrated to fail correctly (deliberate fmt/clippy/tsc/Playwright violation goes red).
- [ ] Warm-cache PR run finishes in <20 min wall time.
- [ ] `docs/maintainer/branch-protection.md` published.
- [ ] No CI workflow runs `pnpm tauri build` (release CI's job, not PR-gate's).
- [ ] No `--all-features` cargo invocation (would break on missing DirectML/CUDA).
