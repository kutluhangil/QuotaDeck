/**
 * Formatting helpers. Pure functions so they can be unit-tested without a DOM.
 */

import { strings } from "./strings";
import type { PaceRisk, QuotaWindow, WindowKind } from "./types";

/** Level ramp thresholds, matching the `--level-*` tokens. */
export type Level = "ample" | "tight" | "critical";

export function levelFor(percent: number): Level {
  if (percent > 85) return "critical";
  if (percent >= 60) return "tight";
  return "ample";
}

/**
 * Colour alone cannot carry the level: about 8% of men cannot separate the green from the
 * amber. Every level also gets a pattern and a word.
 */
export function levelPattern(level: Level): string {
  switch (level) {
    case "ample":
      return "solid";
    case "tight":
      return "hatched";
    case "critical":
      return "dense";
  }
}

/**
 * The level a projected fullness sits at.
 *
 * Kept apart from `levelFor` because the thresholds differ and should: a window at 88% is
 * critical *now*, while a projection landing at 88% by Sunday is still healthy. The pace bands
 * come from the blueprint (§6.2) and the ramp is reused so a colour means the same thing in
 * both places.
 */
export function levelForRisk(risk: PaceRisk): Level {
  switch (risk) {
    case "healthy":
      return "ample";
    case "at-risk":
      return "tight";
    case "over":
      return "critical";
  }
}

export function windowLabel(window: QuotaWindow): string {
  const kind: WindowKind = window.kind;
  if (kind === "other") return strings.window.other(window.windowMinutes);
  return strings.window[kind];
}

/** Compact duration: "6d 19h", "2h 05m", "47m". */
export function formatDuration(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const days = Math.floor(total / 86400);
  const hours = Math.floor((total % 86400) / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  if (days > 0) return `${days}d ${hours}h`;
  if (hours > 0) return `${hours}h ${String(minutes).padStart(2, "0")}m`;
  return `${minutes}m`;
}

/**
 * A window length rather than a countdown: "7d", "30d", "5h", "90m".
 *
 * Distinct from `formatDuration` on purpose. A countdown has to keep its second component —
 * "6d 19h" is the useful form when something is running out — but a window length is a round
 * number the provider chose, and rendering a weekly limit as "7d 0h" buries that.
 */
export function formatSpan(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const days = total / 86400;
  if (days >= 1 && Number.isInteger(days)) return `${days}d`;
  const hours = total / 3600;
  if (hours >= 1 && Number.isInteger(hours)) return `${hours}h`;
  return formatDuration(total);
}

export function secondsUntil(iso: string | null, now: number): number | null {
  if (iso === null) return null;
  const at = Date.parse(iso);
  if (Number.isNaN(at)) return null;
  const seconds = Math.floor((at - now) / 1000);
  return seconds > 0 ? seconds : null;
}

/** Clock time in the viewer's own locale and zone. */
export function formatClock(iso: string | null): string | null {
  if (iso === null) return null;
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return null;
  return at.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
}

/** Token counts get large fast; 4 significant digits keep the column stable. */
export function formatTokens(tokens: number): string {
  if (tokens >= 1_000_000_000) return `${(tokens / 1_000_000_000).toFixed(1)}B`;
  if (tokens >= 1_000_000) return `${(tokens / 1_000_000).toFixed(1)}M`;
  if (tokens >= 1_000) return `${(tokens / 1_000).toFixed(1)}K`;
  return String(tokens);
}

/**
 * A percentage is shown without decimals: providers report whole numbers, and a decimal
 * would imply a precision the source does not have.
 */
export function formatPercent(percent: number): string {
  return `${Math.round(percent)}%`;
}

/**
 * Equivalent API cost. Cents matter under a dollar and are noise above ten, so the precision
 * follows the magnitude rather than being fixed.
 */
export function formatCost(usd: number): string {
  if (usd >= 1_000) return `$${Math.round(usd).toLocaleString()}`;
  if (usd >= 10) return `$${usd.toFixed(0)}`;
  if (usd >= 0.01) return `$${usd.toFixed(2)}`;
  return usd > 0 ? "<$0.01" : "$0";
}

export function formatRelative(iso: string | null, now: number): string | null {
  if (iso === null) return null;
  const at = Date.parse(iso);
  if (Number.isNaN(at)) return null;
  const seconds = Math.max(0, Math.floor((now - at) / 1000));
  if (seconds < 60) return "just now";
  return formatDuration(seconds);
}
