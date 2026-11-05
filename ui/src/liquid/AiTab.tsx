import { useState } from "react";

import { formatCost, formatTokens } from "../format";
import { dailyCells, historyFor } from "../history";
import { intlLocale } from "../i18n";
import { useDeck, useDeckState, useHistory } from "../store";
import { totalTokens } from "../types";
import { UsageHistory } from "../UsageHistory";
import { modelName, openingSoonest, toolGlance, type ToolKey } from "./quota";
import { useLiquidStrings } from "./strings";
import { useSystem } from "./system";
import { Chip, Mark, TOOL_COLOR, until, useNow, Vessel } from "./ui";

const TOOLS: { key: ToolKey; name: string }[] = [
  { key: "claude-code", name: "Claude" },
  { key: "codex", name: "Codex" },
  { key: "antigravity", name: "Antigravity" },
];

function Bars({ values, color, label }: { values: number[]; color: string; label: string }) {
  const strings = useLiquidStrings();
  const locale = useDeck((state) => state.settings.locale);
  const peak = Math.max(...values, 0);
  const first = new Date(Date.now() - (values.length - 1) * 86400_000);
  return (
    <>
      <div className="bars" role="img" aria-label={label}>
        {values.map((value, i) => (
          <div key={i}>
            <i style={{ height: `${peak > 0 ? Math.max(2, (value / peak) * 100) : 2}%`, background: color, opacity: value > 0 ? 1 : 0.25 }} />
          </div>
        ))}
      </div>
      <div className="row lbl" style={{ justifyContent: "space-between" }}>
        <span>{first.toLocaleDateString(intlLocale(locale), { day: "numeric", month: "short" })}</span>
        <span>{strings.now}</span>
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

function PercentTool({ tool, now }: { tool: ToolKey; now: number }) {
  const strings = useLiquidStrings();
  const locale = useDeck((state) => state.settings.locale);
  const deck = useDeckState();
  const history = useHistory();
  const glance = toolGlance(deck, tool, now);
  const snapshot = glance.snapshot;
  if (!glance.installed || !snapshot) {
    return <section className="card empty-card"><span className="h2">{strings.notInstalled}</span></section>;
  }
  const cells = dailyCells(historyFor(history, snapshot.instance), 30, Math.floor(now / 1000));
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
            <Vessel value={window.left} size="lg" color={TOOL_COLOR[tool]} codex={tool === "codex"} pace={window.paceLeft} />
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
        <section className="card" style={{ gap: 8 }}>
          <div className="card-h"><span className="lbl">{strings.todayTokens}</span><span className="badge meas">{strings.measured}</span></div>
          <span className="hero" style={{ fontSize: 30 }}>{formatTokens(totalTokens(snapshot.today), locale)}</span>
        </section>
        <CostCard label={strings.monthCost} cost={snapshot.monthCost} />
        <section className="card" style={{ gap: 8 }}>
          <div className="card-h"><span className="lbl">{strings.monthTokens}</span><span className="badge meas">{strings.measured}</span></div>
          <span className="hero" style={{ fontSize: 30 }}>{formatTokens(totalTokens(snapshot.month), locale)}</span>
        </section>
      </div>
      <section className="card" style={{ padding: 22 }}>
        <div className="card-h"><span className="h2">{strings.dailyCost}</span>{estimated}</div>
        <Bars values={cells.map((cell) => cell.usd)} color={TOOL_COLOR[tool] ?? "#A3ADB8"} label={strings.dailyCost} />
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
  const antigravity = useSystem((state) => state.antigravity);
  const live = useDeck((state) => state.settings.antigravityLive);
  const setLive = useDeck((state) => state.setAntigravityLive);
  const exhausted = openingSoonest(antigravity, now);
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
        <Bars values={perDay(antigravity?.promptTimes ?? [], 30, now)} color={TOOL_COLOR.antigravity ?? "#9485FF"} label={strings.agPrompts} />
      </section>
      <div className="row t3" style={{ gap: 8, padding: 4, fontSize: 12 }}>{strings.agNoLocalPercent}</div>
    </>
  );
}

export function AiTab() {
  const strings = useLiquidStrings();
  const now = useNow(1000);
  const [tool, setTool] = useState<ToolKey>("claude-code");
  return (
    <>
      <div className="row" style={{ justifyContent: "space-between", flexWrap: "wrap", gap: 12, padding: "4px 4px 8px" }}>
        <div className="seg" role="tablist" aria-label={strings.aiQuota}>
          {TOOLS.map((entry) => (
            <button key={entry.key} type="button" role="tab" aria-selected={tool === entry.key} className={`tab${tool === entry.key ? " on" : ""}`} onClick={() => setTool(entry.key)}>
              <Mark tool={entry.key} small />
              {entry.name}
            </button>
          ))}
        </div>
      </div>
      {tool === "antigravity" ? <AntigravityTool now={now} /> : <PercentTool tool={tool} now={now} />}
      <section className="card" aria-label={strings.history} style={{ padding: 0, overflow: "hidden" }}>
        <div className="card-h" style={{ padding: "16px 20px 0" }}><span className="h2">{strings.history}</span></div>
        <div className="legacy legacy-host">
          <UsageHistory />
        </div>
      </section>
    </>
  );
}
