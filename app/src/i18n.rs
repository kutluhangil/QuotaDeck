//! The backend's own copy of the catalogue.
//!
//! The panel has one too, and it is not shared. Two surfaces speak from this process without a
//! webview in front of them: the notifications, raised from the read loop whether or not the
//! panel has ever been opened, and the tray menu, which is the only way to quit the app.
//! Sending either through the frontend would mean a suspended webview can silence a warning.
//!
//! Everything here is copy. Numbers and clock times are formatted by `chrono` from the
//! system's own zone, exactly as the panel leaves them to `Intl`.

use quotadeck_core::types::{ProviderId, QuotaWindow, WindowKind};
use serde::{Deserialize, Serialize};

/// What the user picked. `System` reads the choice from the operating system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Locale {
    #[default]
    System,
    En,
    Tr,
    De,
    Es,
}

/// A language with a complete catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En,
    Tr,
    De,
    Es,
}

impl Locale {
    /// The language to actually speak.
    pub fn language(self) -> Language {
        match self {
            Locale::En => Language::En,
            Locale::Tr => Language::Tr,
            Locale::De => Language::De,
            Locale::Es => Language::Es,
            Locale::System => Language::from_system(),
        }
    }
}

impl Language {
    /// Every language with a complete catalogue. Tests iterate this rather than a hand-written
    /// list, so adding a language cannot leave a surface untranslated and unnoticed.
    pub const ALL: [Language; 4] = [Language::En, Language::Tr, Language::De, Language::Es];

    /// Ask macOS and Windows for their UI locale. Other Unix sessions use POSIX variables.
    fn from_system() -> Language {
        for locale in native_locales() {
            if let Some(language) = Language::from_tag(&locale) {
                return language;
            }
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            for key in ["LC_ALL", "LC_MESSAGES", "LANG"] {
                let Ok(value) = std::env::var(key) else {
                    continue;
                };
                if let Some(language) = Language::from_tag(&value) {
                    return language;
                }
            }
        }
        Language::En
    }

    /// Match the primary subtag of a tag like `tr_TR.UTF-8` or `tr-TR`.
    fn from_tag(tag: &str) -> Option<Language> {
        let primary = tag
            .split(['_', '-', '.'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match primary.as_str() {
            "en" => Some(Language::En),
            "tr" => Some(Language::Tr),
            "de" => Some(Language::De),
            "es" => Some(Language::Es),
            _ => None,
        }
    }

    /// Tray menu: show the panel.
    pub fn tray_open(self) -> &'static str {
        match self {
            Language::En => "Open Quota Deck",
            Language::Tr => "Quota Deck'i aç",
            Language::De => "Quota Deck öffnen",
            Language::Es => "Abrir Quota Deck",
        }
    }

    /// Tray menu: quit. With the accessory activation policy there is no dock icon, so this
    /// is the only way out of the app and it has to be readable.
    pub fn tray_quit(self) -> &'static str {
        match self {
            Language::En => "Quit",
            Language::Tr => "Çık",
            Language::De => "Beenden",
            Language::Es => "Salir",
        }
    }

