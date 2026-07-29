import { formatPercent, levelFor, levelPattern } from "../format";
import { useLocale, useStrings } from "../store";

/**
 * The plain fullness bar. Phase 4 replaces the headline instance with the Horizon strip;
 * this stays for the secondary windows, where a timeline would be noise.
 *
 * Level is carried by colour, by fill pattern and by the adjacent number, so it survives
 * both colour blindness and a screenshot in greyscale.
 *
 * A `meter` rather than a decorative div: it is the one element that states the reading with
 * its name, its value and its bounds together, which is why the printed percentage next to it
 * is hidden from assistive technology rather than announced twice.
 */
export function UsageBar({ percent, windowName }: { percent: number; windowName: string }) {
  const strings = useStrings();
  const locale = useLocale();
  const clamped = Math.max(0, Math.min(100, percent));
  const level = levelFor(clamped);

  return (
    <div
      className="bar"
      role="meter"
      aria-valuenow={Math.round(clamped)}
      aria-valuemin={0}
      aria-valuemax={100}
      // The raw value is a bare number; this is the form the panel prints, sign and all.
      aria-valuetext={formatPercent(clamped, locale)}
      aria-label={strings.card.limitLabel(windowName)}
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
