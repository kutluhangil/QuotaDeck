/**
 * Unified v2 Settings sheet & popover view (`Settings.dc.html` + full Quota Deck settings).
 * All controls use the native v2 design language (`.seg`, `.switch`, `.mark`, `.field`, `.btn`).
 */

import { useState } from "react";

import { formatClock, formatRelative } from "../format";
import type { Catalogue } from "../i18n";
import { hostPlatform } from "../platform";
import { addRoot, MAX_ADDITIONAL_ROOTS, removeRoot, type AddRootFailure } from "../roots";
import { call, useDeck, useLocale, useStrings } from "../store";
import {
  DEFAULT_THRESHOLDS,
  thresholdsFor,
  type Locale,
  type ProviderId,
  type ProviderInstanceId,
  type ProviderPlans,
  type RetentionDays,
  type SystemAlertRules,
  type TrayMode,
} from "../types";
import { useLiquidStrings } from "./strings";
import { useSystem } from "./system";
import { Mark } from "./ui";

function Switch({
  on,
  label,
  disabled,
  onChange,
}: {
  on: boolean;
  label: string;
  disabled?: boolean;
  onChange: (on: boolean) => void;
}) {
  return (
    <button
      type="button"
      className="switch"
      role="switch"
      aria-checked={on}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!on)}
    />
  );
}

function Threshold({
  value,
  fallback,
  unit,
  label,
  onChange,
}: {
  value: number | null;
  fallback: number;
  unit: string;
  label: string;
  onChange: (value: number | null) => void;
}) {
  return (
    <>
      <label className="field">
        <span className="sr">{label}</span>
        <input
          inputMode="numeric"
          value={value ?? fallback}
          disabled={value === null}
          onChange={(event) => {
            const next = Number.parseInt(event.target.value, 10);
            if (Number.isFinite(next) && next > 0 && next < 10_000) onChange(next);
          }}
        />
        <span className="t3">{unit}</span>
      </label>
      <Switch on={value !== null} label={label} onChange={(on) => onChange(on ? fallback : null)} />
    </>
  );
}

const CARD_BOX = {
  borderRadius: 14,
  background: "var(--elev-2)",
  boxShadow: "inset 0 0 0 .5px var(--hair)",
} as const;

const ROW = { padding: "10px 14px", gap: 12, minHeight: 46 } as const;

const SHORT_MARK: Record<ProviderId, string> = {
  "claude-code": "Cl",
  codex: "Cx",
  antigravity: "Ag",
  "gemini-cli": "Gm",
  "copilot-cli": "Cp",
  opencode: "Oc",
  amp: "Am",
  droid: "Dr",
  "pi-agent": "Pi",
};

function ProviderMark({ provider, enabled }: { provider: ProviderId; enabled: boolean }) {
  if (provider === "claude-code" || provider === "codex" || provider === "antigravity") {
    return <Mark tool={provider} />;
  }
  return (
    <span className={`mark${enabled ? "" : " outline"}`} aria-hidden="true" style={enabled ? { background: "var(--elev-4)", color: "var(--fg)" } : undefined}>
      {SHORT_MARK[provider]}
    </span>
  );
}

function minutesUntilTomorrow(now: number): number {
  const midnight = new Date(now);
  midnight.setHours(24, 0, 0, 0);
  return Math.max(1, Math.round((midnight.getTime() - now) / 60_000));
}

function shortcutKeys(): string {
  return hostPlatform() === "macos" ? "⌘⌥Q" : "Ctrl+Alt+Q";
}

function rootRefusal(strings: Catalogue, reason: AddRootFailure): string {
  switch (reason) {
    case "empty":
      return strings.settings.rootsInvalidEmpty;
    case "relative":
      return strings.settings.rootsInvalidRelative;
    case "duplicate":
      return strings.settings.rootsInvalidDuplicate;
    case "too-many":
      return strings.settings.rootsInvalidTooMany(MAX_ADDITIONAL_ROOTS);
  }
}

const EMPTY_ROOTS: string[] = [];
const RETENTION_OPTIONS: RetentionDays[] = [32, 90, 365];

function PlanRow({
  instance,
  entry,
  name,
  busy,
}: {
  instance: ProviderInstanceId;
  entry: ProviderPlans;
  name: string;
  busy: boolean;
}) {
  const catalogue = useStrings();
  const chosen = useDeck((state) => state.settings.plans[instance]);
  const setPlan = useDeck((state) => state.setPlan);
  return (
    <div style={{ padding: "10px 14px", display: "flex", flexDirection: "column", gap: 6 }}>
      <div className="row" style={{ justifyContent: "space-between", flexWrap: "wrap", gap: 8 }}>
        <span style={{ fontWeight: 500 }}>{catalogue.settings.planTitle(name)}</span>
        <div className="seg" role="group" aria-label={catalogue.settings.planTitle(name)}>
          <button
            type="button"
            className={`tab${chosen === undefined ? " on" : ""}`}
            aria-pressed={chosen === undefined}
            disabled={busy}
            onClick={() => setPlan(instance, null)}
          >
            {catalogue.settings.planNone}
          </button>
          {entry.plans.map((plan) => (
            <button
              key={plan.id}
              type="button"
              className={`tab${chosen === plan.id ? " on" : ""}`}
              aria-pressed={chosen === plan.id}
              disabled={busy}
              onClick={() => setPlan(instance, plan.id)}
            >
              {plan.label}
            </button>
          ))}
        </div>
      </div>
      <span className="lbl">
        {chosen === undefined ? catalogue.settings.planNoneHint : catalogue.settings.planHint(entry.provider)}
      </span>
    </div>
  );
}

