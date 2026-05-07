import { useLayoutEffect, useMemo, useRef } from "react";
import {
  RSS_SERIES_LEN,
  useMemoryStats,
  type ModelMemoryRow,
} from "../lib/use-memory-stats";
import { Card, CardContent, CardHeader, CardTitle } from "./ui/card";

const SPARK_W_VB = 600;
const SPARK_H_VB = 96;
const SPARK_PAD_TOP = 6;
const SPARK_PAD_BOT = 4;
const SPARK_TICK_MS = 1000;

export type Platform = "macos" | "windows";

export interface DiagnosticsPaneProps {
  platform?: Platform;
}

// Diagnostics — top-level route, sibling to Home and Settings. Today
// covers Memory only; Performance counters (TASK-78.x) and Crash
// reports (TASK-78.3) land into the same shell as additional sections
// when their telemetry exists. Per the design (`backlog/docs/specs/`,
// chats `chat9.md`), per-model load/release controls live under
// Settings → Models — the budget bar there is the *decision* surface;
// this pane is the *debugging* surface.
export function DiagnosticsPane(_props: DiagnosticsPaneProps) {
  const { stats, rssSeries, error } = useMemoryStats();

  return (
    <div className="ow-diagnostics">
      <header className="ow-diagnostics__header">
        <h1 className="ow-diagnostics__title">Diagnostics</h1>
        <p className="ow-diagnostics__sub">
          OpenWhisper&#39;s memory at a glance. Per-model load and unload
          live in Settings → Models.
        </p>
      </header>

      <MemoryCard stats={stats} series={rssSeries} error={error} />

      <p className="ow-diagnostics__footer">
        <span className="ow-diagnostics__footer-tag">Note</span> Per-model
        RAM is an RSS-delta estimate captured at load time. Concurrent
        allocations and ANE-resident memory on macOS may not be
        reflected. System-wide pressure lives in your OS&#39;s Activity
        Monitor.
      </p>
    </div>
  );
}

interface MemoryCardProps {
  stats: ReturnType<typeof useMemoryStats>["stats"];
  series: number[];
  error: string | null;
}

function MemoryCard({ stats, series, error }: MemoryCardProps) {
  const rss = stats?.process.rss_bytes ?? 0;
  const peak = stats?.process.peak_rss_bytes ?? 0;
  const models = stats?.models ?? [];

  return (
    <Card size="sm" className="ow-diagnostics__card">
      <CardHeader>
        <CardTitle className="ow-diagnostics__card-title">Memory</CardTitle>
      </CardHeader>
      <CardContent className="flex flex-col gap-4">
        <div className="flex items-baseline justify-between gap-4">
          <div className="flex items-baseline gap-7">
            <Readout
              label="OpenWhisper RSS"
              value={formatBytes(rss).value}
              unit={formatBytes(rss).unit}
              sub={`peak ${formatBytes(peak).value} ${formatBytes(peak).unit}`}
              emphasised
            />
            <Readout
              label="Models loaded"
              value={String(countLoaded(models))}
              unit={countLoaded(models) === 1 ? "model" : "models"}
              sub={`${models.length} registered`}
            />
          </div>
          <span className="ow-diagnostics__caption">
            Last {RSS_SERIES_LEN} s · scale 0–{formatBytes(niceCeiling(peak)).value}{" "}
            {formatBytes(niceCeiling(peak)).unit}
          </span>
        </div>

        <Sparkline data={series} peakBytes={peak} />

        <Breakdown rssBytes={rss} models={models} />

        {error && (
          <p
            className="ow-diagnostics__error"
            data-testid="diagnostics-error"
          >
            telemetry_get_memory failed: {error}
          </p>
        )}
      </CardContent>
    </Card>
  );
}

function Readout({
  label,
  value,
  unit,
  sub,
  emphasised,
}: {
  label: string;
  value: string;
  unit: string;
  sub: string;
  emphasised?: boolean;
}) {
  return (
    <div className="ow-diagnostics__readout" data-emphasised={emphasised}>
      <div className="ow-diagnostics__readout-label">{label}</div>
      <div className="ow-diagnostics__readout-row">
        <span
          className="ow-diagnostics__readout-value"
          data-testid={`diagnostics-readout-${slug(label)}`}
        >
          {value}
        </span>
        <span className="ow-diagnostics__readout-unit">{unit}</span>
      </div>
      <div className="ow-diagnostics__readout-sub">{sub}</div>
    </div>
  );
}

