import { describe, expect, it } from "vitest";

import {
  foldBreakdown,
  modelsDroppedFor,
  modelsFor,
  projectsDroppedFor,
  projectsFor,
  shortenPaths,
} from "./breakdown";
import { rollingRange } from "./history";
import type { BreakdownPoint, ProviderHistory } from "./types";

const NOW = 1_785_715_200;
const HOUR = 3_600;

function range(days: "day" | "week" = "week") {
  return rollingRange(days, NOW);
}

function point(
  startOffsetHours: number,
  label: string | null,
  tokens: number,
  usd: number,
  unpricedTokens = 0,
): BreakdownPoint {
  return {
    start: NOW - startOffsetHours * HOUR,
    label,
    tokens: {
      input: tokens,
      output: 0,
      cacheRead: 0,
      cacheCreation: 0,
      reasoning: 0,
    },
    cost: { usd, unpricedTokens },
  };
}

describe("foldBreakdown", () => {
  it("returns nothing for no points rather than a fabricated row", () => {
    expect(foldBreakdown([], range())).toEqual([]);
  });

  it("folds every hour carrying the same label into one row", () => {
    const rows = foldBreakdown(
      [point(1, "opus", 100, 3), point(2, "opus", 50, 1.5)],
      range(),
    );
    expect(rows).toHaveLength(1);
    expect(rows[0]?.tokens).toBe(150);
    expect(rows[0]?.costUsd).toBeCloseTo(4.5, 10);
  });

  it("excludes points outside the rolling range", () => {
    const rows = foldBreakdown(
      [point(2, "inside", 100, 1), point(24 * 8, "outside", 999, 9)],
      range(),
    );
    expect(rows.map((row) => row.label)).toEqual(["inside"]);
  });

  it("keeps a null label null instead of naming it", () => {
    const rows = foldBreakdown([point(1, null, 100, 1)], range("day"));
    expect(rows).toHaveLength(1);
    expect(rows[0]?.label).toBeNull();
  });

  it("sorts by cost descending when everything carried a price", () => {
    const rows = foldBreakdown(
      [point(1, "cheap", 9000, 0.4), point(1, "dear", 100, 12)],
      range(),
    );
    expect(rows.map((row) => row.label)).toEqual(["dear", "cheap"]);
  });

  it("shares sum to one across the rows", () => {
    const rows = foldBreakdown(
      [point(1, "a", 100, 3), point(1, "b", 100, 1), point(2, "c", 100, 6)],
      range(),
    );
    const total = rows.reduce((sum, row) => sum + row.share, 0);
    expect(total).toBeCloseTo(1, 10);
    expect(rows[0]?.share).toBeCloseTo(0.6, 10);
  });

  it("ranks on tokens when any row could not be priced", () => {
    // A model with no known price bills at zero dollars, and ranking on cost would sort the
    // heaviest consumer in the range last — the exact row worth seeing.
    const rows = foldBreakdown(
      [point(1, "priced", 100, 5), point(1, "unpriced", 50_000, 0, 50_000)],
      range(),
    );
    expect(rows.map((row) => row.label)).toEqual(["unpriced", "priced"]);
    expect(rows[0]?.unpricedTokens).toBe(50_000);
    expect(rows[0]?.costUsd).toBe(0);
  });

  it("carries unpriced tokens rather than folding them into the dollar figure", () => {
    const rows = foldBreakdown([point(1, "m", 400, 1.25, 400)], range());
    expect(rows[0]?.costUsd).toBeCloseTo(1.25, 10);
    expect(rows[0]?.unpricedTokens).toBe(400);
  });

  it("gives every row a zero share when nothing was counted", () => {
    const rows = foldBreakdown([point(1, "m", 0, 0)], range());
    expect(rows[0]?.share).toBe(0);
  });

  it("narrows with the range", () => {
    const points = [point(2, "recent", 100, 1), point(48, "older", 100, 1)];
    expect(foldBreakdown(points, range("day")).map((row) => row.label)).toEqual(["recent"]);
    expect(foldBreakdown(points, range())).toHaveLength(2);
  });
});

describe("modelsFor", () => {
  const history: ProviderHistory[] = [
    {
      id: "codex",
      hours: [],
      models: [point(1, null, 100, 1)],
      modelsDropped: 3,
      projects: [point(1, "/work/one", 100, 1), point(2, "/work/two", 50, 0.5)],
      projectsDropped: 7,
      agents: [point(1, "subagent", 100, 1)],
      agentsDropped: 0,
    },
  ];

  it("finds a provider's points", () => {
    expect(modelsFor(history, "codex")).toHaveLength(1);
  });

  it("returns an empty array for a provider the backend sent nothing for", () => {
    expect(modelsFor(history, "claude-code")).toEqual([]);
    expect(modelsDroppedFor(history, "claude-code")).toBe(0);
  });

  it("reports the dropped count", () => {
    expect(modelsDroppedFor(history, "codex")).toBe(3);
  });
});

describe("projectsFor", () => {
  const history: ProviderHistory[] = [
    {
      id: "codex",
      hours: [],
      models: [],
      modelsDropped: 0,
      projects: [point(1, "/work/one", 100, 1)],
      projectsDropped: 7,
      agents: [point(1, "subagent", 100, 1)],
      agentsDropped: 0,
    },
  ];

  it("finds a provider's points", () => {
    expect(projectsFor(history, "codex")).toHaveLength(1);
  });

  it("returns an empty array for a provider the backend sent nothing for", () => {
    expect(projectsFor(history, "claude-code")).toEqual([]);
    expect(projectsDroppedFor(history, "claude-code")).toBe(0);
  });

  it("reports the dropped count", () => {
    expect(projectsDroppedFor(history, "codex")).toBe(7);
  });
});

describe("shortenPaths", () => {
  it("keeps the last segment when that is already unambiguous", () => {
    const short = shortenPaths(["/Volumes/Vault/QuotaDeck", "/Volumes/Vault/LiftGenius"]);
    expect(short.get("/Volumes/Vault/QuotaDeck")).toBe("QuotaDeck");
    expect(short.get("/Volumes/Vault/LiftGenius")).toBe("LiftGenius");
  });

  it("grows only the labels that would otherwise collide", () => {
    // Two real directories here end in `app`. Showing both as "app" would read as one
    // project holding twice the spend it has.
    const short = shortenPaths(["/work/Archives/app", "/work/Ledger/app", "/work/Deck"]);
    expect(short.get("/work/Archives/app")).toBe("Archives/app");
    expect(short.get("/work/Ledger/app")).toBe("Ledger/app");
    expect(short.get("/work/Deck")).toBe("Deck");
  });

  it("falls back to the whole path when nothing shorter separates two labels", () => {
    const short = shortenPaths(["/one/work/app", "/two/work/app"]);
    expect(short.get("/one/work/app")).toBe("one/work/app");
    expect(short.get("/two/work/app")).toBe("two/work/app");
  });

  it("splits a Windows path on its own separator", () => {
    const short = shortenPaths(["C:\\Users\\dev\\Deck"]);
    expect(short.get("C:\\Users\\dev\\Deck")).toBe("Deck");
  });

  it("leaves a label with no segments alone", () => {
    expect(shortenPaths(["/"]).get("/")).toBe("/");
    expect(shortenPaths([""]).get("")).toBe("");
  });
});
