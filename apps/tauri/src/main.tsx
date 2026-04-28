import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";

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
