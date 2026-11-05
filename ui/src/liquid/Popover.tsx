/**
 * The menu bar popover (design: `Main.dc.html`). Glass modules over a desktop-like backdrop.
 */

import dockerAsleep from "../assets/illustrations/docker-asleep.png";
import dockerMissing from "../assets/illustrations/docker-missing.png";
import { EmptyState } from "../components/EmptyState";
import { useDeck, useDeckState, useStrings } from "../store";
import { AiRow, useAiTools } from "./AiRows";
import { LiquidSettings } from "./LiquidSettings";
import { useLiquidStrings } from "./strings";
import { memoryPercent, startupVolume, useSystem, type Snapshot } from "./system";
import { HEALTH_COLOR, Ring, Spark, span, useNow, useNumber, Vessel, gb, gib, rate, heatColor } from "./ui";

function Header({ snapshot, onSettings, settingsOpen }: { snapshot: Snapshot | null; onSettings: () => void; settingsOpen: boolean }) {
  const strings = useLiquidStrings();
  const catalogue = useStrings();
  const health = snapshot?.health;
  const memory = snapshot?.memory;
  return (
    <section
      className="glass mod"
      aria-label={health ? `${strings.health} ${health.score}, ${strings.grade[health.grade]}` : catalogue.appName}
      style={{ display: "flex", alignItems: "center", gap: 10 }}
    >
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", gap: 2 }}>
        <div style={{ fontSize: 12.5, fontWeight: 600 }}>
          {snapshot
            ? [strings.mac, snapshot.machine.chip, memory ? `${Math.round(gib(memory.totalBytes))} GB` : null].filter(Boolean).join(" · ")
            : catalogue.appName}
        </div>
        <div className="lbl">
          {snapshot
            ? [`macOS ${snapshot.machine.osVersion}`, snapshot.cpu ? strings.up(span(snapshot.cpu.uptimeSecs)) : null].filter(Boolean).join(" · ")
            : strings.aiQuota}
        </div>
      </div>
      {health && (
        <div className="glass" style={{ display: "flex", alignItems: "center", gap: 8, height: 32, padding: "0 12px 0 8px", borderRadius: 999 }}>
          <Vessel value={health.score} vertical color={HEALTH_COLOR(health.score)} style={{ width: 10, height: 20 }} />
          <span className="num" style={{ fontSize: 16 }}>{health.score}</span>
          <span className="lbl" style={{ color: "var(--fg-2)" }}>{strings.grade[health.grade]}</span>
        </div>
      )}
      <button type="button" className="btn icon ghost" aria-label={strings.settings} aria-pressed={settingsOpen} onClick={onSettings}>
        <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
          <path d="M2.5 4.5h11M2.5 8h11M2.5 11.5h11" />
          <circle cx="5.5" cy="4.5" r="1.7" fill="currentColor" />
          <circle cx="10.5" cy="8" r="1.7" fill="currentColor" />
          <circle cx="7" cy="11.5" r="1.7" fill="currentColor" />
        </svg>
      </button>
    </section>
  );
}

function Tile({ label, value, unit, fill, color, aria }: { label: string; value: string; unit: React.ReactNode; fill: number; color: string; aria: string }) {
  return (
    <section className="glass mod" aria-label={aria} style={{ gridColumn: "span 2", display: "flex", gap: 10 }}>
      <div style={{ flex: 1, minWidth: 0, display: "flex", flexDirection: "column", justifyContent: "space-between" }}>
        <div className="lbl">{label}</div>
        <div className="row" style={{ alignItems: "baseline", gap: 6 }}>
          <span className="hero" style={{ fontSize: value.length > 3 ? 26 : 30 }}>{value}</span>
          <span className="t2" style={{ fontSize: 12, whiteSpace: "nowrap" }}>{unit}</span>
        </div>
      </div>
      <Vessel value={fill} vertical color={color} />
    </section>
  );
}

