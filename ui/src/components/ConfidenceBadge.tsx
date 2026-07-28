import { formatDuration } from "../format";
import { strings } from "../strings";
import type { Confidence } from "../types";

/**
 * Small, quiet, and always present. Showing an estimate as if it were a measurement is the
 * fastest way to lose a user's trust in this category, so every number states its source.
 */
export function ConfidenceBadge({ confidence }: { confidence: Confidence }) {
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
          {strings.confidence.stale(formatDuration(confidence.ageSeconds))}
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
