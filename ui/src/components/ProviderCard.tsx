import { useId } from "react";

import {
  formatClock,
  formatCost,
  formatDuration,
  formatPercent,
  formatRelative,
  formatSpan,
  formatTokens,
  levelFor,
  levelForRisk,
  secondsUntil,
  windowLabel,
} from "../format";
import { identityHue } from "../identity";
import type { Catalogue } from "../i18n";
import { useLocale, useStrings } from "../store";
import {
  awaitingSetup,
  paceFor,
  sortedWindows,
  totalTokens,
  worstWindow,
  type Locale,
  type ProviderSnapshot,
  type QuotaWindow,
} from "../types";
import { ConfidenceBadge } from "./ConfidenceBadge";
import { HorizonStrip } from "./HorizonStrip";
import { WindowRow } from "./WindowRow";

/**
 * When the headline window lets go, as a clock time.
 *
 * The countdown that used to sit beside it has moved onto the row itself, where it is one of
 * four aligned columns instead of a second half of a sentence.
 */
function ResetLine({ window }: { window: QuotaWindow }) {
  const strings = useStrings();
  const locale = useLocale();
  const clock = formatClock(window.resetsAt, locale);
  if (clock === null) return <span>{strings.card.noReset}</span>;
  return <span>{strings.card.resetsAt(clock)}</span>;
}

/**
 * The rolling day in equivalent API cost, with the unpriced remainder named rather than
 * dropped. A model released after this build carries no price, and a dollar total that
 * silently omits it is worse than one that admits the gap.
 */
function TodayLine({ snapshot, now }: { snapshot: ProviderSnapshot; now: number }) {
  const strings = useStrings();
  const locale = useLocale();
  const today = totalTokens(snapshot.today);
  const { usd, unpricedTokens } = snapshot.todayCost;

  if (usd > 0) {
    return (
      <span>
        {strings.card.todayCost(formatCost(usd, locale))}
        {unpricedTokens > 0 && (
          <span className="card__foot-note">
            {` · ${strings.card.costPartial(formatTokens(unpricedTokens, locale))}`}
          </span>
        )}
      </span>
    );
  }
  if (today > 0) return <span>{strings.card.todayTokens(formatTokens(today, locale))}</span>;

  const last = formatRelative(snapshot.lastActivity, now, strings);
  return <span>{last ? strings.card.lastActivity(last) : strings.card.neverUsed}</span>;
}

/** The countdown a row shows on the right: how long until this window lets go. */
function resetMeta(window: QuotaWindow, now: number, strings: Catalogue): string {
  const seconds = secondsUntil(window.resetsAt, now);
  return seconds === null ? "" : formatDuration(seconds, strings);
}

export function ProviderCard({
  snapshot,
  now,
  onSetUp,
}: {
  snapshot: ProviderSnapshot;
  now: number;
  /** Opens settings, where the plan and the status line live. */
  onSetUp?: () => void;
}) {
  const strings: Catalogue = useStrings();
  const locale: Locale = useLocale();
  const nameId = useId();
  const name = strings.provider[snapshot.id];
  const headline = worstWindow(snapshot.windows);
  const rows = sortedWindows(snapshot.windows);
  const pace = headline === null ? null : paceFor(snapshot, headline);
  const percent = headline?.usedPercent ?? null;
  // Logging, but with no reading to show: a plan pick or the status line fixes this, and
  // saying so beats the flat "has not reported a limit" the tool itself cannot resolve.
  const needsSetup = awaitingSetup(snapshot);
  /*
   * The fullest window as a word. It sits in the heading row, which §7.2 keeps the ramp off —
   * but the rule is about decoration, and this is a reading: it says the same thing as the
   * worst bar below it, in the one form that survives a greyscale screenshot.
   */
  const level = percent === null ? null : levelFor(percent);
  const paceClock = pace === null ? null : formatClock(pace.exhaustedAt, locale);

  return (
    <article className="card" aria-labelledby={nameId}>
      <header className="card__head">
        <h2 className="type-label card__name" id={nameId}>
          <span className="card__dot" data-hue={identityHue(snapshot.id)} aria-hidden="true" />
          {name}
        </h2>
        <span className="card__head-right">
          {/* One badge with its word per card, so the marks on the rows below have been
              introduced before they are relied on. */}
          <ConfidenceBadge
            confidence={
              headline?.confidence ?? {
                level: "unavailable",
                reason: snapshot.unavailable ?? "never-reported",
              }
            }
          />
          {level && (
            <span className="type-caption card__status" data-level={level}>
              {strings.status[level]}
            </span>
          )}
        </span>
      </header>

      {percent !== null ? (
        <>
          <ul className="card__rows" role="list" aria-label={strings.a11y.windows(name)}>
            {rows.map((window) => (
              <WindowRow
                key={`${window.limitId}-${window.windowMinutes}`}
                label={formatSpan(window.windowMinutes * 60, strings)}
                spokenLabel={windowLabel(window, strings)}
                percent={window.usedPercent}
                confidence={window.confidence}
                meta={resetMeta(window, now, strings)}
              />
            ))}
            {pace && (
              <WindowRow
                kind="pace"
                label={strings.pace.rowLabel}
                spokenLabel={strings.pace.label(formatPercent(pace.projectedPercent, locale))}
                percent={pace.projectedPercent}
                level={levelForRisk(pace.risk)}
                meta={
                  paceClock === null
                    ? strings.pace.risk[pace.risk]
                    : `${strings.pace.risk[pace.risk]} · ${paceClock}`
                }
              />
            )}
          </ul>
          {snapshot.series.length > 0 && headline && (
            <HorizonStrip window={headline} series={snapshot.series} now={now} />
          )}
        </>
      ) : needsSetup ? (
        <p className="type-body card__quiet">
          {strings.card.pickPlan}
          {onSetUp && (
            <button type="button" className="card__setup" onClick={onSetUp}>
              {strings.card.pickPlanAction}
            </button>
          )}
        </p>
      ) : (
        <p className="type-body card__quiet">
          {strings.unavailable[snapshot.unavailable ?? "never-reported"]}
        </p>
      )}

      <footer className="type-caption card__foot">
        {headline ? <ResetLine window={headline} /> : null}
        <span className="card__foot-right">
          <TodayLine snapshot={snapshot} now={now} />
        </span>
      </footer>
    </article>
  );
}
