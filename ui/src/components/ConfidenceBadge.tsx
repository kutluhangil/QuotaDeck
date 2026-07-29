import { formatDuration } from "../format";
import { useStrings } from "../store";
import type { Confidence } from "../types";

/**
 * Small, quiet, and always present. Showing an estimate as if it were a measurement is the
 * fastest way to lose a user's trust in this category, so every number states its source.
 *
 * The mark differs in shape as well as in colour — filled, hollow, half, outlined — so the
 * four states survive colour blindness, and the word beside it carries the same reading with
 * no mark at all.
 */
export function ConfidenceBadge({ confidence }: { confidence: Confidence }) {
  const strings = useStrings();

  switch (confidence.level) {
    case "measured":
      return (
        <span className="badge badge--measured type-caption">
          <span className="badge__mark" aria-hidden="true" />
          {strings.confidence.measured}
        </span>
      );
    case "derived":
      return (
        <span className="badge badge--derived type-caption">
          <span className="badge__mark" aria-hidden="true" />
          {strings.confidence.estimated}
        </span>
      );
    case "stale":
      return (
        <span className="badge badge--stale type-caption">
          <span className="badge__mark" aria-hidden="true" />
          {strings.relative.ago(formatDuration(confidence.ageSeconds, strings))}
        </span>
      );
    case "unavailable":
      return (
        <span className="badge badge--idle type-caption">
          <span className="badge__mark" aria-hidden="true" />
          {strings.unavailable[confidence.reason]}
        </span>
      );
  }
}
