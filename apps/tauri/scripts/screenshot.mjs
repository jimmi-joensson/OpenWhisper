// Visual smoke-test for the Tauri main window via Playwright + Chromium.
//
// Stubs `window.__TAURI_INTERNALS__` so the bundle runs in a plain browser
// without crashing on `invoke`/`getCurrentWindow`/`emitTo`/`listen`. Drives
// the dictation hook by synthesizing `dictation_tick` events through a small
// test helper (`window.__owEmit`).
//
// Usage:
//   pnpm dev                       # leave running in another shell
//   node scripts/screenshot.mjs    # writes /tmp/openwhisper-main-*.png
//
// Override defaults with env vars:
//   OW_URL=http://localhost:1420  OW_OUT_DIR=/tmp  OW_VIEWPORT=1280x900

import { chromium } from "@playwright/test";
import { mkdir } from "node:fs/promises";

const URL = process.env.OW_URL ?? "http://localhost:1420";
const OUT_DIR = process.env.OW_OUT_DIR ?? "/tmp";
const [vw, vh] = (process.env.OW_VIEWPORT ?? "1280x900").split("x").map(Number);

await mkdir(OUT_DIR, { recursive: true });

const browser = await chromium.launch();
const context = await browser.newContext({
  viewport: { width: vw, height: vh },
  colorScheme: "dark",
});

await context.addInitScript(() => {
  // Tauri 2 internals shim. `plugin:event|listen` registers a handler and
  // returns the callback id; `__owEmit` invokes registered handlers so tests
  // can simulate backend events.
  const handlers = new Map();
  const callbacks = new Map();
  let nextId = 1;

  window.__TAURI_INTERNALS__ = {
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { label: "main", windowLabel: "main" },
    },
    invoke: async (cmd, args) => {
      if (cmd === "core_version") return "0.1.0-mock";
      if (cmd === "dictation_toggle") return null;
      if (cmd === "dictation_cancel") return false;
      if (cmd === "plugin:event|listen") {
        const { event, handler } = args ?? {};
        if (!handlers.has(event)) handlers.set(event, new Set());
        handlers.get(event).add(handler);
        return handler;
      }
      if (cmd === "plugin:event|unlisten") {
        const { event, eventId } = args ?? {};
        handlers.get(event)?.delete(eventId);
        return null;
      }
      return null;
    },
    transformCallback: (fn) => {
      const id = nextId++;
      callbacks.set(id, fn);
      return id;
    },
    unregisterCallback: (id) => {
      callbacks.delete(id);
    },
    callbacks,
  };

  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: (event, eventId) => {
      handlers.get(event)?.delete(eventId);
      callbacks.delete(eventId);
    },
  };

  let tickCounter = 0;
  window.__owEmit = (event, payload) => {
    const set = handlers.get(event);
    if (!set) return 0;
    let delivered = 0;
    for (const id of set) {
      const cb = callbacks.get(id);
      if (cb) {
        cb({ event, id: ++tickCounter, payload });
        delivered++;
      }
    }
    return delivered;
  };
});

const page = await context.newPage();
page.on("pageerror", (err) => console.error("[pageerror]", err.message));
page.on("console", (msg) => {
  if (msg.type() === "error") console.error("[console]", msg.text());
});

await page.goto(URL, { waitUntil: "networkidle" });
await page.waitForSelector("text=OpenWhisper Dev", { timeout: 5000 });
// Let useEffect register the dictation_tick listener.
await page.waitForFunction(
  () => window.__owEmit("dictation_tick", { phase: 0, status: "idle", status_message: "", transcript: "", confidence: 0, sample_count: 0, elapsed_ms: 0, error_message: "", can_toggle: true, is_recording: false, level: 0 }) > 0,
  { timeout: 3000 },
);

const snap = async (name) => {
  await page.waitForTimeout(150);
  const out = `${OUT_DIR}/openwhisper-main-${name}.png`;
  await page.screenshot({ path: out, fullPage: true });
  console.log(`✓ ${name} → ${out}`);
};

const baseTick = {
  status_message: "",
  transcript: "",
  confidence: 0,
  error_message: "",
  can_toggle: true,
  is_recording: false,
};

// idle
await page.evaluate((tick) => window.__owEmit("dictation_tick", tick), {
  ...baseTick,
  phase: 0,
  status: "idle",
  sample_count: 0,
  elapsed_ms: 0,
  level: 0,
});
await snap("idle");

// recording — fan in 32 ticks of varying level so the meter has data
for (let i = 0; i < 32; i++) {
  const t = i / 32;
  const env =
    0.45 + 0.35 * Math.sin(t * 6.28 * 1.7) + 0.18 * Math.sin(t * 6.28 * 4.3);
  const level = Math.max(0.05, Math.min(1, env * Math.abs(env)));
  await page.evaluate(
    ([tick, lvl, idx]) =>
      window.__owEmit("dictation_tick", {
        ...tick,
        phase: 2,
        status: "recording",
        is_recording: true,
        sample_count: idx * 800,
        elapsed_ms: idx * 50,
        level: lvl,
      }),
    [baseTick, level, i + 1],
  );
}
await snap("recording");

// transcribing — fixed transcript + dim tail
await page.evaluate(
  ([tick]) =>
    window.__owEmit("dictation_tick", {
      ...tick,
      phase: 3,
      status: "transcribing",
      transcript: "the quick brown fox jumps over the lazy dog",
      confidence: 0.92,
      sample_count: 32 * 800,
      elapsed_ms: 32 * 50,
      level: 0,
    }),
  [baseTick],
);
await snap("transcribing");

await browser.close();