function LogFoldersSection({
  provider,
  displayName,
  busy,
}: {
  provider: ProviderInstanceId;
  displayName: string;
  busy: boolean;
}) {
  const catalogue = useStrings();
  const supported = useDeck((state) => state.additionalRootsSupported);
  const roots = useDeck((state) => state.settings.additionalRoots[provider] ?? EMPTY_ROOTS);
  const setAdditionalRoots = useDeck((state) => state.setAdditionalRoots);
  const [draft, setDraft] = useState("");
  const [refusal, setRefusal] = useState<AddRootFailure | null>(null);

  if (!supported) return null;

  function add() {
    const outcome = addRoot(roots, draft);
    if (!outcome.ok) {
      setRefusal(outcome.reason);
      return;
    }
    setRefusal(null);
    setDraft("");
    void setAdditionalRoots(provider, outcome.roots);
  }

  return (
    <div style={{ padding: "10px 14px", display: "flex", flexDirection: "column", gap: 8 }}>
      <span style={{ fontWeight: 500 }}>{catalogue.settings.rootsTitle(displayName)}</span>
      <span className="lbl">{catalogue.settings.rootsHint}</span>
      {roots.length > 0 && (
        <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
          {roots.map((root) => (
            <div key={root} className="row" style={{ justifyContent: "space-between", background: "var(--elev-3)", padding: "4px 10px", borderRadius: 8 }}>
              <span className="mono" style={{ fontSize: 11.5, overflow: "hidden", textOverflow: "ellipsis" }}>{root}</span>
              <button
                type="button"
                className="btn icon ghost"
                style={{ width: 22, height: 22 }}
                disabled={busy}
                aria-label={catalogue.settings.rootsRemove(root)}
                onClick={() => void setAdditionalRoots(provider, removeRoot(roots, root))}
              >
                ×
              </button>
            </div>
          ))}
        </div>
      )}
      <div className="row" style={{ gap: 8 }}>
        <label className="search" style={{ flex: 1, minWidth: 0, background: "var(--elev-3)" }}>
          <input
            type="text"
            value={draft}
            placeholder={catalogue.settings.rootsPlaceholder}
            aria-label={catalogue.settings.rootsPlaceholder}
            spellCheck={false}
            disabled={busy}
            onChange={(event) => {
              setDraft(event.target.value);
              setRefusal(null);
            }}
            onKeyDown={(event) => {
              if (event.key !== "Enter") return;
              event.preventDefault();
              add();
            }}
          />
        </label>
        <button type="button" className="btn" disabled={busy} onClick={add}>
          {catalogue.settings.rootsAdd}
        </button>
      </div>
      {refusal !== null && (
        <span className="lbl" role="alert" style={{ color: "var(--bad)" }}>
          {rootRefusal(catalogue, refusal)}
        </span>
      )}
    </div>
  );
}

