import { useState } from "react";

import { formatRelative } from "../format";
import { useDeck, useStrings } from "../store";

export function StatuslineCard({ now }: { now: number }) {
  const strings = useStrings();
  const statusline = useDeck((state) => state.statusline);
  const error = useDeck((state) => state.statuslineError);
  const action = useDeck((state) => state.statuslineAction);
  const install = useDeck((state) => state.installStatusline);
  const revert = useDeck((state) => state.revertStatusline);
  const refresh = useDeck((state) => state.refreshStatusline);
  const prepareManual = useDeck((state) => state.prepareManualStatusline);
  const [copyMessage, setCopyMessage] = useState<string | null>(null);

  if (statusline === null) return null;

  async function copy(command: string | null) {
    setCopyMessage(null);
    if (command === null) return;
    try {
      if (navigator.clipboard === undefined) {
        throw new Error("clipboard access is unavailable");
      }
      await navigator.clipboard.writeText(command);
      setCopyMessage(strings.settings.statuslineCopied);
    } catch (copyError) {
      setCopyMessage(strings.settings.statuslineCopyFailed(String(copyError)));
    }
  }

  if (statusline.setupMode === "unavailable") {
    return (
      <fieldset className="settings__group">
        <legend className="type-label settings__legend">{strings.settings.statuslineTitle}</legend>
        <p className="type-caption settings__hint">{strings.settings.statuslineUnsupported}</p>
        <button
          type="button"
          className="settings__button"
          disabled={action !== null}
          onClick={() => void refresh()}
        >
          {action === "refresh"
            ? strings.settings.statuslineRefreshing
            : strings.settings.statuslineRefresh}
        </button>
        {error !== null && (
          <p className="type-caption settings__error" role="alert">
            {strings.settings.statuslineFailed(error)}
          </p>
        )}
      </fieldset>
    );
  }

  const lastReading = formatRelative(statusline.lastReadingAt, now, strings);
  const reading =
    statusline.readings > 0 && lastReading
      ? strings.settings.statuslineReadings(statusline.readings, lastReading)
      : strings.settings.statuslineWaiting;
  const busy = action !== null;
  const currentPreview =
    statusline.setupMode === "manual"
      ? statusline.currentStatusLine === null
        ? "—"
        : JSON.stringify(statusline.currentStatusLine, null, 2)
      : (statusline.currentCommand ?? "—");
  const proposedPreview =
    statusline.setupMode === "manual"
      ? statusline.proposedStatusLine === null
        ? "—"
        : JSON.stringify(statusline.proposedStatusLine, null, 2)
      : (statusline.proposedCommand ?? "—");

  return (
    <fieldset className="settings__group" aria-busy={busy}>
      <legend className="type-label settings__legend">{strings.settings.statuslineTitle}</legend>
      <p className="type-body settings__hint">{strings.settings.statuslineBody}</p>

      {statusline.setupMode === "manual" && (
        <p className="type-caption settings__hint">{strings.settings.statuslineManualNotice}</p>
      )}

      {statusline.installed ? (
        <>
          <p className="type-caption settings__hint" data-connected="true">
            {reading}
          </p>
          {statusline.setupMode === "automatic" ? (
            <button
              type="button"
              className="settings__button"
              disabled={busy}
              onClick={() => void revert()}
            >
              {action === "revert"
                ? strings.settings.statuslineReverting
                : strings.settings.statuslineRevert}
            </button>
          ) : (
            <>
              {statusline.manualRevertMode === "restore-object" &&
              statusline.previousStatusLine !== null ? (
                <>
                  <p className="type-caption settings__hint">
                    {strings.settings.statuslineManualRestoreObject}
                  </p>
                  <code className="type-metric settings__command">
                    {JSON.stringify(statusline.previousStatusLine, null, 2)}
                  </code>
                  <button
                    type="button"
                    className="settings__button"
                    disabled={busy}
                    onClick={() =>
                      void copy(JSON.stringify(statusline.previousStatusLine, null, 2))
                    }
                  >
                    {strings.settings.statuslineCopyPreviousObject}
                  </button>
                </>
              ) : statusline.manualRevertMode === "restore-command" &&
              statusline.previousCommand !== null ? (
                <>
                  <p className="type-caption settings__hint">
                    {strings.settings.statuslineManualRestore}
                  </p>
                  <code className="type-metric settings__command">
                    {statusline.previousCommand}
                  </code>
                  <button
                    type="button"
                    className="settings__button"
                    disabled={busy}
                    onClick={() => void copy(statusline.previousCommand)}
                  >
                    {strings.settings.statuslineCopyPrevious}
                  </button>
                </>
              ) : statusline.manualRevertMode === "remove-command" ? (
                <p className="type-caption settings__hint">
                  {strings.settings.statuslineManualRemoveCommand}
                </p>
              ) : (
                <p className="type-caption settings__hint">
                  {strings.settings.statuslineManualRemove}
                </p>
              )}
              <button
                type="button"
                className="settings__button"
                disabled={busy}
                onClick={() => void refresh()}
              >
                {action === "refresh"
                  ? strings.settings.statuslineRefreshing
                  : strings.settings.statuslineRefresh}
              </button>
            </>
          )}
        </>
      ) : (
        <>
          <dl className="settings__diff">
            <dt className="type-caption">{strings.settings.statuslineBefore}</dt>
            <dd className="type-metric settings__command">
              <code>{currentPreview}</code>
            </dd>
            <dt className="type-caption">{strings.settings.statuslineAfter}</dt>
            <dd className="type-metric settings__command">
              <code>{proposedPreview}</code>
            </dd>
          </dl>
          {statusline.currentCommand !== null && (
            <p className="type-caption settings__hint">{strings.settings.statuslineChains}</p>
          )}
          {statusline.setupMode === "automatic" ? (
            <button
              type="button"
              className="settings__button"
              disabled={busy}
              onClick={() => void install()}
            >
              {action === "install"
                ? strings.settings.statuslineConnecting
                : strings.settings.statuslineConnect}
            </button>
          ) : (
            <>
              <p className="type-caption settings__hint">
                {strings.settings.statuslineManualInstruction}
              </p>
              <div className="settings__button-row">
                <button
                  type="button"
                  className="settings__button"
                  disabled={busy || statusline.proposedStatusLine === null}
                  onClick={() =>
                    void (async () => {
                      if (await prepareManual()) await copy(proposedPreview);
                    })()
                  }
                >
                  {strings.settings.statuslineCopyCommand}
                </button>
                <button
                  type="button"
                  className="settings__button"
                  disabled={busy}
                  onClick={() => void refresh()}
                >
                  {action === "refresh"
                    ? strings.settings.statuslineRefreshing
                    : strings.settings.statuslineRefresh}
                </button>
              </div>
            </>
          )}
        </>
      )}

      {statusline.settingsPath !== null && (
        <p className="type-caption settings__hint">
          {strings.settings.statuslineFile(statusline.settingsPath)}
        </p>
      )}
      {copyMessage !== null && (
        <p className="type-caption settings__hint" role="status" aria-live="polite">
          {copyMessage}
        </p>
      )}
      {error !== null && (
        <p className="type-caption settings__error" role="alert">
          {strings.settings.statuslineFailed(error)}
        </p>
      )}
    </fieldset>
  );
}
