import { invoke } from "@tauri-apps/api/core";

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
        <CrashStarGlyph size={20} />
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

/// Star/asterisk glyph from the design — used by the empty state
/// AND by the Diagnostics overview entry card's destructive-tint
/// tile. Single source of truth so both surfaces stay aligned.
export function CrashStarGlyph({ size = 14 }: { size?: number }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 14 14"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M7 1.5 L 8.5 5 L 12.5 5.4 L 9.5 8 L 10.5 12 L 7 9.8 L 3.5 12 L 4.5 8 L 1.5 5.4 L 5.5 5 Z" />
    </svg>
  );
}
