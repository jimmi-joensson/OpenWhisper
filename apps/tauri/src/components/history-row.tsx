import { useMemo, useState } from "react";
import { Check, Copy, Trash2 } from "lucide-react";

import { ModeMarker, TranscriptNotes } from "@/components/transcript-row";
import type { DictionaryReplacement } from "@/lib/dictation";
import { formatClock, groupByDay, type HistoryEntry } from "@/lib/history";
import type { RefineInfo } from "@/lib/refine";

// Text longer than this is folded to a few lines until expanded.
const FOLD_AT_CHARS = 280;

// The dictation that just finished, when it is in the list: its row
// also shows the dictionary fixes and what Refine did, which are only
// known for this one.
export interface LatestNotes {
  id: string;
  replacements: DictionaryReplacement[];
  refine: RefineInfo;
}

interface HistoryDaysProps {
  // Newest first.
  entries: HistoryEntry[];
  latest?: LatestNotes | null;
  onDelete: (id: string) => void;
}

// Stored dictations grouped by the day they were made. Shared by the
// short list on Home and the History screen.
export function HistoryDays({ entries, latest = null, onDelete }: HistoryDaysProps) {
  const days = useMemo(() => groupByDay(entries), [entries]);
  return (
    <>
      {days.map((day) => (
        <div key={day.key} className="ow-history__day">
          <h3 className="ow-history__day-label">{day.label}</h3>
          <ul className="ow-history__rows">
            {day.entries.map((entry) => (
              <HistoryRow
                key={entry.id}
                entry={entry}
                notes={latest && latest.id === entry.id ? latest : null}
                onDelete={() => onDelete(entry.id)}
              />
            ))}
          </ul>
        </div>
      ))}
    </>
  );
}

function HistoryRow({
  entry,
  notes,
  onDelete,
}: {
  entry: HistoryEntry;
  // Set on the dictation that just finished; see `LatestNotes`.
  notes: LatestNotes | null;
  onDelete: () => void;
}) {
  const [copied, setCopied] = useState(false);
  const [showOriginal, setShowOriginal] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const long = entry.text.length > FOLD_AT_CHARS;
  const app = entry.app_name ?? entry.app_id;
  const original = entry.original;

  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(entry.text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      // Clipboard may be unavailable in some contexts; UI stays as-is.
    }
  };

  return (
    <li
      className="ow-history-row ow-hover-host"
      data-testid="history-row"
      data-latest={notes ? "true" : undefined}
    >
      <div className="ow-history-row__body">
        <div className="ow-history-row__meta">
          <time dateTime={new Date(entry.at_ms).toISOString()}>{formatClock(entry.at_ms)}</time>
          {app && <span data-testid="history-row-app">{app}</span>}
        </div>
        <p
          className="ow-history-row__text ow-selectable"
          data-folded={long && !expanded ? "true" : undefined}
        >
          {entry.text}
        </p>
        {notes && (
          <TranscriptNotes
            text={entry.text}
            replacements={notes.replacements}
            refine={notes.refine}
          />
        )}
        {/* The notes of the dictation that just finished name the mode
            themselves, with the reason. */}
        {entry.mode && !notes?.refine.mode && (
          <ModeMarker name={entry.mode} testId="history-row-mode" />
        )}
        {(long || (original && !notes)) && (
          <div className="ow-history-row__links">
            {long && (
              <button
                type="button"
                className="ow-transcript-row__refine-toggle"
                aria-expanded={expanded}
                onClick={() => setExpanded((v) => !v)}
              >
                {expanded ? "Show less" : "Show all"}
              </button>
            )}
            {original && !notes && (
              <button
                type="button"
                className="ow-transcript-row__refine-toggle"
                aria-expanded={showOriginal}
                onClick={() => setShowOriginal((v) => !v)}
              >
                {showOriginal ? "Hide original" : "Show original"}
              </button>
            )}
          </div>
        )}
        {original && !notes && showOriginal && (
          <p
            className="ow-transcript-row__original ow-selectable"
            data-testid="history-row-original"
          >
            {original}
          </p>
        )}
      </div>
      <div className="ow-history-row__actions ow-hover-reveal">
        <button
          type="button"
          className="ow-history-row__action"
          data-testid="history-row-copy"
          onClick={() => void onCopy()}
          aria-label={copied ? "Copied" : "Copy transcript"}
        >
          {copied ? <Check size={14} aria-hidden="true" /> : <Copy size={14} aria-hidden="true" />}
        </button>
        <button
          type="button"
          className="ow-history-row__action"
          data-testid="history-row-delete"
          onClick={onDelete}
          aria-label="Delete from history"
        >
          <Trash2 size={14} aria-hidden="true" />
        </button>
      </div>
    </li>
  );
}
