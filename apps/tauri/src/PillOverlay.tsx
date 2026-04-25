import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import {
  EMPTY_LEVELS,
  INITIAL_PILL_STATE,
  PILL_STATE_EVENT,
  type PillState,
} from "./lib/pill-state";
import "./PillOverlay.css";

export function PillOverlay() {
  const [state, setState] = useState<PillState>(INITIAL_PILL_STATE);

  // Listen for state updates from the main window.
  useEffect(() => {
    const unlisten = listen<PillState>(PILL_STATE_EVENT, (event) => {
      setState(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // Toggle native click-through whenever phase changes.
  // Idle = clickable, recording/transcribing = pass-through.
  useEffect(() => {
    const clickable = state.status === "idle";
    invoke("set_pill_click_through", { passthrough: !clickable }).catch(
      // eslint-disable-next-line no-console
      (e) => console.warn("set_pill_click_through failed", e),
    );
  }, [state.status]);

  // Position once on mount.
  useEffect(() => {
    invoke("position_pill_bottom_center").catch(
      // eslint-disable-next-line no-console
      (e) => console.warn("position_pill_bottom_center failed", e),
    );
  }, []);

  return (
    <div className="pill-root">
      <div className="pill-capsule">
        {state.status === "idle" && <IdleDots />}
        {state.status === "recording" && (
          <LevelMeter levels={state.levels} active />
        )}
        {state.status === "transcribing" && (
          <>
            <Spinner />
            <LevelMeter levels={state.levels} active={false} />
          </>
        )}
      </div>
    </div>
  );
}

function IdleDots() {
  return (
    <div className="pill-idle-dots">
      {[0, 1, 2].map((i) => (
        <span key={i} className="pill-idle-dot" />
      ))}
    </div>
  );
}

function LevelMeter({
  levels,
  active,
}: {
  levels: number[];
  active: boolean;
}) {
  const safe = levels.length === 12 ? levels : EMPTY_LEVELS;
  return (
    <div className="pill-meter" aria-hidden>
      {safe.map((v, i) => {
        const clamped = Math.max(0.05, Math.min(1, v));
        return (
          <span
            key={i}
            className={`pill-meter-bar ${active ? "is-active" : "is-muted"}`}
            style={{ transform: `scaleY(${clamped})` }}
          />
        );
      })}
    </div>
  );
}

function Spinner() {
  return <span className="pill-spinner" aria-hidden />;
}
