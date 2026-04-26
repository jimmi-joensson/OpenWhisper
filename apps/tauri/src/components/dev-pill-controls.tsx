import type { PillStatus } from "../lib/pill-state";

interface DevPillControlsProps {
  enabled: boolean;
  status: PillStatus;
  onToggle: (enabled: boolean) => void;
  onStatus: (status: PillStatus) => void;
}

const STATES: PillStatus[] = ["idle", "recording", "transcribing"];

// Dev-only floating panel for parking the pill in any state. In manual mode
// the App suppresses the auto pill_state emit and we drive emission from
// here instead — including a simulated 20 Hz amplitude envelope for the
// "recording" state so the meter has data without a live mic.
export function DevPillControls({
  enabled,
  status,
  onToggle,
  onStatus,
}: DevPillControlsProps) {
  return (
    <div
      style={{
        position: "fixed",
        right: 16,
        bottom: 16,
        zIndex: 50,
        display: "flex",
        alignItems: "center",
        gap: 8,
        padding: "6px 8px",
        borderRadius: 10,
        background: "rgba(0, 0, 0, 0.55)",
        backdropFilter: "blur(20px) saturate(140%)",
        WebkitBackdropFilter: "blur(20px) saturate(140%)",
        boxShadow: "0 4px 14px rgba(0, 0, 0, 0.35)",
        border: "1px solid rgba(255, 255, 255, 0.08)",
        color: "white",
        fontSize: 11,
        fontFamily: "var(--font-mono, ui-monospace, Menlo, monospace)",
        userSelect: "none",
      }}
    >
      <span
        style={{
          fontSize: 9.5,
          letterSpacing: "0.08em",
          textTransform: "uppercase",
          opacity: 0.55,
          paddingLeft: 4,
        }}
      >
        pill dev
      </span>
      <label
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 4,
          padding: "4px 6px",
          borderRadius: 6,
          background: enabled
            ? "rgba(224, 112, 0, 0.25)"
            : "rgba(255, 255, 255, 0.06)",
          border: `1px solid ${enabled ? "rgba(224, 112, 0, 0.55)" : "rgba(255, 255, 255, 0.12)"}`,
          cursor: "pointer",
        }}
      >
        <input
          type="checkbox"
          checked={enabled}
          onChange={(e) => onToggle(e.target.checked)}
          style={{ accentColor: "#E07000", margin: 0 }}
        />
        <span>{enabled ? "manual" : "auto"}</span>
      </label>
      <div style={{ display: "inline-flex", gap: 4 }}>
        {STATES.map((s) => {
          const active = enabled && status === s;
          return (
            <button
              key={s}
              type="button"
              onClick={() => {
                if (!enabled) onToggle(true);
                onStatus(s);
              }}
              style={{
                appearance: "none",
                fontFamily: "inherit",
                fontSize: 11,
                padding: "4px 8px",
                borderRadius: 6,
                border: `1px solid ${active ? "rgba(224, 112, 0, 0.7)" : "rgba(255, 255, 255, 0.12)"}`,
                background: active
                  ? "rgba(224, 112, 0, 0.35)"
                  : "rgba(255, 255, 255, 0.04)",
                color: active ? "#FFB870" : "rgba(255, 255, 255, 0.85)",
                cursor: "pointer",
              }}
            >
              {s}
            </button>
          );
        })}
      </div>
    </div>
  );
}
