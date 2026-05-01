import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import {
  INITIAL_PILL_STATE,
  PILL_STATE_EVENT,
  type PillState,
  type PillStatus,
} from "./lib/pill-state";
import "./PillOverlay.css";

// Outer capsule width per state. Height (22) and radius (11) stay fixed.
// Inner SVG canvas is fixed at PILL_INNER_W × PILL_INNER_H; the capsule has
// box-sizing: border-box + overflow: hidden so narrowing clips the SVG.
const PILL_OUTER_W: Record<PillStatus, number> = {
  idle: 38,
  recording: 70,
  transcribing: 38,
};
// Outer scale per state. 1 at idle, 2 during recording/transcribing. Driven
// by a hand-rolled 2nd-order spring (asymmetric: subtle overshoot on grow,
// critically damped on shrink) so the morph reads as a Dynamic-Island-style
// alive badge rather than a timed tween.
const PILL_SCALE: Record<PillStatus, number> = {
  idle: 1,
  recording: 2,
  transcribing: 2,
};
const SPRING_GROW = { stiffness: 220, damping: 24 }; // ~18% overshoot
const SPRING_SHRINK = { stiffness: 280, damping: 34 }; // critically damped
const PILL_INNER_W = 54;
const PILL_INNER_H = 12;
const PARTICLE_COUNT = 12;
const RECORDING_FILL = "#E07000";
const WHITE_FILL = "rgb(255 255 255)";

const SPHERE_PULSE = { inflate: 0.12, deflate: 0.16, rotSpeed: 2, pulseSpeed: 1 };

const easeOutQuint = (t: number) => 1 - Math.pow(1 - t, 5);
const easeOutBack = (t: number) => {
  const c1 = 1.4;
  const c3 = c1 + 1;
  return 1 + c3 * Math.pow(t - 1, 3) + c1 * Math.pow(t - 1, 2);
};
const easeInOutQuart = (t: number) =>
  t < 0.5 ? 8 * t * t * t * t : 1 - Math.pow(-2 * t + 2, 4) / 2;

const owNormalize = (amp: number) => {
  const a = Math.max(amp, 1e-6);
  return Math.min(1, Math.max(0, (20 * Math.log10(a) + 55) / 55));
};

type FillKind = "recording" | "white";

interface Particle {
  x: number;
  y: number;
  w: number;
  h: number;
  fill: FillKind;
  opacity: number;
}

const IDLE_ANCHOR_R = 1.5;
const IDLE_CLUSTER_CX = [22, 27, 32];

function idleTarget(i: number): Particle {
  const cluster = Math.floor(i / 4);
  const within = i % 4;
  const cx = IDLE_CLUSTER_CX[cluster];
  return {
    x: cx,
    y: PILL_INNER_H / 2,
    w: IDLE_ANCHOR_R * 2,
    h: IDLE_ANCHOR_R * 2,
    fill: "white",
    opacity: within === 0 ? 0.7 : 0,
  };
}

function recordingTarget(i: number, level: number, gate: number): Particle {
  const stride = 4;
  const totalW = PARTICLE_COUNT * stride - 2;
  const startX = (PILL_INNER_W - totalW) / 2;
  const cx = startX + i * stride + 1;
  const norm = owNormalize(level || 0) * gate;
  const h = Math.max(2, Math.round(norm * 10));
  return {
    x: cx,
    y: PILL_INNER_H / 2,
    w: 2,
    h,
    fill: "recording",
    opacity: 1,
  };
}

// Fibonacci-lattice unit-sphere points; rotated y-axis + projected to 2D.
const SPHERE_POINTS = (() => {
  const pts: { x: number; y: number; z: number }[] = [];
  const phi = Math.PI * (Math.sqrt(5) - 1);
  for (let i = 0; i < PARTICLE_COUNT; i++) {
    const y = 1 - (i / (PARTICLE_COUNT - 1)) * 2;
    const r = Math.sqrt(1 - y * y);
    const theta = phi * i;
    pts.push({ x: Math.cos(theta) * r, y, z: Math.sin(theta) * r });
  }
  return pts;
})();

