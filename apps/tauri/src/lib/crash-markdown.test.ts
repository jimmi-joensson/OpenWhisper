import { describe, expect, it } from "vitest";
import { formatCrashAsMarkdown } from "./crash-markdown";
import type { CrashFile } from "./use-crashes";

// Date.UTC(year, monthIndex, day, hour, minute, second) — avoids
// hand-computed epoch drift across leap years.
const MAY_4_2026_UTC = Date.UTC(2026, 4, 4, 14, 33, 21);

function fixtureFull(): CrashFile {
  return {
    schema_version: 1,
    id: String(MAY_4_2026_UTC),
    ts_unix_ms: MAY_4_2026_UTC,
    app_version: "0.6.0",
    os: "macos (arm64)",
    rust_panic: {
      thread_name: "tokio-runtime-worker",
      message: "called `Result::unwrap()` on an `Err` value: ...",
      location: "core/src/audio.rs:412:17",
      backtrace: "frame 1\nframe 2\nframe 3",
    },
    recording_state: {
      status_message_at_crash: "transcribing on ANE…",
      duration_ms: 18234,
      samples_captured: 291744,
      model_kind: "Parakeet",
      device_id_hash: "sha256:abcd1234",
    },
    events: [
      {
        ts_unix_ms: MAY_4_2026_UTC - 1000,
        kind: "DictationStart",
        data: {},
      },
      {
        ts_unix_ms: MAY_4_2026_UTC - 500,
        kind: "Error",
        data: { message: "boom" },
      },
    ],
  };
}

describe("formatCrashAsMarkdown — full shape", () => {
  it("produces a GitHub-ready report with all sections", () => {
    const out = formatCrashAsMarkdown(fixtureFull());
    expect(out).toContain("**OpenWhisper crash report**");
    expect(out).toContain("- Version: 0.6.0");
    expect(out).toContain("- OS: macos (arm64)");
    expect(out).toContain("- When: 2026-05-04 14:33:21 UTC");
    expect(out).toContain("- Phase at crash: transcribing on ANE… (18.2s in)");
    expect(out).toContain("- Model: Parakeet");
    expect(out).toContain("**What I was doing:**");
    expect(out).toContain("> _replace this with a quick description");
    expect(out).toContain("<summary>Backtrace (click to expand)</summary>");
    expect(out).toContain(
      "called `Result::unwrap()` on an `Err` value: ...",
    );
    expect(out).toContain("at core/src/audio.rs:412:17");
    expect(out).toContain("frame 1");
    expect(out).toContain("frame 2");
    expect(out).toContain("<summary>Recent events (2)</summary>");
    expect(out).toContain("| time | kind | data |");
    expect(out).toContain("| DictationStart | {} |");
    expect(out).toMatch(/\| Error \| \{"message":"boom"\} \|/);
  });
});

describe("formatCrashAsMarkdown — missing recording_state", () => {
  it("emits 'idle (outside dictation)' when recording_state is null", () => {
    const fixture = fixtureFull();
    fixture.recording_state = null;
    const out = formatCrashAsMarkdown(fixture);
    expect(out).toContain("- Phase at crash: idle (outside dictation)");
    expect(out).not.toContain("- Model:");
  });

  it("omits Model line when model_kind is null", () => {
    const fixture = fixtureFull();
    fixture.recording_state = {
      status_message_at_crash: "recording — tap again to stop",
      duration_ms: 5000,
      samples_captured: 80000,
      model_kind: null,
      device_id_hash: null,
    };
    const out = formatCrashAsMarkdown(fixture);
    expect(out).toContain("- Phase at crash: recording — tap again to stop (5.0s in)");
    expect(out).not.toContain("- Model:");
  });
});

describe("formatCrashAsMarkdown — empty events", () => {
  it("omits the events section entirely when events.length === 0", () => {
    const fixture = fixtureFull();
    fixture.events = [];
    const out = formatCrashAsMarkdown(fixture);
    expect(out).not.toContain("Recent events");
    expect(out).not.toContain("| time |");
  });
});

describe("formatCrashAsMarkdown — redaction regression", () => {
  // The on-disk file is already redacted by `core::crashes::redact`.
  // This test asserts the formatter does NOT re-introduce un-redacted
  // strings — it accepts redacted input verbatim and emits the same
  // <redacted>/<HOME> tokens in the output.
  it("preserves redacted markers and never reveals raw PII", () => {
    const fixture: CrashFile = {
      schema_version: 1,
      id: "1",
      ts_unix_ms: MAY_4_2026_UTC,
      app_version: "0.6.0",
      os: "macos (arm64)",
      rust_panic: {
        thread_name: "main",
        message: "panic at /Users/<redacted>/secret",
        location: "/Users/<redacted>/repo/audio.rs:1:1",
        backtrace:
          "OPENAI_API_KEY=<redacted> · <HOME>/cache/foo · C:\\Users\\<redacted>\\AppData",
      },
      recording_state: null,
      events: [
        {
          ts_unix_ms: MAY_4_2026_UTC,
          kind: "Error",
          data: { msg: "failed at /Users/<redacted>/foo" },
        },
      ],
    };
    const out = formatCrashAsMarkdown(fixture);

    // Sanity: redacted markers survive verbatim.
    expect(out).toContain("/Users/<redacted>/secret");
    expect(out).toContain("OPENAI_API_KEY=<redacted>");
    expect(out).toContain("<HOME>/cache/foo");

    // Negative: no raw user-name shapes leak. None of these strings
    // exist in the input — the formatter must not synthesize them.
    expect(out).not.toMatch(/\/Users\/jimmi/);
    expect(out).not.toMatch(/\/Users\/Bob/);
    expect(out).not.toMatch(/sk-[a-zA-Z0-9]+/);
    expect(out).not.toMatch(/ghp_[a-zA-Z0-9]+/);
    expect(out).not.toMatch(/AKIA[A-Z0-9]+/);
  });
});

describe("formatCrashAsMarkdown — markdown safety", () => {
  it("escapes pipe characters in event data so table rows stay single-cell", () => {
    const fixture = fixtureFull();
    fixture.events = [
      {
        ts_unix_ms: MAY_4_2026_UTC,
        kind: "Error",
        data: { msg: "a | b | c" },
      },
    ];
    const out = formatCrashAsMarkdown(fixture);
    // The row should contain `\|` not raw `|` for the escaped pipes
    // — only the cell separators remain unescaped.
    const eventLine = out.split("\n").find((l) => l.includes("Error"))!;
    // Three escaped pipes inside the JSON value:
    expect(eventLine.match(/\\\|/g)?.length).toBe(2);
  });

  it("collapses newlines in event data to spaces so the row stays single-line", () => {
    const fixture = fixtureFull();
    fixture.events = [
      {
        ts_unix_ms: MAY_4_2026_UTC,
        kind: "Error",
        data: "line1\nline2",
      },
    ];
    const out = formatCrashAsMarkdown(fixture);
    const eventLine = out.split("\n").find((l) => l.includes("Error"))!;
    expect(eventLine).not.toContain("\n");
    expect(eventLine).toContain("line1 line2");
  });
});
