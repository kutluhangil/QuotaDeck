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
import type { HistoryRange } from "./history";

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
 * Rows for one concrete half-open range, heaviest first.
 *
 * Share is computed over cost when every row in the range carried a price, and over tokens
 * otherwise. Mixing the two would give an unpriced model a share of zero and sort it last,
 * which is exactly the row a user needs to see.
 */
export function foldBreakdown(
  points: BreakdownPoint[],
  range: HistoryRange,
): BreakdownRow[] {
  const byLabel = new Map<string | null, BreakdownRow>();
  for (const point of points) {
    if (point.start < range.from || point.start >= range.to) continue;
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

/** One provider's project points, or an empty array when the backend sent none. */
export function projectsFor(history: ProviderHistory[], id: string): BreakdownPoint[] {
  return history.find((entry) => entry.id === id)?.projects ?? [];
}

/** How many records the backend refused for carrying a directory past its label cap. */
export function projectsDroppedFor(history: ProviderHistory[], id: string): number {
  return history.find((entry) => entry.id === id)?.projectsDropped ?? 0;
}

/** One provider's agent points, or an empty array when the backend sent none. */
export function agentsFor(history: ProviderHistory[], id: string): BreakdownPoint[] {
  return history.find((entry) => entry.id === id)?.agents ?? [];
}

/**
 * How many records the backend refused for carrying an origin past its label cap.
 *
 * Three fixed keys cannot overflow today. It is read from the payload anyway rather than
 * assumed to be zero: a build that adds a fourth thread of work must not start dropping
 * records silently because this surface decided in advance that it could not happen.
 */
export function agentsDroppedFor(history: ProviderHistory[], id: string): number {
  return history.find((entry) => entry.id === id)?.agentsDropped ?? 0;
}

/**
 * The shortest trailing path segments that still tell these directories apart.
 *
 * A project label is the absolute working directory the tool recorded, which is the only
 * unambiguous form and far too long for a row in a card. Printing the last segment alone would
 * collapse `…/Archives` and `…/Archives/app` into one visible name, so each label is shortened
 * only as far as it stays unique among the labels being drawn beside it. The full path is still
 * carried to the row, which shows it on hover.
 *
 * Separator-agnostic: a Windows path splits on `\` the same way a POSIX one splits on `/`.
 */
export function shortenPaths(labels: string[]): Map<string, string> {
  const segments = new Map<string, string[]>();
  for (const label of labels) {
    segments.set(
      label,
      label.split(/[\\/]+/).filter((part) => part.length > 0),
    );
  }

  const shortened = new Map<string, string>();
  for (const label of labels) {
    const parts = segments.get(label) ?? [];
    if (parts.length === 0) {
      // A label with nothing to shorten — the filesystem root, or an empty string.
      shortened.set(label, label);
      continue;
    }

    let depth = 1;
    while (depth < parts.length) {
      const candidate = parts.slice(-depth).join("/");
      const collides = labels.some((other) => {
        if (other === label) return false;
        const otherParts = segments.get(other) ?? [];
        return otherParts.slice(-depth).join("/") === candidate;
      });
      if (!collides) break;
      depth += 1;
    }
    shortened.set(label, parts.slice(-depth).join("/"));
  }
  return shortened;
}
