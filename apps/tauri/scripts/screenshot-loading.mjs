// Visual smoke test for the LOADING_MODEL phase + download progress UI.
// Mirrors screenshot.mjs but drives phase=1 with a sweep of download states.
//
// Usage: pnpm dev (separate shell) → node scripts/screenshot-loading.mjs

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
  console.log(`[console:${msg.type()}]`, msg.text());
});

await page.goto(URL, { waitUntil: "networkidle" });
// "Dictation debug" is a section title that always renders once the SPA
// mounts; doesn't depend on Tauri runtime mocks for getName / app product.
await page.waitForSelector("text=Dictation debug", { timeout: 8000 });
await page.waitForFunction(
  () =>
    window.__owEmit("dictation_tick", {
      phase: 0,
      status: "idle",
      status_message: "",
      transcript: "",
      confidence: 0,
      sample_count: 0,
      elapsed_ms: 0,
      error_message: "",
      can_toggle: true,
      is_recording: false,
      level: 0,
      download_bytes_done: 0,
      download_bytes_total: 0,
    }) > 0,
  { timeout: 3000 },
);

const snap = async (name) => {
  await page.waitForTimeout(200);
  const out = `${OUT_DIR}/ow-loading-${name}.png`;
  await page.screenshot({ path: out, fullPage: true });
  console.log(`✓ ${name} → ${out}`);
};

const TOTAL = 487 * 1_048_576; // ~487 MB

// Helper to push a LOADING_MODEL tick with explicit progress.
const loadingTick = (done, total, statusMessage) => ({
  phase: 1,
  status: "idle",
  status_message: statusMessage,
  transcript: "",
  confidence: 0,
  sample_count: 0,
  elapsed_ms: 0,
  error_message: "",
  can_toggle: false,
  is_recording: false,
  level: 0,
  download_bytes_done: done,
  download_bytes_total: total,
});

// 1. Determinate: 0% (just kicked off)
await page.evaluate(
  (t) => window.__owEmit("dictation_tick", t),
  loadingTick(0, TOTAL, "downloading model… 0/487 MB (0%)"),
);
await snap("download-0pct");

// 2. Determinate: ~48% (mid-flight)
const half = Math.floor(TOTAL * 0.48);
await page.evaluate(
  (t) => window.__owEmit("dictation_tick", t),
  loadingTick(half, TOTAL, "downloading model… 234/487 MB (48%)"),
);
await snap("download-48pct");

// 3. Determinate: 100%
await page.evaluate(
  (t) => window.__owEmit("dictation_tick", t),
  loadingTick(TOTAL, TOTAL, "downloading model… 487/487 MB (100%)"),
);
await snap("download-100pct");

// 4. Indeterminate (Content-Length missing OR post-download phase).
await page.evaluate(
  (t) => window.__owEmit("dictation_tick", t),
  loadingTick(0, 0, "loading model into memory…"),
);
await snap("loading-session");

await browser.close();
console.log("done.");
