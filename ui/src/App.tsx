import { useEffect, useRef, useState } from "react";

import { EmptyState } from "./components/EmptyState";
import { ProviderCard } from "./components/ProviderCard";
import { QuietTools } from "./components/QuietTools";
import { SettingsView } from "./components/SettingsView";
import { formatClock } from "./format";
import { strings } from "./strings";
import { reportPanelHeight, useDeck } from "./store";
import { awaitingSetup, type ProviderSnapshot } from "./types";

function hasReading(snapshot: ProviderSnapshot): boolean {
  return snapshot.windows.some((window) => window.usedPercent !== null);
}

/**
 * A provider earns a card once it has a window to show — or once it is working but waiting on
 * something the user can supply. Folding "pick your plan" into the quiet section would hide
 * the one action that turns a blank card into a reading.
 */
function earnsCard(snapshot: ProviderSnapshot): boolean {
  return hasReading(snapshot) || awaitingSetup(snapshot);
}

export function App() {
  const deck = useDeck((state) => state.deck);
  const view = useDeck((state) => state.view);
  const setView = useDeck((state) => state.setView);
  const openDashboard = useDeck((state) => state.openDashboard);
  const start = useDeck((state) => state.start);
  const [now, setNow] = useState(() => Date.now());
  const bodyRef = useRef<HTMLElement>(null);

  useEffect(() => {
    void start();
  }, [start]);

  // Countdowns move slowly. One tick a minute keeps them honest without waking the CPU.
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  // A popover should be as tall as what it has to say. The window follows the content
  // rather than leaving a block of empty surface under two cards.
  useEffect(() => {
    const body = bodyRef.current;
    if (body === null) return;
    const observer = new ResizeObserver(() => {
      void reportPanelHeight(body.scrollHeight);
    });
    observer.observe(body);
    void reportPanelHeight(body.scrollHeight);
    return () => observer.disconnect();
  }, [view, deck]);

  const active = deck.providers.filter(earnsCard);
  const quiet = deck.providers.filter((snapshot) => !earnsCard(snapshot));
  const reporting = deck.providers.filter(hasReading).length;
  const updated = formatClock(deck.updatedAt);

  return (
    <div className="panel">
      <header className="panel__head">
        <span className="type-label panel__title">
          <span className="panel__glyph" aria-hidden="true" />
          {strings.appName}
        </span>
        <span className="panel__actions">
          <button
            type="button"
            className="type-caption panel__action"
            onClick={() => void openDashboard()}
          >
            {strings.header.expand}
          </button>
          <button
            type="button"
            className="type-caption panel__action"
            onClick={() => setView(view === "settings" ? "panel" : "settings")}
            aria-pressed={view === "settings"}
          >
            {view === "settings" ? strings.settings.back : strings.header.settings}
          </button>
        </span>
      </header>

      <main className="panel__body" ref={bodyRef}>
        {view === "settings" ? (
          <SettingsView now={now} />
        ) : deck.providers.length === 0 ? (
          deck.scanning ? (
            <EmptyState title={strings.appName} body={strings.empty.scanning} />
          ) : (
            <EmptyState title={strings.empty.noTools.title} body={strings.empty.noTools.body} />
          )
        ) : (
          <>
            {active.map((snapshot) => (
              <ProviderCard
                key={snapshot.id}
                snapshot={snapshot}
                now={now}
                onSetUp={() => setView("settings")}
              />
            ))}
            <QuietTools snapshots={quiet} />
          </>
        )}
      </main>

      <footer className="panel__foot type-caption">
        <span>
          {deck.scanning
            ? strings.empty.scanning
            : updated
              ? strings.footer.updated(updated)
              : ""}
        </span>
        <span>{strings.footer.reporting(reporting, deck.providers.length)}</span>
      </footer>
    </div>
  );
}
