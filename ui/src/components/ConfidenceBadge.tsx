import { formatDuration } from "../format";
import { useStrings } from "../store";
import type { Catalogue } from "../i18n";
import type { Confidence } from "../types";

type Variant = "measured" | "derived" | "stale" | "idle";

/** The mark's shape and the word beside it say the same thing, so either can stand alone. */
function read(confidence: Confidence, strings: Catalogue): { variant: Variant; text: string } {
  switch (confidence.level) {
    case "measured":
      return { variant: "measured", text: strings.confidence.measured };
    case "derived":
      return { variant: "derived", text: strings.confidence.estimated };
    case "stale":
      return {
        variant: "stale",
        text: strings.relative.ago(formatDuration(confidence.ageSeconds, strings)),
      };
    case "unavailable":
      return { variant: "idle", text: strings.unavailable[confidence.reason] };
  }
}

/**
 * Small, quiet, and always present. Showing an estimate as if it were a measurement is the
 * fastest way to lose a user's trust in this category, so every number states its source.
 *
 * The mark differs in shape as well as in colour — filled, hollow, half, outlined — so the
 * four states survive colour blindness, and the word beside it carries the same reading with
 * no mark at all.
 *
 * `compact` drops the word and keeps the mark. It is for the window rows, where four columns
 * already share 380px and a fifth would push the percentage off the grid; the card's own
 * footer still carries one badge with its word, so the vocabulary is taught somewhere on
 * every card rather than left to a shape nobody has been introduced to.
 */
export function ConfidenceBadge({
  confidence,
  compact = false,
}: {
  confidence: Confidence;
  compact?: boolean;
}) {
  const strings = useStrings();
  const { variant, text } = read(confidence, strings);

  if (compact) {
    return (
      <span
        className={`badge badge--${variant} badge--compact`}
        role="img"
        aria-label={strings.a11y.source(text)}
      >
        <span className="badge__mark" aria-hidden="true" />
      </span>
    );
  }

  return (
    <span className={`badge badge--${variant} type-caption`}>
      <span className="badge__mark" aria-hidden="true" />
      {text}
    </span>
  );
}
