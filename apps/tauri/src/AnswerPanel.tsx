import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { ASK_TICK_EVENT, type AskSnapshot } from "./lib/ask";
import "./AnswerPanel.css";

// Placeholder for the answer window of Ask (TASK-97.4). It only shows
// that the window is wired up: the phase and the answer text from
// `ask_tick`. The real panel (states, question, Copy / Insert /
// Dismiss / Cancel) is TASK-97.5 and replaces this component.
export function AnswerPanel() {
  const [ask, setAsk] = useState<AskSnapshot | null>(null);

  useEffect(() => {
    let alive = true;
    // The window may load after the last tick: start from the
    // snapshot, then follow the ticks. An older version never
    // replaces a newer one.
    const take = (next: AskSnapshot | null) => {
      if (!alive || !next) return;
      setAsk((prev) => (prev && prev.version > next.version ? prev : next));
    };
    void invoke<AskSnapshot | null>("ask_snapshot").then(take).catch(() => {});
    const unlisten = listen<AskSnapshot>(ASK_TICK_EVENT, (event) => take(event.payload));
    return () => {
      alive = false;
      void unlisten.then((fn) => fn());
    };
  }, []);

  const phase = ask?.phase ?? "idle";
  return (
    <div className="answer-root" data-testid="answer-panel" data-phase={phase}>
      <div className="answer-card">
        <div className="answer-phase" data-testid="answer-phase">
          {phase}
        </div>
        <div data-testid="answer-text">{ask?.error || ask?.answer || ""}</div>
      </div>
    </div>
  );
}
