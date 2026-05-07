// TASK-62.12 — Settings → Models memory budget bar.
//
// Stacked horizontal bar anchored to physical RAM with hover-ghost
// preview. The headline addition for the Models pane: shows what
// enabling a not-yet-loaded model would cost BEFORE the user toggles
// it on. Sits at the top of the pane; the per-model rows below drive
// the preview via `previewModel`.
//
// Animation tier: T3 (settings pane reveal). Per
// openwhisper-animation-philosophy: spring on container morph is
// permitted, decorative whimsy is not. The diag-pulse keyframe on
// the ghost segment is a subtle "this isn't real yet" cue, not
// decoration. CSS transition (transform-only) on the rest segments
// when toggle commits — 220 ms ease-out.
//
// All props are pure. Pane state owns the hover; the bar just renders.

export interface BudgetModelRow {
  /// Stable id (e.g. "parakeet-en"); used for React keys + test selectors.
  id: string;
  /// Display label (e.g. "Parakeet · English").
  label: string;
  /// Projected resident memory in MB.
  ramMb: number;
  /// Accent color for this model's bar segment + legend swatch + delta
  /// chip. CSS color string (semantic token preferred —
  /// `var(--recording)`, `var(--info)`, etc.).
  accent: string;
}

export type PreviewMode = "add" | "remove";

export interface PreviewModel extends BudgetModelRow {
  mode: PreviewMode;
}

export interface ModelsMemoryBudgetBarProps {
  physicalMb: number;
  /// "Other apps" share — currently-used host memory minus our own
  /// process RSS. Drives the faded leftmost segment. The pane derives
  /// it from `useMemoryStats` so it tracks the live system load.
  otherAppsMb: number;
  /// OpenWhisper baseline (Tauri webview + Rust shell, no models).
  /// 380 MB matches the design prototype's hand-calibrated value.
  owBaseMb: number;
  /// Per-enabled model — one bar segment each, in catalog order.
  enabledModels: BudgetModelRow[];
  /// Hover preview. `undefined` = rest state.
  previewModel?: PreviewModel;
}

function formatMb(mb: number): string {
  if (mb >= 1024) return `${(mb / 1024).toFixed(2)} GB`;
  return `${Math.round(mb)} MB`;
}

