/**
 * Language selection.
 *
 * Two separate questions, deliberately kept apart:
 *
 * - Which *words* the panel uses. That is the catalogue, and the user picks it.
 * - Which *conventions* numbers, dates and clock times follow. That is a regional setting the
 *   user already made in the operating system, and `system` leaves it there. Overriding a
 *   Turkish user's 24-hour clock because they read the panel in English would be wrong.
 *
 * Picking a language explicitly answers both, because someone who asks for Turkish copy on an
 * English system is asking for a Turkish-looking panel.
 */

import type { Locale } from "../types";
import { en, type Catalogue } from "./en";
import { tr } from "./tr";

export type { Catalogue };

/** Languages that have a complete catalogue. `Locale` adds `system` on top of these. */
export const LANGUAGES = ["en", "tr"] as const;
export type Language = (typeof LANGUAGES)[number];

const catalogues: Record<Language, Catalogue> = { en, tr };

function isLanguage(tag: string): tag is Language {
  return (LANGUAGES as readonly string[]).includes(tag);
}

/**
 * The first language the browser reports that we actually have a catalogue for.
 *
 * Matched on the primary subtag, so `tr-TR` and `tr` both land on Turkish. Anything else falls
 * to English rather than to a half-empty catalogue.
 */
export function systemLanguage(): Language {
  if (typeof navigator === "undefined") return "en";
  const tags = navigator.languages ?? [navigator.language];
  for (const tag of tags) {
    const primary = tag.split("-")[0]?.toLowerCase() ?? "";
    if (isLanguage(primary)) return primary;
  }
  return "en";
}

export function languageFor(locale: Locale): Language {
  return locale === "system" ? systemLanguage() : locale;
}

export function catalogueFor(locale: Locale): Catalogue {
  return catalogues[languageFor(locale)];
}

/**
 * What to hand `Intl`. `undefined` means "whatever the system is set to", which is exactly
 * what `system` promises; an explicit pick names its own tag.
 */
export function intlLocale(locale: Locale): string | undefined {
  return locale === "system" ? undefined : locale;
}
