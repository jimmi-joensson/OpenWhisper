// TASK-62.11 — OW RSS Breakdown bar.
//
// Pure-presentation prop-driven component. Renders a stacked
// horizontal bar with one segment per `parts[]` entry plus a
// wrap-flex legend below. The Diagnostics → Memory section composes
// it from the four canonical segments (Parakeet weights / Audio
// buffers / App shell / Caches) attributed by `breakdownEstimate`
// in `use-memory-stats.ts`.
//
// Out of scope here: live attribution. The estimator is a v1
// placeholder; per-component RSS will replace it when TASK-62.7's
// per-model RAM telemetry exposes per-segment numbers. The
// component itself doesn't care — it draws whatever `parts` says.

export interface RSSBreakdownPart {
  /// Display label, e.g. "Parakeet weights".
  label: string;
  /// Segment value in MB. Determines flex-grow and the legend value.
  valueMb: number;
  /// CSS color string applied to the bar segment and legend swatch.
  /// Use semantic tokens (`var(--info)`, `color-mix(in oklch, ...)`)
  /// rather than raw hex so the bar tracks the active theme.
  color: string;
  /// Stable kind for test selectors (e.g. `parakeet`, `audio`).
  kind: string;
}

export interface RSSBreakdownBarProps {
  parts: RSSBreakdownPart[];
  /// Total RSS in MB. Drives the right-hand readout and the
  /// percentage denominator (so segments sum to 100% even when the
  /// estimator's app-shell floor over-attributes on low RSS).
  totalRssMb: number;
}

export function RSSBreakdownBar({ parts, totalRssMb }: RSSBreakdownBarProps) {
  const denom = parts.reduce((s, p) => s + p.valueMb, 0) || totalRssMb || 1;
  const totalGb = (totalRssMb / 1024).toFixed(2);

  return (
    <div
      className="ow-rss-breakdown"
      data-testid="diagnostics-rss-breakdown"
      aria-label="OpenWhisper RSS breakdown — segment sizes are estimates"
    >
      <div className="ow-rss-breakdown__header">
        <span className="ow-rss-breakdown__kicker">
          OpenWhisper RSS Breakdown
        </span>
        <span
          className="ow-rss-breakdown__resident"
          data-testid="diagnostics-rss-breakdown-total"
        >
          {totalGb} GB resident
        </span>
      </div>
      <div className="ow-rss-breakdown__bar" role="presentation">
        {parts.map((part) => {
          if (part.valueMb <= 0) return null;
          return (
            <span
              key={part.kind}
              className="ow-rss-breakdown__seg"
              data-kind={part.kind}
              style={{
                flexGrow: part.valueMb,
                flexShrink: 0,
                flexBasis: 0,
                background: part.color,
              }}
              title={`${part.label}: ${part.valueMb} MB`}
            />
          );
        })}
      </div>
      <ul className="ow-rss-breakdown__legend">
        {parts.map((part) => {
          if (part.valueMb <= 0) return null;
          const pct = Math.round((part.valueMb / denom) * 100);
          return (
            <li
              key={part.kind}
              className="ow-rss-breakdown__item"
              data-testid={`diagnostics-rss-segment-${part.kind}`}
            >
              <span
                className="ow-rss-breakdown__swatch"
                style={{ background: part.color }}
              />
              <span className="ow-rss-breakdown__label">{part.label}</span>
              <span
                className="ow-rss-breakdown__pct"
                data-testid={`diagnostics-rss-segment-${part.kind}-pct`}
              >
                {pct}%
              </span>
              <span className="ow-rss-breakdown__sep">·</span>
              <span className="ow-rss-breakdown__value">{part.valueMb} MB</span>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
