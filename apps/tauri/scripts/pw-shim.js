// Install the Tauri 2 internals stub so the OpenWhisper SPA boots in plain
// Chromium under playwright-cli (no Tauri shell). Mirrors the Playwright
// test fixture in tests/fixtures/tauri-shim.ts.
//
// playwright-cli run-code --filename loads the file as a single function
// expression (no module wrappers). Receives `page` as the first argument.
//
// Usage via the pnpm shortcut:
//   pnpm pw:open               # open + shim + reload, leaves session 'ow' alive
//   pnpm exec playwright-cli -s=ow snapshot
//   pnpm exec playwright-cli -s=ow eval "window.__owEmit('dictation_tick', { phase: 2, status: 'recording', is_recording: true, level: 0.5, status_message: '', transcript: '', confidence: 0, sample_count: 0, elapsed_ms: 0, error_message: '', can_toggle: true })"
//   pnpm pw:close
async (page) => {
  await page.context().addInitScript(() => {
    const handlers = new Map();
    const callbacks = new Map();
    let nextId = 1;
    let tickCounter = 0;

    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: "main" },
        currentWebview: { label: "main", windowLabel: "main" },
      },
      invoke: async (cmd, args) => {
        if (cmd === "core_version") return "0.1.0-pwcli";
        if (cmd === "dictation_toggle") return null;
        if (cmd === "dictation_cancel") return false;
        if (cmd === "hotkey_status_current") return null;
        if (cmd === "hotkey_retry") {
          window.__owHotkeyRetryCount = (window.__owHotkeyRetryCount ?? 0) + 1;
          return null;
        }
        // Hotkey slots, mirroring tests/fixtures/tauri-shim.ts: the
        // optional `cycle_mode` slot is absent until set; its command
        // target is "cycle-mode".
        if (
          cmd === "settings_get_hotkeys" ||
          cmd === "settings_set_hotkey" ||
          cmd === "settings_reset_hotkey"
        ) {
          window.__owHotkeys = window.__owHotkeys ?? {
            toggle: { kind: "modifier-tap", code: "RightCommand", mods: [] },
            cancel: { kind: "chord", code: "Escape", mods: [] },
          };
          // A copy, as over real IPC.
          if (cmd === "settings_get_hotkeys") return structuredClone(window.__owHotkeys);

          const { target, config } = args ?? {};
          const key = target === "cycle-mode" ? "cycle_mode" : target;
          window.__owHotkeyLastTarget = target;
          if (cmd === "settings_set_hotkey") {
            window.__owHotkeys[key] = config;
            window.__owHotkeySetCount = (window.__owHotkeySetCount ?? 0) + 1;
            window.__owHotkeyLastConfig = config;
            return null;
          }
          window.__owHotkeyResetCount = (window.__owHotkeyResetCount ?? 0) + 1;
          if (target === "cycle-mode") {
            delete window.__owHotkeys[key];
            return null;
          }
          const def =
            target === "toggle"
              ? { kind: "modifier-tap", code: "RightCommand", mods: [] }
              : { kind: "chord", code: "Escape", mods: [] };
          window.__owHotkeys[key] = def;
          return def;
        }
        if (cmd === "settings_capture_hotkey_start") {
          window.__owCaptureStartCount = (window.__owCaptureStartCount ?? 0) + 1;
          window.__owCaptureLastTarget = (args ?? {}).target;
          return null;
        }
        if (cmd === "settings_capture_hotkey_cancel") {
          window.__owCaptureCancelCount = (window.__owCaptureCancelCount ?? 0) + 1;
          return null;
        }
        if (cmd === "modes_cycle") {
          window.__owModesCycleCount = (window.__owModesCycleCount ?? 0) + 1;
          return null;
        }
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
      unregisterCallback: (id) => callbacks.delete(id),
      callbacks,
    };

    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
      unregisterListener: (event, eventId) => {
        handlers.get(event)?.delete(eventId);
        callbacks.delete(eventId);
      },
    };

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
}
