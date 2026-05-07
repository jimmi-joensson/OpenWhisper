import { describe, expect, it } from "vitest";
import { _internals, buildGitHubIssueUrl } from "./crash-github";
import type { CrashFile } from "./use-crashes";

const TS = Date.UTC(2026, 4, 4, 14, 33, 21);

function fixture(overrides: Partial<CrashFile> = {}): CrashFile {
  return {
    schema_version: 1,
    id: String(TS),
    ts_unix_ms: TS,
    app_version: "0.6.0",
    os: "macos (arm64)",
    rust_panic: {
      thread_name: "tokio-runtime-worker",
      message: "called `Result::unwrap()` on an `Err` value",
      location: "core/src/audio.rs:412:17",
      backtrace: "frame 1\nframe 2\nframe 3",
    },
    recording_state: null,
    events: [],
    ...overrides,
  };
}

describe("buildGitHubIssueUrl — title", () => {
  it("composes 'Crash report — v<appVersion> — <message>'", () => {
    const url = new URL(
      buildGitHubIssueUrl(fixture(), {
        owner: "jimmi-joensson",
        repo: "OpenWhisper",
        appVersion: "0.6.0",
      }),
    );
    expect(url.searchParams.get("title")).toBe(
      "Crash report — v0.6.0 — called `Result::unwrap()` on an `Err` value",
    );
  });

  it("truncates the message past the cap with a trailing ellipsis", () => {
    const longMsg = "a".repeat(120);
    const url = new URL(
      buildGitHubIssueUrl(
        fixture({
          rust_panic: { ...fixture().rust_panic, message: longMsg },
        }),
        { owner: "o", repo: "r", appVersion: "0.6.0" },
      ),
    );
    const title = url.searchParams.get("title")!;
    expect(title).toMatch(/…$/);
    // 72 char cap on the message + the prefix.
    expect(title).toContain("a".repeat(72));
    expect(title).not.toContain("a".repeat(73));
  });

  it("uses only the first line of a multi-line panic message", () => {
    const msg = "first line\nsecond line\nthird line";
    const url = new URL(
      buildGitHubIssueUrl(
        fixture({ rust_panic: { ...fixture().rust_panic, message: msg } }),
        { owner: "o", repo: "r", appVersion: "0.6.0" },
      ),
    );
    expect(url.searchParams.get("title")).toBe(
      "Crash report — v0.6.0 — first line",
    );
  });
});

describe("buildGitHubIssueUrl — labels + URL shape", () => {
  it("targets the right repo + sets bug,crash labels", () => {
    const url = buildGitHubIssueUrl(fixture(), {
      owner: "jimmi-joensson",
      repo: "OpenWhisper",
      appVersion: "0.6.0",
    });
    expect(url.startsWith(
      "https://github.com/jimmi-joensson/OpenWhisper/issues/new?",
    )).toBe(true);
    expect(new URL(url).searchParams.get("labels")).toBe("bug,crash");
  });
});

describe("buildGitHubIssueUrl — body", () => {
  it("body matches formatCrashAsMarkdown for short crashes", () => {
    const url = new URL(
      buildGitHubIssueUrl(fixture(), {
        owner: "o",
        repo: "r",
        appVersion: "0.6.0",
      }),
    );
    const body = url.searchParams.get("body")!;
    // Headline + identity block survive exactly.
    expect(body).toContain("**OpenWhisper crash report**");
    expect(body).toContain("- Version: 0.6.0");
    expect(body).toContain("called `Result::unwrap()` on an `Err` value");
    // No truncation marker for short bodies.
    expect(body).not.toContain("Truncated");
  });

  it("truncates oversized bodies and appends the marker", () => {
    // Build a backtrace that pushes the body well past the budget.
    const huge = "frame X\n".repeat(2_000);
    const url = new URL(
      buildGitHubIssueUrl(
        fixture({
          rust_panic: { ...fixture().rust_panic, backtrace: huge },
        }),
        { owner: "o", repo: "r", appVersion: "0.6.0" },
      ),
    );
    const body = url.searchParams.get("body")!;
    expect(body).toContain("Truncated");
    expect(body).toContain("Copy GitHub-ready report");
    // Identity block is preserved in full — truncation happens at
    // the backtrace tail.
    expect(body).toContain("- Version: 0.6.0");
    expect(body).toContain("- OS: macos (arm64)");
    // Body byte length stays within budget.
    expect(_internals.byteLength(body)).toBeLessThanOrEqual(
      _internals.BODY_BYTE_BUDGET,
    );
  });

  it("redacted markers in input survive into the body verbatim", () => {
    const fix = fixture({
      rust_panic: {
        ...fixture().rust_panic,
        message: "panic at /Users/<redacted>/secret",
        backtrace: "OPENAI_API_KEY=<redacted>",
      },
    });
    const url = new URL(
      buildGitHubIssueUrl(fix, {
        owner: "o",
        repo: "r",
        appVersion: "0.6.0",
      }),
    );
    const body = url.searchParams.get("body")!;
    expect(body).toContain("/Users/<redacted>/secret");
    expect(body).toContain("OPENAI_API_KEY=<redacted>");
    // Negative sanity: no raw username leaks into the URL.
    expect(body).not.toMatch(/\/Users\/jimmi/);
  });
});
