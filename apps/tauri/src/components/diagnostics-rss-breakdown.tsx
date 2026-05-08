// TASK-62.11 — OpenWhisper Memory Breakdown bar.
//
// Pure-presentation prop-driven component. Renders a stacked
// horizontal bar with one segment per `parts[]` entry plus a
// wrap-flex legend below. The Diagnostics → Memory section composes
// it from the canonical segments (Parakeet weights — process RSS on
// Windows or ANE/GPU on Mac / Audio buffers / App shell / Caches)
// attributed by `breakdownEstimate` in `use-memory-stats.ts`.
//
// Total = sum of segments — covers BOTH process RSS and ANE/GPU
// claim so the bar matches the "OpenWhisper Memory" readout above
// (which sums process RSS + ANE/GPU). Earlier shape limited the bar
// to in-process RSS only, which read as inconsistent on Mac where
// the recognizer's 461 MB ANE claim was excluded from the bar but
// included in the readout.
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
}

export function RSSBreakdownBar({ parts }: RSSBreakdownBarProps) {
  const totalMb = parts.reduce((s, p) => s + p.valueMb, 0);
  const denom = totalMb || 1;
  // Honest unit picking: small totals (<1 GB) read as MB so the
  // readout matches the segment legend numbers exactly.
  const totalLabel =
    totalMb >= 1024
      ? `${(totalMb / 1024).toFixed(2)} GB total`
      : `${totalMb} MB total`;

  return (
    <div
      className="ow-rss-breakdown"
      data-testid="diagnostics-rss-breakdown"
      aria-label="OpenWhisper memory breakdown — segment sizes are estimates"
    >
      <div className="ow-rss-breakdown__header">
        <span className="ow-rss-breakdown__kicker">
          OpenWhisper Memory Breakdown
        </span>
        <span
          className="ow-rss-breakdown__resident"
          data-testid="diagnostics-rss-breakdown-total"
        >
          {totalLabel}
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
