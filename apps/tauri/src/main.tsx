import React from "react";
import ReactDOM from "react-dom/client";
import { getCurrentWindow } from "@tauri-apps/api/window";
import App from "./App";
import { PillOverlay } from "./PillOverlay";

// Single Vite bundle, two windows. Choose the React tree by window label.
const label = getCurrentWindow().label;
const Root = label === "pill" ? PillOverlay : App;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
