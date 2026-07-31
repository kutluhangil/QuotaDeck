import { useEffect, useId, useRef, useState } from "react";

import { ConfidenceBadge } from "./components/ConfidenceBadge";
import { EmptyState } from "./components/EmptyState";
import { Heatmap } from "./components/Heatmap";
import { WindowRow } from "./components/WindowRow";
import {
  formatCost,
  formatDuration,
  formatPercent,
  formatSpan,
  formatTokens,
  levelFor,
  levelForRisk,
  secondsUntil,
  windowLabel,
} from "./format";
import {
  dailyCells,
  historyFor,
  inRange,
  totals,
  HEATMAP_DAYS,
  RANGE_DAYS,
  type Range,
} from "./history";
import { identityHue } from "./identity";
import { useDeck, useDeckState, useHistory, useLocale, useStrings } from "./store";
import { paceFor, sortedWindows, worstWindow, type ProviderSnapshot } from "./types";

const ranges: Range[] = ["day", "week", "month"];

/**
 * The rolling range this window reports over.
 *
 * A radio group rather than three toggle buttons: exactly one is chosen at a time, and that is
 * the semantic a screen reader needs to say "1 of 3" instead of reading three unrelated
 * pressed states. Arrow keys move between them and only the chosen one is in the tab order,
 * which is what a radio group is expected to do.
 */
function RangePicker({ range, onChange }: { range: Range; onChange: (next: Range) => void }) {
  const strings = useStrings();
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);

  function step(index: number, delta: number) {
    const next = (index + delta + ranges.length) % ranges.length;
    const option = ranges[next];
    if (option === undefined) return;
    onChange(option);
    buttons.current[next]?.focus();
  }

  function onKeyDown(event: React.KeyboardEvent<HTMLButtonElement>, index: number) {
    const delta =
      event.key === "ArrowRight" || event.key === "ArrowDown"
        ? 1
        : event.key === "ArrowLeft" || event.key === "ArrowUp"
          ? -1
          : 0;
    if (delta === 0) return;
    event.preventDefault();
    step(index, delta);
  }

  return (
    <div className="board__ranges" role="radiogroup" aria-label={strings.dashboard.rangeLabel}>
      {ranges.map((option, index) => (
        <button
          key={option}
          ref={(element) => {
            buttons.current[index] = element;
          }}
          type="button"
          role="radio"
          className="type-caption board__range"
          aria-checked={range === option}
          tabIndex={range === option ? 0 : -1}
          onKeyDown={(event) => onKeyDown(event, index)}
          onClick={() => onChange(option)}
        >
          {strings.dashboard.range[option]}
        </button>
      ))}
    </div>
  );
}

