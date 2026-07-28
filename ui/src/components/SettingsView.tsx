import { strings } from "../strings";
import { useDeck } from "../store";
import type { Settings, TrayMode } from "../types";

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

export function SettingsView() {
  const settings = useDeck((state) => state.settings);
  const setTrayMode = useDeck((state) => state.setTrayMode);
  const setTheme = useDeck((state) => state.setTheme);

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
    </div>
  );
}
