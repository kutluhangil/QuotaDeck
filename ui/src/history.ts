/**
 * Folding hourly history into what the dashboard draws. Pure functions, no DOM, no React.
 *
 * The backend sends hours in UTC and omits the empty ones. Two different calendars are built
 * from that here, on purpose:
 *
 * - A **range** is rolling — the last 24 hours, 7 days or 30 days — matching the panel's
 *   rolling day and the sliding-window model the whole app teaches.
 * - A **heatmap cell** is a local calendar day, because that is what a heatmap means. Local,
 *   not UTC: an evening session must land on the square the user worked it.
 */

import type { HistoryPoint, ProviderHistory } from "./types";

export type Range = "day" | "week" | "month";

/** How far back each range reaches. `month` is thirty days, not a calendar month. */
export const RANGE_DAYS: Record<Range, number> = { day: 1, week: 7, month: 30 };

/** Days the heatmap lays out, bounded by what the engine retains. */
export const HEATMAP_DAYS = 30;

const SECONDS_PER_DAY = 86_400;

export interface Totals {
  tokens: number;
  usd: number;
  /** Tokens whose model carried no price. Kept apart so a dollar figure can admit its gap. */
  unpricedTokens: number;
}

/** One local calendar day of the heatmap. */
export interface DayCell {
  /** Unix epoch seconds at local midnight. */
  start: number;
  tokens: number;
  usd: number;
}

/**
 * Local midnight for the day containing `seconds`.
 *
 * `setHours(0, 0, 0, 0)` rather than arithmetic on the epoch: a day is not always 86 400
 * seconds long, and the two clock changes a year would shift a column of the heatmap.
 */
export function localDayStart(seconds: number): number {
  const at = new Date(seconds * 1000);
  at.setHours(0, 0, 0, 0);
  return Math.floor(at.getTime() / 1000);
}

/** Points falling inside the rolling range ending at `nowSeconds`. */
export function inRange(hours: HistoryPoint[], range: Range, nowSeconds: number): HistoryPoint[] {
  const from = nowSeconds - RANGE_DAYS[range] * SECONDS_PER_DAY;
  return hours.filter((point) => point.start >= from && point.start <= nowSeconds);
}

export function totals(points: HistoryPoint[]): Totals {
  return points.reduce<Totals>(
    (sum, point) => ({
      tokens:
        sum.tokens +
        point.tokens.input +
        point.tokens.output +
        point.tokens.cacheRead +
        point.tokens.cacheCreation,
      usd: sum.usd + point.cost.usd,
      unpricedTokens: sum.unpricedTokens + point.cost.unpricedTokens,
    }),
    { tokens: 0, usd: 0, unpricedTokens: 0 },
  );
}

/**
 * One cell per local day over the last `days`, oldest first, including the empty ones.
 *
 * The backend omits hours with no usage; a calendar cannot. The gaps are the point of a
 * heatmap, so they are laid out here rather than inferred from what happens to have arrived.
 */
export function dailyCells(
  hours: HistoryPoint[],
  days: number,
  nowSeconds: number,
): DayCell[] {
  const byDay = new Map<number, DayCell>();
  for (const point of hours) {
    const day = localDayStart(point.start);
    const cell = byDay.get(day) ?? { start: day, tokens: 0, usd: 0 };
    const { input, output, cacheRead, cacheCreation } = point.tokens;
    cell.tokens += input + output + cacheRead + cacheCreation;
    cell.usd += point.cost.usd;
    byDay.set(day, cell);
  }

  const cursor = new Date(nowSeconds * 1000);
  cursor.setHours(0, 0, 0, 0);
  cursor.setDate(cursor.getDate() - (days - 1));

  const cells: DayCell[] = [];
  for (let index = 0; index < days; index += 1) {
    const start = Math.floor(cursor.getTime() / 1000);
    cells.push(byDay.get(start) ?? { start, tokens: 0, usd: 0 });
    cursor.setDate(cursor.getDate() + 1);
  }
  return cells;
}

/**
 * Shade for one cell, 0 to 4, against the busiest day on show.
 *
 * A day carrying any work is never shaded as empty, however small it is against a burst. Cache
 * reads put two orders of magnitude between a quiet day and a heavy one, and an empty square
 * has to keep meaning that nothing happened — the same rule the Horizon strip follows.
 */
export function intensity(tokens: number, peak: number): number {
  if (tokens <= 0 || peak <= 0) return 0;
  const share = tokens / peak;
  if (share > 0.75) return 4;
  if (share > 0.5) return 3;
  if (share > 0.25) return 2;
  return 1;
}

/** The busiest day on show, which every other cell is shaded against. */
export function peakTokens(cells: DayCell[]): number {
  return cells.reduce((peak, cell) => Math.max(peak, cell.tokens), 0);
}

/** History for one provider, or an empty record when the backend sent none. */
export function historyFor(history: ProviderHistory[], id: string): HistoryPoint[] {
  return history.find((entry) => entry.id === id)?.hours ?? [];
}
