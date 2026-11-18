import { test as base, expect, type Page } from "@playwright/test";

// Mirrors the DictationTick payload that core/src-tauri emits at 20 Hz.
export interface MockTick {
  phase: number;
  status: "idle" | "recording" | "transcribing";
  status_message?: string;
  transcript?: string;
  replacements?: { from: string; to: string }[];
  // What the Refine step did (mirrors `RefineInfo` in core).
  refine?: {
    raw_transcript: string;
    status: string;
    detail: string;
    instruction?: string;
    backend: string;
    elapsed_ms: number;
    // The mode the dictation ran in (TASK-96): its name ("" for the
    // built-in Dictation mode when it is simply the selected one),
    // whether it is the built-in one, why it ran and, for an app
    // default, the app's name.
    mode?: string;
    mode_builtin?: boolean;
    mode_reason?: "" | "selected" | "app-default";
    mode_app?: string;
  };
  confidence?: number;
  sample_count?: number;
  elapsed_ms?: number;
  error_message?: string;
  can_toggle?: boolean;
  is_recording?: boolean;
  // What the recording is (or the last one was) for (TASK-97).
  session_kind?: "dictation" | "ask";
  // The cancel key would do something: a recording or a running ask.
  // Left out, `emitTick` sends what `is_recording` says, as Rust
  // does while no ask runs.
  cancel_would_act?: boolean;
  level?: number;
  download_bytes_done?: number;
  download_bytes_total?: number;
}

export const TICK_DEFAULTS: Required<
  Omit<MockTick, "phase" | "status" | "cancel_would_act">
> = {
  status_message: "",
  transcript: "",
  replacements: [],
  refine: {
    raw_transcript: "",
    status: "",
    detail: "",
    instruction: "",
    backend: "",
    elapsed_ms: 0,
    mode: "",
    mode_builtin: false,
    mode_reason: "",
    mode_app: "",
  },
  confidence: 0,
  sample_count: 0,
  elapsed_ms: 0,
  error_message: "",
  can_toggle: true,
  is_recording: false,
  session_kind: "dictation",
  level: 0,
  download_bytes_done: 0,
  download_bytes_total: 0,
};

declare global {
  interface Window {
    __owEmit: (event: string, payload: unknown) => number;
    // How many listeners an event has, without emitting anything.
    __owListeners: (event: string) => number;
    __TAURI_INTERNALS__: unknown;
    __TAURI_EVENT_PLUGIN_INTERNALS__: unknown;
  }
}

