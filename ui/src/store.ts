import { create } from "zustand";

import { demoDeck, demoHistory, demoPlans, demoStatusline } from "./demo";
import type {
  DeckState,
  ProviderHistory,
  ProviderId,
  ProviderPlans,
  Settings,
  StatuslineState,
  TrayMode,
} from "./types";
import { thresholdsFor } from "./types";

/** True inside the Tauri shell, false when the UI runs in a plain browser during design work. */
export const inShell = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

interface DeckStore {
  deck: DeckState;
  /**
   * Hourly history, pulled rather than pushed. The panel never renders it; only the dashboard
   * asks, and it refreshes on the same event the snapshots arrive on.
   */
  history: ProviderHistory[];
  settings: Settings;
  /** Tiers each provider declared, fetched once. Empty until the shell answers. */
  plans: ProviderPlans[];
  statusline: StatuslineState | null;
  /** Set when an install or revert failed, so the panel can say what went wrong. */
  statuslineError: string | null;
  view: "panel" | "settings";
  setView: (view: "panel" | "settings") => void;
  setTrayMode: (mode: TrayMode) => void;
  setTheme: (theme: Settings["theme"]) => void;
  setPlan: (provider: ProviderId, planId: string | null) => void;
  toggleThreshold: (provider: ProviderId, threshold: number) => void;
  /** `null` lifts the mute; otherwise the number of minutes to stay quiet for. */
  setMute: (minutes: number | null) => void;
  installStatusline: () => Promise<void>;
  revertStatusline: () => Promise<void>;
  openDashboard: () => Promise<void>;
  start: () => Promise<void>;
}

const emptyDeck: DeckState = { providers: [], updatedAt: new Date(0).toISOString(), scanning: true };

export const useDeck = create<DeckStore>((set, get) => ({
  deck: emptyDeck,
  history: [],
  settings: { trayMode: "glyph", theme: "system", plans: {}, alerts: {}, mutedUntil: null },
  plans: [],
  statusline: null,
  statuslineError: null,
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

  setPlan: (provider, planId) => {
    const plans = { ...get().settings.plans };
    if (planId === null) delete plans[provider];
    else plans[provider] = planId;
    set({ settings: { ...get().settings, plans } });
    void send("set_plan", { provider, planId });
  },

  toggleThreshold: (provider, threshold) => {
    const current = thresholdsFor(get().settings, provider);
    const next = current.includes(threshold)
      ? current.filter((value) => value !== threshold)
      : [...current, threshold].sort((a, b) => a - b);
    set({ settings: { ...get().settings, alerts: { ...get().settings.alerts, [provider]: next } } });
    void send("set_alert_thresholds", { provider, thresholds: next });
  },

  setMute: (minutes) => {
    // The instant is computed in the backend from this duration. "Until the end of today" is
    // a question about the viewer's zone, and only this side knows it.
    const mutedUntil =
      minutes === null ? null : new Date(Date.now() + minutes * 60_000).toISOString();
    set({ settings: { ...get().settings, mutedUntil } });
    void send("set_mute", { minutes });
  },

  installStatusline: async () => {
    set({ statuslineError: null });
    const next = await call<StatuslineState>("install_statusline", {});
    if (next.ok) set({ statusline: next.value });
    else set({ statuslineError: next.error });
  },

  revertStatusline: async () => {
    set({ statuslineError: null });
    const next = await call<StatuslineState>("revert_statusline", {});
    if (next.ok) set({ statusline: next.value });
    else set({ statuslineError: next.error });
  },

  openDashboard: async () => {
    await send("open_dashboard", {});
  },

  start: async () => {
    if (!inShell) {
      // Design work runs against a fixture so the panel can be built without the shell.
      set({
        deck: demoDeck(),
        history: demoHistory(),
        plans: demoPlans(),
        statusline: demoStatusline(),
      });
      applyTheme(get().settings.theme);
      return;
    }

    const [{ invoke }, { listen }] = await Promise.all([
      import("@tauri-apps/api/core"),
      import("@tauri-apps/api/event"),
    ]);

    // History is folded on the same pass that produces the snapshots, so the event that
    // announces one is also the moment the other is worth re-reading.
    await listen<DeckState>("deck://state", (event) => {
      set({ deck: event.payload });
      void invoke<ProviderHistory[]>("usage_history").then((history) => set({ history }));
    });

    const [deck, history, settings, plans, statusline] = await Promise.all([
      invoke<DeckState>("current_state"),
      invoke<ProviderHistory[]>("usage_history"),
      invoke<Settings>("current_settings"),
      invoke<ProviderPlans[]>("provider_plans"),
      invoke<StatuslineState>("statusline_state"),
    ]);
    set({ deck, history, settings, plans, statusline });
    applyTheme(settings.theme);
  },
}));

async function send(command: string, args: Record<string, unknown>): Promise<void> {
  if (!inShell) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke(command, args);
}

type Outcome<T> = { ok: true; value: T } | { ok: false; error: string };

/**
 * Invoke a command that can fail. The backend returns the real message — a path, a permission,
 * a malformed settings file — and it is surfaced rather than replaced with "something failed".
 */
async function call<T>(command: string, args: Record<string, unknown>): Promise<Outcome<T>> {
  if (!inShell) return { ok: false, error: "not running inside the app" };
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return { ok: true, value: await invoke<T>(command, args) };
  } catch (error) {
    return { ok: false, error: String(error) };
  }
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
