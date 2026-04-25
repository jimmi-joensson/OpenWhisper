import type { PillStatus } from "../lib/pill-state";
import { MicGlyph } from "./mic-glyph";

interface RecordButtonProps {
  status?: PillStatus;
  onClick?: () => void;
  size?: "sm" | "md";
}

export function RecordButton({ status = "idle", onClick, size = "md" }: RecordButtonProps) {
  const recording = status === "recording" || status === "transcribing";
  const padY = size === "sm" ? 6 : 8;
  const padX = size === "sm" ? 12 : 14;
  const fontSize = size === "sm" ? 12.5 : 13.5;

  if (recording) {
    return (
      <button
        onClick={onClick}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 8,
          padding: `${padY}px ${padX}px`,
          borderRadius: 8,
          border: "1px solid color-mix(in oklch, var(--destructive) 55%, transparent)",
          background: "color-mix(in oklch, var(--destructive) 18%, transparent)",
          color: "var(--destructive)",
          fontFamily: "var(--font-sys)",
          fontSize,
          fontWeight: 500,
          cursor: "pointer",
        }}
      >
        <StopGlyph size={10} />
        Stop &amp; transcribe
      </button>
    );
  }

  return (
    <button
      onClick={onClick}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 8,
        padding: `${padY}px ${padX}px`,
        borderRadius: 8,
        border: "1px solid var(--border)",
        background: "color-mix(in oklch, var(--card) 92%, transparent)",
        color: "var(--foreground)",
        fontFamily: "var(--font-sys)",
        fontSize,
        fontWeight: 500,
        cursor: "pointer",
      }}
    >
      <MicGlyph size={18} fill="currentColor" />
      Start dictation
    </button>
  );
}

function StopGlyph({ size = 10 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 10 10" aria-hidden="true">
      <rect x="1" y="1" width="8" height="8" rx="1.5" fill="currentColor" />
    </svg>
  );
}
