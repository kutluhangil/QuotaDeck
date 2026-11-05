import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { Board } from "./liquid/Board";
import { hostPlatform } from "./platform";
import { applyDesignTheme, inShell, useDeck } from "./store";
import "./styles/base.css";
import "./styles/panel.css";
import "./styles/dashboard.css";
import "./styles/themes/liquid.css";
import "./styles/themes/claude.css";

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
  applyDesignTheme(useDeck.getState().settings.designTheme);
  // The macOS popover window is transparent over a Liquid Glass material; the glass modules
  // sit on the real desktop. Elsewhere (and in a browser) the window paints its own backdrop.
  document.documentElement.dataset["vibrancy"] =
    inShell && which === "panel" && hostPlatform() === "macos" ? "on" : "off";
  createRoot(root).render(
    <StrictMode>{which === "dashboard" ? <Board /> : <App />}</StrictMode>,
  );
});
