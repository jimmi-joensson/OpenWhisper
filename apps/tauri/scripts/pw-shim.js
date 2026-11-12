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
        // dismiss, insert, copy and the two answers to a confirm
        // (`ask_confirm_allow`, `ask_confirm_deny`, Agent, TASK-99)
        // answer from it the way core does.
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
            // Vision (TASK-98): what became of the screen.
            screen: "none",
            screen_notice: "",
            // Agent (TASK-99): the tool calls of the ask, the one
            // that waits for a yes or no, and where the ask looked.
            steps: [],
            confirm: null,
            context_source: { kind: "none" },
            version,
          });
          const snap = w.__owAskSnapshot ?? idle(0);
          // `AskPhase::is_busy` in core: waiting on a confirm counts.
          const busy = ["listening", "thinking", "answering", "confirming"].includes(snap.phase);
          // `settle_steps` in core: an ask that ends without an answer
          // waits on nothing, and no step stays running or waiting.
          const settled = (steps) =>
            (steps ?? []).map((step) =>
              step.state === "waiting"
                ? {
                    ...step,
                    state: "denied",
                    outcome: "The ask ended before you answered, so it was not done.",
                  }
                : step.state === "running"
                  ? {
                      ...step,
                      state: "failed",
                      outcome: "The ask ended while this was running, so its result is not known.",
                    }
                  : step,
            );
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
                : {
                    ...snap,
                    phase: "cancelled",
                    searching: false,
                    // A confirm that waited is a no from here on.
                    confirm: null,
                    steps: settled(snap.steps),
                    version: snap.version + 1,
                  },
            );
            return true;
          }
          if (cmd === "ask_confirm_allow" || cmd === "ask_confirm_deny") {
            // `confirm_allow` and `confirm_deny` in core: only the
            // confirm the ask is waiting on right now is answered. A
            // click that names another ask or another request, or
            // comes late, changes nothing.
            const { askId, requestId } = args ?? {};
            const waiting = snap.phase === "confirming" ? (snap.confirm ?? null) : null;
            if (askId !== snap.id || !waiting || waiting.request_id !== requestId) return false;
            const allowed = cmd === "ask_confirm_allow";
            change({
              ...snap,
              // Back to where the ask was (`after_confirm` in core).
              phase: snap.answer.trim() ? "answering" : "thinking",
              confirm: null,
              // In the app the backend says what became of the step
              // a moment later: it runs, or it was not done.
              steps: (snap.steps ?? []).map((step) =>
                step.id === waiting.step_id
                  ? {
                      ...step,
                      state: allowed ? "running" : "denied",
                      outcome: allowed ? "" : "You said no.",
                    }
                  : step,
              ),
              version: snap.version + 1,
            });
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
        // Vision (TASK-98), mirroring tests/fixtures/tauri-shim.ts:
        // the Screen section of the Ask pane. An
        // in-memory `vision` block that behaves like core: a list that
        // is null is the built-in one, and every save is tidied the
        // way `VisionSettings::cleaned` does it (one line, single
        // spaces, empty and double entries dropped ignoring case, app
        // ids in lower case). `__owVisionSettings` seeds the stored
        // block, `__owVisionPermission` the permission state
        // (granted unless set), `__owVisionPermissionAfterRequest`
        // what a request for it answers, `__owVisionPlatform` the
        // platform (macos unless set) and `__owRunningApps` the open
        // apps. `__owVisionSetError` makes a save fail. Every call is
        // recorded in `__owVisionCalls` and every save in
        // `__owVisionSaves`; a save emits `vision_changed`, as Rust
        // does. There is no command that takes a screenshot, here or
        // in Rust. The built-in lists here are short stand-ins with
        // made-up apps; the real ones are in
        // core/src/settings/vision.rs.
        if (cmd.startsWith("vision_")) {
          const w = window;
          w.__owVisionCalls = [...(w.__owVisionCalls ?? []), { cmd, args: args ?? {} }];
          const platform = w.__owVisionPlatform ?? "macos";
          const defaultPhrases = [
            "look at my screen",
            "on my screen",
            "this error",
            "se på min skærm",
            "på min skærm",
            "denne fejl",
          ];
          const defaultNeverCapture =
            platform === "windows"
              ? [
                  { id: "vault.exe", name: "Vault" },
                  { id: "keyring.exe", name: "Keyring" },
                ]
              : [
                  { id: "com.example.vault", name: "Vault" },
                  { id: "com.example.keyring", name: "Keyring" },
                ];
          // `tidy` in core: one line, single spaces, at most `max`
          // characters.
          const tidy = (s, max) =>
            Array.from(s.split(/\s+/).filter(Boolean).join(" ")).slice(0, max).join("").trimEnd();
          // `VisionSettings::cleaned` in core.
          const cleaned = (s) => {
            let phrases = null;
            if (Array.isArray(s.phrases)) {
              phrases = [];
              for (const raw of s.phrases) {
                const phrase = tidy(String(raw), 80);
                const key = phrase.toLowerCase();
                if (!phrase || phrases.some((p) => p.toLowerCase() === key)) continue;
                phrases.push(phrase);
                if (phrases.length === 100) break;
              }
            }
            let apps = null;
            if (Array.isArray(s.never_capture)) {
              apps = [];
              for (const raw of s.never_capture) {
                const id = tidy(String(raw.id ?? ""), 200).toLowerCase();
                if (!id || apps.some((a) => a.id === id)) continue;
                apps.push({ id, name: tidy(String(raw.name ?? ""), 200) });
                if (apps.length === 200) break;
              }
            }
            // A trigger core does not know reads as off.
            const trigger = ["off", "phrases", "smart", "always"].includes(s.trigger)
              ? s.trigger
              : "off";
            return { trigger, phrases, never_capture: apps };
          };
          if (!w.__owVisionStored) {
            w.__owVisionStored = cleaned({
              // The trigger of someone who never chose one: off on
              // Windows, where capture is not verified yet
              // (`default_trigger_on` in core).
              trigger: platform === "windows" ? "off" : "phrases",
              phrases: null,
              never_capture: null,
              ...(w.__owVisionSettings ?? {}),
            });
          }
          const stored = w.__owVisionStored;
          const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
          if (cmd === "vision_get") {
            const phrases = stored.phrases ?? defaultPhrases;
            const neverCapture = stored.never_capture ?? defaultNeverCapture;
            return structuredClone({
              settings: stored,
              phrases,
              never_capture: neverCapture,
              default_phrases: defaultPhrases,
              default_never_capture: defaultNeverCapture,
              phrases_are_default: same(phrases, defaultPhrases),
              never_capture_is_default: same(neverCapture, defaultNeverCapture),
              // Windows has nothing to grant.
              permission: platform === "windows" ? "granted" : (w.__owVisionPermission ?? "granted"),
              platform,
              // Verified on macOS, not yet on Windows
              // (`capture_verified_on` in src-tauri/src/vision.rs).
              capture_verified: platform !== "windows",
              default_trigger: platform === "windows" ? "off" : "phrases",
            });
          }
          if (cmd === "vision_set") {
            if (w.__owVisionSetError) throw w.__owVisionSetError;
            const { settings } = args ?? {};
            const saved = cleaned(settings);
            w.__owVisionStored = saved;
            w.__owVisionSaves = [...(w.__owVisionSaves ?? []), saved];
            window.__owEmit("vision_changed", structuredClone(saved));
            return structuredClone(saved);
          }
          if (cmd === "vision_request_permission") {
            // The system's prompt is not played here: the answer is
            // the state a test set for "after the request".
            const next = w.__owVisionPermissionAfterRequest ?? w.__owVisionPermission ?? "granted";
            w.__owVisionPermission = next;
            return next;
          }
          // Whether a pane was opened: only macOS has one.
          if (cmd === "vision_open_settings") return platform === "macos";
          if (cmd === "vision_running_apps") {
            return (w.__owRunningApps ?? []).map((a) => ({ id: a.id.toLowerCase(), name: a.name }));
          }
          return null;
        }
        // Agent (TASK-99), mirroring tests/fixtures/tauri-shim.ts: the
        // Agent pane. An in-memory Agent block that
        // behaves like Rust (src-tauri/src/agent.rs): a save takes the
        // person's choices and lays them over what is stored, so a
        // window can switch connections and tick tools but cannot add
        // a connection or change what one is; a tick only counts for
        // a tool the connection is known to have; the app table is
        // tidied the way core does it; and how a connection is
        // reached is never in a payload. Which tool is of which kind
        // is core's decision, so a seed says it per tool.
        // `__owAgentSettings` seeds the stored block and
        // `__owAgentListing` what a lookup of the connections finds
        // (`connections`, or `error` when it fails; without it a
        // lookup finds what is there). `__owAgentRefreshMs` makes a
        // lookup take that long, `__owAgentStatus` overrides what
        // `agent_status` answers, `__owAgentSetError` makes a save
        // fail and `__owRunningApps` are the open apps. Every call is
        // recorded in `__owAgentCalls`, every save in `__owAgentSaves`
        // and every connection added by hand in `__owAgentAdded`; a
        // change emits `agent_changed`, as Rust does. The built-in app
        // table here is a short stand-in with made-up apps; the real
        // one is in core/src/settings/agent.rs. Keep in step with
        // tests/fixtures/tauri-shim.ts.
        if (cmd.startsWith("agent_")) {
          const w = window;
          w.__owAgentCalls = [...(w.__owAgentCalls ?? []), { cmd, args: args ?? {} }];
          const key = (name) => String(name ?? "").trim().toLowerCase();
          // `tidy_name` in core: one line, single spaces.
          const tidy = (s) => String(s ?? "").split(/\s+/).filter(Boolean).join(" ");
          // `name_key` in core: lower case, without the prefix of an
          // account connector, letters and digits only.
          const nameKey = (name) =>
            key(name)
              .replace(/^claude\.ai /, "")
              .replace(/[^\p{L}\p{N}]/gu, "");
          const connection = (seed) => ({
            name: tidy(seed.name),
            kind: seed.kind ?? "claude",
            enabled: seed.enabled ?? false,
            allowed_tools: [...(seed.allowed_tools ?? [])],
            tools: (seed.tools ?? []).map((t) => ({
              name: String(t.name ?? ""),
              title: t.title ?? "",
              // A tool nobody said anything about is one that acts.
              kind: t.kind ?? "act",
            })),
            state: seed.state ?? "not-looked-up",
            // The person's own entry needs no lookup.
            usable: seed.usable ?? seed.kind === "custom",
            via: seed.kind === "custom" ? (seed.via ?? "command") : null,
          });
          const defaultApps = [
            { app_id: "com.example.teamchat", app_name: "Teamchat", connection: "Teamchat" },
            { app_id: "com.example.tracker", app_name: "Tracker", connection: "Tracker" },
          ];
          if (!w.__owAgentStored) {
            const seed = w.__owAgentSettings ?? {};
            w.__owAgentStored = {
              enabled: seed.enabled ?? false,
              drafts_only: seed.drafts_only ?? true,
              confirm_timeout_s: seed.confirm_timeout_s ?? 120,
              max_turns: seed.max_turns ?? 12,
              connections: (seed.connections ?? []).map(connection),
              app_connections: seed.app_connections ?? null,
              listing: {
                refreshing: false,
                refreshed_at_ms: null,
                error: "",
                custom_error: "",
                ...(seed.listing ?? {}),
              },
            };
          }
          const stored = w.__owAgentStored;
          // The ticked tools of a connection that an ask could call:
          // with drafts only, a tool that acts is not one of them.
          const runnable = (c) =>
            c.tools.filter(
              (t) => c.allowed_tools.includes(t.name) && !(stored.drafts_only && t.kind === "act"),
            );
          const state = () => {
            const table = stored.app_connections ?? defaultApps;
            return structuredClone({
              enabled: stored.enabled,
              drafts_only: stored.drafts_only,
              confirm_timeout_s: stored.confirm_timeout_s,
              max_turns: stored.max_turns,
              connections: stored.connections,
              app_connections: table.map((row) => {
                const named = (c) =>
                  nameKey(row.connection) !== "" && nameKey(c.name) === nameKey(row.connection);
                const known = stored.connections.find(named);
                const used = stored.connections.find((c) => c.enabled && named(c));
                return {
                  ...row,
                  known: known ? known.name : null,
                  active: used !== undefined && runnable(used).length > 0,
                };
              }),
              app_connections_builtin: stored.app_connections === null,
              listing: stored.listing,
            });
          };
          const announce = () => {
            const now = state();
            window.__owEmit("agent_changed", structuredClone(now));
            return now;
          };
          // A lookup of the connections ended (`agent::refresh` and
          // `merge_listing` in core): what the person chose stays,
          // the tools and the state come from what was found, a new
          // connection arrives switched off, and one that is gone
          // stays only when it was touched.
          const applyListing = () => {
            const found = w.__owAgentListing;
            if (found?.error) {
              stored.listing = { ...stored.listing, refreshing: false, error: found.error };
              return;
            }
            if (found?.connections) {
              const names = new Set(found.connections.map((s) => key(s.name)));
              for (const seed of found.connections) {
                const fresh = connection(seed);
                const at = stored.connections.findIndex(
                  (c) => key(c.name) === key(fresh.name) && c.kind === fresh.kind,
                );
                if (at >= 0) {
                  const kept = stored.connections[at];
                  stored.connections[at] = {
                    ...kept,
                    tools: fresh.tools.length > 0 ? fresh.tools : kept.tools,
                    state: seed.state ?? "connected",
                    usable: true,
                  };
                } else if (fresh.kind === "claude") {
                  stored.connections.push({
                    ...fresh,
                    enabled: false,
                    allowed_tools: [],
                    state: seed.state ?? "connected",
                    usable: true,
                  });
                }
              }
              stored.connections = stored.connections
                .filter(
                  (c) =>
                    c.kind === "custom" ||
                    names.has(key(c.name)) ||
                    c.enabled ||
                    c.allowed_tools.length > 0,
                )
                .map((c) =>
                  names.has(key(c.name))
                    ? c
                    : c.kind === "custom"
                      ? { ...c, state: "unknown" }
                      : { ...c, state: "missing", usable: false },
                );
            } else {
              // Nothing seeded: the lookup finds what is there.
              stored.connections = stored.connections.map((c) => ({
                ...c,
                state: c.state === "not-looked-up" ? "connected" : c.state,
                usable: true,
              }));
            }
            stored.listing = {
              refreshing: false,
              refreshed_at_ms: Date.now(),
              error: "",
              custom_error: found?.custom_error ?? "",
            };
          };
          if (cmd === "agent_get") return state();
          if (cmd === "agent_set") {
            if (w.__owAgentSetError) throw w.__owAgentSetError;
            const { choices } = (args ?? {});
            w.__owAgentSaves = [...(w.__owAgentSaves ?? []), structuredClone(choices)];
            const wasOn = stored.enabled;
            // `with_choices` in src-tauri/src/agent.rs. A switch that
            // is left out is the careful one.
            stored.enabled = choices.enabled ?? false;
            stored.drafts_only = choices.drafts_only ?? true;
            for (const c of stored.connections) {
              const choice = (choices.connections ?? []).find((x) => key(x.name) === key(c.name));
              if (!choice) continue;
              c.enabled = choice.enabled ?? false;
              c.allowed_tools = (choice.allowed_tools ?? []).filter(
                (t) => c.tools.some((x) => x.name === t) || c.allowed_tools.includes(t),
              );
            }
            // `AgentSettings::cleaned` in core: ids in lower case,
            // each app once, a row without an id or a connection
            // dropped.
            if (Array.isArray(choices.app_connections)) {
              const rows = [];
              for (const raw of choices.app_connections) {
                const row = {
                  app_id: tidy(raw.app_id).toLowerCase(),
                  app_name: tidy(raw.app_name),
                  connection: tidy(raw.connection),
                };
                if (!row.app_id || !row.connection || rows.some((r) => r.app_id === row.app_id)) {
                  continue;
                }
                rows.push(row);
              }
              stored.app_connections = rows;
            } else {
              stored.app_connections = null;
            }
            // Agent went on: the connections are looked up in the
            // background, and the windows hear when that is over.
            if (!wasOn && stored.enabled) {
              stored.listing = { ...stored.listing, refreshing: true };
              setTimeout(() => {
                applyListing();
                announce();
              }, w.__owAgentRefreshMs ?? 0);
            }
            return announce();
          }
          if (cmd === "agent_refresh") {
            stored.listing = { ...stored.listing, refreshing: true };
            return new Promise((resolve) =>
              setTimeout(() => {
                applyListing();
                resolve(announce());
              }, w.__owAgentRefreshMs ?? 0),
            );
          }
          if (cmd === "agent_status") {
            const askEnabled = w.__owAskStored?.enabled ?? w.__owAskSettings?.enabled ?? false;
            const backend =
              w.__owAskBackend?.backend ??
              w.__owRefineStored?.backend ??
              w.__owRefineSettings?.backend ??
              "claude-cli";
            // `agent::readiness` in core.
            const on = stored.connections.filter((c) => c.enabled);
            const used = on.filter((c) => runnable(c).length > 0);
            const tools = used.flatMap(runnable);
            const asking = tools.filter((t) => t.kind === "act").length;
            const plural = (n) => (n === 1 ? "" : "s");
            let ready = false;
            let message;
            if (!stored.enabled) {
              message = "Agent is switched off.";
            } else if (!askEnabled) {
              message = "Agent only works in an ask, and Ask is switched off.";
            } else if (backend === "openai-compatible") {
              message =
                "Agent needs the Claude backend. An OpenAI-compatible endpoint cannot use connections.";
            } else if (on.length === 0) {
              message = "No connection is enabled yet.";
            } else if (tools.length === 0) {
              message = stored.drafts_only
                ? "No tool that can run is ticked. With drafts only, tools that send, post, delete or change something do not count."
                : "No tool is ticked on an enabled connection.";
            } else {
              ready = true;
              message = `An ask can use ${tools.length} tool${plural(tools.length)} on ${used.length} connection${plural(used.length)}.`;
              if (stored.drafts_only) {
                message +=
                  " Drafts only: nothing that sends, posts, deletes or changes anything can run.";
              } else if (asking > 0) {
                message += ` ${asking} of them ask you first, every time.`;
              }
            }
            return { ready, message, ask_enabled: askEnabled, backend, ...(w.__owAgentStatus ?? {}) };
          }
          if (cmd === "agent_add_connection") {
            // `with_custom` in src-tauri/src/agent.rs.
            const given = (args ?? {});
            const name = tidy(given.name);
            const command = String(given.command ?? "").trim();
            const url = String(given.url ?? "").trim();
            if (!name) throw "Give the connection a name.";
            if (stored.connections.some((c) => key(c.name) === key(name))) {
              throw `There is already a connection called "${name}".`;
            }
            if (!command && !url) throw "Give the connection a command or a URL.";
            w.__owAgentAdded = [
              ...(w.__owAgentAdded ?? []),
              { name, command, args: (given.args ?? []).filter((a) => a.trim() !== ""), url },
            ];
            // Off, nothing ticked, no tools known until a lookup.
            stored.connections.push(
              connection({ name, kind: "custom", via: command ? "command" : "url" }),
            );
            return announce();
          }
          if (cmd === "agent_remove_connection") {
            const { name } = (args ?? {});
            const at = stored.connections.findIndex((c) => key(c.name) === key(name));
            if (at < 0) throw `There is no connection called "${String(name ?? "").trim()}".`;
            stored.connections.splice(at, 1);
            return announce();
          }
          if (cmd === "agent_running_apps") {
            return (w.__owRunningApps ?? []).map((a) => ({
              app_id: a.id.toLowerCase(),
              app_name: a.name,
            }));
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
