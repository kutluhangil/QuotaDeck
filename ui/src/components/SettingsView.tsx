import { strings } from "../strings";
import { useDeck } from "../store";
import type { ProviderPlans, Settings, TrayMode } from "../types";
import { StatuslineCard } from "./StatuslineCard";

const trayModes: { mode: TrayMode; label: string; hint: string }[] = [
  { mode: "glyph", label: strings.settings.trayGlyph, hint: strings.settings.trayGlyphHint },
  { mode: "compact", label: strings.settings.trayCompact, hint: strings.settings.trayCompactHint },
  { mode: "strip", label: strings.settings.trayStrip, hint: strings.settings.trayStripHint },
];

const themes: { theme: Settings["theme"]; label: string }[] = [
  { theme: "system", label: strings.settings.themeSystem },
  { theme: "dark", label: strings.settings.themeDark },
  { theme: "light", label: strings.settings.themeLight },
];

/**
 * Tier picker for one provider.
 *
 * "Not set" is a real option and the default. No vendor publishes a numeric ceiling for these
 * subscriptions, so a tier drives an estimate — and an estimate the user never asked for reads
 * as a measurement. Nothing is picked on their behalf.
 */
function PlanGroup({ entry }: { entry: ProviderPlans }) {
  const chosen = useDeck((state) => state.settings.plans[entry.provider]);
  const setPlan = useDeck((state) => state.setPlan);
  const name = strings.provider[entry.provider];

  return (
    <fieldset className="settings__group">
      <legend className="type-label settings__legend">{strings.settings.planTitle(name)}</legend>
      <div className="settings__row">
        <label className="settings__chip">
          <input
            type="radio"
            name={`plan-${entry.provider}`}
            checked={chosen === undefined}
            onChange={() => setPlan(entry.provider, null)}
          />
          <span className="type-body">{strings.settings.planNone}</span>
        </label>
        {entry.plans.map((plan) => (
          <label key={plan.id} className="settings__chip">
            <input
              type="radio"
              name={`plan-${entry.provider}`}
              value={plan.id}
              checked={chosen === plan.id}
              onChange={() => setPlan(entry.provider, plan.id)}
            />
            <span className="type-body">{plan.label}</span>
          </label>
        ))}
      </div>
      <p className="type-caption settings__hint">
        {chosen === undefined
          ? strings.settings.planNoneHint
          : strings.settings.planHint(entry.provider)}
      </p>
    </fieldset>
  );
}

export function SettingsView({ now }: { now: number }) {
  const settings = useDeck((state) => state.settings);
  const setTrayMode = useDeck((state) => state.setTrayMode);
  const setTheme = useDeck((state) => state.setTheme);
  const plans = useDeck((state) => state.plans);

  return (
    <div className="settings">
      <fieldset className="settings__group">
        <legend className="type-label settings__legend">{strings.settings.trayTitle}</legend>
        {trayModes.map(({ mode, label, hint }) => (
          <label key={mode} className="settings__option">
            <input
              type="radio"
              name="tray-mode"
              value={mode}
              checked={settings.trayMode === mode}
              onChange={() => setTrayMode(mode)}
            />
            <span className="settings__option-text">
              <span className="type-body">{label}</span>
              <span className="type-caption settings__hint">{hint}</span>
            </span>
          </label>
        ))}
      </fieldset>

      <fieldset className="settings__group">
        <legend className="type-label settings__legend">{strings.settings.themeTitle}</legend>
        <div className="settings__row">
          {themes.map(({ theme, label }) => (
            <label key={theme} className="settings__chip">
              <input
                type="radio"
                name="theme"
                value={theme}
                checked={settings.theme === theme}
                onChange={() => setTheme(theme)}
              />
              <span className="type-body">{label}</span>
            </label>
          ))}
        </div>
      </fieldset>

      {plans.map((entry) => (
        <PlanGroup key={entry.provider} entry={entry} />
      ))}

      <StatuslineCard now={now} />
    </div>
  );
}
