/**
 * Formatting helpers. Pure functions so they can be unit-tested without a DOM.
 *
 * Anything that produces words takes a catalogue; anything that produces a number takes a
 * locale. Neither is read from a module-level global: a formatter that quietly depends on
 * whatever language was selected last is a formatter that cannot be tested.
 */

import type { Catalogue } from "./i18n";
import type { Locale, PaceRisk, QuotaWindow, WindowKind } from "./types";

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

export function windowLabel(window: QuotaWindow, strings: Catalogue): string {
  const kind: WindowKind = window.kind;
  if (kind === "other") return strings.window.other(window.windowMinutes);
  return strings.window[kind];
}

/**
 * `Intl` formatters are expensive to construct and cheap to reuse, and the panel builds one
 * per number on every five-second tick without this.
 */
const numberFormatters = new Map<string, Intl.NumberFormat>();

function numberFormat(locale: Locale, options: Intl.NumberFormatOptions): Intl.NumberFormat {
  const key = `${locale}:${JSON.stringify(options)}`;
  const cached = numberFormatters.get(key);
  if (cached !== undefined) return cached;
  const made = new Intl.NumberFormat(intlTag(locale), options);
  numberFormatters.set(key, made);
  return made;
}

/** `undefined` hands the decision to the system, which is what `system` means. */
function intlTag(locale: Locale): string | undefined {
  return locale === "system" ? undefined : locale;
}

/** Compact duration: "6d 19h", "2h 05m", "47m". */
export function formatDuration(seconds: number, strings: Catalogue): string {
  const { day, hour, minute } = strings.units;
  const total = Math.max(0, Math.floor(seconds));
  const days = Math.floor(total / 86400);
  const hours = Math.floor((total % 86400) / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  if (days > 0) return `${days}${day} ${hours}${hour}`;
  if (hours > 0) return `${hours}${hour} ${String(minutes).padStart(2, "0")}${minute}`;
  return `${minutes}${minute}`;
}

/**
 * A window length rather than a countdown: "7d", "30d", "5h", "90m".
 *
 * Distinct from `formatDuration` on purpose. A countdown has to keep its second component —
 * "6d 19h" is the useful form when something is running out — but a window length is a round
 * number the provider chose, and rendering a weekly limit as "7d 0h" buries that.
 */
export function formatSpan(seconds: number, strings: Catalogue): string {
  const total = Math.max(0, Math.floor(seconds));
  const days = total / 86400;
  if (days >= 1 && Number.isInteger(days)) return `${days}${strings.units.day}`;
  const hours = total / 3600;
  if (hours >= 1 && Number.isInteger(hours)) return `${hours}${strings.units.hour}`;
  return formatDuration(total, strings);
}

export function secondsUntil(iso: string | null, now: number): number | null {
  if (iso === null) return null;
  const at = Date.parse(iso);
  if (Number.isNaN(at)) return null;
  const seconds = Math.floor((at - now) / 1000);
  return seconds > 0 ? seconds : null;
}

/** Clock time in the viewer's own zone, on the conventions the locale carries. */
export function formatClock(iso: string | null, locale: Locale): string | null {
  if (iso === null) return null;
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return null;
  return at.toLocaleTimeString(intlTag(locale), { hour: "2-digit", minute: "2-digit" });
}

/** Calendar date in the viewer's own zone. */
export function formatDate(at: Date, locale: Locale): string {
  return at.toLocaleDateString(intlTag(locale));
}

/** Token counts get large fast; 4 significant digits keep the column stable. */
export function formatTokens(tokens: number, locale: Locale): string {
  const scaled = numberFormat(locale, { minimumFractionDigits: 1, maximumFractionDigits: 1 });
  if (tokens >= 1_000_000_000) return `${scaled.format(tokens / 1_000_000_000)}B`;
  if (tokens >= 1_000_000) return `${scaled.format(tokens / 1_000_000)}M`;
  if (tokens >= 1_000) return `${scaled.format(tokens / 1_000)}K`;
  return numberFormat(locale, { maximumFractionDigits: 0 }).format(tokens);
}

/**
 * A percentage is shown without decimals: providers report whole numbers, and a decimal
 * would imply a precision the source does not have.
 *
 * Formatted rather than concatenated, because the sign does not sit on the same side of the
 * number in every language — Turkish writes %76.
 */
export function formatPercent(percent: number, locale: Locale): string {
  return numberFormat(locale, { style: "percent", maximumFractionDigits: 0 }).format(
    Math.round(percent) / 100,
  );
}

/**
 * A multiple of something, as in "8× a usual hour".
 *
 * Whole numbers above ten, one decimal below: the difference between 4.2× and 4.7× is worth
 * seeing at the low end, and nobody needs a tenth of a multiple at forty.
 */
export function formatFactor(factor: number, locale: Locale): string {
  const digits = factor >= 10 ? 0 : 1;
  return numberFormat(locale, {
    minimumFractionDigits: 0,
    maximumFractionDigits: digits,
  }).format(factor);
}

/**
 * Equivalent API cost. Cents matter under a dollar and are noise above ten, so the precision
 * follows the magnitude rather than being fixed.
 *
 * Always in dollars: this is the equivalent list price of the tokens, not a charge in the
 * viewer's own currency, and converting it would invent an exchange rate nobody asked for.
 */
export function formatCost(usd: number, locale: Locale): string {
  // Grouped above a thousand and bare below it, which one formatter already does.
  if (usd >= 10) return `$${numberFormat(locale, { maximumFractionDigits: 0 }).format(usd)}`;
  if (usd >= 0.01) {
    return `$${numberFormat(locale, { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(usd)}`;
  }
  return usd > 0 ? "<$0.01" : "$0";
}

export function formatRelative(
  iso: string | null,
  now: number,
  strings: Catalogue,
): string | null {
  if (iso === null) return null;
  const at = Date.parse(iso);
  if (Number.isNaN(at)) return null;
  const seconds = Math.max(0, Math.floor((now - at) / 1000));
  if (seconds < 60) return strings.relative.justNow;
  return formatDuration(seconds, strings);
}
