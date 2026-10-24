import { useEffect, useRef, useState } from "react";

import { EmptyState } from "./components/EmptyState";
import { Overview } from "./components/Overview";
import { ProviderDetail } from "./components/ProviderDetail";
import { ProviderTabs, type TabKey } from "./components/ProviderTabs";
import { QuietTools } from "./components/QuietTools";
import { SettingsView } from "./components/SettingsView";
import { formatClock } from "./format";
import { visibleProviderHealth } from "./providerHealth";
import { reportPanelHeight, useDeck, useDeckState, useLocale, useStrings } from "./store";
import { awaitingSetup, totalTokens, type ProviderSnapshot } from "./types";

const DETAIL_ID = "panel-detail";

function hasReading(snapshot: ProviderSnapshot): boolean {
  return snapshot.windows.some((window) => window.usedPercent !== null);
}

/**
 * A tool earns a tab once it is on this machine and has something to show: a limit, a plan to
 * pick, or usage. A tool that writes no limit still gets one — its tokens and cost are real —
 * while one that is installed but has never been used stays in the quiet list.
 */
function earnsTab(snapshot: ProviderSnapshot): boolean {
  return (
    snapshot.installed &&
    (hasReading(snapshot) ||
      awaitingSetup(snapshot) ||
      totalTokens(snapshot.month) > 0 ||
      snapshot.burst != null)
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

  const active = deck.providers.filter(earnsTab);
  const quiet = deck.providers.filter((snapshot) => !earnsTab(snapshot));
  const reporting = deck.providers.filter(hasReading).length;
  const updated = formatClock(deck.updatedAt, locale);
  /*
   * A tab for a tool that has since gone quiet would open onto nothing, so the selection falls
   * back. With one tool there is nothing to overview: its detail is the panel.
   */
  const selected: TabKey =
    active.length === 1
      ? (active[0]?.instance ?? "all")
      : active.some((snapshot) => snapshot.instance === filter)
        ? filter
        : "all";
  const detail = active.find((snapshot) => snapshot.instance === selected) ?? null;
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
            {active.length > 1 && (
              <ProviderTabs
                snapshots={active}
                selected={selected}
                onSelect={setFilter}
                panelId={DETAIL_ID}
              />
            )}
            <div
              id={DETAIL_ID}
              role={active.length > 1 ? "tabpanel" : undefined}
              className="panel__detail"
            >
              {detail !== null ? (
                <ProviderDetail
                  key={detail.instance}
                  snapshot={detail}
                  health={visibleProviderHealth(detail, deck.health)}
                  now={now}
                  updatedAt={deck.updatedAt}
                  onSetUp={() => setView("settings")}
                  onOpenDashboard={() => void openDashboard()}
                />
              ) : null}
              {detail !== null && active.length === 1 && <QuietTools snapshots={quiet} />}
              {detail === null && (
                <>
                  <Overview snapshots={active} now={now} onOpen={setFilter} />
                  <QuietTools snapshots={quiet} />
                </>
              )}
            </div>
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
