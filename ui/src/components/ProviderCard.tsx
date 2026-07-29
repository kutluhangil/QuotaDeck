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
import { strings } from "../strings";
import {
  awaitingSetup,
  paceFor,
  totalTokens,
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
  const clock = formatClock(window.resetsAt);
  if (clock === null) return <span>{strings.card.noReset}</span>;

  const at = Date.parse(window.resetsAt ?? "");
  const seconds = Number.isNaN(at) ? null : Math.floor((at - now) / 1000);
  if (seconds === null || seconds <= 0) return <span>{strings.card.resetsAt(clock)}</span>;

  return <span>{`${strings.card.resetsAt(clock)} · ${formatDuration(seconds)}`}</span>;
}

/**
 * Where the headline window is heading.
 *
 * An exhaustion instant is named when there is one, because "full at 17:42" is the thing a
 * user can act on; otherwise the projected level stands on its own. Neither is ever stated as
 * a reading — the copy says "at this pace" and the badge is a projection of a bar, not a bar.
 */
function PaceLine({ pace, now }: { pace: PaceForecast; now: number }) {
  const clock = formatClock(pace.exhaustedAt);
  const seconds = secondsUntil(pace.exhaustedAt, now);

  return (
    <p className="card__pace type-caption">
      <PaceBadge pace={pace} />
      <span className="card__pace-text">
        {clock !== null && seconds !== null
          ? strings.pace.exhausted(clock, formatDuration(seconds))
          : strings.pace.projected(formatPercent(pace.projectedPercent))}
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
  const today = totalTokens(snapshot.today);
  const { usd, unpricedTokens } = snapshot.todayCost;

  if (usd > 0) {
    return (
      <span>
        {strings.card.todayCost(formatCost(usd))}
        {unpricedTokens > 0 && (
          <span className="card__foot-note">
            {` · ${strings.card.costPartial(formatTokens(unpricedTokens))}`}
          </span>
        )}
      </span>
    );
  }
  if (today > 0) return <span>{strings.card.todayTokens(formatTokens(today))}</span>;

  const last = formatRelative(snapshot.lastActivity, now);
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
    <article className="card">
      <header className="card__head">
        <h2 className="type-label card__name">{strings.provider[snapshot.id]}</h2>
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
          <div className="card__reading">
            <span className="type-display card__percent" data-critical={critical}>
              {formatPercent(percent)}
            </span>
            <span className="type-caption card__window">{windowLabel(headline)}</span>
          </div>
          <UsageBar percent={percent} label={windowLabel(headline)} />
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
                <span className="type-caption card__other-label">{windowLabel(window)}</span>
                <span className="type-metric card__other-value">
                  {window.usedPercent === null ? "—" : formatPercent(window.usedPercent)}
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
