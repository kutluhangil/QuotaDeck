import { Fragment } from "react";

import dockerAsleep from "../assets/illustrations/docker-asleep.png";
import dockerMissing from "../assets/illustrations/docker-missing.png";
import { useLiquidStrings } from "./strings";
import { useSystem, type Container } from "./system";
import { Chip, size, useNumber, Vessel } from "./ui";

/** Containers grouped by Compose project, matching ones only, projects first. */
export function composeGroups(containers: Container[], query: string, standalone: string) {
  const needle = query.trim().toLowerCase();
  const groups = new Map<string, Container[]>();
  for (const container of containers) {
    const matches =
      needle === "" ||
      container.name.toLowerCase().includes(needle) ||
      container.image.toLowerCase().includes(needle) ||
      (container.composeProject ?? "").toLowerCase().includes(needle);
    if (!matches) continue;
    const key = container.composeProject ?? standalone;
    groups.set(key, [...(groups.get(key) ?? []), container]);
  }
  return [...groups.entries()]
    .map(([name, list]) => ({ name, containers: list, standalone: name === standalone }))
    .sort((a, b) => Number(a.standalone) - Number(b.standalone) || a.name.localeCompare(b.name));
}

const DISK_COLORS: Record<string, string> = { Images: "#A3ADB8", "Local Volumes": "#6E7781", "Build Cache": "#474D54" };

function Asleep({ installed }: { installed: boolean }) {
  const strings = useLiquidStrings();
  return (
    <section className="card empty-card" aria-label={installed ? strings.dockerNotRunningTitle : strings.dockerNotInstalledTitle} style={{ minHeight: 320, alignItems: "flex-start" }}>
      <span className="cap">{strings.docker}</span>
      <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
        <img
          className={`empty-illust${installed ? " breathing" : ""}`}
          src={installed ? dockerAsleep : dockerMissing}
          alt=""
          aria-hidden="true"
        />
        <Chip dot>{installed ? strings.dockerStopped : strings.notInstalled}</Chip>
        <span style={{ fontSize: 17, fontWeight: 500 }}>{installed ? strings.dockerNotRunningTitle : strings.dockerNotInstalledTitle}</span>
        <span className="t2">{installed ? strings.dockerNotRunningBody : strings.dockerNotInstalledBody}</span>
      </div>
      <span className="lbl">{strings.dockerReadOnly}</span>
    </section>
  );
}

