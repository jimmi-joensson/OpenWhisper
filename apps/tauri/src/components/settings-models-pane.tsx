import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import {
  ModelsStoragePanel,
  type StorageModelRow,
} from "./models-storage-panel";
import {
  ModelsMemoryBudgetBar,
  type BudgetModelRow,
  type PreviewModel,
} from "./models-memory-budget-bar";
import { useMemoryStats } from "../lib/use-memory-stats";
import { Switch } from "./ui/switch";

// Settings → Models pane.
//
// TASK-62.13 (storage panel) and TASK-62.12 (budget bar) compose
// here. The pane shape, top to bottom:
//
//   1. Header (title + intro copy)
//   2. Memory budget bar — anchored to physical RAM, segments per
//      enabled model + ghost preview while hovering a row.
//   3. Footer caveat — projected vs. live; deep-links to Diagnostics.
//   4. Per-model rows — name, projected RAM, enable Switch, hover-
//      driven delta chip (`+<MB>` for disabled rows, `−<MB>` for
//      enabled rows). Pane-local hover state drives the budget bar's
//      previewModel.
//   5. Storage panel — disk + path + Show in Finder/Explorer.
//
// Catalog: hard-coded for v1. Parakeet ships enabled and locked (no
// other recognizer is wired up); the other entries are placeholders
// so the budget bar's hover-ghost preview has something to preview.
// Their toggles are pane-local — flipping them on doesn't actually
// load anything yet. When non-recognizer model lifecycles land, the
// toggle wires through to a real `models_set_enabled` invoke.

interface ModelCatalogEntry {
  id: string;
  label: string;
  ramMb: number;
  diskMb: number;
  accent: string;
  enabledByDefault: boolean;
  /// Locked = the user can't toggle this row in the v1 catalog. The
  /// recognizer is locked because it's the only loadable model and
  /// disabling it would brick dictation.
  locked?: boolean;
}

const MODEL_CATALOG: ModelCatalogEntry[] = [
  {
    id: "parakeet-multilang",
    label: "Parakeet · Multilingual",
    ramMb: 612,
    diskMb: 460,
    accent: "var(--recording)",
    enabledByDefault: true,
    locked: true,
  },
];

const OW_BASE_MB = 380;

