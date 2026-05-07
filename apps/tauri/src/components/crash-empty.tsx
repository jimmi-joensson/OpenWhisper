import { invoke } from "@tauri-apps/api/core";
import { Bug } from "lucide-react";

const CRASHES_DIR_HINT = "~/Library/Logs/OpenWhisper/crashes/";

/// Empty-state composition. Replaces the entire pane (no header).
/// Centred 44 px tile with the design's star glyph, h3 + caption,
/// single Open-crash-folder ghost button.
export function CrashEmpty() {
  const onOpenFolder = () => {
    void invoke("crashes_open_folder").catch((e) => {
      console.error("[crashes_open_folder]", e);
    });
  };

  return (
    <div className="ow-crashes-empty" data-testid="crash-list-empty-body">
      <div className="ow-crashes-empty__tile" aria-hidden="true">
        <CrashGlyph size={20} />
      </div>
      <h2 className="ow-crashes-empty__title">No crashes recorded</h2>
      <p className="ow-crashes-empty__caption">
        We log crashes to{" "}
        <code className="ow-crashes-empty__path">{CRASHES_DIR_HINT}</code>{" "}
        so you can read or delete them yourself.
      </p>
      <div className="ow-crashes-empty__actions">
        <button
          type="button"
          className="ow-crashes-empty__cta"
          data-testid="crash-list-open-folder"
          onClick={onOpenFolder}
        >
          Open crash folder
        </button>
      </div>
    </div>
  );
}

/// Crash icon — lucide's Bug. Used by the empty state AND by the
/// Diagnostics overview entry card's destructive-tint tile. Single
/// source of truth so both surfaces stay aligned. Bug matches the
/// "Report on GitHub" button next door (TASK-78.6) — same icon
/// vocabulary as the GitHub `bug` issue label the report becomes.
export function CrashGlyph({ size = 14 }: { size?: number }) {
  return <Bug size={size} aria-hidden="true" />;
}
