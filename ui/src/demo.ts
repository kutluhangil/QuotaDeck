/**
 * Fixture used when the UI runs outside the Tauri shell, so the panel can be designed and
 * reviewed in a browser. The shapes mirror what Phase 2 measured on a real machine: Codex
 * reporting a weekly window plus a stale monthly one, and a tool that never reported.
 *
 * This is not the shipped demo mode. That lands in Phase 9 and has to be reachable from
 * inside the app before purchase.
 */

import type { Bucket, DeckState, ProviderSnapshot } from "./types";

function series(now: number, count: number): Bucket[] {
  const start = Math.floor(now / 1000 / 300) * 300 - count * 300;
  return Array.from({ length: count }, (_, i) => {
    const wave = Math.abs(Math.sin(i / 4)) * 40_000;
    return {
      start: start + i * 300,
      tokens: {
        input: Math.round(wave),
        output: Math.round(wave / 12),
        cacheRead: Math.round(wave * 9),
        cacheCreation: 0,
        reasoning: 0,
      },
      requests: 0,
    };
  });
}

export function demoDeck(): DeckState {
  const now = Date.now();
  const iso = (offsetMs: number) => new Date(now + offsetMs).toISOString();

  const codex: ProviderSnapshot = {
    id: "codex",
    installed: true,
    windows: [
      {
        limitId: "codex",
        kind: "weekly",
        windowMinutes: 10_080,
        usedPercent: 80,
        resetsAt: iso(6.8 * 86_400_000),
        confidence: { level: "measured", reportedAt: iso(-4 * 60_000) },
      },
      {
        limitId: "codex",
        kind: "monthly",
        windowMinutes: 43_200,
        usedPercent: 72,
        resetsAt: iso(23 * 86_400_000),
        confidence: { level: "stale", reportedAt: iso(-571_678_000), ageSeconds: 571_678 },
      },
    ],
    today: {
      input: 6_167_645,
      output: 448_367,
      cacheRead: 126_567_936,
      cacheCreation: 0,
      reasoning: 0,
    },
    series: series(now, 60),
    pace: [],
    lastActivity: iso(-9 * 60_000),
    unavailable: null,
  };

  const claude: ProviderSnapshot = {
    id: "claude-code",
    installed: true,
    windows: [
      {
        limitId: "claude",
        kind: "session",
        windowMinutes: 300,
        usedPercent: 44,
        resetsAt: iso(76 * 60_000),
        confidence: { level: "measured", reportedAt: iso(-30_000) },
      },
      {
        limitId: "claude",
        kind: "weekly",
        windowMinutes: 10_080,
        usedPercent: 95,
        resetsAt: iso(2.6 * 86_400_000),
        confidence: { level: "measured", reportedAt: iso(-30_000) },
      },
    ],
    today: {
      input: 12_408,
      output: 84_112,
      cacheRead: 4_902_118,
      cacheCreation: 118_004,
      reasoning: 0,
    },
    series: series(now, 60),
    pace: [],
    lastActivity: iso(-30_000),
    unavailable: null,
  };

  const copilot: ProviderSnapshot = {
    id: "copilot-cli",
    installed: true,
    windows: [],
    today: { input: 0, output: 0, cacheRead: 0, cacheCreation: 0, reasoning: 0 },
    series: [],
    pace: [],
    lastActivity: iso(-13 * 86_400_000),
    unavailable: "never-reported",
  };

  return {
    providers: [claude, codex, copilot],
    updatedAt: iso(0),
    scanning: false,
  };
}
