import type { ReactNode } from "react";
import type { PillStatus } from "../lib/pill-state";
import {
  PHASE_DONE,
  PHASE_ERROR,
  PHASE_IDLE,
  PHASE_LOADING_MODEL,
  PHASE_RECORDING,
  PHASE_TRANSCRIBING,
} from "../lib/dictation";
import { Card, CardContent, CardHeader, CardTitle } from "./ui/card";
import { HealthBanner } from "./health-banner";
import { LevelMeter } from "./level-meter";
import { RecordButton } from "./record-button";

export type Platform = "macos" | "windows";

interface MainWindowShellProps {
  title?: string;
  phase?: number;
  status?: PillStatus;
  levels?: number[];
  level?: number;
  elapsed?: number;
  samples?: number;
  transcript?: string;
  confidence?: number;
  statusMessage?: string;
  errorMessage?: string;
  canToggle?: boolean;
  isRecording?: boolean;
  platform?: Platform;
  onToggle?: () => void;
  coreVersion?: string | null;
  coreError?: string | null;
  hotkeyError?: string | null;
  onHotkeyRetry?: () => void;
  micError?: string | null;
  recognizerError?: string | null;
  onRecognizerRetry?: () => void;
}

const PHASE_NAMES: Record<number, string> = {
  [PHASE_IDLE]: "idle",
  [PHASE_LOADING_MODEL]: "loading_model",
  [PHASE_RECORDING]: "recording",
  [PHASE_TRANSCRIBING]: "transcribing",
  [PHASE_DONE]: "done",
  [PHASE_ERROR]: "error",
};

export function MainWindowShell({
  title = "OpenWhisper",
  phase = 0,
  status = "idle",
  levels = [],
  level = 0,
  elapsed = 0,
  samples = 0,
  transcript = "",
  confidence = 0,
  statusMessage = "",
  errorMessage = "",
  canToggle = true,
  isRecording = false,
  platform = "macos",
  onToggle,
  coreVersion,
  coreError,
  hotkeyError,
  onHotkeyRetry,
  micError,
  recognizerError,
  onRecognizerRetry,
}: MainWindowShellProps) {
  const statusText =
    statusMessage ||
    (status === "recording"
      ? "recording — tap again to stop"
      : status === "transcribing"
        ? "transcribing…"
        : "idle");

  return (
    <div
      style={{
        width: "100%",
        maxWidth: 580,
        margin: "0 auto",
        padding: "20px 28px 24px",
        color: "var(--foreground)",
        fontFamily: "var(--font-sys)",
      }}
    >
      <h1
        style={{
          textAlign: "center",
          fontSize: 22,
          fontWeight: 600,
          letterSpacing: "-0.01em",
          margin: "4px 0 18px",
        }}
      >
        {title}
      </h1>

      {hotkeyError ? (
        <div data-testid="hotkey-banner" style={{ marginBottom: 12 }}>
          <HealthBanner
            message={hotkeyError}
            onRetry={onHotkeyRetry}
            retryLabel="Restart"
          />
        </div>
      ) : null}

      {micError ? (
        <div data-testid="mic-banner" style={{ marginBottom: 12 }}>
          <HealthBanner message={micError} />
        </div>
      ) : null}

      {recognizerError ? (
        <div data-testid="recognizer-banner" style={{ marginBottom: 12 }}>
          <HealthBanner
            message={recognizerError}
            onRetry={onRecognizerRetry}
            retryLabel="Retry"
          />
        </div>
      ) : null}

      <Section title="Rust ↔ React FFI">
        <KV
          k="message"
          v={coreError ? `error: ${coreError}` : "Hello from openwhisper-core (Rust)"}
        />
        <KV k="version" v={coreVersion ?? "…"} />
      </Section>

      <p
        style={{
          textAlign: "center",
          color: "var(--muted-foreground)",
          fontSize: 12.5,
          margin: "14px 0",
        }}
      >
        {platform === "macos"
          ? "Right Command to toggle · Escape to cancel while recording"
          : "Ctrl + Space anywhere · Escape to cancel while recording"}
      </p>

      <Section title="Dictation debug">
        <KV k="platform" v={platform} />
        <KV
          k="phase"
          v={`${phase} (${PHASE_NAMES[phase] ?? "unknown"})`}
        />
        <KV k="can_toggle" v={canToggle ? "true" : "false"} />
        <KV k="is_recording" v={isRecording ? "true" : "false"} />
        <KV k="level (raw)" v={level.toFixed(4)} />
        <KV k="last error" v={errorMessage || "—"} />
      </Section>

      <Section title="Dictation (mic → Rust core → Parakeet)">
        <KV k="status" v={statusText} />
        <KV k="elapsed" v={status === "idle" ? "—" : `${elapsed.toFixed(1)} s`} />
        <KV
          k="samples"
          v={status === "idle" ? "—" : `${samples.toLocaleString()} @ 16 kHz`}
        />
        <KV
          k="confidence"
          v={confidence > 0 ? confidence.toFixed(2) : "—"}
        />

        <div style={{ marginTop: 12 }}>
          <LevelMeter
            bars={32}
            levels={levels}
            active={status}
            height={36}
            minHeight={4}
            gap={2}
            fill
          />
        </div>

        <div style={{ marginTop: 14 }}>
          <RecordButton phase={phase} onClick={onToggle} />
        </div>
      </Section>

      <Section title="transcript">
        <div
          className="ow-selectable"
          style={{
            background: "var(--transcript-bg)",
            border: "1px solid var(--border)",
            borderRadius: 6,
            minHeight: 70,
            maxHeight: 160,
            overflowY: "auto",
            padding: "10px 12px",
            fontFamily: "var(--font-mono)",
            fontSize: 12.5,
            color: "var(--foreground)",
            whiteSpace: "pre-wrap",
            lineHeight: 1.45,
          }}
        >
          {transcript || (status === "idle" ? "—" : "…")}
        </div>
      </Section>
    </div>
  );
}

function Section({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Card size="sm" className="mt-3.5">
      <CardHeader>
        <CardTitle className="text-xs font-normal text-muted-foreground tracking-wide">
          {title}
        </CardTitle>
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}

function KV({ k, v }: { k: string; v: string }) {
  return (
    <div
      style={{
        display: "grid",
        gridTemplateColumns: "110px 1fr",
        gap: 8,
        fontFamily: "var(--font-mono)",
        fontSize: 12,
        lineHeight: 1.7,
        color: "var(--foreground)",
      }}
    >
      <span style={{ textAlign: "right", color: "var(--muted-foreground)" }}>{k}:</span>
      <span
        className="ow-selectable"
        style={{
          whiteSpace: "pre-wrap",
          wordBreak: "break-word",
        }}
      >
        {v}
      </span>
    </div>
  );
}
