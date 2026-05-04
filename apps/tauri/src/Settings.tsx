import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  cancelJsCapture,
  startJsCapture,
  type HotkeyCapturedPayload,
  type HotkeyConfig,
  type HotkeySettings,
  type HotkeyTarget,
} from "./lib/use-global-hotkey";
import { configToChipKeys } from "./lib/hotkey-format";
import type { SettingsPaneId } from "./lib/settings-panes";
import { LevelMeter } from "./components/level-meter";
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "./components/ui/select";
import {
  Field,
  FieldContent,
  FieldDescription,
  FieldGroup,
  FieldLabel,
} from "./components/ui/field";
import { Separator } from "./components/ui/separator";
import { Slider } from "./components/ui/slider";
import { Switch } from "./components/ui/switch";
import { useBtResumeDelay } from "./lib/use-bt-resume-delay";
import { useDictation } from "./lib/use-dictation";
import { usePauseAudio } from "./lib/use-pause-audio";
import { GeneralPane } from "./components/general-pane";
import "./Settings.css";

// Step granularity (ms) for the BT resume delay slider. 500 ms gives
// 21 stops over 0–10 s — fine enough to land on user-perceptible
// differences without overwhelming the slider with sub-perceptual
// detents.
const BT_RESUME_DELAY_STEP_MS = 500;
const BT_RESUME_DELAY_MAX_MS = 10_000;

// Live preview meter geometry — same 32-bar count and bar height as the
// main-window meter card so the visual reads identically across surfaces.
const PREVIEW_BAR_COUNT = 32;
// 20 ticks × 50 ms = 1 s rolling window. The KV "peak" row reports the
// loudest sample seen in that window, smoothing single-frame transients
// that would otherwise make the readout flicker.
const PEAK_WINDOW_TICKS = 20;
const PREVIEW_FLOOR_DB = -55;
const PREVIEW_SAMPLE_RATE_HZ = 16_000;

// SettingsShell renders pane content only. The pane chooser lives in the
// outer SidebarNav (route=settings mode), which owns active-pane state via
// App.tsx — the back arrow on the titlebar then restores the route-level
// sidebar (Home/Settings/Diagnostics) by flipping route back to "home".
interface SettingsShellProps {
  active: SettingsPaneId;
}

export function SettingsShell({ active }: SettingsShellProps) {
  return (
    <div className="ow-settings">
      <div
        className="ow-settings__pane"
        role="tabpanel"
        aria-labelledby={active}
      >
        {active === "general" && <GeneralPane />}
        {active === "audio" && <AudioPane />}
        {active === "models" && <PaneStub title="Models" />}
        {active === "shortcuts" && <ShortcutsPane />}
      </div>
    </div>
  );
}

function PaneStub({ title }: { title: string }) {
  return (
    <div className="ow-settings__pane-stub">
      <h2>{title}</h2>
      <p>Coming soon.</p>
    </div>
  );
}

interface AudioDevice {
  // Stable cpal device id ("host:device_id"). Persisted by the picker; the
  // <option value="..."> uses this so two devices with the same friendly
  // label (Windows, where every capture endpoint is "Microphone (...)")
  // round-trip distinctly.
  id: string;
  // Discord/Windows-Sound-style label — Windows FriendlyName
  // ("Microphone (SteelSeries Arctis 5 Chat)") on Windows, cpal description
  // name elsewhere.
  label: string;
  is_default: boolean;
}

// Snapshot the Rust shell pushes via `audio_device_state`. Mirrors
// `AudioDeviceState` in apps/tauri/src-tauri/src/lib.rs. `selected_present`
// flips false when the persisted device is no longer enumerable (unplugged,
// renamed) — capture transparently uses the host default in that case, but
// the picker keeps the saved id selected so a re-plug auto-rebinds.
// `default_label` is the FriendlyName of whatever device the host default
// currently resolves to, used to render Discord's "System default
// (<device label>)" UX without forcing the UI to scan the device list.
interface AudioDeviceState {
  devices: AudioDevice[];
  selected_id: string | null;
  selected_present: boolean;
  default_label: string | null;
}