// 60-sample area sparkline — genuinely continuous flow.
//
// Per-frame RAF rebuilds the path d= attribute from a single time-
// derived `progress` parameter (0..1, ramping over the inter-poll
// interval). Three pieces compose the visible curve:
//
//   1. The first N-1 buffer samples (the older history) at fixed
//      positions, sliding leftward as `progress` advances.
//   2. A "live" interp point at the right edge whose Y blends from
//      buf[N-2] (at progress=0, just after a new sample landed) to
//      buf[N-1] (at progress=1, just before the next one lands).
//   3. A phantom one pitch past the interp, sharing its Y, so the
//      curve doesn't bare a gap between the rightmost sample and
//      the SVG's right edge during the slide.
//
// At swap time (data prop change), `progress` jumps 1→0 and the
// buffer indices shift by one. Because the interp Y at progress=1
// (= old buf[N-1]) equals the new buf[N-2]'s Y at progress=0
// (= old buf[N-1]), every screen X maps to the same Y across the
// swap — visually seamless, not just "smoothed."
//
// Y axis: fixed monotonic ceiling derived from the process peak
// (snaps to a nice ladder, never contracts). Older samples keep the
// same Y across renders; only the rightmost segment morphs as
// progress advances. Eliminates the "jumping in batches" auto-scale
// readers see when min/max shifted every poll.
//
// Animation rules vs. `openwhisper-animation-philosophy` (T3):
//   - Animation state in refs only (dataRef, ceilingRef,
//     lastSampleTimeRef, reducedMotionRef). React never re-renders
//     for the per-frame motion.
//   - **Trade-off, logged here per the skill rule:** path d= is
//     written every frame. The skill calls for transform/opacity-
//     only without an explicit perf trade-off note. Justification:
//     genuine sample-to-sample interpolation cannot be expressed
//     via transform alone, and the user explicitly asked for
//     continuous flow. Cost is ~0.2 ms per frame for a 60-point
//     Catmull-Rom path on a 600×96 SVG — well under the 16 ms RAF
//     budget. Reduced motion short-circuits to progress=1 so the
//     chart still updates per poll but doesn't animate frame-by-
//     frame.
function Sparkline({
  data,
  peakBytes,
}: {
  data: number[];
  peakBytes: number;
}) {
  const linePathRef = useRef<SVGPathElement | null>(null);
  const fillPathRef = useRef<SVGPathElement | null>(null);
  const dataRef = useRef<number[]>(data);
  const ceilingRef = useRef<number>(niceCeiling(peakBytes));
  const lastSampleTimeRef = useRef<number>(performance.now());
  const reducedMotionRef = useRef(false);

  useLayoutEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    const update = () => {
      reducedMotionRef.current = mq.matches;
    };
    update();
    mq.addEventListener("change", update);
    return () => mq.removeEventListener("change", update);
  }, []);

  // Track new data + monotonically raise the Y ceiling. The ceiling
  // never shrinks, so the chart's vertical scale is stable through
  // a session even as RSS dips back down.
  useLayoutEffect(() => {
    if (data !== dataRef.current) {
      dataRef.current = data;
      lastSampleTimeRef.current = performance.now();
    }
    const next = niceCeiling(peakBytes);
    if (next > ceilingRef.current) ceilingRef.current = next;
  }, [data, peakBytes]);

  // Per-frame path rebuild. Reads refs only.
  useLayoutEffect(() => {
    let raf = 0;
    const tick = () => {
      const buf = dataRef.current;
      const ceiling = ceilingRef.current;
      const linePath = linePathRef.current;
      const fillPath = fillPathRef.current;
      if (linePath && fillPath && ceiling > 0 && buf.length >= 2) {
        const elapsed = performance.now() - lastSampleTimeRef.current;
        const progress = reducedMotionRef.current
          ? 1
          : Math.max(0, Math.min(1, elapsed / SPARK_TICK_MS));
        const built = buildLivePath(buf, progress, ceiling);
        if (built) {
          linePath.setAttribute("d", built.line);
          fillPath.setAttribute("d", built.fill);
        }
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  return (
    <svg
      className="ow-diagnostics__spark"
      viewBox={`0 0 ${SPARK_W_VB} ${SPARK_H_VB}`}
      preserveAspectRatio="none"
      role="img"
      aria-label="OpenWhisper RSS over the last 60 seconds"
    >
      <defs>
        <linearGradient id="ow-diag-spark-fill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="var(--primary)" stopOpacity="0.32" />
          <stop offset="100%" stopColor="var(--primary)" stopOpacity="0" />
        </linearGradient>
      </defs>
      <path ref={fillPathRef} d="" fill="url(#ow-diag-spark-fill)" />
      <path
        ref={linePathRef}
        d=""
        fill="none"
        stroke="var(--primary)"
        strokeWidth="1.6"
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  );
}

// Build the path for one RAF frame. Composes:
//   - buf[0..N-2] at fixed positions, sliding leftward by progress.
//   - A live interp point at the right edge whose Y blends from
//     buf[N-2] (at progress=0) to buf[N-1] (at progress=1).
//   - A phantom one pitch past the interp with the same Y, so the
//     curve always reaches the SVG right edge without a gap.
//
// Y axis is fixed at [0, ceiling]; older samples keep their Y
// across renders. At swap (progress jumps 1→0 with new data shifted
// by one slot) every screen position has the same Y on both sides.
function buildLivePath(
  data: number[],
  progress: number,
  ceiling: number,
): { line: string; fill: string } | null {
  if (data.length < 2 || ceiling <= 0) return null;
  const N = data.length;
  const usable = SPARK_H_VB - SPARK_PAD_TOP - SPARK_PAD_BOT;
  const pxPerSampleVB = SPARK_W_VB / (RSS_SERIES_LEN - 1);
  const offsetVB = (RSS_SERIES_LEN - N) * pxPerSampleVB;
  const slide = -progress * pxPerSampleVB;
  const yOf = (v: number) =>
    SPARK_PAD_TOP + usable - Math.min(1, v / ceiling) * usable;

  const points: Array<{ x: number; y: number }> = [];
  // Fixed older samples.
  for (let i = 0; i < N - 1; i++) {
    points.push({ x: offsetVB + i * pxPerSampleVB + slide, y: yOf(data[i]) });
  }
  // Live interp at right edge.
  const interpY = yOf(data[N - 2] + progress * (data[N - 1] - data[N - 2]));
  points.push({
    x: offsetVB + (N - 1) * pxPerSampleVB + slide,
    y: interpY,
  });
  // Phantom past right edge — same Y so the right-edge segment is
  // horizontal, fully covering the slide gap. Clipped by SVG.
  points.push({
    x: offsetVB + N * pxPerSampleVB + slide,
    y: interpY,
  });

  const line = catmullRomToBezier(points);
  const fillStartX = points[0].x;
  const fillEndX = points[points.length - 1].x;
  const fill = `${line} L ${fillEndX.toFixed(2)} ${SPARK_H_VB} L ${fillStartX.toFixed(2)} ${SPARK_H_VB} Z`;
  return { line, fill };
}

// Stable Y ceiling — round peak * 1.2 up to the next nice memory
// boundary. Stable across a session: ceiling steps up only when
// peak crosses a ladder rung. The ladder uses powers-of-two-ish MB
// so each band reads as a familiar memory budget.
function niceCeiling(peakBytes: number): number {
  const MB = 1024 * 1024;
  const ladder = [128, 256, 512, 1024, 2048, 4096, 8192, 16384].map(
    (m) => m * MB,
  );
  const need = peakBytes * 1.2;
  for (const c of ladder) {
    if (c >= need) return c;
  }
  return ladder[ladder.length - 1];
}

// Catmull-Rom cubic Bezier interpolation, tension 0.5 — the same
// "monotone-ish" smoothing shadcn's Area Chart uses by default. For
// each segment Pᵢ→Pᵢ₊₁, control points use the surrounding pair
// (Pᵢ₋₁, Pᵢ₊₂) to set tangents; endpoints duplicate so the curve
// terminates with zero second-derivative. RSS time-series rarely
// have huge spikes between consecutive 1 Hz samples, so the small
// overshoot Catmull-Rom can produce stays imperceptible.
function catmullRomToBezier(pts: Array<{ x: number; y: number }>): string {
  if (pts.length === 0) return "";
  if (pts.length === 1) return `M ${pts[0].x.toFixed(2)} ${pts[0].y.toFixed(2)}`;
  const tension = 0.5;
  const n = pts.length;
  let d = `M ${pts[0].x.toFixed(2)} ${pts[0].y.toFixed(2)}`;
  for (let i = 0; i < n - 1; i++) {
    const p0 = pts[Math.max(0, i - 1)];
    const p1 = pts[i];
    const p2 = pts[i + 1];
    const p3 = pts[Math.min(n - 1, i + 2)];
    const c1x = p1.x + ((p2.x - p0.x) * tension) / 3;
    const c1y = p1.y + ((p2.y - p0.y) * tension) / 3;
    const c2x = p2.x - ((p3.x - p1.x) * tension) / 3;
    const c2y = p2.y - ((p3.y - p1.y) * tension) / 3;
    d += ` C ${c1x.toFixed(2)} ${c1y.toFixed(2)} ${c2x.toFixed(2)} ${c2y.toFixed(2)} ${p2.x.toFixed(2)} ${p2.y.toFixed(2)}`;
  }
  return d;
}

// Stacked horizontal bar — one segment per loaded model handle plus an
// "Other" remainder covering everything we can't attribute (audio
// buffers, app shell, caches, OS overhead). All values are real
// bytes from `telemetry_get_memory`; we never invent finer
// granularity than the registry exposes.
function Breakdown({
  rssBytes,
  models,
}: {
  rssBytes: number;
  models: ModelMemoryRow[];
}) {
  const segments = useMemo(() => buildSegments(rssBytes, models), [
    rssBytes,
    models,
  ]);
  const total = segments.reduce((s, p) => s + p.value, 0);

  return (
    <div
      className="ow-diagnostics__breakdown"
      data-testid="diagnostics-breakdown"
    >
      <div className="ow-diagnostics__breakdown-header">
        <span className="ow-diagnostics__caption">Resident breakdown</span>
        <span className="ow-diagnostics__breakdown-total">
          {formatBytes(rssBytes).value} {formatBytes(rssBytes).unit} resident
        </span>
      </div>
      <div className="ow-diagnostics__breakdown-bar">
        {segments.map((seg, i) => (
          <span
            key={seg.key}
            className="ow-diagnostics__breakdown-seg"
            data-kind={seg.kind}
            style={{
              flexGrow: total > 0 ? seg.value : i === 0 ? 1 : 0,
              flexShrink: 0,
              flexBasis: 0,
            }}
            title={`${seg.label}: ${formatBytes(seg.value).value} ${formatBytes(seg.value).unit}`}
          />
        ))}
      </div>
      <ul className="ow-diagnostics__breakdown-legend">
        {segments.map((seg) => (
          <li
            key={seg.key}
            className="ow-diagnostics__breakdown-item"
            data-kind={seg.kind}
            data-testid={`diagnostics-segment-${seg.key}`}
          >
            <span
              className="ow-diagnostics__breakdown-swatch"
              data-kind={seg.kind}
            />
            <span className="ow-diagnostics__breakdown-label">
              {seg.label}
            </span>
            <span className="ow-diagnostics__breakdown-value">
              {formatBytes(seg.value).value}
              <span className="ow-diagnostics__breakdown-unit">
                {" "}
                {formatBytes(seg.value).unit}
              </span>
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

interface Segment {
  key: string;
  label: string;
  value: number;
  kind: "model" | "other";
}

function buildSegments(rssBytes: number, models: ModelMemoryRow[]): Segment[] {
  const modelSegs: Segment[] = models
    .filter((m) => m.estimated_rss_bytes > 0)
    .map((m) => ({
      key: `model-${m.label}`,
      label: prettyLabel(m.label),
      value: m.estimated_rss_bytes,
      kind: "model" as const,
    }));
  const sumModels = modelSegs.reduce((s, m) => s + m.value, 0);
  // Saturating subtraction — the per-model deltas are RSS-delta
  // *snapshots* taken at load time. The live RSS can dip below the
  // sum (e.g. after compaction), which would otherwise render as a
  // negative "Other" stripe. Floor at 0 and trust the segments.
  const other = Math.max(0, rssBytes - sumModels);
  return [
    ...modelSegs,
    { key: "other", label: "Other", value: other, kind: "other" },
  ];
}

function countLoaded(models: ModelMemoryRow[]): number {
  return models.filter(
    (m) =>
      m.state === "Loaded" ||
      m.state === "Active" ||
      m.state === "Releasing",
  ).length;
}

function prettyLabel(label: string): string {
  // "recognizer" → "Recognizer", "cleanup-llm" → "Cleanup LLM"
  return label
    .split(/[-_]/)
    .map((part) => {
      if (part.length <= 3 && part === part.toLowerCase()) {
        // "llm", "ane" — keep as upper acronym
        return part.toUpperCase();
      }
      return part.charAt(0).toUpperCase() + part.slice(1);
    })
    .join(" ");
}

function formatBytes(bytes: number): { value: string; unit: "B" | "KB" | "MB" | "GB" } {
  if (bytes < 1024) return { value: String(bytes), unit: "B" };
  if (bytes < 1024 * 1024) {
    return { value: (bytes / 1024).toFixed(0), unit: "KB" };
  }
  if (bytes < 1024 * 1024 * 1024) {
    return { value: (bytes / (1024 * 1024)).toFixed(0), unit: "MB" };
  }
  return { value: (bytes / (1024 * 1024 * 1024)).toFixed(2), unit: "GB" };
}

function slug(s: string): string {
  return s.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}