export function ModelsMemoryBudgetBar({
  physicalMb,
  otherAppsMb,
  owBaseMb,
  enabledModels,
  previewModel,
}: ModelsMemoryBudgetBarProps) {
  const enabledTotalMb = enabledModels.reduce((s, m) => s + m.ramMb, 0);
  const owTotalMb = owBaseMb + enabledTotalMb;
  const baselineHeadroom = Math.max(
    0,
    physicalMb - otherAppsMb - owTotalMb,
  );

  // Preview math:
  //   add    → ghost segment costs preview.ramMb, headroom shrinks by it
  //   remove → headroom grows by the existing model's ramMb
  let projectedHeadroom = baselineHeadroom;
  if (previewModel) {
    if (previewModel.mode === "add") {
      projectedHeadroom = Math.max(0, baselineHeadroom - previewModel.ramMb);
    } else {
      projectedHeadroom = baselineHeadroom + previewModel.ramMb;
    }
  }

  const headroomTone =
    previewModel?.mode === "add"
      ? "amber"
      : previewModel?.mode === "remove"
        ? "green"
        : "rest";

  return (
    <section
      className="ow-budget-bar"
      data-testid="settings-models-budget-bar"
      data-preview-mode={previewModel?.mode ?? "rest"}
    >
      <header className="ow-budget-bar__header">
        <div className="ow-budget-bar__kicker">
          <span className="ow-budget-bar__title">Memory budget</span>
          <span
            className="ow-budget-bar__physical"
            data-testid="settings-models-budget-physical"
          >
            of {formatMb(physicalMb)} physical
          </span>
        </div>
        <div className="ow-budget-bar__readouts">
          <span className="ow-budget-bar__readout">
            <span className="ow-budget-bar__readout-label">OpenWhisper</span>
            <span
              className="ow-budget-bar__readout-value"
              data-testid="settings-models-budget-ow-total"
            >
              {formatMb(owTotalMb)}
            </span>
          </span>
          <span className="ow-budget-bar__readout">
            <span className="ow-budget-bar__readout-label">Headroom</span>
            <span
              className="ow-budget-bar__readout-value"
              data-tone={headroomTone}
              data-testid="settings-models-budget-headroom"
            >
              {previewModel
                ? `${formatMb(projectedHeadroom)} (was ${formatMb(baselineHeadroom)})`
                : formatMb(baselineHeadroom)}
            </span>
          </span>
        </div>
      </header>

      <div
        className="ow-budget-bar__bar"
        role="presentation"
        aria-label="Memory budget — projected cost of enabled models against physical RAM"
      >
        <span
          className="ow-budget-bar__seg ow-budget-bar__seg--other"
          style={{ flex: `${otherAppsMb} 0 0` }}
          title={`System & other apps: ${formatMb(otherAppsMb)}`}
        />
        <span
          className="ow-budget-bar__seg ow-budget-bar__seg--base"
          style={{ flex: `${owBaseMb} 0 0` }}
          title={`OpenWhisper base: ${formatMb(owBaseMb)}`}
        />
        {enabledModels.map((m) => {
          const departing =
            previewModel?.mode === "remove" && previewModel.id === m.id;
          return (
            <span
              key={m.id}
              className={
                "ow-budget-bar__seg ow-budget-bar__seg--model" +
                (departing ? " ow-budget-bar__seg--departing" : "")
              }
              data-testid={`settings-models-budget-seg-${m.id}`}
              data-departing={departing ? "true" : undefined}
              style={{ flex: `${m.ramMb} 0 0`, background: m.accent }}
              title={`${m.label}: ${formatMb(m.ramMb)}`}
            />
          );
        })}
        {previewModel?.mode === "add" && (
          <span
            className="ow-budget-bar__seg ow-budget-bar__seg--ghost"
            data-testid={`settings-models-budget-ghost-${previewModel.id}`}
            style={{
              flex: `${previewModel.ramMb} 0 0`,
              background: previewModel.accent,
            }}
            title={`Preview: enabling ${previewModel.label} would cost ${formatMb(previewModel.ramMb)}`}
          />
        )}
        <span
          className="ow-budget-bar__seg ow-budget-bar__seg--headroom"
          style={{ flex: `${projectedHeadroom} 0 0` }}
          title={`Headroom: ${formatMb(projectedHeadroom)}`}
        />
      </div>

      <ul className="ow-budget-bar__legend">
        <li className="ow-budget-bar__legend-item ow-budget-bar__legend-item--other">
          <span className="ow-budget-bar__swatch ow-budget-bar__swatch--other" />
          <span>System & other apps</span>
          <span className="ow-budget-bar__legend-value">
            {formatMb(otherAppsMb)}
          </span>
        </li>
        <li className="ow-budget-bar__legend-item ow-budget-bar__legend-item--base">
          <span className="ow-budget-bar__swatch ow-budget-bar__swatch--base" />
          <span>OpenWhisper base</span>
          <span className="ow-budget-bar__legend-value">
            {formatMb(owBaseMb)}
          </span>
        </li>
        {enabledModels.map((m) => {
          const departing =
            previewModel?.mode === "remove" && previewModel.id === m.id;
          return (
            <li
              key={m.id}
              className={
                "ow-budget-bar__legend-item" +
                (departing ? " ow-budget-bar__legend-item--departing" : "")
              }
              data-testid={`settings-models-budget-legend-${m.id}`}
            >
              <span
                className="ow-budget-bar__swatch"
                style={{ background: m.accent }}
              />
              <span>
                {departing ? "− " : ""}
                {m.label}
              </span>
              <span className="ow-budget-bar__legend-value">
                {formatMb(m.ramMb)}
              </span>
            </li>
          );
        })}
        {previewModel?.mode === "add" && (
          <li
            className="ow-budget-bar__legend-item ow-budget-bar__legend-item--ghost"
            data-testid={`settings-models-budget-legend-ghost-${previewModel.id}`}
          >
            <span
              className="ow-budget-bar__swatch ow-budget-bar__swatch--ghost"
              style={{ background: previewModel.accent }}
            />
            <span>+ {previewModel.label}</span>
            <span className="ow-budget-bar__legend-value">
              {formatMb(previewModel.ramMb)}
            </span>
          </li>
        )}
      </ul>
    </section>
  );
}