export function SettingsModelsPane() {
  const [physicalMb, setPhysicalMb] = useState<number | null>(null);
  const { stats } = useMemoryStats();
  const [enabled, setEnabled] = useState<Record<string, boolean>>(() => {
    const init: Record<string, boolean> = {};
    for (const m of MODEL_CATALOG) init[m.id] = m.enabledByDefault;
    return init;
  });
  const [hoveredId, setHoveredId] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    void invoke<number>("system_physical_ram_mb")
      .then((mb) => {
        if (alive) setPhysicalMb(mb);
      })
      .catch(() => {
        // Anchor falls back to 16 GB so the bar still renders.
        if (alive) setPhysicalMb(16 * 1024);
      });
    return () => {
      alive = false;
    };
  }, []);

  // "Other apps" segment is a live read of (system used − our RSS).
  // When telemetry hasn't landed yet, fall back to a sane default
  // (40% of physical) so the bar doesn't render an absurd headroom
  // figure on first paint.
  const otherAppsMb = useMemo(() => {
    if (!stats || !physicalMb) return null;
    const sysUsedMb = Math.round(stats.system.used_bytes / (1024 * 1024));
    const ourRssMb = Math.round(stats.process.rss_bytes / (1024 * 1024));
    return Math.max(0, sysUsedMb - ourRssMb);
  }, [stats, physicalMb]);

  const enabledModels: BudgetModelRow[] = useMemo(
    () =>
      MODEL_CATALOG.filter((m) => enabled[m.id]).map((m) => ({
        id: m.id,
        label: m.label,
        ramMb: m.ramMb,
        accent: m.accent,
      })),
    [enabled],
  );

  const storageModels: StorageModelRow[] = useMemo(
    () =>
      MODEL_CATALOG.filter((m) => enabled[m.id]).map((m) => ({
        id: m.id,
        diskMb: m.diskMb,
      })),
    [enabled],
  );

  const previewModel: PreviewModel | undefined = useMemo(() => {
    if (!hoveredId) return undefined;
    const row = MODEL_CATALOG.find((m) => m.id === hoveredId);
    if (!row) return undefined;
    return {
      id: row.id,
      label: row.label,
      ramMb: row.ramMb,
      accent: row.accent,
      mode: enabled[row.id] ? "remove" : "add",
    };
  }, [hoveredId, enabled]);

  const onToggle = (id: string, next: boolean) => {
    setEnabled((prev) => ({ ...prev, [id]: next }));
  };

  const goToDiagnostics = () => {
    void emit("ow_navigate", "diagnostics");
  };

  return (
    <div className="ow-models-pane">
      <header className="ow-models-pane__header">
        <h2>Models</h2>
        <p>
          OpenWhisper bundles Parakeet for multilingual dictation across
          25 European languages. The budget bar shows its projected
          memory cost against this machine's physical RAM.
        </p>
      </header>

      {physicalMb !== null && otherAppsMb !== null && (
        <ModelsMemoryBudgetBar
          physicalMb={physicalMb}
          otherAppsMb={otherAppsMb}
          owBaseMb={OW_BASE_MB}
          enabledModels={enabledModels}
          previewModel={previewModel}
        />
      )}

      <p
        className="ow-models-pane__caveat"
        data-testid="settings-models-budget-caveat"
      >
        Memory figures are projected. Real RSS depends on your audio
        inputs, recognizer state, and OS pressure — for exact live
        numbers see{" "}
        <button
          type="button"
          className="ow-models-pane__caveat-link"
          data-testid="settings-models-caveat-link"
          onClick={goToDiagnostics}
        >
          Diagnostics → Memory
        </button>
        .
      </p>

      <ul className="ow-models-pane__list" data-testid="settings-models-list">
        {MODEL_CATALOG.map((m) => {
          const isEnabled = enabled[m.id];
          const isHovered = hoveredId === m.id;
          return (
            <li
              key={m.id}
              className="ow-models-row"
              data-testid={`settings-models-row-${m.id}`}
              data-enabled={isEnabled ? "true" : "false"}
              onMouseEnter={() => setHoveredId(m.id)}
              onMouseLeave={() =>
                setHoveredId((current) => (current === m.id ? null : current))
              }
            >
              <span
                className="ow-models-row__swatch"
                style={{ background: m.accent }}
              />
              <span className="ow-models-row__label">{m.label}</span>
              <span className="ow-models-row__ram">
                {m.ramMb >= 1024
                  ? `${(m.ramMb / 1024).toFixed(2)} GB`
                  : `${m.ramMb} MB`}
              </span>
              {isHovered && (
                <span
                  className={
                    "ow-models-row__chip" +
                    (isEnabled
                      ? " ow-models-row__chip--remove"
                      : " ow-models-row__chip--add")
                  }
                  data-testid={`settings-models-row-${m.id}-chip`}
                >
                  {isEnabled ? "−" : "+"}
                  {m.ramMb >= 1024
                    ? `${(m.ramMb / 1024).toFixed(1)} GB`
                    : `${m.ramMb} MB`}
                </span>
              )}
              <Switch
                checked={isEnabled}
                onCheckedChange={(next) => onToggle(m.id, next)}
                disabled={m.locked}
                aria-label={`${isEnabled ? "Disable" : "Enable"} ${m.label}`}
                data-testid={`settings-models-row-${m.id}-toggle`}
              />
            </li>
          );
        })}
      </ul>

      <ModelsStoragePanel enabledModels={storageModels} />
    </div>
  );
}
