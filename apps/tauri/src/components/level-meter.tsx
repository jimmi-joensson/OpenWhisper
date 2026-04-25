import { useMemo } from "react";
import type { PillStatus } from "../lib/pill-state";

// dB-normalized amplitude → 0..1 bar-height factor.
//   normalized = clamp((20 * log10(max(amp, 1e-6)) + 55) / 55, 0, 1)
export function owNormalize(amp: number): number {
  const a = Math.max(amp, 1e-6);
  return Math.min(1, Math.max(0, (20 * Math.log10(a) + 55) / 55));
}

interface LevelMeterProps {
  bars?: number;
  levels?: number[];
  active?: PillStatus;
  height?: number;
  minHeight?: number;
  barWidth?: number;
  gap?: number;
  fill?: boolean;
  className?: string;
}

export function LevelMeter({
  bars = 12,
  levels = [],
  active = "idle",
  height = 10,
  minHeight = 2,
  barWidth = 2,
  gap = 2,
  fill: fillContainer = false,
  className = "",
}: LevelMeterProps) {
  const data = useMemo(() => {
    const out = new Array(bars).fill(0);
    const n = Math.min(bars, levels.length);
    for (let i = 0; i < n; i++) out[bars - n + i] = levels[i];
    return out;
  }, [bars, levels]);

  const fillColor =
    active === "recording"
      ? "var(--recording)"
      : "rgb(255 255 255 / 0.35)";

  const containerStyle: React.CSSProperties = fillContainer
    ? { height, gap: `${gap}px`, width: "100%" }
    : { height, gap: `${gap}px` };

  return (
    <div className={"flex items-center " + className} style={containerStyle}>
      {data.map((v, i) => {
        const norm = owNormalize(v);
        const h =
          active === "idle"
            ? minHeight
            : Math.max(minHeight, Math.round(norm * height));
        const barStyle: React.CSSProperties = fillContainer
          ? {
              flex: "1 1 0",
              minWidth: 0,
              height: `${h}px`,
              background: fillColor,
              borderRadius: "var(--bar-radius)",
              transition: "height var(--tick-ms) linear, background-color var(--tick-ms) linear",
            }
          : {
              width: `${barWidth}px`,
              height: `${h}px`,
              background: fillColor,
              borderRadius: "var(--bar-radius)",
              transition: "height var(--tick-ms) linear, background-color var(--tick-ms) linear",
            };
        return <div key={i} style={barStyle} />;
      })}
    </div>
  );
}
