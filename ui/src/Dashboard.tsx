import { useEffect, useId, useRef, useState } from "react";

import { BreakdownList } from "./components/BreakdownList";
import { ConfidenceBadge } from "./components/ConfidenceBadge";
import { EmptyState } from "./components/EmptyState";
import { Heatmap } from "./components/Heatmap";
import { WindowRow } from "./components/WindowRow";
import {
  agentsDroppedFor,
  agentsFor,
  foldBreakdown,
  modelsDroppedFor,
  modelsFor,
  projectsDroppedFor,
  projectsFor,
  shortenPaths,
} from "./breakdown";
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
  localDateRange,
  rollingRange,
  totals,
  RANGE_DAYS,
  type HistoryRange,
  type Range,
} from "./history";
import { identityHue } from "./identity";
import {
  confidenceForHealth,
  visibleProviderHealth,
  type VisibleProviderHealth,
} from "./providerHealth";
import type { Catalogue } from "./i18n";
import { ProviderHealthNotice } from "./components/ProviderHealthNotice";
import { useDeck, useDeckState, useHistory, useLocale, useStrings } from "./store";
import { paceFor, sortedWindows, worstWindow, type ProviderSnapshot } from "./types";

const ranges: Range[] = ["day", "week", "month", "quarter", "year"];

/**
 * The rolling range this window reports over.
 *
 * A radio group rather than three toggle buttons: exactly one is chosen at a time, and that is
 * the semantic a screen reader needs to say "1 of 3" instead of reading three unrelated
 * pressed states. Arrow keys move between them and only the chosen one is in the tab order,
 * which is what a radio group is expected to do.
 */
