/**
 * Folding an hourly breakdown into the rows the dashboard draws. Pure functions, no DOM, no
 * React — the same discipline `history.ts` follows, and held to the same tests.
 *
 * The backend sends one point per (hour, label) and never invents a label. Two rules survive
 * the fold:
 *
 * - **Share is not fullness.** A row's `share` is its part of what was spent, which is why the
 *   list is drawn on the neutral ink ramp rather than the level ramp. The same argument put the
 *   heatmap on a neutral ramp: a colour in this app means exactly one thing.
 * - **An unpriced model is not a free one.** Cost and unpriced tokens are carried separately all
 *   the way to the row, so a list that cannot price everything says so instead of ranking the
 *   unpriced model last at zero dollars.
 */

import type { BreakdownPoint, ProviderHistory } from "./types";
import { RANGE_DAYS, type Range } from "./history";

const SECONDS_PER_DAY = 86_400;

export interface BreakdownRow {
  /** `null` is "the provider reported no label", not "unknown" and not a name. */
  label: string | null;
  tokens: number;
  costUsd: number;
  unpricedTokens: number;
  /** This row's part of the range, 0 to 1. */
  share: number;
}

function countedTokens(point: BreakdownPoint): number {
  const { input, output, cacheRead, cacheCreation } = point.tokens;
  return input + output + cacheRead + cacheCreation;
}

/**
 * Rows for the rolling range ending at `nowSeconds`, heaviest first.
 *
 * Share is computed over cost when every row in the range carried a price, and over tokens
 * otherwise. Mixing the two would give an unpriced model a share of zero and sort it last,
 * which is exactly the row a user needs to see.
 */
export function foldBreakdown(
  points: BreakdownPoint[],
  range: Range,
  nowSeconds: number,
): BreakdownRow[] {
  const from = nowSeconds - RANGE_DAYS[range] * SECONDS_PER_DAY;

  const byLabel = new Map<string | null, BreakdownRow>();
  for (const point of points) {
    if (point.start < from || point.start > nowSeconds) continue;
    const row = byLabel.get(point.label) ?? {
      label: point.label,
      tokens: 0,
      costUsd: 0,
      unpricedTokens: 0,
      share: 0,
    };
    row.tokens += countedTokens(point);
    row.costUsd += point.cost.usd;
    row.unpricedTokens += point.cost.unpricedTokens;
    byLabel.set(point.label, row);
  }

  const rows = [...byLabel.values()];
  const everythingPriced = rows.every((row) => row.unpricedTokens === 0);
  const basis = (row: BreakdownRow) => (everythingPriced ? row.costUsd : row.tokens);
  const total = rows.reduce((sum, row) => sum + basis(row), 0);

  for (const row of rows) {
    row.share = total > 0 ? basis(row) / total : 0;
  }

  // Heaviest first on the same basis the share used, so the bar lengths and the order agree.
  // Tokens break a tie: two models at the same cost are not the same amount of work.
  rows.sort((a, b) => basis(b) - basis(a) || b.tokens - a.tokens);
  return rows;
}

/** One provider's model points, or an empty array when the backend sent none. */
export function modelsFor(history: ProviderHistory[], id: string): BreakdownPoint[] {
  return history.find((entry) => entry.id === id)?.models ?? [];
}

/** How many records the backend refused for carrying a model past its label cap. */
export function modelsDroppedFor(history: ProviderHistory[], id: string): number {
  return history.find((entry) => entry.id === id)?.modelsDropped ?? 0;
}
