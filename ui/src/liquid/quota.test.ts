import { describe, expect, it } from "vitest";

import type { DeckState, ProviderSnapshot } from "../types";
import { evenUseLeft, modelName, openingSoonest, toolGlance } from "./quota";

const now = Date.parse("2026-11-05T12:00:00Z");

function snapshot(partial: Partial<ProviderSnapshot>): ProviderSnapshot {
  const empty = { input: 0, output: 0, cacheRead: 0, cacheCreation: 0, reasoning: 0 };
  return {
    id: "claude-code",
    instance: "claude-code",
    label: null,
    installed: true,
    windows: [],
    today: empty,
    todayCost: { usd: 0, unpricedTokens: 0 },
    month: empty,
    monthCost: { usd: 0, unpricedTokens: 0 },
    series: [],
    pace: [],
    lastActivity: null,
    unavailable: null,
    ...partial,
  };
}

function deck(providers: ProviderSnapshot[]): DeckState {
  return { providers } as unknown as DeckState;
}

describe("toolGlance", () => {
  it("leads with the window closest to full and turns used into left", () => {
    const measured = { level: "measured", reportedAt: "2026-11-05T11:00:00Z" } as const;
    const claude = snapshot({
      windows: [
        { limitId: "c", kind: "session", windowMinutes: 300, usedPercent: 10, resetsAt: "2026-11-05T14:30:00Z", confidence: measured },
        { limitId: "c", kind: "weekly", windowMinutes: 10080, usedPercent: 33, resetsAt: "2026-11-08T12:00:00Z", confidence: measured },
      ],
    });
    const result = toolGlance(deck([claude]), "claude-code", now);
    expect(result.installed).toBe(true);
    expect(result.headline?.left).toBe(67);
    expect(result.headline?.windowMinutes).toBe(10080);
    expect(result.windows.map((w) => w.windowMinutes)).toEqual([300, 10080]);
    expect(result.headline?.measured).toBe(true);
  });

  it("reports a missing tool as not installed", () => {
    expect(toolGlance(deck([]), "codex", now)).toMatchObject({ installed: false, headline: null });
  });
});

describe("evenUseLeft", () => {
  it("is the share of the window still ahead", () => {
    const window = {
      limitId: "x",
      kind: "session" as const,
      windowMinutes: 300,
      usedPercent: 0,
      resetsAt: "2026-11-05T14:30:00Z",
      confidence: { level: "measured" as const, reportedAt: "" },
    };
    expect(evenUseLeft(window, now)).toBe(50);
    expect(evenUseLeft({ ...window, resetsAt: null }, now)).toBeNull();
  });
});

describe("antigravity", () => {
  it("lists still-exhausted models soonest first", () => {
    const state = {
      exhausted: [
        { model: "gemini-3.8-pro-high", resetsAt: "2026-11-05T16:00:00Z" },
        { model: "gemini-3.1-flash-image", resetsAt: "2026-11-05T13:00:00Z" },
        { model: "old", resetsAt: "2026-11-05T11:00:00Z" },
      ],
      live: null,
      liveCheckedAt: null,
      liveAvailable: true,
      promptTimes: [],
    };
    expect(openingSoonest(state, now).map((r) => r.model)).toEqual([
      "gemini-3.1-flash-image",
      "gemini-3.8-pro-high",
    ]);
  });

  it("names models the way people say them", () => {
    expect(modelName("gemini-3.8-pro-high")).toBe("Pro");
    expect(modelName("gemini-3.1-flash-image")).toBe("Flash Image");
    expect(modelName("mystery")).toBe("mystery");
  });
});

describe("perDay", () => {
  it("bins times into local days, oldest first", async () => {
    const { perDay } = await import("./AiTab");
    const noon = new Date(2026, 10, 5, 12).getTime();
    const yesterday = new Date(2026, 10, 4, 9).getTime();
    expect(perDay([noon, noon - 3600_000, yesterday, noon - 40 * 86400_000], 3, noon)).toEqual([0, 1, 2]);
  });
});

describe("costText", () => {
  it("never shows unknown-price tokens as zero dollars", async () => {
    const { costText } = await import("./AiTab");
    const usd = (v: number) => `$${v}`;
    expect(costText({ usd: 0, unpricedTokens: 5000 }, usd)).toEqual({ value: "—", partial: true });
    expect(costText({ usd: 12, unpricedTokens: 5 }, usd)).toEqual({ value: "$12", partial: true });
    expect(costText({ usd: 0, unpricedTokens: 0 }, usd)).toEqual({ value: "$0", partial: false });
  });
});
