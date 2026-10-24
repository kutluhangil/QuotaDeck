import { useId } from "react";

import {
  formatClock,
  formatCost,
  formatDuration,
  formatFactor,
  formatPercent,
  formatRelative,
  formatSpan,
  formatTokens,
  levelForRisk,
  secondsUntil,
  windowLabel,
} from "../format";
import { identityHue } from "../identity";
import type { Catalogue } from "../i18n";
import { confidenceForHealth, type VisibleProviderHealth } from "../providerHealth";
import { useDeck, useLocale, useStrings } from "../store";
import {
  awaitingSetup,
  paceFor,
  sortedWindows,
  totalTokens,
  worstWindow,
  type CostRange,
  type LiveReading,
  type ProviderSnapshot,
  type QuotaWindow,
  type TokenRollup,
  type UsageDisplay,
} from "../types";
import { ConfidenceBadge } from "./ConfidenceBadge";
import { HorizonStrip } from "./HorizonStrip";
import { ProviderHealthNotice } from "./ProviderHealthNotice";
import { UsageBar } from "./UsageBar";

/** A used percentage, turned the way the user chose to read it, as the sentence it goes in. */
export function usageSentence(
  usedPercent: number,
  display: UsageDisplay,
  strings: Catalogue,
  locale: Parameters<typeof formatPercent>[1],
): string {
  return display === "remaining"
    ? strings.detail.left(formatPercent(Math.max(0, 100 - usedPercent), locale))
    : strings.detail.used(formatPercent(usedPercent, locale));
}

export function windowTitle(window: QuotaWindow, strings: Catalogue): string {
  const titles = strings.detail.windowTitle;
  const title =
    window.kind === "other" ? titles.other(window.windowMinutes) : titles[window.kind];
  return window.kind === "other" ? title : `${title} · ${formatSpan(window.windowMinutes * 60, strings)}`;
}

/** A live reading older than this describes a session that has since gone quiet. */
const LIVE_FRESH_MS = 2 * 60 * 60 * 1000;

/**
 * The running session, as the tool reported it: a context bar, and whether the prompt cache is
 * still warm. Both come from Claude Code's own status line, so nothing here is estimated.
 */
function LiveSession({ live, now }: { live: LiveReading; now: number }) {
  const strings = useStrings();
  const locale = useLocale();
  if (now - Date.parse(live.observedAt) > LIVE_FRESH_MS) return null;
  const expires = secondsUntil(live.cacheExpiresAt, now);
  const cache =
    live.cacheWarm === true && expires !== null && expires > 0
      ? strings.detail.cacheWarm(formatDuration(expires, strings), live.cacheTtl ?? "")
      : live.cacheWarm === null
        ? null
        : strings.detail.cacheCold;
  const hits =
    live.cacheHitRatio === null ? null : strings.detail.cacheHits(formatPercent(live.cacheHitRatio * 100, locale));
  return (
    <>
      {live.contextUsedPercent !== null && (
        <div className="block">
          <div className="block__head">
            <span className="type-label block__title">{strings.detail.contextTitle}</span>
          </div>
          <UsageBar percent={live.contextUsedPercent} windowName={strings.detail.contextTitle} />
          <div className="type-caption block__foot">
            <span className="block__value">
              {strings.detail.used(formatPercent(live.contextUsedPercent, locale))}
            </span>
            {live.contextWindowSize !== null && (
              <span>{strings.detail.contextOf(formatTokens(live.contextWindowSize, locale))}</span>
            )}
          </div>
        </div>
      )}
      {(cache !== null || hits !== null) && (
        <p className="type-caption detail__note">{[cache, hits].filter(Boolean).join(" · ")}</p>
      )}
    </>
  );
}

/** One limit: its name, a full-width bar, how much is spent or left, and when it lets go. */
function WindowBlock({
  window,
  health,
  now,
  display,
}: {
  window: QuotaWindow;
  health: VisibleProviderHealth | null;
  now: number;
  display: UsageDisplay;
}) {
  const strings = useStrings();
  const locale = useLocale();
  const seconds = secondsUntil(window.resetsAt, now);
  const percent = window.usedPercent;
  return (
    <li className="block">
      <div className="block__head">
        <span className="type-label block__title">{windowTitle(window, strings)}</span>
        <ConfidenceBadge confidence={confidenceForHealth(window.confidence, health, now)} compact />
      </div>
      {percent === null ? (
        <span className="row__bar-empty" />
      ) : (
        <UsageBar percent={percent} windowName={windowLabel(window, strings)} />
      )}
      <div className="type-caption block__foot">
        <span className="block__value">
          {percent === null ? "—" : usageSentence(percent, display, strings, locale)}
        </span>
        <span>
          {seconds === null
            ? strings.card.noReset
            : strings.detail.resetsIn(formatDuration(seconds, strings))}
        </span>
      </div>
    </li>
  );
}

