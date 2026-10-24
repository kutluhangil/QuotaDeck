import { formatClock, formatCost, formatDate, formatDuration, formatTokens } from "../format";
import { useLocale, useStrings } from "../store";
import { totalTokens, type SessionSummary } from "../types";

/** Rows shown. The rest are a scroll of the export, not of the dashboard. */
const SHOWN = 8;

/**
 * The newest sessions in the range, one row each: when it started and for how long, the model,
 * where, how many tokens and what they cost. A long session is the unit people think in, and
 * no hourly total answers "what did that one cost".
 */
export function SessionList({
  sessions,
  label,
  display,
}: {
  sessions: SessionSummary[];
  /** Names the list for a screen reader. */
  label: string;
  /** Shortens a project path for the screen; the full path stays in the row's title. */
  display: (path: string) => string;
}) {
  const strings = useStrings();
  const locale = useLocale();
  if (sessions.length === 0) {
    return <p className="type-caption board__note">{strings.breakdown.sessionsEmpty}</p>;
  }
  return (
    <ul className="sessions" role="list" aria-label={label}>
      {sessions.slice(0, SHOWN).map((row) => {
        const started = new Date(row.startedAt);
        const seconds = Math.max(0, (Date.parse(row.lastAt) - started.getTime()) / 1000);
        const when = `${formatDate(started, locale)} ${formatClock(row.startedAt, locale) ?? ""}`;
        const cost =
          row.cost.usd > 0 ? formatCost(row.cost.usd, locale) : null;
        return (
          <li key={row.session} className="sessions__row" title={row.project ?? undefined}>
            <span className="type-caption sessions__when">
              {when}
              {row.project && <span className="sessions__project"> · {display(row.project)}</span>}
            </span>
            <span className="type-caption sessions__meta">
              {strings.breakdown.sessionMeta(
                formatDuration(seconds, strings),
                row.model ?? strings.breakdown.sessionModelUnreported,
              )}
            </span>
            <span className="type-caption sessions__amount">
              {[formatTokens(totalTokens(row.tokens), locale), cost].filter(Boolean).join(" · ")}
            </span>
          </li>
        );
      })}
    </ul>
  );
}