function RangePicker({
  range,
  available,
  onChange,
}: {
  range: Range;
  available: Range[];
  onChange: (next: Range) => void;
}) {
  const strings = useStrings();
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);

  function step(index: number, delta: number) {
    const next = (index + delta + available.length) % available.length;
    const option = available[next];
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
      {available.map((option, index) => (
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
  health,
  historyRange,
  heatmapDays,
  nowSeconds,
}: {
  snapshot: ProviderSnapshot;
  health: VisibleProviderHealth | null;
  historyRange: HistoryRange;
  heatmapDays: number;
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
  const sum = totals(inRange(hours, historyRange));
  const cells = dailyCells(inRange(hours, historyRange), heatmapDays, historyRange.to - 1);
  const models = foldBreakdown(modelsFor(history, snapshot.id), historyRange);
  const modelsDropped = modelsDroppedFor(history, snapshot.id);
  const agents = foldBreakdown(agentsFor(history, snapshot.id), historyRange);
  const agentsDropped = agentsDroppedFor(history, snapshot.id);
  const projects = foldBreakdown(projectsFor(history, snapshot.id), historyRange);
  const projectsDropped = projectsDroppedFor(history, snapshot.id);
  // Shortened against the rows actually on screen, so two directories sharing a last segment
  // never draw as one project.
  const projectNames = shortenPaths(
    projects.map((row) => row.label).filter((label): label is string => label !== null),
  );

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
          {headline && (
            <ConfidenceBadge
              confidence={confidenceForHealth(headline.confidence, health, nowSeconds * 1000)}
            />
          )}
          {level && (
            <span className="type-caption card__status" data-level={level}>
              {strings.status[level]}
            </span>
          )}
        </span>
      </header>

      <ProviderHealthNotice health={health} />

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
                confidence={confidenceForHealth(window.confidence, health, nowSeconds * 1000)}
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

      {/* What the range went on. The totals above say how much; this says what for, which is
          the question a total cannot answer and the one that changes what a user does next. */}
      <section className="board__breakdown">
        <h3 className="type-caption board__total-label">{strings.breakdown.models}</h3>
        <BreakdownList
          rows={models}
          dropped={modelsDropped}
          label={strings.breakdown.listLabel(name)}
          unreported={strings.breakdown.unreported}
          droppedNote={strings.breakdown.dropped}
        />
      </section>

      {/* The same spend, cut the other way. "Which model" says what it cost; "which directory"
          says what it was for, and that is the cut a user acts on. */}
      <section className="board__breakdown">
        <h3 className="type-caption board__total-label">{strings.breakdown.projects}</h3>
        <BreakdownList
          rows={projects}
          dropped={projectsDropped}
          label={strings.breakdown.projectListLabel(name)}
          unreported={strings.breakdown.unattributed}
          droppedNote={strings.breakdown.droppedProjects}
          display={(label) => projectNames.get(label) ?? label}
        />
      </section>

      {/* Not "what for" but "who by". The one cut that separates spend somebody typed from
          spend that ran on its own, which is the difference this app was built to show. */}
      <section className="board__breakdown">
        <h3 className="type-caption board__total-label">{strings.breakdown.agents}</h3>
        <BreakdownList
          rows={agents}
          dropped={agentsDropped}
          label={strings.breakdown.agentListLabel(name)}
          unreported={strings.breakdown.unattributed}
          droppedNote={strings.breakdown.droppedAgents}
          display={(label) => originName(label, strings)}
        />
      </section>

      <Heatmap cells={cells} />
    </article>
  );
}

/**
 * The catalogue's name for an origin key.
 *
 * A key the backend added and this build does not know is shown verbatim rather than dropped
 * or renamed — it is still a real label, and inventing a translation for it would be worse.
 */
function originName(label: string, strings: Catalogue): string {
  const names: Record<string, string> = strings.breakdown.origin;
  return names[label] ?? label;
}

export function Dashboard() {
  const strings = useStrings();
  const deck = useDeckState();
  const start = useDeck((state) => state.start);
  const refreshNow = useDeck((state) => state.refreshNow);
  const refreshBusy = useDeck((state) => state.refreshBusy);
  const refreshError = useDeck((state) => state.refreshError);
  const copyUsageExport = useDeck((state) => state.copyUsageExport);
  const exportBusy = useDeck((state) => state.exportBusy);
  const exportError = useDeck((state) => state.exportError);
  const exportMessage = useDeck((state) => state.exportMessage);
  const [range, setRange] = useState<Range>("week");
  const [nowSeconds, setNowSeconds] = useState(() => Math.floor(Date.now() / 1000));
  const [customFrom, setCustomFrom] = useState(() => daysBeforeToday(6));
  const [customTo, setCustomTo] = useState(() => dateInputValue(new Date()));
  const [customRange, setCustomRange] = useState(false);

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
  const availableRanges = ranges.filter((option) => RANGE_DAYS[option] <= deck.retention.effectiveDays);
  const visibleRange = availableRanges.includes(range) ? range : "month";
  const selectedRange = customRange
    ? localDateRange(dateFromInput(customFrom), dateFromInput(customTo))
    : rollingRange(visibleRange, nowSeconds);
  const selectedDays = customRange
    ? calendarDays(customFrom, customTo)
    : RANGE_DAYS[visibleRange];
  const heatmapDays = Math.min(90, deck.retention.effectiveDays, Math.max(1, selectedDays));
  const exportDisabled = deck.scanning || deck.retention.rebuilding || exportBusy;
  const exportRange = {
    from: new Date(selectedRange.from * 1000).toISOString(),
    to: new Date(selectedRange.to * 1000).toISOString(),
  };

  return (
    <div className="board">
      <header className="board__head">
        <span className="type-label board__title">
          <span className="panel__glyph" aria-hidden="true" />
          {strings.dashboard.title}
        </span>
        <div className="board__actions">
          <button
            type="button"
            className="type-caption panel__action"
            onClick={() => void refreshNow()}
            disabled={refreshBusy}
            aria-busy={refreshBusy}
          >
            {strings.footer.refresh}
          </button>
          <RangePicker
            range={visibleRange}
            available={availableRanges}
            onChange={(next) => {
              setRange(next);
              setCustomRange(false);
            }}
          />
          <div className="board__export" aria-label={strings.dashboard.customRange}>
            <label className="type-caption board__date-label">
              {strings.dashboard.rangeFrom}
              <input
                type="date"
                value={customFrom}
                max={customTo}
                onChange={(event) => {
                  setCustomFrom(event.target.value);
                  setCustomRange(true);
                }}
              />
            </label>
            <label className="type-caption board__date-label">
              {strings.dashboard.rangeTo}
              <input
                type="date"
                value={customTo}
                min={customFrom}
                onChange={(event) => {
                  setCustomTo(event.target.value);
                  setCustomRange(true);
                }}
              />
            </label>
            <button
              type="button"
              className="type-caption panel__action"
              disabled={exportDisabled}
              aria-busy={exportBusy}
              onClick={() => void copyUsageExport("json", exportRange, null)}
            >
              {exportBusy ? strings.dashboard.exporting : strings.dashboard.copyJson}
            </button>
            <button
              type="button"
              className="type-caption panel__action"
              disabled={exportDisabled}
              aria-busy={exportBusy}
              onClick={() => void copyUsageExport("csv", exportRange, null)}
            >
              {exportBusy ? strings.dashboard.exporting : strings.dashboard.copyCsv}
            </button>
          </div>
        </div>
      </header>

      <main className="board__body" aria-label={strings.a11y.tools}>
        {refreshError !== null && (
          <p className="type-caption settings__error" role="alert">
            {strings.refreshFailed(refreshError)}
          </p>
        )}
        {deck.retention.rebuilding && (
          <p className="type-caption board__notice" role="status">
            {strings.dashboard.rebuilding(deck.retention.effectiveDays, deck.retention.requestedDays)}
          </p>
        )}
        {deck.retention.error !== null && (
          <p className="type-caption settings__error" role="alert">
            {strings.dashboard.rebuildFailed(deck.retention.error)}
          </p>
        )}
        {exportError !== null && (
          <p className="type-caption settings__error" role="alert">
            {strings.dashboard.exportFailed(exportError)}
          </p>
        )}
        {exportMessage !== null && (
          <>
            <p className="type-caption board__notice" role="status">
              {strings.dashboard.copied(exportMessage.format.toUpperCase(), exportMessage.rows)}
            </p>
            {exportMessage.clamped && (
              <p className="type-caption board__notice" role="status">
                {strings.dashboard.exportClamped(
                  exportMessage.effectiveRange.from,
                  exportMessage.effectiveRange.to,
                )}
              </p>
            )}
          </>
        )}
        {exportDisabled && !exportBusy && (
          <p className="type-caption board__notice" role="status">
            {strings.dashboard.exportUnavailable}
          </p>
        )}
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
                health={visibleProviderHealth(snapshot, deck.health)}
                historyRange={selectedRange}
                heatmapDays={heatmapDays}
                nowSeconds={nowSeconds}
              />
            ))}
          </div>
        )}
      </main>

      <footer className="board__foot type-caption">
        <span>{customRange ? strings.dashboard.customRange : strings.dashboard.rangeSpan(RANGE_DAYS[visibleRange])}</span>
        <span>{strings.dashboard.retention(deck.retention.effectiveDays)}</span>
        <span>{strings.dashboard.hourlyHistory}</span>
      </footer>
    </div>
  );
}

function dateInputValue(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function dateFromInput(value: string): Date {
  const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (match === null) return new Date();
  return new Date(Number(match[1]), Number(match[2]) - 1, Number(match[3]));
}

/** Calendar count, not elapsed milliseconds: DST has no reason to change a selected day count. */
function calendarDays(from: string, to: string): number {
  const start = dateFromInput(from);
  const end = dateFromInput(to);
  const startDay = Date.UTC(start.getFullYear(), start.getMonth(), start.getDate());
  const endDay = Date.UTC(end.getFullYear(), end.getMonth(), end.getDate());
  return Math.max(1, Math.floor((endDay - startDay) / 86_400_000) + 1);
}

function daysBeforeToday(days: number): string {
  const date = new Date();
  date.setDate(date.getDate() - days);
  return dateInputValue(date);
}
