import {
  ModelsStoragePanel,
  type StorageModelRow,
} from "./models-storage-panel";

// Settings → Models pane.
//
// TASK-62.13 lands the storage panel below the model list; TASK-62.12
// lands the budget bar above the list and the per-row hover-ghost
// preview. This commit scaffolds the shell — header + a placeholder
// model-list note + the storage panel — so the pane is no longer a
// "Coming soon" stub.
//
// Catalog: hard-coded for v1, single entry (Parakeet recognizer). The
// `diskMb` figure mirrors `core::recognizer::PARAKEET_WEIGHT_BYTES_FALLBACK`
// (460 MB), which is the conservative on-disk weight used when the
// platform-specific cache probe fails. When the catalog gains more
// entries (TASK-62.12+ adds cleanup-LLM, Whisper, Qwen toggles) this
// list extends; the storage panel re-aggregates automatically.
const MODEL_CATALOG: StorageModelRow[] = [
  {
    id: "parakeet-en",
    diskMb: 460,
  },
];

export function SettingsModelsPane() {
  return (
    <div className="ow-models-pane">
      <header className="ow-models-pane__header">
        <h2>Models</h2>
        <p>
          OpenWhisper bundles Parakeet for English dictation. Per-model
          load and unload run automatically — Diagnostics → Memory shows
          the live cost.
        </p>
      </header>

      <p
        className="ow-models-pane__list-placeholder"
        data-testid="settings-models-list-placeholder"
      >
        Recognizer (Parakeet) loaded automatically on first dictation.
      </p>

      <ModelsStoragePanel enabledModels={MODEL_CATALOG} />
    </div>
  );
}
