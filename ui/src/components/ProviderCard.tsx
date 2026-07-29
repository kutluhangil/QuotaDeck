import { useId } from "react";

import {
  formatClock,
  formatCost,
  formatDuration,
  formatPercent,
  formatRelative,
  formatTokens,
  levelFor,
  secondsUntil,
  windowLabel,
} from "../format";
import type { Catalogue } from "../i18n";
import { useLocale, useStrings } from "../store";
import {
  awaitingSetup,
  paceFor,
  totalTokens,
  type Locale,
  type PaceForecast,
  type ProviderSnapshot,
  type QuotaWindow,
} from "../types";
import { ConfidenceBadge } from "./ConfidenceBadge";
import { HorizonStrip } from "./HorizonStrip";
import { PaceBadge } from "./PaceBadge";
import { UsageBar } from "./UsageBar";

/**
 * The window the user needs to worry about first: the one closest to full. A provider can
 * report several independent limits, so no fixed slot ordering is assumed.
 */
function headlineWindow(windows: QuotaWindow[]): QuotaWindow | null {
  const measurable = windows.filter((window) => window.usedPercent !== null);
  if (measurable.length === 0) return windows[0] ?? null;
  return measurable.reduce((worst, window) =>
    (window.usedPercent ?? 0) > (worst.usedPercent ?? 0) ? window : worst,
  );
}

function ResetLine({ window, now }: { window: QuotaWindow; now: number }) {
  const strings = useStrings();
  const locale = useLocale();
  const clock = formatClock(window.resetsAt, locale);
  if (clock === null) return <span>{strings.card.noReset}</span>;

  const at = Date.parse(window.resetsAt ?? "");
  const seconds = Number.isNaN(at) ? null : Math.floor((at - now) / 1000);
  if (seconds === null || seconds <= 0) return <span>{strings.card.resetsAt(clock)}</span>;

  return <span>{`${strings.card.resetsAt(clock)} · ${formatDuration(seconds, strings)}`}</span>;
}

/**
 * Where the headline window is heading.
 *
 * An exhaustion instant is named when there is one, because "full at 17:42" is the thing a
 * user can act on; otherwise the projected level stands on its own. Neither is ever stated as
 * a reading — the copy says "at this pace" and the badge is a projection of a bar, not a bar.
 */
function PaceLine({ pace, now }: { pace: PaceForecast; now: number }) {
  const strings = useStrings();
  const locale = useLocale();
  const clock = formatClock(pace.exhaustedAt, locale);
  const seconds = secondsUntil(pace.exhaustedAt, now);

  return (
    <p className="card__pace type-caption">
      <PaceBadge pace={pace} />
      <span className="card__pace-text">
        {clock !== null && seconds !== null
          ? strings.pace.exhausted(clock, formatDuration(seconds, strings))
          : strings.pace.projected(formatPercent(pace.projectedPercent, locale))}
      </span>
    </p>
  );
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
  const headline = headlineWindow(snapshot.windows);
  const others = snapshot.windows.filter((window) => window !== headline);
  const pace = headline === null ? null : paceFor(snapshot, headline);
  const percent = headline?.usedPercent ?? null;
  // Logging, but with no reading to show: a plan pick or the status line fixes this, and
  // saying so beats the flat "has not reported a limit" the tool itself cannot resolve.
  const needsSetup = awaitingSetup(snapshot);
  // The level ramp reaches the headline number only once the quota is actually at risk.
  // Below that the number stays neutral, so red in this panel means exactly one thing.
  const critical = percent !== null && levelFor(percent) === "critical";

  return (
    <article className="card" aria-labelledby={nameId}>
      <header className="card__head">
        <h2 className="type-label card__name" id={nameId}>
          {strings.provider[snapshot.id]}
        </h2>
        {headline ? (
          <ConfidenceBadge confidence={headline.confidence} />
        ) : (
          <ConfidenceBadge
            confidence={{
              level: "unavailable",
              reason: snapshot.unavailable ?? "never-reported",
            }}
          />
        )}
      </header>

      {headline && percent !== null ? (
        <>
          {/* Hidden from assistive technology, not from anyone else: the meter below states
              the same window and the same percentage, with the bounds this pair cannot. */}
          <div className="card__reading" aria-hidden="true">
            <span className="type-display card__percent" data-critical={critical}>
              {formatPercent(percent, locale)}
            </span>
            <span className="type-caption card__window">{windowLabel(headline, strings)}</span>
          </div>
          <UsageBar percent={percent} windowName={windowLabel(headline, strings)} />
          {snapshot.series.length > 0 && (
            <HorizonStrip window={headline} series={snapshot.series} now={now} />
          )}
          {pace && <PaceLine pace={pace} now={now} />}
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

      {others.length > 0 && (
        <ul className="card__others">
          {others.map((window) => {
            const otherPace = paceFor(snapshot, window);
            return (
              <li key={`${window.limitId}-${window.windowMinutes}`} className="card__other">
                <span className="type-caption card__other-label">
                  {windowLabel(window, strings)}
                </span>
                <span className="type-metric card__other-value">
                  {window.usedPercent === null ? "—" : formatPercent(window.usedPercent, locale)}
                </span>
                <span className="card__other-badges">
                  {/* The risk word only. A second percentage on the same row would put four
                      figures across 380px, and the headline already carries the number. */}
                  {otherPace && <PaceBadge pace={otherPace} />}
                  <ConfidenceBadge confidence={window.confidence} />
                </span>
              </li>
            );
          })}
        </ul>
      )}

      <footer className="type-caption card__foot">
        {headline ? <ResetLine window={headline} now={now} /> : null}
        <span className="card__foot-right">
          <TodayLine snapshot={snapshot} now={now} />
        </span>
      </footer>
    </article>
  );
}
