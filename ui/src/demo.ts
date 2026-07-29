/**
 * Fixture used when the UI runs outside the Tauri shell, so the panel can be designed and
 * reviewed in a browser. The shapes mirror what Phase 2 measured on a real machine: Codex
 * reporting a weekly window plus a stale monthly one, and a tool that never reported.
 *
 * This is not the shipped demo mode. That lands in Phase 9 and has to be reachable from
 * inside the app before purchase.
 */

import type {
  Bucket,
  DeckState,
  ProviderPlans,
  ProviderSnapshot,
  StatuslineState,
} from "./types";

/**
 * Buckets shaped like real work rather than like a wave: sessions of an hour or two with long
 * quiet stretches between them, and the occasional burst an order of magnitude above the
 * rest. A smooth series would make the strip look good and hide exactly the cases it has to
 * survive — a single spike, a week of gaps, one lonely bucket at the left edge.
 *
 * Deterministic, so the panel looks the same on every reload while it is being designed.
 */
function series(now: number, spanSeconds: number, seed: number): Bucket[] {
  const end = Math.floor(now / 1000 / 300) * 300;
  const start = end - spanSeconds;
  const buckets: Bucket[] = [];

  const random = () => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed / 0x7fffffff;
  };

  for (let at = start; at <= end; at += 300) {
    // Working hours, roughly: nothing overnight, and not every day.
    const hour = new Date(at * 1000).getHours();
    const active = hour >= 9 && hour <= 23 && random() > 0.55;
    if (!active) continue;

    const burst = random() > 0.97 ? 14 : 1;
    const base = (8_000 + random() * 26_000) * burst;
    const tokens = {
      input: Math.round(base),
      output: Math.round(base / 12),
      // Cache reads dominate every real total measured in Phase 0.
      cacheRead: Math.round(base * 9),
      cacheCreation: 0,
      reasoning: 0,
    };
    buckets.push({
      start: at,
      tokens,
      requests: 0,
      // Roughly Opus rates, so the demo's dollar figures land in a believable range.
      costUsd: (tokens.input * 5e-6 + tokens.output * 2.5e-5 + tokens.cacheRead * 5e-7),
      unpricedTokens: 0,
    });
  }

  return buckets;
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
    // Codex names no model in its records and the price table covers Anthropic only, so its
    // tokens are counted and left unpriced rather than billed at zero.
    todayCost: { usd: 0, unpricedTokens: 133_183_948 },
    // A weekly window, so the strip draws seven days.
    series: series(now, 7 * 86_400, 0x5eed),
    // Only the weekly window: the monthly reading is stale, and a projection is never built
    // on a reading the panel has already marked old.
    pace: [
      {
        limitId: "codex",
        windowMinutes: 10_080,
        projectedPercent: 93,
        risk: "at-risk",
        exhaustedAt: null,
      },
    ],
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
    todayCost: { usd: 86.42, unpricedTokens: 0 },
    // The card leads with the fullest window, which here is the weekly one at 95%.
    series: series(now, 7 * 86_400, 0xc0ffee),
    // The weekly limit runs out before it resets; the session one coasts. Both shapes are
    // here so the card is designed against a named instant and a bare projection at once.
    pace: [
      {
        limitId: "claude",
        windowMinutes: 300,
        projectedPercent: 61,
        risk: "healthy",
        exhaustedAt: null,
      },
      {
        limitId: "claude",
        windowMinutes: 10_080,
        projectedPercent: 118,
        risk: "over",
        exhaustedAt: iso(2.1 * 3_600_000),
      },
    ],
    lastActivity: iso(-30_000),
    unavailable: null,
  };

  const copilot: ProviderSnapshot = {
    id: "copilot-cli",
    installed: true,
    windows: [],
    today: { input: 0, output: 0, cacheRead: 0, cacheCreation: 0, reasoning: 0 },
    todayCost: { usd: 0, unpricedTokens: 0 },
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

/** Mirrors what `provider_plans` returns, so the settings view can be designed in a browser. */
export function demoPlans(): ProviderPlans[] {
  return [
    {
      provider: "claude-code",
      plans: [
        {
          id: "pro",
          label: "Pro",
          ceilings: [
            { windowMinutes: 300, costUsd: 5 },
            { windowMinutes: 10_080, costUsd: 35 },
          ],
        },
        {
          id: "max-5x",
          label: "Max 5x",
          ceilings: [
            { windowMinutes: 300, costUsd: 25 },
            { windowMinutes: 10_080, costUsd: 175 },
          ],
        },
        {
          id: "max-20x",
          label: "Max 20x",
          ceilings: [
            { windowMinutes: 300, costUsd: 100 },
            { windowMinutes: 10_080, costUsd: 700 },
          ],
        },
      ],
    },
    {
      // Five tiers against Claude Code's three, and a hint that reads differently. Both are
      // here so the settings view is designed against the wider of the two shapes.
      provider: "copilot-cli",
      plans: [
        { id: "pro", label: "Pro", ceilings: [{ windowMinutes: 43_200, costUsd: 15 }] },
        { id: "pro-plus", label: "Pro+", ceilings: [{ windowMinutes: 43_200, costUsd: 70 }] },
        { id: "max", label: "Max", ceilings: [{ windowMinutes: 43_200, costUsd: 200 }] },
        { id: "business", label: "Business", ceilings: [{ windowMinutes: 43_200, costUsd: 19 }] },
        {
          id: "enterprise",
          label: "Enterprise",
          ceilings: [{ windowMinutes: 43_200, costUsd: 39 }],
        },
      ],
    },
  ];
}

/** The pre-install state, which is the one worth designing against. */
export function demoStatusline(): StatuslineState {
  return {
    supported: true,
    installed: false,
    settingsPath: "/Users/you/.claude/settings.json",
    currentCommand: "npx -y ccstatusline@latest",
    proposedCommand:
      "'/Applications/Quota Deck.app/Contents/MacOS/quotadeck-statusline' --log '/Users/you/Library/Application Support/QuotaDeck/claude-code/statusline' --chain 'npx -y ccstatusline@latest'",
    previousCommand: "npx -y ccstatusline@latest",
    readings: 0,
    lastReadingAt: null,
  };
}
