import { useState } from "react";

import toolMissing from "../assets/illustrations/tool-missing.png";
import { foldBreakdown, modelsFor } from "../breakdown";
import { formatCost, formatTokens } from "../format";
import { dailyCells, historyFor, inRange, rollingRange, totals } from "../history";
import { intlLocale } from "../i18n";
import { useDeck, useDeckState, useHistory } from "../store";
import { totalTokens, type ProviderInstanceId } from "../types";
import { UsageHistory } from "../UsageHistory";
import { instanceGlance, modelName, openingSoonest } from "./quota";
import { useLiquidStrings } from "./strings";
import { useSystem } from "./system";
import { Chip, Mark, TOOL_COLOR, until, useNow, Vessel } from "./ui";

interface BarPoint {
  key: string | number;
  val: number;
  dateLabel: string;
  amountLabel: string;
}

function Bars({
  points,
  color,
  label,
  startLabel,
  endLabel,
}: {
  points: BarPoint[];
  color: string;
  label: string;
  startLabel: string;
  endLabel: string;
}) {
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);

  const values = points.map((p) => p.val);
  const peak = Math.max(...values, 0);
  const peakIndex = values.indexOf(peak);
  const activeIndex = hoveredIndex ?? (peak > 0 ? peakIndex : points.length - 1);
  const activePoint = points[activeIndex];

  return (
    <>
      <div style={{ position: "relative", height: 180, marginTop: 24 }}>
        <div className="bars" role="img" aria-label={label} style={{ position: "absolute", inset: 0, height: "100%" }}>
          {points.map((point, i) => {
            const h = peak > 0 ? Math.max(3, (point.val / peak) * 100) : 3;
            const isHovered = hoveredIndex === i;
            return (
              <div
                key={point.key}
                onMouseEnter={() => setHoveredIndex(i)}
                onMouseLeave={() => setHoveredIndex(null)}
                style={{ cursor: "pointer", position: "relative" }}
              >
                <i
                  style={{
                    height: `${h}%`,
                    background: color,
                    opacity: isHovered ? 1 : point.val > 0 ? 0.85 : 0.25,
                    boxShadow: isHovered ? "0 0 8px rgba(255,255,255,0.4)" : undefined,
                  }}
                />
              </div>
            );
          })}
        </div>
        {activePoint && activeIndex >= 0 && (
          <div
            className="glass vlabel"
            style={{
              left: `${((activeIndex + 0.5) / points.length) * 100}%`,
              top: -8,
              pointerEvents: "none",
            }}
          >
            <span className="lbl">{activePoint.dateLabel}</span>{" "}
            <span className="num" style={{ fontSize: 13, fontWeight: 600 }}>{activePoint.amountLabel}</span>
          </div>
        )}
      </div>
      <div className="row lbl" style={{ justifyContent: "space-between", marginTop: 8 }}>
        <span>{startLabel}</span>
        <span>{endLabel}</span>
      </div>
    </>
  );
}

/** A cost with tokens of unknown price is never shown as a bare "$0". */
export function costText(cost: { usd: number; unpricedTokens: number }, format: (usd: number) => string): { value: string; partial: boolean } {
  if (cost.unpricedTokens > 0 && cost.usd === 0) return { value: "—", partial: true };
  return { value: format(cost.usd), partial: cost.unpricedTokens > 0 };
}

function CostCard({ label, cost }: { label: string; cost: { usd: number; unpricedTokens: number } }) {
  const strings = useLiquidStrings();
  const locale = useDeck((state) => state.settings.locale);
  const shown = costText(cost, (usd) => formatCost(usd, locale));
  return (
    <section className="card" style={{ gap: 8 }}>
      <div className="card-h"><span className="lbl">{label}</span><span className="badge est">{strings.estimated}</span></div>
      <span className="hero" style={{ fontSize: 30 }}>{shown.value}{shown.partial && shown.value !== "—" ? "+" : ""}</span>
      {shown.partial && <span className="lbl">{strings.unpriced}</span>}
    </section>
  );
}

