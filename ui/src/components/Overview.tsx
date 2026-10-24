import { formatCost, formatDuration, formatTokens, secondsUntil, windowLabel } from "../format";
import { identityHue } from "../identity";
import { useDeck, useLocale, useStrings } from "../store";
import { totalTokens, worstWindow, type ProviderInstanceId, type ProviderSnapshot } from "../types";
import { usageSentence } from "./ProviderDetail";
import { UsageBar } from "./UsageBar";

/**
 * Every tool on one screen, one row each: its fullest limit, or for a tool without one, what
 * it spent today. A row is a button that opens that tool's tab.
 */
export function Overview({
  snapshots,
  now,
  onOpen,
}: {
  snapshots: ProviderSnapshot[];
  now: number;
  onOpen: (instance: ProviderInstanceId) => void;
}) {
  const strings = useStrings();
  const locale = useLocale();
  const display = useDeck((state) => state.settings.usageDisplay);

  return (
    <ul className="overview" role="list">
      {snapshots.map((snapshot) => {
        const name = snapshot.label ?? strings.provider[snapshot.id];
        const headline = worstWindow(snapshot.windows);
        const percent = headline?.usedPercent ?? null;
        const seconds = headline === null ? null : secondsUntil(headline.resetsAt, now);
        const today = totalTokens(snapshot.today);
        const detail =
          percent !== null
            ? [
                usageSentence(percent, display, strings, locale),
                seconds === null ? null : strings.detail.resetsIn(formatDuration(seconds, strings)),
              ]
                .filter(Boolean)
                .join(" · ")
            : snapshot.todayCost.usd > 0
              ? strings.cost.today(formatCost(snapshot.todayCost.usd, locale), formatTokens(today, locale))
              : strings.cost.todayTokens(formatTokens(today, locale));
        return (
          <li key={snapshot.instance}>
            <button
              type="button"
              className="overview__row"
              onClick={() => onOpen(snapshot.instance)}
              aria-label={`${strings.detail.openTool(name)}: ${detail}`}
            >
              <span className="type-label overview__name">
                <span
                  className="card__dot"
                  data-hue={identityHue(snapshot.instance)}
                  aria-hidden="true"
                />
                {name}
              </span>
              {percent !== null && headline !== null ? (
                <UsageBar percent={percent} windowName={windowLabel(headline, strings)} />
              ) : (
                <span className="row__bar-empty" />
              )}
              <span className="type-caption overview__detail">{detail}</span>
            </button>
          </li>
        );
      })}
    </ul>
  );
}
