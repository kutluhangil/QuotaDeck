import { useEffect, useRef, useState } from "react";

import { EmptyState } from "./components/EmptyState";
import { ProviderCard } from "./components/ProviderCard";
import { QuietTools } from "./components/QuietTools";
import { SettingsView } from "./components/SettingsView";
import { formatClock } from "./format";
import { reportPanelHeight, useDeck, useLocale, useStrings } from "./store";
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
  const strings = useStrings();
  const locale = useLocale();
  const deck = useDeck((state) => state.deck);
  const view = useDeck((state) => state.view);
  const setView = useDeck((state) => state.setView);
  const openDashboard = useDeck((state) => state.openDashboard);
  const hidePanel = useDeck((state) => state.hidePanel);
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

  /*
   * Escape steps back the way a popover is expected to: out of settings first, then out of the
   * panel itself. Clicking away already dismisses it, and a menu bar window that can only be
   * closed with the mouse is one a keyboard user is stuck inside.
   */
  useEffect(() => {
    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      event.preventDefault();
      if (view === "settings") setView("panel");
      else void hidePanel();
    }
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [view, setView, hidePanel]);

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

  /*
   * Move focus with the view. Toggling to settings replaces everything under the header, and a
   * keyboard user whose focus stayed on the button would tab from the top of a screen that is
   * no longer there.
   */
  useEffect(() => {
    bodyRef.current?.focus();
  }, [view]);

  const active = deck.providers.filter(earnsCard);
  const quiet = deck.providers.filter((snapshot) => !earnsCard(snapshot));
  const reporting = deck.providers.filter(hasReading).length;
  const updated = formatClock(deck.updatedAt, locale);

  return (
    <div className="panel">
      <header className="panel__head">
        <span className="type-label panel__title">
          <span className="panel__glyph" aria-hidden="true" />
          {strings.appName}
        </span>
        <span className="panel__actions" role="toolbar" aria-label={strings.a11y.panelActions}>
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

      <main
        className="panel__body"
        ref={bodyRef}
        // Focused programmatically on a view change, never reachable by tabbing into it.
        tabIndex={-1}
        aria-label={view === "settings" ? strings.a11y.settingsRegion : strings.a11y.tools}
      >
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
        {/*
          Polite, and cheap to be: the clock is minute-precision, so the five-second refresh
          rewrites the same text and announces nothing. What does change — the scan finishing —
          is exactly the transition worth hearing about.
        */}
        <span role="status" aria-live="polite">
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