function PercentTool({ instance, now }: { instance: ProviderInstanceId; now: number }) {
  const strings = useLiquidStrings();
  const locale = useDeck((state) => state.settings.locale);
  const deck = useDeckState();
  const history = useHistory();
  const glance = instanceGlance(deck, instance, now);
  const snapshot = glance.snapshot;

  if (!glance.installed || !snapshot) {
    return (
      <section className="card empty-card" style={{ minHeight: 240, alignItems: "flex-start", justifyContent: "center", gap: 14 }}>
        <img className="empty-illust" src={toolMissing} alt="" aria-hidden="true" />
        <Chip dot>{strings.notInstalled}</Chip>
        <span className="h2">{strings.notInstalled}</span>
      </section>
    );
  }

  const hours = historyFor(history, snapshot.instance);
  const nowSecs = Math.floor(now / 1000);
  const weekRange = rollingRange("week", nowSecs);
  const monthRange = rollingRange("month", nowSecs);
  const weekTotals = totals(inRange(hours, weekRange));
  const models = foldBreakdown(modelsFor(history, snapshot.instance), monthRange);
  const topModel = models[0];

  const cells = dailyCells(hours, 30, nowSecs);
  const firstDate = new Date(Date.now() - (cells.length - 1) * 86400_000);
  const startLabel = firstDate.toLocaleDateString(intlLocale(locale), { day: "numeric", month: "short" });

  const points: BarPoint[] = cells.map((cell) => {
    const d = new Date(cell.start * 1000);
    return {
      key: cell.start,
      val: cell.usd,
      dateLabel: d.toLocaleDateString(intlLocale(locale), { weekday: "short", day: "numeric", month: "short" }),
      amountLabel: formatCost(cell.usd, locale),
    };
  });

  const estimated = (
    <span className="badge est">{strings.estimated}</span>
  );

  return (
    <>
      <div className="bento">
        {glance.windows.length === 0 && (
          <section className="card s12"><span className="t2">{strings.noData}</span></section>
        )}
        {glance.windows.map((window) => (
          <section key={window.windowMinutes} className="card s6" style={{ padding: 22, gap: 14 }}>
            <div className="card-h">
              <span className="h2">{strings.windowName(window.windowMinutes)}</span>
              <span className={`badge ${window.measured ? "meas" : "est"}`}>{window.measured ? strings.measured : strings.estimated}</span>
            </div>
            <div className="row" style={{ alignItems: "baseline", justifyContent: "space-between", flexWrap: "wrap" }}>
              <div className="row" style={{ alignItems: "baseline", gap: 8 }}>
                <span className="hero" style={{ fontSize: 64 }}>{Math.round(window.left)}%</span>
                <span className="t3" style={{ fontSize: 15 }}>{strings.left}</span>
              </div>
              {until(window.resetsAt, now) && <span className="mono t2" style={{ fontSize: 13 }}>{strings.resetsIn(until(window.resetsAt, now) ?? "")}</span>}
            </div>
            <Vessel value={window.left} size="lg" color={TOOL_COLOR[snapshot.id] ?? "#5E7E9E"} codex={snapshot.id === "codex"} pace={window.paceLeft} />
            {window.pace && (
              <span style={{ fontSize: 13, color: window.pace.risk === "healthy" ? undefined : "var(--warn)" }}>
                {window.pace.risk === "over"
                  ? strings.overPace(until(window.pace.exhaustedAt, now))
                  : window.pace.risk === "at-risk"
                    ? strings.atRisk
                    : strings.lastsUntilReset}
              </span>
            )}
          </section>
        ))}
      </div>
      <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(min(170px, 100%), 1fr))", gap: 12 }}>
        <CostCard label={strings.todayCost} cost={snapshot.todayCost} />
        <CostCard label={strings.windowCost} cost={{ usd: weekTotals.usd, unpricedTokens: weekTotals.unpricedTokens }} />
        <section className="card" style={{ gap: 8 }}>
          <div className="card-h"><span className="lbl">{strings.windowTokens}</span><span className="badge meas">{strings.measured}</span></div>
          <span className="hero" style={{ fontSize: 30 }}>{formatTokens(weekTotals.tokens, locale)}</span>
        </section>
        <CostCard label={strings.monthCost} cost={snapshot.monthCost} />
        <section className="card" style={{ gap: 8 }}>
          <div className="card-h"><span className="lbl">{strings.monthTokens}</span><span className="badge meas">{strings.measured}</span></div>
          <span className="hero" style={{ fontSize: 30 }}>{formatTokens(totalTokens(snapshot.month), locale)}</span>
        </section>
        <section className="card" style={{ gap: 8 }}>
          <div className="card-h"><span className="lbl">{strings.topModel}</span><span className="badge meas">{strings.measured}</span></div>
          {topModel && topModel.label ? (
            <div className="row" style={{ alignItems: "baseline", gap: 6 }}>
              <span className="hero" style={{ fontSize: 26, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                {modelName(topModel.label)}
              </span>
              {topModel.share > 0 && <span className="t3" style={{ fontSize: 12 }}>{strings.ofTokens(Math.round(topModel.share * 100))}</span>}
            </div>
          ) : (
            <span className="hero" style={{ fontSize: 30 }}>—</span>
          )}
        </section>
      </div>
      <section className="card" style={{ padding: 22 }}>
        <div className="card-h"><span className="h2">{strings.dailyCost}</span>{estimated}</div>
        <Bars points={points} color={TOOL_COLOR[snapshot.id] ?? "#A3ADB8"} label={strings.dailyCost} startLabel={startLabel} endLabel={strings.now} />
      </section>
    </>
  );
}

/** Counts per local day for the last `days` days, oldest first. */
export function perDay(times: number[], days: number, now: number): number[] {
  const today = new Date(now);
  today.setHours(0, 0, 0, 0);
  const counts = new Array<number>(days).fill(0);
  for (const at of times) {
    const day = new Date(at);
    day.setHours(0, 0, 0, 0);
    const back = Math.round((today.getTime() - day.getTime()) / 86400_000);
    if (back >= 0 && back < days) counts[days - 1 - back] = (counts[days - 1 - back] ?? 0) + 1;
  }
  return counts;
}

function AntigravityTool({ now }: { now: number }) {
  const strings = useLiquidStrings();
  const locale = useDeck((state) => state.settings.locale);
  const antigravity = useSystem((state) => state.antigravity);
  const live = useDeck((state) => state.settings.antigravityLive);
  const setLive = useDeck((state) => state.setAntigravityLive);
  const exhausted = openingSoonest(antigravity, now);

  const promptCounts = perDay(antigravity?.promptTimes ?? [], 30, now);
  const firstDate = new Date(Date.now() - (promptCounts.length - 1) * 86400_000);
  const startLabel = firstDate.toLocaleDateString(intlLocale(locale), { day: "numeric", month: "short" });

  const promptPoints: BarPoint[] = promptCounts.map((count, i) => {
    const d = new Date(Date.now() - (promptCounts.length - 1 - i) * 86400_000);
    return {
      key: i,
      val: count,
      dateLabel: d.toLocaleDateString(intlLocale(locale), { weekday: "short", day: "numeric", month: "short" }),
      amountLabel: `${count}`,
    };
  });

  return (
    <>
      <section className="card" aria-label={strings.agModels} style={{ padding: 22, gap: 4 }}>
        <div className="card-h" style={{ marginBottom: 8 }}>
          <span className="h2">{strings.agModels}</span>
          {exhausted.length > 0 ? <Chip tone="warn" dot>{strings.agExhausted(exhausted.length)}</Chip> : <Chip tone="ok" dot>{strings.agAllOpen}</Chip>}
        </div>
        {exhausted.length === 0 && <span className="t2" style={{ padding: "8px 0" }}>{strings.agAllOpen}</span>}
        {exhausted.map((reset) => (
          <div key={reset.model} style={{ display: "grid", gridTemplateColumns: "minmax(140px, 1fr) 130px minmax(200px, 2.4fr)", gap: 20, alignItems: "center", padding: "14px 0", borderTop: ".5px solid var(--hair)" }}>
            <div className="row" style={{ gap: 10 }}>
              <Mark tool="antigravity" />
              <div>
                <div style={{ fontWeight: 600, fontSize: 14 }}>{modelName(reset.model)}</div>
                <div className="lbl mono">{reset.model}</div>
              </div>
            </div>
            <span style={{ justifySelf: "start" }}><Chip tone="warn" dot>{strings.agExhaustedChip}</Chip></span>
            <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
              <Vessel value={0} empty size="md" />
              <span style={{ fontSize: 13 }}>{strings.agOpensIn(modelName(reset.model), until(reset.resetsAt, now) ?? "")}</span>
            </div>
          </div>
        ))}
      </section>
      <section className="card" aria-label={strings.agLiveTitle} style={{ padding: 22 }}>
        <div className="card-h">
          <span className="h2">{strings.agLiveTitle}</span>
          {antigravity?.liveAvailable && (
            <button type="button" className="switch" role="switch" aria-checked={live} aria-label={strings.agLiveTitle} onClick={() => setLive(!live)} />
          )}
        </div>
        {antigravity?.live ? (
          <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(min(240px, 100%), 1fr))", gap: 20 }}>
            {antigravity.live.map((bucket) => (
              <div key={bucket.group} style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                <div className="row"><span style={{ fontWeight: 600 }}>{bucket.group}</span><span className="badge meas" style={{ marginLeft: "auto" }}>{strings.measured}</span></div>
                <div className="row" style={{ alignItems: "baseline", gap: 6 }}>
                  <span className="hero" style={{ fontSize: 40 }}>{Math.round(bucket.remainingPercent)}%</span>
                  <span className="t3">{strings.left}</span>
                </div>
                <Vessel value={bucket.remainingPercent} size="md" color={TOOL_COLOR.antigravity} />
                <span className="lbl">{strings.weekly}{bucket.resetsAt && until(bucket.resetsAt, now) ? ` · ${strings.resetsIn(until(bucket.resetsAt, now) ?? "")}` : ""}</span>
              </div>
            ))}
          </div>
        ) : (
          <span className="t2">{antigravity?.liveAvailable ? (live ? strings.agChecking : strings.agLiveOff) : strings.agLiveStoreOnly}</span>
        )}
        <span className="lbl">{strings.agLiveBody}</span>
      </section>
      <section className="card" aria-label={strings.agPrompts} style={{ padding: 22 }}>
        <div className="card-h">
          <span className="h2">{strings.agPrompts}</span>
          <span className="badge meas">{strings.measured}</span>
        </div>
        <Bars points={promptPoints} color={TOOL_COLOR.antigravity ?? "#9485FF"} label={strings.agPrompts} startLabel={startLabel} endLabel={strings.now} />
      </section>
      <div className="row t3" style={{ gap: 8, padding: 4, fontSize: 12 }}>{strings.agNoLocalPercent}</div>
    </>
  );
}

