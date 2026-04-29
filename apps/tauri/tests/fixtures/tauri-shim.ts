import { test as base, expect, type Page } from "@playwright/test";

// Mirrors the DictationTick payload that core/src-tauri emits at 20 Hz.
export interface MockTick {
  phase: number;
  status: "idle" | "recording" | "transcribing";
  status_message?: string;
  transcript?: string;
  confidence?: number;
  sample_count?: number;
  elapsed_ms?: number;
  error_message?: string;
  can_toggle?: boolean;
  is_recording?: boolean;
  level?: number;
  download_bytes_done?: number;
  download_bytes_total?: number;
}

export const TICK_DEFAULTS: Required<Omit<MockTick, "phase" | "status">> = {
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
};

declare global {
  interface Window {
    __owEmit: (event: string, payload: unknown) => number;
    __TAURI_INTERNALS__: unknown;
    __TAURI_EVENT_PLUGIN_INTERNALS__: unknown;
  }
}

// Install the Tauri 2 internals stub before the SPA boots. Records emitted
// `dictation_tick` events into a queue the test can replay.
async function installTauriShim(page: Page, label: "main" = "main") {
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
        if (cmd === "core_version") return "0.1.0-test";
        if (cmd === "plugin:app|name") return "OpenWhisper Dev";
        if (cmd === "dictation_toggle") return null;
        if (cmd === "dictation_cancel") return false;
        if (cmd === "hotkey_status_current") return null;
        if (cmd === "permissions_status_current") return null;
        if (cmd === "hotkey_retry") {
          (window as unknown as { __owHotkeyRetryCount?: number }).__owHotkeyRetryCount =
            ((window as unknown as { __owHotkeyRetryCount?: number }).__owHotkeyRetryCount ?? 0) + 1;
          return null;
        }
        if (cmd === "open_settings_window") {
          (window as unknown as { __owOpenSettingsCount?: number }).__owOpenSettingsCount =
            ((window as unknown as { __owOpenSettingsCount?: number }).__owOpenSettingsCount ?? 0) + 1;
          return null;
        }
        if (cmd === "settings_get_hotkeys") {
          const stored = (window as unknown as { __owHotkeys?: unknown }).__owHotkeys;
          return (
            stored ?? {
              toggle: { kind: "modifier-tap", code: "RightCommand", mods: [] },
              cancel: { kind: "chord", code: "Escape", mods: [] },
            }
          );
        }
        if (cmd === "settings_set_hotkey") {
          const { target, config } = (args ?? {}) as {
            target: "toggle" | "cancel";
            config: unknown;
          };
          const w = window as unknown as { __owHotkeys?: Record<string, unknown> };
          w.__owHotkeys = w.__owHotkeys ?? {
            toggle: { kind: "modifier-tap", code: "RightCommand", mods: [] },
            cancel: { kind: "chord", code: "Escape", mods: [] },
          };
          w.__owHotkeys[target] = config;
          (window as unknown as { __owHotkeySetCount?: number }).__owHotkeySetCount =
            ((window as unknown as { __owHotkeySetCount?: number }).__owHotkeySetCount ?? 0) + 1;
          (window as unknown as { __owHotkeyLastTarget?: string }).__owHotkeyLastTarget = target;
          return null;
        }
        if (cmd === "settings_reset_hotkey") {
          const { target } = (args ?? {}) as { target: "toggle" | "cancel" };
          const def =
            target === "toggle"
              ? { kind: "modifier-tap", code: "RightCommand", mods: [] }
              : { kind: "chord", code: "Escape", mods: [] };
          const w = window as unknown as { __owHotkeys?: Record<string, unknown> };
          w.__owHotkeys = w.__owHotkeys ?? {
            toggle: { kind: "modifier-tap", code: "RightCommand", mods: [] },
            cancel: { kind: "chord", code: "Escape", mods: [] },
          };
          w.__owHotkeys[target] = def;
          (window as unknown as { __owHotkeyResetCount?: number }).__owHotkeyResetCount =
            ((window as unknown as { __owHotkeyResetCount?: number }).__owHotkeyResetCount ?? 0) + 1;
          (window as unknown as { __owHotkeyLastTarget?: string }).__owHotkeyLastTarget = target;
          return def;
        }
        if (cmd === "settings_capture_hotkey_start") {
          const { target } = (args ?? {}) as { target?: "toggle" | "cancel" };
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
        if (cmd === "audio_list_devices") {
          const w = window as unknown as { __owAudioDevices?: unknown };
          return (
            w.__owAudioDevices ?? [
              { name: "MacBook Pro Microphone", is_default: true },
              { name: "AirPods Pro", is_default: false },
            ]
          );
        }
        if (cmd === "audio_get_device") {
          const w = window as unknown as { __owAudioDevice?: string | null };
          return w.__owAudioDevice ?? null;
        }
        if (cmd === "audio_get_device_state") {
          // Source state from the same window stash the legacy commands
          // use so existing fixtures (e.g. setting __owAudioDevice before
          // opening the pane) keep working without bespoke setup.
          const w = window as unknown as {
            __owAudioDevices?: Array<{ name: string; is_default: boolean }>;
            __owAudioDevice?: string | null;
            __owAudioDefaultName?: string | null;
            __owAudioSelectedPresent?: boolean;
          };
          const devices = w.__owAudioDevices ?? [
            { name: "MacBook Pro Microphone", is_default: true },
            { name: "AirPods Pro", is_default: false },
          ];
          const selectedName = w.__owAudioDevice ?? null;
          const defaultName =
            w.__owAudioDefaultName ??
            devices.find((d) => d.is_default)?.name ??
            null;
          // Honour an explicit override (used by disconnect tests). Otherwise
          // derive presence from whether the saved name is enumerable.
          const selectedPresent =
            w.__owAudioSelectedPresent ??
            (selectedName === null ||
              devices.some((d) => d.name === selectedName));
          return {
            devices,
            selected_name: selectedName,
            selected_present: selectedPresent,
            default_name: defaultName,
          };
        }
        if (cmd === "audio_set_device") {
          const { name } = (args ?? {}) as { name: string | null };
          const w = window as unknown as {
            __owAudioDevice?: string | null;
            __owAudioSetCount?: number;
            __owAudioLastSet?: string | null;
          };
          w.__owAudioDevice = name;
          w.__owAudioLastSet = name;
          w.__owAudioSetCount = (w.__owAudioSetCount ?? 0) + 1;
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
  }, label);
}

