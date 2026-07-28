import {
  formatClock,
  formatDuration,
  formatPercent,
  formatRelative,
  formatTokens,
  levelFor,
  windowLabel,
} from "../format";
import { strings } from "../strings";
import { totalTokens, type ProviderSnapshot, type QuotaWindow } from "../types";
import { ConfidenceBadge } from "./ConfidenceBadge";
import { HorizonStrip } from "./HorizonStrip";
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

export function ProviderCard({
  snapshot,
  now,
}: {
  snapshot: ProviderSnapshot;
  now: number;
}) {
  const headline = headlineWindow(snapshot.windows);
  const others = snapshot.windows.filter((window) => window !== headline);
  const today = totalTokens(snapshot.today);
  const percent = headline?.usedPercent ?? null;
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
        </>
      ) : (
        <p className="type-body card__quiet">
          {strings.unavailable[snapshot.unavailable ?? "never-reported"]}
        </p>
      )}

      {others.length > 0 && (
        <ul className="card__others">
          {others.map((window) => (
            <li key={`${window.limitId}-${window.windowMinutes}`} className="card__other">
              <span className="type-caption card__other-label">{windowLabel(window)}</span>
              <span className="type-metric card__other-value">
                {window.usedPercent === null ? "—" : formatPercent(window.usedPercent)}
              </span>
              <ConfidenceBadge confidence={window.confidence} />
            </li>
          ))}
        </ul>
      )}

      <footer className="type-caption card__foot">
        {headline ? <ResetLine window={headline} now={now} /> : null}
        <span className="card__foot-right">
          {today > 0
            ? strings.card.todayTokens(formatTokens(today))
            : (formatRelative(snapshot.lastActivity, now)
              ? strings.card.lastActivity(formatRelative(snapshot.lastActivity, now) ?? "")
              : strings.card.neverUsed)}
        </span>
      </footer>
    </article>
  );
}
