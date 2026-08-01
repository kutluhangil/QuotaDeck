import { formatClock } from "../format";
import type { Catalogue } from "../i18n";
import { hostPlatform } from "../platform";
import { useDeck, useLocale, useStrings } from "../store";
import { DEFAULT_THRESHOLDS, thresholdsFor } from "../types";
import type { Locale, ProviderId, ProviderPlans, Settings, TrayMode } from "../types";
import { StatuslineCard } from "./StatuslineCard";

/**
 * The option lists are built per render rather than held as module constants: their labels
 * come from the catalogue, and a constant would freeze whichever language was loaded first.
 */
function trayModes(strings: Catalogue): { mode: TrayMode; label: string; hint: string }[] {
  return [
    { mode: "glyph", label: strings.settings.trayGlyph, hint: strings.settings.trayGlyphHint },
    {
      mode: "compact",
      label: strings.settings.trayCompact,
      hint: strings.settings.trayCompactHint,
    },
    { mode: "strip", label: strings.settings.trayStrip, hint: strings.settings.trayStripHint },
  ];
}

function themes(strings: Catalogue): { theme: Settings["theme"]; label: string }[] {
  return [
    { theme: "system", label: strings.settings.themeSystem },
    { theme: "dark", label: strings.settings.themeDark },
    { theme: "light", label: strings.settings.themeLight },
  ];
}

/**
 * Every language names itself, in its own language.
 *
 * Someone who has landed in a language they cannot read has to be able to find their way back
 * out of it, and "Turkish" written in Turkish is the only label that works for both directions.
 */
function locales(strings: Catalogue): { locale: Locale; label: string }[] {
  return [
    { locale: "system", label: strings.settings.languageSystem },
    { locale: "en", label: strings.settings.languageEnglish },
    { locale: "tr", label: strings.settings.languageTurkish },
  ];
}

/**
 * Tier picker for one provider.
 *
 * "Not set" is a real option and the default. No vendor publishes a numeric ceiling for these
 * subscriptions, so a tier drives an estimate — and an estimate the user never asked for reads
 * as a measurement. Nothing is picked on their behalf.
 */
