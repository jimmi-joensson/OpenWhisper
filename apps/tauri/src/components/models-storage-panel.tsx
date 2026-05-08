// TASK-62.13 — Settings → Models storage panel.
//
// Inline strip below the model list with total disk used by enabled
// models and install count. Path + reveal button are intentionally
// omitted in v1 — bundled Parakeet weights live inside the app
// resources, not the user-data models folder, so surfacing the empty
// folder would be misleading. Re-enable when on-demand model
// downloads land and the folder actually contains user-installed
// weights.

export interface StorageModelRow {
  id: string;
  /// On-disk weight in MB. When the catalog doesn't carry this yet,
  /// fall back to `ramMb * 0.78` per the design heuristic.
  diskMb: number;
}

export interface ModelsStoragePanelProps {
  enabledModels: StorageModelRow[];
}

export function ModelsStoragePanel({ enabledModels }: ModelsStoragePanelProps) {
  const totalMb = enabledModels.reduce((s, m) => s + m.diskMb, 0);

  const totalLabel =
    totalMb < 1024
      ? `${Math.round(totalMb)} MB`
      : `${(totalMb / 1024).toFixed(2)} GB`;

  return (
    <section
      className="ow-models-storage"
      data-testid="settings-models-storage"
    >
      <span className="ow-models-storage__title">Storage</span>
      <div className="ow-models-storage__box">
        <span
          className="ow-models-storage__total"
          data-testid="settings-models-storage-total"
        >
          {totalLabel}
        </span>
        <span
          className="ow-models-storage__meta"
          data-testid="settings-models-storage-count"
        >
          on disk · {enabledModels.length} model
          {enabledModels.length === 1 ? "" : "s"} installed
        </span>
      </div>
    </section>
  );
}
