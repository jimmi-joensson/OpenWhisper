import type { CrashFile } from "./use-crashes";

/// Format a crash file as a GitHub-ready markdown report. Pure
/// function: input → exact output, no IO. The on-disk crash file is
/// already redacted at write time (per `core::crashes::redact`), so
/// this formatter does NOT re-redact — its only contract is "don't
/// re-introduce un-redacted fields", asserted by the regression test
/// in `crash-markdown.test.ts`.
///
/// Output shape mirrors the spec body in `backlog/docs/specs/doc-22`:
/// - Identity bullets (Version, OS, When, Phase, Model)
/// - "What I was doing" placeholder block — the ONE thing the user
///   has to fill in before posting
/// - <details> backtrace block — collapsed by default in GitHub Issues
/// - <details> recent events block — table of Time/Kind/Data
export function formatCrashAsMarkdown(crash: CrashFile): string {
  const lines: string[] = [];
  lines.push("**OpenWhisper crash report**");
  lines.push("");
  lines.push(`- Version: ${crash.app_version}`);
  lines.push(`- OS: ${crash.os}`);
  lines.push(`- When: ${formatAbsoluteUtc(crash.ts_unix_ms)}`);

  const rs = crash.recording_state;
  if (rs) {
    const phase = stripStatusEmoji(rs.status_message_at_crash);
    lines.push(
      `- Phase at crash: ${phase} (${formatDuration(rs.duration_ms)} in)`,
    );
    if (rs.model_kind) {
      lines.push(`- Model: ${rs.model_kind}`);
    }
  } else {
    lines.push(`- Phase at crash: idle (outside dictation)`);
  }
  lines.push("");
  lines.push("**What I was doing:**");
  lines.push("");
  lines.push("> _replace this with a quick description before submitting_");
  lines.push("");
  lines.push("<details>");
  lines.push("<summary>Backtrace (click to expand)</summary>");
  lines.push("");
  lines.push("```");
  lines.push(crash.rust_panic.message);
  lines.push(`   at ${crash.rust_panic.location}`);
  lines.push("");
  lines.push(crash.rust_panic.backtrace);
  lines.push("```");
  lines.push("");
  lines.push("</details>");

  if (crash.events.length > 0) {
    lines.push("");
    lines.push("<details>");
    lines.push(`<summary>Recent events (${crash.events.length})</summary>`);
    lines.push("");
    lines.push("| time | kind | data |");
    lines.push("| --- | --- | --- |");
    for (const ev of crash.events) {
      const time = formatTimeOfDay(ev.ts_unix_ms);
      const dataStr = formatEventData(ev.data);
      lines.push(`| ${time} | ${ev.kind} | ${dataStr} |`);
    }
    lines.push("");
    lines.push("</details>");
  }

  return lines.join("\n");
}

/// Format an absolute UTC timestamp for the "When" line. Matches the
/// detail-sheet header format. ISO-ish but human-friendly:
/// `2026-05-04 14:33:21 UTC`.
export function formatAbsoluteUtc(tsUnixMs: number): string {
  const d = new Date(tsUnixMs);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return (
    `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ` +
    `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())} UTC`
  );
}

/// Format a duration as "1m 12s" / "18.2s" — used by the markdown
/// "(N in)" suffix and by the detail-sheet's session-length chip.
export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const sec = ms / 1000;
  if (sec < 60) return `${sec.toFixed(1)}s`;
  const minutes = Math.floor(sec / 60);
  const remSec = Math.round(sec - minutes * 60);
  return `${minutes}m ${remSec}s`;
}

function formatTimeOfDay(tsUnixMs: number): string {
  const d = new Date(tsUnixMs);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`;
}

function stripStatusEmoji(s: string): string {
  // Some status_message strings carry trailing ellipsis or em-dashes
  // ("transcribing on ANE…"); leave those — they're informative and
  // already redacted. Only normalise leading whitespace.
  return s.trim();
}

function formatEventData(data: unknown): string {
  if (data === null || data === undefined) return "";
  if (typeof data === "string") return escapeMd(data);
  try {
    const json = JSON.stringify(data);
    return escapeMd(json);
  } catch {
    return "";
  }
}

function escapeMd(s: string): string {
  // Pipe characters break GitHub markdown table rows; escape them.
  // Also collapse any newlines so the row stays single-line.
  return s.replace(/\|/g, "\\|").replace(/\r?\n/g, " ");
}
