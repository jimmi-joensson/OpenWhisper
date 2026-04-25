import type { ReactNode } from "react";
import type { PillStatus } from "../lib/pill-state";
import { HealthBanner } from "./health-banner";
import { LevelMeter } from "./level-meter";
import { RecordButton } from "./record-button";

export type Platform = "macos" | "windows";

interface MainWindowShellProps {
  status?: PillStatus;
  levels?: number[];
  elapsed?: number;
  samples?: number;
  transcript?: string;
  platform?: Platform;
  showHealth?: boolean;
  onToggle?: () => void;
  onRetry?: () => void;
  coreVersion?: string | null;
  coreError?: string | null;
}

export function MainWindowShell({
  status = "idle",
  levels = [],
  elapsed = 0,
  samples = 0,
  transcript = "",
  platform = "macos",
  showHealth = false,
  onToggle,
  onRetry,
  coreVersion,
  coreError,
}: MainWindowShellProps) {
  return (
    <div
      style={{
        width: "100%",
        maxWidth: 560,
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
        OpenWhisper Dev
      </h1>

      {showHealth && (
        <div style={{ marginBottom: 16 }}>
          <HealthBanner
            message="Microphone access denied. OpenWhisper needs the microphone to capture audio."
            onRetry={onRetry}
            retryLabel="Open System Settings"
          />
        </div>
      )}

      <Section label="Rust ↔ React FFI">
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

      <Section label="Permissions & hotkey debug">
        <KV k="accessibility" v="granted" />
        <KV k="microphone" v="granted" />
        <KV k="tap" v="installed" />
        <KV k="events seen" v="2641" />
        <KV
          k="last event"
          v="flagsChanged keyCode=0 flags=0x000000 rCmd=·"
          wrap
        />
        <div style={{ marginTop: 8 }}>
          <SmallButton label="Retry tap install" />
        </div>
      </Section>

      <Section label="Dictation (mic → Rust core → Parakeet)">
        <KV
          k="status"
          v={
            status === "recording"
              ? "recording — tap again to stop"
              : status === "transcribing"
                ? "transcribing…"
                : "idle"
          }
        />
        <KV k="elapsed" v={status === "idle" ? "—" : `${elapsed.toFixed(1)} s`} />
        <KV
          k="samples"
          v={status === "idle" ? "—" : samples.toLocaleString()}
        />
        <KV k="confidence" v={status === "transcribing" ? "0.92" : "—"} />

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
      </Section>

      <Section label="transcript">
        <div
          style={{
            background: "var(--transcript-bg)",
            border: "1px solid var(--border)",
            borderRadius: 6,
            minHeight: 70,
            padding: "10px 12px",
            fontFamily: "var(--font-mono)",
            fontSize: 12.5,
            color: "var(--foreground)",
            whiteSpace: "pre-wrap",
            lineHeight: 1.45,
          }}
        >
          {transcript ||
            (status === "idle"
              ? "—"
              : status === "recording"
                ? "…"
                : "the quick brown fox jumps over the lazy dog")}
        </div>
      </Section>

      <div style={{ marginTop: 16, display: "flex", justifyContent: "flex-start" }}>
        <RecordButton status={status} onClick={onToggle} />
      </div>
    </div>
  );
}

function Section({ label, children }: { label: string; children: ReactNode }) {
  return (
    <section style={{ marginTop: 14 }}>
      <div
        style={{
          fontSize: 11.5,
          color: "var(--muted-foreground)",
          marginBottom: 6,
        }}
      >
        {label}
      </div>
      <div
        style={{
          background: "color-mix(in oklch, var(--card) 30%, transparent)",
          border: "1px solid var(--border)",
          borderRadius: 8,
          padding: "10px 12px",
        }}
      >
        {children}
      </div>
    </section>
  );
}

function KV({ k, v, wrap = false }: { k: string; v: string; wrap?: boolean }) {
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
        style={{
          whiteSpace: wrap ? "pre-wrap" : "nowrap",
          overflow: "hidden",
          textOverflow: "ellipsis",
        }}
      >
        {v}
      </span>
    </div>
  );
}

function SmallButton({ label, onClick }: { label: string; onClick?: () => void }) {
  return (
    <button
      onClick={onClick}
      style={{
        fontFamily: "var(--font-sys)",
        fontSize: 11.5,
        padding: "3px 9px",
        borderRadius: 5,
        border: "1px solid var(--border)",
        background: "color-mix(in oklch, var(--card) 60%, transparent)",
        color: "var(--foreground)",
        cursor: "pointer",
      }}
    >
      {label}
    </button>
  );
}
