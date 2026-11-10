import { useState } from "react";
import {
  ArrowRight,
  BookA,
  Check,
  Copy,
  Layers,
  Sparkles,
  TriangleAlert,
} from "lucide-react";

import type { DictionaryReplacement } from "../lib/dictation";
import { NO_REFINE, refineFellBack, type RefineInfo } from "../lib/refine";

interface TranscriptRowProps {
  text: string;
  timestamp: number;
  // Dictionary fixes in `text`, shown under it so the user sees what
  // the dictionary changed (and can spot a wrong entry).
  replacements?: DictionaryReplacement[];
  // What the Refine step did. When it refined, `text` is the refined
  // text and the unrefined transcript stays one click away; when it
  // fell back, the row says why.
  refine?: RefineInfo;
}

export function TranscriptRow({
  text,
  timestamp,
  replacements = [],
  refine = NO_REFINE,
}: TranscriptRowProps) {
  const [copied, setCopied] = useState(false);

  const onCopy = async () => {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      // Clipboard may be unavailable in some contexts; UI stays as-is.
    }
  };

  return (
    <div className="ow-transcript-row ow-hover-host" data-testid="home-latest-row">
      <div className="ow-transcript-row__body">
        <p className="ow-transcript-row__text ow-selectable">{text}</p>
        <TranscriptNotes text={text} replacements={replacements} refine={refine} />
        <span className="ow-transcript-row__time">{formatRelativeTime(timestamp)}</span>
      </div>
      <button
        type="button"
        className="ow-transcript-row__copy ow-hover-reveal"
        data-testid="home-latest-copy"
        onClick={() => void onCopy()}
        aria-label={copied ? "Copied" : "Copy transcript"}
      >
        {copied ? <Check size={14} aria-hidden="true" /> : <Copy size={14} aria-hidden="true" />}
      </button>
    </div>
  );
}

interface TranscriptNotesProps {
  text: string;
  replacements?: DictionaryReplacement[];
  refine?: RefineInfo;
}

// What happened to a transcript on its way out: the dictionary fixes,
// what Refine did (with the original one click away), or why Refine
// was skipped, and the mode it ran in when that was one of the user's
// own. Only known for the dictation that just finished, so it
// is shown on the latest row, wherever that row lives: on its own on
// Home, or as the first entry of the history list.
export function TranscriptNotes({
  text,
  replacements = [],
  refine = NO_REFINE,
}: TranscriptNotesProps) {
  const [showOriginal, setShowOriginal] = useState(false);
  const refined = refine.status === "refined";
  const original = refine.raw_transcript.trim();
  const changed = refined && original !== text.trim();

  return (
    <>
      {replacements.length > 0 && (
        <div className="ow-transcript-row__fixes" data-testid="home-latest-fixes">
          <BookA size={12} aria-hidden="true" className="ow-transcript-row__fixes-icon" />
          <ul className="ow-transcript-row__fix-list" aria-label="Dictionary fixes">
            {replacements.map((r) => (
              <li key={`${r.from}->${r.to}`} className="ow-transcript-row__fix">
                <span className="ow-transcript-row__fix-from">{r.from}</span>
                <ArrowRight size={10} aria-label="replaced with" />
                <span className="ow-transcript-row__fix-to">{r.to}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
      {refined && (
        <div className="ow-transcript-row__refine" data-testid="home-latest-refine">
          <Sparkles size={12} aria-hidden="true" className="ow-transcript-row__fixes-icon" />
          <span>
            {refine.instruction
              ? `Refined, as you asked: “${refine.instruction}”`
              : changed
                ? "Refined"
                : "Refined, nothing to change"}
          </span>
          {changed && (
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
      {changed && showOriginal && (
        <p
          className="ow-transcript-row__original ow-selectable"
          data-testid="home-latest-original"
        >
          {original}
        </p>
      )}
      {refineFellBack(refine) && (
        <div
          className="ow-transcript-row__refine"
          data-testid="home-latest-refine"
          data-fallback="true"
        >
          <TriangleAlert size={12} aria-hidden="true" className="ow-transcript-row__fixes-icon" />
          <span>
            Not refined, typed as transcribed
            {refine.detail ? `: ${refine.detail}` : "."}
          </span>
        </div>
      )}
      {refine.mode && (
        <ModeMarker
          name={refine.mode}
          app={refine.mode_reason === "app-default" ? refine.mode_app : ""}
          testId="home-latest-mode"
        />
      )}
    </>
  );
}

// The mode a dictation ran in, when there is something to say about
// it: a mode the user made, or the mode the app has as its default,
// with that reason ("Mode: Formal, default for Chatter"; also "Mode:
// Dictation, default for Terminal", since that default may have
// overridden a mode the user selected). Rust decides when a mode is
// named (`RefineInfo.mode`). Same quiet line as the Refine marker. A
// stored history entry only knows the name, so it shows the name alone.
export function ModeMarker({
  name,
  app = "",
  testId,
}: {
  name: string;
  // The app whose default mode it was; "" when the mode was simply
  // the selected one, or when the reason is not known.
  app?: string;
  testId: string;
}) {
  return (
    <div className="ow-transcript-row__refine" data-testid={testId}>
      <Layers size={12} aria-hidden="true" className="ow-transcript-row__fixes-icon" />
      <span>
        Mode: {name}
        {app ? `, default for ${app}` : ""}
      </span>
    </div>
  );
}

// Row replaces (not persists), so the longest realistic age is the current
// session. v1 doesn't auto-update — the label is "just now" on render and
// re-renders only when the row replaces. Follow-up adds an interval refresh
// if users find that surprising.
function formatRelativeTime(timestamp: number): string {
  const seconds = Math.max(0, Math.floor((Date.now() - timestamp) / 1000));
  if (seconds < 5) return "just now";
  if (seconds < 60) return `${seconds}s ago`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  return new Date(timestamp).toLocaleDateString();
}
