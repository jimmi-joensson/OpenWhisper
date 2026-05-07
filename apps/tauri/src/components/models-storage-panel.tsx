import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { FolderOpen } from "lucide-react";
import { Button } from "./ui/button";

// TASK-62.13 — Settings → Models storage panel.
//
// Inline strip below the model list with: total disk used by enabled
// models, install count, the canonical models folder path, and a
// platform-conditional "Show in Finder" / "Show in Explorer" button.
//
// Path resolution goes through `models_storage_path` (Tauri command,
// command-time-resolved so env-overridden config dirs survive). Click
// goes through `models_open_folder` — mirrors `crashes_open_folder`'s
// shell-out approach to avoid wrangling per-path opener-plugin scopes.

export interface StorageModelRow {
  id: string;
  /// On-disk weight in MB. When the catalog doesn't carry this yet,
  /// fall back to `ramMb * 0.78` per the design heuristic.
  diskMb: number;
}

export interface ModelsStoragePanelProps {
  enabledModels: StorageModelRow[];
}

function detectPlatform(): "macos" | "windows" | "other" {
  if (typeof navigator === "undefined") return "other";
  const ua = navigator.userAgent || navigator.platform || "";
  if (/Mac|iPhone|iPad/.test(ua)) return "macos";
  if (/Win/.test(ua)) return "windows";
  return "other";
}

function formatDiskTotal(totalMb: number): string {
  if (totalMb < 1024) return `${totalMb} MB on disk`;
  return `${(totalMb / 1024).toFixed(2)} GB on disk`;
}

export function ModelsStoragePanel({ enabledModels }: ModelsStoragePanelProps) {
  const [path, setPath] = useState<string>("");
  const [platform] = useState(detectPlatform);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let alive = true;
    void invoke<string>("models_storage_path")
      .then((p) => {
        if (alive) setPath(p);
      })
      .catch((e) => {
        if (alive) setPath(`Unresolved (${String(e)})`);
      });
    return () => {
      alive = false;
    };
  }, []);

  const totalMb = enabledModels.reduce((s, m) => s + m.diskMb, 0);
  const buttonLabel =
    platform === "windows" ? "Show in Explorer" : "Show in Finder";

  const onReveal = () => {
    setBusy(true);
    void invoke("models_open_folder")
      .catch(() => {
        // Surface only via the dev console — the strip is read-only on
        // the user-facing side; failing to reveal is rare and recoverable
        // (user can copy the path).
      })
      .finally(() => {
        setBusy(false);
      });
  };

  return (
    <section
      className="ow-models-storage"
      data-testid="settings-models-storage"
    >
      <div className="ow-models-storage__row">
        <span
          className="ow-models-storage__total"
          data-testid="settings-models-storage-total"
        >
          {formatDiskTotal(totalMb)}
        </span>
        <span className="ow-models-storage__sep">·</span>
        <span
          className="ow-models-storage__count"
          data-testid="settings-models-storage-count"
        >
          {enabledModels.length} model{enabledModels.length === 1 ? "" : "s"}{" "}
          installed
        </span>
        <span className="ow-models-storage__sep">·</span>
        <span
          className="ow-models-storage__path"
          data-testid="settings-models-storage-path"
          title={path}
        >
          {path || "Resolving…"}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="ow-models-storage__reveal"
          data-testid="settings-models-storage-reveal"
          onClick={onReveal}
          disabled={busy || !path}
        >
          <FolderOpen data-icon="inline-start" />
          {buttonLabel}
        </Button>
      </div>
    </section>
  );
}
