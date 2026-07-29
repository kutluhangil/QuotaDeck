import { describe, expect, it } from "vitest";

import { levelForRisk } from "./format";
import { paceFor } from "./types";
import type { PaceForecast, ProviderSnapshot, QuotaWindow } from "./types";

function window(limitId: string, windowMinutes: number): QuotaWindow {
  return {
    limitId,
    kind: "session",
    windowMinutes,
    usedPercent: 40,
    resetsAt: null,
    confidence: { level: "derived", basis: "token-window" },
  };
}

function pace(limitId: string, windowMinutes: number): PaceForecast {
  return { limitId, windowMinutes, projectedPercent: 70, risk: "healthy", exhaustedAt: null };
}

function snapshot(paces: PaceForecast[]): ProviderSnapshot {
  return {
    id: "claude-code",
    installed: true,
    windows: [],
    today: { input: 0, output: 0, cacheRead: 0, cacheCreation: 0, reasoning: 0 },
    todayCost: { usd: 0, unpricedTokens: 0 },
    series: [],
    pace: paces,
    lastActivity: null,
    unavailable: null,
  };
}

describe("paceFor", () => {
  it("separates two windows sharing one limit id", () => {
    // Claude Code reports its five-hour and its weekly limit under one id, and they run out
    // at very different rates. Matching on the id alone would put the weekly forecast on the
    // session row.
    const deck = snapshot([pace("claude", 300), pace("claude", 10_080)]);
    expect(paceFor(deck, window("claude", 300))?.windowMinutes).toBe(300);
    expect(paceFor(deck, window("claude", 10_080))?.windowMinutes).toBe(10_080);
  });

  it("returns null for a window nothing could be projected for", () => {
    // Every refusal in core/src/pace.rs arrives here as an absent entry, and an absent entry
    // must render as no line rather than as a zero.
    expect(paceFor(snapshot([]), window("codex", 10_080))).toBeNull();
    expect(paceFor(snapshot([pace("codex", 300)]), window("codex", 10_080))).toBeNull();
  });
});

describe("levelForRisk", () => {
  it("maps the pace bands onto the shared ramp", () => {
    expect(levelForRisk("healthy")).toBe("ample");
    expect(levelForRisk("at-risk")).toBe("tight");
    expect(levelForRisk("over")).toBe("critical");
  });
});
