import { useLayoutEffect, useRef, useState } from "react";
import {
  RSS_SERIES_LEN,
  breakdownEstimate,
  externalClaim,
  isRecognizerInProcessResident,
  useMemoryStats,
  type ModelMemoryRow,
  type PressureLevel,
  type SystemMemory,
} from "../lib/use-memory-stats";
import {
  formatRelative,
  useCrashes,
  type CrashSummary,
} from "../lib/use-crashes";
import { Card, CardContent } from "./ui/card";
import { CrashList } from "./crash-list";
import { CrashGlyph } from "./crash-empty";
import {
  RSSBreakdownBar,
  type RSSBreakdownPart,
} from "./diagnostics-rss-breakdown";

const SPARK_W_VB = 600;
const SPARK_H_VB = 96;
const SPARK_PAD_TOP = 6;
const SPARK_PAD_BOT = 4;
const SPARK_TICK_MS = 1000;

// Each series rides its own band of the canvas so the absolute
// scale gap between system memory (~20 GB) and OpenWhisper RSS
// (~1 GB) doesn't squash the smaller line into a flat ribbon. The
// upper 62% of the canvas is system; the lower 62% is RSS, with a
// 24% overlap zone where both can swing through.
const SYS_BAND_TOP_FRAC = 0.0;
const SYS_BAND_BOT_FRAC = 0.62;
const RSS_BAND_TOP_FRAC = 0.38;
const RSS_BAND_BOT_FRAC = 1.0;

export type Platform = "macos" | "windows";

export interface DiagnosticsPaneProps {
  platform?: Platform;
}

// Diagnostics — top-level route, sibling to Home and Settings. Today
// covers Memory + Crashes; per the 2026-05-07 design pivot, Crashes is
// reached as an entry card on the overview pane (NOT a sub-sidebar)
// and tapping it swaps the pane content to the full-pane crash list
// with a `Diagnostics /` breadcrumb. Per-model load/release controls
// live under Settings → Models — the budget bar there is the
// *decision* surface; this pane is the *debugging* surface.
type DiagnosticsView = "overview" | "crashes";

export function DiagnosticsPane(_props: DiagnosticsPaneProps) {
  const [view, setView] = useState<DiagnosticsView>("overview");
  const { markSeen } = useCrashes();

  if (view === "crashes") {
    return <CrashList onBack={() => setView("overview")} />;
  }

  // Entering the crash inspector is the explicit "read" action per
  // TASK-78.5 AC #4: it acknowledges the current unread count so the
  // delta-driven launch toast doesn't re-fire on next boot. Fire-and-
  // forget — failure leaves the snapshot self-healing via the poll.
  return (
    <DiagnosticsOverview
      onOpenCrashes={() => {
        void markSeen().catch(() => {});
        setView("crashes");
      }}
    />
  );
}

function DiagnosticsOverview({
  onOpenCrashes,
}: {
  onOpenCrashes: () => void;
}) {
  const {
    stats,
    openWhisperSeries,
    systemSeries,
    pressure,
    error,
    lastSampleTimeMs,
  } = useMemoryStats();
  // Hook polls list + unread at 2 Hz while the overview is mounted.
  // The card uses the freshest unread count + the latest summary row
  // to drive the "Last: <relative>" sub-line, per the design.
  const { list, unreadCount } = useCrashes();
  const latest: CrashSummary | undefined = list[0];

  return (
    <div className="ow-diagnostics">
      <header className="ow-diagnostics__header">
        <h1 className="ow-diagnostics__title">Diagnostics</h1>
        <p className="ow-diagnostics__sub">
          System and OpenWhisper memory at a glance. Per-model load and
          unload live in Settings → Models.
        </p>
      </header>

      <section className="ow-diagnostics__section">
        <div className="ow-diagnostics__section-header">
          <h2 className="ow-diagnostics__section-title">Memory</h2>
          <span className="ow-diagnostics__section-meta">
            Live · 1 Hz · {detectPlatformLabel()}
          </span>
        </div>
        <MemoryCard
          stats={stats}
          openWhisperSeries={openWhisperSeries}
          systemSeries={systemSeries}
          pressure={pressure}
          error={error}
          lastSampleTimeMs={lastSampleTimeMs}
        />
      </section>

      <section className="ow-diagnostics__section">
        <div className="ow-diagnostics__section-header">
          <h2 className="ow-diagnostics__section-title">Crashes</h2>
          <span className="ow-diagnostics__section-meta">Live · 2 Hz</span>
        </div>
        <CrashesEntryCard
          unreadCount={unreadCount}
          totalCount={list.length}
          latest={latest}
          onOpen={onOpenCrashes}
        />
      </section>
    </div>
  );
}

