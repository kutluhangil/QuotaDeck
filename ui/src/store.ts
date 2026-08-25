import { create } from "zustand";

import { demoDeck, demoHistory, demoPlans, demoStatusline } from "./demo";
import { catalogueFor, languageFor, type Catalogue } from "./i18n";
import { hostPlatform } from "./platform";
import { completeRefresh } from "./refresh";
import { applyProviderPolicy, catalogueForPolicy, providerPolicySettings } from "./providerPolicy";
import type {
  AccessState,
  DeckState,
  ExportFormat,
  HistoryRange,
  Locale,
  ProviderHistory,
  ProviderDescriptor,
  ProviderId,
  ProviderPolicyOutcome,
  RefreshReceipt,
  ProviderPlans,
  PreparedExport,
  RetentionDays,
  Settings,
  StartupState,
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
  providerCatalogue: ProviderDescriptor[];
  statusline: StatuslineState | null;
  /** Set when an install or revert failed, so the panel can say what went wrong. */
  statuslineError: string | null;
  statuslineAction: "install" | "revert" | "refresh" | null;
  startup: StartupState;
  startupError: string | null;
  startupBusy: boolean;
  settingsError: string | null;
  settingsAction: string | null;
  exportBusy: boolean;
  exportError: string | null;
  exportMessage: string | null;
  /** A shell/window command failed outside a settings transaction. */
  shellError: string | null;
  refreshBusy: boolean;
  refreshRequest: number | null;
  refreshError: string | null;
  /** What we may read. Null until the shell answers; a browser build needs no grant. */
  access: AccessState | null;
  view: "panel" | "settings";
  /**
   * Which tool the panel is narrowed to. Deliberately not persisted: it is a glance-level
   * choice, and a panel that opened tomorrow still hiding two of three tools would read as a
   * panel that had lost them.
   */
  filter: ProviderId | "all";
  setView: (view: "panel" | "settings") => void;
  setFilter: (filter: ProviderId | "all") => void;
  setTrayMode: (mode: TrayMode) => void;
  setTheme: (theme: Settings["theme"]) => void;
  setLocale: (locale: Locale) => void;
  setDemo: (demo: boolean) => void;
  setRetentionDays: (days: RetentionDays) => Promise<void>;
  setStartup: (enabled: boolean) => void;
  /** Open the system folder panel. Resolves once the user has answered it. */
  requestAccess: () => Promise<void>;
  forgetAccess: () => Promise<void>;
  setPlan: (provider: ProviderId, planId: string | null) => void;
  toggleThreshold: (provider: ProviderId, threshold: number) => void;
  setProviderEnabled: (provider: ProviderId, enabled: boolean) => Promise<void>;
  moveProvider: (provider: ProviderId, direction: -1 | 1) => Promise<void>;
  /** `null` lifts the mute; otherwise the number of minutes to stay quiet for. */
  setMute: (minutes: number | null) => void;
  installStatusline: () => Promise<void>;
  revertStatusline: () => Promise<void>;
  refreshStatusline: () => Promise<void>;
  prepareManualStatusline: () => Promise<boolean>;
  openDashboard: () => Promise<void>;
  refreshNow: () => Promise<void>;
  copyUsageExport: (
    format: ExportFormat,
    range: HistoryRange,
    provider: ProviderId | null,
  ) => Promise<void>;
  /** Dismiss the popover from the keyboard, the way clicking away already does. */
  hidePanel: () => Promise<void>;
  /** End the process. With no dock icon the tray menu was the only way out. */
  quit: () => Promise<void>;
  start: () => Promise<void>;
}

const emptyDeck: DeckState = {
  providers: [],
  health: [],
  updatedAt: new Date(0).toISOString(),
  scanning: true,
  refreshing: false,
  refreshGeneration: 0,
  refreshError: null,
  retention: { requestedDays: 32, effectiveDays: 32, rebuilding: false, error: null },
};

