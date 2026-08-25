import { useEffect, useRef, useState } from "react";

import { EmptyState } from "./components/EmptyState";
import { ProviderCard } from "./components/ProviderCard";
import { QuietTools } from "./components/QuietTools";
import { SettingsView } from "./components/SettingsView";
import { formatClock } from "./format";
import { identityHue } from "./identity";
import { visibleProviderHealth } from "./providerHealth";
import { reportPanelHeight, useDeck, useDeckState, useLocale, useStrings } from "./store";
import { awaitingSetup, type ProviderId, type ProviderSnapshot } from "./types";

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

/**
 * One chip per tool with a card, plus the one that clears the narrowing.
 *
 * Only drawn from two tools up: a filter offering a single choice is a control that cannot
 * change anything, and it would cost a row of panel height to say so.
 */
function FilterChips({
  snapshots,
  filter,
  onPick,
}: {
  snapshots: ProviderSnapshot[];
  filter: ProviderId | "all";
  onPick: (filter: ProviderId | "all") => void;
}) {
  const strings = useStrings();
  if (snapshots.length < 2) return null;

  return (
    <div className="filters" role="group" aria-label={strings.filters.label}>
      <button
        type="button"
        className="type-caption filters__chip"
        aria-pressed={filter === "all"}
        onClick={() => onPick("all")}
      >
        {strings.filters.all}
      </button>
      {snapshots.map((snapshot) => (
        <button
          key={snapshot.id}
          type="button"
          className="type-caption filters__chip"
          aria-pressed={filter === snapshot.id}
          onClick={() => onPick(snapshot.id)}
        >
          <span className="card__dot" data-hue={identityHue(snapshot.id)} aria-hidden="true" />
          {strings.provider[snapshot.id]}
        </button>
      ))}
    </div>
  );
}

export function App() {
  const strings = useStrings();
  const locale = useLocale();
  const deck = useDeckState();
  const access = useDeck((state) => state.access);
  const setDemo = useDeck((state) => state.setDemo);
  const requestAccess = useDeck((state) => state.requestAccess);
  const view = useDeck((state) => state.view);
  const setView = useDeck((state) => state.setView);
  const filter = useDeck((state) => state.filter);
  const setFilter = useDeck((state) => state.setFilter);
  const openDashboard = useDeck((state) => state.openDashboard);
  const hidePanel = useDeck((state) => state.hidePanel);
  const quit = useDeck((state) => state.quit);
  const start = useDeck((state) => state.start);
  const shellError = useDeck((state) => state.shellError);
  const refreshBusy = useDeck((state) => state.refreshBusy);
  const refreshError = useDeck((state) => state.refreshError);
  const refreshNow = useDeck((state) => state.refreshNow);
  const providerCatalogue = useDeck((state) => state.providerCatalogue);
  const [now, setNow] = useState(() => Date.now());
  const bodyRef = useRef<HTMLElement>(null);

  useEffect(() => {
    void start().catch((error) => {
      const message = `start app: ${String(error)}`;
      console.error(message);
      useDeck.setState({ shellError: message });
    });
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
  /*
   * A narrowing to a tool that has since stopped reporting would empty the panel and leave
   * no visible reason why. The chip for it is gone by then, so the filter goes with it.
   */
  const narrowed = active.some((snapshot) => snapshot.id === filter) ? filter : "all";
  const shown = narrowed === "all" ? active : active.filter((s) => s.id === narrowed);
  /*
   * Asked for before anything else. Without the grant every provider root is unreadable, and
   * the panel would report three working tools as broken instead of asking for the one thing
   * that fixes all of them at once.
   */
  const needsAccess = access !== null && access.required && !access.granted;
  const allProvidersDisabled =
    providerCatalogue.length > 0 && providerCatalogue.every((provider) => !provider.enabled);

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
        {shellError !== null && (
          <p className="type-caption settings__error" role="alert">
            {strings.shellFailed(shellError)}
          </p>
        )}
        {refreshError !== null && (
          <p className="type-caption settings__error" role="alert">
            {strings.refreshFailed(refreshError)}
          </p>
        )}
        {view === "settings" ? (
          <SettingsView now={now} />
        ) : needsAccess ? (
          <EmptyState
            title={strings.empty.noPermission.title}
            body={
              access.error === null
                ? strings.empty.noPermission.body
                : `${strings.empty.noPermission.body} ${strings.settings.accessFailed(access.error)}`
            }
            action={strings.empty.noPermission.action}
            onAction={() => void requestAccess()}
            /* A machine with no supported tool would otherwise end here. The sample is the
               one honest thing left to offer, and it says on the tin that it is a sample. */
            secondaryAction={strings.empty.demoAction}
            onSecondaryAction={() => setDemo(true)}
          />
        ) : allProvidersDisabled ? (
          <EmptyState
            title={strings.empty.providersDisabled.title}
            body={strings.empty.providersDisabled.body}
            action={strings.header.settings}
            onAction={() => setView("settings")}
          />
        ) : deck.providers.length === 0 ? (
          deck.scanning ? (
            <EmptyState title={strings.appName} body={strings.empty.scanning} />
          ) : (
            <EmptyState
              title={strings.empty.noTools.title}
              body={strings.empty.noTools.body}
              action={strings.empty.demoAction}
              onAction={() => setDemo(true)}
            />
          )
        ) : (
          <>
            <FilterChips snapshots={active} filter={narrowed} onPick={setFilter} />
            {shown.map((snapshot) => (
              <ProviderCard
                key={snapshot.id}
                snapshot={snapshot}
                health={visibleProviderHealth(snapshot, deck.health)}
                now={now}
                onSetUp={() => setView("settings")}
              />
            ))}
            {/* Narrowed to one tool, the other tools are not the answer to anything. */}
            {narrowed === "all" && <QuietTools snapshots={quiet} />}
          </>
        )}
      </main>

      <footer className="panel__foot">
        {/*
          Polite, and cheap to be: the clock is minute-precision, so the five-second refresh
          rewrites the same text and announces nothing. What does change — the scan finishing —
          is exactly the transition worth hearing about.

          Silent about the count while the folder is still being asked for: "0 of 3 reporting"
          beside a screen explaining that nothing can be read yet is a second answer to a
          question the user has already been given.
        */}
        <span className="type-caption panel__status" role="status" aria-live="polite">
          {deck.scanning
            ? strings.empty.scanning
            : [
                updated ? strings.footer.updated(updated) : "",
                needsAccess ? "" : strings.footer.reporting(reporting, deck.providers.length),
              ]
                .filter(Boolean)
                .join(" · ")}
        </span>
        <span className="panel__actions" role="toolbar" aria-label={strings.a11y.footerActions}>
          <button
            type="button"
            className="type-caption panel__action"
            onClick={() => void refreshNow()}
            disabled={refreshBusy}
            aria-busy={refreshBusy}
          >
            {strings.footer.refresh}
          </button>
          <button
            type="button"
            className="type-caption panel__action"
            onClick={() => void openDashboard()}
          >
            {strings.footer.dashboard}
          </button>
          {/*
            The only way out used to be the tray menu, which is a right click on macOS and
            Windows and a left click on Linux. Three gestures for one action, none of them
            written down anywhere.
          */}
          <button type="button" className="type-caption panel__action" onClick={() => void quit()}>
            {strings.footer.quit}
          </button>
        </span>
      </footer>
    </div>
  );
}
