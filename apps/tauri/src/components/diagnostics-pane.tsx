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
            Last {RSS_SERIES_LEN} s
          </span>
        </div>

        <Sparkline data={series} />

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

// 60-sample area sparkline — Activity-Monitor-style continuous
// scroll. Two pieces:
//
//   1. A RAF clock continuously sets `<g>` transform from
//      `(now - lastSampleTime) / SPARK_TICK_MS`, ramping the slide
//      offset from 0 to -pxPerSample over the inter-poll interval.
//      No CSS transition + snap (which created the visible
//      pause-then-jump previous reviewers flagged); the offset is
//      derived from real time, so polls arriving early/late don't
//      produce a hitch.
//   2. Path uses Catmull-Rom cubic Bezier interpolation through the
//      sample points (tension 0.5), matching the curvy look of
//      shadcn's Area Chart `type="monotone"`. A phantom point one
//      sample-pitch past the right edge mirrors the latest Y so the
//      line covers the slide gap without an empty fringe; the SVG
//      clips beyond the viewBox.
//
// Animation rules vs. `openwhisper-animation-philosophy`:
//   - Animation state in refs only (lastDataRef, lastSampleTimeRef,
//     pxPerSampleCssRef, reducedMotionRef). React doesn't see the
//     RAF clock.
//   - Transform-only on the `<g>` — no per-frame d= morph.
//   - The path d= updates ONCE per data prop change (1 Hz). That's
//     React's normal render cadence, not animation.
//   - Reduced-motion: skip transform writes; the path d= still
//     updates per poll, so the chart redraws but doesn't flow.
//
// CSS transforms on SVG inner elements use CSS pixels, not viewBox
// units, when `preserveAspectRatio="none"` scales the SVG to its
// container. ResizeObserver tracks the rendered width and converts
// the per-sample step to CSS pixels so the slide pitch matches the
// path's visual sample pitch on any container width.
function Sparkline({ data }: { data: number[] }) {
  const svgRef = useRef<SVGSVGElement | null>(null);
  const groupRef = useRef<SVGGElement | null>(null);
  const lastDataRef = useRef<number[]>(data);
  const lastSampleTimeRef = useRef<number>(performance.now());
  const pxPerSampleCssRef = useRef(0);
  const reducedMotionRef = useRef(false);

  useLayoutEffect(() => {
    const el = svgRef.current;
    if (!el) return;
    const update = () => {
      const w = el.getBoundingClientRect().width;
      pxPerSampleCssRef.current = w / (RSS_SERIES_LEN - 1);
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

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

  // Continuous RAF clock — drives the left-slide between polls.
  // Resets implicitly when lastSampleTimeRef advances on new data.
  // No-op when reduced motion is on or width hasn't been measured
  // yet; the path itself still draws the latest data via React render.
  useLayoutEffect(() => {
    let raf = 0;
    const tick = () => {
      const g = groupRef.current;
      if (g && !reducedMotionRef.current && pxPerSampleCssRef.current > 0) {
        const elapsed = performance.now() - lastSampleTimeRef.current;
        const progress = Math.max(0, Math.min(1, elapsed / SPARK_TICK_MS));
        const offset = -progress * pxPerSampleCssRef.current;
        g.style.transform = `translate3d(${offset.toFixed(2)}px, 0, 0)`;
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  // Mark the moment a new poll's data arrived. The RAF clock above
  // sees lastSampleTimeRef advance and starts a fresh 0→-pxPerSample
  // ramp from the new "now" baseline. Because the path data has
  // shifted by one slot in the same render, every old sample's
  // *screen* position is preserved across the swap; only the new
  // rightmost sample (and the phantom past it) introduce new content.
  useLayoutEffect(() => {
    if (data === lastDataRef.current) return;
    const wasEmpty = lastDataRef.current.length === 0;
    lastDataRef.current = data;
    lastSampleTimeRef.current = performance.now();
    if (wasEmpty || data.length === 0) {
      // First sample (or cleared): make sure the transform is at
      // rest before the RAF tick reads `progress = 0`.
      const g = groupRef.current;
      if (g) g.style.transform = "translate3d(0,0,0)";
    }
  }, [data]);

  const path = useMemo(() => buildSparkPath(data), [data]);

  return (
    <svg
      ref={svgRef}
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
      <g ref={groupRef} className="ow-diagnostics__spark-group">
        {path && (
          <>
            <path d={path.fill} fill="url(#ow-diag-spark-fill)" />
            <path
              d={path.line}
              fill="none"
              stroke="var(--primary)"
              strokeWidth="1.6"
              strokeLinejoin="round"
              strokeLinecap="round"
            />
          </>
        )}
      </g>
    </svg>
  );
}

// Pin the latest sample to x=W (right edge); older samples extend
// leftward by one viewBox-pitch each. During initial fill the line
// grows toward x=0; once the ring is full it spans the full width
// and each new sample pushes the oldest off the left.
//
// One phantom point at x=W+pxPerSampleVB sharing the latest Y. The
// `<g>` slides leftward by pxPerSampleCss over the poll interval,
// so without the phantom the right edge would briefly bare a gap
// of pxPerSampleCss. The phantom fills it with a horizontal stub;
// when the next sample arrives the rightmost segment changes slope
// to the new value, but cubic Bezier smoothing keeps that local
// adjustment soft.
function buildSparkPath(
  data: number[],
): { line: string; fill: string } | null {
  if (data.length < 2) return null;
  const min = Math.min(...data);
  const max = Math.max(...data);
  const span = Math.max(1, max - min);
  const usable = SPARK_H_VB - SPARK_PAD_TOP - SPARK_PAD_BOT;
  const pxPerSampleVB = SPARK_W_VB / (RSS_SERIES_LEN - 1);
  const offsetVB = (RSS_SERIES_LEN - data.length) * pxPerSampleVB;
  const points: Array<{ x: number; y: number }> = data.map((v, i) => ({
    x: offsetVB + i * pxPerSampleVB,
    y: SPARK_PAD_TOP + usable - ((v - min) / span) * usable,
  }));
  // Phantom: one pitch past the latest, same Y. SVG clips it.
  const last = points[points.length - 1];
  points.push({ x: last.x + pxPerSampleVB, y: last.y });

  const line = catmullRomToBezier(points);
  const fillStartX = points[0].x;
  const fillEndX = points[points.length - 1].x;
  const fill = `${line} L ${fillEndX.toFixed(2)} ${SPARK_H_VB} L ${fillStartX.toFixed(2)} ${SPARK_H_VB} Z`;
  return { line, fill };
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
