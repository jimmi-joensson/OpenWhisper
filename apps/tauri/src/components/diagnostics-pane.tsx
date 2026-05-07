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

// 60-sample area sparkline. Smooth left-scroll on every poll —
// classic Activity-Monitor scroll: samples are pinned to fixed x
// positions in viewBox space (latest at the right edge), and the
// containing `<g>` translates left by one sample-width over the
// 1-Hz tick interval. At swap time the transform snaps back to 0;
// because the path data has shifted indices by one, every sample's
// screen position is preserved across the swap (no jump). The new
// sample appears at the right edge with no fanfare.
//
// All animation state in refs (per `openwhisper-animation-philosophy`).
// Transform-only — no width/height/d-attribute interpolation.
// Reduced-motion: snaps each tick; values still read.
//
// CSS transforms on SVG inner elements operate in CSS-pixel space,
// not viewBox units, when `preserveAspectRatio="none"` scales the
// SVG to its container. ResizeObserver tracks the rendered width
// and converts the per-sample step into the matching CSS-pixel
// distance so the slide aligns exactly with the path's visual sample
// pitch.
function Sparkline({ data }: { data: number[] }) {
  const svgRef = useRef<SVGSVGElement | null>(null);
  const groupRef = useRef<SVGGElement | null>(null);
  const lastDataRef = useRef<number[]>(data);
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

  // Animation kick — runs on every data prop change. Snap to
  // translateX=0 (so the slide always restarts from the right-pinned
  // position), force a reflow so the next style write is treated as
  // the start of a new transition, then schedule the slide to
  // -pxPerSample over the tick interval.
  useLayoutEffect(() => {
    const prev = lastDataRef.current;
    if (data === prev) return;
    lastDataRef.current = data;
    const g = groupRef.current;
    if (!g) return;
    const pxCss = pxPerSampleCssRef.current;
    const wasEmpty = prev.length === 0;

    g.style.transition = "none";
    g.style.transform = "translate3d(0,0,0)";

    if (
      reducedMotionRef.current ||
      wasEmpty ||
      data.length === 0 ||
      pxCss === 0
    ) {
      // First sample, empty data, reduced-motion, or pre-measure:
      // leave the group at rest. The path itself already draws the
      // latest data; a snap-cut is correct here.
      return;
    }

    void g.getBoundingClientRect();
    g.style.transition = `transform ${SPARK_TICK_MS}ms linear`;
    g.style.transform = `translate3d(-${pxCss}px,0,0)`;
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

// Pin the latest sample to x=W (right edge). Older samples extend
// leftward at one viewBox-unit-per-sample-pitch increments. During
// the initial fill the line just grows leftward toward x=0; once
// the ring is full it spans the full width and each new sample
// pushes the oldest off the left.
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
  const xs = data.map((_, i) => offsetVB + i * pxPerSampleVB);
  const ys = data.map(
    (v) => SPARK_PAD_TOP + usable - ((v - min) / span) * usable,
  );
  const line = xs
    .map(
      (x, i) =>
        `${i === 0 ? "M" : "L"} ${x.toFixed(1)} ${ys[i].toFixed(1)}`,
    )
    .join(" ");
  const fill = `${line} L ${xs[xs.length - 1].toFixed(1)} ${SPARK_H_VB} L ${xs[0].toFixed(1)} ${SPARK_H_VB} Z`;
  return { line, fill };
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
