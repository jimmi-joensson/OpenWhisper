import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Sheet,
  SheetContent,
  SheetTitle,
} from "./ui/sheet";
import { type CrashFile, type UseCrashesResult } from "../lib/use-crashes";
import {
  formatAbsoluteUtc,
  formatCrashAsMarkdown,
  formatDuration,
} from "../lib/crash-markdown";

const COPIED_FLASH_MS = 1200;

export interface CrashDetailSheetProps {
  openId: string | null;
  onOpenChange: (open: boolean) => void;
  read: UseCrashesResult["read"];
  markRead: UseCrashesResult["markRead"];
  deleteOne: UseCrashesResult["deleteOne"];
  /// Endpoint string from the build; when null/empty the Upload
  /// button is OMITTED (not disabled). Wired in TASK-78.6 — for
  /// 78.4 we leave it null so the Upload button isn't visible at
  /// all yet.
  uploadEndpoint?: string | null;
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
  uploadEndpoint = null,
}: CrashDetailSheetProps) {
  const [crash, setCrash] = useState<CrashFile | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [eventsOpen, setEventsOpen] = useState(false);
  const [copyState, setCopyState] = useState<"idle" | "ok" | "fail">("idle");
  const open = openId !== null;

  useEffect(() => {
    if (openId === null) {
      setCrash(null);
      setError(null);
      setEventsOpen(false);
      setCopyState("idle");
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

  const onCopy = async () => {
    if (!crash) return;
    const md = formatCrashAsMarkdown(crash);
    try {
      await navigator.clipboard.writeText(md);
      setCopyState("ok");
      window.setTimeout(() => setCopyState("idle"), COPIED_FLASH_MS);
    } catch {
      setCopyState("fail");
      window.setTimeout(() => setCopyState("idle"), COPIED_FLASH_MS);
    }
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
                data-testid="crash-detail-copy"
                onClick={onCopy}
              >
                <span>
                  {copyState === "ok"
                    ? "✓ Copied"
                    : copyState === "fail"
                      ? "Copy failed — try again"
                      : "Copy GitHub-ready report"}
                </span>
                <span className="ow-crashes__sheet-shortcut">⌘C</span>
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
                {uploadEndpoint && uploadEndpoint.trim().length > 0 && (
                  <UploadAffordance crash={crash} />
                )}
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
  onCopy: () => void;
}) {
  return (
    <section className="ow-crashes__sheet-section">
      <header className="ow-crashes__sheet-section-header">
        <span className="ow-crashes__sheet-kicker">Backtrace</span>
        <span className="ow-crashes__sheet-spacer" />
        <button
          type="button"
          className="ow-crashes__sheet-ghost ow-crashes__sheet-ghost--sm"
          data-testid="crash-detail-copy-backtrace"
          onClick={onCopy}
        >
          Copy backtrace
        </button>
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

function UploadAffordance({ crash }: { crash: CrashFile }) {
  // Uploaded-state swap is the headline UX rule of this footer per
  // spec — no re-upload affordance once a crash has been sent.
  // Wired against the real `uploaded_at` once TASK-78.6 lands; for
  // 78.4 the `crash` object's uploaded_at lives only on the
  // CrashSummary, not the full CrashFile, so we render the Upload
  // button placeholder. 78.6 swaps in the real flow.
  void crash;
  return (
    <button
      type="button"
      className="ow-crashes__sheet-ghost"
      data-testid="crash-detail-upload"
      // 78.6 wires the dialog. For 78.4 the button surface exists
      // so the design renders end-to-end.
      onClick={(e) => e.preventDefault()}
    >
      Upload
    </button>
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
