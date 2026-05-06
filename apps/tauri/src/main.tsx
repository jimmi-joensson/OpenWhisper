import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";

// Suppress the WebView right-click context menu in production. The menu's
// "Reload" entry would re-mount the React tree against a Rust process
// that already holds engine state (cpal worker, recognizer global,
// settings caches), and the AirPods HFP / preview-stream / dictation-tick
// listener interactions that survive a reload aren't worth the support
// burden for a release. Dev keeps the menu so DevTools / Inspect Element
// stays one click away.
if (import.meta.env.PROD) {
  window.addEventListener("contextmenu", (event) => {
    event.preventDefault();
  });
}

// Single Vite bundle, two windows. Choose the React tree by window label.
// PillOverlay (and its global html/body/#root CSS) is dynamically imported
// only in the pill window so it doesn't override the main-window layout.
// Settings is an in-window route inside App, not its own window.
const label = getCurrentWindow().label;

const Root: React.ComponentType =
  label === "pill"
    ? React.lazy(() =>
        import("./PillOverlay").then((m) => ({ default: m.PillOverlay })),
      )
    : React.lazy(() => import("./App").then((m) => ({ default: m.default })));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <React.Suspense fallback={null}>
      <Root />
    </React.Suspense>
  </React.StrictMode>,
);