/** Today and the rolling 30 days, in equivalent API cost where a price is known. */
export function CostSection({
  snapshot,
  onOpen,
}: {
  snapshot: ProviderSnapshot;
  onOpen?: () => void;
}) {
  const strings = useStrings();
  const locale = useLocale();

  function line(
    tokens: TokenRollup,
    cost: CostRange,
    priced: (amount: string, tokens: string) => string,
    bare: (tokens: string) => string,
  ): string {
    const count = formatTokens(totalTokens(tokens), locale);
    const text = cost.usd > 0 ? priced(formatCost(cost.usd, locale), count) : bare(count);
    // A dollar total that leaves tokens out says so rather than quietly under-reporting.
    return cost.usd > 0 && cost.unpricedTokens > 0
      ? `${text} · ${strings.cost.unpriced(formatTokens(cost.unpricedTokens, locale))}`
      : text;
  }

  const idle = totalTokens(snapshot.month) === 0;
  const prompt = snapshot.month.input + snapshot.month.cacheRead + snapshot.month.cacheCreation;
  const hitRate = snapshot.month.cacheRead > 0 && prompt > 0 ? (snapshot.month.cacheRead / prompt) * 100 : null;
  return (
    <section className="cost" aria-label={strings.cost.title}>
      <div className="cost__head">
        <span className="type-label">{strings.cost.title}</span>
        {onOpen && (
          <button
            type="button"
            className="type-caption cost__open"
            onClick={onOpen}
            aria-label={strings.cost.open}
          >
            ›
          </button>
        )}
      </div>
      {idle ? (
        <p className="type-caption cost__line">{strings.cost.none}</p>
      ) : (
        <>
          <p className="type-caption cost__line">
            {line(snapshot.today, snapshot.todayCost, strings.cost.today, strings.cost.todayTokens)}
          </p>
          <p className="type-caption cost__line">
            {line(snapshot.month, snapshot.monthCost, strings.cost.month, strings.cost.monthTokens)}
          </p>
          {hitRate !== null && (
            <p className="type-caption cost__line">
              {strings.cost.cacheHits(formatPercent(hitRate, locale))}
            </p>
          )}
        </>
      )}
    </section>
  );
}

/**
 * One tool, the way a tab opens it: a header naming it and saying how fresh it is, one block
 * per limit, the pace projection under the limit it projects, the timeline, and what it cost.
 */
export function ProviderDetail({
  snapshot,
  health,
  now,
  updatedAt,
  onSetUp,
  onOpenDashboard,
}: {
  snapshot: ProviderSnapshot;
  health: VisibleProviderHealth | null;
  now: number;
  /** When the deck last refreshed, for the freshness line. */
  updatedAt: string;
  onSetUp?: () => void;
  onOpenDashboard?: () => void;
}) {
  const strings = useStrings();
  const locale = useLocale();
  const display = useDeck((state) => state.settings.usageDisplay);
  const planId = useDeck((state) => state.settings.plans[snapshot.instance] ?? null);
  const plans = useDeck((state) => state.plans);
  // Only a tool that sells tiers can be set up; one that writes no limit is shown as it is.
  const offersPlans = plans.some(
    (entry) => entry.provider === snapshot.id && entry.plans.length > 0,
  );
  const nameId = useId();
  const name = snapshot.label ?? strings.provider[snapshot.id];
  const headline = worstWindow(snapshot.windows);
  const measurable = snapshot.windows.some((window) => window.usedPercent !== null);
  const pace = headline === null || !measurable ? null : paceFor(snapshot, headline);
  const paceClock = pace === null ? null : formatClock(pace.exhaustedAt, locale);
  const plan =
    planId === null
      ? null
      : (plans
          .find((entry) => entry.provider === snapshot.id)
          ?.plans.find((option) => option.id === planId)?.label ?? null);
  const relative = formatRelative(health?.lastSuccessAt ?? updatedAt, now, strings);
  const updated =
    relative === null
      ? null
      : strings.detail.updated(
          relative === strings.relative.justNow ? relative : strings.relative.ago(relative),
        );

  return (
    <article className="detail" aria-labelledby={nameId}>
      <header className="detail__head">
        <div className="detail__title">
          <h2 className="type-label detail__name" id={nameId}>
            <span className="card__dot" data-hue={identityHue(snapshot.instance)} aria-hidden="true" />
            {name}
          </h2>
          {updated && <span className="type-caption detail__updated">{updated}</span>}
        </div>
        <span className="detail__head-right">
          {plan && <span className="type-caption detail__plan">{plan}</span>}
          {/* A tool without a limit says so once, in the body, not again as a badge. */}
          {headline && (
            <ConfidenceBadge confidence={confidenceForHealth(headline.confidence, health, now)} />
          )}
        </span>
      </header>

      <ProviderHealthNotice health={health} />
      {snapshot.readError && (
        <p className="type-caption settings__error" role="alert">
          {snapshot.readError}
        </p>
      )}

      {measurable ? (
        <ul className="detail__blocks" role="list" aria-label={strings.a11y.windows(name)}>
          {sortedWindows(snapshot.windows).map((window) => (
            <WindowBlock
              key={`${window.limitId}-${window.windowMinutes}`}
              window={window}
              health={health}
              now={now}
              display={display}
            />
          ))}
        </ul>
      ) : awaitingSetup(snapshot) && offersPlans ? (
        <p className="type-body card__quiet">
          {strings.card.pickPlan}
          {onSetUp && (
            <button type="button" className="card__setup" onClick={onSetUp}>
              {strings.card.pickPlanAction}
            </button>
          )}
        </p>
      ) : (
        <p className="type-caption detail__note">{strings.detail.tokensOnly}</p>
      )}

      {pace && (
        <p className="type-caption detail__pace" data-level={levelForRisk(pace.risk)}>
          {strings.detail.pace(
            strings.pace.risk[pace.risk],
            formatPercent(pace.projectedPercent, locale),
          )}
          {paceClock && ` · ${strings.detail.runsOut(paceClock)}`}
        </p>
      )}

      {snapshot.live && <LiveSession live={snapshot.live} now={now} />}

      {snapshot.burst && (
        <p className="type-caption card__burst" role="note">
          <span className="card__burst-label">{strings.burst.label}</span>{" "}
          {strings.burst.detail(
            formatTokens(snapshot.burst.tokens, locale),
            formatFactor(snapshot.burst.factor, locale),
          )}
        </p>
      )}

      {snapshot.series.length > 0 && headline && (
        <HorizonStrip window={headline} series={snapshot.series} now={now} />
      )}

      <CostSection snapshot={snapshot} onOpen={onOpenDashboard} />
    </article>
  );
}
