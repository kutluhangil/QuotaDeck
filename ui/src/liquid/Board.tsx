/**
 * The dashboard window (designs: `Overview`, `Mac`, `Docker`, `AIClaude`, `AIAntigravity`).
 * A floating glass toolbar over a scrolling bento of content cards.
 */

import { useEffect, useState } from "react";

import { useDeck } from "../store";
import { AiTab } from "./AiTab";
import { DockerTab } from "./DockerTab";
import { LiquidSettings } from "./LiquidSettings";
import { MacTab } from "./MacTab";
import { OverviewTab } from "./OverviewTab";
import { useLiquidStrings } from "./strings";
import { useSystem, type Span } from "./system";
import { useNow, useQdClass } from "./ui";

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
  const [settingsOpen, setSettingsOpen] = useState(false);
  const minuteNow = useNow(60_000);

  useEffect(() => {
    void start();
    startSystem();
  }, [start, startSystem]);

  useEffect(() => {
    if (!settingsOpen) return;
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        event.preventDefault();
        setSettingsOpen(false);
      }
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [settingsOpen]);

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
          <button
            type="button"
            className="btn icon glass"
            aria-label={strings.settings}
            aria-pressed={settingsOpen}
            onClick={() => setSettingsOpen((open) => !open)}
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" aria-hidden="true">
              <path d="M2.5 4.5h11M2.5 8h11M2.5 11.5h11" />
              <circle cx="5.5" cy="4.5" r="1.7" fill="currentColor" />
              <circle cx="10.5" cy="8" r="1.7" fill="currentColor" />
              <circle cx="7" cy="11.5" r="1.7" fill="currentColor" />
            </svg>
          </button>
        </div>
      </nav>
      <main className="content">
        {current === "overview" && <OverviewTab onOpen={setTab} query={query} />}
        {current === "mac" && <MacTab query={query} />}
        {current === "docker" && <DockerTab query={query} />}
        {current === "ai" && <AiTab />}
      </main>
      {settingsOpen && (
        <div
          style={{
            position: "fixed",
            inset: 0,
            zIndex: 50,
            background: "rgba(0,0,0,0.38)",
            backdropFilter: "blur(6px)",
            WebkitBackdropFilter: "blur(6px)",
            display: "flex",
            justifyContent: "center",
            alignItems: "flex-start",
            padding: "40px 20px",
            overflowY: "auto",
          }}
          onClick={(event) => {
            if (event.target === event.currentTarget) setSettingsOpen(false);
          }}
        >
          <div
            role="dialog"
            aria-modal="true"
            aria-label={strings.settings}
            className="glass"
            style={{
              position: "relative",
              width: "min(640px, 100%)",
              borderRadius: 26,
              padding: "20px 22px 24px",
              display: "flex",
              flexDirection: "column",
              gap: 14,
              background: "var(--material)",
              maxHeight: "calc(100vh - 80px)",
              overflowY: "auto",
            }}
          >
            <div className="row" style={{ justifyContent: "space-between", padding: "0 8px" }}>
              <span className="h1" style={{ fontSize: 18 }}>{strings.settings}</span>
              <button type="button" className="btn primary" onClick={() => setSettingsOpen(false)}>
                {strings.done}
              </button>
            </div>
            <LiquidSettings now={minuteNow} />
          </div>
        </div>
      )}
    </div>
  );
}
