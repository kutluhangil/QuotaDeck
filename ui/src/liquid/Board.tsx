/**
 * The dashboard window (designs: `Overview`, `Mac`, `Docker`, `AIClaude`, `AIAntigravity`).
 * A floating glass toolbar over a scrolling bento of content cards.
 */

import { useEffect, useState } from "react";

import { useDeck } from "../store";
import { AiTab } from "./AiTab";
import { DockerTab } from "./DockerTab";
import { MacTab } from "./MacTab";
import { OverviewTab } from "./OverviewTab";
import { useLiquidStrings } from "./strings";
import { useSystem, type Span } from "./system";
import { useQdClass } from "./ui";

export type Tab = "overview" | "mac" | "docker" | "ai";

const SPANS: Span[] = [1, 24, 168];

export function Board() {
  const strings = useLiquidStrings();
  const qd = useQdClass();
  const start = useDeck((state) => state.start);
  const startSystem = useSystem((state) => state.start);
  const monitored = useSystem((state) => state.snapshot !== null);
  const span = useSystem((state) => state.span);
  const setSpan = useSystem((state) => state.setSpan);
  const [tab, setTab] = useState<Tab>("overview");
  const [query, setQuery] = useState("");

  useEffect(() => {
    void start();
    startSystem();
  }, [start, startSystem]);

  const tabs: Tab[] = monitored ? ["overview", "mac", "docker", "ai"] : ["ai"];
  const current = tabs.includes(tab) ? tab : (tabs[0] ?? "ai");

  return (
    <div className={`${qd} win dash`}>
      <nav className="toolbar" aria-label="Dashboard" data-tauri-drag-region>
        <div data-tauri-drag-region />
        <div className="tabs glass" role="tablist">
          {tabs.map((key) => (
            <button
              key={key}
              type="button"
              role="tab"
              aria-selected={current === key}
              className={`tab${current === key ? " on" : ""}`}
              onClick={() => setTab(key)}
            >
              {strings.tabs[key]}
            </button>
          ))}
        </div>
        <div style={{ display: "flex", gap: 8, justifyContent: "flex-end", alignItems: "center", flexWrap: "wrap" }}>
          {monitored && (
            <div className="seg glass" role="group" aria-label="Time range">
              {SPANS.map((value) => (
                <button
                  key={value}
                  type="button"
                  className={`tab${span === value ? " on" : ""}`}
                  aria-pressed={span === value}
                  onClick={() => setSpan(value)}
                >
                  {strings.span[value]}
                </button>
              ))}
            </div>
          )}
          {(current === "mac" || current === "docker" || current === "overview") && (
            <label className="search glass">
              <svg width="14" height="14" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" aria-hidden="true">
                <circle cx="7" cy="7" r="4.5" />
                <path d="M10.5 10.5 14 14" />
              </svg>
              <span className="sr">{strings.search}</span>
              <input type="search" placeholder={strings.search} value={query} onChange={(event) => setQuery(event.target.value)} />
            </label>
          )}
        </div>
      </nav>
      <main className="content">
        {current === "overview" && <OverviewTab onOpen={setTab} query={query} />}
        {current === "mac" && <MacTab query={query} />}
        {current === "docker" && <DockerTab query={query} />}
        {current === "ai" && <AiTab />}
      </main>
    </div>
  );
}
