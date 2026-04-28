import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import "./Settings.css";

// Sidebar items mirror the design (project_recognizer_tauri / screens.jsx
// SettingsSidebarLayout). Order matters — General is the landing pane.
const PANES = [
  { id: "general", label: "General", icon: "⚙" },
  { id: "audio", label: "Audio", icon: "🎙" },
  { id: "models", label: "Models", icon: "◆" },
  { id: "shortcuts", label: "Shortcuts", icon: "⌘" },
] as const;

type PaneId = (typeof PANES)[number]["id"];

// Cross-platform hotkey descriptors — mirror the Rust types.
export type HotkeyKind = "modifier-tap" | "chord";
export interface HotkeyConfig {
  kind: HotkeyKind;
  code: string;
  mods: string[];
}
export interface HotkeySettings {
  toggle: HotkeyConfig;
  cancel: HotkeyConfig;
}
export type HotkeyTarget = "toggle" | "cancel";
export interface HotkeyCapturedPayload {
  target: HotkeyTarget;
  config: HotkeyConfig;
}

// SettingsShell renders the body only — sidebar + pane. The titlebar
// (back-arrow + "Settings" title) lives at the window level in App.tsx
// so it shares the strip with the macOS traffic-light buttons.
export function SettingsShell() {
  const [active, setActive] = useState<PaneId>("general");
  const sidebarRef = useRef<HTMLDivElement>(null);

  // ↑/↓ on a focused sidebar item moves selection. Keeps focus on the
  // newly-active item so the keyboard cycle stays continuous.
  const onSidebarKey = useCallback(
    (e: React.KeyboardEvent<HTMLDivElement>) => {
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      e.preventDefault();
      const idx = PANES.findIndex((p) => p.id === active);
      const next =
        e.key === "ArrowDown"
          ? PANES[(idx + 1) % PANES.length]
          : PANES[(idx - 1 + PANES.length) % PANES.length];
      setActive(next.id);
      requestAnimationFrame(() => {
        const node = sidebarRef.current?.querySelector<HTMLButtonElement>(
          `[data-pane="${next.id}"]`,
        );
        node?.focus();
      });
    },
    [active],
  );

  return (
    <div className="ow-settings">
      <div
        ref={sidebarRef}
        className="ow-settings__sidebar"
        role="tablist"
        aria-orientation="vertical"
        onKeyDown={onSidebarKey}
      >
        {PANES.map((p) => (
          <button
            key={p.id}
            data-pane={p.id}
            role="tab"
            aria-selected={active === p.id}
            tabIndex={active === p.id ? 0 : -1}
            className={
              "ow-settings__sidebar-item" +
              (active === p.id ? " ow-settings__sidebar-item--active" : "")
            }
            onClick={() => setActive(p.id)}
          >
            <span className="ow-settings__sidebar-icon">{p.icon}</span>
            <span>{p.label}</span>
          </button>
        ))}
      </div>

      <div
        className="ow-settings__pane"
        role="tabpanel"
        aria-labelledby={active}
      >
        {active === "general" && <PaneStub title="General" />}
        {active === "audio" && <PaneStub title="Audio" />}
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

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    void listen<HotkeyCapturedPayload>("hotkey_captured", (evt) => {
      const { target, config } = evt.payload;
      void invoke("settings_set_hotkey", { target, config })
        .then(() => {
          setSettings((prev) =>
            prev ? { ...prev, [target]: config } : prev,
          );
          setRecordingTarget(null);
          setError(null);
        })
        .catch((e) => {
          setError(String(e));
          setRecordingTarget(null);
        });
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, []);

  const startCapture = useCallback((target: HotkeyTarget) => {
    setError(null);
    setRecordingTarget(target);
    void invoke("settings_capture_hotkey_start", { target }).catch((e) => {
      setError(String(e));
      setRecordingTarget(null);
    });
  }, []);

  const cancelCapture = useCallback(async () => {
    setRecordingTarget(null);
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

function configToChipKeys(cfg: HotkeyConfig | null): string[] {
  if (!cfg) return [];
  if (cfg.kind === "modifier-tap") {
    return [modifierLabel(cfg.code)];
  }
  return [...cfg.mods.map(modShortLabel), codeLabel(cfg.code)];
}

function modifierLabel(code: string): string {
  switch (code) {
    case "RightCommand":
      return "Right ⌘";
    case "LeftCommand":
      return "Left ⌘";
    case "RightShift":
      return "Right ⇧";
    case "LeftShift":
      return "Left ⇧";
    case "RightOption":
      return "Right ⌥";
    case "LeftOption":
      return "Left ⌥";
    case "RightControl":
      return "Right ⌃";
    case "LeftControl":
      return "Left ⌃";
    default:
      return code;
  }
}

function modShortLabel(name: string): string {
  switch (name) {
    case "Ctrl":
      return "Ctrl";
    case "Shift":
      return "Shift";
    case "Alt":
      return "Alt";
    case "Cmd":
      return "⌘";
    case "Win":
      return "Win";
    default:
      return name;
  }
}

function codeLabel(code: string): string {
  switch (code) {
    case "ArrowLeft":
      return "←";
    case "ArrowRight":
      return "→";
    case "ArrowUp":
      return "↑";
    case "ArrowDown":
      return "↓";
    case "Return":
      return "Enter";
    case "Escape":
      return "Esc";
    case "ForwardDelete":
      return "Del";
    default:
      return code;
  }
}
