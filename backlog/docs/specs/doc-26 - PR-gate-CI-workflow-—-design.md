---
id: doc-26
title: PR-gate CI workflow — design
type: spec
created_date: '2026-05-04 15:43'
---

# PR-gate CI workflow — design

**Backlog parent:** TASK-82
**Milestone:** m-1 — v1.0 public release readiness

## Problem

OpenWhisper has zero workflow files in `.github/workflows/` today. Nothing automatically catches:

- A Rust regression on Windows when a Mac contributor opens a PR.
- A clippy warning landed under `-D warnings` discipline.
- A Playwright snapshot drift in `apps/tauri/tests/*.spec.ts`.
- A `pnpm-lock.yaml` divergence between contributor branches and `main`.
- A `cargo fmt` drift.

The whole point of OpenWhisper's split (FluidAudio on Mac, sherpa-onnx on Windows, shared core in between) is two-platform parity. CI is what makes that promise enforceable instead of aspirational.

Today the platform-gotchas skill captures the cross-platform regressions we've already eaten — every entry there is a CI gate we don't have yet. The Backlog discipline (memory note `feedback_tauri_ui_test_loop`) says "run `pnpm test:ui` before commit" — but discipline alone doesn't survive contributor handoffs.

## Goal

A single workflow at `.github/workflows/ci.yml` that runs on every PR to `main` and on every push to `main`. Gates merge on:

- **Rust correctness** on `macos-latest` and `windows-latest`: `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace --features tauri`.
- **Frontend correctness** on `macos-latest` only: `pnpm install --frozen-lockfile`, `pnpm exec tsc --noEmit` (typecheck), `pnpm test:ui` (Playwright).

Every gate is independently visible in the GitHub PR status check list, so a maintainer can tell at a glance which platform/concern is red.

## Non-goals

- **`pnpm tauri build` in PR-gate CI.** Building the full bundle (DMG / MSI) is the *release* CI's job (TASK-66). PR-gate stays at test/lint/typecheck level. The vendor-natives.cjs ↔ MSI bundler interactions documented in `openwhisper-dev-workflow` are TASK-66's concern, not this task's.
- **Sign + notarize.** Same — TASK-66.
- **bench-sherpa.** Built locally, never in CI (per the same skill — its CUDA DLL siblings would explode the runner cache).
- **CodeQL / cargo-audit / dependabot.** Belong to OSS scaffold (TASK-NEW-3), not the PR gate. CI workflow stays focused on correctness; supply-chain scanning is a separate workflow file.
- **Linux runners.** Linux port is out of v1 scope; no need to gate on a platform we don't ship.
- **Windows Playwright.** Tauri's Windows WebView2 runtime makes Playwright spin-up brittle and expensive. Ship Mac-only Playwright for v1; revisit when Windows-shipped builds need a regression backstop (post-v1).

## Behavior model

```
                            PR opened / pushed
                                   │
                  ┌────────────────┼────────────────┐
                  ▼                ▼                ▼
         rust-gate-mac    rust-gate-win    frontend-gate-mac
         (macos-latest)   (windows-latest) (macos-latest)
                  │                │                │
                  │ fmt --check    │ fmt --check    │ pnpm install
                  │ clippy -D warn │ clippy -D warn │ tsc --noEmit
                  │ cargo test     │ cargo test     │ pnpm test:ui
                  │   (incl. CLI   │   (incl. CLI   │
                  │    smoke from  │    smoke from  │
                  │    TASK-81.9)  │    TASK-81.9)  │
                  │                │                │
                  ▼                ▼                ▼
              All three jobs must pass for merge
```

Three independent jobs, run in parallel. Each posts a status check. Branch-protection rules (configured in GitHub UI per `docs/maintainer/branch-protection.md`) require all three to pass before merge is allowed.

### Cost / runtime budget

GitHub Actions billing per minute (Free plan, private repo):

| Runner | Multiplier | 2000-min cap = N runs of 15 min each |
|---|---|---|
| `ubuntu-latest` | 1× | 133 |
| `windows-latest` | 2× | 66 |
| `macos-latest` | 10× | 13 |

Rough per-job runtime estimates (cold cache → warm cache):

| Job | Runner | Cold | Warm |
|---|---|---|---|
| `rust-gate-mac` | macos-latest | 12 min | 4 min |
| `rust-gate-win` | windows-latest | 14 min | 5 min |
| `frontend-gate-mac` | macos-latest | 14 min | 6 min |

