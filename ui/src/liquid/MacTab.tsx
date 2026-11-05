import { useState } from "react";

import { call } from "../store";

import { useLiquidStrings } from "./strings";
import { chartPoints, startupVolume, useSystem, type Snapshot } from "./system";
import { Chip, HeatField, Lines, Spark, gb, heatColor, rate, size, thermalTone, useNumber, Vessel } from "./ui";

/**
 * Fan control ships hidden: the helper needs a signed build to install, and is tried on this
 * Mac first (docs/USER_ACTIONS.md). Flip to `true` once that test has passed.
 */
const FAN_CONTROL_READY = false;

type FanOutcome = { state: "applied" } | { state: "needsApproval" } | { state: "unavailable" } | { state: "failed"; reason: string };

function FanControls({ fan }: { fan: number }) {
  const strings = useLiquidStrings();
  const [mode, setMode] = useState<"auto" | "cool" | "max">("auto");
  const [note, setNote] = useState<string | null>(null);
  const pick = async (next: "auto" | "cool" | "max") => {
    const result = await call<FanOutcome>("set_fan_mode", { fan, mode: next });
    if (!result.ok) return setNote(result.error);
    if (result.value.state === "applied") {
      setMode(next);
      setNote(null);
    } else if (result.value.state === "needsApproval") setNote(strings.fanNeedsApproval);
    else if (result.value.state === "failed") setNote(result.value.reason);
    else setNote(strings.fanReadOnly);
  };
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 6, marginTop: "auto" }}>
      <div className="seg" role="group" aria-label={strings.fan}>
        {(["auto", "cool", "max"] as const).map((option) => (
          <button key={option} type="button" className={`tab${mode === option ? " on" : ""}`} aria-pressed={mode === option} onClick={() => void pick(option)}>
            {option === "auto" ? "Auto" : option === "cool" ? "Cool" : "Max"}
          </button>
        ))}
      </div>
      {note && <span className="lbl" style={{ color: "var(--warn)" }}>{note}</span>}
    </div>
  );
}

type SortKey = "name" | "memoryBytes" | "cpu" | "powerWatts" | "pid";