export async function emitTick(page: Page, tick: MockTick): Promise<number> {
  const merged = { ...TICK_DEFAULTS, ...tick };
  return page.evaluate(
    (payload) => window.__owEmit("dictation_tick", payload),
    merged,
  );
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
export interface MockAudioDeviceState {
  devices?: Array<{ name: string; is_default: boolean }>;
  selected_name?: string | null;
  selected_present?: boolean;
  default_name?: string | null;
}

export async function emitDeviceState(
  page: Page,
  state: MockAudioDeviceState = {},
): Promise<number> {
  return page.evaluate((partial) => {
    const w = window as unknown as {
      __owAudioDevices?: Array<{ name: string; is_default: boolean }>;
      __owAudioDevice?: string | null;
    };
    const devices = partial.devices ??
      w.__owAudioDevices ?? [
        { name: "MacBook Pro Microphone", is_default: true },
        { name: "AirPods Pro", is_default: false },
      ];
    const selectedName = partial.selected_name ?? w.__owAudioDevice ?? null;
    const defaultName =
      partial.default_name ??
      devices.find((d) => d.is_default)?.name ??
      null;
    const selectedPresent =
      partial.selected_present ??
      (selectedName === null ||
        devices.some((d) => d.name === selectedName));
    return window.__owEmit("audio_device_state", {
      devices,
      selected_name: selectedName,
      selected_present: selectedPresent,
      default_name: defaultName,
    });
  }, state);
}

// Wait for AudioPane's `audio_device_state` listener to attach. Probe by
// emitting a benign snapshot and asserting at least one delivery — same
// trick as `waitForTickListener`.
export async function waitForDeviceStateListener(page: Page) {
  await page.waitForFunction(
    () => {
      const w = window as unknown as {
        __owAudioDevices?: Array<{ name: string; is_default: boolean }>;
        __owAudioDevice?: string | null;
      };
      const devices = w.__owAudioDevices ?? [
        { name: "MacBook Pro Microphone", is_default: true },
        { name: "AirPods Pro", is_default: false },
      ];
      const selectedName = w.__owAudioDevice ?? null;
      const defaultName = devices.find((d) => d.is_default)?.name ?? null;
      const selectedPresent =
        selectedName === null ||
        devices.some((d) => d.name === selectedName);
      return (
        window.__owEmit("audio_device_state", {
          devices,
          selected_name: selectedName,
          selected_present: selectedPresent,
          default_name: defaultName,
        }) > 0
      );
    },
    { timeout: 3000 },
  );
}

export const test = base.extend({
  page: async ({ page }, use) => {
    await installTauriShim(page);
    await use(page);
  },
});

export { expect };
