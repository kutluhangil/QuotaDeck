import { describe, expect, it } from "vitest";

import {
  axisFor,
  columns,
  historySeconds,
  MIN_VISIBLE_HEIGHT,
  type Axis,
  type Column,
} from "./horizon";
import { formatSpan } from "./format";
import type { Bucket, QuotaWindow } from "./types";

/**
 * The fold cases mirror `core/src/horizon.rs`, which draws the menu bar item from the same
 * buckets. Both sides must agree on where a bucket lands and how tall it is drawn.
 */

function bucket(start: number, input: number): Bucket {
  return {
    start,
    tokens: { input, output: 0, cacheRead: 0, cacheCreation: 0, reasoning: 0 },
    requests: 0,
  };
}

/** Ten columns over an hour, so one column is six minutes. */
const HOUR: Axis = { start: 0, end: 3_600 };

/** Indexed access that fails the test rather than quietly yielding undefined. */
function at(drawn: Column[], index: number): Column {
  const column = drawn[index];
  if (column === undefined) throw new Error(`no column at ${index}`);
  return column;
}

function window(overrides: Partial<QuotaWindow> = {}): QuotaWindow {
  return {
    limitId: "test",
    kind: "session",
    windowMinutes: 300,
    usedPercent: 40,
    resetsAt: null,
    confidence: { level: "measured", reportedAt: new Date(0).toISOString() },
    ...overrides,
  };
}

describe("columns", () => {
  it("produces the full axis even with no usage", () => {
    const drawn = columns([], HOUR, 10);
    expect(drawn).toHaveLength(10);
    expect(drawn.every((column) => column.height === 0)).toBe(true);
    expect(at(drawn, 0).start).toBe(0);
    expect(at(drawn, 9).start).toBe(3_240);
  });

  it("puts a bucket in the column holding its instant", () => {
    const drawn = columns([bucket(0, 5), bucket(300, 5), bucket(3_540, 100)], HOUR, 10);
    expect(at(drawn, 0).tokens).toBe(10);
    expect(at(drawn, 9).tokens).toBe(100);
    expect(at(drawn, 5).tokens).toBe(0);
  });

  it("draws a bucket on the right edge rather than dropping it", () => {
    expect(at(columns([bucket(3_600, 7)], HOUR, 10), 9).tokens).toBe(7);
  });

  it("drops usage that has fallen out of the window", () => {
    const drawn = columns([bucket(-300, 999)], HOUR, 10);
    expect(drawn.every((column) => column.tokens === 0)).toBe(true);
  });

  it("draws a burst full height and keeps quiet work visible beside it", () => {
    const series = [...Array(9)].map((_, i) => bucket(i * 360, 1_000));
    series.push(bucket(3_240, 500_000));

    const drawn = columns(series, HOUR, 10);
    expect(at(drawn, 9).height).toBe(1);
    expect(at(drawn, 0).height).toBe(MIN_VISIBLE_HEIGHT);
    expect(at(drawn, 9).tokens).toBe(500_000);
  });

  it("counts every non-overlapping token bucket", () => {
    const drawn = columns(
      [
        {
          start: 0,
          tokens: { input: 1, output: 2, cacheRead: 4, cacheCreation: 8, reasoning: 16 },
          requests: 0,
        },
      ],
      HOUR,
      10,
    );
    // Reasoning is already inside output for every provider measured in Phase 0; adding it
    // again would inflate the column.
    expect(at(drawn, 0).tokens).toBe(15);
  });

  it("draws nothing rather than dividing by zero when asked for no columns", () => {
    expect(columns([bucket(0, 1)], HOUR, 0)).toEqual([]);
  });
});

describe("axisFor", () => {
  it("spans the reported window backwards from now", () => {
    const axis = axisFor(window({ windowMinutes: 300 }), 10_000);
    expect(axis.end).toBe(10_000);
    expect(axis.start).toBe(10_000 - 300 * 60);
  });

  it("ignores the reported reset instant entirely", () => {
    // `resetsAt - window` is not the start of the counted span: the Codex sample in
    // docs/DISCOVERY.md reads 68% of a seven-day window whose reset is 6.8 days out. Whatever
    // that instant means, the axis must not be built on it.
    const withReset = axisFor(
      window({ windowMinutes: 10_080, resetsAt: new Date(1785594976 * 1000).toISOString() }),
      1785003192,
    );
    const withoutReset = axisFor(window({ windowMinutes: 10_080, resetsAt: null }), 1785003192);
    expect(withReset).toEqual(withoutReset);
  });

  it("takes the span from the reported duration, never a fixed five hours", () => {
    const weekly = axisFor(window({ windowMinutes: 10_080, resetsAt: null }), 0);
    expect(weekly.end - weekly.start).toBe(7 * 86_400);

    const monthly = axisFor(window({ windowMinutes: 43_200, resetsAt: null }), 0);
    expect(monthly.end - monthly.start).toBe(30 * 86_400);
  });

  it("still draws an axis for a window reported with no usable duration", () => {
    const axis = axisFor(window({ windowMinutes: 0, resetsAt: null }), 10_000);
    expect(axis.end - axis.start).toBeGreaterThan(0);
  });
});

describe("historySeconds", () => {
  it("is the span the strip draws", () => {
    expect(historySeconds({ start: 0, end: 18_000 })).toBe(18_000);
  });
});

describe("formatSpan", () => {
  it("renders a window length as the round number the provider chose", () => {
    expect(formatSpan(7 * 86_400)).toBe("7d");
    expect(formatSpan(30 * 86_400)).toBe("30d");
    expect(formatSpan(5 * 3_600)).toBe("5h");
  });

  it("falls back to the countdown form when the span is not round", () => {
    // One column of a weekly strip is 1h 58m, and rounding that to "1h" would be a lie.
    expect(formatSpan(7_080)).toBe("1h 58m");
    expect(formatSpan(90)).toBe("1m");
  });
});
