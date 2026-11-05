import dockerAsleep from "../assets/illustrations/docker-asleep.png";
import dockerMissing from "../assets/illustrations/docker-missing.png";
import { AiRow, useAiTools } from "./AiRows";
import type { Tab } from "./Board";
import { useLiquidStrings } from "./strings";
import { chartPoints, memoryPercent, startupVolume, useSystem, type Snapshot } from "./system";
import { Chip, HEALTH_COLOR, HeatField, Ring, Spark, gb, gib, gradeTone, rate, size, span, thermalTone, useNow, useNumber, Vessel } from "./ui";
import { ProcessTable } from "./MacTab";
import { composeGroups } from "./DockerTab";

function HealthCard({ snapshot }: { snapshot: Snapshot }) {
  const strings = useLiquidStrings();
  const health = snapshot.health;
  const penalties = Object.entries(health.penalties) as [keyof typeof strings.penalty, number][];
  const worst = penalties.filter(([, points]) => points >= 1).sort((a, b) => b[1] - a[1])[0];
  return (
    <section className="card s4" aria-label={`${strings.health} ${health.score}, ${strings.grade[health.grade]}`} style={{ flexDirection: "row", gap: 20, alignItems: "stretch", padding: 20 }}>
      <Vessel value={health.score} vertical size="lg" color={HEALTH_COLOR(health.score)} style={{ height: 150, flex: "none" }} />
      <div style={{ display: "flex", flexDirection: "column", justifyContent: "space-between", minWidth: 0 }}>
        <div className="lbl">{strings.health}</div>
        <div>
          <div className="row" style={{ alignItems: "baseline", gap: 10 }}>
            <span className="hero" style={{ fontSize: 72 }}>{health.score}</span>
            <span style={{ fontSize: 15, fontWeight: 500, color: `var(--${gradeTone(health.score)})` }}>{strings.grade[health.grade]}</span>
          </div>
          <div className="t2" style={{ fontSize: 14, marginTop: 8 }}>{strings.healthSummary(worst ? strings.penalty[worst[0]] : null)}</div>
        </div>
        <div className="lbl">
          {[strings.mac, snapshot.machine.chip, snapshot.memory ? `${Math.round(gib(snapshot.memory.totalBytes))} GB` : null, snapshot.cpu ? strings.up(span(snapshot.cpu.uptimeSecs)) : null]
            .filter(Boolean)
            .join(" · ")}
        </div>
      </div>
    </section>
  );
}