Per PR run (worst case, no cache hit): 12 + 14 + 14 = 40 min wall, weighted = (12 × 10) + (14 × 2) + (14 × 10) = 288 minutes.
Per PR run (warm cache, typical): 4 + 5 + 6 = 15 min wall, weighted = 40 + 10 + 60 = 110 minutes.

**At ~110 min weighted per PR run, the 2000-min/month cap absorbs ~18 PR runs/month.** Adequate for v1 prep cadence (a few PRs per week) but not generous. Caching strategy (Task 4) is what makes the math work.

After TASK-NEW-5 (rename → public flip), the repo's free Actions minutes are unmetered and this whole calculation goes away.

## Trade-offs

| Choice | Alternative | Why this |
|---|---|---|
| Three parallel jobs | One job with sequential matrix | Parallel surfaces three independent status checks; the maintainer sees which platform is red without reading logs. Sequential would block merge on the slowest leg's full failure trace. |
| `--features tauri` for `cargo test` | `--all-features` | `recognizer-directml`, `-cuda`, `-tensorrt` features need EPs that aren't installable on default GHA runners. `--all-features` would fail. The `tauri` feature implies `recognizer` (CPU/ONNX path), which is what the CLI smoke needs. |
| `pnpm exec tsc --noEmit` for typecheck | Add a `typecheck` script to package.json | Both work; the `tsc --noEmit` form is a one-liner in CI without forcing a package.json edit that's only useful here. Add the script if a contributor finds the bare invocation surprising. |
| Mac-only Playwright | Mac + Windows Playwright | Tauri WebView2 on `windows-latest` requires installing the WebView2 runtime per-job, which is slow and brittle (WebView2's bootstrapper is interactive-friendly, not CI-friendly). The cross-platform regressions Playwright catches today are all WebKit-rendering issues, which Mac WKWebView already exercises. |
| `setup:ort` invoked from CI | Skip ort entirely on PRs, defer to release CI | TASK-81.9's CLI smoke transcribes a WAV → that needs the ort dylib on Windows (sherpa-onnx) and the FluidAudio bridge on Mac. Skipping ort would break the smoke test. The fetch is small (~30 MB) and cacheable. |
| Cache `~/.cache/openwhisper/onnxruntime/` keyed on the `OPENWHISPER_ORT_VERSION` constant | Refetch every run | Refetch costs ~30s on every job; cache hit makes it ~0. The cache key only changes when ort version bumps. |
| `cargo fmt --check` and `clippy` as separate `cargo` invocations | One mega-step | Independent `cargo` calls means each shows up as its own log section; faster to spot which gate failed. Trivial cost difference. |
| Branch protection configured manually in GitHub UI | Configured via `branch-protection.json` + a Terraform-style apply | Single-maintainer project. The doc tells the maintainer which boxes to tick once. Automation would be premature. |

## Risk register

- **Cache invalidation flakiness.** GitHub Actions cache occasionally returns a partial restore that breaks builds. Mitigation: `cargo clean -p openwhisper-core` if a CI run is suspicious; document in branch-protection.md.
- **macOS Playwright timing on shared runners.** GHA's macOS runners are slower than physical Macs; Playwright timeouts tuned to local laptop speeds may flake in CI. Mitigation: bump default timeout in `apps/tauri/playwright.config.ts` if flakes appear; document in the post-merge follow-up.
- **`pnpm-lock.yaml` drift.** `--frozen-lockfile` will fail CI if a contributor edits `package.json` without committing the regenerated lockfile. That's the correct behavior — make it a documented PR review check.
- **2000-min cap during v1 prep.** If PR cadence spikes (multiple contributors landing in parallel), the cap is hit. Mitigation: rename + public-flip is the unblocker, not a per-workflow optimization. Don't over-engineer caching.
- **Workflow runs concurrent with each other.** Default behavior is serial-per-branch; a force-push during a running CI doubles minute consumption. Mitigation: `concurrency: group: ci-${{ github.ref }}, cancel-in-progress: true` on the workflow.

## Cross-task dependency

TASK-81.9 lands a CLI smoke test (`cli/tests/smoke.rs`) that runs as part of `cargo test --workspace`. This workflow is what enforces TASK-81.9 actually runs on every PR — without TASK-82, the smoke test is just sitting in `cli/tests/` and could rot.

Suggest landing TASK-81.4 (CLI scaffold) and TASK-81.9 (CLI smoke) before TASK-82 ships, so the workflow has a real test to gate on. Order: TASK-81.4 → TASK-81.5a (Mac transcribe) → TASK-81.9 → TASK-82.
