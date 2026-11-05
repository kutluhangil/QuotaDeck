/**
 * One row per AI tool: Claude and Codex as a fill with a pace marker, Antigravity as its
 * model states (it writes no percentage locally).
 */

import { useDeck, useDeckState, useStrings } from "../store";
import type { ProviderId } from "../types";
import { worstWindow } from "../types";
import { modelName, openingSoonest, toolGlance, type ToolGlance } from "./quota";
import { useLiquidStrings } from "./strings";
import { useSystem } from "./system";
import { Mark, TOOL_COLOR, until, Vessel } from "./ui";

export function toolName(key: ProviderId, catalogue: ReturnType<typeof useStrings>): string {
  return catalogue.provider[key] ?? key;
}

function PercentRow({ glance, now, large }: { glance: ToolGlance; now: number; large?: boolean }) {
  const strings = useLiquidStrings();
  const catalogue = useStrings();
  const headline = glance.headline;
  const resets = headline ? until(headline.resetsAt, now) : null;
  const name = glance.key === "claude-code" ? "Claude" : toolName(glance.key, catalogue);
  const sub = headline
    ? [strings.windowName(headline.windowMinutes), resets ? strings.resetsIn(resets) : null].filter(Boolean).join(" · ")
    : strings.noData;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: large ? 8 : 5 }}>
      <div className="row" style={{ gap: 6 }}>
        <Mark tool={glance.key} small={!large} />
        <span style={{ fontSize: large ? 13 : 12, fontWeight: 600 }}>{name}</span>
        {large && headline && (
          <span className={`badge ${headline.measured ? "meas" : "est"}`} style={{ marginLeft: "auto" }}>
            {headline.measured ? strings.measured : strings.estimated}
          </span>
        )}
        {!large && (
          <span className="num" style={{ marginLeft: "auto", fontSize: 15 }}>
            {headline ? `${Math.round(headline.left)}%` : "—"}
          </span>
        )}
      </div>
      {large && (
        <div className="row" style={{ alignItems: "baseline", gap: 6 }}>
          <span className="hero" style={{ fontSize: 40 }}>{headline ? `${Math.round(headline.left)}%` : "—"}</span>
          <span className="t3">{strings.left}</span>
        </div>
      )}
      <Vessel
        value={headline?.left ?? 0}
        size={large ? "md" : "sm"}
        color={TOOL_COLOR[glance.key]}
        codex={glance.key === "codex"}
        empty={headline === null}
        pace={headline?.paceLeft ?? null}
      />
      <div className="lbl" style={large ? undefined : { whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }} title={sub}>{sub}</div>
      {large && headline?.pace && <PaceLine glance={glance} now={now} />}
    </div>
  );
}

function PaceLine({ glance, now }: { glance: ToolGlance; now: number }) {
  const strings = useLiquidStrings();
  const pace = glance.headline?.pace;
  if (!pace) return null;
  if (pace.risk === "over") {
    return <div style={{ fontSize: 12, color: "var(--warn)" }}>{strings.overPace(until(pace.exhaustedAt, now))}</div>;
  }
  if (pace.risk === "at-risk") return <div style={{ fontSize: 12, color: "var(--warn)" }}>{strings.atRisk}</div>;
  return <div className="t2" style={{ fontSize: 12 }}>{strings.lastsUntilReset}</div>;
}

function AntigravityRow({ now, large }: { now: number; large?: boolean }) {
  const strings = useLiquidStrings();
  const antigravity = useSystem((state) => state.antigravity);
  const live = antigravity?.live ?? null;
  const exhausted = openingSoonest(antigravity, now);
  const first = exhausted[0];
  const sub = first
    ? strings.agOpensIn(modelName(first.model), until(first.resetsAt, now) ?? "")
    : strings.agAllOpen;
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: large ? 8 : 5 }}>
      <div className="row" style={{ gap: 6 }}>
        <Mark tool="antigravity" small={!large} />
        <span style={{ fontSize: large ? 13 : 12, fontWeight: 600 }}>Antigravity</span>
        {large && <span className="badge meas" style={{ marginLeft: "auto" }}>{strings.measured}</span>}
        {!large && live && (
          <span className="num" style={{ marginLeft: "auto", fontSize: 15 }}>
            {`${Math.round(Math.min(...live.map((b) => b.remainingPercent)))}%`}
          </span>
        )}
      </div>
      {large && (
        <div className="row" style={{ alignItems: "baseline", gap: 6 }}>
          {live ? (
            <>
              <span className="hero" style={{ fontSize: 40 }}>{`${Math.round(Math.min(...live.map((b) => b.remainingPercent)))}%`}</span>
              <span className="t3">{strings.left}</span>
            </>
          ) : (
            <>
              <span className="hero" style={{ fontSize: 40 }}>{exhausted.length === 0 ? "✓" : exhausted.length}</span>
              <span className="t3">{exhausted.length === 0 ? strings.agAllOpen : strings.agExhausted(exhausted.length)}</span>
            </>
          )}
        </div>
      )}
      <div style={{ display: "flex", gap: large ? 4 : 3 }}>
        {live
          ? live.map((bucket) => (
              <Vessel key={bucket.group} value={bucket.remainingPercent} size={large ? "md" : "sm"} color={TOOL_COLOR.antigravity} style={{ flex: 1 }} />
            ))
          : exhausted.length === 0
            ? <Vessel value={100} size={large ? "md" : "sm"} color={TOOL_COLOR.antigravity} style={{ flex: 1 }} />
            : exhausted.map((reset) => <Vessel key={reset.model} value={0} empty size={large ? "md" : "sm"} style={{ flex: 1 }} />)}
      </div>
      <div className="lbl" style={large ? undefined : { whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }} title={sub}>{sub}</div>
    </div>
  );
}

/** The tools shown, in order: the three v2 tools first, then anything else the user enabled. */
export function useAiTools(now: number): { key: ProviderId; glance: ToolGlance | null }[] {
  const deck = useDeckState();
  const catalogue = useDeck((state) => state.providerCatalogue);
  const antigravity = useSystem((state) => state.antigravity);
  const enabled = (key: ProviderId) =>
    catalogue.length === 0 || catalogue.some((entry) => entry.id === key && entry.enabled);
  const tools: { key: ProviderId; glance: ToolGlance | null }[] = [];
  for (const key of ["claude-code", "codex"] as const) {
    const glance = toolGlance(deck, key, now);
    if (glance.installed && enabled(key)) tools.push({ key, glance });
  }
  const agInstalled = deck.providers.some((p) => p.id === "antigravity" && p.installed) || (antigravity?.exhausted.length ?? 0) > 0 || antigravity?.live != null;
  if (agInstalled && enabled("antigravity")) tools.push({ key: "antigravity", glance: null });
  for (const snapshot of deck.providers) {
    if (["claude-code", "codex", "antigravity"].includes(snapshot.id) || !snapshot.installed) continue;
    if (snapshot.instance !== snapshot.id || worstWindow(snapshot.windows)?.usedPercent == null) continue;
    tools.push({ key: snapshot.id, glance: toolGlance(deck, snapshot.id as "codex", now) });
  }
  return tools;
}

export function AiRow({ tool, now, large }: { tool: { key: ProviderId; glance: ToolGlance | null }; now: number; large?: boolean }) {
  if (tool.key === "antigravity") return <AntigravityRow now={now} large={large} />;
  return tool.glance ? <PercentRow glance={tool.glance} now={now} large={large} /> : null;
}