function transcribingTarget(i: number, t: number): Particle {
  const { inflate, deflate, rotSpeed, pulseSpeed } = SPHERE_PULSE;
  const cx = PILL_INNER_W / 2;
  const cy = PILL_INNER_H / 2;
  const s = Math.sin(t * 0.0039 * pulseSpeed);
  const pulse = 1 + (s >= 0 ? s * inflate : s * deflate);
  const radius = 4.6 * pulse;
  const rotation = t * 0.0009 * rotSpeed;
  const p = SPHERE_POINTS[i];
  const cosR = Math.cos(rotation);
  const sinR = Math.sin(rotation);
  const rx = p.x * cosR + p.z * sinR;
  const rz = -p.x * sinR + p.z * cosR;
  const ry = p.y;
  const x = cx + rx * radius;
  const y = cy + ry * radius;
  const depth = (rz + 1) / 2;
  // Mass tuned so dots read clearly at 1× on a 38×22 capsule with backdrop
  // blur. Front dots ~3 px, back dots ~1.5 px; opacity floor lifted so the
  // back hemisphere still registers instead of fading into the material.
  const size = (1.5 + depth * 1.6) * pulse;
  const opacity = 0.4 + depth * 0.55;
  return { x, y, w: size, h: size, fill: "white", opacity };
}

interface Tween {
  from: Particle[] | null;
  fromWidth: number;
  start: number;
  duration: number;
  status: PillStatus;
  fromStatus: PillStatus;
}