function ProviderPanel({
  snapshot,
  range,
  nowSeconds,
}: {
  snapshot: ProviderSnapshot;
  range: Range;
  nowSeconds: number;
}) {
  const strings = useStrings();
  const locale = useLocale();
  const nameId = useId();
  // Selected whole, narrowed here. `historyFor` builds a fresh empty array for a provider
  // with no history, and a selector that returns one hands zustand a new snapshot on every
  // render.
  const history = useHistory();
  const hours = historyFor(history, snapshot.id);
  const sum = totals(inRange(hours, range, nowSeconds));
  const cells = dailyCells(hours, HEATMAP_DAYS, nowSeconds);

  const name = strings.provider[snapshot.id];
  const headline = worstWindow(snapshot.windows);
  const headlinePace = headline === null ? null : paceFor(snapshot, headline);
  const level = headline?.usedPercent == null ? null : levelFor(headline.usedPercent);

  return (
    <article className="board__card" aria-labelledby={nameId}>
      <header className="board__card-head">
        <h2 className="type-label board__name" id={nameId}>
          <span className="card__dot" data-hue={identityHue(snapshot.id)} aria-hidden="true" />
          {name}
        </h2>
        <span className="card__head-right">
          {headline && <ConfidenceBadge confidence={headline.confidence} />}
          {level && (
            <span className="type-caption card__status" data-level={level}>
              {strings.status[level]}
            </span>
          )}
        </span>
      </header>

      {snapshot.windows.length > 0 ? (
        /* The same four columns the panel draws. Two surfaces reading the same limits in two
           different grammars is two things to learn for one fact. */
        <ul className="card__rows" role="list" aria-label={strings.a11y.windows(name)}>
          {sortedWindows(snapshot.windows).map((window) => {
            const seconds = secondsUntil(window.resetsAt, nowSeconds * 1000);
            return (
              <WindowRow
                key={`${window.limitId}-${window.windowMinutes}`}
                label={formatSpan(window.windowMinutes * 60, strings)}
                spokenLabel={windowLabel(window, strings)}
                percent={window.usedPercent}
                confidence={window.confidence}
                meta={seconds === null ? "" : formatDuration(seconds, strings)}
              />
            );
          })}
          {headlinePace && (
            <WindowRow
              kind="pace"
              label={strings.pace.rowLabel}
              spokenLabel={strings.pace.label(
                formatPercent(headlinePace.projectedPercent, locale),
              )}
              percent={headlinePace.projectedPercent}
              level={levelForRisk(headlinePace.risk)}
              meta={strings.pace.risk[headlinePace.risk]}
            />
          )}
        </ul>
      ) : (
        <p className="type-body board__quiet">
          {strings.unavailable[snapshot.unavailable ?? "never-reported"]}
        </p>
      )}

      <dl className="board__totals">
        <div className="board__total">
          <dt className="type-caption board__total-label">{strings.dashboard.rangeTokens}</dt>
          <dd className="type-metric board__total-value">{formatTokens(sum.tokens, locale)}</dd>
        </div>
        <div className="board__total">
          <dt className="type-caption board__total-label">{strings.dashboard.rangeCost}</dt>
          <dd className="type-metric board__total-value">
            {sum.usd > 0 ? formatCost(sum.usd, locale) : "—"}
          </dd>
        </div>
      </dl>
      {/* Said outright rather than folded into the dollar figure. A total that quietly drops
          a model it could not price under-reports a month without admitting it. */}
      {sum.unpricedTokens > 0 && (
        <p className="type-caption board__note">
          {strings.dashboard.unpriced(formatTokens(sum.unpricedTokens, locale))}
        </p>
      )}

      <Heatmap cells={cells} />
    </article>
  );
}

export function Dashboard() {
  const strings = useStrings();
  const deck = useDeckState();
  const start = useDeck((state) => state.start);
  const [range, setRange] = useState<Range>("week");
  const [nowSeconds, setNowSeconds] = useState(() => Math.floor(Date.now() / 1000));

  useEffect(() => {
    void start();
  }, [start]);

  // The heatmap's last column is today; it has to move when the day does.
  useEffect(() => {
    const timer = window.setInterval(() => setNowSeconds(Math.floor(Date.now() / 1000)), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  const reporting = deck.providers.filter(
    (snapshot) => snapshot.installed && snapshot.unavailable !== "not-installed",
  );

  return (
    <div className="board">
      <header className="board__head">
        <span className="type-label board__title">
          <span className="panel__glyph" aria-hidden="true" />
          {strings.dashboard.title}
        </span>
        <RangePicker range={range} onChange={setRange} />
      </header>

      <main className="board__body" aria-label={strings.a11y.tools}>
        {reporting.length === 0 ? (
          <EmptyState
            title={strings.empty.noTools.title}
            body={deck.scanning ? strings.empty.scanning : strings.empty.noTools.body}
          />
        ) : (
          <div className="board__grid">
            {reporting.map((snapshot) => (
              <ProviderPanel
                key={snapshot.id}
                snapshot={snapshot}
                range={range}
                nowSeconds={nowSeconds}
              />
            ))}
          </div>
        )}
      </main>

      <footer className="board__foot type-caption">
        <span>{strings.dashboard.rangeSpan(RANGE_DAYS[range])}</span>
        <span>{strings.dashboard.retention(HEATMAP_DAYS)}</span>
      </footer>
    </div>
  );
}