export function AiTab() {
  const strings = useLiquidStrings();
  const now = useNow(1000);
  const catalogue = useDeck((state) => state.providerCatalogue);
  const enabledProviders = catalogue.filter((p) => p.enabled);
  const [selectedInstance, setSelectedInstance] = useState<string>("claude-code");

  // Keep selection valid if providers change
  const activeInstance = enabledProviders.some((p) => p.id === selectedInstance)
    ? selectedInstance
    : (enabledProviders[0]?.id ?? "claude-code");

  const activeProvider = catalogue.find((p) => p.id === activeInstance);
  const isAntigravity = activeProvider?.provider === "antigravity";

  return (
    <>
      <div className="row" style={{ justifyContent: "space-between", flexWrap: "wrap", gap: 12, padding: "4px 4px 8px" }}>
        <div className="seg" role="tablist" aria-label={strings.aiQuota}>
          {enabledProviders.map((entry) => (
            <button
              key={entry.id}
              type="button"
              role="tab"
              aria-selected={activeInstance === entry.id}
              className={`tab${activeInstance === entry.id ? " on" : ""}`}
              onClick={() => setSelectedInstance(entry.id)}
            >
              {entry.provider === "claude-code" || entry.provider === "codex" || entry.provider === "antigravity" ? (
                <Mark tool={entry.provider} small />
              ) : (
                <span className="mark sm outline" style={{ marginRight: 4 }}>
                  {entry.displayName.slice(0, 2)}
                </span>
              )}
              {entry.label ?? entry.displayName}
            </button>
          ))}
        </div>
      </div>
      {isAntigravity ? <AntigravityTool now={now} /> : <PercentTool instance={activeInstance} now={now} />}
      <section className="card" aria-label={strings.history} style={{ padding: 0, overflow: "hidden" }}>
        <div className="card-h" style={{ padding: "16px 20px 0" }}><span className="h2">{strings.history}</span></div>
        <div className="legacy legacy-host">
          <UsageHistory filterInstance={activeInstance} />
        </div>
      </section>
    </>
  );
}