function PlanGroup({ entry }: { entry: ProviderPlans }) {
  const strings = useStrings();
  const chosen = useDeck((state) => state.settings.plans[entry.provider]);
  const setPlan = useDeck((state) => state.setPlan);
  const name = strings.provider[entry.provider];
  const busy = useDeck((state) => state.settingsAction !== null);

  return (
    <fieldset className="settings__group" disabled={busy}>
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

/**
 * Which percentages one provider warns at.
 *
 * Checkboxes rather than radios, and on by default. Unlike a plan — where guessing would put a
 * fabricated percentage in front of the user — a default threshold invents nothing. It only
 * decides when a number the app already has is worth interrupting for, and the operating
 * system asks its own permission before the first interruption.
 */
function AlertGroup({ provider }: { provider: ProviderId }) {
  const strings = useStrings();
  const chosen = useDeck((state) => thresholdsFor(state.settings, provider));
  const toggleThreshold = useDeck((state) => state.toggleThreshold);
  const name = strings.provider[provider];
  const busy = useDeck((state) => state.settingsAction !== null);

  return (
    <fieldset className="settings__group" disabled={busy}>
      <legend className="type-label settings__legend">
        {`${name} · ${strings.settings.alertsTitle}`}
      </legend>
      <div className="settings__row">
        {DEFAULT_THRESHOLDS.map((threshold) => (
          <label key={threshold} className="settings__chip">
            <input
              type="checkbox"
              checked={chosen.includes(threshold)}
              onChange={() => toggleThreshold(provider, threshold)}
            />
            <span className="type-body">{strings.settings.alertsThreshold(threshold)}</span>
          </label>
        ))}
      </div>
      {/* The explanation lives once, above, in the Quiet group. Repeating it under every
          provider would triple the height of this section to say the same thing. */}
      {chosen.length === 0 && (
        <p className="type-caption settings__hint">{strings.settings.alertsOff}</p>
      )}
    </fieldset>
  );
}

/**
 * The grant, and the way to take it back.
 *
 * Hidden entirely where it does not apply: a build that is not sandboxed has nothing to ask
 * for, and a settings group that explains a permission the user never gave is noise.
 */
function AccessGroup() {
  const strings = useStrings();
  const access = useDeck((state) => state.access);
  const requestAccess = useDeck((state) => state.requestAccess);
  const forgetAccess = useDeck((state) => state.forgetAccess);

  if (access === null || !access.required) return null;

  return (
    <fieldset className="settings__group">
      <legend className="type-label settings__legend">{strings.settings.accessTitle}</legend>
      <p className="type-caption settings__hint">
        {access.granted && access.path !== null
          ? strings.settings.accessGranted(access.path)
          : strings.settings.accessMissing}
      </p>
      <div className="settings__row">
        <button
          type="button"
          className="settings__button"
          onClick={() => void requestAccess()}
        >
          <span className="type-body">{strings.settings.accessChoose}</span>
        </button>
        {access.granted && (
          <button type="button" className="settings__button" onClick={() => void forgetAccess()}>
            <span className="type-body">{strings.settings.accessRevoke}</span>
          </button>
        )}
      </div>
      <p className="type-caption settings__hint">{strings.settings.accessHint}</p>
      {access.error !== null && (
        <p className="type-caption settings__error">
          {strings.settings.accessFailed(access.error)}
        </p>
      )}
    </fieldset>
  );
}

/**
 * The sample deck.
 *
 * Two labelled choices rather than a checkbox, because the off state has to say what it shows
 * instead. "Sample: off" leaves the user to work out what they are looking at.
 */
function DemoGroup() {
  const strings = useStrings();
  const demo = useDeck((state) => state.settings.demo);
  const setDemo = useDeck((state) => state.setDemo);
  const busy = useDeck((state) => state.settingsAction !== null);

  return (
    <fieldset className="settings__group" disabled={busy}>
      <legend className="type-label settings__legend">{strings.settings.demoTitle}</legend>
      <div className="settings__row">
        {[
          { on: false, label: strings.settings.demoOff },
          { on: true, label: strings.settings.demoOn },
        ].map(({ on, label }) => (
          <label key={String(on)} className="settings__chip">
            <input
              type="radio"
              name="demo"
              checked={demo === on}
              onChange={() => setDemo(on)}
            />
            <span className="type-body">{label}</span>
          </label>
        ))}
      </div>
      <p className="type-caption settings__hint">{strings.settings.demoHint(hostPlatform())}</p>
    </fieldset>
  );
}

/** The UI reflects the exact Run registration, not Windows' separate Startup Apps override. */
function StartupGroup() {
  const strings = useStrings();
  const startup = useDeck((state) => state.startup);
  const error = useDeck((state) => state.startupError);
  const busy = useDeck((state) => state.startupBusy);
  const setStartup = useDeck((state) => state.setStartup);

  if (!startup.supported) return null;

  return (
    <fieldset className="settings__group" disabled={busy}>
      <legend className="type-label settings__legend">{strings.settings.startupTitle}</legend>
      <div className="settings__row">
        {[
          { enabled: false, label: strings.settings.startupOff },
          { enabled: true, label: strings.settings.startupOn },
        ].map(({ enabled, label }) => (
          <label key={String(enabled)} className="settings__chip">
            <input
              type="radio"
              name="startup"
              checked={startup.enabled === enabled}
              onChange={() => setStartup(enabled)}
            />
            <span className="type-body">{label}</span>
          </label>
        ))}
      </div>
      <p className="type-caption settings__hint">{strings.settings.startupHint}</p>
      {error !== null && (
        <p className="type-caption settings__error" role="alert">
          {strings.settings.startupFailed(error)}
        </p>
      )}
    </fieldset>
  );
}

/** Minutes from now until local midnight, so "until tomorrow" means the viewer's tomorrow. */
function minutesUntilTomorrow(now: number): number {
  const midnight = new Date(now);
  midnight.setHours(24, 0, 0, 0);
  return Math.max(1, Math.round((midnight.getTime() - now) / 60_000));
}

function MuteGroup({ now }: { now: number }) {
  const strings = useStrings();
  const locale = useLocale();
  const mutedUntil = useDeck((state) => state.settings.mutedUntil);
  const setMute = useDeck((state) => state.setMute);
  const until = mutedUntil === null ? null : Date.parse(mutedUntil);
  const muted = until !== null && !Number.isNaN(until) && until > now;
  const busy = useDeck((state) => state.settingsAction !== null);

  return (
    <fieldset className="settings__group" disabled={busy}>
      <legend className="type-label settings__legend">{strings.settings.muteTitle}</legend>
      <div className="settings__row">
        {muted ? (
          <button type="button" className="settings__button" onClick={() => setMute(null)}>
            <span className="type-body">{strings.settings.muteClear}</span>
          </button>
        ) : (
          <>
            <button type="button" className="settings__button" onClick={() => setMute(60)}>
              <span className="type-body">{strings.settings.muteHour}</span>
            </button>
            <button
              type="button"
              className="settings__button"
              onClick={() => setMute(minutesUntilTomorrow(now))}
            >
              <span className="type-body">{strings.settings.muteToday}</span>
            </button>
          </>
        )}
      </div>
      <p className="type-caption settings__hint">
        {muted
          ? strings.settings.mutedUntil(formatClock(mutedUntil, locale) ?? "")
          : strings.settings.alertsHint}
      </p>
    </fieldset>
  );
}

export function SettingsView({ now }: { now: number }) {
  const strings = useStrings();
  const settings = useDeck((state) => state.settings);
  const settingsError = useDeck((state) => state.settingsError);
  const settingsBusy = useDeck((state) => state.settingsAction !== null);
  const setTrayMode = useDeck((state) => state.setTrayMode);
  const setTheme = useDeck((state) => state.setTheme);
  const setLocale = useDeck((state) => state.setLocale);
  const plans = useDeck((state) => state.plans);
  // Selected whole and filtered here: a selector returning a fresh array every call gives
  // zustand a new snapshot on every render and the component never settles.
  const providers = useDeck((state) => state.deck.providers);
  const alerting = providers.filter((snapshot) => snapshot.windows.length > 0);

  return (
    <div className="settings" aria-busy={settingsBusy}>
      {settingsError !== null && (
        <p className="type-caption settings__error" role="alert">
          {strings.settings.settingsFailed(settingsError)}
        </p>
      )}
      <fieldset className="settings__group" disabled={settingsBusy}>
        <legend className="type-label settings__legend">
          {strings.settings.trayTitle(hostPlatform())}
        </legend>
        {trayModes(strings).map(({ mode, label, hint }) => (
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

      <StartupGroup />

      <fieldset className="settings__group" disabled={settingsBusy}>
        <legend className="type-label settings__legend">{strings.settings.themeTitle}</legend>
        <div className="settings__row">
          {themes(strings).map(({ theme, label }) => (
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

      <fieldset className="settings__group" disabled={settingsBusy}>
        <legend className="type-label settings__legend">{strings.settings.languageTitle}</legend>
        <div className="settings__row">
          {locales(strings).map(({ locale, label }) => (
            <label key={locale} className="settings__chip">
              <input
                type="radio"
                name="locale"
                value={locale}
                checked={settings.locale === locale}
                onChange={() => setLocale(locale)}
                // The label is in the language it names, so the control has to say so or a
                // screen reader announces "Türkçe" with English phonemes.
                lang={locale === "system" ? undefined : locale}
              />
              <span className="type-body" lang={locale === "system" ? undefined : locale}>
                {label}
              </span>
            </label>
          ))}
        </div>
        <p className="type-caption settings__hint">{strings.settings.languageHint}</p>
      </fieldset>

      {/* Above the plans: a tier picked against a folder we cannot read produces nothing. */}
      <AccessGroup />
      <DemoGroup />

      {plans.map((entry) => (
        <PlanGroup key={entry.provider} entry={entry} />
      ))}

      <MuteGroup now={now} />
      {/* Only tools that actually reported something. Offering to warn about a limit that
          does not exist would be an empty promise. */}
      {alerting.map((snapshot) => (
        <AlertGroup key={snapshot.id} provider={snapshot.id} />
      ))}

      <StatuslineCard now={now} />
    </div>
  );
}