    pub fn tray_dashboard(self) -> &'static str {
        match self {
            Language::En => "Dashboard",
            Language::Tr => "Pano",
            Language::De => "Übersicht",
            Language::Es => "Panel",
        }
    }

    pub fn tray_refresh(self) -> &'static str {
        match self {
            Language::En => "Refresh",
            Language::Tr => "Yenile",
            Language::De => "Aktualisieren",
            Language::Es => "Actualizar",
        }
    }

    pub fn tray_stale(self) -> &'static str {
        match self {
            Language::En => "Stale",
            Language::Tr => "Eski veri",
            Language::De => "Veraltet",
            Language::Es => "Desactualizado",
        }
    }

    pub fn tray_rebuilding(self) -> &'static str {
        match self {
            Language::En => "Rebuilding",
            Language::Tr => "Yeniden oluşturuluyor",
            Language::De => "Wird neu aufgebaut",
            Language::Es => "Reconstruyendo",
        }
    }

    pub fn tray_error(self) -> &'static str {
        match self {
            Language::En => "Error",
            Language::Tr => "Hata",
            Language::De => "Fehler",
            Language::Es => "Error",
        }
    }

    pub fn tray_unavailable(self) -> &'static str {
        match self {
            Language::En => "Unavailable",
            Language::Tr => "Kullanılamıyor",
            Language::De => "Nicht verfügbar",
            Language::Es => "No disponible",
        }
    }

    /// The sentence inside `NSOpenPanel`.
    ///
    /// Written in Rust because the panel that asks belongs to AppKit, not to the webview. It
    /// says what is being asked for and what is not: the answer to "why does this want my whole
    /// home folder" has to be visible at the moment the question is asked.
    pub fn folder_message(self) -> &'static str {
        match self {
            Language::En => "Choose your home folder so Quota Deck can read the session logs your coding tools already write. It is read-only, and nothing leaves this device.",
            Language::Tr => "Quota Deck'in kodlama araçlarının zaten yazdığı oturum günlüklerini okuyabilmesi için ev klasörünü seç. Yalnızca okunur ve hiçbir şey bu cihazdan çıkmaz.",
            Language::De => "Wähle deinen Persönlichen Ordner, damit Quota Deck die Sitzungsprotokolle lesen kann, die deine Coding-Werkzeuge ohnehin schreiben. Nur Lesezugriff, und nichts verlässt dieses Gerät.",
            Language::Es => "Elige tu carpeta personal para que Quota Deck pueda leer los registros de sesión que tus herramientas de programación ya escriben. Es solo de lectura y nada sale de este dispositivo.",
        }
    }

    /// The panel's confirm button. A verb for what happens, not "OK".
    pub fn folder_prompt(self) -> &'static str {
        match self {
            Language::En => "Grant read access",
            Language::Tr => "Okuma izni ver",
            Language::De => "Lesezugriff erteilen",
            Language::Es => "Conceder acceso de lectura",
        }
    }

    /// How a window is named in a notification. Classified by the reported duration, never by
    /// the provider's slot name.
    pub fn window_label(self, window: &QuotaWindow) -> String {
        match (self, window.kind) {
            (Language::En, WindowKind::Session) => "session".into(),
            (Language::En, WindowKind::Weekly) => "weekly".into(),
            (Language::En, WindowKind::Monthly) => "monthly".into(),
            (Language::En, WindowKind::Other) => format!("{}-minute", window.window_minutes),
            (Language::Tr, WindowKind::Session) => "oturum".into(),
            (Language::Tr, WindowKind::Weekly) => "haftalık".into(),
            (Language::Tr, WindowKind::Monthly) => "aylık".into(),
            (Language::Tr, WindowKind::Other) => format!("{} dakikalık", window.window_minutes),
            (Language::De, WindowKind::Session) => "Sitzung".into(),
            (Language::De, WindowKind::Weekly) => "Woche".into(),
            (Language::De, WindowKind::Monthly) => "Monat".into(),
            (Language::De, WindowKind::Other) => format!("{}-Minuten", window.window_minutes),
            (Language::Es, WindowKind::Session) => "sesión".into(),
            (Language::Es, WindowKind::Weekly) => "semanal".into(),
            (Language::Es, WindowKind::Monthly) => "mensual".into(),
            (Language::Es, WindowKind::Other) => format!("de {} minutos", window.window_minutes),
        }
    }

    /// The notification body when the provider said when the window frees up.
    pub fn alert_body_with_reset(self, window: &str, percent: f32, clock: &str) -> String {
        match self {
            Language::En => {
                format!("The {window} limit is {percent:.0}% used. It resets at {clock}.")
            }
            Language::Tr => {
                format!("{window} limiti %{percent:.0} doldu. {clock} sıfırlanıyor.")
            }
            // German and Spanish both put a space before the sign, and both write the noun
            // ahead of it: "Das Wochen-Limit", "El límite semanal".
            Language::De => format!(
                "Das {window}-Limit ist zu {percent:.0} % ausgeschöpft. Es wird um {clock} zurückgesetzt."
            ),
            Language::Es => format!(
                "El límite {window} está al {percent:.0} %. Se restablece a las {clock}."
            ),
        }
    }

    /// The same, for a provider that reports no reset instant.
    pub fn alert_body(self, window: &str, percent: f32) -> String {
        match self {
            Language::En => format!("The {window} limit is {percent:.0}% used."),
            Language::Tr => format!("{window} limiti %{percent:.0} doldu."),
            Language::De => format!("Das {window}-Limit ist zu {percent:.0} % ausgeschöpft."),
            Language::Es => format!("El límite {window} está al {percent:.0} %."),
        }
    }

    pub fn pace_body(self, window: &str, clock: &str) -> String {
        match self {
            Language::En => format!(
                "At the current pace, the {window} limit is projected to be exhausted at {clock}."
            ),
            Language::Tr => {
                format!("Mevcut hızla {window} limitinin {clock} itibarıyla tükenmesi öngörülüyor.")
            }
            Language::De => format!(
                "Beim aktuellen Tempo ist das {window}-Limit voraussichtlich um {clock} aufgebraucht."
            ),
            Language::Es => format!(
                "Al ritmo actual, se prevé que el límite {window} se agote a las {clock}."
            ),
        }
    }

    /// The notification for an hour of agent spend far past this user's usual.
    ///
    /// Says what it is measured against, because the whole claim rests on that: "a lot" is not
    /// a fact, "eight times your usual hour" is. The figure is the user's own median hour.
    pub fn burst_body(self, factor: f32, tokens: &str) -> String {
        match self {
            Language::En => format!(
                "Agents have spent {tokens} tokens in the last hour — about {factor:.0}× a usual hour. Check nothing is running unattended."
            ),
            Language::Tr => format!(
                "Agent'lar son bir saatte {tokens} token harcadı — olağan bir saatin yaklaşık {factor:.0} katı. Başıboş çalışan bir şey var mı, bak."
            ),
            Language::De => format!(
                "Agenten haben in der letzten Stunde {tokens} Token verbraucht — etwa das {factor:.0}-Fache einer üblichen Stunde. Prüfe, ob etwas unbeaufsichtigt läuft."
            ),
            Language::Es => format!(
                "Los agentes han gastado {tokens} tokens en la última hora, unas {factor:.0} veces una hora normal. Comprueba que no haya nada ejecutándose sin supervisión."
            ),
        }
    }

    /// Thousands-separated token count for the notification body.
    ///
    /// The panel leaves this to `Intl`; nothing equivalent exists on this side, and a raw
    /// nine-digit run is unreadable in a notification.
    pub fn group_digits(self, value: u64) -> String {
        let digits = value.to_string();
        // The thousands separator is a property of the language the sentence is written in,
        // and this side has no `Intl` to ask.
        let separator = match self {
            Language::En => ',',
            Language::Tr | Language::De | Language::Es => '.',
        };
        let mut out = String::with_capacity(digits.len() + digits.len() / 3);
        for (i, ch) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i) % 3 == 0 {
                out.push(separator);
            }
            out.push(ch);
        }
        out
    }
}