interface CrashesEntryCardProps {
  unreadCount: number;
  totalCount: number;
  latest: CrashSummary | undefined;
  onOpen: () => void;
}

function CrashesEntryCard({
  unreadCount,
  totalCount,
  latest,
  onOpen,
}: CrashesEntryCardProps) {
  // Quiet branch: no crashes ever recorded. Per the design, the card
  // is replaced by a one-line muted link rather than hidden — silence
  // is louder than a missing affordance.
  if (totalCount === 0) {
    return (
      <p
        className="ow-diagnostics__crashes-quiet"
        data-testid="diagnostics-crashes-quiet"
      >
        No crashes recorded ·{" "}
        <button
          type="button"
          className="ow-diagnostics__crashes-quiet-link"
          data-testid="diagnostics-crashes-open-folder"
          onClick={onOpen}
        >
          Open Crashes
        </button>
      </p>
    );
  }

  // Last-crash sub-line: relative when + version. Module/signal in the
  // design fixture (e.g. "recognizer.so · SIGSEGV") don't map to Rust
  // panics — we substitute the panic message's first segment so the
  // line still carries useful at-a-glance signal. Truncate hard.
  const subSig = latest ? truncateSignal(latest.message_truncated) : null;

  return (
    <button
      type="button"
      className="ow-crashes-card"
      data-testid="diagnostics-crashes-entry"
      onClick={onOpen}
    >
      <span className="ow-crashes-card__tile" aria-hidden="true">
        <CrashGlyph size={14} />
      </span>
      <span className="ow-crashes-card__body">
        <span className="ow-crashes-card__title-row">
          <span className="ow-crashes-card__title">Crash reports</span>
          {unreadCount > 0 && (
            <span
              className="ow-crashes-card__unread"
              data-testid="diagnostics-crashes-unread-pill"
            >
              {unreadCount} unread
            </span>
          )}
        </span>
        <span className="ow-crashes-card__sub">
          {latest
            ? `Last: ${formatRelative(latest.ts_unix_ms)} · ${subSig}`
            : `Last: —`}
        </span>
      </span>
      <span className="ow-crashes-card__caret" aria-hidden="true">
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none" stroke="currentColor" strokeWidth="1.3" strokeLinecap="round" strokeLinejoin="round">
          <path d="M3.5 2 L 7 5 L 3.5 8" />
        </svg>
      </span>
    </button>
  );
}

function truncateSignal(message: string): string {
  const max = 48;
  if (message.length <= max) return message;
  return message.slice(0, max) + "…";
}

// Platform label for the section meta — UA-derived since the React
// side has no Tauri shell context here. Used purely as a readout
// hint ("we are reading macOS counters", "we are reading Windows
// counters"). Falls back to a neutral label if UA sniffing fails;
// the underlying telemetry is the same call either way.
function detectPlatformLabel(): string {
  if (typeof navigator === "undefined") return "host";
  const ua = navigator.userAgent || "";
  if (/Mac|iPhone|iPad/.test(ua)) return "macOS";
  if (/Win/.test(ua)) return "Windows";
  if (/Linux/.test(ua)) return "Linux";
  return "host";
}

interface MemoryCardProps {
  stats: ReturnType<typeof useMemoryStats>["stats"];
  openWhisperSeries: number[];
  systemSeries: number[];
  pressure: PressureLevel;
  error: string | null;
  lastSampleTimeMs: number | null;
}