function SystemModules({ snapshot }: { snapshot: Snapshot }) {
  const strings = useLiquidStrings();
  const num = useNumber();
  const docker = useSystem((state) => state.docker);
  const awake = useSystem((state) => state.awake);
  const setAwake = useSystem((state) => state.setAwake);
  const live = useSystem((state) => state.live);
  const cpu = snapshot.cpu;
  const memory = memoryPercent(snapshot);
  const volume = startupVolume(snapshot);
  const fan = snapshot.fans[0];
  const [down, downUnit] = rate(snapshot.network?.downBytesPerSec ?? 0, num);
  const [upValue, upUnit] = rate(snapshot.network?.upBytesPerSec ?? 0, num);
  const running = docker?.status === "running" ? docker.containers.filter((c) => c.state === "running") : [];
  const dockerMemory = running.reduce((sum, c) => sum + (c.memoryBytes ?? 0), 0);
  const cpuTemp = snapshot.thermal.cpu?.average;
  return (
    <>
      <Tile
        label={strings.cpu}
        value={cpu ? `${Math.round(cpu.usage)}%` : "—"}
        unit={cpuTemp != null ? `${num(cpuTemp, 1)}°` : ""}
        fill={cpu?.usage ?? 0}
        color="#5E7E9E"
        aria={`${strings.cpu} ${cpu ? Math.round(cpu.usage) : "—"}%`}
      />
      <Tile
        label={strings.memory}
        value={memory != null ? `${Math.round(memory)}%` : "—"}
        unit={
          snapshot.memory ? (
            <span style={{ color: snapshot.memory.pressure === "normal" ? "var(--ok)" : snapshot.memory.pressure === "warning" ? "var(--warn)" : "var(--bad)", fontWeight: 500 }}>
              {strings.pressure[snapshot.memory.pressure]}
            </span>
          ) : ""
        }
        fill={memory ?? 0}
        color="#A3ADB8"
        aria={`${strings.memory} ${memory != null ? Math.round(memory) : "—"}%`}
      />
      <Tile
        label={strings.disk}
        value={volume ? (volume.availableBytes >= 1e12 ? num(volume.availableBytes / 1e12, 1) : num(gb(volume.availableBytes), 0)) : "—"}
        unit={`${volume && volume.availableBytes >= 1e12 ? "TB" : "GB"} ${strings.free}`}
        fill={volume ? 100 - (volume.availableBytes * 100) / Math.max(1, volume.totalBytes) : 0}
        color="#4E6E8E"
        aria={`${strings.disk} ${volume ? num(gb(volume.availableBytes), 0) : "—"} ${strings.gbFree}`}
      />
      <section className="glass mod" aria-label={strings.docker} style={{ gridColumn: "span 2", display: "flex", flexDirection: "column", justifyContent: "space-between" }}>
        <div className="lbl">{strings.docker}</div>
        {docker?.status === "running" ? (
          <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
            <div className="row" style={{ gap: 6, fontSize: 15, fontWeight: 500 }}>
              <span className="dot" style={{ color: running.some((c) => c.health === "unhealthy") ? "var(--warn)" : "var(--ok)" }} />
              {strings.running(running.length)}
            </div>
            <div className="lbl">
              {strings.ofContainers(docker.containers.length)}
              {dockerMemory > 0 ? ` · ${num(gb(dockerMemory), 1)} GB` : ""}
            </div>
          </div>
        ) : (
          <div style={{ display: "flex", alignItems: "flex-end", justifyContent: "space-between", gap: 6 }}>
            <div className="lbl">{docker === null ? strings.dockerChecking : docker.status === "notRunning" ? strings.dockerStopped : strings.notInstalled}</div>
            {docker !== null && (
              <img
                className={`empty-illust xs${docker.status === "notRunning" ? " breathing" : ""}`}
                src={docker.status === "notRunning" ? dockerAsleep : dockerMissing}
                alt=""
                aria-hidden="true"
              />
            )}
          </div>
        )}
      </section>
      <section className="glass mod" aria-label={fan ? `${strings.fan} ${Math.round(fan.rpm)} rpm` : strings.noFan} style={{ display: "flex", flexDirection: "column", alignItems: "center", justifyContent: "center", gap: 2, padding: 8 }}>
        {fan ? (
          <>
            <Ring percent={fan.percent} />
            <div className="mono" style={{ fontSize: 11 }}>{Math.round(fan.rpm)} <span className="t3">rpm</span></div>
          </>
        ) : (
          <div className="lbl">{strings.noFan}</div>
        )}
      </section>
      <section className="glass mod" aria-label={`${strings.power} ${snapshot.power.systemWatts ?? "—"} W`} style={{ padding: 10, display: "flex", flexDirection: "column", justifyContent: "space-between", alignItems: "center" }}>
        <div className="lbl">{strings.power}</div>
        <div className="hero" style={{ fontSize: 22 }}>{snapshot.power.systemWatts != null ? num(snapshot.power.systemWatts, 1) : "—"}</div>
        <div className="lbl">{strings.watts}</div>
      </section>
      <section className="glass mod" aria-label={`${strings.network} ↓ ${down} ${downUnit} ↑ ${upValue} ${upUnit}`} style={{ padding: 10, display: "flex", flexDirection: "column", justifyContent: "space-between" }}>
        <Spark values={live.map((p) => p.down)} height={18} width={60} stroke="var(--fg-2)" />
        <div className="mono" style={{ fontSize: 9.5, lineHeight: 1.35, whiteSpace: "nowrap", letterSpacing: "-.02em" }}>
          ↓ {down} {downUnit}
          <br />↑ {upValue} {upUnit}
        </div>
      </section>
      <button
        type="button"
        className={awake ? "clickable" : "glass clickable"}
        aria-pressed={awake}
        aria-label={`Keep ${strings.awake}`}
        onClick={() => void setAwake(!awake)}
        style={{
          borderRadius: 22,
          background: awake ? "rgba(255,255,255,.94)" : undefined,
          color: awake ? "#1d1d1f" : "var(--fg)",
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          gap: 4,
          boxShadow: awake ? "0 8px 22px rgba(0,0,0,.25)" : undefined,
        }}
      >
        <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M5 9h11v5a5 5 0 0 1-5 5h-1a5 5 0 0 1-5-5z" />
          <path d="M16 11h1.5a2.5 2.5 0 0 1 0 5H16" />
          {awake && <path d="M9 3.5c0 1 1 1.5 1 2.5M12.5 3.5c0 1 1 1.5 1 2.5" />}
        </svg>
        <span style={{ fontSize: 11, fontWeight: awake ? 600 : 500 }} className={awake ? undefined : "t2"}>{strings.awake}</span>
      </button>
    </>
  );
}