export function OverviewTab({ onOpen, query }: { onOpen: (tab: Tab) => void; query: string }) {
  const strings = useLiquidStrings();
  const num = useNumber();
  const now = useNow(1000);
  const snapshot = useSystem((state) => state.snapshot);
  const docker = useSystem((state) => state.docker);
  const points = chartPoints(useSystem((state) => state.history), useSystem((state) => state.live));
  const tools = useAiTools(now);
  if (!snapshot) return null;
  const memory = memoryPercent(snapshot);
  const volume = startupVolume(snapshot);
  const fan = snapshot.fans[0];
  const [down, downUnit] = rate(snapshot.network?.downBytesPerSec ?? 0, num);
  const [up, upUnit] = rate(snapshot.network?.upBytesPerSec ?? 0, num);
  const running = docker?.status === "running" ? docker.containers.filter((c) => c.state === "running") : [];
  const dockerMemory = running.reduce((sum, c) => sum + (c.memoryBytes ?? 0), 0);
  const dockerLimit = running.find((c) => c.memoryLimitBytes)?.memoryLimitBytes ?? 0;

  return (
    <div className="bento">
      <HealthCard snapshot={snapshot} />
      <section className="card s8" aria-label={strings.aiQuota} style={{ padding: 20 }}>
        <div className="card-h">
          <span className="h2">{strings.aiQuota}</span>
          <button type="button" className="btn ghost" style={{ height: 24, padding: "0 8px", fontSize: 12 }} onClick={() => onOpen("ai")}>
            {strings.details}
          </button>
        </div>
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(min(200px, 100%), 1fr))", gap: 20 }}>
          {tools.slice(0, 3).map((tool) => (
            <AiRow key={tool.key} tool={tool} now={now} large />
          ))}
          {tools.length === 0 && <span className="t2">{strings.noData}</span>}
        </div>
      </section>
      <section className="card s4" aria-label={strings.heatField} style={{ padding: 20 }}>
        <div className="card-h">
          <span className="h2">{strings.heatField}</span>
          <Chip tone={thermalTone(snapshot.thermal.state)} dot>{strings.thermalState[snapshot.thermal.state]}</Chip>
        </div>
        <HeatField snapshot={snapshot} strings={strings} />
      </section>
      <div className="s8" style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(min(140px, 100%), 1fr))", gap: 12 }}>
        <section className="card" aria-label={`${strings.memory} ${memory != null ? Math.round(memory) : "—"}%`} style={{ flexDirection: "row", gap: 12 }}>
          <div style={{ flex: 1, display: "flex", flexDirection: "column", justifyContent: "space-between", gap: 10 }}>
            <span className="lbl">{strings.memory}</span>
            <div>
              <div className="hero" style={{ fontSize: 30 }}>{memory != null ? `${Math.round(memory)}%` : "—"}</div>
              {snapshot.memory && (
                <div style={{ fontSize: 12, marginTop: 4, color: snapshot.memory.pressure === "normal" ? "var(--ok)" : "var(--warn)" }}>
                  {strings.pressure[snapshot.memory.pressure]}
                </div>
              )}
            </div>
            {snapshot.memory && (
              <span className="lbl">
                {num(gib(snapshot.memory.usedBytes), 1)} {strings.of(`${Math.round(gib(snapshot.memory.totalBytes))} GB`)}
              </span>
            )}
          </div>
          <Vessel value={memory ?? 0} vertical color="#A3ADB8" style={{ height: 110 }} />
        </section>
        <section className="card" aria-label={strings.disk} style={{ flexDirection: "row", gap: 12 }}>
          <div style={{ flex: 1, display: "flex", flexDirection: "column", justifyContent: "space-between", gap: 10 }}>
            <span className="lbl">{strings.disk}</span>
            <div>
              <div className="hero" style={{ fontSize: 30 }}>{volume ? num(gb(volume.availableBytes), 0) : "—"}</div>
              <div className="t2" style={{ fontSize: 12, marginTop: 4 }}>{strings.gbFree}</div>
            </div>
            <span className="lbl">
              {[snapshot.thermal.ssd ? `SSD ${num(snapshot.thermal.ssd.average, 0)}°` : null, snapshot.smart ? (snapshot.smart.failing ? strings.smartFailing : strings.smartOk) : null].filter(Boolean).join(" · ")}
            </span>
          </div>
          <Vessel value={volume ? 100 - (volume.availableBytes * 100) / Math.max(1, volume.totalBytes) : 0} vertical color="#4E6E8E" style={{ height: 110 }} />
        </section>
        <section className="card" aria-label={strings.fan} style={{ alignItems: "flex-start" }}>
          <span className="lbl">{strings.fan}</span>
          {fan ? (
            <>
              <Ring percent={fan.percent} size={60} stroke={5} />
              <div>
                <span className="num" style={{ fontSize: 17 }}>{Math.round(fan.rpm)}</span>{" "}
                <span className="lbl">rpm · {fan.mode === "auto" ? "Auto" : strings.fanForced}</span>
              </div>
            </>
          ) : (
            <span className="t2">{strings.noFan}</span>
          )}
        </section>
        <section className="card" aria-label={strings.power}>
          <span className="lbl">{strings.power}</span>
          <div style={{ flex: 1, display: "flex", alignItems: "flex-end" }}>
            <span className="hero" style={{ fontSize: 36 }}>{snapshot.power.systemWatts != null ? num(snapshot.power.systemWatts, 1) : "—"}</span>
            <span className="t3" style={{ margin: "0 0 4px 4px" }}>W</span>
          </div>
          <Spark values={points.map((p) => p.watts)} />
        </section>
        <section className="card" aria-label={strings.network}>
          <span className="lbl">{strings.network}</span>
          <div className="mono" style={{ flex: 1, display: "flex", flexDirection: "column", justifyContent: "flex-end", gap: 2 }}>
            <div style={{ fontSize: 15 }}>↓ {down} <span className="t3" style={{ fontSize: 11 }}>{downUnit}</span></div>
            <div style={{ fontSize: 15 }}>↑ {up} <span className="t3" style={{ fontSize: 11 }}>{upUnit}</span></div>
          </div>
          <Spark values={points.map((p) => p.down)} />
        </section>
      </div>
      <section className="card s5" aria-label={strings.docker} style={{ padding: 20 }}>
        <div className="card-h">
          <span className="h2">{strings.docker}</span>
          {docker?.status === "running" ? (
            <Chip tone="ok" dot>{strings.dockerRunning(docker.serverVersion)}</Chip>
          ) : (
            <Chip dot>{docker?.status === "notInstalled" ? strings.notInstalled : strings.dockerStopped}</Chip>
          )}
        </div>
        {docker?.status === "running" ? (
          <>
            <div style={{ display: "flex", gap: 24, alignItems: "flex-end" }}>
              <div className="row" style={{ alignItems: "baseline", gap: 4 }}>
                <span className="hero" style={{ fontSize: 40 }}>{running.length}</span>
                <span className="t3">/ {docker.containers.length}</span>
              </div>
              {dockerLimit > 0 && (
                <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 6 }}>
                  <div className="row" style={{ justifyContent: "space-between" }}>
                    <span className="lbl">{strings.memory}</span>
                    <span className="mono" style={{ fontSize: 12 }}>{size(dockerMemory, num)} / {size(dockerLimit, num)}</span>
                  </div>
                  <Vessel value={(dockerMemory * 100) / dockerLimit} size="md" color="#A3ADB8" />
                </div>
              )}
            </div>
            <div style={{ display: "flex", flexDirection: "column" }}>
              {composeGroups(docker.containers, query, strings.ungrouped).slice(0, 4).map((group) => {
                const up = group.containers.filter((c) => c.state === "running").length;
                const unhealthy = group.containers.filter((c) => c.health === "unhealthy").length;
                const checked = group.containers.some((c) => c.health !== null);
                return (
                  <div key={group.name} className="row" style={{ padding: "9px 0", borderTop: ".5px solid var(--hair)" }}>
                    <span style={{ flex: 1, fontWeight: 500 }}>{group.name}</span>
                    <span className="lbl">{up} / {group.containers.length}</span>
                    {unhealthy > 0 ? (
                      <Chip tone="warn">{strings.nUnhealthy(unhealthy)}</Chip>
                    ) : checked ? (
                      <Chip tone="ok">{strings.healthy}</Chip>
                    ) : (
                      <Chip>{strings.noCheck}</Chip>
                    )}
                  </div>
                );
              })}
            </div>
          </>
        ) : (
          <div style={{ display: "flex", alignItems: "center", gap: 16 }}>
            <img
              className={`empty-illust sm${docker?.status === "notRunning" ? " breathing" : ""}`}
              src={docker?.status === "notInstalled" ? dockerMissing : dockerAsleep}
              alt=""
              aria-hidden="true"
            />
            <span className="t2">{docker?.status === "notInstalled" ? strings.dockerNotInstalledBody : strings.dockerNotRunningBody}</span>
          </div>
        )}
      </section>
      <section className="card s7" aria-label={strings.topProcesses} style={{ padding: 20 }}>
        <div className="card-h">
          <span className="h2">{strings.topProcesses}</span>
          <button type="button" className="btn ghost" style={{ height: 24, padding: "0 8px", fontSize: 12 }} onClick={() => onOpen("mac")}>
            {strings.processes}
          </button>
        </div>
        <ProcessTable processes={snapshot.processes} query={query} limit={5} compact />
      </section>
    </div>
  );
}
