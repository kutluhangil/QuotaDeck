import { useEffect, useState } from "react";

import { ConfidenceBadge } from "./components/ConfidenceBadge";
import { EmptyState } from "./components/EmptyState";
import { Heatmap } from "./components/Heatmap";
import { PaceBadge } from "./components/PaceBadge";
import { formatCost, formatPercent, formatTokens, windowLabel } from "./format";
import {
  dailyCells,
  historyFor,
  inRange,
  totals,
  HEATMAP_DAYS,
  RANGE_DAYS,
  type Range,
} from "./history";
import { strings } from "./strings";
import { useDeck } from "./store";
import { paceFor, type ProviderSnapshot } from "./types";

const ranges: Range[] = ["day", "week", "month"];

function ProviderPanel({
  snapshot,
  range,
  nowSeconds,
}: {
  snapshot: ProviderSnapshot;
  range: Range;
  nowSeconds: number;
}) {
  const hours = useDeck((state) => historyFor(state.history, snapshot.id));
  const sum = totals(inRange(hours, range, nowSeconds));
  const cells = dailyCells(hours, HEATMAP_DAYS, nowSeconds);

  return (
    <article className="board__card">
      <header className="board__card-head">
        <h2 className="type-label board__name">{strings.provider[snapshot.id]}</h2>
      </header>

      {snapshot.windows.length > 0 ? (
        <ul className="board__windows">
          {snapshot.windows.map((window) => {
            const pace = paceFor(snapshot, window);
            return (
              <li key={`${window.limitId}-${window.windowMinutes}`} className="board__window">
                <span className="type-caption board__window-label">{windowLabel(window)}</span>
                <span className="type-metric board__window-value">
                  {window.usedPercent === null ? "—" : formatPercent(window.usedPercent)}
                </span>
                {pace ? <PaceBadge pace={pace} /> : <span />}
                <ConfidenceBadge confidence={window.confidence} />
              </li>
            );
          })}
        </ul>
      ) : (
        <p className="type-body board__quiet">
          {strings.unavailable[snapshot.unavailable ?? "never-reported"]}
        </p>
      )}

      <dl className="board__totals">
        <div className="board__total">
          <dt className="type-caption board__total-label">{strings.dashboard.rangeTokens}</dt>
          <dd className="type-metric board__total-value">{formatTokens(sum.tokens)}</dd>
        </div>
        <div className="board__total">
          <dt className="type-caption board__total-label">{strings.dashboard.rangeCost}</dt>
          <dd className="type-metric board__total-value">
            {sum.usd > 0 ? formatCost(sum.usd) : "—"}
          </dd>
        </div>
      </dl>
      {/* Said outright rather than folded into the dollar figure. A total that quietly drops
          a model it could not price under-reports a month without admitting it. */}
      {sum.unpricedTokens > 0 && (
        <p className="type-caption board__note">
          {strings.dashboard.unpriced(formatTokens(sum.unpricedTokens))}
        </p>
      )}

      <Heatmap cells={cells} />
    </article>
  );
}

export function Dashboard() {
  const deck = useDeck((state) => state.deck);
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
        <div className="board__ranges" role="group" aria-label={strings.dashboard.rangeLabel}>
          {ranges.map((option) => (
            <button
              key={option}
              type="button"
              className="type-caption board__range"
              aria-pressed={range === option}
              onClick={() => setRange(option)}
            >
              {strings.dashboard.range[option]}
            </button>
          ))}
        </div>
      </header>

      <main className="board__body">
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
