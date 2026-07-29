import { useStrings } from "../store";
import type { ProviderSnapshot } from "../types";

/**
 * Installed tools with nothing to report. Collapsed by default: they are evidence that
 * detection worked, not something to spend panel height on.
 *
 * A native `<details>`, so the disclosure is already in the tab order and already answers the
 * space bar and the Enter key without a line of script.
 */
export function QuietTools({ snapshots }: { snapshots: ProviderSnapshot[] }) {
  const strings = useStrings();
  if (snapshots.length === 0) return null;

  return (
    <details className="quiet">
      <summary className="type-label quiet__summary">
        {strings.quiet.heading(snapshots.length)}
      </summary>
      <ul className="quiet__list">
        {snapshots.map((snapshot) => (
          <li key={snapshot.id} className="quiet__row">
            <span className="type-label quiet__name">{strings.provider[snapshot.id]}</span>
            <span className="type-caption quiet__reason">
              {strings.unavailable[snapshot.unavailable ?? "never-reported"]}
            </span>
          </li>
        ))}
      </ul>
    </details>
  );
}