export function PillOverlay() {
  const [renderedStatus, setRenderedStatus] = useState<PillStatus>("idle");

  const stateRef = useRef<PillState>(INITIAL_PILL_STATE);
  const statusRef = useRef<PillStatus>("idle");
  const particlesRef = useRef<Particle[]>(
    Array.from({ length: PARTICLE_COUNT }, (_, i) => idleTarget(i)),
  );
  const widthRef = useRef<number>(PILL_OUTER_W.idle);
  const scaleStateRef = useRef<{ x: number; v: number }>({ x: 1, v: 0 });
  const prevScaleWriteRef = useRef<number>(1);
  const prevTickRef = useRef<number>(0);
  const tweenRef = useRef<Tween>({
    from: null,
    fromWidth: PILL_OUTER_W.idle,
    start: 0,
    duration: 0,
    status: "idle",
    fromStatus: "idle",
  });

  const capsuleRef = useRef<HTMLDivElement>(null);
  const rectRefs = useRef<(SVGRectElement | null)[]>(
    Array.from({ length: PARTICLE_COUNT }, () => null),
  );
  const prevFillRef = useRef<FillKind[]>(
    Array.from({ length: PARTICLE_COUNT }, () => "white"),
  );
  const prevOpacityRef = useRef<number[]>(
    Array.from({ length: PARTICLE_COUNT }, () => -1),
  );

  // Listen for state from main window. Only status changes trigger a tween +
  // React re-render (so click-through invoke fires). Levels stream into the
  // ref and are read by the RAF loop without rendering.
  useEffect(() => {
    const unlisten = listen<PillState>(PILL_STATE_EVENT, (event) => {
      const next = event.payload;
      const prev = stateRef.current;
      stateRef.current = next;

      if (prev.status !== next.status) {
        const sphere = next.status === "transcribing" || prev.status === "transcribing";
        tweenRef.current = {
          from: particlesRef.current.map((p) => ({ ...p })),
          fromWidth: widthRef.current,
          start: performance.now(),
          duration: sphere ? 820 : 520,
          status: next.status,
          fromStatus: prev.status,
        };
        statusRef.current = next.status;
        setRenderedStatus(next.status);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Click-through follows status: idle = clickable, otherwise pass-through.
  useEffect(() => {
    const clickable = renderedStatus === "idle";
    invoke("set_pill_click_through", { passthrough: !clickable }).catch(
      // eslint-disable-next-line no-console
      (e) => console.warn("set_pill_click_through failed", e),
    );
  }, [renderedStatus]);

  useEffect(() => {
    invoke("reposition_pill", { monitor_origin: null }).catch(
      // eslint-disable-next-line no-console
      (e) => console.warn("reposition_pill failed", e),
    );
  }, []);

  // RAF loop — mutates DOM only; no React state writes per frame.
  useEffect(() => {
    let raf = 0;

    const writeParticle = (i: number, p: Particle) => {
      const node = rectRefs.current[i];
      if (!node) return;
      // Centered rect translated to (p.x, p.y). One transform write per frame
      // plus size attrs; keeps SVG layout invalidation minimal at 12 nodes.
      node.setAttribute(
        "transform",
        `translate(${p.x.toFixed(3)} ${p.y.toFixed(3)})`,
      );
      node.setAttribute("x", (-p.w / 2).toFixed(3));
      node.setAttribute("y", (-p.h / 2).toFixed(3));
      node.setAttribute("width", p.w.toFixed(3));
      node.setAttribute("height", p.h.toFixed(3));
      node.setAttribute("rx", (p.w / 2).toFixed(3));
      if (p.fill !== prevFillRef.current[i]) {
        prevFillRef.current[i] = p.fill;
        node.setAttribute("fill", p.fill === "recording" ? RECORDING_FILL : WHITE_FILL);
      }
      if (Math.abs(p.opacity - prevOpacityRef.current[i]) > 0.005) {
        prevOpacityRef.current[i] = p.opacity;
        node.setAttribute("opacity", p.opacity.toFixed(3));
      }
    };

    const tick = (now: number) => {
      const tw = tweenRef.current;
      const status = statusRef.current;
      const baseT =
        tw.duration === 0
          ? 1
          : Math.min(1, Math.max(0, (now - tw.start) / tw.duration));
      const tweening = baseT < 1 && tw.from != null;

      const targetFor = (i: number): Particle => {
        if (status === "idle") return idleTarget(i);
        if (status === "recording") {
          const lv = stateRef.current.levels[i] ?? 0;
          // Recording bars come up late so amplitude jitter doesn't fight
          // the pose tween's interpolation.
          const gate = tw.from ? Math.max(0, (baseT - 0.55) / 0.45) : 1;
          return recordingTarget(i, lv, gate);
        }
        return transcribingTarget(i, now);
      };

      for (let i = 0; i < PARTICLE_COUNT; i++) {
        const target = targetFor(i);
        let p: Particle;

        if (tweening && tw.from) {
          const f = tw.from[i];
          const stagger = (i / (PARTICLE_COUNT - 1)) * 0.18;
          const tStaggered = Math.min(
            1,
            Math.max(0, (baseT - stagger) / (1 - stagger)),
          );
          const toSphere = tw.status === "transcribing";
          const fromSphere = tw.fromStatus === "transcribing";

          if (toSphere || fromSphere) {
            const cx = PILL_INNER_W / 2;
            const cy = PILL_INNER_H / 2;
            type Phase = "inflate" | "implode" | "hold" | "explode";
            let phase: Phase;
            let k: number;
            if (fromSphere) {
              if (tStaggered < 0.15) {
                phase = "inflate";
                k = tStaggered / 0.15;
              } else if (tStaggered < 0.45) {
                phase = "implode";
                k = (tStaggered - 0.15) / 0.30;
              } else if (tStaggered < 0.55) {
                phase = "hold";
                k = 1;
              } else {
                phase = "explode";
                k = (tStaggered - 0.55) / 0.45;
              }
            } else {
              if (tStaggered < 0.40) {
                phase = "implode";
                k = tStaggered / 0.40;
              } else if (tStaggered < 0.50) {
                phase = "hold";
                k = 1;
              } else {
                phase = "explode";
                k = (tStaggered - 0.50) / 0.50;
              }
            }

            const inflated: Particle = {
              x: cx + (f.x - cx) * 1.12,
              y: cy + (f.y - cy) * 1.12,
              w: f.w * 1.12,
              h: f.h * 1.12,
              fill: f.fill,
              opacity: f.opacity,
            };
            const pinch: Particle = {
              x: cx,
              y: cy,
              w: 0.4,
              h: 0.4,
              fill: f.fill,
              opacity: Math.min(f.opacity, target.opacity) * 0.85,
            };

            if (phase === "hold") {
              p = { ...pinch };
            } else {
              let a: Particle;
              let b: Particle;
              let kPos: number;
              let kSize: number;
              let kOpa: number;
              if (phase === "inflate") {
                a = f;
                b = inflated;
                kPos = easeOutQuint(k);
                kSize = easeOutQuint(k);
                kOpa = k;
              } else if (phase === "implode") {
                a = fromSphere ? inflated : f;
                b = pinch;
                kPos = k * k;
                kSize = k * k;
                kOpa = k;
              } else {
                // explode
                a = pinch;
                b = target;
                kPos = easeOutBack(k);
                kSize = easeOutBack(k);
                kOpa = easeInOutQuart(k);
              }
              p = {
                x: a.x + (b.x - a.x) * kPos,
                y: a.y + (b.y - a.y) * kPos,
                w: a.w + (b.w - a.w) * kSize,
                h: a.h + (b.h - a.h) * kSize,
                fill: kOpa > 0.55 ? b.fill : a.fill,
                opacity: a.opacity + (b.opacity - a.opacity) * kOpa,
              };
            }
          } else {
            const kPos = easeOutQuint(tStaggered);
            const kSize = easeOutBack(tStaggered);
            const kOpa = easeInOutQuart(tStaggered);
            p = {
              x: f.x + (target.x - f.x) * kPos,
              y: f.y + (target.y - f.y) * kPos,
              w: f.w + (target.w - f.w) * kSize,
              h: f.h + (target.h - f.h) * kSize,
              fill: kOpa > 0.55 ? target.fill : f.fill,
              opacity: f.opacity + (target.opacity - f.opacity) * kOpa,
            };
          }
        } else {
          p = { ...target };
        }

        particlesRef.current[i] = p;
        writeParticle(i, p);
      }

      // Outer width: for sphere transitions, hold at fromWidth through
      // implode (0–0.45), then ease to target across hold + explode
      // (0.45–1) so dots never clip outside in either direction.
      const targetWidth = PILL_OUTER_W[status];
      let nextWidth = widthRef.current;
      if (tweening && tw.from) {
        const sphere = tw.status === "transcribing" || tw.fromStatus === "transcribing";
        if (sphere) {
          if (baseT < 0.45) {
            nextWidth = tw.fromWidth;
          } else {
            const k = (baseT - 0.45) / 0.55;
            nextWidth = tw.fromWidth + (targetWidth - tw.fromWidth) * easeOutQuint(k);
          }
        } else {
          nextWidth = tw.fromWidth + (targetWidth - tw.fromWidth) * easeOutQuint(baseT);
        }
      } else {
        nextWidth = targetWidth;
      }
      if (nextWidth !== widthRef.current) {
        widthRef.current = nextWidth;
        if (capsuleRef.current) {
          capsuleRef.current.style.width = `${nextWidth}px`;
        }
      }

      // Spring-driven scale tween (1× ↔ 2×). Runs on its own clock — does not
      // share tweenRef.duration; the spring settles on physics, not a timer.
      // Asymmetric: SPRING_GROW has subtle overshoot for the "alive" feel,
      // SPRING_SHRINK is critically damped for a decisive return-to-rest.
      // Velocity is preserved across direction reversal (cancel mid-grow ⇒
      // shrink inherits current v ⇒ no jolt).
      const targetScale = PILL_SCALE[status];
      const s = scaleStateRef.current;
      const dt = Math.min(1 / 30, Math.max(0, (now - prevTickRef.current) / 1000));
      prevTickRef.current = now;
      if (s.x !== targetScale || s.v !== 0) {
        const cfg = targetScale > s.x ? SPRING_GROW : SPRING_SHRINK;
        const accel = (targetScale - s.x) * cfg.stiffness - s.v * cfg.damping;
        s.v += accel * dt;
        s.x += s.v * dt;
        if (Math.abs(targetScale - s.x) < 5e-4 && Math.abs(s.v) < 5e-3) {
          s.x = targetScale;
          s.v = 0;
        }
      }
      if (s.x !== prevScaleWriteRef.current) {
        prevScaleWriteRef.current = s.x;
        if (capsuleRef.current) {
          capsuleRef.current.style.transform = `scale(${s.x.toFixed(4)})`;
        }
      }

      raf = requestAnimationFrame(tick);
    };

    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, []);

  const handleClick = () => {
    // Click-through is toggled at the OS level so this only fires while idle;
    // matches the shipped macOS PillOverlay behavior of raising the main
    // window on tap.
    invoke("show_main_window").catch(
      // eslint-disable-next-line no-console
      (e) => console.warn("show_main_window failed", e),
    );
  };

  return (
    <div className="pill-root">
      <div
        ref={capsuleRef}
        className="pill-capsule"
        style={{ width: PILL_OUTER_W.idle }}
        onClick={handleClick}
      >
        <svg
          width={PILL_INNER_W}
          height={PILL_INNER_H}
          viewBox={`0 0 ${PILL_INNER_W} ${PILL_INNER_H}`}
          className="pill-svg"
          aria-hidden
        >
          {Array.from({ length: PARTICLE_COUNT }, (_, i) => (
            <rect
              key={i}
              fill={WHITE_FILL}
              ref={(el) => {
                rectRefs.current[i] = el;
              }}
            />
          ))}
        </svg>
      </div>
    </div>
  );
}