function StatuslineSection({ now }: { now: number }) {
  const catalogue = useStrings();
  const statusline = useDeck((state) => state.statusline);
  const error = useDeck((state) => state.statuslineError);
  const action = useDeck((state) => state.statuslineAction);
  const install = useDeck((state) => state.installStatusline);
  const revert = useDeck((state) => state.revertStatusline);
  const refresh = useDeck((state) => state.refreshStatusline);
  const prepareManual = useDeck((state) => state.prepareManualStatusline);
  const [copyMessage, setCopyMessage] = useState<string | null>(null);

  if (statusline === null) return null;

  async function copy(command: string | null) {
    setCopyMessage(null);
    if (command === null) return;
    try {
      await navigator.clipboard.writeText(command);
      setCopyMessage(catalogue.settings.statuslineCopied);
    } catch (copyError) {
      setCopyMessage(catalogue.settings.statuslineCopyFailed(String(copyError)));
    }
  }

  const busy = action !== null;
  const lastReading = formatRelative(statusline.lastReadingAt, now, catalogue);
  const reading =
    statusline.readings > 0 && lastReading
      ? catalogue.settings.statuslineReadings(statusline.readings, lastReading)
      : catalogue.settings.statuslineWaiting;
  const currentPreview =
    statusline.setupMode === "manual"
      ? statusline.currentStatusLine === null
        ? "—"
        : JSON.stringify(statusline.currentStatusLine, null, 2)
      : (statusline.currentCommand ?? "—");
  const proposedPreview =
    statusline.setupMode === "manual"
      ? statusline.proposedStatusLine === null
        ? "—"
        : JSON.stringify(statusline.proposedStatusLine, null, 2)
      : (statusline.proposedCommand ?? "—");

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      <span className="cap">{catalogue.settings.statuslineTitle}</span>
      <div style={{ ...CARD_BOX, padding: 14, display: "flex", flexDirection: "column", gap: 10 }}>
        {statusline.setupMode === "unavailable" ? (
          <>
            <span className="lbl">{catalogue.settings.statuslineUnsupported}</span>
            <div className="row">
              <button type="button" className="btn" disabled={busy} onClick={() => void refresh()}>
                {action === "refresh" ? catalogue.settings.statuslineRefreshing : catalogue.settings.statuslineRefresh}
              </button>
            </div>
          </>
        ) : (
          <>
            <span className="t2" style={{ fontSize: 12.5 }}>{catalogue.settings.statuslineBody}</span>
            {statusline.installed ? (
              <div className="row" style={{ justifyContent: "space-between", flexWrap: "wrap", gap: 8 }}>
                <span className="lbl" style={{ color: "var(--ok)" }}>{reading}</span>
                {statusline.setupMode === "automatic" ? (
                  <button type="button" className="btn" disabled={busy} onClick={() => void revert()}>
                    {action === "revert" ? catalogue.settings.statuslineReverting : catalogue.settings.statuslineRevert}
                  </button>
                ) : (
                  <button type="button" className="btn" disabled={busy} onClick={() => void refresh()}>
                    {action === "refresh" ? catalogue.settings.statuslineRefreshing : catalogue.settings.statuslineRefresh}
                  </button>
                )}
              </div>
            ) : (
              <>
                <div style={{ display: "grid", gridTemplateColumns: "auto 1fr", gap: "4px 10px", fontSize: 11.5, background: "var(--elev-3)", padding: "8px 10px", borderRadius: 8 }}>
                  <span className="lbl">{catalogue.settings.statuslineBefore}</span>
                  <code className="mono" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{currentPreview}</code>
                  <span className="lbl">{catalogue.settings.statuslineAfter}</span>
                  <code className="mono" style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{proposedPreview}</code>
                </div>
                {statusline.currentCommand !== null && <span className="lbl">{catalogue.settings.statuslineChains}</span>}
                <div className="row" style={{ gap: 8, flexWrap: "wrap" }}>
                  {statusline.setupMode === "automatic" ? (
                    <button type="button" className="btn primary" disabled={busy} onClick={() => void install()}>
                      {action === "install" ? catalogue.settings.statuslineConnecting : catalogue.settings.statuslineConnect}
                    </button>
                  ) : (
                    <>
                      <button
                        type="button"
                        className="btn primary"
                        disabled={busy || statusline.proposedStatusLine === null}
                        onClick={() =>
                          void (async () => {
                            if (await prepareManual()) await copy(proposedPreview);
                          })()
                        }
                      >
                        {catalogue.settings.statuslineCopyCommand}
                      </button>
                      <button type="button" className="btn" disabled={busy} onClick={() => void refresh()}>
                        {action === "refresh" ? catalogue.settings.statuslineRefreshing : catalogue.settings.statuslineRefresh}
                      </button>
                    </>
                  )}
                </div>
              </>
            )}
            {statusline.settingsPath !== null && <span className="lbl mono">{catalogue.settings.statuslineFile(statusline.settingsPath)}</span>}
          </>
        )}
        {copyMessage !== null && <span className="lbl">{copyMessage}</span>}
        {error !== null && (
          <span className="lbl" role="alert" style={{ color: "var(--bad)" }}>
            {catalogue.settings.statuslineFailed(error)}
          </span>
        )}
      </div>
    </div>
  );
}

