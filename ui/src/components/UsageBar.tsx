import { formatPercent, levelFor, levelPattern, type Level } from "../format";
import { useLocale, useStrings } from "../store";

/**
 * The plain fullness bar, one per window row.
 *
 * Level is carried by colour, by fill pattern and by the adjacent number, so it survives
 * both colour blindness and a screenshot in greyscale.
 *
 * A `meter` rather than a decorative div: it is the one element that states the reading with
 * its name, its value and its bounds together, which is why the printed percentage next to it
 * is hidden from assistive technology rather than announced twice.
 *
 * `level` and `label` are overrides for the pace row. A projection is not a reading, so its
 * bands differ (`levelForRisk`) and its spoken name has to say so — but it is still the same
 * bar, and drawing it with a second component would let the two drift apart.
 */
export function UsageBar({
  percent,
  windowName,
  level: override,
  label,
}: {
  percent: number;
  windowName: string;
  level?: Level;
  label?: string;
}) {
  const strings = useStrings();
  const locale = useLocale();
  const clamped = Math.max(0, Math.min(100, percent));
  const level = override ?? levelFor(clamped);

  return (
    <div
      className="bar"
      role="meter"
      aria-valuenow={Math.round(clamped)}
      aria-valuemin={0}
      aria-valuemax={100}
      // The raw value is a bare number; this is the form the panel prints, sign and all.
      aria-valuetext={formatPercent(clamped, locale)}
      aria-label={label ?? strings.card.limitLabel(windowName)}
    >
      <div
        className="bar__fill"
        data-level={level}
        data-pattern={levelPattern(level)}
        style={{ inlineSize: `${clamped}%` }}
      />
    </div>
  );
}