export const useDeck = create<DeckStore>((set, get) => ({
  deck: emptyDeck,
  history: [],
  settings: {
    trayMode: "glyph",
    theme: "system",
    locale: "system",
    plans: {},
    alerts: {},
    mutedUntil: null,
    demo: false,
    disabledProviders: [],
    providerOrder: ["claude-code", "codex", "copilot-cli"],
    retentionDays: 32,
  },
  plans: [],
  providerCatalogue: [],
  statusline: null,
  statuslineError: null,
  statuslineAction: null,
  startup: { supported: false, enabled: false },
  startupError: null,
  startupBusy: false,
  settingsError: null,
  settingsAction: null,
  exportBusy: false,
  exportError: null,
  exportMessage: null,
  shellError: null,
  refreshBusy: false,
  refreshRequest: null,
  refreshError: null,
  access: null,
  view: "panel",
  filter: "all",

  setView: (view) => set({ view }),

  setFilter: (filter) => set({ filter }),

  setTrayMode: (trayMode) => {
    if (!inShell) {
      set({ settings: { ...get().settings, trayMode } });
      return;
    }
    if (get().settingsAction !== null) return;
    set({ settingsError: null, settingsAction: "tray-mode" });
    void call<Settings>("set_tray_mode", { mode: trayMode }).then((next) => {
      if (next.ok) set({ settings: next.value, settingsAction: null });
      else set({ settingsError: next.error, settingsAction: null });
    });
  },

  setTheme: (theme) => {
    if (!inShell) {
      set({ settings: { ...get().settings, theme } });
      applyTheme(theme);
      return;
    }
    if (get().settingsAction !== null) return;
    set({ settingsError: null, settingsAction: "theme" });
    void call<Settings>("set_theme", { theme }).then((next) => {
      if (next.ok) {
        set({ settings: next.value, settingsAction: null });
        applyTheme(next.value.theme);
      } else set({ settingsError: next.error, settingsAction: null });
    });
  },

  setLocale: (locale) => {
    if (!inShell) {
      set({ settings: { ...get().settings, locale } });
      applyLocale(locale);
      return;
    }
    if (get().settingsAction !== null) return;
    set({ settingsError: null, settingsAction: "locale" });
    // The backend keeps its own copy: notifications are raised from the read loop, which runs
    // whether or not this panel has ever been opened.
    void call<Settings>("set_locale", { locale }).then((next) => {
      if (next.ok) {
        set({ settings: next.value, settingsAction: null });
        applyLocale(next.value.locale);
      } else set({ settingsError: next.error, settingsAction: null });
    });
  },

  setRetentionDays: async (retentionDays) => {
    if (get().settingsAction !== null || get().deck.retention.rebuilding) return;
    if (!inShell) {
      set({ settings: { ...get().settings, retentionDays } });
      return;
    }
    set({ settingsError: null, settingsAction: "retention" });
    const next = await call<Settings>("set_retention_days", { retentionDays });
    if (next.ok) set({ settings: next.value, settingsAction: null });
    else set({ settingsError: next.error, settingsAction: null });
  },

  setPlan: (provider, planId) => {
    const plans = { ...get().settings.plans };
    if (planId === null) delete plans[provider];
    else plans[provider] = planId;
    if (!inShell) {
      set({ settings: { ...get().settings, plans } });
      return;
    }
    if (get().settingsAction !== null) return;
    set({ settingsError: null, settingsAction: "plan" });
    void call<Settings>("set_plan", { provider, planId }).then((next) => {
      if (next.ok) set({ settings: next.value, settingsAction: null });
      else set({ settingsError: next.error, settingsAction: null });
    });
  },

  toggleThreshold: (provider, threshold) => {
    const current = thresholdsFor(get().settings, provider);
    const next = current.includes(threshold)
      ? current.filter((value) => value !== threshold)
      : [...current, threshold].sort((a, b) => a - b);
    if (!inShell) {
      set({
        settings: { ...get().settings, alerts: { ...get().settings.alerts, [provider]: next } },
      });
      return;
    }
    if (get().settingsAction !== null) return;
    set({ settingsError: null, settingsAction: "threshold" });
    void call<Settings>("set_alert_thresholds", { provider, thresholds: next }).then((outcome) => {
      if (outcome.ok) set({ settings: outcome.value, settingsAction: null });
      else set({ settingsError: outcome.error, settingsAction: null });
    });
  },

  setProviderEnabled: async (provider, enabled) => {
    if (get().settingsAction !== null) return;
    const previous = get().settings;
    const disabledProviders = enabled
      ? previous.disabledProviders.filter((id) => id !== provider)
      : [...previous.disabledProviders.filter((id) => id !== provider), provider];
    const optimistic = providerPolicySettings(
      previous,
      disabledProviders,
      previous.providerOrder,
    );
    const previousCatalogue = get().providerCatalogue;
    const previousDeck = get().deck;
    const previousHistory = get().history;
    set({
      settings: optimistic,
      providerCatalogue: catalogueForPolicy(previousCatalogue, optimistic),
      deck: {
        ...previousDeck,
        providers: previousDeck.providers.filter((snapshot) => enabled || snapshot.id !== provider),
      },
      history: previousHistory.filter((entry) => enabled || entry.id !== provider),
      settingsError: null,
      settingsAction: "provider-policy",
    });
    if (!inShell) {
      set({ settingsAction: null });
      return;
    }
    const outcome = await applyProviderPolicy(previous, optimistic, async (pending) => {
      const saved = await call<ProviderPolicyOutcome>("set_provider_policy", {
        disabledProviders: pending.disabledProviders,
        providerOrder: pending.providerOrder,
      });
      if (!saved.ok) throw new Error(saved.error);
      return saved.value;
    });
    set({
      settings: outcome.settings,
      providerCatalogue:
        outcome.persisted
          ? catalogueForPolicy(previousCatalogue, outcome.settings)
          : previousCatalogue,
      deck: outcome.persisted ? get().deck : previousDeck,
      history: outcome.persisted ? get().history : previousHistory,
      settingsError: outcome.error,
      settingsAction: null,
    });
  },

  moveProvider: async (provider, direction) => {
    if (get().settingsAction !== null) return;
    const previous = get().settings;
    const from = previous.providerOrder.indexOf(provider);
    const to = from + direction;
    if (from < 0 || to < 0 || to >= previous.providerOrder.length) return;
    const providerOrder = [...previous.providerOrder];
    [providerOrder[from], providerOrder[to]] = [providerOrder[to]!, providerOrder[from]!];
    const optimistic = providerPolicySettings(
      previous,
      previous.disabledProviders,
      providerOrder,
    );
    const previousCatalogue = get().providerCatalogue;
    const previousDeck = get().deck;
    const previousHistory = get().history;
    const positions = new Map(providerOrder.map((id, index) => [id, index]));
    set({
      settings: optimistic,
      providerCatalogue: catalogueForPolicy(previousCatalogue, optimistic),
      deck: {
        ...previousDeck,
        providers: [...previousDeck.providers].sort(
          (left, right) => (positions.get(left.id) ?? 0) - (positions.get(right.id) ?? 0),
        ),
      },
      history: [...previousHistory].sort(
        (left, right) => (positions.get(left.id) ?? 0) - (positions.get(right.id) ?? 0),
      ),
      settingsError: null,
      settingsAction: "provider-policy",
    });
    if (!inShell) {
      set({ settingsAction: null });
      return;
    }
    const outcome = await applyProviderPolicy(previous, optimistic, async (pending) => {
      const saved = await call<ProviderPolicyOutcome>("set_provider_policy", {
        disabledProviders: pending.disabledProviders,
        providerOrder: pending.providerOrder,
      });
      if (!saved.ok) throw new Error(saved.error);
      return saved.value;
    });
    set({
      settings: outcome.settings,
      providerCatalogue:
        outcome.persisted
          ? catalogueForPolicy(previousCatalogue, outcome.settings)
          : previousCatalogue,
      deck: outcome.persisted ? get().deck : previousDeck,
      history: outcome.persisted ? get().history : previousHistory,
      settingsError: outcome.error,
      settingsAction: null,
    });
  },

  setMute: (minutes) => {
    // The instant is computed in the backend from this duration. "Until the end of today" is
    // a question about the viewer's zone, and only this side knows it.
    if (!inShell) {
      const mutedUntil =
        minutes === null ? null : new Date(Date.now() + minutes * 60_000).toISOString();
      set({ settings: { ...get().settings, mutedUntil } });
      return;
    }
    if (get().settingsAction !== null) return;
    set({ settingsError: null, settingsAction: "mute" });
    void call<Settings>("set_mute", { minutes }).then((next) => {
      if (next.ok) set({ settings: next.value, settingsAction: null });
      else set({ settingsError: next.error, settingsAction: null });
    });
  },

  installStatusline: async () => {
    set({ statuslineError: null, statuslineAction: "install" });
    const next = await call<StatuslineState>("install_statusline", {});
    if (next.ok) set({ statusline: next.value, statuslineAction: null });
    else set({ statuslineError: next.error, statuslineAction: null });
  },

  revertStatusline: async () => {
    set({ statuslineError: null, statuslineAction: "revert" });
    const next = await call<StatuslineState>("revert_statusline", {});
    if (next.ok) set({ statusline: next.value, statuslineAction: null });
    else set({ statuslineError: next.error, statuslineAction: null });
  },

  refreshStatusline: async () => {
    set({ statuslineError: null, statuslineAction: "refresh" });
    const next = await call<StatuslineState>("statusline_state", {});
    if (next.ok) set({ statusline: next.value, statuslineAction: null });
    else set({ statuslineError: next.error, statuslineAction: null });
  },

  prepareManualStatusline: async () => {
    set({ statuslineError: null, statuslineAction: "install" });
    const next = await call<StatuslineState>("prepare_manual_statusline", {});
    if (next.ok) {
      set({ statusline: next.value, statuslineAction: null });
      return true;
    }
    set({ statuslineError: next.error, statuslineAction: null });
    return false;
  },

  openDashboard: async () => {
    try {
      await send("open_dashboard", {});
      set({ shellError: null });
    } catch (error) {
      reportShellError("open dashboard", error);
    }
  },

  refreshNow: async () => {
    if (get().refreshBusy) return;
    if (!inShell) {
      set({ refreshBusy: true, refreshError: null });
      const generation = get().deck.refreshGeneration + 1;
      set({
        deck: { ...get().deck, refreshGeneration: generation, refreshing: false },
        refreshBusy: false,
      });
      return;
    }
    set({ refreshBusy: true, refreshError: null });
    const queued = await call<RefreshReceipt>("refresh_now", {});
    if (!queued.ok) {
      set({ refreshBusy: false, refreshRequest: null, refreshError: queued.error });
      return;
    }
    const completion = completeRefresh(queued.value.requestId, get().deck);
    set({
      refreshBusy: completion.pendingRequest !== null,
      refreshRequest: completion.pendingRequest,
      refreshError: completion.error,
    });
  },

  copyUsageExport: async (format, range, provider) => {
    if (get().exportBusy) return;
    set({ exportBusy: true, exportError: null, exportMessage: null });
    const prepared = await call<PreparedExport>("prepare_usage_export", {
      request: { format, range, provider },
    });
    if (!prepared.ok) {
      set({ exportBusy: false, exportError: `prepare export: ${prepared.error}` });
      return;
    }
    try {
      await navigator.clipboard.writeText(prepared.value.text);
      set({
        exportBusy: false,
        exportMessage: `${format.toUpperCase()} copied: ${prepared.value.rows} rows`,
      });
    } catch (error) {
      set({ exportBusy: false, exportError: `copy export: ${String(error)}` });
    }
  },

  hidePanel: async () => {
    try {
      await send("hide_panel", {});
      set({ shellError: null });
    } catch (error) {
      reportShellError("hide panel", error);
    }
  },

  quit: async () => {
    try {
      await send("quit_app", {});
      set({ shellError: null });
    } catch (error) {
      reportShellError("quit app", error);
    }
  },

  setDemo: (demo) => {
    if (!inShell) {
      set({ settings: { ...get().settings, demo } });
      return;
    }
    if (get().settingsAction !== null) return;
    set({ settingsError: null, settingsAction: "demo" });
    void call<Settings>("set_demo", { demo }).then((next) => {
      if (next.ok) set({ settings: next.value, settingsAction: null });
      else set({ settingsError: next.error, settingsAction: null });
    });
  },

  setStartup: (enabled) => {
    if (!inShell || !get().startup.supported || get().startupBusy) return;
    set({ startupError: null, startupBusy: true });
    void call<StartupState>("set_startup", { enabled }).then((next) => {
      if (next.ok) set({ startup: next.value, startupBusy: false });
      else set({ startupError: next.error, startupBusy: false });
    });
  },

  requestAccess: async () => {
    const next = await call<AccessState>("request_access", {});
    // The command answers with the state either way; a cancelled panel is a person who has
    // not decided, not a failure, and it comes back as the unchanged state.
    if (next.ok) {
      set({ access: next.value });
      if (next.value.granted) await get().refreshStatusline();
    } else set({ access: { ...blankAccess(), error: next.error } });
  },

  forgetAccess: async () => {
    const next = await call<AccessState>("forget_access", {});
    if (next.ok) set({ access: next.value, shellError: null });
    else reportShellError("forget folder access", next.error);
  },

  start: async () => {
    if (!inShell) {
      // Design work runs against a fixture so the panel can be built without the shell.
      set({
        deck: demoDeck(),
        history: demoHistory(),
        plans: demoPlans(),
        providerCatalogue: [
          { id: "claude-code", displayName: "Claude Code", supportsMeasured: true, enabled: true },
          { id: "codex", displayName: "Codex", supportsMeasured: true, enabled: true },
          { id: "copilot-cli", displayName: "Copilot CLI", supportsMeasured: false, enabled: true },
        ],
        statusline: demoStatusline(),
        // A browser has nothing to grant, and leaving this null would hold the panel on a
        // screen asking for a permission that does not exist here.
        access: { required: false, granted: true, path: null, error: null },
      });
      applyTheme(get().settings.theme);
      applyLocale(get().settings.locale);
      return;
    }

    const [{ invoke }, { listen }] = await Promise.all([
      import("@tauri-apps/api/core"),
      import("@tauri-apps/api/event"),
    ]);

    // History is folded on the same pass that produces the snapshots, so the event that
    // announces one is also the moment the other is worth re-reading.
    await listen<DeckState>("deck://state", (event) => {
      const completion = completeRefresh(get().refreshRequest, event.payload);
      set({
        deck: event.payload,
        refreshBusy: completion.pendingRequest !== null,
        refreshRequest: completion.pendingRequest,
        refreshError: completion.error,
      });
      void invoke<ProviderHistory[]>("usage_history")
        .then((history) => set({ history }))
        .catch((error) => reportShellError("refresh usage history", error));
    });

    const [deck, history, settings, providerCatalogue, plans, access, startup] = await Promise.all([
      invoke<DeckState>("current_state"),
      invoke<ProviderHistory[]>("usage_history"),
      invoke<Settings>("current_settings"),
      invoke<ProviderDescriptor[]>("provider_catalogue"),
      invoke<ProviderPlans[]>("provider_plans"),
      invoke<AccessState>("access_state"),
      call<StartupState>("startup_state", {}),
    ]);
    const statusline = await call<StatuslineState>("statusline_state", {});
    set({
      deck,
      history,
      settings,
      providerCatalogue,
      plans,
      statusline: statusline.ok ? statusline.value : unavailableStatusline(),
      statuslineError: statusline.ok ? null : statusline.error,
      startup:
        startup.ok
          ? startup.value
          : { supported: hostPlatform() === "windows", enabled: false },
      startupError: startup.ok ? null : startup.error,
      access,
    });
    applyTheme(settings.theme);
    applyLocale(settings.locale);
  },
}));

