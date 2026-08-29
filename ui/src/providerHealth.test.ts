import { describe, expect, it } from "vitest";

import { confidenceForHealth, visibleProviderHealth } from "./providerHealth";
import { en } from "./i18n/en";
import { tr } from "./i18n/tr";
import type { ProviderHealth, ProviderSnapshot } from "./types";

const snapshot: ProviderSnapshot = {
  id: "codex",
  instance: "codex",
  label: null,
  installed: true,
  windows: [
    {
      limitId: "codex",
      kind: "weekly",
      windowMinutes: 10_080,
      usedPercent: 72,
      resetsAt: null,
      confidence: { level: "measured", reportedAt: "2026-08-25T10:00:00Z" },
    },
  ],
  today: { input: 0, output: 0, cacheRead: 0, cacheCreation: 0, reasoning: 0 },
  todayCost: { usd: 0, unpricedTokens: 0 },
  series: [],
  pace: [],
  lastActivity: null,
  unavailable: null,
};

function health(state: ProviderHealth["state"]): ProviderHealth {
  return {
    provider: "codex",
    state,
    lastAttemptAt: "2026-08-25T10:01:00Z",
    lastSuccessAt: "2026-08-25T10:00:00Z",
    consecutiveFailures: state === "stale" ? 1 : 0,
    lastError: state === "stale" ? "provider parser failed" : null,
    nextRetryAt: null,
  };
}

describe("provider health presentation", () => {
  it("marks a preserved measured snapshot stale after a later failure", () => {
    const presented = visibleProviderHealth(snapshot, [health("stale")]);

    expect(snapshot.windows[0]?.confidence.level).toBe("measured");
    expect(presented?.state).toBe("stale");
    expect(presented?.lastError).toBe("provider parser failed");
    expect(
      confidenceForHealth(
        snapshot.windows[0]!.confidence,
        presented,
        Date.parse("2026-08-25T10:01:00Z"),
      ).level,
    ).toBe("stale");
    expect(snapshot.windows[0]?.confidence.level).toBe("measured");
  });

  it("keeps healthy and disabled operational states out of visible warnings", () => {
    expect(visibleProviderHealth(snapshot, [health("healthy")])).toBeNull();
    expect(visibleProviderHealth(snapshot, [health("disabled")])).toBeNull();
  });

  it("keeps rebuilding visible with screen-readable EN and TR copy", () => {
    const presented = visibleProviderHealth(snapshot, [health("rebuilding")]);
    expect(presented?.state).toBe("rebuilding");
    expect(en.health.rebuilding(null)).toContain("Rebuilding");
    expect(tr.health.rebuilding(null)).toContain("yeniden oluşturuluyor");
  });

  it("matches error health to the provider and ignores another provider", () => {
    const other = { ...health("error"), provider: "claude-code" as const };
    expect(visibleProviderHealth(snapshot, [other])).toBeNull();
    expect(visibleProviderHealth(snapshot, [other, health("error")])?.state).toBe("error");
  });
});

describe("instances", () => {
  it("matches health to the instance, not to the tool", () => {
    // Two copies of one tool. The default one is fine; the named one cannot be read. Matching
    // on the tool would paint both cards with whichever entry happened to come first.
    const work: ProviderSnapshot = { ...snapshot, instance: "codex#work", label: "Work" };
    const health: ProviderHealth[] = [
      {
        provider: "codex",
        state: "healthy",
        lastAttemptAt: null,
        lastSuccessAt: null,
        consecutiveFailures: 0,
        lastError: null,
        nextRetryAt: null,
      },
      {
        provider: "codex#work",
        state: "error",
        lastAttemptAt: null,
        lastSuccessAt: null,
        consecutiveFailures: 2,
        lastError: "the log directory could not be read",
        nextRetryAt: null,
      },
    ];

    expect(visibleProviderHealth(snapshot, health)).toBeNull();
    expect(visibleProviderHealth(work, health)?.state).toBe("error");
  });
});