#[cfg(target_os = "macos")]
fn native_locales() -> Vec<String> {
    use objc2_foundation::NSLocale;

    NSLocale::preferredLanguages()
        .iter()
        .map(|language| language.to_string())
        .collect()
}

#[cfg(target_os = "windows")]
fn native_locales() -> Vec<String> {
    const LOCALE_NAME_MAX_LENGTH: usize = 85;

    #[link(name = "kernel32")]
    extern "system" {
        fn GetUserDefaultLocaleName(locale_name: *mut u16, locale_name_count: i32) -> i32;
    }

    let mut locale = [0_u16; LOCALE_NAME_MAX_LENGTH];
    // SAFETY: `locale` is writable for the exact capacity passed to the Windows API.
    let length =
        unsafe { GetUserDefaultLocaleName(locale.as_mut_ptr(), LOCALE_NAME_MAX_LENGTH as i32) };
    if length <= 1 {
        return Vec::new();
    }
    vec![String::from_utf16_lossy(&locale[..length as usize - 1])]
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn native_locales() -> Vec<String> {
    Vec::new()
}

/// The provider's own untranslated name. Falls back to the stable key, which is never pretty
/// but is never wrong either.
///
/// Product names are not translated in any language.
pub fn provider_name(provider: ProviderId) -> String {
    quotadeck_providers::by_key(provider.key())
        .map(|p| p.display_name().to_string())
        .unwrap_or_else(|| provider.key().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use quotadeck_core::types::Confidence;

    fn window(kind: WindowKind, minutes: u32) -> QuotaWindow {
        QuotaWindow {
            limit_id: "codex".into(),
            kind,
            window_minutes: minutes,
            used_percent: Some(72.0),
            resets_at: None,
            confidence: Confidence::Measured {
                reported_at: Utc::now(),
            },
        }
    }

    #[test]
    fn an_explicit_pick_never_consults_the_environment() {
        assert_eq!(Locale::Tr.language(), Language::Tr);
        assert_eq!(Locale::En.language(), Language::En);
        assert_eq!(Locale::De.language(), Language::De);
        assert_eq!(Locale::Es.language(), Language::Es);
    }

    #[test]
    fn a_posix_locale_tag_resolves_on_its_primary_subtag() {
        assert_eq!(Language::from_tag("tr_TR.UTF-8"), Some(Language::Tr));
        assert_eq!(Language::from_tag("tr"), Some(Language::Tr));
        assert_eq!(Language::from_tag("en-GB"), Some(Language::En));
        // A language with no catalogue is not a match, so the search moves on to the next
        // variable rather than settling for a half-translated panel.
        assert_eq!(Language::from_tag("de_DE.UTF-8"), Some(Language::De));
        assert_eq!(Language::from_tag("es-419"), Some(Language::Es));
        // A language with no catalogue is not a match, so the search moves on to the next
        // variable rather than settling for a half-translated panel.
        assert_eq!(Language::from_tag("fr_FR.UTF-8"), None);
        assert_eq!(Language::from_tag("C"), None);
    }

    #[test]
    fn every_window_shape_has_a_name_in_both_languages() {
        for language in Language::ALL {
            for kind in [
                WindowKind::Session,
                WindowKind::Weekly,
                WindowKind::Monthly,
                WindowKind::Other,
            ] {
                let label = language.window_label(&window(kind, 90));
                assert!(!label.is_empty(), "{language:?} has no name for {kind:?}");
            }
        }
        assert_eq!(
            Language::Tr.window_label(&window(WindowKind::Other, 90)),
            "90 dakikalık"
        );
    }

    #[test]
    fn the_folder_request_explains_itself_in_both_languages() {
        // This sentence is the entire answer to "why does this want my home folder", shown at
        // the moment the system asks. An empty one is a grant nobody should make.
        for language in Language::ALL {
            assert!(language.folder_message().len() > 60, "{language:?}");
            assert!(!language.folder_prompt().is_empty(), "{language:?}");
        }
    }

    #[test]
    fn the_percent_sign_sits_where_the_language_puts_it() {
        // Turkish writes the sign in front of the number. Concatenating it after would be a
        // sentence no Turkish reader writes.
        assert!(Language::Tr.alert_body("haftalık", 72.0).contains("%72"));
        assert!(Language::En.alert_body("weekly", 72.0).contains("72%"));
        // German and Spanish keep a space before the sign.
        assert!(Language::De.alert_body("Wochen", 72.0).contains("72 %"));
        assert!(Language::Es.alert_body("semanal", 72.0).contains("72 %"));
        // Every language says something, in its own shape.
        for language in Language::ALL {
            assert!(!language.alert_body("x", 72.0).is_empty(), "{language:?}");
            assert!(!language.pace_body("x", "10:00").is_empty(), "{language:?}");
            assert!(
                !language.burst_body(8.0, "1.000").is_empty(),
                "{language:?}"
            );
        }
    }
}
