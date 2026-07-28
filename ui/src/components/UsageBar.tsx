import { formatPercent, levelFor, levelPattern } from "../format";

/**
 * The plain fullness bar. Phase 4 replaces the headline instance with the Horizon strip;
 * this stays for the secondary windows, where a timeline would be noise.
 *
 * Level is carried by colour, by fill pattern and by the adjacent number, so it survives
 * both colour blindness and a screenshot in greyscale.
 */
export function UsageBar({ percent, label }: { percent: number; label: string }) {
  const clamped = Math.max(0, Math.min(100, percent));
  const level = levelFor(clamped);

  return (
    <div
      className="bar"
      role="meter"
      aria-valuenow={Math.round(clamped)}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-label={`${label} ${formatPercent(clamped)}`}
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