function MemoryCard({
  stats,
  openWhisperSeries,
  systemSeries,
  pressure,
  error,
  lastSampleTimeMs,
}: MemoryCardProps) {
  const rss = stats?.process.rss_bytes ?? 0;
  const peak = stats?.process.peak_rss_bytes ?? 0;
  const system: SystemMemory = stats?.system ?? {
    total_bytes: 0,
    used_bytes: 0,
    available_bytes: 0,
    swap_total_bytes: 0,
    swap_used_bytes: 0,
  };
  const models = stats?.models ?? [];
  const ane = externalClaim(models);
  const total = rss + ane;

  const sysUsedFmt = formatBytes(system.used_bytes);
  const sysTotalFmt = formatBytes(system.total_bytes);
  const totalFmt = formatBytes(total);
  const rssFmt = formatBytes(rss);
  const aneFmt = formatBytes(ane);
  const peakFmt = formatBytes(peak);
  // Sub-text exposes the in-process / out-of-process split when an
  // ANE-resident model is loaded; otherwise fall back to the peak
  // RSS so the readout still has context on Windows / cold start.
  const owSub =
    ane > 0
      ? `${rssFmt.value} ${rssFmt.unit} process + ${aneFmt.value} ${aneFmt.unit} ANE / GPU`
      : `peak ${peakFmt.value} ${peakFmt.unit}`;
  // Y-ceiling for the OpenWhisper line: the larger of the historical
  // peak RSS and the current total, so the line never clips when
  // the ANE claim alone exceeds peak RSS (typical on Mac).
  const owCeiling = Math.max(peak, total);
  // Min / peak across the buffered window — useful at-a-glance signal
  // for "did we spike recently?" without requiring the user to read
  // the chart's Y-axis. min/max are honest 0-default until the ring
  // has any samples.
  const owMin = openWhisperSeries.length > 0
    ? Math.min(...openWhisperSeries)
    : total;
  const owPeak = openWhisperSeries.length > 0
    ? Math.max(...openWhisperSeries)
    : total;
  const owMinFmt = formatBytes(owMin);
  const owPeakFmt = formatBytes(owPeak);

  return (
    <Card size="sm" className="ow-diagnostics__card">
      <CardContent className="flex flex-col gap-4">
        <div className="ow-diagnostics__readout-row-top">
          <div className="ow-diagnostics__readouts">
            <Readout
              label="System Memory Used"
              value={sysUsedFmt.value}
              unit={sysUsedFmt.unit}
              sub={`of ${sysTotalFmt.value} ${sysTotalFmt.unit}`}
              swatchKind="system"
            />
            <Readout
              label="OpenWhisper Memory"
              value={totalFmt.value}
              unit={totalFmt.unit}
              sub={owSub}
              swatchKind="app"
              emphasised
            />
          </div>
          <div className="ow-diagnostics__window-stats">
            <span className="ow-diagnostics__caption">
              Last {RSS_SERIES_LEN} s
            </span>
            <span
              className="ow-diagnostics__window-extrema"
              data-testid="diagnostics-window-extrema"
            >
              min {owMinFmt.value} {owMinFmt.unit} · peak {owPeakFmt.value}{" "}
              {owPeakFmt.unit}
            </span>
          </div>
        </div>

        <DualSparkline
          systemSeries={systemSeries}
          rssSeries={openWhisperSeries}
          systemTotalBytes={system.total_bytes}
          rssCeilingBytes={owCeiling}
          lastSampleTimeMs={lastSampleTimeMs}
        />

        <div className="ow-diagnostics__legend-row">
          <div className="ow-diagnostics__legend">
            <LegendDot kind="system" label="System Memory" />
            <LegendDot kind="app" label="OpenWhisper" />
          </div>
          <PressurePill level={pressure} />
        </div>

        <RSSBreakdown rssBytes={rss} models={models} />

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
  swatchKind,
  emphasised,
}: {
  label: string;
  value: string;
  unit: string;
  sub: string;
  swatchKind: "system" | "app";
  emphasised?: boolean;
}) {
  return (
    <div className="ow-diagnostics__readout" data-emphasised={emphasised}>
      <span
        className="ow-diagnostics__readout-swatch"
        data-kind={swatchKind}
      />
      <div className="ow-diagnostics__readout-body">
        <div className="ow-diagnostics__readout-label">{label}</div>
        <div className="ow-diagnostics__readout-value-row">
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
    </div>
  );
}

function LegendDot({
  kind,
  label,
}: {
  kind: "system" | "app";
  label: string;
}) {
  return (
    <span className="ow-diagnostics__legend-item">
      <span
        className="ow-diagnostics__legend-swatch"
        data-kind={kind}
      />
      <span>{label}</span>
    </span>
  );
}

function PressurePill({ level }: { level: PressureLevel }) {
  const labelMap: Record<PressureLevel, string> = {
    normal: "Normal",
    warning: "Warning",
    critical: "Critical",
  };
  return (
    <span
      className="ow-diagnostics__pressure"
      data-testid="diagnostics-pressure"
      data-level={level}
    >
      <span className="ow-diagnostics__pressure-caption">Pressure</span>
      <span className="ow-diagnostics__pressure-value">
        <span
          className="ow-diagnostics__pressure-dot"
          data-level={level}
        />
        <span data-testid="diagnostics-pressure-label">
          {labelMap[level]}
        </span>
      </span>
    </span>
  );
}

