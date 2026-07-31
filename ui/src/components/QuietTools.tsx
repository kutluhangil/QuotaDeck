import { identityHue } from "../identity";
import { useStrings } from "../store";
import type { ProviderSnapshot } from "../types";

/**
 * Tools with nothing to report, in two groups, because they are two different facts.
 *
 * A tool that is *installed* and quiet is a live reading — it says the detection worked and
 * the tool has simply not been used, which is exactly what someone glancing at the panel wants
 * to know. Those get a pill each, always visible.
 *
 * A tool that is not on this machine is not news. There are sixteen providers and most people
 * have two, so fourteen pills would bury the three cards above them. Those stay behind a
 * native `<details>` — already in the tab order, already answering the space bar.
 *
 * The pills replace what a competitor puts in the same spot: a row of service-status badges
 * fetched from a status page. This app makes no network request, ever, so it reports the one
 * thing it can actually see — what is on this disk.
 */
export function QuietTools({ snapshots }: { snapshots: ProviderSnapshot[] }) {
  const strings = useStrings();
  const present = snapshots.filter((snapshot) => snapshot.installed);
  const absent = snapshots.filter((snapshot) => !snapshot.installed);

  if (snapshots.length === 0) return null;

  return (
    <>
      {present.length > 0 && (
        <ul className="pills" aria-label={strings.a11y.tools}>
          {present.map((snapshot) => (
            <li key={snapshot.id} className="type-caption pills__pill">
              <span
                className="card__dot"
                data-hue={identityHue(snapshot.id)}
                aria-hidden="true"
              />
              <span className="pills__name">{strings.provider[snapshot.id]}</span>
              <span className="pills__reason">
                {strings.unavailable[snapshot.unavailable ?? "never-reported"]}
              </span>
            </li>
          ))}
        </ul>
      )}

      {absent.length > 0 && (
        <details className="quiet">
          <summary className="type-label quiet__summary">
            {strings.quiet.heading(absent.length)}
          </summary>
          <ul className="quiet__list">
            {absent.map((snapshot) => (
              <li key={snapshot.id} className="quiet__row">
                <span className="type-label quiet__name">{strings.provider[snapshot.id]}</span>
                <span className="type-caption quiet__reason">
                  {strings.unavailable[snapshot.unavailable ?? "never-reported"]}
                </span>
              </li>
            ))}
          </ul>
        </details>
      )}
    </>
  );
}