// Discord-style label for the host-default option. We mirror Discord's
// platform-specific phrasing so the dropdown reads like the OS audio
// settings the user already knows: "Windows Default" on Windows,
// "macOS Default" on Mac, plain "Default" elsewhere. Resolves at module
// load via `navigator.platform`, matching the platform-detection pattern
// in `lib/use-global-hotkey.ts` rather than introducing a Tauri OS plugin
// just for one string.
// Mac uses adaptive sample-rate polling for BT switchback (see
// `media_control/mac.rs`), so the user-configurable BT resume delay
// has no effect there. Hide the slider on Mac entirely; it's a
// Windows-only fallback for the Windows lack of a switchback event.
const SHOW_BT_RESUME_DELAY = (() => {
  if (typeof navigator === "undefined") return true;
  return !/mac/i.test(navigator.platform);
})();

const DEFAULT_DEVICE_PREFIX = (() => {
  if (typeof navigator === "undefined") return "Default";
  if (/win/i.test(navigator.platform)) return "Windows Default";
  if (/mac/i.test(navigator.platform)) return "macOS Default";
  return "Default";
})();

// Audio pane — device picker + opt-in test meter (TASK-53).
//
// Lifecycle: the meter does NOT start automatically — the user clicks
// "Test microphone" to open a meter-only stream in core
// (`preview=true` in `core::audio::begin_capture`, which suppresses
// sample buffering). Clicking again stops it. Unmounting always stops
// any in-flight test, even if the user navigated away mid-test.
//
// Why opt-in: the auto-start version meant the app was always listening
// while the Audio pane was open, which surprised users. The button gives
// an explicit on/off so people don't feel tracked.
//
// Mutual exclusion with recording is enforced on the Rust side: starting
// a recording auto-stops the test stream, and `audio_preview_start`
// refuses if a recording is already in flight (AC #3 in task-53).
function AudioPane() {
  const [devices, setDevices] = useState<AudioDevice[]>([]);
  // Empty string maps to the "System default" option in the <select> AND to
  // `None` in core's selector (host default at begin_capture time).
  // Otherwise this is the cpal device id of the active selection.
  const [selected, setSelected] = useState<string>("");
  // Tracks whether the currently selected device is still enumerable. False
  // means the saved id doesn't match any present input device — capture
  // transparently falls back to the host default. We keep the saved id
  // in `selected` so a re-plug rebinds the user's choice without re-pick.
  const [selectedPresent, setSelectedPresent] = useState<boolean>(true);
  // Friendly name of whatever device the host default currently resolves
  // to. Powers the Discord-style "System default (<label>)" row.
  const [defaultLabel, setDefaultLabel] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [testing, setTesting] = useState(false);
  const [busy, setBusy] = useState(false);
  const dictation = useDictation();
  // Audio behavior settings — moved out of GeneralPane to keep
  // related concerns together. The pause toggle gates the slider:
  // when off, the entire ducking path short-circuits in Rust and
  // the slider value has no effect, so disabling matches the
  // user-visible behavior.
  const { enabled: pauseAudio, setEnabled: setPauseAudio } = usePauseAudio();
  const { delayMs: btResumeDelayMs, setDelayMs: setBtResumeDelayMs } =
    useBtResumeDelay();

  // Rolling 32-bar buffer, fed from `dictation.level` at the 20 Hz tick
  // emit cadence. Matching the main-window meter geometry so users build a
  // single mental model for "what the meter looks like when it's working".
  const [levels, setLevels] = useState<number[]>(() =>
    new Array(PREVIEW_BAR_COUNT).fill(0),
  );
  // Rolling 1 s peak — written through a ref so we don't re-render purely
  // because of bookkeeping. The displayed value lives in state and only
  // updates when the rounded dB readout actually changes.
  const peakWindowRef = useRef<number[]>([]);
  const [peakDb, setPeakDb] = useState<number | null>(null);

  // Mount: pull initial state synchronously, then subscribe to live updates.
  // The Rust shell emits `audio_device_state` from the dictation tick loop
  // every ~2 s, but only when the snapshot hash changes — so an unplugged
  // mic surfaces in the picker within 2 s, and a steady-state pane doesn't
  // churn on every tick.
  useEffect(() => {
    let alive = true;
    let unlisten: UnlistenFn | null = null;
    const apply = (state: AudioDeviceState) => {
      setDevices(state.devices);
      setSelected(state.selected_id ?? "");
      setSelectedPresent(state.selected_present);
      setDefaultLabel(state.default_label);
    };
    void (async () => {
      try {
        const initial = await invoke<AudioDeviceState>("audio_get_device_state");
        if (!alive) return;
        apply(initial);
      } catch (e) {
        if (alive) setError(String(e));
      }
      try {
        const off = await listen<AudioDeviceState>("audio_device_state", (evt) => {
          if (!alive) return;
          apply(evt.payload);
        });
        if (!alive) {
          off();
          return;
        }
        unlisten = off;
      } catch (e) {
        if (alive) setError(String(e));
      }
    })();
    return () => {
      alive = false;
      if (unlisten) unlisten();
      // Always tear down the preview stream on unmount, even if the user
      // navigated away mid-test. audio_preview_stop is a no-op when
      // nothing is running.
      void invoke("audio_preview_stop").catch(() => {});
    };
  }, []);

  // Slide the rolling buffers on every dictation tick. The dependency is
  // `dictation.levels` (a fresh array reference each tick), NOT
  // `dictation.level` — when the level stays at the same primitive value
  // (e.g. a virtual mic that produces zero callbacks, leaving level
  // pinned at 0 forever), useEffect would skip its rerun and the bar
  // buffer would freeze on the last sampled values. Using the array
  // reference guarantees the effect fires on every emit, draining the
  // bars to baseline whenever new audio stops arriving.
  useEffect(() => {
    const lvl = testing ? dictation.level : 0;
    setLevels((prev) => {
      const next = prev.slice(1);
      next.push(lvl);
      return next;
    });
    if (!testing) return;
    const w = peakWindowRef.current;
    w.push(lvl);
    if (w.length > PEAK_WINDOW_TICKS) w.shift();
    let max = 0;
    for (const v of w) if (v > max) max = v;
    if (max <= 0) {
      setPeakDb((prev) => (prev === null ? prev : null));
      return;
    }
    const db = 20 * Math.log10(Math.max(max, 1e-6));
    // Round to one decimal so the readout doesn't churn on every frame.
    const rounded = Math.round(db * 10) / 10;
    setPeakDb((prev) => (prev === rounded ? prev : rounded));
  }, [dictation.levels, dictation.level, testing]);

  const startTest = useCallback(async () => {
    setError(null);
    setBusy(true);
    try {
      await invoke("audio_preview_start");
      peakWindowRef.current = [];
      setPeakDb(null);
      setTesting(true);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }, []);

  const stopTest = useCallback(async () => {
    setBusy(true);
    try {
      await invoke("audio_preview_stop");
    } catch (err) {
      setError(String(err));
    } finally {
      setTesting(false);
      setPeakDb(null);
      setBusy(false);
    }
  }, []);

  // Switching device: persist immediately. If we're currently testing,
  // bounce the stream so the meter jumps cleanly to the new mic instead
  // of "sticking" on the prior level. If not testing, just persist —
  // the next test (or recording) will pick up the new device.
  //
  // Why snap-to-zero up front: switching to a slow-to-activate device
  // (Continuity Camera mic / iPhone microphone) holds cpal's
  // `begin_capture` for several seconds while CoreAudio negotiates the
  // route. During that window no audio callbacks fire, so the prior
  // mic's bar pattern would otherwise sit frozen at the right edge of
  // the meter for ~1.6 s before sliding out at the 20 Hz tick rate.
  // Resetting the buffer + peak window synchronously on click gives
  // immediate "I heard you, retuning…" feedback instead.
  const onChange = useCallback(
    async (id: string) => {
      setSelected(id);
      setError(null);
      setBusy(true);
      setLevels(new Array(PREVIEW_BAR_COUNT).fill(0));
      peakWindowRef.current = [];
      setPeakDb(null);
      try {
        const wasTesting = testing;
        if (wasTesting) {
          await invoke("audio_preview_stop");
          setTesting(false);
        }
        await invoke("audio_set_device", { id: id === "" ? null : id });
        if (wasTesting) {
          await invoke("audio_preview_start");
          setTesting(true);
        }
      } catch (err) {
        setError(String(err));
      } finally {
        setBusy(false);
      }
    },
    [testing],
  );

  // Status flag for the LevelMeter: while testing, treat the meter as
  // "recording" so it picks up the recording-color tokens. Idle styling
  // would lock the bars to minHeight.
  const meterStatus = testing ? "recording" : "idle";

  return (
    <div className="ow-audio">
      <header className="ow-audio__header">
        <h2>Audio</h2>
        <p>
          Pick a microphone, then press Test to confirm OpenWhisper is
          picking up your voice. Input gain, suppression, AGC, channels,
          and sample rate aren't configurable yet — captures always run
          at the device's native rate and resample to 16 kHz mono
          internally.
        </p>
      </header>

      <section className="ow-audio__row">
        <div className="ow-audio__row-label">
          <div className="ow-audio__row-title">Microphone</div>
          <div className="ow-audio__row-hint">
            {dictation.isRecording
              ? "Locked while a recording is in flight — stop dictation to change device."
              : "Choose the device OpenWhisper listens on."}
          </div>
        </div>
        <div className="ow-audio__row-control">
          {/*
            Custom Select (shadcn / BaseUI) instead of a native <select>:
            - The trigger collapses long labels with ellipsis (so the row
              stays compact), but the dropdown popup is allowed to grow
              wider than the trigger via `alignItemWithTrigger={false}` so
              "Windows Default (Microphone (Steam Streaming Microphone))"
              and friends render in full.
            - The default option uses Discord's two-line layout: a
              "Windows Default" / "macOS Default" / "Default" heading with
              the resolved device label underneath in the muted token. Per-
              device options stay one-line.

            BaseUI requires an `items` prop on the root for trigger-side
            value lookup (Selects without it can render an empty trigger
            on hydrate, see rules/base-vs-radix.md). Each entry is a flat
            { value, label } pair — the dropdown rendering is independent
            and stays inside <SelectItem>.

            Effective value: when the saved device isn't enumerable,
            render the picker as the platform default rather than a
            Frankenstein "device (disconnected)" entry. The saved
            preference (`selected`) is kept in core, so a re-plug auto-
            rebinds the picker on the next snapshot. If the user actively
            picks the default option while disconnected, onChange writes
            null → clearing the saved preference (intent override, not a
            display flip).
          */}
          <Select
            items={[
              {
                value: "",
                label: defaultLabel
                  ? `${DEFAULT_DEVICE_PREFIX} (${defaultLabel})`
                  : DEFAULT_DEVICE_PREFIX,
              },
              ...devices.map((d) => ({ value: d.id, label: d.label })),
            ]}
            value={selectedPresent ? selected : ""}
            onValueChange={(v) => void onChange(typeof v === "string" ? v : "")}
            disabled={busy || dictation.isRecording}
          >
            <SelectTrigger
              className="w-[264px]"
              aria-label="Microphone device"
            >
              {/*
                `truncate` overrides shadcn's default `line-clamp-1` on the
                value so long FriendlyNames ellipsize cleanly inside the
                fixed-width trigger. line-clamp-1 silently fails here
                because the trigger also sets `whitespace-nowrap`, which
                stops -webkit-box from breaking the line line-clamp wants
                to clamp on.
              */}
              <SelectValue className="truncate" />
            </SelectTrigger>
            <SelectContent
              className="min-w-[264px] max-w-[480px]"
              alignItemWithTrigger={false}
              align="start"
            >
              <SelectGroup>
                <SelectItem value="">
                  {/*
                    Tailwind utilities (not custom CSS classes) so the
                    shadcn item's `focus:**:text-accent-foreground`
                    cascade reaches both lines. With `color:
                    var(--foreground)` we fought the highlight color and
                    lost — the title sat invisible on the focused row.
                  */}
                  <div className="flex flex-col gap-0.5 leading-tight">
                    <span className="font-medium">
                      {DEFAULT_DEVICE_PREFIX}
                    </span>
                    {defaultLabel && (
                      <span className="text-xs text-muted-foreground">
                        {defaultLabel}
                      </span>
                    )}
                  </div>
                </SelectItem>
                {devices.map((d) => (
                  <SelectItem key={d.id} value={d.id}>
                    {d.label}
                  </SelectItem>
                ))}
              </SelectGroup>
            </SelectContent>
          </Select>
        </div>
      </section>

      <section className="ow-audio__preview">
        <div className="ow-audio__preview-head">
          <div className="ow-audio__preview-label">Test microphone</div>
          <button
            type="button"
            className={
              "ow-audio__btn" +
              (testing ? " ow-audio__btn--active" : "")
            }
            onClick={() => void (testing ? stopTest() : startTest())}
            disabled={busy || dictation.isRecording}
            aria-pressed={testing}
          >
            {testing ? "Stop test" : "Start test"}
          </button>
        </div>
        <dl className="ow-audio__kv">
          <div className="ow-audio__kv-row">
            <dt>floor</dt>
            <dd>{PREVIEW_FLOOR_DB} dBFS</dd>
          </div>
          <div className="ow-audio__kv-row">
            <dt>peak</dt>
            <dd>{peakDb === null ? "—" : `${peakDb.toFixed(1)} dBFS`}</dd>
          </div>
          <div className="ow-audio__kv-row">
            <dt>sample rate</dt>
            <dd>{(PREVIEW_SAMPLE_RATE_HZ / 1000).toFixed(0)} kHz</dd>
          </div>
        </dl>
        <div className="ow-audio__meter">
          <LevelMeter
            bars={PREVIEW_BAR_COUNT}
            levels={levels}
            active={meterStatus}
            height={36}
            minHeight={4}
            gap={2}
            fill
          />
        </div>
      </section>

      {/*
        Audio behavior settings — pause-during-dictation toggle and
        the BT resume delay slider. Lives here (not in GeneralPane)
        because both rules act on audio output specifically; keeping
        them next to the device picker and mic test gives the user
        one mental "Audio" surface to find them on.

        Uses shadcn FieldGroup/Field while the rest of AudioPane is
        still BEM-styled (`ow-audio__*`). Visual mismatch is a known
        scoped cost — modernizing the rest of AudioPane is its own
        follow-up. The Separator above gives a clean visual break.
      */}
      <Separator className="my-4" />

      <FieldGroup className="px-1 pb-2">
        <Field orientation="horizontal">
          <FieldContent>
            <FieldLabel htmlFor="pause-audio">
              Pause audio during dictation
            </FieldLabel>
            <FieldDescription>
              Pauses Spotify, browser playback, and other media when
              you start recording, then resumes when recording ends.
              Falls back to muting system output for apps that don't
              support media controls.
            </FieldDescription>
          </FieldContent>
          <Switch
            id="pause-audio"
            checked={pauseAudio}
            onCheckedChange={(next) => {
              void setPauseAudio(next);
            }}
          />
        </Field>
        {SHOW_BT_RESUME_DELAY && (
          <Field>
            {/*
              Label + current value share a baseline so the right-
              aligned value floats opposite the label, not embedded
              in the description text — the description block stays
              stable as the slider drags. The value label mirrors the
              "Current version" row's `font-mono text-sm` for cross-
              pane consistency. Always one decimal so the readout
              never reflows between e.g. "5s" and "5.5s" widths.
            */}
            <div className="flex items-baseline justify-between gap-2">
              <FieldLabel htmlFor="bt-resume-delay">
                Bluetooth resume delay
              </FieldLabel>
              <span
                data-testid="bt-resume-delay-value"
                className="font-mono text-sm"
              >
                {formatDelayShort(btResumeDelayMs)}
              </span>
            </div>
            <FieldDescription>
              Pauses Bluetooth headphones long enough for them to
              switch back to stereo before music resumes. Wired or USB
              outputs ignore this setting.
            </FieldDescription>
            <div className="flex items-center gap-3">
              <span className="text-xs tabular-nums text-muted-foreground">
                0s
              </span>
              <Slider
                id="bt-resume-delay"
                className="flex-1"
                value={btResumeDelayMs}
                onValueChange={(next) => {
                  void setBtResumeDelayMs(next);
                }}
                min={0}
                max={BT_RESUME_DELAY_MAX_MS}
                step={BT_RESUME_DELAY_STEP_MS}
                disabled={!pauseAudio}
                aria-label="Bluetooth resume delay"
              />
              <span className="text-xs tabular-nums text-muted-foreground">
                10s
              </span>
            </div>
          </Field>
        )}
      </FieldGroup>

      {error && <div className="ow-audio__error">{error}</div>}
    </div>
  );
}

// Compact value-label form for the BT delay slider's right-aligned
// readout: "Off" at 0, otherwise always one decimal place ("5.0s",
// "5.5s", "10.0s") so the label width is stable across drags. With
// font-mono each character is also fixed-width, so the readout never
// reflows the row layout.
function formatDelayShort(ms: number): string {
  if (ms === 0) return "Off";
  const seconds = ms / 1000;
  return `${seconds.toFixed(1)}s`;
}

// Shortcuts pane — capture-on-click rebind for both toggle and cancel
// hotkeys. Each row is independent: clicking one chip starts a capture
// targeted at that slot; the backend tags the captured event with the
// active target and the UI saves to the matching slot.
function ShortcutsPane() {
  const [settings, setSettings] = useState<HotkeySettings | null>(null);
  const [recordingTarget, setRecordingTarget] = useState<HotkeyTarget | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    invoke<HotkeySettings>("settings_get_hotkeys")
      .then((s) => {
        if (alive) setSettings(s);
      })
      .catch((e) => {
        if (alive) setError(String(e));
      });
    return () => {
      alive = false;
    };
  }, []);

  // Apply a captured chord to the configured slot. Both capture paths
  // (Rust LL hook, JS keydown fallback) funnel through this so the UI
  // updates identically regardless of which fired first.
  const applyCapture = useCallback((payload: HotkeyCapturedPayload) => {
    const { target, config } = payload;
    // Whichever path fired, also clear the other so a delayed event from
    // the other source doesn't re-apply / re-trigger.
    cancelJsCapture();
    void invoke("settings_capture_hotkey_cancel").catch(() => {});
    void invoke("settings_set_hotkey", { target, config })
      .then(() => {
        setSettings((prev) => (prev ? { ...prev, [target]: config } : prev));
        setRecordingTarget(null);
        setError(null);
      })
      .catch((e) => {
        setError(String(e));
        setRecordingTarget(null);
      });
  }, []);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    void listen<HotkeyCapturedPayload>("hotkey_captured", (evt) => {
      applyCapture(evt.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [applyCapture]);

  // Cancel any in-flight capture if the pane unmounts (user navigates
  // away mid-rebind). Otherwise a stray keydown would silently apply as
  // a new binding.
  useEffect(() => {
    return () => {
      cancelJsCapture();
      void invoke("settings_capture_hotkey_cancel").catch(() => {});
    };
  }, []);

  const startCapture = useCallback(
    (target: HotkeyTarget) => {
      setError(null);
      setRecordingTarget(target);
      // JS path — handles the in-focus case where Chromium swallows
      // events before our LL hook sees them. No-ops on macOS.
      startJsCapture(target, applyCapture);
      // Rust path — handles the unfocused case (user wants to capture a
      // chord that's also a Chromium shortcut, e.g. Ctrl+J, by clicking
      // 'press keys…' then alt-tabbing away before pressing it).
      void invoke("settings_capture_hotkey_start", { target }).catch((e) => {
        setError(String(e));
        setRecordingTarget(null);
        cancelJsCapture();
      });
    },
    [applyCapture],
  );

  const cancelCapture = useCallback(async () => {
    setRecordingTarget(null);
    cancelJsCapture();
    try {
      await invoke("settings_capture_hotkey_cancel");
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const resetTarget = useCallback(async (target: HotkeyTarget) => {
    setError(null);
    try {
      const cfg = await invoke<HotkeyConfig>("settings_reset_hotkey", {
        target,
      });
      setSettings((prev) => (prev ? { ...prev, [target]: cfg } : prev));
    } catch (e) {
      setError(String(e));
    }
  }, []);

  return (
    <div className="ow-shortcuts">
      <header className="ow-shortcuts__header">
        <h2>Shortcuts</h2>
        <p>
          Captured as raw keycodes — survives layout changes (US ↔ Dvorak)
          and language switches. Both bindings are fully customizable.
        </p>
      </header>

      <ShortcutRow
        title="Toggle dictation"
        hint="Press anywhere to start. Press again to stop and transcribe."
        target="toggle"
        config={settings?.toggle ?? null}
        recordingTarget={recordingTarget}
        onStart={startCapture}
        onCancel={cancelCapture}
        onReset={resetTarget}
      />

      <ShortcutRow
        title="Cancel while recording"
        hint="Discards audio without transcribing. Only fires while a recording is active."
        target="cancel"
        config={settings?.cancel ?? null}
        recordingTarget={recordingTarget}
        onStart={startCapture}
        onCancel={cancelCapture}
        onReset={resetTarget}
      />

      <div className="ow-shortcuts__note">
        Hotkeys are captured at the OS level. Click a chip and press the
        key combination you want to bind.
      </div>

      {error && <div className="ow-shortcuts__error">{error}</div>}
    </div>
  );
}

interface ShortcutRowProps {
  title: string;
  hint: string;
  target: HotkeyTarget;
  config: HotkeyConfig | null;
  recordingTarget: HotkeyTarget | null;
  onStart: (target: HotkeyTarget) => void;
  onCancel: () => Promise<void>;
  onReset: (target: HotkeyTarget) => Promise<void>;
}

function ShortcutRow({
  title,
  hint,
  target,
  config,
  recordingTarget,
  onStart,
  onCancel,
  onReset,
}: ShortcutRowProps) {
  const recording = recordingTarget === target;
  const otherRecording =
    recordingTarget !== null && recordingTarget !== target;
  return (
    <section className="ow-shortcuts__row">
      <div className="ow-shortcuts__row-label">
        <div className="ow-shortcuts__row-title">{title}</div>
        <div className="ow-shortcuts__row-hint">{hint}</div>
      </div>
      <div className="ow-shortcuts__row-control">
        <button
          type="button"
          className={
            "ow-shortcuts__chip-button" +
            (recording ? " ow-shortcuts__chip-button--recording" : "")
          }
          onClick={recording || otherRecording ? undefined : () => onStart(target)}
          disabled={otherRecording}
          aria-label={`Rebind ${title}`}
          data-recording={recording ? "true" : "false"}
          data-target={target}
        >
          {recording ? (
            <span className="ow-shortcuts__chip-recording">press keys…</span>
          ) : (
            <HotkeyChip keys={configToChipKeys(config)} />
          )}
        </button>
        {recording ? (
          <button
            type="button"
            className="ow-shortcuts__btn"
            onClick={() => void onCancel()}
          >
            Cancel
          </button>
        ) : (
          <button
            type="button"
            className="ow-shortcuts__reset"
            onClick={() => void onReset(target)}
            disabled={otherRecording}
          >
            Reset to default
          </button>
        )}
      </div>
    </section>
  );
}

function HotkeyChip({ keys }: { keys: string[] }) {
  if (keys.length === 0) {
    return (
      <span className="ow-shortcuts__chip ow-shortcuts__chip--empty">none</span>
    );
  }
  return (
    <span className="ow-shortcuts__chip">
      {keys.map((k, i) => (
        <kbd key={i}>{k}</kbd>
      ))}
    </span>
  );
}

