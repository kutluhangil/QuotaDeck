import { useEffect, useRef, useState } from "react";

import { Popover } from "./liquid/Popover";
import { useSystem } from "./liquid/system";
import { useQdClass } from "./liquid/ui";
import { reportPanelHeight, useDeck, useStrings } from "./store";

/** The menu bar popover. Its content lives in `liquid/Popover.tsx`. */
export function App() {
  const strings = useStrings();
  const view = useDeck((state) => state.view);
  const setView = useDeck((state) => state.setView);
  const hidePanel = useDeck((state) => state.hidePanel);
  const start = useDeck((state) => state.start);
  const startSystem = useSystem((state) => state.start);
  const shellError = useDeck((state) => state.shellError);
  const refreshError = useDeck((state) => state.refreshError);
  const qd = useQdClass();
  const [now, setNow] = useState(() => Date.now());
  const bodyRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void start().catch((error) => {
      const message = `start app: ${String(error)}`;
      console.error(message);
      useDeck.setState({ shellError: message });
    });
    startSystem();
  }, [start, startSystem]);

  // Countdowns in settings move slowly; once a minute keeps them honest.
  useEffect(() => {
    const timer = window.setInterval(() => setNow(Date.now()), 60_000);
    return () => window.clearInterval(timer);
  }, []);

  // Escape steps out of settings first, then out of the popover.
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

  // The window is as tall as what it has to say.
  useEffect(() => {
    const body = bodyRef.current;
    if (body === null) return;
    const observer = new ResizeObserver(() => void reportPanelHeight(body.scrollHeight));
    observer.observe(body);
    void reportPanelHeight(body.scrollHeight);
    return () => observer.disconnect();
  }, [view]);

  return (
    <div className={`${qd} pop`} ref={bodyRef} role="dialog" aria-label={strings.appName}>
      <div className="backdrop" aria-hidden="true" />
      {shellError !== null && (
        <p className="lbl" role="alert" style={{ color: "var(--bad)" }}>
          {strings.shellFailed(shellError)}
        </p>
      )}
      {refreshError !== null && (
        <p className="lbl" role="alert" style={{ color: "var(--bad)" }}>
          {strings.refreshFailed(refreshError)}
        </p>
      )}
      <Popover view={view} now={now} onToggleSettings={() => setView(view === "settings" ? "panel" : "settings")} />
    </div>
  );
}
