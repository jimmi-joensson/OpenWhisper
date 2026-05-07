import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { CodeXml } from "lucide-react";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from "./ui/sheet";
import { Button } from "./ui/button";
import { Switch } from "./ui/switch";
import { ToggleGroup, ToggleGroupItem } from "./ui/toggle-group";
import type { PillStatus } from "../lib/pill-state";
import { refetchCrashes } from "../lib/use-crashes";

const PILL_STATES: PillStatus[] = ["idle", "recording", "transcribing"];

export interface DevToolsPanelProps {
  pillEnabled: boolean;
  pillStatus: PillStatus;
  onPillEnabledChange: (enabled: boolean) => void;
  onPillStatusChange: (status: PillStatus) => void;
}

/// Dev-only floating tools — TanStack-Devtools-style. A persistent
/// trigger button sits in the bottom-right corner across every route;
/// clicking it opens a right-side Sheet with the dev controls
/// (pill-state override, simulate crash) inside.
///
/// Gated to `import.meta.env.DEV` at the call site — release builds
/// don't render this at all.
export function DevToolsPanel({
  pillEnabled,
  pillStatus,
  onPillEnabledChange,
  onPillStatusChange,
}: DevToolsPanelProps) {
  const [open, setOpen] = useState(false);
  const [crashFeedback, setCrashFeedback] = useState<
    | { kind: "idle" }
    | { kind: "ok"; message: string }
    | { kind: "fail"; message: string }
  >({ kind: "idle" });

  const onSimulateCrash = async () => {
    try {
      await invoke("crashes_debug_trigger_panic");
      // The panic happens on a spawned worker thread, so the file
      // write doesn't always finish before this `invoke` returns.
      // Bump the shared store on a short delay so the sidebar dot
      // and overview entry card pick it up without waiting for
      // the next 2 s poll. The poll itself is the backstop.
      window.setTimeout(() => {
        void refetchCrashes();
      }, 150);
      setCrashFeedback({
        kind: "ok",
        message: "Panic dispatched on a worker thread. Sidebar + Diagnostics update within ~150 ms.",
      });
    } catch (e) {
      setCrashFeedback({
        kind: "fail",
        message: e instanceof Error ? e.message : String(e),
      });
    }
  };

  return (
    <>
      {/* Floating trigger — fixed bottom-right, persistent across
          routes, OW-orange tint so it reads as a dev affordance and
          not a primary action. */}
      <button
        type="button"
        className="ow-devtools__trigger"
        data-testid="devtools-trigger"
        aria-label="Open developer tools"
        onClick={() => setOpen(true)}
      >
        <CodeXml size={14} aria-hidden="true" />
        <span className="ow-devtools__trigger-label">DEV</span>
      </button>

      <Sheet open={open} onOpenChange={setOpen}>
        <SheetContent
          side="right"
          className="ow-devtools__sheet"
          data-testid="devtools-sheet"
        >
          <SheetHeader className="ow-devtools__sheet-header">
            <SheetTitle>Developer tools</SheetTitle>
            <SheetDescription>
              In-app overrides for testing. Only visible in dev builds.
            </SheetDescription>
          </SheetHeader>

          <div className="ow-devtools__sheet-body">
            <PillControlsSection
              enabled={pillEnabled}
              status={pillStatus}
              onEnabledChange={onPillEnabledChange}
              onStatusChange={onPillStatusChange}
            />
            <SimulateCrashSection
              feedback={crashFeedback}
              onSimulate={onSimulateCrash}
              onClear={() => setCrashFeedback({ kind: "idle" })}
            />
          </div>
        </SheetContent>
      </Sheet>
    </>
  );
}

function PillControlsSection({
  enabled,
  status,
  onEnabledChange,
  onStatusChange,
}: {
  enabled: boolean;
  status: PillStatus;
  onEnabledChange: (v: boolean) => void;
  onStatusChange: (s: PillStatus) => void;
}) {
  return (
    <section className="ow-devtools__section">
      <header className="ow-devtools__section-header">
        <h3 className="ow-devtools__section-title">Pill state</h3>
        <p className="ow-devtools__section-sub">
          Manual override suspends auto-emit from the dictation hook so
          the pill renders the picked state. Recording uses a simulated
          20 Hz envelope so the meter has motion without a live mic.
        </p>
      </header>

      <label className="ow-devtools__row">
        <div className="ow-devtools__row-body">
          <span className="ow-devtools__row-label">Manual override</span>
          <span className="ow-devtools__row-hint">
            {enabled ? "Pill listens to the picker below" : "Pill follows the live dictation phase"}
          </span>
        </div>
        <Switch
          checked={enabled}
          onCheckedChange={onEnabledChange}
          data-testid="devtools-pill-manual"
        />
      </label>

      <ToggleGroup
        value={enabled ? [status] : []}
        onValueChange={(values) => {
          // Base-UI's ToggleGroup is array-shaped even for single-
          // select usage (mirrors the project's existing
          // `general-pane.tsx` Theme picker). Take the first entry;
          // empty array = user toggled the active option off — keep
          // the picker idempotent by no-op'ing rather than letting
          // the pill fall to an undefined phase.
          const next = values[0];
          if (typeof next !== "string") return;
          if (!enabled) onEnabledChange(true);
          onStatusChange(next as PillStatus);
        }}
        variant="outline"
        className="ow-devtools__toggle-group"
        data-testid="devtools-pill-status"
      >
        {PILL_STATES.map((s) => (
          <ToggleGroupItem
            key={s}
            value={s}
            aria-label={`Set pill to ${s}`}
            data-testid={`devtools-pill-status-${s}`}
          >
            {s}
          </ToggleGroupItem>
        ))}
      </ToggleGroup>
    </section>
  );
}

function SimulateCrashSection({
  feedback,
  onSimulate,
  onClear,
}: {
  feedback:
    | { kind: "idle" }
    | { kind: "ok"; message: string }
    | { kind: "fail"; message: string };
  onSimulate: () => void;
  onClear: () => void;
}) {
  return (
    <section className="ow-devtools__section">
      <header className="ow-devtools__section-header">
        <h3 className="ow-devtools__section-title">Simulate crash</h3>
        <p className="ow-devtools__section-sub">
          Panics on a worker thread via{" "}
          <code className="ow-devtools__code">crashes_debug_trigger_panic</code>.
          The Rust panic hook captures the backtrace, redacts string fields,
          and writes <code className="ow-devtools__code">&lt;unix-ms&gt;.json</code>{" "}
          into the crash dir. Diagnostics → Crashes polls every 2 s — the
          row appears without a relaunch.
        </p>
      </header>

      <div className="ow-devtools__cta-row">
        <Button
          variant="outline"
          onClick={() => {
            onClear();
            onSimulate();
          }}
          data-testid="devtools-simulate-crash"
        >
          Trigger panic
        </Button>
        {feedback.kind === "ok" && (
          <span
            className="ow-devtools__feedback ow-devtools__feedback--ok"
            data-testid="devtools-simulate-crash-feedback"
          >
            {feedback.message}
          </span>
        )}
        {feedback.kind === "fail" && (
          <span
            className="ow-devtools__feedback ow-devtools__feedback--fail"
            data-testid="devtools-simulate-crash-feedback"
          >
            {feedback.message}
          </span>
        )}
      </div>
    </section>
  );
}