export function LiquidSettings({ now }: { now: number }) {
  const strings = useLiquidStrings();
  const catalogue = useStrings();
  const locale = useLocale();
  const settings = useDeck((state) => state.settings);
  const settingsError = useDeck((state) => state.settingsError);
  const settingsBusy = useDeck((state) => state.settingsAction !== null);
  const retention = useDeck((state) => state.deck.retention);
  const providerCatalogue = useDeck((state) => state.providerCatalogue);
  const providers = useDeck((state) => state.deck.providers);
  const plans = useDeck((state) => state.plans);
  const startup = useDeck((state) => state.startup);
  const startupBusy = useDeck((state) => state.startupBusy);
  const access = useDeck((state) => state.access);

  const setDesignTheme = useDeck((state) => state.setDesignTheme);
  const setTheme = useDeck((state) => state.setTheme);
  const setLocale = useDeck((state) => state.setLocale);
  const setUsageDisplay = useDeck((state) => state.setUsageDisplay);
  const setTrayMode = useDeck((state) => state.setTrayMode);
  const setStartup = useDeck((state) => state.setStartup);
  const setGlobalShortcut = useDeck((state) => state.setGlobalShortcut);
  const setRetentionDays = useDeck((state) => state.setRetentionDays);
  const setDemo = useDeck((state) => state.setDemo);
  const setMute = useDeck((state) => state.setMute);
  const toggleThreshold = useDeck((state) => state.toggleThreshold);
  const setProviderEnabled = useDeck((state) => state.setProviderEnabled);
  const moveProvider = useDeck((state) => state.moveProvider);
  const addInstance = useDeck((state) => state.addInstance);
  const removeInstance = useDeck((state) => state.removeInstance);
  const requestAccess = useDeck((state) => state.requestAccess);
  const forgetAccess = useDeck((state) => state.forgetAccess);
  const setSystemAlerts = useDeck((state) => state.setSystemAlerts);
  const setAntigravityLive = useDeck((state) => state.setAntigravityLive);
  const setRefreshSpeed = useDeck((state) => state.setRefreshSpeed);
  const setMenuBar = useDeck((state) => state.setMenuBar);

  const monitored = useSystem((state) => state.snapshot !== null);
  const hasFans = useSystem((state) => (state.snapshot?.fans.length ?? 0) > 0);
  const liveAvailable = useSystem((state) => state.antigravity?.liveAvailable ?? false);
  const [fanRemoved, setFanRemoved] = useState(false);

  // Separate account creation state
  const tools = Array.from(new Set(providerCatalogue.map((entry) => entry.provider)));
  const [instProvider, setInstProvider] = useState<ProviderId | null>(null);
  const [instName, setInstName] = useState("");
  const [instLabel, setInstLabel] = useState("");
  const [instRefused, setInstRefused] = useState(false);
  const chosenTool = instProvider ?? tools[0];
  const namedInstances = providerCatalogue.filter((entry) => entry.id.includes("#"));

  const rules = settings.systemAlerts;
  const updateRules = (patch: Partial<SystemAlertRules>) => setSystemAlerts({ ...rules, ...patch });

  const mutedUntilMs = settings.mutedUntil === null ? null : Date.parse(settings.mutedUntil);
  const muted = mutedUntilMs !== null && !Number.isNaN(mutedUntilMs) && mutedUntilMs > now;
  const alertingProviders = providers.filter((snapshot) => snapshot.windows.length > 0);

  const removeFanHelper = async () => {
    const result = await call<{ state: string }>("remove_fan_helper", {});
    if (result.ok && result.value.state === "applied") {
      setFanRemoved(true);
    }
  };

  function handleAddInstance() {
    if (chosenTool === undefined) return;
    if (!/^[a-z0-9]+(-[a-z0-9]+)*$/.test(instName.trim())) {
      setInstRefused(true);
      return;
    }
    setInstRefused(false);
    const trimmed = instLabel.trim();
    void addInstance(chosenTool, instName.trim(), trimmed === "" ? null : trimmed).then(() => {
      setInstName("");
      setInstLabel("");
    });
  }

  const localeOptions: { locale: Locale; label: string }[] = [
    { locale: "system", label: catalogue.settings.languageSystem },
    { locale: "en", label: catalogue.settings.languageEnglish },
    { locale: "tr", label: catalogue.settings.languageTurkish },
    { locale: "de", label: catalogue.settings.languageGerman },
    { locale: "es", label: catalogue.settings.languageSpanish },
  ];

  const trayModes: { mode: TrayMode; label: string }[] = [
    { mode: "glyph", label: catalogue.settings.trayGlyph },
    { mode: "compact", label: catalogue.settings.trayCompact },
    { mode: "strip", label: catalogue.settings.trayStrip },
  ];

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 18, padding: 10 }} aria-busy={settingsBusy}>
      {settingsError !== null && (
        <p className="lbl" role="alert" style={{ color: "var(--bad)", margin: 0 }}>
          {catalogue.settings.settingsFailed(settingsError)}
        </p>
      )}

      {/* 1. Design, Appearance & Language */}
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <span className="cap">{strings.designTheme} &amp; {catalogue.settings.themeTitle}</span>
        <div style={CARD_BOX}>
          <div className="row" style={{ ...ROW, flexWrap: "wrap", justifyContent: "space-between" }}>
            <span>{strings.designTheme}</span>
            <div className="seg" role="group" aria-label={strings.designTheme}>
              {(["liquid", "claude"] as const).map((theme) => (
                <button
                  key={theme}
                  type="button"
                  className={`tab${settings.designTheme === theme ? " on" : ""}`}
                  aria-pressed={settings.designTheme === theme}
                  onClick={() => setDesignTheme(theme)}
                >
                  {strings.design[theme]}
                </button>
              ))}
            </div>
          </div>
          <div className="sep" style={{ marginLeft: 14 }} />
          <div className="row" style={{ ...ROW, flexWrap: "wrap", justifyContent: "space-between" }}>
            <span>{catalogue.settings.themeTitle}</span>
            <div className="seg" role="group" aria-label={catalogue.settings.themeTitle}>
              {(
                [
                  ["system", catalogue.settings.themeSystem],
                  ["dark", catalogue.settings.themeDark],
                  ["light", catalogue.settings.themeLight],
                ] as const
              ).map(([mode, label]) => (
                <button
                  key={mode}
                  type="button"
                  className={`tab${settings.theme === mode ? " on" : ""}`}
                  aria-pressed={settings.theme === mode}
                  disabled={settingsBusy}
                  onClick={() => setTheme(mode)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
          <div className="sep" style={{ marginLeft: 14 }} />
          <div className="row" style={{ ...ROW, flexWrap: "wrap", justifyContent: "space-between" }}>
            <span>{catalogue.settings.languageTitle}</span>
            <div className="seg" role="group" aria-label={catalogue.settings.languageTitle} style={{ flexWrap: "wrap" }}>
              {localeOptions.map(({ locale: loc, label }) => (
                <button
                  key={loc}
                  type="button"
                  lang={loc === "system" ? undefined : loc}
                  className={`tab${settings.locale === loc ? " on" : ""}`}
                  aria-pressed={settings.locale === loc}
                  disabled={settingsBusy}
                  onClick={() => setLocale(loc)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
          <div className="sep" style={{ marginLeft: 14 }} />
          <div className="row" style={{ ...ROW, flexWrap: "wrap", justifyContent: "space-between" }}>
            <span>{catalogue.settings.usageDisplayTitle}</span>
            <div className="seg" role="group" aria-label={catalogue.settings.usageDisplayTitle}>
              {(
                [
                  ["remaining", catalogue.settings.usageDisplayRemaining],
                  ["used", catalogue.settings.usageDisplayUsed],
                ] as const
              ).map(([display, label]) => (
                <button
                  key={display}
                  type="button"
                  className={`tab${settings.usageDisplay === display ? " on" : ""}`}
                  aria-pressed={settings.usageDisplay === display}
                  disabled={settingsBusy}
                  onClick={() => setUsageDisplay(display)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>
        </div>
      </div>

      {/* 2. Menu Bar, Startup & Shortcut (Unified as in Settings.dc.html) */}
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <span className="cap">{strings.menuBarShow}</span>
        <div style={CARD_BOX}>
          <div className="row" style={{ ...ROW, flexWrap: "wrap", justifyContent: "space-between" }}>
            <div style={{ display: "flex", gap: 8, flexWrap: "wrap", alignItems: "center" }}>
              {monitored && (
                <div className="seg" role="group" aria-label={strings.menuBarShow}>
                  {(["temperature", "memory"] as const).map((key) => (
                    <button
                      key={key}
                      type="button"
                      className={`tab${settings.menuBar[key] ? " on" : ""}`}
                      aria-pressed={settings.menuBar[key]}
                      disabled={settingsBusy}
                      onClick={() => setMenuBar({ ...settings.menuBar, [key]: !settings.menuBar[key] })}
                    >
                      {key === "temperature" ? strings.menuBarTemp : strings.memory}
                    </button>
                  ))}
                </div>
              )}
              <div className="seg" role="group" aria-label={catalogue.settings.trayTitle(hostPlatform())}>
                {trayModes.map(({ mode, label }) => (
                  <button
                    key={mode}
                    type="button"
                    className={`tab${settings.trayMode === mode ? " on" : ""}`}
                    aria-pressed={settings.trayMode === mode}
                    disabled={settingsBusy}
                    onClick={() => setTrayMode(mode)}
                  >
                    {label}
                  </button>
                ))}
              </div>
            </div>
          </div>
          {startup.supported && (
            <>
              <div className="sep" style={{ marginLeft: 14 }} />
              <div className="row" style={ROW}>
                <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 2 }}>
                  <span>{catalogue.settings.startupTitle}</span>
                  <span className="lbl">{catalogue.settings.startupHint}</span>
                </div>
                <Switch
                  on={startup.enabled}
                  label={catalogue.settings.startupTitle}
                  disabled={startupBusy}
                  onChange={setStartup}
                />
              </div>
            </>
          )}
          <div className="sep" style={{ marginLeft: 14 }} />
          <div className="row" style={ROW}>
            <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 2 }}>
              <span>{catalogue.settings.shortcutTitle} · <span className="mono">{shortcutKeys()}</span></span>
              <span className="lbl">{catalogue.settings.shortcutHint(shortcutKeys())}</span>
            </div>
            <Switch
              on={settings.globalShortcut}
              label={catalogue.settings.shortcutTitle}
              disabled={settingsBusy}
              onChange={setGlobalShortcut}
            />
          </div>
        </div>
      </div>

      {/* 3. Refresh Rate */}
      {monitored && (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <span className="cap">{strings.refreshRate}</span>
          <div className="seg" role="group" aria-label={strings.refreshRate} style={{ alignSelf: "flex-start" }}>
            {(["live", "balanced", "saver"] as const).map((speed) => (
              <button
                key={speed}
                type="button"
                className={`tab${settings.refreshSpeed === speed ? " on" : ""}`}
                aria-pressed={settings.refreshSpeed === speed}
                onClick={() => setRefreshSpeed(speed)}
              >
                {strings.refreshSpeed[speed]}
              </button>
            ))}
          </div>
          <span className="lbl">{strings.refreshHint}</span>
        </div>
      )}

      {/* 4. Alerts (Mac + AI Quota + Mute) */}
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <span className="cap">{strings.alerts}</span>
        <div style={CARD_BOX}>
          {monitored && (
            <>
              <div className="row" style={ROW}>
                <span style={{ flex: 1 }}>{strings.cpuAbove}</span>
                <Threshold value={rules.cpuCelsiusAbove} fallback={90} unit="°C" label={strings.cpuAbove} onChange={(cpuCelsiusAbove) => updateRules({ cpuCelsiusAbove })} />
              </div>
              <div className="sep" style={{ marginLeft: 14 }} />
              <div className="row" style={ROW}>
                <span style={{ flex: 1 }}>{strings.diskBelow}</span>
                <Threshold value={rules.freeDiskGbBelow} fallback={20} unit="GB" label={strings.diskBelow} onChange={(freeDiskGbBelow) => updateRules({ freeDiskGbBelow })} />
              </div>
              <div className="sep" style={{ marginLeft: 14 }} />
              <div className="row" style={ROW}>
                <span style={{ flex: 1 }}>{strings.memCritical}</span>
                <Switch on={rules.memoryCritical} label={strings.memCritical} onChange={(memoryCritical) => updateRules({ memoryCritical })} />
              </div>
              <div className="sep" style={{ marginLeft: 14 }} />
              <div className="row" style={ROW}>
                <span style={{ flex: 1 }}>{strings.containerUnhealthy}</span>
                <Switch on={rules.containerUnhealthy} label={strings.containerUnhealthy} onChange={(containerUnhealthy) => updateRules({ containerUnhealthy })} />
              </div>
              <div className="sep" style={{ marginLeft: 14 }} />
            </>
          )}
          {alertingProviders.map((snapshot) => {
            const chosen = thresholdsFor(settings, snapshot.instance);
            const name = snapshot.label ?? catalogue.provider[snapshot.id];
            return (
              <div key={snapshot.instance}>
                <div className="row" style={{ ...ROW, flexWrap: "wrap", justifyContent: "space-between" }}>
                  <span>{name} · {catalogue.settings.alertsTitle}</span>
                  <div className="seg" role="group" aria-label={`${name} · ${catalogue.settings.alertsTitle}`}>
                    {DEFAULT_THRESHOLDS.map((threshold) => {
                      const active = chosen.includes(threshold);
                      return (
                        <button
                          key={threshold}
                          type="button"
                          className={`tab${active ? " on" : ""}`}
                          aria-pressed={active}
                          disabled={settingsBusy}
                          onClick={() => toggleThreshold(snapshot.instance, threshold)}
                        >
                          {catalogue.settings.alertsThreshold(threshold)}
                        </button>
                      );
                    })}
                  </div>
                </div>
                <div className="sep" style={{ marginLeft: 14 }} />
              </div>
            );
          })}
          <div className="row" style={{ ...ROW, flexWrap: "wrap", justifyContent: "space-between" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
              <span>{catalogue.settings.muteTitle}</span>
              <span className="lbl">
                {muted
                  ? catalogue.settings.mutedUntil(formatClock(settings.mutedUntil, locale) ?? "")
                  : catalogue.settings.alertsHint}
              </span>
            </div>
            <div className="row" style={{ gap: 6 }}>
              {muted ? (
                <button type="button" className="btn" disabled={settingsBusy} onClick={() => setMute(null)}>
                  {catalogue.settings.muteClear}
                </button>
              ) : (
                <>
                  <button type="button" className="btn" disabled={settingsBusy} onClick={() => setMute(60)}>
                    {catalogue.settings.muteHour}
                  </button>
                  <button type="button" className="btn" disabled={settingsBusy} onClick={() => setMute(minutesUntilTomorrow(now))}>
                    {catalogue.settings.muteToday}
                  </button>
                </>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* 5. Tools & Plans (`Settings.dc.html` Tools card) */}
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <span className="cap">{strings.tools}</span>
        <div style={CARD_BOX}>
          {providerCatalogue.map((provider, index) => (
            <div key={provider.id}>
              {index > 0 && <div className="sep" style={{ marginLeft: 50 }} />}
              <div className="row" style={{ padding: "8px 14px", gap: 12, minHeight: 46 }}>
                <ProviderMark provider={provider.provider} enabled={provider.enabled} />
                <span style={{ flex: 1, fontWeight: provider.enabled ? 500 : 400 }} className={provider.enabled ? undefined : "t2"}>
                  {provider.label ? `${provider.label} · ${provider.displayName}` : provider.displayName}
                </span>
                <div className="row" style={{ gap: 2 }}>
                  <button
                    type="button"
                    className="btn icon ghost"
                    style={{ width: 24, height: 24 }}
                    disabled={settingsBusy || index === 0}
                    aria-label={catalogue.settings.providerUp(provider.displayName)}
                    onClick={() => void moveProvider(provider.id, -1)}
                  >
                    ↑
                  </button>
                  <button
                    type="button"
                    className="btn icon ghost"
                    style={{ width: 24, height: 24 }}
                    disabled={settingsBusy || index === providerCatalogue.length - 1}
                    aria-label={catalogue.settings.providerDown(provider.displayName)}
                    onClick={() => void moveProvider(provider.id, 1)}
                  >
                    ↓
                  </button>
                </div>
                <Switch
                  on={provider.enabled}
                  label={catalogue.settings.providerEnabled(provider.displayName)}
                  disabled={settingsBusy}
                  onChange={(checked) => void setProviderEnabled(provider.id, checked)}
                />
              </div>
            </div>
          ))}
          {providerCatalogue
            .filter((instance) => instance.enabled)
            .map((instance) => {
              const entry = plans.find((candidate) => candidate.provider === instance.provider);
              if (!entry) return null;
              return (
                <div key={`plan-${instance.id}`}>
                  <div className="sep" style={{ marginLeft: 14 }} />
                  <PlanRow
                    instance={instance.id}
                    entry={entry}
                    name={instance.label ?? catalogue.provider[instance.provider]}
                    busy={settingsBusy}
                  />
                </div>
              );
            })}
        </div>
      </div>

      {/* 6. Antigravity Live Quota */}
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <span className="cap">Antigravity</span>
        <div style={CARD_BOX}>
          <div className="row" style={ROW}>
            <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 3 }}>
              <span>{strings.agLiveTitle}</span>
              <span className="lbl">{liveAvailable ? strings.agLiveBody : strings.agLiveStoreOnly}</span>
            </div>
            {liveAvailable && (
              <Switch on={settings.antigravityLive} label={strings.agLiveTitle} onChange={setAntigravityLive} />
            )}
          </div>
        </div>
      </div>

      {/* 7. Claude Code Status Line */}
      <StatuslineSection now={now} />

      {/* 8. Fan Control Helper */}
      {monitored && hasFans && (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <span className="cap">{strings.fan}</span>
          <div style={CARD_BOX}>
            <div className="row" style={ROW}>
              <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 3 }}>
                <span>{strings.fanHelperTitle}</span>
                <span className="lbl">{strings.fanHelperBody}</span>
              </div>
              <button
                type="button"
                className="btn ghost"
                disabled={fanRemoved}
                onClick={() => void removeFanHelper()}
              >
                {fanRemoved ? strings.fanHelperRemoved : strings.fanHelperRemove}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* 9. Data, Accounts & Log Folders */}
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <span className="cap">{strings.moreSettings}</span>
        <div style={CARD_BOX}>
          {/* History retention */}
          <div className="row" style={{ ...ROW, flexWrap: "wrap", justifyContent: "space-between" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
              <span>{catalogue.settings.retentionTitle}</span>
              <span className="lbl">
                {retention.rebuilding
                  ? catalogue.settings.retentionRebuilding(retention.effectiveDays, retention.requestedDays)
                  : catalogue.settings.retentionHint}
              </span>
            </div>
            <div className="seg" role="group" aria-label={catalogue.settings.retentionTitle}>
              {RETENTION_OPTIONS.map((days) => (
                <button
                  key={days}
                  type="button"
                  className={`tab${settings.retentionDays === days ? " on" : ""}`}
                  aria-pressed={settings.retentionDays === days}
                  disabled={settingsBusy || retention.rebuilding}
                  onClick={() => void setRetentionDays(days)}
                >
                  {catalogue.settings.retentionDays(days)}
                </button>
              ))}
            </div>
          </div>

          {/* Sample deck */}
          <div className="sep" style={{ marginLeft: 14 }} />
          <div className="row" style={{ ...ROW, flexWrap: "wrap", justifyContent: "space-between" }}>
            <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
              <span>{catalogue.settings.demoTitle}</span>
              <span className="lbl">{catalogue.settings.demoHint(hostPlatform())}</span>
            </div>
            <div className="seg" role="group" aria-label={catalogue.settings.demoTitle}>
              {[
                { on: false, label: catalogue.settings.demoOff },
                { on: true, label: catalogue.settings.demoOn },
              ].map(({ on, label }) => (
                <button
                  key={String(on)}
                  type="button"
                  className={`tab${settings.demo === on ? " on" : ""}`}
                  aria-pressed={settings.demo === on}
                  disabled={settingsBusy}
                  onClick={() => setDemo(on)}
                >
                  {label}
                </button>
              ))}
            </div>
          </div>

          {/* Sandbox folder access (only when required) */}
          {access !== null && access.required && (
            <>
              <div className="sep" style={{ marginLeft: 14 }} />
              <div className="row" style={{ ...ROW, flexWrap: "wrap", justifyContent: "space-between" }}>
                <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
                  <span>{catalogue.settings.accessTitle}</span>
                  <span className="lbl">
                    {access.granted && access.path !== null
                      ? catalogue.settings.accessGranted(access.path)
                      : catalogue.settings.accessMissing}
                  </span>
                </div>
                <div className="row" style={{ gap: 6 }}>
                  <button type="button" className="btn" onClick={() => void requestAccess()}>
                    {catalogue.settings.accessChoose}
                  </button>
                  {access.granted && (
                    <button type="button" className="btn ghost" onClick={() => void forgetAccess()}>
                      {catalogue.settings.accessRevoke}
                    </button>
                  )}
                </div>
              </div>
            </>
          )}

          {/* Separate accounts */}
          {tools.length > 0 && (
            <>
              <div className="sep" style={{ marginLeft: 14 }} />
              <div style={{ padding: "10px 14px", display: "flex", flexDirection: "column", gap: 8 }}>
                <span style={{ fontWeight: 500 }}>{catalogue.settings.instancesTitle}</span>
                <span className="lbl">{catalogue.settings.instancesHint}</span>
                {namedInstances.length > 0 && (
                  <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                    {namedInstances.map((entry) => (
                      <div key={entry.id} className="row" style={{ justifyContent: "space-between", background: "var(--elev-3)", padding: "4px 10px", borderRadius: 8 }}>
                        <span style={{ fontSize: 12 }}>{entry.label ?? entry.id} · {entry.displayName}</span>
                        <button
                          type="button"
                          className="btn icon ghost"
                          style={{ width: 22, height: 22 }}
                          disabled={settingsBusy}
                          aria-label={catalogue.settings.instancesRemove(entry.label ?? entry.id)}
                          onClick={() => void removeInstance(entry.id)}
                        >
                          ×
                        </button>
                      </div>
                    ))}
                  </div>
                )}
                <div className="row" style={{ gap: 6, flexWrap: "wrap" }}>
                  <select
                    aria-label={catalogue.settings.instancesTool}
                    value={chosenTool}
                    disabled={settingsBusy}
                    onChange={(event) => setInstProvider(event.target.value as ProviderId)}
                    style={{ height: 28, borderRadius: 8, padding: "0 8px", background: "var(--elev-3)", color: "var(--fg)", border: "none", font: "inherit" }}
                  >
                    {tools.map((tool) => (
                      <option key={tool} value={tool}>
                        {catalogue.provider[tool]}
                      </option>
                    ))}
                  </select>
                  <label className="search" style={{ minWidth: 110, flex: 1, height: 28, background: "var(--elev-3)" }}>
                    <input
                      type="text"
                      value={instName}
                      placeholder={catalogue.settings.instancesNamePlaceholder}
                      aria-label={catalogue.settings.instancesNamePlaceholder}
                      spellCheck={false}
                      disabled={settingsBusy}
                      onChange={(event) => {
                        setInstName(event.target.value);
                        setInstRefused(false);
                      }}
                    />
                  </label>
                  <label className="search" style={{ minWidth: 110, flex: 1, height: 28, background: "var(--elev-3)" }}>
                    <input
                      type="text"
                      value={instLabel}
                      placeholder={catalogue.settings.instancesLabelPlaceholder}
                      aria-label={catalogue.settings.instancesLabelPlaceholder}
                      disabled={settingsBusy}
                      onChange={(event) => setInstLabel(event.target.value)}
                    />
                  </label>
                  <button type="button" className="btn" disabled={settingsBusy} onClick={handleAddInstance}>
                    {catalogue.settings.instancesAdd}
                  </button>
                </div>
                {instRefused && (
                  <span className="lbl" role="alert" style={{ color: "var(--bad)" }}>
                    {catalogue.settings.instancesInvalidName}
                  </span>
                )}
              </div>
            </>
          )}

          {/* Additional log folders per enabled tool */}
          {providerCatalogue
            .filter((entry) => entry.enabled)
            .map((entry) => (
              <div key={`roots-${entry.id}`}>
                <div className="sep" style={{ marginLeft: 14 }} />
                <LogFoldersSection
                  provider={entry.id}
                  displayName={entry.label ?? entry.displayName}
                  busy={settingsBusy}
                />
              </div>
            ))}
        </div>
      </div>
    </div>
  );
}
