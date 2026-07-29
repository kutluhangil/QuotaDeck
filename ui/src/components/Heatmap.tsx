import { formatTokens } from "../format";
import { intensity, peakTokens, type DayCell } from "../history";
import { strings } from "../strings";

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
 * Weekday initials in the viewer's own locale, Monday first.
 *
 * Built from real dates rather than from a hardcoded list: the letters differ by language,
 * and Phase 8 turns this file's copy into a catalogue but never its calendar.
 */
const weekdays = Array.from({ length: 7 }, (_, index) =>
  // 2026-08-03 was a Monday.
  new Date(Date.UTC(2026, 7, 3 + index)).toLocaleDateString(undefined, {
    weekday: "narrow",
    timeZone: "UTC",
  }),
);

export function Heatmap({ cells }: { cells: DayCell[] }) {
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
      <div className="heatmap__grid" role="img" aria-label={strings.dashboard.heatmapLabel}>
        {Array.from({ length: lead }, (_, index) => (
          <span key={`lead-${index}`} className="heatmap__pad" aria-hidden="true" />
        ))}
        {cells.map((cell) => (
          <span
            key={cell.start}
            className="heatmap__cell"
            data-heat={intensity(cell.tokens, peak)}
            title={`${new Date(cell.start * 1000).toLocaleDateString()} · ${
              cell.tokens > 0 ? strings.strip.tokens(formatTokens(cell.tokens)) : strings.strip.quiet
            }`}
          />
        ))}
      </div>
      <p className="type-caption heatmap__legend">
        <span>{strings.dashboard.heatmapQuiet}</span>
        {[0, 1, 2, 3, 4].map((step) => (
          <span key={step} className="heatmap__cell heatmap__cell--key" data-heat={step} />
        ))}
        <span>{strings.dashboard.heatmapBusy}</span>
      </p>
    </div>
  );
}
