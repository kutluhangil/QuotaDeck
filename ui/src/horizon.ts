/**
 * The arithmetic behind the Horizon strip. Pure functions, no DOM, no React.
 *
 * `core/src/horizon.rs` folds the same buckets for the menu bar item, and the fold rule is
 * deliberately identical: sum the buckets falling in each column, scale linearly against the
 * tallest column, and floor a column that carries any usage so a gap always means nothing
 * happened. The axis differs by surface — a 44px menu bar item shows history up to now, while
 * the panel shows the whole window — so only the fold is shared, not the framing.
 */

import type { Bucket, QuotaWindow } from "./types";

export interface Column {
  /** Unix epoch seconds at the left edge of this column. */
  start: number;
  seconds: number;
  tokens: number;
  /** Drawn height, 0 to 1. */
  height: number;
}

/**
 * The time the strip spans: the reported window length, ending at now.
 *
 * `resetsAt` deliberately plays no part in the axis. Phase 0 measured two different reset
 * shapes — Claude Code resets on clean half-hour boundaries, Codex at an arbitrary instant —
 * and neither survives being read as a window boundary: the Codex sample in
 * `docs/DISCOVERY.md` reports a seven-day window resetting 6.8 days out while already at 68%,
 * which puts `resetsAt - window` only 3.6 hours before the reading. Whatever that instant
 * means, it is not the start of the counted span. The one thing true under every reading is
 * that the provider is measuring the last `windowMinutes`, so that is what the strip draws.
 */
export interface Axis {
  /** Unix epoch seconds at the left edge. */
  start: number;
  /** Unix epoch seconds at the right edge, which is always now. */
  end: number;
}

/**
 * A column carrying usage is never drawn as empty, however small it is against the tallest
 * one. Cache reads put two orders of magnitude between a quiet column and a burst, and a gap
 * in the timeline has to keep meaning "nothing happened here".
 */
export const MIN_VISIBLE_HEIGHT = 0.06;

/** Shortest axis worth drawing, for a provider that reports an unusable window length. */
const MIN_SPAN_SECONDS = 300;

export function axisFor(window: QuotaWindow, nowSeconds: number): Axis {
  const span = Math.max(window.windowMinutes * 60, MIN_SPAN_SECONDS);
  return { start: nowSeconds - span, end: nowSeconds };
}

/** Fold `series` into `count` equal slices of the axis. */
export function columns(series: Bucket[], axis: Axis, count: number): Column[] {
  if (count <= 0) return [];
  const span = Math.max(1, axis.end - axis.start);
  const totals = new Array<number>(count).fill(0);

  for (const bucket of series) {
    if (bucket.start < axis.start || bucket.start > axis.end) continue;
    const offset = bucket.start - axis.start;
    // A bucket landing exactly on the right edge belongs to the last column rather than to a
    // column that does not exist.
    const index = Math.min(count - 1, Math.floor((offset * count) / span));
    // `index` is clamped into range above; the read is only written this way because
    // noUncheckedIndexedAccess cannot see that.
    totals[index] = (totals[index] as number) + totalTokens(bucket);
  }

  const ceiling = Math.max(...totals, 0);
  return totals.map((tokens, index) => {
    const start = axis.start + Math.floor((index * span) / count);
    const end = axis.start + Math.floor(((index + 1) * span) / count);
    return { start, seconds: end - start, tokens, height: scale(tokens, ceiling) };
  });
}

/**
 * Linear against the tallest column, with a floor so quiet work stays visible.
 *
 * Clipping the tail or bending it onto a log scale would both understate a burst, and a burst
 * is the one thing on this strip the user most needs to see.
 */
function scale(tokens: number, ceiling: number): number {
  if (tokens <= 0 || ceiling <= 0) return 0;
  return Math.min(1, Math.max(MIN_VISIBLE_HEIGHT, tokens / ceiling));
}

function totalTokens(bucket: Bucket): number {
  const { input, output, cacheRead, cacheCreation } = bucket.tokens;
  return input + output + cacheRead + cacheCreation;
}

/**
 * How far back the left edge sits, in seconds.
 *
 * This is what the strip's leading label states, and it is the single most useful thing the
 * strip says: almost nobody knows their Codex limit is weekly rather than five-hourly. It
 * comes from the axis rather than from a fixed set of hours, because Phase 0 found the
 * duration to be a per-provider, per-plan fact.
 */
export function historySeconds(axis: Axis): number {
  return Math.max(0, axis.end - axis.start);
}
