import { useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CircleCheck, Copy } from "lucide-react";
import {
  Sheet,
  SheetContent,
  SheetTitle,
} from "./ui/sheet";
import { type CrashFile, type UseCrashesResult } from "../lib/use-crashes";
import {
  formatAbsoluteUtc,
  formatDuration,
} from "../lib/crash-markdown";
import { buildGitHubIssueUrl } from "../lib/crash-github";

// Copy-success flash duration. Long enough to register as feedback,
// short enough that a second click feels responsive.
const COPY_FLASH_MS = 1200;

// Hard-coded so the button always points at the canonical OW
// repo. If we ever fork or vendor, this is a one-line swap.
const GITHUB_OWNER = "jimmi-joensson";
const GITHUB_REPO = "OpenWhisper";
// `Cargo.toml` is the source of truth; Vite injects this at build
// time so the value matches what the panic hook stamps into
// `crash.app_version`. Falls back gracefully if missing.
const APP_VERSION =
  (import.meta.env.VITE_APP_VERSION as string | undefined) ?? "dev";

export interface CrashDetailSheetProps {
  openId: string | null;
  onOpenChange: (open: boolean) => void;
  read: UseCrashesResult["read"];
  markRead: UseCrashesResult["markRead"];
  deleteOne: UseCrashesResult["deleteOne"];
}

