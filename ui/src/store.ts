import { create } from "zustand";

import { demoDeck } from "./demo";
import type { DeckState, Settings, TrayMode } from "./types";

/** True inside the Tauri shell, false when the UI runs in a plain browser during design work. */
export const inShell = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

interface DeckStore {
  deck: DeckState;
  settings: Settings;
  view: "panel" | "settings";
  setView: (view: "panel" | "settings") => void;
  setTrayMode: (mode: TrayMode) => void;
  setTheme: (theme: Settings["theme"]) => void;
  start: () => Promise<void>;
}

const emptyDeck: DeckState = { providers: [], updatedAt: new Date(0).toISOString(), scanning: true };

export const useDeck = create<DeckStore>((set, get) => ({
  deck: emptyDeck,
  settings: { trayMode: "glyph", theme: "system" },
  view: "panel",

  setView: (view) => set({ view }),

  setTrayMode: (trayMode) => {
    set({ settings: { ...get().settings, trayMode } });
    void send("set_tray_mode", { mode: trayMode });
  },

  setTheme: (theme) => {
    set({ settings: { ...get().settings, theme } });
    applyTheme(theme);
  },

  start: async () => {
    if (!inShell) {
      // Design work runs against a fixture so the panel can be built without the shell.
      set({ deck: demoDeck() });
      applyTheme(get().settings.theme);
      return;
    }

    const [{ invoke }, { listen }] = await Promise.all([
      import("@tauri-apps/api/core"),
      import("@tauri-apps/api/event"),
    ]);

    await listen<DeckState>("deck://state", (event) => set({ deck: event.payload }));

    const [deck, settings] = await Promise.all([
      invoke<DeckState>("current_state"),
      invoke<Settings>("current_settings"),
    ]);
    set({ deck, settings });
    applyTheme(settings.theme);
  },
}));

async function send(command: string, args: Record<string, unknown>): Promise<void> {
  if (!inShell) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke(command, args);
}

/** Chrome around the scrolling body: header plus footer plus the panel border. */
const PANEL_CHROME = 48 + 40 + 2;
let lastReportedHeight = 0;

/**
 * Ask the shell to size the window to its content. The backend clamps the result, so a
 * runaway measurement cannot produce a window taller than the screen.
 */
export async function reportPanelHeight(bodyHeight: number): Promise<void> {
  const height = Math.round(bodyHeight + PANEL_CHROME);
  // ResizeObserver fires on every sub-pixel change; only real movement is worth an IPC call.
  if (Math.abs(height - lastReportedHeight) < 2) return;
  lastReportedHeight = height;
  await send("set_panel_height", { height });
}

/**
 * `system` removes the attribute so the CSS media query decides. An explicit choice stamps
 * the root, which wins over the query in both directions.
 */
export function applyTheme(theme: Settings["theme"]): void {
  const root = document.documentElement;
  if (theme === "system") root.removeAttribute("data-theme");
  else root.setAttribute("data-theme", theme);
}
