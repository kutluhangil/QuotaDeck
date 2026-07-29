import { formatPercent, levelForRisk } from "../format";
import { strings } from "../strings";
import type { PaceForecast } from "../types";

/**
 * Where a window is heading, as a miniature of the bar it is projecting.
 *
 * The meter is a genuine fullness indicator, so it earns the level ramp: red here means the
 * same thing it means everywhere else in the panel. The word beside it carries the same
 * reading without colour, and the meter's height carries it without either.
 */
export function PaceBadge({ pace }: { pace: PaceForecast }) {
  const level = levelForRisk(pace.risk);
  const clamped = Math.max(0, Math.min(100, pace.projectedPercent));

  return (
    <span className="pace type-caption" data-risk={pace.risk}>
      <span
        className="pace__meter"
        role="img"
        aria-label={strings.pace.label(formatPercent(pace.projectedPercent))}
      >
        <span className="pace__fill" data-level={level} style={{ blockSize: `${clamped}%` }} />
      </span>
      {strings.pace.risk[pace.risk]}
    </span>
  );
}
