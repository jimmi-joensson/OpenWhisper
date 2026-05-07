import { formatCrashAsMarkdown } from "./crash-markdown";
import type { CrashFile } from "./use-crashes";

// GitHub Issues URL handles ~8 KB body in practice (browser cap;
// GitHub server has no documented hard limit but breaks fuzzy past
// that). 6 KB target leaves slack for the title + label segment +
// percent-encoding overhead.
const BODY_BYTE_BUDGET = 6_000;
const TITLE_MESSAGE_MAX_CHARS = 72;

const TRUNCATION_MARKER =
  "\n\n_Truncated — paste the full report from `Copy GitHub-ready report` if needed._";

export interface GitHubIssueUrlOptions {
  owner: string;
  repo: string;
  appVersion: string;
}

/// Build a prefilled GitHub Issues URL for the given crash. The
/// body is the same redacted markdown `formatCrashAsMarkdown`
/// produces for the Copy flow — single source of truth, no separate
/// formatter. Body is truncated to fit GitHub's URL length cap; the
/// identity block (panic message, version, OS, when, phase, model)
/// is preserved in full and the backtrace tail is trimmed instead.
export function buildGitHubIssueUrl(
  crash: CrashFile,
  opts: GitHubIssueUrlOptions,
): string {
  const title = buildTitle(crash, opts.appVersion);
  const body = buildBody(crash);
  const params = new URLSearchParams();
  params.set("title", title);
  params.set("body", body);
  params.set("labels", "bug,crash");
  return `https://github.com/${opts.owner}/${opts.repo}/issues/new?${params.toString()}`;
}

function buildTitle(crash: CrashFile, appVersion: string): string {
  const message = crash.rust_panic.message.split("\n")[0];
  const trimmed =
    message.length <= TITLE_MESSAGE_MAX_CHARS
      ? message
      : `${message.slice(0, TITLE_MESSAGE_MAX_CHARS)}…`;
  return `Crash report — v${appVersion} — ${trimmed}`;
}

/// Compose the body, truncating to budget when necessary. The
/// formatter output is structured so the identity block always
/// precedes the backtrace; we exploit that by trimming from the
/// end (where the backtrace + events live) rather than the middle.
function buildBody(crash: CrashFile): string {
  const full = formatCrashAsMarkdown(crash);
  if (byteLength(full) <= BODY_BYTE_BUDGET) {
    return full;
  }
  const reservedForMarker = byteLength(TRUNCATION_MARKER);
  const cap = Math.max(0, BODY_BYTE_BUDGET - reservedForMarker);
  // UTF-8 byte cap, not char cap — emoji + non-ASCII characters
  // expand to multi-byte sequences and a naive char-based slice
  // can blow past the budget by 2–4×.
  const truncated = sliceByByteLength(full, cap);
  return `${truncated}${TRUNCATION_MARKER}`;
}

function byteLength(s: string): number {
  return new TextEncoder().encode(s).length;
}

function sliceByByteLength(s: string, maxBytes: number): string {
  if (byteLength(s) <= maxBytes) return s;
  // Encode once, slice on a UTF-8 boundary, decode back. Cheap and
  // exact — TextEncoder is already in scope.
  const bytes = new TextEncoder().encode(s);
  const sliced = bytes.subarray(0, maxBytes);
  // The slice may have ended mid-codepoint; the lenient decoder
  // strips trailing garbage. `fatal: false` is the default but
  // make it explicit so a future reader doesn't wonder.
  return new TextDecoder("utf-8", { fatal: false }).decode(sliced);
}

// Exported for test introspection only.
export const _internals = {
  BODY_BYTE_BUDGET,
  TITLE_MESSAGE_MAX_CHARS,
  TRUNCATION_MARKER,
  byteLength,
};
