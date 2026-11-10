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
        // Modes (TASK-96), mirroring tests/fixtures/tauri-shim.ts: an
        // in-memory modes block that behaves like core. Ids are slugs
        // made from the name, the built-in Dictation mode is listed
        // first, deleting a mode drops the app defaults that used it
        // and moves the selection back to Dictation. Seed the block
        // with `__owModesSettings`. The apps offered for a default are
        // the apps of `__owHistoryEntries` plus the apps that already
        // have one. Every change is recorded in `__owModesCalls` and
        // emits `modes_changed`, as Rust does.
        if (cmd.startsWith("modes_")) {
          const w = window;
          const BUILTIN = "dictation";
          // `mode_slug` in core.
          const slug = (text) =>
            text
              .trim()
              .toLowerCase()
              .replace(/[^\p{L}\p{N}]+/gu, "-")
              .replace(/^-+|-+$/g, "");
          // `ModesSettings::cleaned` in core.
          const cleaned = (raw) => {
            const custom = [];
            const exists = (id) => id === BUILTIN || custom.some((m) => m.id === id);
            for (const mode of raw.custom) {
              const name = mode.name.trim().slice(0, 40).trimEnd();
              if (!name) continue;
              const base = slug(mode.id) || slug(name) || "mode";
              let id = base;
              for (let n = 2; exists(id); n++) id = `${base}-${n}`;
              custom.push({
                id,
                name,
                instructions: mode.instructions.trim(),
                model: mode.model?.trim() || null,
                effort: mode.effort ?? null,
              });
            }
            const active = exists(slug(raw.active)) ? slug(raw.active) : BUILTIN;
            const app_defaults = [];
            for (const d of raw.app_defaults) {
              const entry = { app_id: d.app_id.trim(), app_name: d.app_name.trim(), mode: slug(d.mode) };
              if (!entry.app_id || !exists(entry.mode)) continue;
              const at = app_defaults.findIndex((x) => x.app_id === entry.app_id);
              if (at >= 0) app_defaults[at] = entry;
              else app_defaults.push(entry);
            }
            return { active, custom, app_defaults };
          };
          if (!w.__owModesStored) {
            w.__owModesStored = cleaned({
              active: BUILTIN,
              custom: [],
              app_defaults: [],
              ...(w.__owModesSettings ?? {}),
            });
          }
          const stored = w.__owModesStored;
          // `modes::list` in core.
          const list = (settings) => [
            {
              id: BUILTIN,
              name: "Dictation",
              instructions:
                w.__owRefineStored?.instructions ?? w.__owRefineSettings?.instructions ?? "",
              model: null,
              effort: null,
              builtin: true,
            },
            ...settings.custom.map((m) => ({ ...m, builtin: false })),
          ];
          // A copy, as over real IPC.
          const now = (settings) => structuredClone({ settings, modes: list(settings) });
          if (cmd === "modes_get") {
            const apps = [];
            for (const e of w.__owHistoryEntries ?? []) {
              if (e.app_id && !apps.some((a) => a.id === e.app_id)) {
                apps.push({ id: e.app_id, name: e.app_name ?? e.app_id });
              }
            }
            for (const d of stored.app_defaults) {
              if (!apps.some((a) => a.id === d.app_id)) {
                apps.push({ id: d.app_id, name: d.app_name || d.app_id });
              }
            }
            return { ...now(stored), apps };
          }
          const a = args ?? {};
          const unknown = (id) => `There is no mode with the id "${String(id)}".`;
          const has = (id) => id === BUILTIN || stored.custom.some((m) => m.id === id);
          let next = structuredClone(stored);
          // The user switching mode, as opposed to editing the modes.
          let switched = false;
          if (cmd === "modes_save") {
            next = a.settings;
          } else if (cmd === "modes_select") {
            if (!has(a.id)) throw unknown(a.id);
            next.active = a.id;
            switched = true;
          } else if (cmd === "modes_cycle") {
            w.__owModesCycleCount = (w.__owModesCycleCount ?? 0) + 1;
            const ids = [BUILTIN, ...next.custom.map((m) => m.id)];
            next.active = ids[(ids.indexOf(next.active) + 1) % ids.length];
            switched = true;
          } else if (cmd === "modes_add") {
            if (!String(a.name ?? "").trim()) throw "A mode needs a name.";
            next.custom.push({
              id: "",
              name: a.name,
              instructions: a.instructions ?? "",
              model: a.model ?? null,
              effort: a.effort ?? null,
            });
          } else if (cmd === "modes_edit") {
            if (a.id === BUILTIN) {
              throw "The Dictation mode is built in. It cannot be renamed, and its instructions are the Refine instructions.";
            }
            const mode = next.custom.find((m) => m.id === a.id);
            if (!mode) throw unknown(a.id);
            if (!String(a.name ?? "").trim()) throw "A mode needs a name.";
            mode.name = a.name;
            mode.instructions = a.instructions ?? "";
            mode.model = a.model ?? null;
            mode.effort = a.effort ?? null;
          } else if (cmd === "modes_delete") {
            if (a.id === BUILTIN) throw "The Dictation mode is built in and cannot be deleted.";
            if (!has(a.id)) throw unknown(a.id);
            // `cleaned` drops its app defaults and the selection.
            next.custom = next.custom.filter((m) => m.id !== a.id);
          } else if (cmd === "modes_set_app_default") {
            if (!String(a.appId ?? "").trim()) throw "An app default needs the id of the app.";
            if (!has(a.modeId)) throw unknown(a.modeId);
            next.app_defaults.push({
              app_id: a.appId,
              app_name: a.appName ?? "",
              mode: a.modeId,
            });
          } else if (cmd === "modes_clear_app_default") {
            if (!next.app_defaults.some((d) => d.app_id === a.appId)) {
              throw `The app "${String(a.appId)}" has no default mode.`;
            }
            next.app_defaults = next.app_defaults.filter((d) => d.app_id !== a.appId);
          } else {
            return null;
          }
          const saved = cleaned(next);
          w.__owModesStored = saved;
          w.__owModesCalls = [...(w.__owModesCalls ?? []), { cmd, args: a }];
          const result = now(saved);
          const selected = result.modes.find((m) => m.id === saved.active);
          window.__owEmit("modes_changed", {
            ...structuredClone(result),
            switched_to: switched ? (selected?.name ?? "Dictation") : null,
          });
          return result;
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

    // How many listeners an event has, without emitting anything.
    window.__owListeners = (event) => handlers.get(event)?.size ?? 0;
  });
}
