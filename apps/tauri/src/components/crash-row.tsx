import { Check, ChevronRight, Trash2 } from "lucide-react";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "./ui/tooltip";

// Base-UI's TooltipTrigger uses the `render` prop (it forwards via
// React.cloneElement-equivalent rather than radix's `asChild`). Wraps
// a plain element so the existing class-based span keeps its styles.
function spanRender(props: React.HTMLAttributes<HTMLSpanElement>) {
  return <span {...props} />;
}
import {
  type CrashSummary,
  formatAbsoluteUtc,
  formatRelative,
} from "../lib/use-crashes";

export interface CrashRowProps {
  crash: CrashSummary;
  /// Phase / model labels parsed from the row's structured fields if
  /// available; the spec calls for a "phase: <X> · model: <Y>" mono
  /// meta line. We don't have these on the summary today (they live
  /// only inside the full crash file's `recording_state`), so the
  /// list pane derives them when present and the row falls back to a
  /// neutral "—" when not. Wired as plain props to keep the row
  /// trivially testable.
  phase?: string;
  model?: string;
  onOpen: (id: string) => void;
  onMarkRead: (id: string) => void;
  onDelete: (id: string) => void;
}

/// Three-column row: 20 px unread dot · body · actions column.
/// Resting actions = chevron only; hover reveals [✓] mark-read +
/// [🗑] delete. Per the spec, single-row delete is one-click — no
/// confirm dialog.
export function CrashRow({
  crash,
  phase,
  model,
  onOpen,
  onMarkRead,
  onDelete,
}: CrashRowProps) {
  const handleRowClick = () => {
    onOpen(crash.id);
  };

  const handleMarkRead = (e: React.MouseEvent) => {
    e.stopPropagation();
    onMarkRead(crash.id);
  };

  const handleDelete = (e: React.MouseEvent) => {
    e.stopPropagation();
    onDelete(crash.id);
  };

  const phaseLabel = phase ?? "—";
  const modelLabel = model ?? "—";

  return (
    <TooltipProvider delay={300}>
      <div
        className="ow-crashes__row"
        data-testid={`crash-row-${crash.id}`}
        data-unread={crash.unread || undefined}
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
            <span className="ow-crashes__row-chip">{crash.app_version}</span>
            <span className="ow-crashes__row-chip">{crash.os}</span>
            {crash.uploaded_at !== null && (
              <span
                className="ow-crashes__row-chip ow-crashes__row-chip--uploaded"
                data-testid={`crash-row-uploaded-${crash.id}`}
              >
                uploaded
              </span>
            )}
          </div>
          <div
            className="ow-crashes__row-line ow-crashes__row-cause"
            title={crash.message_truncated}
          >
            {crash.message_truncated}
          </div>
          <div className="ow-crashes__row-line ow-crashes__row-line--mono">
            phase: {phaseLabel} · model: {modelLabel}
          </div>
        </div>
        <div className="ow-crashes__row-actions">
          {/* Hover-revealed action buttons. data-action stops the row's
              click handler (each button's onClick already stopPropagation,
              this mirrors the design's resting/hover split). */}
          {crash.unread && (
            <button
              type="button"
              className="ow-crashes__row-action ow-crashes__row-action--read"
              aria-label="Mark crash as read"
              data-testid={`crash-row-mark-read-${crash.id}`}
              onClick={handleMarkRead}
            >
              <Check size={14} aria-hidden="true" />
            </button>
          )}
          <button
            type="button"
            className="ow-crashes__row-action ow-crashes__row-action--delete"
            aria-label="Delete crash report"
            data-testid={`crash-row-delete-${crash.id}`}
            onClick={handleDelete}
          >
            <Trash2 size={14} aria-hidden="true" />
          </button>
          <ChevronRight
            size={16}
            className="ow-crashes__row-chevron"
            aria-hidden="true"
          />
        </div>
      </div>
    </TooltipProvider>
  );
}
