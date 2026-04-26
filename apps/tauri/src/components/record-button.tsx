import {
  PHASE_LOADING_MODEL,
  PHASE_RECORDING,
  PHASE_TRANSCRIBING,
} from "../lib/dictation";
import { MicGlyph } from "./mic-glyph";

interface RecordButtonProps {
  phase?: number;
  onClick?: () => void;
  size?: "sm" | "md";
}

// Mirrors apps/macos/App/ContentView.swift buttonLabel/isButtonDisabled.
function buttonState(phase: number): {
  label: string;
  disabled: boolean;
  recording: boolean;
} {
  switch (phase) {
    case PHASE_LOADING_MODEL:
      return { label: "Loading…", disabled: true, recording: false };
    case PHASE_TRANSCRIBING:
      return { label: "Transcribing…", disabled: true, recording: true };
    case PHASE_RECORDING:
      return { label: "Stop & transcribe", disabled: false, recording: true };
    default:
      return { label: "Record", disabled: false, recording: false };
  }
}

export function RecordButton({ phase = 0, onClick, size = "md" }: RecordButtonProps) {
  const { label, disabled, recording } = buttonState(phase);
  const padY = size === "sm" ? 6 : 8;
  const padX = size === "sm" ? 12 : 14;
  const fontSize = size === "sm" ? 12.5 : 13.5;

  const baseStyle: React.CSSProperties = {
    display: "inline-flex",
    alignItems: "center",
    gap: 8,
    padding: `${padY}px ${padX}px`,
    borderRadius: 8,
    fontFamily: "var(--font-sys)",
    fontSize,
    fontWeight: 500,
    cursor: disabled ? "default" : "pointer",
    opacity: disabled ? 0.55 : 1,
  };

  if (recording) {
    return (
      <button
        onClick={onClick}
        disabled={disabled}
        style={{
          ...baseStyle,
          border: "1px solid color-mix(in oklch, var(--destructive) 55%, transparent)",
          background: "color-mix(in oklch, var(--destructive) 18%, transparent)",
          color: "var(--destructive)",
        }}
      >
        <StopGlyph size={10} />
        {label}
      </button>
    );
  }

  return (
    <button
      onClick={onClick}
      disabled={disabled}
      style={{
        ...baseStyle,
        border: "1px solid var(--border)",
        background: "color-mix(in oklch, var(--card) 92%, transparent)",
        color: "var(--foreground)",
      }}
    >
      <MicGlyph size={18} fill="currentColor" />
      {label}
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
