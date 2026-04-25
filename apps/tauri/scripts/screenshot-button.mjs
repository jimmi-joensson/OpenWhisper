// Zoom-in screenshot of the Start dictation button mic icon, plus the
// rendered DOM so we can inspect what MicGlyph actually emits.
import { chromium } from "@playwright/test";

const URL = process.env.OW_URL ?? "http://localhost:1420";

const browser = await chromium.launch();
const context = await browser.newContext({
  viewport: { width: 1280, height: 900 },
  deviceScaleFactor: 3,
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
      if (cmd === "plugin:event|listen") {
        const { event, handler } = args ?? {};
        if (!handlers.has(event)) handlers.set(event, new Set());
        handlers.get(event).add(handler);
        return handler;
      }
      return null;
    },
    transformCallback: (fn) => {
      const id = nextId++;
      callbacks.set(id, fn);
      return id;
    },
    unregisterCallback: (id) => callbacks.delete(id),
    callbacks,
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: (event, eventId) => {
      handlers.get(event)?.delete(eventId);
      callbacks.delete(eventId);
    },
  };
});

const page = await context.newPage();
await page.goto(URL, { waitUntil: "networkidle" });
await page.waitForSelector("text=Start dictation", { timeout: 5000 });

const btn = page.getByRole("button", { name: /Start dictation/i });
await btn.screenshot({ path: "/tmp/btn-start.png" });

const html = await btn.evaluate((el) => el.querySelector("svg")?.outerHTML ?? el.innerHTML);
console.log("---button SVG---");
console.log(html);

await browser.close();