/// Right-side detail sheet, ~580 px wide. Sticky header + sticky
/// action footer; scrollable identity / backtrace / events body
/// between them. Per spec: opening the sheet IS the read action,
/// closing the sheet does NOT un-read, deleting from the sheet
/// closes it and removes the row from the list.
export function CrashDetailSheet({
  openId,
  onOpenChange,
  read,
  markRead,
  deleteOne,
}: CrashDetailSheetProps) {
  const [crash, setCrash] = useState<CrashFile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [eventsOpen, setEventsOpen] = useState(false);
  const open = openId !== null;

  useEffect(() => {
    if (openId === null) {
      setCrash(null);
      setError(null);
      setEventsOpen(false);
      return;
    }
    let cancelled = false;
    // Mark-read fires once on open per AC. Closing does not un-read.
    void markRead(openId).catch(() => {});
    void read(openId)
      .then((file) => {
        if (cancelled) return;
        setCrash(file);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [openId, read, markRead]);

  const onReportOnGitHub = () => {
    if (!crash) return;
    const url = buildGitHubIssueUrl(crash, {
      owner: GITHUB_OWNER,
      repo: GITHUB_REPO,
      appVersion: crash.app_version || APP_VERSION,
    });
    // Goes through `tauri-plugin-opener`'s open_url command —
    // covered by `opener:default` capability so no per-URL scope
    // wiring is required (vs. open_path which we shell out for).
    // Failure is logged + silently swallowed; the user can still
    // get the report via `Copy backtrace` + the inline identity
    // block, or via `openwhisper crash-dump` from the CLI.
    void invoke("plugin:opener|open_url", { url }).catch((e) => {
      console.error("[plugin:opener|open_url]", e);
    });
  };

  const onCopyBacktrace = async () => {
    if (!crash) return;
    try {
      await navigator.clipboard.writeText(crash.rust_panic.backtrace);
    } catch {
      // Best-effort — secondary action, no surface needed.
    }
  };

  const onOpenFolder = () => {
    void invoke("crashes_open_folder").catch((e) => {
      console.error("[crashes_open_folder]", e);
    });
  };

  const onDelete = () => {
    if (openId) {
      void deleteOne(openId);
      onOpenChange(false);
    }
  };

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent
        side="right"
        showCloseButton={false}
        className="ow-crashes__sheet"
        data-testid="crash-detail-sheet"
      >
        {/* shadcn Sheet wraps everything in flex column; we override
            via the .ow-crashes__sheet class so header/body/footer
            get sticky behavior. SheetTitle is required by base-ui
            for a11y; visually hide it and use our own kicker. */}
        <SheetTitle className="sr-only">Crash report</SheetTitle>

        <header className="ow-crashes__sheet-header">
          <span className="ow-crashes__sheet-kicker">Crash report</span>
          <span className="ow-crashes__sheet-spacer" />
          <button
            type="button"
            className="ow-crashes__sheet-close"
            aria-label="Close crash report"
            data-testid="crash-detail-close"
            onClick={() => onOpenChange(false)}
          >
            <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round">
              <path d="M3 3 L 9 9 M 9 3 L 3 9" />
            </svg>
          </button>
        </header>

        {error && (
          <p className="ow-crashes__sheet-error" data-testid="crash-detail-error">
            {error}
          </p>
        )}

        {crash && (
          <>
            <div className="ow-crashes__sheet-body">
              <CrashIdentity crash={crash} />
              <CrashBacktrace
                backtrace={crash.rust_panic.backtrace}
                location={crash.rust_panic.location}
                onCopy={onCopyBacktrace}
              />
              <CrashEvents
                events={crash.events}
                open={eventsOpen}
                onToggle={() => setEventsOpen((o) => !o)}
              />
            </div>

            <footer className="ow-crashes__sheet-footer">
              <button
                type="button"
                className="ow-crashes__sheet-primary"
                data-testid="crash-detail-report-github"
                onClick={onReportOnGitHub}
              >
                <GitHubGlyph size={14} />
                <span>Report on GitHub</span>
              </button>
              <div className="ow-crashes__sheet-secondary">
                <button
                  type="button"
                  className="ow-crashes__sheet-ghost"
                  data-testid="crash-detail-open-folder"
                  onClick={onOpenFolder}
                >
                  Open crash folder
                </button>
                <span className="ow-crashes__sheet-spacer" />
                <button
                  type="button"
                  className="ow-crashes__sheet-destructive"
                  data-testid="crash-detail-delete"
                  onClick={onDelete}
                >
                  Delete
                </button>
              </div>
            </footer>
          </>
        )}
      </SheetContent>
    </Sheet>
  );
}

function CrashIdentity({ crash }: { crash: CrashFile }) {
  const rs = crash.recording_state;
  const phaseLine = rs ? (
    <>
      <span className="ow-crashes__sheet-meta-mono">phase: </span>
      <span>{rs.status_message_at_crash}</span>
      {rs.model_kind && (
        <>
          <span className="ow-crashes__sheet-meta-mono"> · model: </span>
          <span>{rs.model_kind}</span>
        </>
      )}
      <span className="ow-crashes__sheet-meta-mono">
        {" "}· session{" "}
      </span>
      <span>{formatDuration(rs.duration_ms)}</span>
    </>
  ) : (
    <span className="ow-crashes__sheet-meta-mono">
      phase: idle (outside dictation)
    </span>
  );

  return (
    <section className="ow-crashes__sheet-section">
      <h3 className="ow-crashes__sheet-message" data-testid="crash-detail-message">
        {crash.rust_panic.message}
      </h3>
      <div className="ow-crashes__sheet-meta">
        <div>
          {formatAbsoluteUtc(crash.ts_unix_ms)}
          <span className="ow-crashes__sheet-meta-sep" />
          <span className="ow-crashes__sheet-meta-mono">{crash.app_version}</span>
        </div>
        <div>{crash.os}</div>
        <div className="ow-crashes__sheet-meta-row-mono">{phaseLine}</div>
        <div className="ow-crashes__sheet-meta-mono ow-crashes__sheet-location">
          at {crash.rust_panic.location}
        </div>
      </div>
    </section>
  );
}

function CrashBacktrace({
  backtrace,
  location: _location,
  onCopy,
}: {
  backtrace: string;
  location: string;
  onCopy: () => Promise<void> | void;
}) {
  return (
    <section className="ow-crashes__sheet-section">
      <header className="ow-crashes__sheet-section-header">
        <span className="ow-crashes__sheet-kicker">Backtrace</span>
        <span className="ow-crashes__sheet-spacer" />
        <CopyBacktraceButton onCopy={onCopy} />
      </header>
      <pre
        className="ow-crashes__sheet-backtrace"
        data-testid="crash-detail-backtrace"
      >
        {backtrace}
      </pre>
    </section>
  );
}

/// Copy-backtrace button with icon-morph success feedback.
/// Two icons (`Copy`, `CircleCheck`) stacked in a fixed-size slot,
/// cross-faded + scaled via CSS transitions on the `data-copied`
/// attribute. T2 surface (button feedback) per
/// `openwhisper-animation-philosophy`: ≤180 ms cap, transform +
/// opacity only, custom-bezier ease-out, reduced-motion snaps.
/// Auto-revert after `COPY_FLASH_MS` so a quick eye glance still
/// catches the success state without leaving a stale check on
/// screen.
function CopyBacktraceButton({
  onCopy,
}: {
  onCopy: () => Promise<void> | void;
}) {
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (timerRef.current !== null) {
        window.clearTimeout(timerRef.current);
      }
    };
  }, []);

  const onClick = async () => {
    try {
      await onCopy();
      setCopied(true);
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
      timerRef.current = window.setTimeout(() => {
        setCopied(false);
        timerRef.current = null;
      }, COPY_FLASH_MS);
    } catch {
      // Best-effort — secondary action, no surface needed.
    }
  };

  return (
    <button
      type="button"
      className="ow-crashes__sheet-ghost ow-crashes__sheet-ghost--sm ow-crashes__copy-btn"
      data-copied={copied || undefined}
      aria-label={copied ? "Backtrace copied" : "Copy backtrace"}
      data-testid="crash-detail-copy-backtrace"
      onClick={onClick}
    >
      <span className="ow-crashes__copy-btn-icon" aria-hidden="true">
        <Copy size={13} className="ow-crashes__copy-btn-icon-copy" />
        <CircleCheck size={13} className="ow-crashes__copy-btn-icon-check" />
      </span>
      Copy backtrace
    </button>
  );
}

