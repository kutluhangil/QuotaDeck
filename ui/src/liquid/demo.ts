/**
 * Sample Mac readings for design work in a plain browser. Never shown inside the app: the
 * shell always has the real thing or nothing.
 */

import type { Antigravity, Docker, HistoryPoint, Snapshot } from "./system";

const GB = 1024 ** 3;

export function demoSnapshot(): Snapshot {
  return {
    takenAtMs: Date.now(),
    machine: { chip: "Apple M6", osVersion: "27.2" },
    cpu: {
      usage: 18,
      clusters: [
        { name: "Efficiency", usage: 34, cores: [41, 36, 30, 29, 22, 18] },
        { name: "Super", usage: 12, cores: [14, 9] },
        { name: "Performance", usage: 6, cores: [7, 6, 5, 7] },
      ],
      load: [2.41, 2.86, 3.02],
      uptimeSecs: 47 * 3600 + 1200,
    },
    memory: {
      totalBytes: 32 * GB,
      usedBytes: 24.3 * GB,
      appBytes: 11.1 * GB,
      wiredBytes: 3.9 * GB,
      compressedBytes: 9.3 * GB,
      cachedBytes: 6.2 * GB,
      swapUsedBytes: 5.1 * GB,
      swapTotalBytes: 6 * GB,
      pressure: "normal",
    },
    volumes: [
      { mountPoint: "/", name: null, fileSystem: "apfs", startup: true, totalBytes: 2000e9, availableBytes: 1628e9 },
      { mountPoint: "/Volumes/Projects", name: "Projects", fileSystem: "apfs", startup: false, totalBytes: 250e9, availableBytes: 216e9 },
    ],
    diskIo: { readBytesPerSec: 38e6, writeBytesPerSec: 12e6 },
    network: { downBytesPerSec: 2.4e6, upBytesPerSec: 180e3 },
    processes: [
      { pid: 812, name: "Docker VM", cpu: 6.4, memoryBytes: 3.2e9, powerWatts: 1.8 },
      { pid: 2291, name: "Code Helper (Renderer)", cpu: 4.1, memoryBytes: 1.9e9, powerWatts: 0.9 },
      { pid: 40417, name: "claude", cpu: 3.2, memoryBytes: 612e6, powerWatts: 0.6 },
      { pid: 398, name: "WindowServer", cpu: 2.7, memoryBytes: 410e6, powerWatts: 0.7 },
      { pid: 5120, name: "Safari Web Content", cpu: 1.4, memoryBytes: 880e6, powerWatts: 0.3 },
      { pid: 41802, name: "codex", cpu: 1.1, memoryBytes: 298e6, powerWatts: 0.2 },
      { pid: 1187, name: "Quota Deck", cpu: 0.4, memoryBytes: 74e6, powerWatts: 0.1 },
    ],
    thermal: {
      cpu: { average: 51.9, max: 60.5 },
      cpuPerformance: { average: 52.7, max: 60.5 },
      cpuEfficiency: { average: 50.4, max: 51.2 },
      gpu: { average: 49, max: 50.3 },
      soc: { average: 47.8, max: 48.6 },
      ssd: { average: 44, max: 45.1 },
      memory: null,
      peakCelsius: 60.5,
      state: "nominal",
    },
    fans: [{ index: 0, rpm: 999, minRpm: 1000, maxRpm: 4900, targetRpm: 1000, percent: 20.4, mode: "auto" }],
    power: { systemWatts: 21.2, cpuWatts: null, gpuWatts: 0.06, aneWatts: null, dramWatts: null },
    gpuUsage: 12,
    smart: { failing: false, criticalWarning: 0, percentUsed: 0, celsius: 44 },
    health: {
      score: 92,
      grade: "excellent",
      penalties: { cpu: 0, memory: 7.9, disk: 0, thermal: 0, io: 0 },
    },
  };
}

export function demoDocker(): Docker {
  const running = (name: string, project: string | null, image: string, health: "healthy" | "unhealthy" | null, cpu: number, mem: number) => ({
    id: name,
    name,
    image,
    state: "running",
    status: "Up 2 days",
    health,
    composeProject: project,
    ports: "",
    cpuPercent: cpu,
    memoryBytes: mem,
    memoryLimitBytes: 4.8 * GB,
  });
  return {
    status: "running",
    serverVersion: "29.8.2",
    containers: [
      running("api", "quota-api", "quota-api:1.4.2", "healthy", 2.1, 640e6),
      running("worker", "quota-api", "quota-worker:1.4.2", "healthy", 0.8, 410e6),
      running("redis", "quota-api", "redis:7.4-alpine", "healthy", 0.3, 64e6),
      running("uptime-kuma", "home-server", "louislam/uptime-kuma:2", "unhealthy", 1.2, 186e6),
      running("homepage", "home-server", "gethomepage/homepage:1.6", null, 0.3, 120e6),
      {
        id: "jellyfin",
        name: "jellyfin",
        image: "jellyfin/jellyfin:10.11",
        state: "exited",
        status: "Exited (0) 3 days ago",
        health: null,
        composeProject: "home-server",
        ports: "",
        cpuPercent: null,
        memoryBytes: null,
        memoryLimitBytes: null,
      },
      running("postgres", null, "postgres:17", "healthy", 1.9, 1.6e9),
    ],
    disk: [
      { kind: "Images", total: 9, active: 6, sizeBytes: 18.4e9, reclaimableBytes: 3e9 },
      { kind: "Containers", total: 7, active: 6, sizeBytes: 0.2e9, reclaimableBytes: 0 },
      { kind: "Local Volumes", total: 4, active: 3, sizeBytes: 9.2e9, reclaimableBytes: 1e9 },
      { kind: "Build Cache", total: 12, active: 0, sizeBytes: 4.1e9, reclaimableBytes: 4.1e9 },
    ],
  };
}

export function demoAntigravity(): Antigravity {
  return {
    exhausted: [
      { model: "gemini-3.8-pro-high", resetsAt: new Date(Date.now() + (4 * 60 + 52) * 60_000).toISOString() },
    ],
    live: null,
    liveCheckedAt: null,
    liveAvailable: true,
    promptTimes: Array.from({ length: 140 }, (_, i) => Date.now() - Math.floor((i * 30 * 86400_000) / 140) - (i % 5) * 3600_000),
  };
}

/** Smooth, slightly noisy curves over `hours`, at most 360 points. */
export function demoHistory(hours: number): HistoryPoint[] {
  const now = Math.floor(Date.now() / 1000);
  const count = 360;
  const step = Math.max(1, Math.floor((hours * 3600) / count));
  return Array.from({ length: count }, (_, i) => {
    const x = i / count;
    const wave = Math.sin(x * Math.PI * 4) * 0.5 + Math.sin(x * Math.PI * 11) * 0.2;
    return {
      t: now - (count - i) * step,
      cpu: 18 + wave * 14 + (i % 7) * 0.6,
      memory: 74 + wave * 3,
      cpuCelsius: 52 + wave * 6,
      gpuCelsius: 48 + wave * 3,
      ssdCelsius: 43 + wave * 1.5,
      gpu: 10 + wave * 8,
      watts: 21 + wave * 4,
      down: 2.2e6 + wave * 1.2e6,
      up: 1.8e5 + wave * 6e4,
      diskRead: 30e6 + wave * 12e6,
      diskWrite: 10e6 + wave * 4e6,
    };
  });
}
