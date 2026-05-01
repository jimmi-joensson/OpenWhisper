import { useState } from "react";
import { Check, Copy } from "lucide-react";

interface TranscriptRowProps {
  text: string;
  timestamp: number;
}

export function TranscriptRow({ text, timestamp }: TranscriptRowProps) {
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
    <div className="ow-transcript-row" data-testid="home-latest-row">
      <div className="ow-transcript-row__body">
        <p className="ow-transcript-row__text ow-selectable">{text}</p>
        <span className="ow-transcript-row__time">{formatRelativeTime(timestamp)}</span>
      </div>
      <button
        type="button"
        className="ow-transcript-row__copy"
        data-testid="home-latest-copy"
        onClick={() => void onCopy()}
        aria-label={copied ? "Copied" : "Copy transcript"}
      >
        {copied ? <Check size={14} aria-hidden="true" /> : <Copy size={14} aria-hidden="true" />}
      </button>
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
