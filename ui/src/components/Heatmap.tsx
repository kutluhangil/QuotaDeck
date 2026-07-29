import { useMemo } from "react";

import { formatDate, formatTokens } from "../format";
import { intensity, peakTokens, type DayCell } from "../history";
import { useLocale, useStrings } from "../store";
import type { Locale } from "../types";

/**
 * A month of activity, one square per local day.
 *
 * Shaded on a neutral ink ramp, never on the level ramp. The level colours mean one thing in
 * this app — a quota running out — and a busy Tuesday is not that. Volume and fullness are
 * different questions and must not borrow each other's colour.
 *
 * Laid out as a calendar rather than as GitHub's week-columns: at five weeks the column form
 * is a tall narrow strip, where seven columns of weekdays reads as the month it is.
 */

/**
 * Weekday initials, Monday first.
 *
 * Built from real dates rather than from a hardcoded list: the letters differ by language, and
 * the catalogue holds copy, never a calendar. `Intl` already knows every one of them.
 */
function weekdayInitials(locale: Locale): string[] {
  const tag = locale === "system" ? undefined : locale;
  return Array.from({ length: 7 }, (_, index) =>
    // 2026-08-03 was a Monday.
    new Date(Date.UTC(2026, 7, 3 + index)).toLocaleDateString(tag, {
      weekday: "narrow",
      timeZone: "UTC",
    }),
  );
}

export function Heatmap({ cells }: { cells: DayCell[] }) {
  const strings = useStrings();
  const locale = useLocale();
  const weekdays = useMemo(() => weekdayInitials(locale), [locale]);
  const peak = peakTokens(cells);
  // Weekday of the first cell, so a column is always the same day of the week.
  const lead = (new Date((cells[0]?.start ?? 0) * 1000).getDay() + 6) % 7;

  return (
    <div className="heatmap">
      <div className="heatmap__days type-caption" aria-hidden="true">
        {weekdays.map((letter, index) => (
          <span key={index}>{letter}</span>
        ))}
      </div>
      {/* One image with one name. Thirty-one separately announced squares would be a month of
          noise to step through, and the shape is the point — not the individual day. */}
      <div className="heatmap__grid" role="img" aria-label={strings.dashboard.heatmapLabel}>
        {Array.from({ length: lead }, (_, index) => (
          <span key={`lead-${index}`} className="heatmap__pad" aria-hidden="true" />
        ))}
        {cells.map((cell) => (
          <span
            key={cell.start}
            className="heatmap__cell"
            data-heat={intensity(cell.tokens, peak)}
            title={`${formatDate(new Date(cell.start * 1000), locale)} · ${
              cell.tokens > 0
                ? strings.strip.tokens(formatTokens(cell.tokens, locale))
                : strings.strip.quiet
            }`}
          />
        ))}
      </div>
      {/* The ramp is neutral, so the two ends have to be named: without the words a reader
          who cannot separate the middle steps has nothing to anchor the scale to. */}
      <p className="type-caption heatmap__legend">
        <span>{strings.dashboard.heatmapQuiet}</span>
        {[0, 1, 2, 3, 4].map((step) => (
          <span
            key={step}
            className="heatmap__cell heatmap__cell--key"
            data-heat={step}
            aria-hidden="true"
          />
        ))}
        <span>{strings.dashboard.heatmapBusy}</span>
      </p>
    </div>
  );
}