// Two-series area chart — System Memory Used (upper band) and
// OpenWhisper RSS (lower band). Each series scales to its own min/max
// inside its band so both lines have visible texture even when their
// absolute values differ by an order of magnitude (20 GB vs 1 GB).
//
// Per-frame RAF rebuilds both path d= attributes from a single
// time-derived `progress` parameter (0..1, ramping over the
// inter-poll interval). Three pieces compose each visible curve:
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
// Y axes: each band has its own monotonic ceiling — system uses
// `total_bytes` as a fixed ceiling (the box is N GB, full stop);
// RSS uses the same nice-ladder ceiling derived from `peak_rss`
// that the previous single-series sparkline used.
//
// Animation rules vs. `openwhisper-animation-philosophy` (T3):
//   - Animation state in refs only. React never re-renders for
//     the per-frame motion.
//   - **Trade-off, logged here per the skill rule:** path d= is
//     written every frame, twice (once per series). The skill
//     calls for transform/opacity-only without an explicit perf
//     trade-off note. Justification: genuine sample-to-sample
//     interpolation cannot be expressed via transform alone, and
//     the user explicitly asked for continuous flow. Cost is
//     ~0.4 ms per frame for two 60-point Catmull-Rom paths on a
//     600×96 SVG — well under the 16 ms RAF budget.
//   - **Reduced-motion carve-out:** the slide is intentionally
//     NOT gated on `prefers-reduced-motion: reduce`. Per the
//     skill's "essential continuous data motion" exception, the
//     right-edge advancement is part of how the chart reads as
//     "live" — without it, every Windows-RDP user (where
//     `SPI_GETCLIENTAREAANIMATION` is auto-off) and every user
//     who toggled "Show animations" off sees the chart freeze
//     between polls and update in 1 Hz steps. The motion is
//     monotonic, sub-pixel per frame, ~10 px/sec — none of the
//     attention-grabbing motion (parallax, autoplay, rotation)
//     WCAG 2.3.3 was written to mitigate.
function DualSparkline({
  systemSeries,
  rssSeries,
  systemTotalBytes,
  rssCeilingBytes,
  lastSampleTimeMs,
}: {
  systemSeries: number[];
  rssSeries: number[];
  systemTotalBytes: number;
  rssCeilingBytes: number;
  lastSampleTimeMs: number | null;
}) {
  const sysLineRef = useRef<SVGPathElement | null>(null);
  const sysFillRef = useRef<SVGPathElement | null>(null);
  const rssLineRef = useRef<SVGPathElement | null>(null);
  const rssFillRef = useRef<SVGPathElement | null>(null);
  const sysDataRef = useRef<number[]>(systemSeries);
  const rssDataRef = useRef<number[]>(rssSeries);
  const sysCeilingRef = useRef<number>(Math.max(systemTotalBytes, 1));
  const rssCeilingRef = useRef<number>(niceCeiling(rssCeilingBytes));
  // Seed the animation clock from the store's last sample time so a
  // remount after navigation picks up the slide at the same `progress`
  // value the previous mount would have shown — no leftward jump
  // when the user re-enters the pane mid-second. Falls back to
  // `now` on cold start before the first sample lands.
  const lastSampleTimeRef = useRef<number>(
    lastSampleTimeMs ?? performance.now(),
  );

  // Track new data + monotonically raise the RSS ceiling. The system
  // ceiling is the host's physical total — fixed for the session.
  // Anchor the animation clock to the store's `lastSampleTimeMs`
  // (the wall-clock time of the most recent ring push) rather than
  // `performance.now()` at effect time, so layout-tick jitter
  // doesn't accumulate phase error across remounts.
  useLayoutEffect(() => {
    let bumpedSampleTime = false;
    if (systemSeries !== sysDataRef.current) {
      sysDataRef.current = systemSeries;
      if (lastSampleTimeMs !== null) {
        lastSampleTimeRef.current = lastSampleTimeMs;
      }
      bumpedSampleTime = true;
    }
    if (rssSeries !== rssDataRef.current) {
      rssDataRef.current = rssSeries;
      if (!bumpedSampleTime && lastSampleTimeMs !== null) {
        lastSampleTimeRef.current = lastSampleTimeMs;
      }
    }
    if (systemTotalBytes > sysCeilingRef.current) {
      sysCeilingRef.current = systemTotalBytes;
    }
    const nextRss = niceCeiling(rssCeilingBytes);
    if (nextRss > rssCeilingRef.current) rssCeilingRef.current = nextRss;
  }, [systemSeries, rssSeries, systemTotalBytes, rssCeilingBytes, lastSampleTimeMs]);

  // Per-frame path rebuild for both series. Reads refs only.
  useLayoutEffect(() => {
    let raf = 0;
    const tick = () => {
      const elapsed = performance.now() - lastSampleTimeRef.current;
      const progress = Math.max(0, Math.min(1, elapsed / SPARK_TICK_MS));

      const sys = sysDataRef.current;
      const sysLine = sysLineRef.current;
      const sysFill = sysFillRef.current;
      if (
        sysLine &&
        sysFill &&
        sysCeilingRef.current > 0 &&
        sys.length >= 2
      ) {
        const built = buildLivePath(sys, progress, {
          ceiling: sysCeilingRef.current,
          floor: 0,
          bandTopFrac: SYS_BAND_TOP_FRAC,
          bandBotFrac: SYS_BAND_BOT_FRAC,
        });
        if (built) {
          sysLine.setAttribute("d", built.line);
          sysFill.setAttribute("d", built.fill);
        }
      }

      const rss = rssDataRef.current;
      const rssLine = rssLineRef.current;
      const rssFill = rssFillRef.current;
      if (
        rssLine &&
        rssFill &&
        rssCeilingRef.current > 0 &&
        rss.length >= 2
      ) {
        const built = buildLivePath(rss, progress, {
          ceiling: rssCeilingRef.current,
          floor: 0,
          bandTopFrac: RSS_BAND_TOP_FRAC,
          bandBotFrac: RSS_BAND_BOT_FRAC,
        });
        if (built) {
          rssLine.setAttribute("d", built.line);
          rssFill.setAttribute("d", built.fill);
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
      aria-label="System memory and OpenWhisper RSS over the last 60 seconds"
    >
      <defs>
        <linearGradient id="ow-diag-sys-fill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="var(--muted-foreground)" stopOpacity="0.28" />
          <stop offset="100%" stopColor="var(--muted-foreground)" stopOpacity="0" />
        </linearGradient>
        <linearGradient id="ow-diag-rss-fill" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="var(--info)" stopOpacity="0.42" />
          <stop offset="100%" stopColor="var(--info)" stopOpacity="0" />
        </linearGradient>
      </defs>
      {/* Faint horizontal gridlines so the bands read as a single
          chart, not two stacked tiles. */}
      <line
        x1="0"
        x2={SPARK_W_VB}
        y1={SPARK_H_VB * 0.25}
        y2={SPARK_H_VB * 0.25}
        className="ow-diagnostics__grid"
      />
      <line
        x1="0"
        x2={SPARK_W_VB}
        y1={SPARK_H_VB * 0.5}
        y2={SPARK_H_VB * 0.5}
        className="ow-diagnostics__grid"
      />
      <line
        x1="0"
        x2={SPARK_W_VB}
        y1={SPARK_H_VB * 0.75}
        y2={SPARK_H_VB * 0.75}
        className="ow-diagnostics__grid"
      />
      <path
        ref={sysFillRef}
        d=""
        fill="url(#ow-diag-sys-fill)"
      />
      <path
        ref={sysLineRef}
        d=""
        fill="none"
        stroke="var(--muted-foreground)"
        strokeWidth="1.4"
        strokeLinejoin="round"
        strokeLinecap="round"
      />
      <path
        ref={rssFillRef}
        d=""
        fill="url(#ow-diag-rss-fill)"
      />
      <path
        ref={rssLineRef}
        d=""
        fill="none"
        stroke="var(--info)"
        strokeWidth="1.6"
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  );
}

interface BandSpec {
  ceiling: number;
  floor: number;
  bandTopFrac: number;
  bandBotFrac: number;
}

// Build the path for one RAF frame inside a given Y band. Composes:
//   - buf[0..N-2] at fixed positions, sliding leftward by progress.
//   - A live interp point at the right edge whose Y blends from
//     buf[N-2] (at progress=0) to buf[N-1] (at progress=1).
//   - A phantom one pitch past the interp with the same Y, so the
//     curve always reaches the SVG right edge without a gap.
//
// Y axis maps `[floor, ceiling]` into the `[bandTopFrac, bandBotFrac]`
// slice of the canvas (with `SPARK_PAD_TOP/BOT` honoured globally),
// so two series can ride different bands without dual <svg>s.
function buildLivePath(
  data: number[],
  progress: number,
  band: BandSpec,
): { line: string; fill: string } | null {
  if (data.length < 2 || band.ceiling <= band.floor) return null;
  const N = data.length;
  const usable = SPARK_H_VB - SPARK_PAD_TOP - SPARK_PAD_BOT;
  const yTop = SPARK_PAD_TOP + usable * band.bandTopFrac;
  const yBot = SPARK_PAD_TOP + usable * band.bandBotFrac;
  const yRange = yBot - yTop;
  const span = band.ceiling - band.floor;
  const pxPerSampleVB = SPARK_W_VB / (RSS_SERIES_LEN - 1);
  const offsetVB = (RSS_SERIES_LEN - N) * pxPerSampleVB;
  const slide = -progress * pxPerSampleVB;
  const yOf = (v: number) =>
    yBot - Math.max(0, Math.min(1, (v - band.floor) / span)) * yRange;

  const points: Array<{ x: number; y: number }> = [];
  for (let i = 0; i < N - 1; i++) {
    points.push({ x: offsetVB + i * pxPerSampleVB + slide, y: yOf(data[i]) });
  }
  const interpY = yOf(data[N - 2] + progress * (data[N - 1] - data[N - 2]));
  points.push({
    x: offsetVB + (N - 1) * pxPerSampleVB + slide,
    y: interpY,
  });
  points.push({
    x: offsetVB + N * pxPerSampleVB + slide,
    y: interpY,
  });

  const line = catmullRomToBezier(points);
  const fillStartX = points[0].x;
  const fillEndX = points[points.length - 1].x;
  const fill = `${line} L ${fillEndX.toFixed(2)} ${yBot.toFixed(2)} L ${fillStartX.toFixed(2)} ${yBot.toFixed(2)} Z`;
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
// "monotone-ish" smoothing shadcn's Area Chart uses by default.
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

// Wraps the prop-driven `<RSSBreakdownBar />` with the four
// canonical segments the design pivot calls for. Source of
// per-segment values is `breakdownEstimate` (process RSS) plus
// `externalClaim` (ANE/GPU on Mac) — a v1 placeholder until
// per-component RSS attribution lands. See
// `diagnostics-rss-breakdown.tsx` for the presentation layer.
//
// Parakeet weights are sourced from process RSS on Windows
// (recognizer is in-process via ort) and from ANE/GPU on Mac
// (recognizer lives outside the host RSS via FluidAudio). One
// `parakeet` segment either way so the bar reads as a single
// memory total — whichever side carries the weights, the bar
// matches the "OpenWhisper Memory" readout above.
function RSSBreakdown({
  rssBytes,
  models,
}: {
  rssBytes: number;
  models: ModelMemoryRow[];
}) {
  const rssMb = Math.round(rssBytes / (1024 * 1024));
  const recognizerInProcess = isRecognizerInProcessResident(models);
  const aneMb = Math.round(externalClaim(models) / (1024 * 1024));
  const est = breakdownEstimate(rssMb, recognizerInProcess);
  // In-process build keeps Parakeet inside RSS (Windows ort path);
  // out-of-process build attributes the ANE claim instead (Mac
  // FluidAudio path). aneMb=0 with no recognizer loaded → segment hides.
  const parakeetMb = recognizerInProcess ? est.parakeetMb : aneMb;
  const parakeetLabel = recognizerInProcess
    ? "Parakeet weights"
    : "Parakeet weights (ANE)";
  // Order matches the design (largest expected → smallest).
  // Color tokens match doc-43 §"OW RSS Breakdown — segment composition".
  const parts: RSSBreakdownPart[] = [
    {
      kind: "parakeet",
      label: parakeetLabel,
      valueMb: parakeetMb,
      color: "var(--info)",
    },
    {
      kind: "audio",
      label: "Audio buffers",
      valueMb: est.audioBuffersMb,
      color: "color-mix(in oklch, var(--info) 65%, var(--background))",
    },
    {
      kind: "shell",
      label: "App shell",
      valueMb: est.appShellMb,
      color: "color-mix(in oklch, var(--info) 35%, var(--background))",
    },
    {
      kind: "caches",
      label: "Caches",
      valueMb: est.cachesMb,
      color: "color-mix(in oklch, var(--muted-foreground) 60%, transparent)",
    },
  ];
  return <RSSBreakdownBar parts={parts} />;
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