/*
 * A handle on the store during development, so states that only the shell can produce — a
 * missing folder grant, a failed one — can be put on screen and looked at in a browser.
 *
 * `import.meta.env.DEV` is a compile-time constant, so this whole block is removed from a
 * production build rather than merely skipped at runtime.
 */
if (import.meta.env.DEV && typeof window !== "undefined") {
  (window as unknown as Record<string, unknown>)["__quotadeckStore"] = useDeck;
}

/** Nothing granted and nothing known. The shape the panel renders its onboarding from. */
function blankAccess(): AccessState {
  return { required: true, granted: false, path: null, error: null };
}

function unavailableStatusline(): StatuslineState {
  return {
    setupMode: "unavailable",
    installed: false,
    settingsPath: null,
    currentStatusLine: null,
    currentCommand: null,
    proposedStatusLine: null,
    proposedCommand: null,
    previousCommand: null,
    previousStatusLine: null,
    manualRevertMode: null,
    readings: 0,
    lastReadingAt: null,
  };
}

/**
 * The sample deck, built once.
 *
 * A fresh object per call would hand zustand a new snapshot on every render and the panel would
 * never settle — the same trap the `historyFor` selector fell into in Phase 7.
 */
const sample = {
  deck: demoDeck(),
  history: demoHistory(),
};

