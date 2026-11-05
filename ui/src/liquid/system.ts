/**
 * The Mac side of the app: live sensor readings, Docker, Antigravity and Keep Awake.
 *
 * Shapes mirror the Rust structs (`sysmon::Snapshot`, `docker::Docker`,
 * `antigravity::AntigravityState`). In the App Store build there is no system monitor, so
 * `snapshot` and `docker` stay null and every surface shows only the AI side.
 */

import { create } from "zustand";

import { call, inShell } from "../store";
import { demoAntigravity, demoDocker, demoHistory, demoSnapshot } from "./demo";

export interface Stat {
  average: number;
  max: number;
}

export type ThermalState = "nominal" | "fair" | "serious" | "critical";
export type Pressure = "normal" | "warning" | "critical";
export type Grade = "excellent" | "good" | "fair" | "poor";

export interface Snapshot {
  takenAtMs: number;
  machine: { chip: string; osVersion: string };
  cpu: {
    usage: number;
    clusters: { name: string; usage: number; cores: number[] }[];
    load: [number, number, number];
    uptimeSecs: number;
  } | null;
  memory: {
    totalBytes: number;
    usedBytes: number;
    appBytes: number;
    wiredBytes: number;
    compressedBytes: number;
    cachedBytes: number;
    swapUsedBytes: number;
    swapTotalBytes: number;
    pressure: Pressure;
  } | null;
  volumes: {
    mountPoint: string;
    name: string | null;
    fileSystem: string;
    startup: boolean;
    totalBytes: number;
    availableBytes: number;
  }[];
  diskIo: { readBytesPerSec: number; writeBytesPerSec: number } | null;
  network: { downBytesPerSec: number; upBytesPerSec: number } | null;
  processes: { pid: number; name: string; cpu: number; memoryBytes: number; powerWatts: number }[];
  thermal: {
    cpu: Stat | null;
    cpuPerformance: Stat | null;
    cpuEfficiency: Stat | null;
    gpu: Stat | null;
    soc: Stat | null;
    ssd: Stat | null;
    memory: Stat | null;
    peakCelsius: number | null;
    state: ThermalState;
  };
  fans: {
    index: number;
    rpm: number;
    minRpm: number;
    maxRpm: number;
    targetRpm: number | null;
    percent: number;
    mode: "auto" | "forced";
  }[];
  power: {
    systemWatts: number | null;
    cpuWatts: number | null;
    gpuWatts: number | null;
    aneWatts: number | null;
    dramWatts: number | null;
  };
  gpuUsage: number | null;
  smart: { failing: boolean; criticalWarning: number; percentUsed: number; celsius: number | null } | null;
  health: {
    score: number;
    grade: Grade;
    penalties: { cpu: number; memory: number; disk: number; thermal: number; io: number };
  };
}

export type ContainerHealth = "healthy" | "unhealthy" | "starting";

export interface Container {
  id: string;
  name: string;
  image: string;
  state: string;
  status: string;
  health: ContainerHealth | null;
  composeProject: string | null;
  ports: string;
  cpuPercent: number | null;
  memoryBytes: number | null;
  memoryLimitBytes: number | null;
}

export interface DockerDisk {
  kind: string;
  total: number;
  active: number;
  sizeBytes: number;
  reclaimableBytes: number;
}

export type Docker =
  | { status: "notInstalled" }
  | { status: "notRunning" }
  | { status: "running"; serverVersion: string; containers: Container[]; disk: DockerDisk[] };

export interface ModelReset {
  model: string;
  resetsAt: string;
}

export interface LiveBucket {
  group: string;
  window: string;
  remainingPercent: number;
  resetsAt: string | null;
}

export interface Antigravity {
  exhausted: ModelReset[];
  live: LiveBucket[] | null;
  liveCheckedAt: string | null;
  liveAvailable: boolean;
  /** Unix milliseconds of each prompt in the last 30 days. */
  promptTimes: number[];
}

/** One minute of history, from the backend's week-long store. */
export interface HistoryPoint {
  t: number;
  cpu: number | null;
  memory: number | null;
  cpuCelsius: number | null;
  gpuCelsius: number | null;
  ssdCelsius: number | null;
  gpu: number | null;
  watts: number | null;
  down: number | null;
  up: number | null;
  diskRead: number | null;
  diskWrite: number | null;
}

/** How far back the dashboard charts reach. */
export type Span = 1 | 24 | 168;

/** Live readings kept in memory for the sparklines, newest last. */
const LIVE_POINTS = 120;

