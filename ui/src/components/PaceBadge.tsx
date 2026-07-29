import { formatPercent, levelForRisk, levelPattern } from "../format";
import { useLocale, useStrings } from "../store";
import type { PaceForecast } from "../types";

/**
 * Where a window is heading, as a miniature of the bar it is projecting.
 *
 * The meter is a genuine fullness indicator, so it earns the level ramp: red here means the
 * same thing it means everywhere else in the panel. The word beside it carries the same
 * reading without colour, the fill pattern carries it without hue, and the meter's height
 * carries it without any of the three.
 */
export function PaceBadge({ pace }: { pace: PaceForecast }) {
  const strings = useStrings();
  const locale = useLocale();
  const level = levelForRisk(pace.risk);
  const clamped = Math.max(0, Math.min(100, pace.projectedPercent));

  return (
    <span className="pace type-caption" data-risk={pace.risk}>
      <span
        className="pace__meter"
        role="img"
        aria-label={strings.pace.label(formatPercent(pace.projectedPercent, locale))}
      >
        <span
          className="pace__fill"
          data-level={level}
          data-pattern={levelPattern(level)}
          style={{ blockSize: `${clamped}%` }}
        />
      </span>
      {strings.pace.risk[pace.risk]}
    </span>
  );
}
