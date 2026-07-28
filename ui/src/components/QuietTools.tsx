import { strings } from "../strings";
import type { ProviderSnapshot } from "../types";

/**
 * Installed tools with nothing to report. Collapsed by default: they are evidence that
 * detection worked, not something to spend panel height on.
 */
export function QuietTools({ snapshots }: { snapshots: ProviderSnapshot[] }) {
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