interface SystemStore {
  snapshot: Snapshot | null;
  /** The last couple of minutes, for sparklines that move with every tick. */
  live: HistoryPoint[];
  docker: Docker | null;
  antigravity: Antigravity | null;
  awake: boolean;
  history: HistoryPoint[];
  span: Span;
  started: boolean;
  start: () => void;
  setSpan: (span: Span) => void;
  loadHistory: () => Promise<void>;
  setAwake: (on: boolean) => Promise<void>;
  refreshAntigravity: () => Promise<void>;
}

export function pointFrom(snapshot: Snapshot): HistoryPoint {
  const memory = snapshot.memory;
  return {
    t: Math.floor(snapshot.takenAtMs / 1000),
    cpu: snapshot.cpu?.usage ?? null,
    memory: memory ? (memory.usedBytes * 100) / Math.max(1, memory.totalBytes) : null,
    cpuCelsius: snapshot.thermal.cpu?.max ?? null,
    gpuCelsius: snapshot.thermal.gpu?.average ?? null,
    ssdCelsius: snapshot.thermal.ssd?.average ?? null,
    gpu: snapshot.gpuUsage,
    watts: snapshot.power.systemWatts,
    down: snapshot.network?.downBytesPerSec ?? null,
    up: snapshot.network?.upBytesPerSec ?? null,
    diskRead: snapshot.diskIo?.readBytesPerSec ?? null,
    diskWrite: snapshot.diskIo?.writeBytesPerSec ?? null,
  };
}

export const useSystem = create<SystemStore>((set, get) => ({
  snapshot: null,
  live: [],
  docker: null,
  antigravity: null,
  awake: false,
  history: [],
  span: 24,
  started: false,

  start: () => {
    if (get().started) return;
    set({ started: true });
    if (!inShell) {
      // Design work in a browser: a believable Mac, clearly not this one.
      const snapshot = demoSnapshot();
      set({
        snapshot,
        live: demoHistory(1).slice(-LIVE_POINTS),
        docker: demoDocker(),
        antigravity: demoAntigravity(),
        history: demoHistory(get().span),
      });
      return;
    }
    const accept = (snapshot: Snapshot) =>
      set((state) => ({
        snapshot,
        live: [...state.live, pointFrom(snapshot)].slice(-LIVE_POINTS),
      }));
    void (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      await listen<Snapshot>("sys://state", (event) => accept(event.payload));
      await listen<Docker>("docker://state", (event) => set({ docker: event.payload }));
      const [snapshot, docker, awake] = await Promise.all([
        call<Snapshot | null>("system_state", {}),
        call<Docker | null>("docker_state", {}),
        call<boolean>("keep_awake", {}),
      ]);
      if (snapshot.ok && snapshot.value) accept(snapshot.value);
      if (docker.ok && docker.value) set({ docker: docker.value });
      if (awake.ok) set({ awake: awake.value });
      await get().refreshAntigravity();
      await get().loadHistory();
    })();
    window.setInterval(() => void get().refreshAntigravity(), 30_000);
    window.setInterval(() => void get().loadHistory(), 60_000);
  },

  setSpan: (span) => {
    set({ span, history: inShell ? get().history : demoHistory(span) });
    void get().loadHistory();
  },

  loadHistory: async () => {
    if (!inShell) return;
    const result = await call<HistoryPoint[]>("system_history", { hours: get().span });
    if (result.ok) set({ history: result.value });
  },

  setAwake: async (on) => {
    if (!inShell) {
      set({ awake: on });
      return;
    }
    const result = await call<boolean>("set_keep_awake", { enabled: on });
    if (result.ok) set({ awake: result.value });
  },

  refreshAntigravity: async () => {
    if (!inShell) return;
    const result = await call<Antigravity>("antigravity_state", {});
    if (result.ok) set({ antigravity: result.value });
  },
}));

/** The history to chart: the stored minutes, or the live ticks while the store is still empty. */
export function chartPoints(history: HistoryPoint[], live: HistoryPoint[]): HistoryPoint[] {
  return history.length >= 2 ? history : live;
}

export function memoryPercent(snapshot: Snapshot | null): number | null {
  const memory = snapshot?.memory;
  return memory ? (memory.usedBytes * 100) / Math.max(1, memory.totalBytes) : null;
}

export function startupVolume(snapshot: Snapshot | null) {
  return snapshot?.volumes.find((volume) => volume.startup) ?? null;
}