// Install the Tauri 2 internals stub before the SPA boots. Records emitted
// `dictation_tick` events into a queue the test can replay.
async function installTauriShim(
  page: Page,
  label: "main" | "pill" | "answer" = "main",
) {
  await page.addInitScript((windowLabel) => {
    const handlers = new Map<string, Set<number>>();
    const callbacks = new Map<number, (payload: unknown) => void>();
    let nextId = 1;
    let tickCounter = 0;

    window.__TAURI_INTERNALS__ = {
      metadata: {
        currentWindow: { label: windowLabel },
        currentWebview: { label: windowLabel, windowLabel },
      },
      invoke: async (cmd: string, args?: Record<string, unknown>) => {
        // plugin:window|<verb> — record + return sane defaults so the
        // WindowControls component can call minimize/toggleMaximize/close
        // without a real Tauri runtime. Verb is the suffix after the pipe.
        if (cmd.startsWith("plugin:window|")) {
          const verb = cmd.slice("plugin:window|".length);
          const w = window as unknown as {
            __owWindowCalls?: string[];
            __owMaximized?: boolean;
          };
          w.__owWindowCalls = w.__owWindowCalls ?? [];
          w.__owWindowCalls.push(verb);
          if (verb === "is_maximized") return w.__owMaximized ?? false;
          if (verb === "toggle_maximize") {
            w.__owMaximized = !(w.__owMaximized ?? false);
            return null;
          }
          return null;
        }
        if (cmd === "core_version") return "0.1.0-test";
        if (cmd === "plugin:app|name") return "OpenWhisper Dev";
        if (cmd === "dictation_toggle") return null;
        if (cmd === "dictation_cancel") return false;
        // Modes (TASK-96). An in-memory modes block that behaves like
        // core: ids are slugs made from the name, names are unique
        // ignoring case, app ids are kept in lower case, the built-in
        // Dictation mode is listed first, deleting a mode drops the app
        // defaults that used it and moves the selection back to
        // Dictation, and a mode's own model and effort belong to the
        // Refine backend they were picked for. `__owModesSettings`
        // seeds the block. The apps offered for a default are the apps
        // that have one, then the apps of the seeded history entries,
        // then the open apps seeded with `__owRunningApps`. Every
        // change is recorded in `__owModesCalls` and emits
        // `modes_changed`, as Rust does. Keep in step with
        // scripts/pw-shim.js.
        if (cmd.startsWith("modes_")) {
          type Backend = "claude-cli" | "openai-compatible";
          type Mode = {
            id: string;
            name: string;
            instructions: string;
            model: string | null;
            effort: string | null;
            backend?: Backend | null;
          };
          type AppDefault = { app_id: string; app_name: string; mode: string };
          type ModesSettings = { active: string; custom: Mode[]; app_defaults: AppDefault[] };
          const w = window as unknown as {
            __owModesSettings?: Partial<ModesSettings>;
            __owModesStored?: ModesSettings;
            __owModesCalls?: { cmd: string; args: Record<string, unknown> }[];
            __owModesCycleCount?: number;
            __owHistoryEntries?: { app_id: string | null; app_name: string | null }[];
            __owRunningApps?: { id: string; name: string }[];
            __owRefineSettings?: { instructions?: string; backend?: Backend };
            __owRefineStored?: { instructions: string; backend: Backend };
          };
          const BUILTIN = "dictation";
          const BUILTIN_NAME = "Dictation";
          // `mode_slug` in core: lower case, letters and digits kept,
          // every run of anything else one hyphen.
          const slug = (text: string) =>
            text
              .trim()
              .toLowerCase()
              .replace(/[^\p{L}\p{N}]+/gu, "-")
              .replace(/^-+|-+$/g, "");
          // `app_key` and `mode_name_key` in core: how app ids and mode
          // names are compared (and app ids stored).
          const key = (text: string) => text.trim().toLowerCase();
          // `Mode::override_backend` in core: the backend a mode's own
          // model and effort are for. A block from before modes stored
          // one means the Claude CLI.
          const overrideBackend = (m: { model: string | null; effort: string | null; backend?: Backend | null }) =>
            m.model || m.effort ? (m.backend ?? "claude-cli") : null;
          // The backend Refine uses: the one a model picked in the
          // dialog is for.
          const activeBackend = (): Backend =>
            w.__owRefineStored?.backend ?? w.__owRefineSettings?.backend ?? "claude-cli";
          // `ModesSettings::cleaned` in core.
          const cleaned = (raw: ModesSettings): ModesSettings => {
            const custom: Mode[] = [];
            const exists = (id: string) => id === BUILTIN || custom.some((m) => m.id === id);
            const nameTaken = (name: string) =>
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
            const app_defaults: AppDefault[] = [];
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
            } as ModesSettings);
          }
          const stored = w.__owModesStored;
          // `modes::list` in core (the built-in mode, put together from
          // the Refine instructions, then the user's own), each with
          // what its own model and effort mean for the backend in use,
          // as `ModeRow` in src-tauri/src/modes.rs adds.
          const list = (settings: ModesSettings) =>
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
          const now = (settings: ModesSettings) =>
            structuredClone({ settings, modes: list(settings) });
          if (cmd === "modes_get") {
            // `modes::apps_for_defaults` in core: the apps with a
            // default, then the apps in the history, then the apps
            // that are open, each once whatever the case of its id.
            const apps: { id: string; name: string }[] = [];
            const offer = (id: string | null, name: string | null) => {
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
          const a = (args ?? {}) as Record<string, unknown>;
          const unknown = (id: unknown) => `There is no mode with the id "${String(id)}".`;
          const has = (id: unknown) =>
            id === BUILTIN || stored.custom.some((m) => m.id === id);
          // `check_name_free` in core: a name another mode has, ignoring
          // case and outer space, is refused.
          const nameHolder = (name: string, except: string | null) =>
            key(name) === key(BUILTIN_NAME)
              ? BUILTIN_NAME
              : stored.custom.find((m) => m.id !== except && key(m.name) === key(name))?.name;
          const taken = (holder: string) =>
            `There is already a mode called "${holder}". Give this one another name.`;
          let next: ModesSettings = structuredClone(stored);
          // The user switching mode, as opposed to editing the modes.
          let switched = false;
          if (cmd === "modes_select") {
            if (!has(a.id)) throw unknown(a.id);
            next.active = a.id as string;
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
              instructions: (a.instructions as string) ?? "",
              model: (a.model as string | null) ?? null,
              effort: (a.effort as string | null) ?? null,
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
            mode.instructions = (a.instructions as string) ?? "";
            // `ModeEdit::backend` in core: a model and effort kept for
            // the other backend are left alone by an edit that names
            // neither, and replaced together by one that names either.
            const model = (a.model as string | null) ?? null;
            const effort = (a.effort as string | null) ?? null;
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
              mode: a.modeId as string,
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
        if (cmd === "hotkey_status_current") return null;
        if (cmd === "permissions_status_current") return null;
        if (cmd === "hotkey_retry") {
          (window as unknown as { __owHotkeyRetryCount?: number }).__owHotkeyRetryCount =
            ((window as unknown as { __owHotkeyRetryCount?: number }).__owHotkeyRetryCount ?? 0) + 1;
          return null;
        }
        // Brings the main window forward on Settings. With a `pane`
        // (and a `focus` row on it) Rust sends the object form of
        // `ow_navigate`, without one the plain string
        // (`settings_payload` in src-tauri/src/navigate.rs). The
        // arguments of every call are kept in `__owOpenSettingsCalls`.
        // Keep in step with scripts/pw-shim.js.
        if (cmd === "open_settings_window") {
          const w = window as unknown as {
            __owOpenSettingsCount?: number;
            __owOpenSettingsCalls?: Record<string, unknown>[];
          };
          w.__owOpenSettingsCount = (w.__owOpenSettingsCount ?? 0) + 1;
          w.__owOpenSettingsCalls = [...(w.__owOpenSettingsCalls ?? []), args ?? {}];
          const named = (value: unknown) =>
            typeof value === "string" && value.trim() ? value.trim() : null;
          const { pane, focus } = (args ?? {}) as { pane?: unknown; focus?: unknown };
          const toPane = named(pane);
          const toRow = named(focus);
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
        // Hotkey slots. `toggle` and `cancel` always have a binding;
        // the optional `cycle_mode` slot is absent until set, the way
        // the Rust side leaves it out of the payload. The command
        // target for it is "cycle-mode" (kebab-case), the settings key
        // "cycle_mode". The optional `ask` slot (TASK-97) has a
        // default (the mac one here, like the toggle default), which
        // a seed without the key gets too, the way Rust fills it in
        // for a file from before the slot; cleared, it is null. Keep
        // in step with scripts/pw-shim.js.
        if (
          cmd === "settings_get_hotkeys" ||
          cmd === "settings_set_hotkey" ||
          cmd === "settings_reset_hotkey" ||
          cmd === "settings_clear_hotkey"
        ) {
          const w = window as unknown as {
            __owHotkeys?: Record<string, unknown>;
            __owHotkeySetCount?: number;
            __owHotkeyResetCount?: number;
            __owHotkeyClearCount?: number;
            __owHotkeyLastTarget?: string;
            __owHotkeyLastConfig?: unknown;
          };
          const askDefault = { kind: "modifier-tap", code: "RightOption", mods: [] };
          w.__owHotkeys = w.__owHotkeys ?? {
            toggle: { kind: "modifier-tap", code: "RightCommand", mods: [] },
            cancel: { kind: "chord", code: "Escape", mods: [] },
          };
          if (!("ask" in w.__owHotkeys)) w.__owHotkeys.ask = askDefault;
          // A copy, as over real IPC: the caller must not see later
          // writes through a shared object.
          if (cmd === "settings_get_hotkeys") return structuredClone(w.__owHotkeys);

          const { target, config } = (args ?? {}) as {
            target: "toggle" | "cancel" | "cycle-mode" | "ask";
            config?: unknown;
          };
          const key = target === "cycle-mode" ? "cycle_mode" : target;
          w.__owHotkeyLastTarget = target;
          if (cmd === "settings_set_hotkey") {
            w.__owHotkeys[key] = config;
            w.__owHotkeySetCount = (w.__owHotkeySetCount ?? 0) + 1;
            w.__owHotkeyLastConfig = config;
            return null;
          }
          if (cmd === "settings_clear_hotkey") {
            // Only the optional slots can be without a key.
            if (target === "toggle" || target === "cancel") {
              throw `The ${target} shortcut cannot be removed, only changed.`;
            }
            w.__owHotkeyClearCount = (w.__owHotkeyClearCount ?? 0) + 1;
            if (target === "ask") w.__owHotkeys.ask = null;
            else delete w.__owHotkeys[key];
            return null;
          }
          // Reset: the default binding, or "not set" (null) for the
          // switch-mode slot, which is removed.
          w.__owHotkeyResetCount = (w.__owHotkeyResetCount ?? 0) + 1;
          if (target === "cycle-mode") {
            delete w.__owHotkeys[key];
            return null;
          }
          const def =
            target === "toggle"
              ? { kind: "modifier-tap", code: "RightCommand", mods: [] }
              : target === "ask"
                ? askDefault
                : { kind: "chord", code: "Escape", mods: [] };
          w.__owHotkeys[key] = def;
          return def;
        }
        // Ask (TASK-97). `__owAskSettings` seeds the stored block and
        // `__owAskBackend` overrides the backend status (`ask_status`).
        // `ask_context_preview` answers with the facts of a made-up
        // computer for the groups the stored block switches on;
        // `__owAskContextFail` makes it fail.
        // The one ask lives on `__owAskSnapshot`: `emitAskTick` (below)
        // sets it, and cancel, dismiss, insert and copy answer from it
        // the way core does. Every call is recorded in `__owAskCalls`,
        // saves in `__owAskSaves`, copied text in `__owAskCopied` and
        // inserted text in `__owAskInserted`. Keep in step with
        // scripts/pw-shim.js.
        if (cmd.startsWith("ask_")) {
          type AskSettings = {
            enabled: boolean;
            web_search: boolean;
            instructions: string;
            model: string | null;
            effort: string | null;
            timeout_ms: number;
            context: {
              date_time: boolean;
              computer: boolean;
              front_app: boolean;
              locale: boolean;
              place: string;
            };
          };
          type AskSnapshot = Record<string, unknown> & {
            id: number;
            phase: string;
            answer: string;
            version: number;
          };
          const w = window as unknown as {
            __owAskSettings?: Partial<Omit<AskSettings, "context">> & {
              context?: Partial<AskSettings["context"]>;
            };
            __owAskStored?: AskSettings;
            __owAskContextFail?: boolean;
            __owAskSaves?: AskSettings[];
            __owAskBackend?: Record<string, unknown>;
            __owAskCalls?: { cmd: string; args: Record<string, unknown> }[];
            __owAskSnapshot?: AskSnapshot;
            __owAskCopied?: string[];
            __owAskInserted?: string[];
          };
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
          const idle = (version: number): AskSnapshot => ({
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
          // A change of the ask: stored, and sent as `ask_tick`.
          const change = (next: AskSnapshot) => {
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
          const meant = (args ?? {}) as { id?: number | null };
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
            const { settings } = (args ?? {}) as { settings: AskSettings };
            // `AskSettings::cleaned` in core.
            const saved: AskSettings = {
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
            const lines: string[] = [];
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
            // No model here: the question is on the snapshot, and a
            // test plays the answer with `emitAskTick`.
            const { text } = (args ?? {}) as { text?: string };
            const next = {
              ...idle(snap.version + 1),
              id: snap.id + 1,
              phase: "thinking",
              question: (text ?? "").trim(),
            };
            change(next);
            return structuredClone(next);
          }
          if (cmd === "ask_toggle") return null;
          if (cmd === "ask_cancel") {
            if (!busy || other) return false;
            // A listening ask is simply gone; a running one shows as
            // cancelled, with what arrived of the answer.
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
          const { target } = (args ?? {}) as {
            target?: "toggle" | "cancel" | "cycle-mode" | "ask";
          };
          (window as unknown as { __owCaptureStartCount?: number }).__owCaptureStartCount =
            ((window as unknown as { __owCaptureStartCount?: number }).__owCaptureStartCount ?? 0) + 1;
          (window as unknown as { __owCaptureLastTarget?: string }).__owCaptureLastTarget = target;
          return null;
        }
        if (cmd === "settings_capture_hotkey_cancel") {
          (window as unknown as { __owCaptureCancelCount?: number }).__owCaptureCancelCount =
            ((window as unknown as { __owCaptureCancelCount?: number }).__owCaptureCancelCount ?? 0) + 1;
          return null;
        }
        if (cmd === "audio_get_device_state") {
          // Mirror the Rust shell's snapshot shape. Tests stash device
          // fixtures on `__owAudioDevices` and the saved selection on
          // `__owAudioDevice` (the cpal id). Defaults match the seed
          // pair below so a test that just opens the pane gets a sane
          // pre-populated picker.
          const w = window as unknown as {
            __owAudioDevices?: Array<{
              id: string;
              label: string;
              is_default: boolean;
            }>;
            __owAudioDevice?: string | null;
            __owAudioDefaultLabel?: string | null;
            __owAudioSelectedPresent?: boolean;
          };
          const devices = w.__owAudioDevices ?? [
            {
              id: "default-mic",
              label: "MacBook Pro Microphone",
              is_default: true,
            },
            { id: "airpods-pro", label: "AirPods Pro", is_default: false },
          ];
          const selectedId = w.__owAudioDevice ?? null;
          const defaultLabel =
            w.__owAudioDefaultLabel ??
            devices.find((d) => d.is_default)?.label ??
            null;
          const selectedPresent =
            w.__owAudioSelectedPresent ??
            (selectedId === null || devices.some((d) => d.id === selectedId));
          return {
            devices,
            selected_id: selectedId,
            selected_present: selectedPresent,
            default_label: defaultLabel,
          };
        }
        if (cmd === "audio_set_device") {
          const { id } = (args ?? {}) as { id: string | null };
          const w = window as unknown as {
            __owAudioDevice?: string | null;
            __owAudioSetCount?: number;
            __owAudioLastSet?: string | null;
          };
          w.__owAudioDevice = id;
          w.__owAudioLastSet = id;
          w.__owAudioSetCount = (w.__owAudioSetCount ?? 0) + 1;
          return null;
        }
        if (cmd === "behavior_get_show_in_fullscreen") {
          const w = window as unknown as { __owShowInFullscreen?: boolean };
          return w.__owShowInFullscreen ?? false;
        }
        if (cmd === "behavior_set_show_in_fullscreen") {
          const { enabled } = (args ?? {}) as { enabled: boolean };
          const w = window as unknown as {
            __owShowInFullscreen?: boolean;
            __owShowInFullscreenLastSet?: boolean;
            __owShowInFullscreenSetCount?: number;
          };
          w.__owShowInFullscreen = enabled;
          w.__owShowInFullscreenLastSet = enabled;
          w.__owShowInFullscreenSetCount =
            (w.__owShowInFullscreenSetCount ?? 0) + 1;
          return null;
        }
        if (cmd === "behavior_get_pause_audio_during_dictation") {
          const w = window as unknown as { __owPauseAudio?: boolean };
          return w.__owPauseAudio ?? true;
        }
        if (cmd === "behavior_set_pause_audio_during_dictation") {
          const { enabled } = (args ?? {}) as { enabled: boolean };
          const w = window as unknown as {
            __owPauseAudio?: boolean;
            __owPauseAudioLastSet?: boolean;
            __owPauseAudioSetCount?: number;
          };
          w.__owPauseAudio = enabled;
          w.__owPauseAudioLastSet = enabled;
          w.__owPauseAudioSetCount = (w.__owPauseAudioSetCount ?? 0) + 1;
          return null;
        }
        if (cmd === "behavior_get_bt_resume_delay_ms") {
          const w = window as unknown as { __owBtResumeDelayMs?: number };
          return w.__owBtResumeDelayMs ?? 5000;
        }
        if (cmd === "behavior_set_bt_resume_delay_ms") {
          const { delayMs } = (args ?? {}) as { delayMs: number };
          const clamped = Math.min(Math.max(delayMs, 0), 10000);
          const w = window as unknown as {
            __owBtResumeDelayMs?: number;
            __owBtResumeDelayLastSet?: number;
            __owBtResumeDelaySetCount?: number;
          };
          w.__owBtResumeDelayMs = clamped;
          w.__owBtResumeDelayLastSet = clamped;
          w.__owBtResumeDelaySetCount =
            (w.__owBtResumeDelaySetCount ?? 0) + 1;
          return null;
        }
        if (cmd === "audio_preview_start") {
          const w = window as unknown as { __owAudioPreviewStarts?: number };
          w.__owAudioPreviewStarts = (w.__owAudioPreviewStarts ?? 0) + 1;
          return null;
        }
        if (cmd === "audio_preview_stop") {
          const w = window as unknown as { __owAudioPreviewStops?: number };
          w.__owAudioPreviewStops = (w.__owAudioPreviewStops ?? 0) + 1;
          return null;
        }
        if (cmd === "plugin:autostart|is_enabled") {
          const w = window as unknown as {
            __owAutostart?: boolean;
            __owAutostartIsEnabledShouldThrow?: boolean;
          };
          if (w.__owAutostartIsEnabledShouldThrow) {
            throw new Error("autostart isEnabled failed");
          }
          return w.__owAutostart ?? false;
        }
        if (cmd === "plugin:autostart|enable") {
          const w = window as unknown as {
            __owAutostart?: boolean;
            __owAutostartEnableShouldThrow?: boolean;
            __owAutostartEnableCount?: number;
            __owAutostartLastSet?: boolean;
          };
          w.__owAutostartEnableCount = (w.__owAutostartEnableCount ?? 0) + 1;
          if (w.__owAutostartEnableShouldThrow) {
            throw new Error("autostart enable failed");
          }
          w.__owAutostart = true;
          w.__owAutostartLastSet = true;
          return null;
        }
        if (cmd === "plugin:autostart|disable") {
          const w = window as unknown as {
            __owAutostart?: boolean;
            __owAutostartDisableShouldThrow?: boolean;
            __owAutostartDisableCount?: number;
            __owAutostartLastSet?: boolean;
          };
          w.__owAutostartDisableCount = (w.__owAutostartDisableCount ?? 0) + 1;
          if (w.__owAutostartDisableShouldThrow) {
            throw new Error("autostart disable failed");
          }
          w.__owAutostart = false;
          w.__owAutostartLastSet = false;
          return null;
        }
        if (cmd === "settings_get_pill") {
          const stored = (window as unknown as { __owPillFollow?: boolean })
            .__owPillFollow;
          return { follow_active_screen: stored ?? true };
        }
        if (cmd === "stats_get_summary") {
          const w = window as unknown as {
            __owStatsSummary?: {
              words_today: number;
              words_week: number;
              words_all_time: number;
              seconds_total: number;
            };
            __owStatsGetCount?: number;
          };
          w.__owStatsGetCount = (w.__owStatsGetCount ?? 0) + 1;
          return (
            w.__owStatsSummary ?? {
              words_today: 0,
              words_week: 0,
              words_all_time: 0,
              seconds_total: 0,
            }
          );
        }
        if (cmd === "stats_reset") {
          const w = window as unknown as { __owStatsResetCount?: number };
          w.__owStatsResetCount = (w.__owStatsResetCount ?? 0) + 1;
          return null;
        }
        if (cmd === "settings_get_stats") {
          const w = window as unknown as { __owUserWpm?: number };
          return { user_wpm: w.__owUserWpm ?? 40 };
        }
        if (cmd === "settings_set_user_wpm") {
          const { wpm } = (args ?? {}) as { wpm: number };
          const clamped = Math.max(10, Math.min(300, wpm));
          const w = window as unknown as {
            __owUserWpm?: number;
            __owUserWpmSetCount?: number;
            __owUserWpmLast?: number;
          };
          w.__owUserWpm = clamped;
          w.__owUserWpmSetCount = (w.__owUserWpmSetCount ?? 0) + 1;
          w.__owUserWpmLast = clamped;
          return clamped;
        }
        if (cmd === "telemetry_get_memory") {
          const w = window as unknown as {
            __owMemoryStats?: MockMemoryStats;
            __owTelemetryGetCount?: number;
          };
          w.__owTelemetryGetCount = (w.__owTelemetryGetCount ?? 0) + 1;
          const stash = w.__owMemoryStats;
          if (!stash) {
            return {
              system: {
                total_bytes: 0,
                used_bytes: 0,
                available_bytes: 0,
                swap_total_bytes: 0,
                swap_used_bytes: 0,
              },
              process: {
                rss_bytes: 0,
                peak_rss_bytes: 0,
                timestamp_unix_ms: 0,
              },
              models: [],
            };
          }
          // Fill in claimed_bytes / in_process defaults so legacy
          // fixtures that predate ModelClaim keep working: missing
          // in_process → true, missing claimed_bytes → 0. Specs that
          // test ANE-resident memory must set both explicitly.
          return {
            ...stash,
            models: stash.models.map((m) => ({
              label: m.label,
              state: m.state,
              estimated_rss_bytes: m.estimated_rss_bytes,
              claimed_bytes: m.claimed_bytes ?? 0,
              in_process: m.in_process ?? true,
            })),
          };
        }
        // Dictionary pane. `__owDictionary` holds the stored terms;
        // save mimics core's clean-up (trim, drop empties, merge a
        // repeated term, sort) so the pane renders what Rust returns.
        if (cmd === "dictionary_get" || cmd === "dictionary_save") {
          type Term = { term: string; heard_as?: string[] };
          const w = window as unknown as {
            __owDictionary?: Term[];
            __owDictionarySaveCount?: number;
          };
          if (cmd === "dictionary_save") {
            const { terms } = (args ?? {}) as { terms: Term[] };
            const merged: Term[] = [];
            for (const t of terms) {
              const term = t.term.trim();
              if (!term) continue;
              const heard = (t.heard_as ?? []).map((h) => h.trim()).filter(Boolean);
              const existing = merged.find(
                (m) => m.term.toLowerCase() === term.toLowerCase(),
              );
              if (existing) {
                existing.heard_as = [...(existing.heard_as ?? []), ...heard];
              } else {
                merged.push(heard.length ? { term, heard_as: heard } : { term });
              }
            }
            merged.sort((a, b) => a.term.toLowerCase().localeCompare(b.term.toLowerCase()));
            w.__owDictionary = merged;
            w.__owDictionarySaveCount = (w.__owDictionarySaveCount ?? 0) + 1;
          }
          return {
            path: "/Users/test/Library/Application Support/com.openwhisper.app/dictionary.json",
            terms: w.__owDictionary ?? [],
          };
        }
        if (cmd === "dictionary_reveal") {
          const w = window as unknown as { __owDictionaryRevealCount?: number };
          w.__owDictionaryRevealCount = (w.__owDictionaryRevealCount ?? 0) + 1;
          return null;
        }
        // Transcript history (TASK-95). `__owHistorySettings` seeds the
        // settings block and `__owHistoryEntries` the stored dictations
        // (newest first). Saves, deletes, clears and reveals are
        // recorded for assertions. Search, paging and removal mimic
        // core::history. The folder picker answers with `__owDialogPick`.
        if (cmd.startsWith("history_")) {
          type HistorySettings = {
            enabled: boolean;
            retention_days: number;
            folder: string | null;
          };
          type HistoryEntry = {
            id: string;
            at_ms: number;
            duration_ms: number;
            word_count: number;
            text: string;
            original: string | null;
            app_id: string | null;
            app_name: string | null;
            // Name of the mode the text was rewritten in (TASK-96).
            mode?: string | null;
            // An ask (TASK-97): `kind` is "ask", `text` the question
            // and `answer` its answer. Left out for a dictation; the
            // list fills in null, as Rust sends it.
            kind?: "ask" | null;
            answer?: string | null;
          };
          const w = window as unknown as {
            __owHistorySettings?: Partial<HistorySettings>;
            __owHistoryStored?: HistorySettings;
            __owHistorySaves?: HistorySettings[];
            __owHistoryEntries?: HistoryEntry[];
            __owHistoryDeleted?: string[];
            __owHistoryClearCount?: number;
            __owHistoryRevealCount?: number;
          };
          if (!w.__owHistoryStored) {
            w.__owHistoryStored = {
              enabled: false,
              retention_days: 30,
              folder: null,
              ...(w.__owHistorySettings ?? {}),
            };
          }
          const entries = w.__owHistoryEntries ?? [];
          const defaultFolder =
            "/Users/test/Library/Application Support/com.openwhisper.app/history";
          const state = () => ({
            settings: w.__owHistoryStored,
            folder: w.__owHistoryStored?.folder ?? defaultFolder,
            default_folder: defaultFolder,
          });
          if (cmd === "history_get") {
            return state();
          }
          if (cmd === "history_set") {
            const { settings } = (args ?? {}) as { settings: HistorySettings };
            const saved = {
              ...settings,
              retention_days: Math.min(settings.retention_days, 3650),
              folder: settings.folder?.trim() || null,
            };
            w.__owHistoryStored = saved;
            w.__owHistorySaves = [...(w.__owHistorySaves ?? []), saved];
            return state();
          }
          if (cmd === "history_reveal") {
            w.__owHistoryRevealCount = (w.__owHistoryRevealCount ?? 0) + 1;
            return null;
          }
          if (cmd === "history_list") {
            const { search, limit, offset } = (args ?? {}) as {
              search?: string;
              limit?: number;
              offset?: number;
            };
            const needle = (search ?? "").trim().toLowerCase();
            const hits = needle
              ? entries.filter((e) =>
                  [e.text, e.original ?? "", e.app_name ?? "", e.answer ?? ""].some((s) =>
                    s.toLowerCase().includes(needle),
                  ),
                )
              : entries;
            const from = offset ?? 0;
            return {
              entries: hits
                .slice(from, from + (limit ?? 30))
                .map((e) => ({ ...e, kind: e.kind ?? null, answer: e.answer ?? null })),
              matched: hits.length,
              stored: entries.length,
            };
          }
          if (cmd === "history_delete") {
            const { id } = (args ?? {}) as { id: string };
            w.__owHistoryDeleted = [...(w.__owHistoryDeleted ?? []), id];
            const before = entries.length;
            w.__owHistoryEntries = entries.filter((e) => e.id !== id);
            return w.__owHistoryEntries.length < before;
          }
          if (cmd === "history_clear") {
            w.__owHistoryClearCount = (w.__owHistoryClearCount ?? 0) + 1;
            w.__owHistoryEntries = [];
            return entries.length;
          }
        }
        // Refine pane (TASK-93). `__owRefineSettings` seeds the stored
        // block; `__owRefineStatus` / `__owRefineTest` override what the
        // backend probe and the connection test answer. Saves, test
        // runs and the stored key are recorded for assertions. The key
        // is write-only here too: no command ever returns it.
        if (cmd.startsWith("refine_")) {
          type Settings = {
            enabled: boolean;
            backend: "claude-cli" | "openai-compatible";
            instructions: string;
            timeout_ms: number;
            min_words: number;
            drift_guard: boolean;
            no_long_dashes: boolean;
            claude_cli: { model: string; effort: string; path: string | null };
            openai: { base_url: string; model: string; effort: string | null };
          };
          const w = window as unknown as {
            __owRefineSettings?: Partial<Settings>;
            __owRefineStored?: Settings;
            __owRefineSaves?: Settings[];
            __owRefineStatus?: Record<string, unknown>;
            __owRefineStatusCount?: number;
            __owRefineTest?: Record<string, unknown>;
            __owRefineTestCount?: number;
            __owRefineKey?: string;
            __owRefineHistory?: { summary: unknown; runs: unknown[] };
            __owRefineHistoryClearCount?: number;
          };
          const defaults: Settings = {
            enabled: false,
            backend: "claude-cli",
            instructions: "",
            timeout_ms: 5000,
            min_words: 4,
            drift_guard: true,
            no_long_dashes: true,
            claude_cli: { model: "sonnet", effort: "low", path: null },
            openai: { base_url: "https://api.openai.com/v1", model: "", effort: null },
          };
          if (!w.__owRefineStored) {
            const seed = w.__owRefineSettings ?? {};
            w.__owRefineStored = {
              ...defaults,
              ...seed,
              claude_cli: { ...defaults.claude_cli, ...(seed.claude_cli ?? {}) },
              openai: { ...defaults.openai, ...(seed.openai ?? {}) },
            };
          }
          const stored = w.__owRefineStored;
          const testSentence =
            "so um this is a test of the refine step uh it should come back with punctuation";
          if (cmd === "refine_get") {
            return {
              settings: stored,
              default_instructions:
                "Fix punctuation and capitalization. Remove filler words and false starts. Fix words that were obviously misheard by the speech recognizer.",
              test_sentence: testSentence,
            };
          }
          if (cmd === "refine_set") {
            const { settings } = (args ?? {}) as { settings: Settings };
            // Mimic core's clean-up: trimmed strings.
            const saved: Settings = {
              ...settings,
              instructions: settings.instructions.trim(),
              claude_cli: {
                ...settings.claude_cli,
                model: settings.claude_cli.model.trim() || "sonnet",
              },
              openai: {
                ...settings.openai,
                base_url: settings.openai.base_url.trim().replace(/\/+$/, ""),
                model: settings.openai.model.trim(),
              },
            };
            w.__owRefineStored = saved;
            w.__owRefineSaves = [...(w.__owRefineSaves ?? []), saved];
            return saved;
          }
          if (cmd === "refine_status") {
            w.__owRefineStatusCount = (w.__owRefineStatusCount ?? 0) + 1;
            const base = {
              backend: stored.backend,
              path: null,
              version: null,
              auth_method: null,
              plan: null,
              has_api_key: Boolean(w.__owRefineKey),
            };
            if (stored.backend === "claude-cli") {
              return {
                ...base,
                state: "ready",
                ready: true,
                message: "Claude Code is signed in with your Claude subscription (max).",
                destination:
                  "Anthropic (Claude), through the Claude Code app signed in on this computer",
                path: "/Users/test/.local/bin/claude",
                version: "2.1.322 (Claude Code)",
                auth_method: "claude.ai",
                plan: "max",
                ...(w.__owRefineStatus ?? {}),
              };
            }
            const configured = Boolean(stored.openai.base_url && stored.openai.model);
            let host = "the endpoint";
            try {
              host = new URL(stored.openai.base_url).host;
            } catch {
              // keep the placeholder
            }
            return {
              ...base,
              state: configured ? "ready" : "not-configured",
              ready: configured,
              message: configured
                ? `Set up for ${stored.openai.model}. Use the connection test to check it.`
                : "No model name is set.",
              destination: `the server at ${host}`,
              ...(w.__owRefineStatus ?? {}),
            };
          }
          if (cmd === "refine_test") {
            w.__owRefineTestCount = (w.__owRefineTestCount ?? 0) + 1;
            return {
              ok: true,
              backend: stored.backend,
              output:
                "This is a test of the Refine step. It should come back with punctuation.",
              message: "",
              elapsed_ms: 1180,
              ...(w.__owRefineTest ?? {}),
            };
          }
          if (cmd === "refine_history") {
            return (
              w.__owRefineHistory ?? {
                summary: {
                  total: 0,
                  asks: 0,
                  asks_answered: 0,
                  ask_median_first_text_ms: null,
                  ask_median_ms: null,
                  refined: 0,
                  skipped: 0,
                  fallbacks: 0,
                  timeouts: 0,
                  unavailable: 0,
                  failed: 0,
                  rejected: 0,
                  instructions: 0,
                  median_ms: null,
                  p90_ms: null,
                  max_ms: null,
                  median_warm_ms: null,
                  median_cold_ms: null,
                  warm_runs: 0,
                  cold_runs: 0,
                  last_problem_at_ms: null,
                  last_problem: "",
                  usage_runs: 0,
                  avg_tokens_in: null,
                  avg_tokens_out: null,
                  avg_cost_usd: null,
                  total_cost_usd: null,
                  plan: null,
                },
                runs: [],
              }
            );
          }
          if (cmd === "refine_history_clear") {
            w.__owRefineHistory = undefined;
            w.__owRefineHistoryClearCount = (w.__owRefineHistoryClearCount ?? 0) + 1;
            return null;
          }
          if (cmd === "refine_set_api_key") {
            const { key } = (args ?? {}) as { key: string };
            w.__owRefineKey = key.trim() || undefined;
            return Boolean(w.__owRefineKey);
          }
        }
        if (cmd === "settings_get_performance") {
          const w = window as unknown as { __owKeepModelsWarm?: boolean };
          return { keep_models_warm: w.__owKeepModelsWarm ?? false };
        }
        if (cmd === "settings_set_keep_models_warm") {
          const { value } = (args ?? {}) as { value: boolean };
          const w = window as unknown as {
            __owKeepModelsWarm?: boolean;
            __owKeepModelsWarmSetCount?: number;
            __owKeepModelsWarmLast?: boolean;
            __owKeepModelsWarmShouldThrow?: boolean;
          };
          if (w.__owKeepModelsWarmShouldThrow) {
            throw new Error("settings_set_keep_models_warm failed");
          }
          w.__owKeepModelsWarm = value;
          w.__owKeepModelsWarmLast = value;
          w.__owKeepModelsWarmSetCount =
            (w.__owKeepModelsWarmSetCount ?? 0) + 1;
          return null;
        }
        if (cmd === "settings_set_pill_follow") {
          const { follow } = (args ?? {}) as { follow: boolean };
          const w = window as unknown as {
            __owPillFollow?: boolean;
            __owPillSetCount?: number;
            __owPillLastFollow?: boolean;
          };
          w.__owPillFollow = follow;
          w.__owPillLastFollow = follow;
          w.__owPillSetCount = (w.__owPillSetCount ?? 0) + 1;
          return null;
        }
        if (cmd === "reposition_pill") {
          // PillOverlay's mount-effect calls this; stub returns OK so the
          // catch path doesn't surface in unrelated specs.
          return null;
        }
        if (cmd === "set_pill_click_through") {
          const { passthrough } = (args ?? {}) as { passthrough: boolean };
          const w = window as unknown as {
            __owPillPassthrough?: boolean;
            __owPillPassthroughCalls?: number;
          };
          w.__owPillPassthrough = passthrough;
          w.__owPillPassthroughCalls = (w.__owPillPassthroughCalls ?? 0) + 1;
          return null;
        }
        if (cmd === "show_main_window") {
          const w = window as unknown as { __owShowMainCount?: number };
          w.__owShowMainCount = (w.__owShowMainCount ?? 0) + 1;
          return null;
        }
        if (cmd === "crashes_list") {
          const w = window as unknown as { __owCrashes?: MockCrashSummary[] };
          return (w.__owCrashes ?? []).slice().sort((a, b) => b.ts_unix_ms - a.ts_unix_ms);
        }
        if (cmd === "crashes_unread_count") {
          const w = window as unknown as { __owCrashes?: MockCrashSummary[] };
          return (w.__owCrashes ?? []).filter((c) => c.unread).length;
        }
        if (cmd === "crashes_get_last_seen_unread") {
          // Default is a high sentinel so the boot-time launch toast
          // (TASK-78.5) is suppressed for tests that don't explicitly
          // exercise it — keeps existing crash-inspector specs free
          // of a constant "dismiss toast" preamble. Tests that DO
          // exercise the toast set this to 0 via
          // `setCrashesLastSeenUnread(page, 0)` before seeding crashes.
          const w = window as unknown as {
            __owCrashesLastSeenUnread?: number;
          };
          return w.__owCrashesLastSeenUnread ?? Number.MAX_SAFE_INTEGER;
        }
        if (cmd === "crashes_mark_seen") {
          const { count } = (args ?? {}) as { count: number };
          const w = window as unknown as {
            __owCrashesLastSeenUnread?: number;
            __owCrashesMarkSeenCalls?: number[];
          };
          w.__owCrashesLastSeenUnread = count;
          w.__owCrashesMarkSeenCalls = [
            ...(w.__owCrashesMarkSeenCalls ?? []),
            count,
          ];
          return null;
        }
        if (cmd === "crashes_read") {
          const { id } = (args ?? {}) as { id: string };
          const w = window as unknown as {
            __owCrashes?: MockCrashSummary[];
            __owCrashFiles?: Record<string, MockCrashFile>;
          };
          const file = w.__owCrashFiles?.[id];
          if (file) return file;
          // Synthesize a minimal CrashFile from the summary if no
          // explicit fixture was set — keeps test setup terse.
          const summary = w.__owCrashes?.find((c) => c.id === id);
          if (!summary) {
            throw new Error(`crash ${id} not found`);
          }
          return {
            schema_version: 1,
            id: summary.id,
            ts_unix_ms: summary.ts_unix_ms,
            app_version: summary.app_version,
            os: summary.os,
            rust_panic: {
              thread_name: "main",
              message: summary.message_truncated,
              location: "test:1:1",
              backtrace: "<test fixture>",
            },
            recording_state: null,
            events: [],
          };
        }
        if (cmd === "crashes_mark_read") {
          const { id } = (args ?? {}) as { id: string };
          const w = window as unknown as {
            __owCrashes?: MockCrashSummary[];
            __owCrashesMarkReadCount?: number;
            __owCrashesMarkReadLast?: string;
          };
          if (w.__owCrashes) {
            for (const c of w.__owCrashes) {
              if (c.id === id) c.unread = false;
            }
          }
          w.__owCrashesMarkReadCount = (w.__owCrashesMarkReadCount ?? 0) + 1;
          w.__owCrashesMarkReadLast = id;
          return null;
        }
        if (cmd === "crashes_delete") {
          const { id } = (args ?? {}) as { id: string };
          const w = window as unknown as {
            __owCrashes?: MockCrashSummary[];
            __owCrashesDeleteCount?: number;
            __owCrashesDeleteLast?: string;
          };
          if (w.__owCrashes) {
            w.__owCrashes = w.__owCrashes.filter((c) => c.id !== id);
          }
          w.__owCrashesDeleteCount = (w.__owCrashesDeleteCount ?? 0) + 1;
          w.__owCrashesDeleteLast = id;
          return null;
        }
        if (cmd === "crashes_delete_all") {
          const w = window as unknown as {
            __owCrashes?: MockCrashSummary[];
            __owCrashesDeleteAllCount?: number;
          };
          w.__owCrashes = [];
          w.__owCrashesDeleteAllCount = (w.__owCrashesDeleteAllCount ?? 0) + 1;
          return null;
        }
        if (cmd === "plugin:opener|open_url") {
          const { url } = (args ?? {}) as { url: string };
          const w = window as unknown as {
            __owOpenerOpenUrlCalls?: string[];
          };
          w.__owOpenerOpenUrlCalls = w.__owOpenerOpenUrlCalls ?? [];
          w.__owOpenerOpenUrlCalls.push(url);
          return null;
        }
        if (cmd === "crashes_open_folder") {
          const w = window as unknown as { __owCrashesOpenFolderCount?: number };
          w.__owCrashesOpenFolderCount =
            (w.__owCrashesOpenFolderCount ?? 0) + 1;
          return null;
        }
        if (cmd === "system_physical_ram_mb") {
          const w = window as unknown as { __owPhysicalRamMb?: number };
          return w.__owPhysicalRamMb ?? 24576;
        }
        if (cmd === "models_storage_path") {
          const w = window as unknown as { __owModelsStoragePath?: string };
          return (
            w.__owModelsStoragePath ??
            "/Users/test/Library/Application Support/com.openwhisper.dev/models"
          );
        }
        if (cmd === "models_open_folder") {
          const w = window as unknown as { __owModelsOpenFolderCount?: number };
          w.__owModelsOpenFolderCount =
            (w.__owModelsOpenFolderCount ?? 0) + 1;
          return null;
        }
        if (cmd === "crashes_debug_trigger_panic") {
          const w = window as unknown as { __owCrashesDebugTriggerCount?: number };
          w.__owCrashesDebugTriggerCount =
            (w.__owCrashesDebugTriggerCount ?? 0) + 1;
          return null;
        }
        // TASK-91 file transcription. Tests drive the job by writing
        // `window.__owFileJob` (a FileJobSnapshot) directly; start /
        // cancel / dialog calls are recorded for assertions.
        if (cmd === "file_transcribe_snapshot") {
          const w = window as unknown as { __owFileJob?: unknown };
          return w.__owFileJob ?? null;
        }
        if (cmd === "file_transcribe_start") {
          const { path } = (args ?? {}) as { path: string };
          const w = window as unknown as {
            __owFileJob?: unknown;
            __owFileStarts?: string[];
            __owFileStartError?: string;
          };
          if (w.__owFileStartError) throw w.__owFileStartError;
          w.__owFileStarts = [...(w.__owFileStarts ?? []), path];
          w.__owFileJob = {
            phase: 1,
            busy: true,
            status: "Reading audio…",
            path,
            file_name: path.split("/").pop() ?? path,
            progress: 0,
            chunks_done: 0,
            chunks_total: 0,
            audio_ms: 0,
            elapsed_ms: 0,
            text: "",
            confidence: 0,
            error: "",
            cancelling: false,
          };
          return null;
        }
        if (cmd === "file_transcribe_cancel") {
          const w = window as unknown as { __owFileCancelCount?: number };
          w.__owFileCancelCount = (w.__owFileCancelCount ?? 0) + 1;
          return true;
        }
        if (cmd === "file_transcribe_clear") return null;
        if (cmd === "file_transcribe_extensions") {
          return ["wav", "mp3", "m4a", "opus", "mp4"];
        }
        if (cmd === "plugin:dialog|open") {
          const w = window as unknown as {
            __owDialogPick?: string | null;
            __owDialogOptions?: unknown;
          };
          w.__owDialogOptions = (args as { options?: unknown } | undefined)?.options;
          return w.__owDialogPick ?? null;
        }
        if (cmd === "plugin:event|listen") {
          const { event, handler } = (args ?? {}) as {
            event: string;
            handler: number;
          };
          if (!handlers.has(event)) handlers.set(event, new Set());
          handlers.get(event)!.add(handler);
          return handler;
        }
        if (cmd === "plugin:event|unlisten") {
          const { event, eventId } = (args ?? {}) as {
            event: string;
            eventId: number;
          };
          handlers.get(event)?.delete(eventId);
          return null;
        }
        if (cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") {
          // Outbound webview emit — fire the same registered listeners
          // so listen() handlers in the React tree see the event. Without
          // this, components calling `emit("ow_navigate", "X")` would
          // hit the default `return null` branch and the listener side
          // would never wake.
          const { event, payload } = (args ?? {}) as {
            event: string;
            payload?: unknown;
          };
          const set = handlers.get(event);
          if (set) {
            for (const id of set) {
              const cb = callbacks.get(id);
              if (cb) cb({ event, payload, id });
            }
          }
          return null;
        }
        return null;
      },
      transformCallback: (fn: (payload: unknown) => void) => {
        const id = nextId++;
        callbacks.set(id, fn);
        return id;
      },
      unregisterCallback: (id: number) => {
        callbacks.delete(id);
      },
      callbacks,
    } as never;

    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
      unregisterListener: (event: string, eventId: number) => {
        handlers.get(event)?.delete(eventId);
        callbacks.delete(eventId);
      },
    } as never;

    window.__owEmit = (event: string, payload: unknown) => {
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

    window.__owListeners = (event: string) => handlers.get(event)?.size ?? 0;
  }, label);
}

// What `modes_changed` carries (mirrors `ModesChanged` in
// src-tauri/src/modes.rs): the stored block, the list of modes with
// the built-in one first, and the name of the mode now selected when
// the change was the user switching mode.
export interface MockModesChanged {
  settings: {
    active: string;
    custom: {
      id: string;
      name: string;
      instructions: string;
      model: string | null;
      effort: string | null;
      backend?: "claude-cli" | "openai-compatible" | null;
    }[];
    app_defaults: { app_id: string; app_name: string; mode: string }[];
  };
  modes: {
    id: string;
    name: string;
    instructions: string;
    model: string | null;
    effort: string | null;
    backend?: "claude-cli" | "openai-compatible" | null;
    builtin: boolean;
    override_backend?: "claude-cli" | "openai-compatible" | null;
    overrides_apply?: boolean;
  }[];
  switched_to: string | null;
}

// Wait until something listens for `modes_changed`: App in the main
// window, PillOverlay in the pill window. Nothing is emitted, since a
// probe payload would itself change what is on screen.
export async function waitForModesListener(page: Page) {
  await page.waitForFunction(() => window.__owListeners("modes_changed") > 0, undefined, {
    timeout: 3000,
  });
}

// Push `modes_changed` as Rust does after a change made outside this
// window: the switch-mode hotkey, the tray menu.
export async function emitModesChanged(page: Page, payload: MockModesChanged): Promise<number> {
  return page.evaluate((value) => window.__owEmit("modes_changed", value), payload);
}

// Push a `behavior_show_in_fullscreen_changed` event at the
// useShowInFullscreen subscriber. Mirrors the Rust setter's emit.
export async function emitShowInFullscreenChanged(
  page: Page,
  enabled: boolean,
): Promise<number> {
  return page.evaluate(
    (value) => window.__owEmit("behavior_show_in_fullscreen_changed", value),
    enabled,
  );
}

// Push a `behavior_pause_audio_changed` event at the usePauseAudio
// subscriber. Mirrors the Rust setter's emit.
export async function emitPauseAudioChanged(
  page: Page,
  enabled: boolean,
): Promise<number> {
  return page.evaluate(
    (value) => window.__owEmit("behavior_pause_audio_changed", value),
    enabled,
  );
}

// Push a `behavior_bt_resume_delay_changed` event at the
// useBtResumeDelay subscriber. Mirrors the Rust setter's emit.
export async function emitBtResumeDelayChanged(
  page: Page,
  delayMs: number,
): Promise<number> {
  return page.evaluate(
    (value) => window.__owEmit("behavior_bt_resume_delay_changed", value),
    delayMs,
  );
}

export interface MockStatsSummary {
  words_today: number;
  words_week: number;
  words_all_time: number;
  seconds_total: number;
}

/// Stash a stats summary on the window so the next `stats_get_summary`
/// invoke (initial mount or post-event refetch) returns this fixture
/// instead of the all-zeros default.
export async function setStatsSummary(
  page: Page,
  summary: MockStatsSummary,
): Promise<void> {
  await page.evaluate((s) => {
    (window as unknown as { __owStatsSummary?: MockStatsSummary }).__owStatsSummary = s;
  }, summary);
}

/// Fire `stats_changed` so the useStatsSummary hook re-fetches.
export async function emitStatsChanged(page: Page): Promise<number> {
  return page.evaluate(() => window.__owEmit("stats_changed", null));
}

/// Stash the WPM fixture so `settings_get_stats` returns it.
export async function setUserWpm(page: Page, wpm: number): Promise<void> {
  await page.evaluate((value) => {
    (window as unknown as { __owUserWpm?: number }).__owUserWpm = value;
  }, wpm);
}

/// Fire `settings_stats_changed` so the useUserWpm hook updates.
export async function emitSettingsStatsChanged(
  page: Page,
  wpm: number,
): Promise<number> {
  return page.evaluate(
    (value) => window.__owEmit("settings_stats_changed", value),
    wpm,
  );
}

export async function emitTick(page: Page, tick: MockTick): Promise<number> {
  const merged = {
    ...TICK_DEFAULTS,
    cancel_would_act: tick.is_recording ?? false,
    ...tick,
  };
  return page.evaluate(
    (payload) => window.__owEmit("dictation_tick", payload),
    merged,
  );
}

// ---------------------------------------------------------------
// Ask (TASK-97).
// ---------------------------------------------------------------

// Mirrors `AskSnapshot` in core/src/ask/mod.rs: the payload of
// `ask_tick` and what `ask_snapshot` returns.
export interface MockAskSnapshot {
  id: number;
  phase:
    | "idle"
    | "listening"
    | "thinking"
    | "answering"
    | "done"
    | "error"
    | "cancelled";
  question: string;
  answer: string;
  notice: string;
  // Mirrors `AskSuggestion` in core; null when there is none.
  suggestion: "web-search" | null;
  error: string;
  backend: string;
  model: string;
  effort: string;
  web_search: boolean;
  searching: boolean;
  search_query: string;
  started_at_ms: number;
  elapsed_ms: number;
  first_text_ms: number | null;
  usage: {
    tokens_in: number;
    tokens_cached: number;
    tokens_out: number;
    cost_usd: number | null;
    plan: unknown;
  } | null;
  version: number;
}

export const ASK_SNAPSHOT_DEFAULTS: MockAskSnapshot = {
  id: 0,
  phase: "idle",
  question: "",
  answer: "",
  notice: "",
  suggestion: null,
  error: "",
  backend: "claude-cli",
  model: "sonnet",
  effort: "low",
  web_search: false,
  searching: false,
  search_query: "",
  started_at_ms: 0,
  elapsed_ms: 0,
  first_text_ms: null,
  usage: null,
  version: 0,
};

// Mirrors `AskSettings` in core/src/settings/ask.rs: the payload of
// `ask_changed`.
export interface MockAskSettings {
  enabled: boolean;
  web_search: boolean;
  instructions: string;
  model: string | null;
  effort: string | null;
  timeout_ms: number;
  context: {
    date_time: boolean;
    computer: boolean;
    front_app: boolean;
    locale: boolean;
    place: string;
  };
}

// Push `ask_tick` as the Rust emitter does, and make the snapshot the
// one the shim's `ask_snapshot`, `ask_cancel`, `ask_dismiss`,
// `ask_insert` and `ask_copy` answer from. `version` goes up by
// itself unless the test sets it. Returns how many listeners got it.
export async function emitAskTick(
  page: Page,
  snapshot: Partial<MockAskSnapshot> & Pick<MockAskSnapshot, "phase">,
): Promise<number> {
  return page.evaluate(
    ({ defaults, given }) => {
      const w = window as unknown as { __owAskSnapshot?: { version: number } };
      const next = {
        ...defaults,
        id: 1,
        version: (w.__owAskSnapshot?.version ?? 0) + 1,
        ...given,
      };
      w.__owAskSnapshot = next;
      return window.__owEmit("ask_tick", next);
    },
    { defaults: ASK_SNAPSHOT_DEFAULTS, given: snapshot },
  );
}

// Wait until something listens for `ask_tick`: App (the history
// sync) in the main window, AnswerPanel in the answer window.
// Nothing is emitted.
export async function waitForAskTickListener(page: Page) {
  await page.waitForFunction(() => window.__owListeners("ask_tick") > 0, undefined, {
    timeout: 3000,
  });
}

// Push `ask_changed` as Rust does after the Ask settings were saved
// (the Ask pane, or the block edited by the headless CLI). The block
// is stored first, as in Rust, so an `ask_get` that follows the
// event answers with it.
export async function emitAskChanged(
  page: Page,
  settings: Partial<MockAskSettings>,
): Promise<number> {
  return page.evaluate((value) => {
    const next = {
      enabled: false,
      web_search: false,
      instructions: "",
      model: null,
      effort: null,
      timeout_ms: 60000,
      context: { date_time: true, computer: true, front_app: true, locale: true, place: "" },
      ...value,
    };
    (window as unknown as { __owAskStored?: unknown }).__owAskStored = next;
    return window.__owEmit("ask_changed", structuredClone(next));
  }, settings);
}

// Wait for useHotkeyStatus's listener to attach. Probe by emitting an ok=true
// status and looking for delivered > 0; harmless because that's the default.
export async function waitForHotkeyStatusListener(page: Page) {
  await page.waitForFunction(
    () => window.__owEmit("hotkey_status", { ok: true, error: "" }) > 0,
    { timeout: 3000 },
  );
}

// Wait for usePermissionsStatus's listener to attach. Probe by emitting an
// authorized status — same harmless-default trick as the hotkey probe.
export async function waitForPermissionsStatusListener(page: Page) {
  await page.waitForFunction(
    () =>
      window.__owEmit("permissions_status", {
        mic_ok: true,
        mic_state: "authorized",
        error: "",
      }) > 0,
    { timeout: 3000 },
  );
}

// Wait for the dictation hook's listener to have been registered before the
// first tick lands. Otherwise the emit returns 0 and React state stays at INITIAL.
export async function waitForTickListener(page: Page) {
  await page.waitForFunction(
    () => {
      const ok = window.__owEmit("dictation_tick", {
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
      });
      return ok > 0;
    },
    { timeout: 3000 },
  );
}

// Push a fresh `audio_device_state` snapshot at the AudioPane subscriber.
// Mirrors `AudioDeviceState` in apps/tauri/src-tauri/src/lib.rs. Defaults
// follow the shim's seeded device fixture so the helper composes cleanly
// with `__owAudioDevice` overrides set earlier in a test.
export interface MockAudioDevice {
  id: string;
  label: string;
  is_default: boolean;
}

export interface MockAudioDeviceState {
  devices?: MockAudioDevice[];
  selected_id?: string | null;
  selected_present?: boolean;
  default_label?: string | null;
}

const DEFAULT_DEVICE_FIXTURE: MockAudioDevice[] = [
  { id: "default-mic", label: "MacBook Pro Microphone", is_default: true },
  { id: "airpods-pro", label: "AirPods Pro", is_default: false },
];

export async function emitDeviceState(
  page: Page,
  state: MockAudioDeviceState = {},
): Promise<number> {
  return page.evaluate(
    ({ partial, fallback }) => {
      const w = window as unknown as {
        __owAudioDevices?: Array<{
          id: string;
          label: string;
          is_default: boolean;
        }>;
        __owAudioDevice?: string | null;
      };
      const devices = partial.devices ?? w.__owAudioDevices ?? fallback;
      const selectedId = partial.selected_id ?? w.__owAudioDevice ?? null;
      const defaultLabel =
        partial.default_label ??
        devices.find((d) => d.is_default)?.label ??
        null;
      const selectedPresent =
        partial.selected_present ??
        (selectedId === null || devices.some((d) => d.id === selectedId));
      return window.__owEmit("audio_device_state", {
        devices,
        selected_id: selectedId,
        selected_present: selectedPresent,
        default_label: defaultLabel,
      });
    },
    { partial: state, fallback: DEFAULT_DEVICE_FIXTURE },
  );
}

// Wait for AudioPane's `audio_device_state` listener to attach. Probe by
// emitting a benign snapshot and asserting at least one delivery — same
// trick as `waitForTickListener`.
export async function waitForDeviceStateListener(page: Page) {
  await page.waitForFunction(
    (fallback) => {
      const w = window as unknown as {
        __owAudioDevices?: Array<{
          id: string;
          label: string;
          is_default: boolean;
        }>;
        __owAudioDevice?: string | null;
      };
      const devices = w.__owAudioDevices ?? fallback;
      const selectedId = w.__owAudioDevice ?? null;
      const defaultLabel = devices.find((d) => d.is_default)?.label ?? null;
      const selectedPresent =
        selectedId === null || devices.some((d) => d.id === selectedId);
      return (
        window.__owEmit("audio_device_state", {
          devices,
          selected_id: selectedId,
          selected_present: selectedPresent,
          default_label: defaultLabel,
        }) > 0
      );
    },
    DEFAULT_DEVICE_FIXTURE,
    { timeout: 3000 },
  );
}

// ---------------------------------------------------------------
// Diagnostics — memory telemetry shim helpers (TASK-62.10).
// ---------------------------------------------------------------

export type MockLifecycleState =
  | "Unloaded"
  | "Loading"
  | "Loaded"
  | "Active"
  | "Releasing";

export interface MockProcessMemory {
  rss_bytes: number;
  peak_rss_bytes: number;
  timestamp_unix_ms: number;
}

export interface MockSystemMemory {
  total_bytes: number;
  used_bytes: number;
  available_bytes: number;
  swap_total_bytes: number;
  swap_used_bytes: number;
}

export interface MockModelMemoryRow {
  label: string;
  state: MockLifecycleState;
  estimated_rss_bytes: number;
  /// Static weight footprint reported by the loader. Defaults to 0
  /// in fixtures that don't care; tests for ANE-resident memory
  /// must set both `claimed_bytes` and `in_process: false`.
  claimed_bytes?: number;
  /// `true` (default) when the claim is already inside RSS; `false`
  /// for Mac ANE-resident weights.
  in_process?: boolean;
}

export interface MockMemoryStats {
  system: MockSystemMemory;
  process: MockProcessMemory;
  models: MockModelMemoryRow[];
}

/// Stash a MemoryStats fixture on the window so the next
/// `telemetry_get_memory` invoke returns this snapshot. Called both
/// before mount (initial paint) and between polls (1 Hz refresh
/// observation).
export async function setMemoryStats(
  page: Page,
  stats: MockMemoryStats,
): Promise<void> {
  await page.evaluate((s) => {
    (window as unknown as { __owMemoryStats?: MockMemoryStats }).__owMemoryStats = s;
  }, stats);
}

/// Fire `model-state-changed` so the diagnostics pane refetches without
/// waiting for the 1 Hz poll. Mirrors the Rust setup in
/// `apps/tauri/src-tauri/src/lib.rs` registering an `on_state_change`
/// callback that emits `{ label, state }`.
export async function emitModelStateChanged(
  page: Page,
  payload: { label: string; state: MockLifecycleState },
): Promise<number> {
  return page.evaluate(
    (value) => window.__owEmit("model-state-changed", value),
    payload,
  );
}

// ---------------------------------------------------------------
// Crash inspector — TASK-78.3 / 78.7 shim helpers.
// ---------------------------------------------------------------

export interface MockCrashSummary {
  id: string;
  ts_unix_ms: number;
  app_version: string;
  os: string;
  message_truncated: string;
  unread: boolean;
  uploaded_at: number | null;
}

export interface MockCrashFile {
  schema_version: number;
  id: string;
  ts_unix_ms: number;
  app_version: string;
  os: string;
  rust_panic: {
    thread_name: string;
    message: string;
    location: string;
    backtrace: string;
  };
  recording_state:
    | {
        status_message_at_crash: string;
        duration_ms: number;
        samples_captured: number;
        model_kind: string | null;
        device_id_hash: string | null;
      }
    | null;
  events: Array<{ ts_unix_ms: number; kind: string; data: unknown }>;
}

/// Stash a list of crash summaries. Subsequent `crashes_list` /
/// `crashes_unread_count` invokes mirror this fixture; mark-read /
/// delete mutate it in place so a Playwright spec can observe the
/// state transitions through the same shim.
export async function setCrashes(
  page: Page,
  crashes: MockCrashSummary[],
): Promise<void> {
  await page.evaluate((rows) => {
    (window as unknown as { __owCrashes?: MockCrashSummary[] }).__owCrashes =
      rows;
  }, crashes);
}

/// Override the `last_seen_unread_count` the shim returns from
/// `crashes_get_last_seen_unread`. Default is a high sentinel so the
/// boot-time launch toast is suppressed in tests that don't explicitly
/// exercise it. Set to 0 (or any value lower than the seeded unread
/// count) to surface the toast in a test.
export async function setCrashesLastSeenUnread(
  page: Page,
  count: number,
): Promise<void> {
  await page.evaluate((value) => {
    (window as unknown as { __owCrashesLastSeenUnread?: number })
      .__owCrashesLastSeenUnread = value;
  }, count);
}

/// Optionally seed full crash files by id, so `crashes_read` returns a
/// hand-picked fixture (otherwise the shim synthesises a minimal one
/// from the matching summary).
export async function setCrashFiles(
  page: Page,
  files: Record<string, MockCrashFile>,
): Promise<void> {
  await page.evaluate((map) => {
    (window as unknown as { __owCrashFiles?: Record<string, MockCrashFile> })
      .__owCrashFiles = map;
  }, files);
}

/// Override what `models_storage_path` returns. Otherwise the shim
/// returns a Mac-shaped placeholder string; tests that need to assert
/// on a specific path or platform-shaped value can pin it explicitly.
export async function setModelsStoragePath(
  page: Page,
  path: string,
): Promise<void> {
  await page.evaluate((p) => {
    (window as unknown as { __owModelsStoragePath?: string })
      .__owModelsStoragePath = p;
  }, path);
}

/// Override what `system_physical_ram_mb` returns. Default 24 576 MB
/// (24 GB) matches the design mock; tests that need a different
/// machine size set it explicitly here.
export async function setPhysicalRamMb(
  page: Page,
  mb: number,
): Promise<void> {
  await page.evaluate((value) => {
    (window as unknown as { __owPhysicalRamMb?: number }).__owPhysicalRamMb =
      value;
  }, mb);
}

/// Wait for the diagnostics pane to attach its `model-state-changed`
/// listener (via `useMemoryStats`). Probes by emitting an idempotent
/// payload — the listener triggers a refetch but doesn't change state.
export async function waitForModelStateChangedListener(page: Page) {
  await page.waitForFunction(
    () =>
      window.__owEmit("model-state-changed", {
        label: "__probe",
        state: "Unloaded",
      }) > 0,
    { timeout: 3000 },
  );
}

export const test = base.extend({
  page: async ({ page }, use) => {
    await installTauriShim(page);
    await use(page);
  },
});

// Same shim, but boots the SPA as the "pill" window — main.tsx's
// React.lazy switch then loads PillOverlay instead of App.
export const pillTest = base.extend({
  page: async ({ page }, use) => {
    await installTauriShim(page, "pill");
    await use(page);
  },
});

// Same shim, booted as the "answer" window: main.tsx then loads
// AnswerPanel, the answer window of Ask.
export const answerTest = base.extend({
  page: async ({ page }, use) => {
    await installTauriShim(page, "answer");
    await use(page);
  },
});

export { expect };