function ThermalStrip({ snapshot }: { snapshot: Snapshot }) {
  const strings = useLiquidStrings();
  const num = useNumber();
  const live = useSystem((state) => state.live);
  const values = live.slice(-60).map((p) => p.cpuCelsius);
  const peak = snapshot.thermal.peakCelsius;
  return (
    <section className="glass mod" aria-label={`${strings.thermal}, ${strings.peak5} ${peak != null ? num(peak, 1) : "—"}°`} style={{ padding: "10px 14px 12px", display: "flex", flexDirection: "column", gap: 6 }}>
      <div className="row" style={{ justifyContent: "space-between" }}>
        <span className="lbl">
          {strings.thermal} · {strings.thermalState[snapshot.thermal.state]}
        </span>
        <span className="lbl">
          {strings.peak5} <span style={{ color: "var(--fg)" }}>{peak != null ? `${num(peak, 1)}°` : "—"}</span>
        </span>
      </div>
      <Spark values={values} height={24} width={336} stroke={heatColor(snapshot.thermal.cpu?.max ?? null)} area="color-mix(in srgb, var(--fg) 8%, transparent)" />
    </section>
  );
}

export function Popover({ view, onToggleSettings, now: minuteNow }: { view: "panel" | "settings"; onToggleSettings: () => void; now: number }) {
  const strings = useLiquidStrings();
  const catalogue = useStrings();
  const deck = useDeckState();
  const access = useDeck((state) => state.access);
  const requestAccess = useDeck((state) => state.requestAccess);
  const setDemo = useDeck((state) => state.setDemo);
  const openDashboard = useDeck((state) => state.openDashboard);
  const snapshot = useSystem((state) => state.snapshot);
  const storeBuild = useSystem((state) => state.antigravity?.liveAvailable === false);
  const now = useNow(1000);
  const tools = useAiTools(now);
  const needsAccess = access !== null && access.required && !access.granted;
  const updatedAt = snapshot ? snapshot.takenAtMs : Date.parse(deck.updatedAt);
  const ago = Number.isFinite(updatedAt) ? Math.max(0, Math.round((now - updatedAt) / 1000)) : null;

  return (
    <>
      <Header snapshot={snapshot} settingsOpen={view === "settings"} onSettings={onToggleSettings} />
      {view === "settings" ? (
        <section className="glass pop-legacy" aria-label={strings.settings}>
          <LiquidSettings now={minuteNow} />
        </section>
      ) : (
        <>
          {needsAccess ? (
            <section className="glass mod legacy" style={{ padding: 4 }}>
              <EmptyState
                title={catalogue.empty.noPermission.title}
                body={catalogue.empty.noPermission.body}
                action={catalogue.empty.noPermission.action}
                onAction={() => void requestAccess()}
                secondaryAction={catalogue.empty.demoAction}
                onSecondaryAction={() => setDemo(true)}
              />
            </section>
          ) : null}
          <div className="pop-grid">
            {tools.length > 0 && (
              <section
                className="glass mod"
                aria-label={strings.aiQuota}
                style={{
                  gridColumn: snapshot ? "span 2" : "1 / -1",
                  gridRow: `span ${snapshot ? 2 : Math.max(1, Math.ceil(tools.length * 0.75))}`,
                  display: "flex",
                  flexDirection: "column",
                  justifyContent: "space-between",
                  gap: 8,
                  padding: 12,
                }}
              >
                {tools.slice(0, snapshot ? 3 : 6).map((tool) => (
                  <AiRow key={tool.key} tool={tool} now={now} />
                ))}
              </section>
            )}
            {snapshot && <SystemModules snapshot={snapshot} />}
          </div>
          {!needsAccess && tools.length === 0 && !deck.scanning && (
            <section className="glass mod legacy" style={{ padding: 4 }}>
              <EmptyState title={catalogue.empty.noTools.title} body={catalogue.empty.noTools.body} action={catalogue.empty.demoAction} onAction={() => setDemo(true)} />
            </section>
          )}
          {snapshot && <ThermalStrip snapshot={snapshot} />}
        </>
      )}
      <section className="glass mod" style={{ padding: "8px 8px 8px 14px", display: "flex", alignItems: "center", justifyContent: "space-between", gap: 8 }}>
        <span className="lbl" role="status" aria-live="polite">
          {deck.scanning ? catalogue.empty.scanning : ago !== null ? strings.updated(ago < 60 ? `${ago} s` : span(ago)) : ""}
        </span>
        <button type="button" className="btn primary" style={{ height: 32, padding: "0 16px" }} onClick={() => void openDashboard()}>
          {strings.openDashboard}
        </button>
      </section>
      {storeBuild && <p className="lbl" style={{ margin: "0 6px" }}>{strings.monitorOff}</p>}
    </>
  );
}

