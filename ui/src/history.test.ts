import { describe, expect, it } from "vitest";

import { dailyCells, inRange, intensity, localDayStart, peakTokens, totals } from "./history";
import type { HistoryPoint } from "./types";

function point(start: number, tokens: number, usd: number, unpriced = 0): HistoryPoint {
  return {
    start,
    tokens: { input: tokens, output: 0, cacheRead: 0, cacheCreation: 0, reasoning: 0 },
    cost: { usd, unpricedTokens: unpriced },
  };
}

/** 2026-08-03 12:00 UTC, a Monday. */
const NOON = 1_785_758_400;
const HOUR = 3600;
const DAY = 86_400;

describe("inRange", () => {
  it("takes a rolling window back from now, matching the panel's rolling day", () => {
    const hours = [point(NOON - 2 * DAY, 10, 1), point(NOON - HOUR, 10, 1)];
    expect(inRange(hours, "day", NOON)).toHaveLength(1);
    expect(inRange(hours, "week", NOON)).toHaveLength(2);
  });

  it("excludes anything after now", () => {
    // Clock skew between the machine writing the log and this process is real; a point in
    // the future must not be counted into a range that has not happened yet.
    expect(inRange([point(NOON + HOUR, 10, 1)], "month", NOON)).toHaveLength(0);
  });
});

describe("totals", () => {
  it("keeps unpriced tokens apart from the dollar figure", () => {
    const sum = totals([point(NOON, 100, 1.5), point(NOON - HOUR, 200, 0, 200)]);
    expect(sum.tokens).toBe(300);
    expect(sum.usd).toBe(1.5);
    expect(sum.unpricedTokens).toBe(200);
  });
});

describe("dailyCells", () => {
  it("lays out every day including the empty ones", () => {
    // The backend omits hours with no usage. A heatmap's gaps are its whole point, so the
    // calendar is built here rather than inferred from what happened to arrive.
    const cells = dailyCells([point(NOON, 500, 1)], 7, NOON);
    expect(cells).toHaveLength(7);
    expect(cells.filter((cell) => cell.tokens > 0)).toHaveLength(1);
    expect(cells[cells.length - 1]?.tokens).toBe(500);
  });

  it("puts each cell on local midnight and runs oldest first", () => {
    const cells = dailyCells([], 3, NOON);
    expect(cells[0]?.start).toBe(localDayStart(NOON - 2 * DAY));
    expect(cells[2]?.start).toBe(localDayStart(NOON));
    expect(cells[0]!.start).toBeLessThan(cells[1]!.start);
  });

  it("folds every hour of one local day into a single cell", () => {
    const hours = [point(NOON, 100, 1), point(NOON + HOUR, 200, 2), point(NOON - HOUR, 300, 3)];
    const cells = dailyCells(hours, 1, NOON + HOUR);
    expect(cells).toHaveLength(1);
    expect(cells[0]?.tokens).toBe(600);
    expect(cells[0]?.usd).toBe(6);
  });
});

describe("intensity", () => {
  it("never shades a day that carried work as empty", () => {
    // Cache reads put two orders of magnitude between a quiet day and a burst. An empty
    // square has to keep meaning that nothing happened.
    expect(intensity(1, 1_000_000)).toBe(1);
    expect(intensity(0, 1_000_000)).toBe(0);
  });

  it("climbs with the share of the busiest day", () => {
    expect(intensity(1_000, 1_000)).toBe(4);
    expect(intensity(600, 1_000)).toBe(3);
    expect(intensity(400, 1_000)).toBe(2);
    expect(intensity(100, 1_000)).toBe(1);
  });

  it("shades nothing when no day carried anything", () => {
    expect(intensity(0, 0)).toBe(0);
  });
});

describe("peakTokens", () => {
  it("is zero for a month with no usage at all", () => {
    expect(peakTokens(dailyCells([], 30, NOON))).toBe(0);
  });
});
