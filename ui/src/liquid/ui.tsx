/**
 * Building blocks of the Liquid surfaces, after `docs/design/liquid-instruments`.
 */

import { useEffect, useState, type CSSProperties, type ReactNode } from "react";

import { intlLocale } from "../i18n";
import { useDeck } from "../store";
import type { ProviderId } from "../types";
import type { Snapshot } from "./system";
import type { LiquidStrings } from "./strings";

/* ---------- hooks ---------- */

/** Wall clock that re-renders every `ms`. */
export function useNow(ms = 1000): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), ms);
    return () => window.clearInterval(timer);
  }, [ms]);
  return now;
}

function useMedia(query: string): boolean {
  const [matches, setMatches] = useState(() => window.matchMedia(query).matches);
  useEffect(() => {
    const media = window.matchMedia(query);
    const onChange = () => setMatches(media.matches);
    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, [query]);
  return matches;
}

/** The class list for a `.qd` root: appearance plus Reduce Transparency. */
export function useQdClass(): string {
  const theme = useDeck((state) => state.settings.theme);
  const prefersDark = useMedia("(prefers-color-scheme: dark)");
  const reduceTransparency = useMedia("(prefers-reduced-transparency: reduce)");
  const dark = theme === "dark" || (theme === "system" && prefersDark);
  return `qd ${dark ? "dark" : "light"}${reduceTransparency ? " rt" : ""}`;
}

export function useNumber(): (value: number, digits?: number) => string {
  const locale = useDeck((state) => state.settings.locale);
  return (value, digits = 0) =>
    new Intl.NumberFormat(intlLocale(locale), {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    }).format(value);
}

/* ---------- formatting ---------- */

export function gb(bytes: number): number {
  return bytes / 1e9;
}

/** "2.4 MB/s", "180 KB/s". */
export function rate(bytesPerSec: number, num: (v: number, d?: number) => string): [string, string] {
  if (bytesPerSec >= 1e9) return [num(bytesPerSec / 1e9, 1), "GB/s"];
  if (bytesPerSec >= 1e6) return [num(bytesPerSec / 1e6, 1), "MB/s"];
  if (bytesPerSec >= 1e3) return [num(bytesPerSec / 1e3, 0), "KB/s"];
  return [num(bytesPerSec, 0), "B/s"];
}

/** "3.2 GB", "612 MB". */
export function size(bytes: number, num: (v: number, d?: number) => string): string {
  if (bytes >= 1e12) return `${num(bytes / 1e12, 1)} TB`;
  if (bytes >= 1e9) return `${num(bytes / 1e9, 1)} GB`;
  if (bytes >= 1e6) return `${num(bytes / 1e6, 0)} MB`;
  return `${num(bytes / 1e3, 0)} KB`;
}

/** "4 h 52 m", "5 d 22 h", "12 m". */
export function span(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  if (d > 0) return `${d} d ${h} h`;
  if (h > 0) return `${h} h ${m} m`;
  return `${m} m`;
}

export function until(iso: string | null, now: number): string | null {
  if (iso === null) return null;
  const seconds = (Date.parse(iso) - now) / 1000;
  return Number.isFinite(seconds) && seconds > 0 ? span(seconds) : null;
}

/** Colour of a temperature, on the design's 40–100 °C ramp. */
export function heat(celsius: number): [number, number, number] {
  const stops: [number, [number, number, number]][] = [
    [40, [78, 110, 142]],
    [60, [110, 141, 173]],
    [70, [217, 162, 74]],
    [90, [229, 116, 58]],
    [100, [224, 71, 60]],
  ];
  const first = stops[0]![1];
  if (celsius <= stops[0]![0]) return first;
  for (let i = 1; i < stops.length; i++) {
    const [t1, c1] = stops[i]!;
    const [t0, c0] = stops[i - 1]!;
    if (celsius <= t1) {
      const f = (celsius - t0) / (t1 - t0);
      return [0, 1, 2].map((k) => Math.round(c0[k]! + (c1[k]! - c0[k]!) * f)) as [number, number, number];
    }
  }
  return stops[stops.length - 1]![1];
}

export function heatGradient(celsius: number | null, strength = 0.85): string {
  const [r, g, b] = heat(celsius ?? 40);
  return `radial-gradient(90% 80% at 50% 55%, rgba(${r},${g},${b},${strength}), rgba(${r},${g},${b},${strength * 0.24}) 66%, rgba(${r},${g},${b},0.06))`;
}

export function heatColor(celsius: number | null): string {
  const [r, g, b] = heat(celsius ?? 40);
  return `rgb(${r},${g},${b})`;
}

/* ---------- primitives ---------- */

const MARK: Partial<Record<ProviderId, string>> = {
  "claude-code": "cl",
  codex: "cx",
  antigravity: "ag",
};

export const TOOL_COLOR: Record<string, string> = {
  "claude-code": "#DE8A68",
  codex: "#DDE3E0",
  antigravity: "#9485FF",
};

export function Mark({ tool, small, label }: { tool: ProviderId; small?: boolean; label?: string }) {
  const kind = MARK[tool];
  return (
    <span
      className={`mark${small ? " sm" : ""} ${kind ?? "outline"}`}
      role={label ? "img" : undefined}
      aria-label={label}
      aria-hidden={label ? undefined : true}
    >
      {kind ? null : tool.slice(0, 2).replace(/^./, (c) => c.toUpperCase())}
    </span>
  );
}

export function Vessel({
  value,
  vertical,
  size: sizeClass,
  color,
  pace,
  empty,
  codex,
  ticks,
  style,
}: {
  value: number;
  vertical?: boolean;
  size?: "sm" | "md" | "lg";
  color?: string;
  pace?: number | null;
  empty?: boolean;
  codex?: boolean;
  ticks?: boolean;
  style?: CSSProperties;
}) {
  const fill = Math.max(0, Math.min(100, value));
  return (
    <div className={`vessel ${vertical ? "v" : "h"}${sizeClass ? ` ${sizeClass}` : ""}`} style={style} aria-hidden="true">
      <div
        className={`liquid${empty ? " empty" : ""}${codex ? " codex" : ""}`}
        style={vertical ? { height: `${empty ? 100 : fill}%`, backgroundColor: color } : { width: `${empty ? 100 : fill}%`, backgroundColor: color }}
      />
      {pace != null && <i className="pace" style={vertical ? { bottom: `${pace}%` } : { left: `${pace}%` }} />}
      {ticks && <div className="ticks" />}
    </div>
  );
}

export function Chip({ tone, dot, children }: { tone?: "ok" | "warn" | "bad"; dot?: boolean; children: ReactNode }) {
  return (
    <span className={`chip${tone ? ` ${tone}` : ""}`}>
      {dot && <span className="dot" />}
      {children}
    </span>
  );
}

/** A smooth line through `values` (nulls are skipped), scaled to its own range or `[min, max]`. */
export function Spark({
  values,
  height = 20,
  stroke = "var(--fg-2)",
  area,
  min,
  max,
  dashed,
  width = 100,
}: {
  values: (number | null)[];
  height?: number;
  stroke?: string;
  area?: string;
  min?: number;
  max?: number;
  dashed?: string;
  width?: number;
}) {
  const points = values
    .map((v, i) => (v === null || !Number.isFinite(v) ? null : ([i, v] as const)))
    .filter((p): p is readonly [number, number] => p !== null);
  if (points.length < 2) {
    return <svg className="chart" viewBox={`0 0 ${width} ${height}`} height={height} aria-hidden="true" />;
  }
  const lo = min ?? Math.min(...points.map((p) => p[1]));
  const hi = max ?? Math.max(...points.map((p) => p[1]));
  const range = hi - lo || 1;
  const n = Math.max(1, values.length - 1);
  const xy = points.map(([i, v]) => [(i / n) * width, height - 1 - ((v - lo) / range) * (height - 2)] as const);
  const d = xy.map(([x, y], i) => `${i === 0 ? "M" : "L"}${x.toFixed(1)} ${y.toFixed(1)}`).join(" ");
  const first = xy[0]!;
  const last = xy[xy.length - 1]!;
  return (
    <svg className="chart" viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" height={height} aria-hidden="true">
      {area && <path d={`${d} L${last[0]} ${height} L${first[0]} ${height} Z`} fill={area} />}
      <path className="gl" d={d} style={{ stroke }} strokeDasharray={dashed} />
    </svg>
  );
}

export function Ring({ percent, size: px = 40, stroke = 6 }: { percent: number; size?: number; stroke?: number }) {
  const r = 16;
  const c = 2 * Math.PI * r;
  const filled = (Math.max(0, Math.min(100, percent)) / 100) * c;
  return (
    <svg width={px} height={px} viewBox="0 0 40 40" aria-hidden="true" style={{ transform: "rotate(-90deg)" }}>
      <circle cx="20" cy="20" r={r} fill="none" strokeWidth={stroke} style={{ stroke: "var(--vessel)" }} />
      <circle cx="20" cy="20" r={r} fill="none" strokeWidth={stroke} strokeLinecap="round" strokeDasharray={`${filled} ${c}`} stroke="#A3ADB8" />
    </svg>
  );
}

/** The SoC heat field: P and E clusters, GPU, SoC fabric and the SSD beside the die. */
export function HeatField({ snapshot, strings, large }: { snapshot: Snapshot; strings: LiquidStrings; large?: boolean }) {
  const t = snapshot.thermal;
  const num = useNumber();
  const [pinned, setPinned] = useState<string | null>(null);
  const zones: { key: string; label: string; stat: typeof t.cpu; style: CSSProperties }[] = [
    { key: "p", label: large ? strings.zone.p.toUpperCase() : "P", stat: t.cpuPerformance ?? t.cpu, style: { gridRow: "span 2" } },
    { key: "e", label: large ? strings.zone.e.toUpperCase() : "E", stat: t.cpuEfficiency, style: large ? { gridRow: "span 2" } : {} },
    { key: "gpu", label: "GPU", stat: t.gpu, style: { gridRow: "span 2", gridColumn: 3 } },
    { key: "soc", label: large ? strings.zone.soc.toUpperCase() : "SoC", stat: t.soc, style: large ? { gridColumn: "1 / -1" } : {} },
  ];
  const rows = large ? "118px 118px 46px" : "64px 64px";
  return (
    <div style={{ display: "flex", gap: large ? 16 : 10, alignItems: "center" }}>
      <div
        className="die"
        style={{ flex: 1, display: "grid", gridTemplateColumns: large ? "1.3fr .95fr 1.5fr" : "1.25fr 1fr 1.5fr", gridTemplateRows: rows, gap: large ? 8 : 6, padding: large ? 14 : 12 }}
      >
        {zones.map((zone) => (
          <button
            type="button"
            key={zone.key}
            className={`zone clickable${pinned === zone.key ? " pin" : ""}`}
            style={{ ...zone.style, background: heatGradient(zone.stat?.average ?? null) }}
            onClick={() => setPinned(pinned === zone.key ? null : zone.key)}
            aria-label={`${zone.label} ${zone.stat ? `${num(zone.stat.average, 1)}°` : "—"}`}
          >
            <span className="zl">{zone.label}</span>
            <span className="zt" style={large && zone.key === "gpu" ? { fontSize: 18 } : undefined}>
              {zone.stat ? `${num(zone.stat.average, 1)}°` : "—"}
            </span>
            {zone.stat && (
              <div className="tip glass">
                <div style={{ fontWeight: 600, marginBottom: 4 }}>{zone.label}</div>
                <div className="tr"><span className="t3">{strings.average}</span><span className="mono">{num(zone.stat.average, 1)}°</span></div>
                <div className="tr"><span className="t3">{strings.max}</span><span className="mono">{num(zone.stat.max, 1)}°</span></div>
              </div>
            )}
          </button>
        ))}
      </div>
      <div className="die" style={{ width: large ? 84 : 54, height: large ? 132 : 72, padding: large ? 8 : 6, borderRadius: large ? 16 : 14 }}>
        <div className="zone" style={{ height: "100%", minHeight: 0, background: heatGradient(t.ssd?.average ?? null, 0.8) }}>
          <span className="zl" style={large ? undefined : { left: 6, top: 5, fontSize: 9.5 }}>SSD</span>
          <span className="zt" style={large ? undefined : { right: 6, bottom: 4, fontSize: 11.5 }}>
            {t.ssd ? `${num(t.ssd.average, 0)}°` : "—"}
          </span>
        </div>
      </div>
    </div>
  );
}

export function thermalTone(state: Snapshot["thermal"]["state"]): "ok" | "warn" | "bad" {
  return state === "nominal" ? "ok" : state === "fair" ? "warn" : "bad";
}

export function gradeTone(score: number): "ok" | "warn" | "bad" {
  return score >= 65 ? "ok" : score >= 45 ? "warn" : "bad";
}

export const HEALTH_COLOR = (score: number) => (score >= 85 ? "#5BBF84" : score >= 65 ? "#8FBF5B" : score >= 45 ? "#E0A53C" : "#E0473C");

/** Several lines on one shared scale, e.g. CPU, GPU and SSD temperature over a day. */
export function Lines({
  series,
  height = 120,
  width = 300,
  min,
  max,
  label,
}: {
  series: { values: (number | null)[]; stroke: string; dashed?: string; area?: string }[];
  height?: number;
  width?: number;
  min?: number;
  max?: number;
  label?: string;
}) {
  const all = series.flatMap((s) => s.values).filter((v): v is number => v !== null && Number.isFinite(v));
  if (all.length < 2) {
    return <svg className="chart" viewBox={`0 0 ${width} ${height}`} height={height} role={label ? "img" : undefined} aria-label={label} />;
  }
  const lo = min ?? Math.min(...all);
  const hi = max ?? Math.max(...all);
  const range = hi - lo || 1;
  const paths = series.map((s) => {
    const n = Math.max(1, s.values.length - 1);
    const xy = s.values
      .map((v, i) => (v === null || !Number.isFinite(v) ? null : ([(i / n) * width, height - 2 - ((v - lo) / range) * (height - 6)] as const)))
      .filter((p): p is readonly [number, number] => p !== null);
    const d = xy.map(([x, y], i) => `${i === 0 ? "M" : "L"}${x.toFixed(1)} ${y.toFixed(1)}`).join(" ");
    const first = xy[0];
    const last = xy[xy.length - 1];
    return { d, area: s.area && first && last ? `${d} L${last[0]} ${height} L${first[0]} ${height} Z` : null, ...s };
  });
  return (
    <svg className="chart" viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" height={height} role={label ? "img" : undefined} aria-label={label} aria-hidden={label ? undefined : true}>
      {paths.map((p, i) => p.area && <path key={`a${i}`} d={p.area} fill={p.area.startsWith("M") ? (series[i]?.area ?? "none") : "none"} />)}
      {paths.map((p, i) => (
        <path key={i} className="gl" d={p.d} style={{ stroke: p.stroke }} strokeDasharray={p.dashed} />
      ))}
      <path className="base" d={`M0 ${height} H${width}`} />
    </svg>
  );
}