function CrashEvents({
  events,
  open,
  onToggle,
}: {
  events: CrashFile["events"];
  open: boolean;
  onToggle: () => void;
}) {
  const rows = useMemo(
    () =>
      events.map((ev) => ({
        time: formatTimeOfDay(ev.ts_unix_ms),
        phase: phaseFromEvent(ev.kind, ev.data),
        kind: ev.kind,
        detail: formatEventDetail(ev.data),
        isCrash: ev.kind === "Error" || ev.kind === "crash",
      })),
    [events],
  );

  return (
    <section className="ow-crashes__sheet-section">
      <button
        type="button"
        className="ow-crashes__sheet-events-toggle"
        data-testid="crash-detail-events-toggle"
        onClick={onToggle}
        aria-expanded={open}
      >
        <span
          className="ow-crashes__sheet-events-caret"
          data-open={open || undefined}
          aria-hidden="true"
        >
          <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
            <path d="M3.5 2 L 7 5 L 3.5 8" />
          </svg>
        </span>
        <span className="ow-crashes__sheet-kicker">Events</span>
        <span className="ow-crashes__sheet-events-count">({events.length})</span>
      </button>
      {open && events.length > 0 && (
        <div
          className="ow-crashes__sheet-events"
          data-testid="crash-detail-events-table"
        >
          <div className="ow-crashes__sheet-events-row ow-crashes__sheet-events-row--head">
            <span>Time</span>
            <span>Phase</span>
            <span>Event</span>
            <span>Detail</span>
          </div>
          <div className="ow-crashes__sheet-events-body">
            {rows.map((row, i) => (
              <div
                key={i}
                className="ow-crashes__sheet-events-row"
                data-crash={row.isCrash || undefined}
                data-zebra={i % 2 === 1 || undefined}
              >
                <span className="ow-crashes__sheet-events-mute">{row.time}</span>
                <span className="ow-crashes__sheet-events-mute">{row.phase}</span>
                <span data-crash={row.isCrash || undefined}>{row.kind}</span>
                <span
                  className="ow-crashes__sheet-events-mute ow-crashes__sheet-events-detail"
                  title={row.detail}
                >
                  {row.detail}
                </span>
              </div>
            ))}
          </div>
        </div>
      )}
    </section>
  );
}

/// GitHub mark — Lucide doesn't ship a brand-faithful version, and
/// adding `@primer/octicons-react` or `simple-icons` for one glyph
/// is overkill. Inline SVG matches the existing project pattern
/// (see `CrashStarGlyph` in `crash-empty.tsx`). Source: GitHub
/// Octicons `mark-github` (MIT-licensed,
/// https://github.com/primer/octicons).
function GitHubGlyph({ size = 14 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 16 16"
      fill="currentColor"
      aria-hidden="true"
    >
      <path d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82.64-.18 1.32-.27 2-.27.68 0 1.36.09 2 .27 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.013 8.013 0 0 0 16 8c0-4.42-3.58-8-8-8z" />
    </svg>
  );
}

function formatTimeOfDay(tsUnixMs: number): string {
  const d = new Date(tsUnixMs);
  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`;
}

function phaseFromEvent(kind: string, data: unknown): string {
  if (kind === "PhaseChange" && typeof data === "object" && data !== null) {
    const to = (data as { to?: unknown }).to;
    if (typeof to === "string") return to.toLowerCase();
  }
  if (kind === "ModelLoaded") return "loaded";
  if (kind === "DictationStart") return "recording";
  if (kind === "Error") return "error";
  return "—";
}

function formatEventDetail(data: unknown): string {
  if (data === null || data === undefined) return "";
  if (typeof data === "string") return data;
  try {
    const json = JSON.stringify(data);
    if (json === "{}") return "";
    return json;
  } catch {
    return "";
  }
}
