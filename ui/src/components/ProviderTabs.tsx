import { useRef, type KeyboardEvent } from "react";

import { levelFor } from "../format";
import { identityHue } from "../identity";
import { useStrings } from "../store";
import { worstWindow, type ProviderInstanceId, type ProviderSnapshot } from "../types";

export type TabKey = ProviderInstanceId | "all";

/** The first letter of the name, for the mark. A brand logo is not ours to draw. */
function monogram(name: string): string {
  return name.trim().charAt(0).toUpperCase();
}

/**
 * One tab per tool along the top of the panel, with Overview first.
 *
 * Each tab carries a hairline meter of the tool's fullest window, so the strip answers "is
 * anything close?" before a tab is opened. A tool with no limit gets an empty track rather
 * than a guessed one.
 *
 * A real tablist: arrow keys move between tabs and only the selected one is in the tab order,
 * so a keyboard user crosses the strip in one stop.
 */
export function ProviderTabs({
  snapshots,
  selected,
  onSelect,
  panelId,
}: {
  snapshots: ProviderSnapshot[];
  selected: TabKey;
  onSelect: (key: TabKey) => void;
  /** The element the tabs control. */
  panelId: string;
}) {
  const strings = useStrings();
  const refs = useRef(new Map<TabKey, HTMLButtonElement>());
  const keys: TabKey[] = ["all", ...snapshots.map((snapshot) => snapshot.instance)];

  function onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const step =
      event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : event.key === "Home" ? 0 : null;
    const last = event.key === "End";
    if (step === null && !last) return;
    event.preventDefault();
    const index = keys.indexOf(selected);
    const next = last
      ? keys.length - 1
      : step === 0
        ? 0
        : (index + (step ?? 0) + keys.length) % keys.length;
    const key = keys[next];
    if (key === undefined) return;
    onSelect(key);
    refs.current.get(key)?.focus();
  }

  return (
    <div className="tabs" role="tablist" aria-label={strings.tabs.label} onKeyDown={onKeyDown}>
      <button
        type="button"
        role="tab"
        className="tab"
        aria-selected={selected === "all"}
        aria-controls={panelId}
        tabIndex={selected === "all" ? 0 : -1}
        ref={(node) => {
          if (node) refs.current.set("all", node);
        }}
        onClick={() => onSelect("all")}
      >
        <span className="tab__mark tab__mark--overview" aria-hidden="true" />
        <span className="type-caption tab__name">{strings.tabs.overview}</span>
        <span className="tab__meter" aria-hidden="true" />
      </button>
      {snapshots.map((snapshot) => {
        const name = snapshot.label ?? strings.provider[snapshot.id];
        const percent = worstWindow(snapshot.windows)?.usedPercent ?? null;
        const isSelected = selected === snapshot.instance;
        return (
          <button
            key={snapshot.instance}
            type="button"
            role="tab"
            className="tab"
            aria-selected={isSelected}
            aria-controls={panelId}
            tabIndex={isSelected ? 0 : -1}
            ref={(node) => {
              if (node) refs.current.set(snapshot.instance, node);
            }}
            onClick={() => onSelect(snapshot.instance)}
          >
            <span className="tab__mark" data-hue={identityHue(snapshot.instance)} aria-hidden="true">
              {monogram(name)}
            </span>
            <span className="type-caption tab__name">{name}</span>
            <span className="tab__meter" aria-hidden="true">
              {percent !== null && (
                <span
                  className="tab__meter-fill"
                  data-level={levelFor(percent)}
                  style={{ inlineSize: `${Math.max(2, Math.min(100, percent))}%` }}
                />
              )}
            </span>
          </button>
        );
      })}
    </div>
  );
}
