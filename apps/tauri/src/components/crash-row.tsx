import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "./ui/tooltip";
import {
  type CrashSummary,
  formatAbsoluteUtc,
  formatRelative,
} from "../lib/use-crashes";

export interface CrashRowProps {
  crash: CrashSummary;
  first: boolean;
  /// `phase` / `model` are best-effort labels for the third meta
  /// line. The list summary doesn't carry them today (they live in
  /// the full crash file's `recording_state`); leave undefined and
  /// the row falls back to a neutral "—".
  phase?: string;
  model?: string;
  onOpen: (id: string) => void;
  onMarkRead: (id: string) => void;
  onDelete: (id: string) => void;
}

/// Three-column row: 20 px unread dot · body · actions column.
/// Resting actions = caret only; hover reveals [✓] mark-read +
/// [🗑] delete (24×24, 1px border per design). Single-row delete
/// is one-click — no confirm dialog.
export function CrashRow({
  crash,
  first,
  phase,
  model,
  onOpen,
  onMarkRead,
  onDelete,
}: CrashRowProps) {
  const handleRowClick = () => onOpen(crash.id);
  const handleMarkRead = (e: React.MouseEvent) => {
    e.stopPropagation();
    onMarkRead(crash.id);
  };
  const handleDelete = (e: React.MouseEvent) => {
    e.stopPropagation();
    onDelete(crash.id);
  };

  return (
    <TooltipProvider delay={300}>
      <div
        className="ow-crashes__row"
        data-testid={`crash-row-${crash.id}`}
        data-unread={crash.unread || undefined}
        data-first={first || undefined}
        role="button"
        tabIndex={0}
        onClick={handleRowClick}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            handleRowClick();
          }
        }}
      >
        <div className="ow-crashes__row-dot-col">
          <span
            className="ow-crashes__row-dot"
            data-active={crash.unread || undefined}
            aria-hidden="true"
          />
        </div>
        <div className="ow-crashes__row-body">
          <div className="ow-crashes__row-line ow-crashes__row-line--meta">
            <Tooltip>
              <TooltipTrigger render={spanRender} className="ow-crashes__row-when">
                {formatRelative(crash.ts_unix_ms)}
              </TooltipTrigger>
              <TooltipContent>{formatAbsoluteUtc(crash.ts_unix_ms)}</TooltipContent>
            </Tooltip>
            <span className="ow-crashes__row-mono-mute">{crash.app_version}</span>
            <span className="ow-crashes__row-mono-mute">· {crash.os}</span>
            {crash.uploaded_at !== null && (
              <span
                className="ow-crashes__row-uploaded"
                data-testid={`crash-row-uploaded-${crash.id}`}
              >
                uploaded
              </span>
            )}
          </div>
          <div
            className="ow-crashes__row-cause"
            title={crash.message_truncated}
          >
            {crash.message_truncated}
          </div>
          <div className="ow-crashes__row-meta-mono">
            phase: {phase ?? "—"}
            {model && model !== "—" ? ` · model: ${model}` : ""}
          </div>
        </div>
        <div className="ow-crashes__row-actions">
          {/* Resting state: design caret. Hover state: action buttons.
              CSS handles the visibility swap via :hover so the buttons
              are always queryable from Playwright (they're just opacity-
              0 on rest, opacity-1 on hover). */}
          <div className="ow-crashes__row-actions-rest" aria-hidden="true">
            <CaretRight />
          </div>
          <div className="ow-crashes__row-actions-hover">
            {crash.unread && (
              <button
                type="button"
                className="ow-crashes__row-action"
                aria-label="Mark crash as read"
                data-testid={`crash-row-mark-read-${crash.id}`}
                onClick={handleMarkRead}
              >
                <CheckGlyph />
              </button>
            )}
            <button
              type="button"
              className="ow-crashes__row-action"
              aria-label="Delete crash report"
              data-testid={`crash-row-delete-${crash.id}`}
              onClick={handleDelete}
            >
              <TrashGlyph />
            </button>
          </div>
        </div>
      </div>
    </TooltipProvider>
  );
}

// Base-UI's TooltipTrigger forwards via `render` (radix's `asChild`
// equivalent). Keep the styled span so the underline-on-hover stays
// driven by CSS rather than inline styles.
function spanRender(props: React.HTMLAttributes<HTMLSpanElement>) {
  return <span {...props} />;
}

function CaretRight() {
  return (
    <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3.5 2 L 7 5 L 3.5 8" />
    </svg>
  );
}

function CheckGlyph() {
  return (
    <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <path d="M2.5 6.5 L 5 9 L 9.5 3.5" />
    </svg>
  );
}

function TrashGlyph() {
  return (
    <svg width="11" height="11" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
      <path d="M2 3.5 H 10 M 4.5 3.5 V 2.5 a 0.5 0.5 0 0 1 0.5 -0.5 H 7 a 0.5 0.5 0 0 1 0.5 0.5 V 3.5 M 3 3.5 V 10 a 0.5 0.5 0 0 0 0.5 0.5 H 8.5 a 0.5 0.5 0 0 0 0.5 -0.5 V 3.5" />
    </svg>
  );
}