export function DockerTab({ query }: { query: string }) {
  const strings = useLiquidStrings();
  const num = useNumber();
  const docker = useSystem((state) => state.docker);
  if (docker === null) return <section className="card"><span className="t2">{strings.dockerChecking}</span></section>;
  if (docker.status !== "running") return <Asleep installed={docker.status === "notRunning"} />;

  const running = docker.containers.filter((c) => c.state === "running");
  const unhealthy = running.filter((c) => c.health === "unhealthy").length;
  const cpu = running.reduce((sum, c) => sum + (c.cpuPercent ?? 0), 0);
  const memory = running.reduce((sum, c) => sum + (c.memoryBytes ?? 0), 0);
  const limit = running.find((c) => c.memoryLimitBytes)?.memoryLimitBytes ?? 0;
  const disk = docker.disk.filter((d) => d.kind !== "Containers" && d.sizeBytes > 0);
  const diskTotal = docker.disk.reduce((sum, d) => sum + d.sizeBytes, 0);
  const groups = composeGroups(docker.containers, query, strings.ungrouped);

  return (
    <>
      <div className="row" style={{ justifyContent: "space-between", flexWrap: "wrap", padding: "4px 4px 8px" }}>
        <div className="row" style={{ gap: 12 }}>
          <span className="h1">{strings.docker}</span>
          <Chip tone="ok" dot>{strings.dockerRunning(docker.serverVersion)}</Chip>
        </div>
      </div>
      <div className="bento">
        <section className="card s3" aria-label={strings.containers}>
          <span className="lbl">{strings.containers}</span>
          <div className="row" style={{ alignItems: "baseline", gap: 6 }}>
            <span className="hero" style={{ fontSize: 44 }}>{running.length}</span>
            <span className="t3">/ {docker.containers.length}</span>
          </div>
          <div className="lbl">{strings.stoppedUnhealthy(docker.containers.length - running.length, unhealthy)}</div>
        </section>
        <section className="card s3" aria-label={strings.cpu}>
          <span className="lbl">{strings.cpu}</span>
          <div className="row" style={{ alignItems: "baseline", gap: 6 }}>
            <span className="hero" style={{ fontSize: 44 }}>{num(cpu, 1)}</span>
            <span className="t3">%</span>
          </div>
        </section>
        <section className="card s3" aria-label={strings.memory}>
          <span className="lbl">{strings.memory}</span>
          <div className="row" style={{ alignItems: "baseline", gap: 6 }}>
            <span className="hero" style={{ fontSize: 44 }}>{num(memory / 1e9, 1)}</span>
            <span className="t3">{limit > 0 ? strings.ofVm(size(limit, num)) : "GB"}</span>
          </div>
          {limit > 0 && <Vessel value={(memory * 100) / limit} size="md" color="#A3ADB8" />}
        </section>
        <section className="card s3" aria-label={strings.disk}>
          <span className="lbl">{strings.disk}</span>
          <div className="row" style={{ alignItems: "baseline", gap: 6 }}>
            <span className="hero" style={{ fontSize: 44 }}>{num(diskTotal / 1e9, 1)}</span>
            <span className="t3">GB</span>
          </div>
          {diskTotal > 0 && (
            <div style={{ display: "flex", gap: 2, height: 10, borderRadius: 99, overflow: "hidden" }} aria-hidden="true">
              {disk.map((d) => <i key={d.kind} style={{ flex: d.sizeBytes, background: DISK_COLORS[d.kind] ?? "#5B636C" }} />)}
            </div>
          )}
          <div className="row lbl" style={{ gap: 12, flexWrap: "wrap" }}>
            {disk.map((d) => (
              <span key={d.kind} className="row" style={{ gap: 5 }}>
                <i className="dot" style={{ color: DISK_COLORS[d.kind] ?? "#5B636C" }} />
                {strings.imagesVolumesCache[d.kind] ?? d.kind} {num(d.sizeBytes / 1e9, 1)}
              </span>
            ))}
          </div>
        </section>
      </div>
      <section className="card" aria-label={strings.containers} style={{ padding: "8px 8px 12px" }}>
        <div className="tblbox">
          <table className="tbl" style={{ minWidth: 760 }}>
            <thead>
              <tr>
                <th style={{ width: "26%" }}>{strings.name}</th>
                <th>{strings.image}</th>
                <th>{strings.healthCol}</th>
                <th className="r">CPU %</th>
                <th className="r">{strings.memory}</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {groups.map((group) => {
                const up = group.containers.filter((c) => c.state === "running").length;
                return (
                  <Fragment key={group.name}>
                    <tr className="group-row">
                      <td colSpan={6}>
                        <div className="glass">
                          <span style={{ fontWeight: 600 }}>{group.name}</span>
                          <span className="lbl">{up} / {group.containers.length}</span>
                        </div>
                      </td>
                    </tr>
                    {group.containers.map((c) => {
                      const on = c.state === "running";
                      return (
                        <tr key={c.id}>
                          <td className={on ? undefined : "t3"}>
                            <span className="row">
                              <i className="dot" style={{ color: !on ? "var(--fg-3)" : c.health === "unhealthy" ? "var(--warn)" : "var(--ok)" }} />
                              {c.name}
                            </span>
                          </td>
                          <td className={`mono ${on ? "t2" : "t3"}`}>{c.image}</td>
                          <td>
                            {!on ? (
                              <span className="t3">{c.state}</span>
                            ) : c.health ? (
                              <Chip tone={c.health === "healthy" ? "ok" : c.health === "unhealthy" ? "warn" : undefined}>{c.health}</Chip>
                            ) : (
                              <Chip>{strings.noCheck}</Chip>
                            )}
                          </td>
                          <td className={`r mono${on ? "" : " t3"}`}>{c.cpuPercent != null ? num(c.cpuPercent, 1) : "—"}</td>
                          <td className={`r mono${on ? "" : " t3"}`}>{c.memoryBytes != null ? size(c.memoryBytes, num) : "—"}</td>
                          <td className="t3">{c.status}</td>
                        </tr>
                      );
                    })}
                  </Fragment>
                );
              })}
            </tbody>
          </table>
        </div>
      </section>
      <div className="lbl" style={{ padding: "0 4px" }}>{strings.dockerReadOnly}</div>
    </>
  );
}