/**
 * What the surfaces render: this machine, or the sample when it was asked for.
 *
 * The switch lives here rather than in each component so there is exactly one place where a
 * real reading can be replaced by an invented one.
 */
export function useDeckState(): DeckState {
  return useDeck((state) => (state.settings.demo ? sample.deck : state.deck));
}

export function useHistory(): ProviderHistory[] {
  return useDeck((state) => (state.settings.demo ? sample.history : state.history));
}

/**
 * The catalogue the panel is currently reading from.
 *
 * A selector rather than a module-level global, so a language change re-renders every
 * component that shows copy. The catalogues are module constants, so the returned reference is
 * stable and zustand does not see a new snapshot on every render.
 */
export function useStrings(): Catalogue {
  return useDeck((state) => catalogueFor(state.settings.locale));
}

/** The locale the number and date formatters should follow. */
export function useLocale(): Locale {
  return useDeck((state) => state.settings.locale);
}

async function send(command: string, args: Record<string, unknown>): Promise<void> {
  if (!inShell) return;
  const { invoke } = await import("@tauri-apps/api/core");
  await invoke(command, args);
}

function reportShellError(action: string, error: unknown): void {
  const message = `${action}: ${String(error)}`;
  console.error(message);
  useDeck.setState({ shellError: message });
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
  try {
    await send("set_panel_height", { height });
    lastReportedHeight = height;
    if (useDeck.getState().shellError?.startsWith("resize panel:") === true) {
      useDeck.setState({ shellError: null });
    }
  } catch (error) {
    reportShellError("resize panel", error);
  }
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

/**
 * Stamp the resolved language on the document.
 *
 * This is what a screen reader reads the pronunciation rules off; the same sentence in the
 * wrong `lang` is announced with the wrong phonemes and is genuinely hard to follow.
 */
export function applyLocale(locale: Locale): void {
  document.documentElement.lang = languageFor(locale);
}
