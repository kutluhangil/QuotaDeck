/**
 * Fixture used when the UI runs outside the Tauri shell, so the panel can be designed and
 * reviewed in a browser. The shapes mirror what Phase 2 measured on a real machine: Codex
 * reporting a weekly window plus a stale monthly one, and a tool that never reported.
 *
 * This is not the shipped demo mode. That lands in Phase 9 and has to be reachable from
 * inside the app before purchase.
 */

import type {
  BreakdownPoint,
  Bucket,
  DeckState,
  HistoryPoint,
  ProviderHistory,
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
    instance: "codex",
    label: null,
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
    instance: "claude-code",
    label: null,
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
    instance: "copilot-cli",
    label: null,
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
    health: ["claude-code", "codex", "copilot-cli"].map((provider) => ({
      provider: provider as ProviderSnapshot["id"],
      state: "healthy" as const,
      lastAttemptAt: iso(0),
      lastSuccessAt: iso(0),
      consecutiveFailures: 0,
      lastError: null,
      nextRetryAt: null,
    })),
    updatedAt: iso(0),
    scanning: false,
    refreshing: false,
    refreshGeneration: 0,
    refreshError: null,
    retention: { requestedDays: 32, effectiveDays: 32, rebuilding: false, error: null },
  };
}

/**
 * A month of hourly history, so the dashboard's ranges and heatmap can be designed without
 * the shell. Sparse on purpose: real usage leaves whole days blank, and a full grid would
 * hide the one thing a heatmap is for.
 */
function hourly(days: number, seed: number, costPerToken: number): HistoryPoint[] {
  const nowHour = Math.floor(Date.now() / 3_600_000) * 3600;
  const points: HistoryPoint[] = [];

  const random = () => {
    seed = (seed * 1103515245 + 12345) & 0x7fffffff;
    return seed / 0x7fffffff;
  };

  for (let hour = nowHour - days * 24 * 3600; hour <= nowHour; hour += 3600) {
    const at = new Date(hour * 1000);
    const weekday = at.getDay() !== 0 && at.getDay() !== 6;
    const working = at.getHours() >= 9 && at.getHours() <= 22;
    if (!working || random() > (weekday ? 0.55 : 0.15)) continue;

    const base = Math.round((40_000 + random() * 180_000) * (random() > 0.94 ? 6 : 1));
    const tokens = {
      input: Math.round(base / 10),
      output: Math.round(base / 40),
      cacheRead: base,
      cacheCreation: 0,
      reasoning: 0,
    };
    const total = tokens.input + tokens.output + tokens.cacheRead;
    points.push({
      start: hour,
      tokens,
      cost: {
        usd: total * costPerToken,
        unpricedTokens: costPerToken > 0 ? 0 : total,
      },
    });
  }

  return points;
}

/**
 * Splits an hourly series across labels, so the sample deck shows a breakdown built from the
 * same numbers as its own totals rather than a second invented set that disagrees with them.
 *
 * `shares` must sum to 1. A `null` label is carried through as one, because a provider that
 * reports no model is a real state the list has to be able to draw.
 */
function split(
  hours: HistoryPoint[],
  shares: [label: string | null, share: number][],
): BreakdownPoint[] {
  const points: BreakdownPoint[] = [];
  for (const hour of hours) {
    for (const [label, share] of shares) {
      points.push({
        start: hour.start,
        label,
        tokens: {
          input: Math.round(hour.tokens.input * share),
          output: Math.round(hour.tokens.output * share),
          cacheRead: Math.round(hour.tokens.cacheRead * share),
          cacheCreation: Math.round(hour.tokens.cacheCreation * share),
          reasoning: Math.round(hour.tokens.reasoning * share),
        },
        cost: {
          usd: hour.cost.usd * share,
          unpricedTokens: Math.round(hour.cost.unpricedTokens * share),
        },
      });
    }
  }
  return points;
}

export function demoHistory(): ProviderHistory[] {
  const claude = hourly(30, 0xc0ffee, 1.4e-6);
  // Codex names no model in any record, so its history carries tokens and no dollars.
  const codex = hourly(30, 0x5eed, 0);
  return [
    {
      id: "claude-code",
      hours: claude,
      models: split(claude, [
        ["claude-opus-5", 0.58],
        ["claude-sonnet-5", 0.34],
        ["claude-haiku-4-5", 0.08],
      ]),
      modelsDropped: 0,
      // Two directories ending in the same segment, so the sample exercises the shortening
      // rule rather than only the easy case.
      projects: split(claude, [
        ["/Volumes/Vault/QuotaDeck", 0.62],
        ["/Volumes/Vault/Archives/app", 0.27],
        ["/Volumes/Vault/Ledger/app", 0.11],
      ]),
      projectsDropped: 0,
      // Claude Code is the one tool that writes a transcript per agent, so it is the only one
      // whose sample carries more than the main thread.
      agents: split(claude, [
        ["main", 0.55],
        ["subagent", 0.31],
        ["workflow", 0.14],
      ]),
      agentsDropped: 0,
    },
    {
      id: "codex",
      hours: codex,
      // One label, and it is `null`: the sample must show the state a real Codex install is in
      // rather than inventing the model name the tool never wrote.
      models: split(codex, [[null, 1]]),
      modelsDropped: 0,
      // Codex does name the directory, in the record that opens a rollout file.
      projects: split(codex, [["/Volumes/Vault/QuotaDeck", 1]]),
      projectsDropped: 0,
      agents: split(codex, [["main", 1]]),
      agentsDropped: 0,
    },
    {
      id: "copilot-cli",
      hours: [],
      models: [],
      modelsDropped: 0,
      projects: [],
      projectsDropped: 0,
      agents: [],
      agentsDropped: 0,
    },
  ];
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
      // Individual plans only. Business and Enterprise contribute credits to an
      // organisation-level pool, so a local user's logs cannot yield an honest denominator.
      provider: "copilot-cli",
      plans: [
        { id: "pro", label: "Pro", ceilings: [{ windowMinutes: 43_200, costUsd: 15 }] },
        { id: "pro-plus", label: "Pro+", ceilings: [{ windowMinutes: 43_200, costUsd: 70 }] },
        { id: "max", label: "Max", ceilings: [{ windowMinutes: 43_200, costUsd: 200 }] },
      ],
    },
  ];
}

/** The pre-install state, which is the one worth designing against. */
export function demoStatusline(): StatuslineState {
  return {
    setupMode: "automatic",
    installed: false,
    settingsPath: "/Users/you/.claude/settings.json",
    currentStatusLine: { type: "command", command: "npx -y ccstatusline@latest" },
    currentCommand: "npx -y ccstatusline@latest",
    proposedStatusLine: {
      type: "command",
      command:
        "'/Applications/Quota Deck.app/Contents/MacOS/quotadeck' --statusline-helper --log '/Users/you/Library/Application Support/QuotaDeck/claude-code/statusline' --chain 'npx -y ccstatusline@latest'",
    },
    proposedCommand:
      "'/Applications/Quota Deck.app/Contents/MacOS/quotadeck' --statusline-helper --log '/Users/you/Library/Application Support/QuotaDeck/claude-code/statusline' --chain 'npx -y ccstatusline@latest'",
    previousCommand: "npx -y ccstatusline@latest",
    previousStatusLine: null,
    manualRevertMode: null,
    readings: 0,
    lastReadingAt: null,
  };
}
