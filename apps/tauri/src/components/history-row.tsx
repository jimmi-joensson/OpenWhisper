import { useMemo, useState } from "react";
import { Check, Copy, Trash2 } from "lucide-react";

import { ModeMarker, TranscriptNotes } from "@/components/transcript-row";
import { Badge } from "@/components/ui/badge";
import type { DictionaryReplacement } from "@/lib/dictation";
import { formatClock, groupByDay, isAskEntry, type HistoryEntry } from "@/lib/history";
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

// Stored dictations and asks grouped by the day they were made.
// Shared by the short list on Home and the History screen.
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
  const [showAnswer, setShowAnswer] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const long = entry.text.length > FOLD_AT_CHARS;
  const app = entry.app_name ?? entry.app_id;
  const original = entry.original;
  // An ask (TASK-97): the row text is the question, and the answer
  // is one click away, like the original of a refined dictation.
  const ask = isAskEntry(entry);
  const answer = ask ? entry.answer : null;
  // Copy takes what is being read: the answer while it is shown,
  // otherwise the row text.
  const copiesAnswer = answer !== null && showAnswer;
  const copyLabel = !ask ? "Copy transcript" : copiesAnswer ? "Copy answer" : "Copy question";

  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(copiesAnswer ? answer : entry.text);
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
      data-kind={ask ? "ask" : undefined}
    >
      <div className="ow-history-row__body">
        <div className="ow-history-row__meta">
          <time dateTime={new Date(entry.at_ms).toISOString()}>{formatClock(entry.at_ms)}</time>
          {app && <span data-testid="history-row-app">{app}</span>}
          {ask && (
            <Badge variant="outline" data-testid="history-row-ask">
              Ask
            </Badge>
          )}
          {/* The ask was sent with a picture of the screen
              (TASK-98). Only the fact is stored, never the picture. */}
          {ask && entry.screen && (
            <Badge variant="outline" data-testid="history-row-screen">
              Screen
            </Badge>
          )}
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
        {(long || (original && !notes) || answer !== null) && (
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
            {answer !== null && (
              <button
                type="button"
                className="ow-transcript-row__refine-toggle"
                aria-expanded={showAnswer}
                onClick={() => setShowAnswer((v) => !v)}
              >
                {showAnswer ? "Hide answer" : "Show answer"}
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
        {answer !== null && showAnswer && (
          <p
            className="ow-transcript-row__original ow-history-row__answer ow-selectable"
            data-testid="history-row-answer"
          >
            {answer}
          </p>
        )}
      </div>
      <div className="ow-history-row__actions ow-hover-reveal">
        <button
          type="button"
          className="ow-history-row__action"
          data-testid="history-row-copy"
          onClick={() => void onCopy()}
          aria-label={copied ? "Copied" : copyLabel}
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
