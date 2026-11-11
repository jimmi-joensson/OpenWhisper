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
        // Brings the main window forward on Settings, mirroring
        // tests/fixtures/tauri-shim.ts: with a `pane` (and a `focus`
        // row on it) the object form of `ow_navigate`, without one
        // the plain string.
        if (cmd === "open_settings_window") {
          window.__owOpenSettingsCount = (window.__owOpenSettingsCount ?? 0) + 1;
          window.__owOpenSettingsCalls = [...(window.__owOpenSettingsCalls ?? []), args ?? {}];
          const named = (value) =>
            typeof value === "string" && value.trim() ? value.trim() : null;
          const toPane = named((args ?? {}).pane);
          const toRow = named((args ?? {}).focus);
          window.__owEmit(
            "ow_navigate",
            toPane === null
              ? "settings"
              : toRow === null
                ? { view: "settings", pane: toPane }
                : { view: "settings", pane: toPane, focus: toRow },
          );
          return null;
        }
        // Hotkey slots, mirroring tests/fixtures/tauri-shim.ts: the
        // optional `cycle_mode` slot is absent until set; its command
        // target is "cycle-mode". The optional `ask` slot has a
        // default, which a seed without the key gets too; cleared, it
        // is null.
        if (
          cmd === "settings_get_hotkeys" ||
          cmd === "settings_set_hotkey" ||
          cmd === "settings_reset_hotkey" ||
          cmd === "settings_clear_hotkey"
        ) {
          const askDefault = { kind: "modifier-tap", code: "RightOption", mods: [] };
          window.__owHotkeys = window.__owHotkeys ?? {
            toggle: { kind: "modifier-tap", code: "RightCommand", mods: [] },
            cancel: { kind: "chord", code: "Escape", mods: [] },
          };
          if (!("ask" in window.__owHotkeys)) window.__owHotkeys.ask = askDefault;
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
          if (cmd === "settings_clear_hotkey") {
            // Only the optional slots can be without a key.
            if (target === "toggle" || target === "cancel") {
              throw `The ${target} shortcut cannot be removed, only changed.`;
            }
            window.__owHotkeyClearCount = (window.__owHotkeyClearCount ?? 0) + 1;
            if (target === "ask") window.__owHotkeys.ask = null;
            else delete window.__owHotkeys[key];
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
              : target === "ask"
                ? askDefault
                : { kind: "chord", code: "Escape", mods: [] };
          window.__owHotkeys[key] = def;
          return def;
        }
        // Ask (TASK-97), mirroring tests/fixtures/tauri-shim.ts.
        // `ask_context_preview` answers with the facts of a made-up
        // computer for the groups the stored block switches on.
        // `__owAskSettings` seeds the stored block; the one ask lives
        // on `__owAskSnapshot` (set it, then
        // `__owEmit('ask_tick', window.__owAskSnapshot)`), and cancel,
        // dismiss, insert and copy answer from it the way core does.
        if (cmd.startsWith("ask_")) {
          const w = window;
          w.__owAskCalls = [...(w.__owAskCalls ?? []), { cmd, args: args ?? {} }];
          if (!w.__owAskStored) {
            w.__owAskStored = {
              enabled: false,
              web_search: false,
              instructions: "",
              model: null,
              effort: null,
              timeout_ms: 60000,
              ...(w.__owAskSettings ?? {}),
              // `ContextSettings::default()` in core: every group
              // on, no place. A seed may name only some of it.
              context: {
                date_time: true,
                computer: true,
                front_app: true,
                locale: true,
                place: "",
                ...(w.__owAskSettings?.context ?? {}),
              },
            };
          }
          const stored = w.__owAskStored;
          // `AskSnapshot::default()` in core.
          const idle = (version) => ({
            id: 0,
            phase: "idle",
            question: "",
            answer: "",
            notice: "",
            suggestion: null,
            error: "",
            backend: "",
            model: "",
            effort: "",
            web_search: false,
            searching: false,
            search_query: "",
            started_at_ms: 0,
            elapsed_ms: 0,
            first_text_ms: null,
            usage: null,
            version,
          });
          const snap = w.__owAskSnapshot ?? idle(0);
          const busy = ["listening", "thinking", "answering"].includes(snap.phase);
          const change = (next) => {
            w.__owAskSnapshot = next;
            window.__owEmit("ask_tick", structuredClone(next));
          };
          // What `ask_status` answers: the state of the backend,
          // which only the Ask pane asks for (in the app it starts
          // the Claude CLI). `__owAskBackend` overrides it.
          const backendStatus = {
            backend: "claude-cli",
            state: "ready",
            ready: true,
            message: "Claude Code is signed in with your Claude subscription (max).",
            destination:
              "Anthropic (Claude), through the Claude Code app signed in on this computer",
            path: "/Users/test/.local/bin/claude",
            version: "2.1.322 (Claude Code)",
            auth_method: "claude.ai",
            plan: "max",
            has_api_key: false,
            ...(w.__owAskBackend ?? {}),
          };
          if (cmd === "ask_status") return structuredClone(backendStatus);
          // A cancel, close or insert that names an ask only acts on
          // that ask (`cancel_if` and `dismiss_if` in core).
          const meant = (args ?? {});
          const other = meant.id != null && meant.id !== snap.id;
          if (cmd === "ask_get") {
            // `AskPlan::with_terms` in core: the OpenAI-compatible
            // backend (set with `__owAskBackend`) answers with
            // Refine's model and has no web search.
            const openai = backendStatus.backend === "openai-compatible";
            const webSearch = stored.web_search && !openai;
            // No backend state in it: that is `ask_status`.
            return structuredClone({
              settings: stored,
              backend_kind: backendStatus.backend,
              destination: backendStatus.destination,
              model: openai ? "small-model" : (stored.model ?? "sonnet"),
              effort: openai ? "" : (stored.effort ?? "low"),
              web_search: webSearch,
              timeout_ms: webSearch ? stored.timeout_ms * 2 : stored.timeout_ms,
              notice:
                stored.web_search && openai
                  ? "Web search is only available with the Claude backend. This answer comes from the model alone."
                  : "",
              limits: {
                timeout_min_ms: 5000,
                timeout_max_ms: 300000,
                timeout_default_ms: 60000,
                instructions_max_chars: 4000,
                place_max_chars: 120,
              },
              default_model: "sonnet",
            });
          }
          if (cmd === "ask_set") {
            const { settings } = args ?? {};
            // `AskSettings::cleaned` in core.
            const saved = {
              ...settings,
              instructions: settings.instructions.trim().slice(0, 4000).trimEnd(),
              model: settings.model?.trim() || null,
              effort: settings.effort === "minimal" ? "low" : settings.effort,
              timeout_ms: Math.min(Math.max(settings.timeout_ms, 5000), 300000),
              // `ContextSettings::cleaned` in core: the place on one
              // line, at most 120 characters.
              context: {
                ...settings.context,
                place: settings.context.place
                  .split(/\s+/)
                  .filter(Boolean)
                  .join(" ")
                  .slice(0, 120)
                  .trimEnd(),
              },
            };
            w.__owAskStored = saved;
            w.__owAskSaves = [...(w.__owAskSaves ?? []), saved];
            window.__owEmit("ask_changed", structuredClone(saved));
            return structuredClone(saved);
          }
          if (cmd === "ask_context_preview") {
            // `ask::context_preview` in core: the facts the stored
            // block switches on, as labelled lines. The computer is
            // made up. No app in front: the window asking is
            // OpenWhisper's own, which an ask never names.
            if (w.__owAskContextFail) throw "ask context: not available";
            const c = stored.context;
            const lines = [];
            if (c.place) lines.push(`Where the person is: ${c.place}`);
            if (c.date_time) {
              lines.push(
                "Date: Wednesday 11 November 2026",
                "Local time: 14:32",
                "Time zone: Europe/Copenhagen (UTC+01:00)",
              );
            }
            if (c.computer) {
              lines.push(
                "Operating system: macOS 26.0",
                "Computer: Northwind Book Pro 14, NB14,2",
                "Chip: Northwind N3 Pro",
                "Memory: 32 GB",
                "Power: on battery, 82% charged",
                "Displays: 1 (1512 x 982)",
                "Free disk space: about 210 GB",
                "Internet: online",
              );
            }
            if (c.locale) {
              lines.push(
                "Language: English (en)",
                "Region: Denmark (DK)",
                "Units: metric",
                "Clock: 24-hour",
                "Currency: DKK",
                "Keyboard layout: Danish",
              );
            }
            return lines.join("\n");
          }
          if (cmd === "ask_snapshot") return structuredClone(snap);
          if (cmd === "ask_start") {
            if (!stored.enabled) {
              throw "Ask is switched off. Switch it on in the settings to ask questions.";
            }
            const next = {
              ...idle(snap.version + 1),
              id: snap.id + 1,
              phase: "thinking",
              question: ((args ?? {}).text ?? "").trim(),
            };
            change(next);
            return structuredClone(next);
          }
          if (cmd === "ask_toggle") return null;
          if (cmd === "ask_cancel") {
            if (!busy || other) return false;
            change(
              snap.phase === "listening"
                ? idle(snap.version + 1)
                : { ...snap, phase: "cancelled", searching: false, version: snap.version + 1 },
            );
            return true;
          }
          if (cmd === "ask_dismiss") {
            if (!other) change(idle(snap.version + 1));
            return null;
          }
          if (cmd === "ask_insert") {
            if (snap.phase !== "done" || other) return false;
            w.__owAskInserted = [...(w.__owAskInserted ?? []), snap.answer];
            change(idle(snap.version + 1));
            return true;
          }
          if (cmd === "ask_copy") {
            if (!snap.answer.trim()) return false;
            w.__owAskCopied = [...(w.__owAskCopied ?? []), snap.answer];
            return true;
          }
          return null;
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
        // made from the name, names are unique ignoring case, app ids
        // are kept in lower case, the built-in Dictation mode is
        // listed first, deleting a mode drops the app defaults that
        // used it and moves the selection back to Dictation, and a
        // mode's own model and effort belong to the Refine backend
        // they were picked for. Seed the block with
        // `__owModesSettings`. The apps offered for a default are the
        // apps that have one, then the apps of `__owHistoryEntries`,
        // then the open apps in `__owRunningApps`. Every change is
        // recorded in `__owModesCalls` and emits `modes_changed`, as
        // Rust does.
        if (cmd.startsWith("modes_")) {
          const w = window;
          const BUILTIN = "dictation";
          const BUILTIN_NAME = "Dictation";
          // `mode_slug` in core: lower case, letters and digits kept,
          // every run of anything else one hyphen.
          const slug = (text) =>
            text
              .trim()
              .toLowerCase()
              .replace(/[^\p{L}\p{N}]+/gu, "-")
              .replace(/^-+|-+$/g, "");
          // `app_key` and `mode_name_key` in core: how app ids and mode
          // names are compared (and app ids stored).
          const key = (text) => text.trim().toLowerCase();
          // `Mode::override_backend` in core: the backend a mode's own
          // model and effort are for. A block from before modes stored
          // one means the Claude CLI.
          const overrideBackend = (m) =>
            m.model || m.effort ? (m.backend ?? "claude-cli") : null;
          // The backend Refine uses: the one a model picked in the
          // dialog is for.
          const activeBackend = () =>
            w.__owRefineStored?.backend ?? w.__owRefineSettings?.backend ?? "claude-cli";
          // `ModesSettings::cleaned` in core.
          const cleaned = (raw) => {
            const custom = [];
            const exists = (id) => id === BUILTIN || custom.some((m) => m.id === id);
            const nameTaken = (name) =>
              key(name) === key(BUILTIN_NAME) || custom.some((m) => key(m.name) === key(name));
            for (const mode of raw.custom) {
              const given = mode.name.trim().slice(0, 40).trimEnd();
              if (!given) continue;
              const base = slug(mode.id) || slug(given) || "mode";
              let id = base;
              for (let n = 2; exists(id); n++) id = `${base}-${n}`;
              // A name edited into a duplicate gets a number.
              let name = given;
              for (let n = 2; nameTaken(name); n++) {
                const suffix = ` ${n}`;
                name = `${given.slice(0, 40 - suffix.length).trimEnd()}${suffix}`;
              }
              const model = mode.model?.trim() || null;
              const effort = mode.effort ?? null;
              custom.push({
                id,
                name,
                instructions: mode.instructions.trim(),
                model,
                effort,
                backend: model || effort ? (mode.backend ?? null) : null,
              });
            }
            const active = exists(slug(raw.active)) ? slug(raw.active) : BUILTIN;
            const app_defaults = [];
            for (const d of raw.app_defaults) {
              const entry = { app_id: key(d.app_id), app_name: d.app_name.trim(), mode: slug(d.mode) };
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
          // `modes::list` in core (the built-in mode, put together from
          // the Refine instructions, then the user's own), each with
          // what its own model and effort mean for the backend in use,
          // as `ModeRow` in src-tauri/src/modes.rs adds.
          const list = (settings) =>
            [
              {
                id: BUILTIN,
                name: BUILTIN_NAME,
                instructions:
                  w.__owRefineStored?.instructions ?? w.__owRefineSettings?.instructions ?? "",
                model: null,
                effort: null,
                backend: null,
                builtin: true,
              },
              ...settings.custom.map((m) => ({ ...m, builtin: false })),
            ].map((m) => ({
              ...m,
              override_backend: overrideBackend(m),
              overrides_apply: overrideBackend(m) === activeBackend(),
            }));
          // A copy, as over real IPC.
          const now = (settings) =>
            structuredClone({ settings, modes: list(settings) });
          if (cmd === "modes_get") {
            // `modes::apps_for_defaults` in core: the apps with a
            // default, then the apps in the history, then the apps
            // that are open, each once whatever the case of its id.
            const apps = [];
            const offer = (id, name) => {
              const appId = key(id ?? "");
              const appName = (name ?? "").trim();
              if (!appId) return;
              const known = apps.find((x) => x.id === appId);
              if (!known) apps.push({ id: appId, name: appName || appId });
              else if (known.name === known.id && appName) known.name = appName;
            };
            for (const d of stored.app_defaults) offer(d.app_id, d.app_name);
            for (const e of w.__owHistoryEntries ?? []) offer(e.app_id, e.app_name);
            for (const r of w.__owRunningApps ?? []) offer(r.id, r.name);
            return { ...now(stored), apps };
          }
          const a = (args ?? {});
          const unknown = (id) => `There is no mode with the id "${String(id)}".`;
          const has = (id) =>
            id === BUILTIN || stored.custom.some((m) => m.id === id);
          // `check_name_free` in core: a name another mode has, ignoring
          // case and outer space, is refused.
          const nameHolder = (name, except) =>
            key(name) === key(BUILTIN_NAME)
              ? BUILTIN_NAME
              : stored.custom.find((m) => m.id !== except && key(m.name) === key(name))?.name;
          const taken = (holder) =>
            `There is already a mode called "${holder}". Give this one another name.`;
          let next = structuredClone(stored);
          // The user switching mode, as opposed to editing the modes.
          let switched = false;
          if (cmd === "modes_select") {
            if (!has(a.id)) throw unknown(a.id);
            next.active = a.id;
            switched = true;
          } else if (cmd === "modes_cycle") {
            w.__owModesCycleCount = (w.__owModesCycleCount ?? 0) + 1;
            const ids = [BUILTIN, ...next.custom.map((m) => m.id)];
            next.active = ids[(ids.indexOf(next.active) + 1) % ids.length];
            switched = true;
          } else if (cmd === "modes_add") {
            const name = String(a.name ?? "");
            if (!name.trim()) throw "A mode needs a name.";
            const holder = nameHolder(name, null);
            if (holder) throw taken(holder);
            next.custom.push({
              id: "",
              name,
              instructions: (a.instructions) ?? "",
              model: (a.model) ?? null,
              effort: (a.effort) ?? null,
              backend: activeBackend(),
            });
          } else if (cmd === "modes_edit") {
            if (a.id === BUILTIN) {
              throw "The Dictation mode is built in. It cannot be renamed, and its instructions are the Refine instructions.";
            }
            const mode = next.custom.find((m) => m.id === a.id);
            if (!mode) throw unknown(a.id);
            const name = String(a.name ?? "");
            if (!name.trim()) throw "A mode needs a name.";
            const holder = nameHolder(name, mode.id);
            if (holder) throw taken(holder);
            mode.name = name;
            mode.instructions = (a.instructions) ?? "";
            // `ModeEdit::backend` in core: a model and effort kept for
            // the other backend are left alone by an edit that names
            // neither, and replaced together by one that names either.
            const model = (a.model) ?? null;
            const effort = (a.effort) ?? null;
            const storedFor = overrideBackend(mode);
            if (storedFor === null || storedFor === activeBackend() || model || effort) {
              mode.model = model;
              mode.effort = effort;
              if (model || effort) mode.backend = activeBackend();
            }
          } else if (cmd === "modes_delete") {
            if (a.id === BUILTIN) throw "The Dictation mode is built in and cannot be deleted.";
            if (!has(a.id)) throw unknown(a.id);
            // `cleaned` drops its app defaults and the selection.
            next.custom = next.custom.filter((m) => m.id !== a.id);
          } else if (cmd === "modes_set_app_default") {
            const appId = key(String(a.appId ?? ""));
            if (!appId) throw "An app default needs the id of the app.";
            if (!has(a.modeId)) throw unknown(a.modeId);
            const known = next.app_defaults.find((d) => d.app_id === appId)?.app_name ?? "";
            next.app_defaults.push({
              app_id: appId,
              app_name: String(a.appName ?? "").trim() || known,
              mode: a.modeId,
            });
          } else if (cmd === "modes_clear_app_default") {
            const appId = key(String(a.appId ?? ""));
            if (!next.app_defaults.some((d) => d.app_id === appId)) {
              throw `The app "${String(a.appId).trim()}" has no default mode.`;
            }
            next.app_defaults = next.app_defaults.filter((d) => d.app_id !== appId);
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