export function ProcessTable({
  processes,
  query,
  limit,
  compact,
}: {
  processes: Snapshot["processes"];
  query: string;
  limit?: number;
  compact?: boolean;
}) {
  const strings = useLiquidStrings();
  const num = useNumber();
  const [sort, setSort] = useState<SortKey>("cpu");
  const needle = query.trim().toLowerCase();
  const rows = processes
    .filter((p) => needle === "" || p.name.toLowerCase().includes(needle) || String(p.pid) === needle)
    .sort((a, b) => (sort === "name" ? a.name.localeCompare(b.name) : (b[sort] as number) - (a[sort] as number)))
    .slice(0, limit ?? processes.length);
  const maxCpu = Math.max(1, ...rows.map((p) => p.cpu));
  const header = (key: SortKey, label: string, right?: boolean) => (
    <th className={right ? "r" : undefined} aria-sort={sort === key ? "descending" : undefined}>
      {compact ? label : <button type="button" onClick={() => setSort(key)}>{label}{sort === key ? " ▾" : ""}</button>}
    </th>
  );
  return (
    <div className="tblbox">
      <table className="tbl" style={compact ? { minWidth: 480 } : undefined}>
        <thead>
          <tr>
            {header("name", strings.name)}
            {header("memoryBytes", strings.memory, true)}
            {header("cpu", "% CPU")}
            {header("powerWatts", strings.energy, true)}
            {!compact && header("pid", strings.pid, true)}
          </tr>
        </thead>
        <tbody>
          {rows.map((p) => (
            <tr key={p.pid}>
              <td><span className="row">{p.name}</span></td>
              <td className="r mono">{size(p.memoryBytes, num)}</td>
              <td>
                <span className="row">
                  <span className="minibar"><i style={{ width: `${(p.cpu * 100) / maxCpu}%`, backgroundColor: "#A3ADB8" }} /></span>
                  <span className="mono">{num(p.cpu, 1)}</span>
                </span>
              </td>
              <td className="r mono">{num(p.powerWatts, 1)} W</td>
              {!compact && <td className="r mono t3">{p.pid}</td>}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

export function MacTab({ query }: { query: string }) {
  const strings = useLiquidStrings();
  const num = useNumber();
  const snapshot = useSystem((state) => state.snapshot);
  const span = useSystem((state) => state.span);
  const points = chartPoints(useSystem((state) => state.history), useSystem((state) => state.live));
  if (!snapshot) return null;
  const t = snapshot.thermal;
  const cpu = snapshot.cpu;
  const memory = snapshot.memory;
  const volume = startupVolume(snapshot);
  const fan = snapshot.fans[0];
  const power = snapshot.power;
  const [down, downUnit] = rate(snapshot.network?.downBytesPerSec ?? 0, num);
  const [up, upUnit] = rate(snapshot.network?.upBytesPerSec ?? 0, num);
  const [readV, readU] = rate(snapshot.diskIo?.readBytesPerSec ?? 0, num);
  const [writeV, writeU] = rate(snapshot.diskIo?.writeBytesPerSec ?? 0, num);
  const others = snapshot.volumes.filter((v) => !v.startup);
  const spanLabel = strings.span[span] ?? "";
  const parts = [
    { label: "CPU", value: power.cpuWatts, color: "#D3D8DD" },
    { label: "GPU", value: power.gpuWatts, color: "#A3ADB8" },
    { label: "ANE", value: power.aneWatts, color: "#7C8590" },
    { label: "DRAM", value: power.dramWatts, color: "#5B636C" },
  ].filter((part): part is { label: string; value: number; color: string } => part.value !== null);
  const rest = power.systemWatts !== null ? Math.max(0, power.systemWatts - parts.reduce((s, p) => s + p.value, 0)) : null;

  return (
    <div className="bento">
      <section className="card s12" aria-label={strings.thermal} style={{ padding: 22, gap: 18 }}>
        <div className="card-h">
          <div className="row" style={{ gap: 12 }}>
            <span className="h2" style={{ fontSize: 15 }}>{strings.thermal}</span>
            <Chip tone={thermalTone(t.state)} dot>{strings.thermalState[t.state]}</Chip>
          </div>
        </div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 28, alignItems: "stretch" }}>
          <div style={{ flex: "3 1 520px", minWidth: 0, display: "flex", flexDirection: "column", gap: 14 }}>
            <HeatField snapshot={snapshot} strings={strings} large />
            <div style={{ display: "flex", flexDirection: "column", gap: 6, maxWidth: 520 }}>
              <div aria-hidden="true" style={{ height: 6, borderRadius: 99, background: "linear-gradient(90deg,#4E6E8E 0%,#6E8DAD 33%,#D9A24A 50%,#E5743A 83%,#E0473C 100%)" }} />
              <div className="row lbl" style={{ justifyContent: "space-between" }}><span>40°</span><span>60°</span><span>70°</span><span>90°</span><span>100°</span></div>
            </div>
          </div>
          <div style={{ flex: "2 1 300px", minWidth: 0, display: "flex", flexDirection: "column", gap: 16 }}>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 12 }}>
              <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                <span className="lbl">{strings.thermal}</span>
                <span style={{ fontSize: 20, fontWeight: 500 }}>{strings.thermalState[t.state]}</span>
                <span className="lbl">{strings.fromMacOS}</span>
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                <span className="lbl">{strings.peak5}</span>
                <span className="num" style={{ fontSize: 20 }}>{t.peakCelsius != null ? `${num(t.peakCelsius, 1)}°` : "—"}</span>
              </div>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
              <div className="row lbl" style={{ gap: 14 }}>
                <span className="row" style={{ gap: 6 }}><svg width="16" height="4" aria-hidden="true"><path d="M0 2H16" style={{ stroke: "var(--fg)" }} strokeWidth="1.5" /></svg>CPU</span>
                <span className="row" style={{ gap: 6 }}><svg width="16" height="4" aria-hidden="true"><path d="M0 2H16" style={{ stroke: "var(--fg-2)" }} strokeWidth="1.5" strokeDasharray="4 2" /></svg>GPU</span>
                <span className="row" style={{ gap: 6 }}><svg width="16" height="4" aria-hidden="true"><path d="M0 2H16" style={{ stroke: "var(--fg-3)" }} strokeWidth="1.5" strokeDasharray="1.5 2" /></svg>SSD</span>
              </div>
              <Lines
                height={150}
                label={`${strings.thermal}, ${spanLabel}`}
                series={[
                  { values: points.map((p) => p.cpuCelsius), stroke: "var(--fg)", area: "rgba(143,167,192,.18)" },
                  { values: points.map((p) => p.gpuCelsius), stroke: "var(--fg-2)", dashed: "4 2" },
                  { values: points.map((p) => p.ssdCelsius), stroke: "var(--fg-3)", dashed: "1.5 2.5" },
                ]}
              />
              <div className="row lbl" style={{ justifyContent: "space-between" }}><span>{spanLabel}</span><span>{strings.now}</span></div>
            </div>
          </div>
        </div>
      </section>

      <section className="card s8" aria-label={strings.cpu} style={{ padding: 22 }}>
        <div className="card-h">
          <span className="h2">{strings.cpu}</span>
          <span className="lbl">{cpu?.clusters.map((c) => `${c.cores.length} ${c.name}`).join(" · ")}</span>
        </div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 24 }}>
          <div style={{ display: "flex", gap: 14, alignItems: "flex-end" }}>
            {cpu?.clusters.map((cluster, i) => (
              <div key={cluster.name} style={{ display: "flex", flexDirection: "column", alignItems: "center", gap: 6 }}>
                <Vessel value={cluster.usage} vertical size="lg" color={["#5A799A", "#6E8DAD", "#5E7E9E"][i % 3]} style={{ height: 130 }} />
                <span className="lbl">{cluster.name.charAt(0)} {Math.round(cluster.usage)}%</span>
              </div>
            ))}
          </div>
          <div style={{ flex: 1, minWidth: 240, display: "flex", flexDirection: "column", gap: 14 }}>
            <div className="row" style={{ alignItems: "baseline", gap: 12, flexWrap: "wrap" }}>
              <span className="hero" style={{ fontSize: 56 }}>{cpu ? `${Math.round(cpu.usage)}%` : "—"}</span>
              <span className="t2" style={{ fontSize: 13 }}>
                {[t.cpuPerformance ? `P ${num(t.cpuPerformance.average, 1)}° · ${num(t.cpuPerformance.max, 1)}° ${strings.max.toLowerCase()}` : null, t.cpuEfficiency ? `E ${num(t.cpuEfficiency.average, 1)}°` : null].filter(Boolean).join(" · ")}
              </span>
            </div>
            {cpu && (
              <div style={{ display: "grid", gridTemplateColumns: "repeat(3, minmax(0, 1fr))", gap: 8, maxWidth: 300 }}>
                {(["1 m", "5 m", "15 m"] as const).map((label, i) => (
                  <div key={label}>
                    <div className="lbl">{i === 0 ? `${strings.loadAvg} ${label}` : label}</div>
                    <div className="num" style={{ fontSize: 17 }}>{num(cpu.load[i] ?? 0, 2)}</div>
                  </div>
                ))}
              </div>
            )}
            {cpu && (
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <span className="lbl">{strings.perCore}</span>
                <div style={{ display: "flex", gap: 4, height: 36, alignItems: "flex-end" }} aria-hidden="true">
                  {cpu.clusters.flatMap((cluster, ci) =>
                    cluster.cores.map((core, i) => (
                      <div key={`${ci}-${i}`} style={{ flex: 1, height: "100%", borderRadius: 4, background: "var(--vessel)", display: "flex", alignItems: "flex-end", overflow: "hidden" }}>
                        <i style={{ display: "block", width: "100%", height: `${Math.max(2, core)}%`, background: ["#5A799A", "#6E8DAD", "#5E7E9E"][ci % 3], borderRadius: 4 }} />
                      </div>
                    )),
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
        <Lines height={80} width={600} series={[{ values: points.map((p) => p.cpu), stroke: "var(--fg)", area: "rgba(143,167,192,.18)" }]} min={0} max={100} />
        <div className="row lbl" style={{ justifyContent: "space-between" }}><span>{spanLabel}</span><span>{strings.now}</span></div>
      </section>

      <section className="card s4" aria-label={strings.gpu} style={{ padding: 22 }}>
        <div className="card-h"><span className="h2">{strings.gpu}</span></div>
        <span className="hero" style={{ fontSize: 56 }}>{snapshot.gpuUsage != null ? `${Math.round(snapshot.gpuUsage)}%` : "—"}</span>
        <Vessel value={snapshot.gpuUsage ?? 0} size="md" color="#577696" />
        <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 8 }}>
          <div><div className="lbl">°C</div><div className="num" style={{ fontSize: 17 }}>{t.gpu ? `${num(t.gpu.average, 0)}°` : "—"}</div></div>
          <div><div className="lbl">{strings.power}</div><div className="num" style={{ fontSize: 17 }}>{power.gpuWatts != null ? `${num(power.gpuWatts, power.gpuWatts < 1 ? 2 : 1)} W` : "—"}</div></div>
        </div>
        <Spark values={points.map((p) => p.gpu)} height={24} min={0} max={100} />
      </section>

      <section className="card s6" aria-label={strings.memory} style={{ padding: 22 }}>
        <div className="card-h">
          <span className="h2">{strings.memory}</span>
          {memory && <Chip tone={memory.pressure === "normal" ? "ok" : memory.pressure === "warning" ? "warn" : "bad"} dot>{strings.pressure[memory.pressure]}</Chip>}
        </div>
        {memory && (
          <>
            <div className="row" style={{ alignItems: "baseline", gap: 8 }}>
              <span className="hero" style={{ fontSize: 56 }}>{num(memory.usedBytes / 1024 ** 3, 1)}</span>
              <span className="t3" style={{ fontSize: 15 }}>/ {Math.round(memory.totalBytes / 1024 ** 3)} GB</span>
            </div>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 16 }}>
              <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
                <span className="lbl">{strings.compressed}</span>
                <span className="num" style={{ fontSize: 17 }}>{num(memory.compressedBytes / 1024 ** 3, 1)} GB</span>
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
                <div className="row" style={{ justifyContent: "space-between" }}>
                  <span className="lbl">{strings.swap}</span>
                  <span className="mono" style={{ fontSize: 12 }}>{num(memory.swapUsedBytes / 1024 ** 3, 1)} / {num(memory.swapTotalBytes / 1024 ** 3, 0)} GB</span>
                </div>
                <Vessel value={(memory.swapUsedBytes * 100) / Math.max(1, memory.swapTotalBytes)} size="sm" color="#A3ADB8" />
              </div>
            </div>
          </>
        )}
        <Lines height={90} width={400} series={[{ values: points.map((p) => p.memory), stroke: "var(--fg-2)", area: "rgba(163,173,184,.2)" }]} min={0} max={100} />
      </section>

      <section className="card s6" aria-label={strings.memTemp} style={{ padding: 22 }}>
        <div className="card-h"><span className="h2">{strings.memTemp}</span></div>
        {t.memory ? (
          <div style={{ flex: 1, display: "flex", flexDirection: "column", justifyContent: "flex-end", gap: 10 }}>
            <span className="hero" style={{ fontSize: 48 }}>{num(t.memory.average, 0)}°</span>
            <Vessel value={t.memory.average} size="sm" color={heatColor(t.memory.average)} />
          </div>
        ) : (
          <div style={{ flex: 1, display: "flex", flexDirection: "column", justifyContent: "flex-end", gap: 8 }}>
            <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.4" style={{ color: "var(--fg-3)" }} aria-hidden="true">
              <rect x="4" y="7" width="16" height="10" rx="2" />
              <path d="M8 17v2M12 17v2M16 17v2M8 5v2M12 5v2M16 5v2" />
            </svg>
            <span style={{ fontSize: 13, fontWeight: 500 }}>{strings.noMemSensor}</span>
            <span className="lbl">{strings.wontEstimate}</span>
          </div>
        )}
      </section>

      <section className="card s6" aria-label={strings.storage} style={{ padding: 22 }}>
        <div className="card-h">
          <span className="h2">{strings.storage}</span>
          {snapshot.smart && (
            <Chip tone={snapshot.smart.failing ? "bad" : "ok"} dot>
              {snapshot.smart.failing ? strings.smartFailing : strings.smartOk}
              {` · ${strings.wear(snapshot.smart.percentUsed)}`}
            </Chip>
          )}
        </div>
        {volume && (
          <>
            <div className="row" style={{ alignItems: "baseline", gap: 8, flexWrap: "wrap" }}>
              <span className="hero" style={{ fontSize: 48 }}>{num(gb(volume.availableBytes), 0)}</span>
              <span className="t3" style={{ fontSize: 15 }}>
                {strings.gbFree} {strings.of(size(volume.totalBytes, num))}
                {t.ssd ? ` · NAND ${num(t.ssd.average, 0)}°` : ""}
              </span>
            </div>
            <Vessel value={100 - (volume.availableBytes * 100) / Math.max(1, volume.totalBytes)} size="md" color="#4E6E8E" />
          </>
        )}
        <div style={{ display: "grid", gridTemplateColumns: "repeat(2, minmax(0, 1fr))", gap: 16 }}>
          <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span className="lbl">{strings.read}</span>
            <span className="mono" style={{ fontSize: 14 }}>{readV} {readU}</span>
            <Spark values={points.map((p) => p.diskRead)} stroke="var(--fg)" />
          </div>
          <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
            <span className="lbl">{strings.write}</span>
            <span className="mono" style={{ fontSize: 14 }}>{writeV} {writeU}</span>
            <Spark values={points.map((p) => p.diskWrite)} stroke="var(--fg-3)" dashed="3 2" />
          </div>
        </div>
        {others.length > 0 && (
          <div style={{ display: "flex", flexDirection: "column" }}>
            {others.map((v) => (
              <div key={v.mountPoint} className="row" style={{ padding: "9px 0", borderTop: ".5px solid var(--hair)", gap: 12 }}>
                <span style={{ flex: 1, fontWeight: 500 }}>{v.name}</span>
                <Vessel value={100 - (v.availableBytes * 100) / Math.max(1, v.totalBytes)} size="sm" color="#A3ADB8" style={{ width: 100 }} />
                <span className="mono t2" style={{ fontSize: 12, width: 110, textAlign: "right" }}>{size(v.availableBytes, num)} {strings.free}</span>
              </div>
            ))}
          </div>
        )}
      </section>

      <section className="card s6" aria-label={strings.power} style={{ padding: 22 }}>
        <div className="card-h"><span className="h2">{strings.power}</span><span className="lbl">{strings.systemTotal}</span></div>
        <div className="row" style={{ alignItems: "baseline", gap: 6 }}>
          <span className="hero" style={{ fontSize: 56 }}>{power.systemWatts != null ? num(power.systemWatts, 1) : "—"}</span>
          <span className="t3" style={{ fontSize: 15 }}>W</span>
        </div>
        {power.systemWatts != null && (
          <div style={{ display: "flex", gap: 2, height: 14, borderRadius: 99, overflow: "hidden" }} aria-hidden="true">
            {parts.map((part) => <i key={part.label} style={{ flex: Math.max(0.15, part.value), background: part.color }} />)}
            <i style={{ flex: Math.max(0.15, rest ?? 0), background: "var(--vessel)", boxShadow: "inset 0 0 0 .5px var(--vessel-rim)" }} />
          </div>
        )}
        <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(90px, 1fr))", gap: 10 }}>
          {parts.map((part) => (
            <div key={part.label}>
              <div className="row lbl" style={{ gap: 6 }}><i className="dot" style={{ color: part.color }} />{part.label}</div>
              <div className="num" style={{ fontSize: 16 }}>{num(part.value, part.value < 1 ? 2 : 1)} W</div>
            </div>
          ))}
          {rest != null && (
            <div>
              <div className="row lbl" style={{ gap: 6 }}><i className="dot" style={{ color: "var(--fg-3)" }} />{strings.rest}</div>
              <div className="num" style={{ fontSize: 16 }}>{num(rest, 1)} W</div>
            </div>
          )}
        </div>
        {parts.length < 4 && <span className="lbl">{strings.powerSplitMissing}</span>}
        <Spark values={points.map((p) => p.watts)} height={24} />
      </section>

      <section className="card s4" aria-label={strings.fan} style={{ padding: 22 }}>
        <div className="card-h">
          <span className="h2">{strings.fan}</span>
          {fan && <Chip>{fan.mode === "auto" ? strings.fanAuto : strings.fanForced}</Chip>}
        </div>
        {fan ? (
          <>
            <div className="row" style={{ alignItems: "baseline", gap: 8 }}>
              <span className="hero" style={{ fontSize: 48 }}>{Math.round(fan.rpm)}</span>
              <span className="t3" style={{ fontSize: 15 }}>rpm · {Math.round(fan.percent)}%</span>
            </div>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <Vessel value={fan.percent} size="md" color="#A3ADB8" ticks />
              <div className="row lbl" style={{ justifyContent: "space-between" }}>
                <span>0</span><span>min {num(fan.minRpm, 0)}</span><span>max {num(fan.maxRpm, 0)}</span>
              </div>
            </div>
          </>
        ) : (
          <span className="t2">{strings.noFan}</span>
        )}
        {FAN_CONTROL_READY && fan ? <FanControls fan={fan.index} /> : (
        <div className="row t3" style={{ gap: 6, fontSize: 12, marginTop: "auto" }}>
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.2" aria-hidden="true">
            <rect x="2.5" y="5.5" width="7" height="5" rx="1" />
            <path d="M4 5.5V4a2 2 0 0 1 4 0v1.5" />
          </svg>
          {strings.fanReadOnly}
        </div>
        )}
      </section>

      <section className="card s8" aria-label={strings.network} style={{ padding: 22 }}>
        <div className="card-h"><span className="h2">{strings.network}</span></div>
        <div className="row" style={{ gap: 32, flexWrap: "wrap" }}>
          <div><div className="lbl">{strings.down}</div><div className="row" style={{ alignItems: "baseline", gap: 4 }}><span className="hero" style={{ fontSize: 40 }}>{down}</span><span className="t3">{downUnit}</span></div></div>
          <div><div className="lbl">{strings.up_}</div><div className="row" style={{ alignItems: "baseline", gap: 4 }}><span className="hero" style={{ fontSize: 40 }}>{up}</span><span className="t3">{upUnit}</span></div></div>
        </div>
        <Lines
          height={100}
          width={600}
          series={[
            { values: points.map((p) => p.down), stroke: "var(--fg)", area: "rgba(163,173,184,.22)" },
            { values: points.map((p) => p.up), stroke: "var(--fg-3)" },
          ]}
          min={0}
        />
      </section>

      <section className="card s12" aria-label={strings.processes} style={{ padding: "14px 14px 10px" }}>
        <div className="card-h" style={{ padding: "4px 8px" }}>
          <span className="h2">{strings.processes} <span className="t3" style={{ fontWeight: 400 }}>({snapshot.processes.length})</span></span>
        </div>
        <ProcessTable processes={snapshot.processes} query={query} />
      </section>
    </div>
  );
}
