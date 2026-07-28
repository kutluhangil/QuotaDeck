import { formatRelative } from "../format";
import { strings } from "../strings";
import { useDeck } from "../store";

/**
 * Consent for the one thing this app writes outside its own data directory.
 *
 * The rule from Phase 0: never change someone's `settings.json` without showing them exactly
 * what changes. This renders the current command and the proposed one side by side, states
 * that the existing command keeps running, and offers a one-click revert afterwards. The
 * button is the only path to `install_statusline`; nothing installs on its own.
 */
export function StatuslineCard({ now }: { now: number }) {
  const statusline = useDeck((state) => state.statusline);
  const error = useDeck((state) => state.statuslineError);
  const install = useDeck((state) => state.installStatusline);
  const revert = useDeck((state) => state.revertStatusline);

  if (statusline === null) return null;

  if (!statusline.supported) {
    return (
      <fieldset className="settings__group">
        <legend className="type-label settings__legend">{strings.settings.statuslineTitle}</legend>
        <p className="type-caption settings__hint">{strings.settings.statuslineUnsupported}</p>
      </fieldset>
    );
  }

  const lastReading = formatRelative(statusline.lastReadingAt, now);

  return (
    <fieldset className="settings__group">
      <legend className="type-label settings__legend">{strings.settings.statuslineTitle}</legend>
      <p className="type-body settings__hint">{strings.settings.statuslineBody}</p>

      {statusline.installed ? (
        <>
          <p className="type-caption settings__hint" data-connected="true">
            {statusline.readings > 0 && lastReading
              ? strings.settings.statuslineReadings(statusline.readings, lastReading)
              : strings.settings.statuslineWaiting}
          </p>
          <button type="button" className="settings__button" onClick={() => void revert()}>
            {strings.settings.statuslineRevert}
          </button>
          {statusline.previousCommand === null && (
            <p className="type-caption settings__hint">{strings.settings.statuslineNoPrevious}</p>
          )}
        </>
      ) : (
        <>
          <dl className="settings__diff">
            <dt className="type-caption">{strings.settings.statuslineBefore}</dt>
            <dd className="type-metric settings__command">
              {statusline.currentCommand ?? "—"}
            </dd>
            <dt className="type-caption">{strings.settings.statuslineAfter}</dt>
            <dd className="type-metric settings__command">
              {statusline.proposedCommand ?? "—"}
            </dd>
          </dl>
          {statusline.currentCommand !== null && (
            <p className="type-caption settings__hint">{strings.settings.statuslineChains}</p>
          )}
          <button type="button" className="settings__button" onClick={() => void install()}>
            {strings.settings.statuslineConnect}
          </button>
        </>
      )}

      {statusline.settingsPath !== null && (
        <p className="type-caption settings__hint">
          {strings.settings.statuslineFile(statusline.settingsPath)}
        </p>
      )}
      {error !== null && (
        <p className="type-caption settings__error">{strings.settings.statuslineFailed(error)}</p>
      )}
    </fieldset>
  );
}
