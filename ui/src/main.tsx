import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { Dashboard } from "./Dashboard";
import { inShell } from "./store";
import "./styles/base.css";
import "./styles/panel.css";
import "./styles/dashboard.css";

const root = document.getElementById("root");
if (root === null) {
  throw new Error("index.html is missing the #root element the panel mounts into");
}

/**
 * One bundle serves both surfaces; the window's own label decides which one mounts.
 *
 * A label rather than a route: the shell creates the dashboard window itself, and a URL the
 * frontend could navigate to would let the popover become the dashboard inside a 380px frame.
 * Outside the shell — design work in a browser — the panel is the surface worth loading.
 */
async function surface(): Promise<"panel" | "dashboard"> {
  if (!inShell) {
    return window.location.hash === "#dashboard" ? "dashboard" : "panel";
  }
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow().label === "dashboard" ? "dashboard" : "panel";
}

surface().then((which) => {
  document.documentElement.dataset["surface"] = which;
  createRoot(root).render(
    <StrictMode>{which === "dashboard" ? <Dashboard /> : <App />}</StrictMode>,
  );
});
