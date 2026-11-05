/**
 * The v2 settings: Mac alerts and Antigravity live quota. Everything older (tools, theme,
 * language, plans, menu bar, status line) stays in the v1 settings view below it.
 */

import { SettingsView } from "../components/SettingsView";
import { useDeck } from "../store";
import type { SystemAlertRules } from "../types";
import { useLiquidStrings } from "./strings";
import { useSystem } from "./system";

function Switch({ on, label, onChange }: { on: boolean; label: string; onChange: (on: boolean) => void }) {
  return <button type="button" className="switch" role="switch" aria-checked={on} aria-label={label} onClick={() => onChange(!on)} />;
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

const ROW = { padding: "10px 14px", gap: 12, minHeight: 48 } as const;

export function LiquidSettings({ now }: { now: number }) {
  const strings = useLiquidStrings();
  const settings = useDeck((state) => state.settings);
  const setSystemAlerts = useDeck((state) => state.setSystemAlerts);
  const setAntigravityLive = useDeck((state) => state.setAntigravityLive);
  const setRefreshSpeed = useDeck((state) => state.setRefreshSpeed);
  const setMenuBar = useDeck((state) => state.setMenuBar);
  const monitored = useSystem((state) => state.snapshot !== null);
  const liveAvailable = useSystem((state) => state.antigravity?.liveAvailable ?? false);
  const rules = settings.systemAlerts;
  const update = (patch: Partial<SystemAlertRules>) => setSystemAlerts({ ...rules, ...patch });

  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 18, padding: 10 }}>
      {monitored && (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <span className="cap">{strings.alerts}</span>
          <div style={{ borderRadius: 14, background: "var(--elev-2)", boxShadow: "inset 0 0 0 .5px var(--hair)" }}>
            <div className="row" style={ROW}>
              <span style={{ flex: 1 }}>{strings.cpuAbove}</span>
              <Threshold value={rules.cpuCelsiusAbove} fallback={90} unit="°C" label={strings.cpuAbove} onChange={(cpuCelsiusAbove) => update({ cpuCelsiusAbove })} />
            </div>
            <div className="sep" style={{ marginLeft: 14 }} />
            <div className="row" style={ROW}>
              <span style={{ flex: 1 }}>{strings.diskBelow}</span>
              <Threshold value={rules.freeDiskGbBelow} fallback={20} unit="GB" label={strings.diskBelow} onChange={(freeDiskGbBelow) => update({ freeDiskGbBelow })} />
            </div>
            <div className="sep" style={{ marginLeft: 14 }} />
            <div className="row" style={ROW}>
              <span style={{ flex: 1 }}>{strings.memCritical}</span>
              <Switch on={rules.memoryCritical} label={strings.memCritical} onChange={(memoryCritical) => update({ memoryCritical })} />
            </div>
            <div className="sep" style={{ marginLeft: 14 }} />
            <div className="row" style={ROW}>
              <span style={{ flex: 1 }}>{strings.containerUnhealthy}</span>
              <Switch on={rules.containerUnhealthy} label={strings.containerUnhealthy} onChange={(containerUnhealthy) => update({ containerUnhealthy })} />
            </div>
          </div>
        </div>
      )}
      {monitored && (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <span className="cap">{strings.refreshRate}</span>
          <div className="seg" role="group" aria-label={strings.refreshRate} style={{ alignSelf: "flex-start" }}>
            {(["live", "balanced", "saver"] as const).map((speed) => (
              <button key={speed} type="button" className={`tab${settings.refreshSpeed === speed ? " on" : ""}`} aria-pressed={settings.refreshSpeed === speed} onClick={() => setRefreshSpeed(speed)}>
                {strings.refreshSpeed[speed]}
              </button>
            ))}
          </div>
          <span className="lbl">{strings.refreshHint}</span>
        </div>
      )}
      {monitored && (
        <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          <span className="cap">{strings.menuBarShow}</span>
          <div className="seg" role="group" aria-label={strings.menuBarShow} style={{ alignSelf: "flex-start" }}>
            {(["temperature", "memory"] as const).map((key) => (
              <button
                key={key}
                type="button"
                className={`tab${settings.menuBar[key] ? " on" : ""}`}
                aria-pressed={settings.menuBar[key]}
                onClick={() => setMenuBar({ ...settings.menuBar, [key]: !settings.menuBar[key] })}
              >
                {key === "temperature" ? strings.menuBarTemp : strings.memory}
              </button>
            ))}
          </div>
          <span className="lbl">{strings.menuBarHint}</span>
        </div>
      )}
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <span className="cap">Antigravity</span>
        <div style={{ borderRadius: 14, background: "var(--elev-2)", boxShadow: "inset 0 0 0 .5px var(--hair)" }}>
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
      <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
        <span className="cap">{strings.moreSettings}</span>
        <div className="legacy legacy-host">
          <SettingsView now={now} />
        </div>
      </div>
    </div>
  );
}
