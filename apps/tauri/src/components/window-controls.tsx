import { useEffect, useState } from "react";
import { Minus, X } from "lucide-react";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface WindowControlsProps {
  platform: "macos" | "windows";
}

// Win 11 chrome maximize glyph — single rounded square outline.
function MaximizeGlyph() {
  return (
    <svg width="11" height="11" viewBox="0 0 11 11" aria-hidden="true">
      <rect
        x="0.75"
        y="0.75"
        width="9.5"
        height="9.5"
        rx="1"
        fill="none"
        stroke="currentColor"
        strokeWidth="1"
      />
    </svg>
  );
}

// Win 11 chrome restore-down glyph — two overlapping squares
// (front lower-left, back upper-right). lucide-react has no direct
// equivalent (Copy is a clipboard, not the restore glyph), so we
// hand-roll it to match the OS chrome convention.
function RestoreGlyph() {
  return (
    <svg width="11" height="11" viewBox="0 0 11 11" aria-hidden="true">
      <rect
        x="2.5"
        y="0.75"
        width="7.75"
        height="7.75"
        rx="1"
        fill="none"
        stroke="currentColor"
        strokeWidth="1"
      />
      <rect
        x="0.75"
        y="2.5"
        width="7.75"
        height="7.75"
        rx="1"
        fill="none"
        stroke="currentColor"
        strokeWidth="1"
      />
    </svg>
  );
}

export function WindowControls({ platform }: WindowControlsProps) {
  const [maximized, setMaximized] = useState(false);

  // Subscribe via getCurrentWindow().onResized — NOT the global
  // listen("tauri://resize"), which doesn't fire reliably for synthetic
  // resize events in Tauri 2.10.
  useEffect(() => {
    if (platform !== "windows") return;
    const win = getCurrentWindow();
    void win.isMaximized().then(setMaximized);
    const unlistenPromise = win.onResized(() => {
      void win.isMaximized().then(setMaximized);
    });
    return () => {
      void unlistenPromise.then((fn) => fn());
    };
  }, [platform]);

  if (platform !== "windows") return null;

  const win = getCurrentWindow();
  return (
    <div className="ow-window-controls" data-tauri-drag-region>
      <button
        type="button"
        className="ow-window-controls__btn"
        data-testid="window-control-minimize"
        aria-label="Minimize"
        data-tauri-drag-region="false"
        onClick={() => void win.minimize()}
      >
        <Minus size={14} aria-hidden="true" />
      </button>
      <button
        type="button"
        className="ow-window-controls__btn"
        data-testid="window-control-maximize"
        aria-label={maximized ? "Restore" : "Maximize"}
        data-tauri-drag-region="false"
        onClick={() => void win.toggleMaximize()}
      >
        {maximized ? <RestoreGlyph /> : <MaximizeGlyph />}
      </button>
      <button
        type="button"
        className="ow-window-controls__btn ow-window-controls__btn--close"
        data-testid="window-control-close"
        aria-label="Close"
        data-tauri-drag-region="false"
        onClick={() => void win.close()}
      >
        <X size={14} aria-hidden="true" />
      </button>
    </div>
  );
}
