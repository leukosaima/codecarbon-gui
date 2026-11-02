//! Languages of the tray, the macOS app menu and the tooltips (the panel
//! has its own tables in `src/i18n/`). English is the default and the
//! fallback; French is the only other language for now.
//!
//! Errors are not translated here: they reach the panel as stable codes
//! (see [`crate::error::AppError`]) that the front-end translates.

use std::sync::OnceLock;

/// A language of the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    Fr,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::Fr];

    /// The language of a BCP 47 / POSIX locale name, if we have it:
    /// `fr`, `fr-CA`, `fr_FR.UTF-8` → French, `en-GB` → English.
    pub fn parse(locale: &str) -> Option<Lang> {
        let lang = locale
            .split(['-', '_', '.', '@'])
            .next()
            .unwrap_or_default();
        if lang.eq_ignore_ascii_case("fr") {
            Some(Lang::Fr)
        } else if lang.eq_ignore_ascii_case("en") {
            Some(Lang::En)
        } else {
            None
        }
    }

    /// [`Lang::parse`], English for anything else.
    pub fn from_locale(locale: &str) -> Lang {
        Lang::parse(locale).unwrap_or_default()
    }

    /// The first language we have in the user's list of preferred ones
    /// (the panel does the same with [`system_locales`]), else English.
    pub fn detect() -> Lang {
        Lang::detect_from(system_locales())
    }

    pub fn detect_from<S: AsRef<str>>(locales: impl IntoIterator<Item = S>) -> Lang {
        locales
            .into_iter()
            .find_map(|l| Lang::parse(l.as_ref()))
            .unwrap_or_default()
    }

    /// [`Lang::detect`], computed once (the tray refreshes every second).
    pub fn system() -> Lang {
        static SYSTEM: OnceLock<Lang> = OnceLock::new();
        *SYSTEM.get_or_init(Lang::detect)
    }

    /// The `language` setting: `"en"`, `"fr"`, or `"system"` (and anything
    /// unknown) for the system language.
    pub fn from_setting(setting: &str) -> Lang {
        match setting {
            "en" => Lang::En,
            "fr" => Lang::Fr,
            _ => Lang::system(),
        }
    }

    /// Decimal separator of the numbers in the tray: `23.4 W` / `23,4 W`.
    pub fn decimal_separator(self) -> char {
        match self {
            Lang::En => '.',
            Lang::Fr => ',',
        }
    }
}

/// The user's preferred languages, most preferred first, as BCP 47 tags
/// (`["fr-FR", "en-US"]`). Also sent to the panel: WebViews do not always
/// report them (WebKitGTK follows the C locale, macOS the app bundle).
pub fn system_locales() -> Vec<String> {
    sys_locale::get_locales().collect()
}

/// Defines [`Key`], [`Key::ALL`] and the two tables of [`tr`] from one
/// list, so a key without its French text does not compile.
macro_rules! messages {
    ($($key:ident => $en:literal, $fr:literal;)*) => {
        /// A tray / menu string.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Key { $($key),* }

        impl Key {
            pub const ALL: &'static [Key] = &[$(Key::$key),*];
        }

        /// The text of `key` in `lang`. `{}` marks where a value goes (see
        /// [`fill`]).
        pub fn tr(lang: Lang, key: Key) -> &'static str {
            match (lang, key) {
                $(
                    (Lang::En, Key::$key) => $en,
                    (Lang::Fr, Key::$key) => $fr,
                )*
            }
        }
    };
}

// French: "vous" form, non-breaking space (U+00A0) before ":" (and the
// other high punctuation), ellipsis character.
messages! {
    StartMeasuring => "Start measuring", "Démarrer la mesure";
    StopMeasuring => "Stop measuring", "Arrêter la mesure";
    Starting => "Starting…", "Démarrage…";
    Stopping => "Stopping…", "Arrêt…";
    Power => "Power: {}", "Puissance\u{a0}: {}";
    Co2 => "CO₂e: {}", "CO₂e\u{a0}: {}";
    Energy => "Energy: {}", "Énergie\u{a0}: {}";
    ShowApp => "Show CodeCarbon", "Afficher CodeCarbon";
    Settings => "Settings…", "Réglages…";
    OpenDataFolder => "Open data folder", "Ouvrir le dossier des données";
    Quit => "Quit", "Quitter";
    QuitApp => "Quit CodeCarbon", "Quitter CodeCarbon";
    HideApp => "Hide CodeCarbon", "Masquer CodeCarbon";
    TooltipLive => "CodeCarbon — {} · {} CO₂e", "CodeCarbon — {} · {} CO₂e";
    TooltipNotMeasuring => "CodeCarbon — not measuring", "CodeCarbon — aucune mesure en cours";
    TooltipStarting => "CodeCarbon — starting…", "CodeCarbon — démarrage…";
    TooltipStopping => "CodeCarbon — stopping…", "CodeCarbon — arrêt…";
    TooltipUnavailable => "CodeCarbon — measurement engine unavailable",
        "CodeCarbon — moteur de mesure indisponible";
    TooltipLoading => "CodeCarbon — loading…", "CodeCarbon — chargement…";
}

/// Replace the `{}` of `template`, in order, with `values`.
pub fn fill(template: &str, values: &[&str]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut values = values.iter();
    let mut rest = template;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        out.push_str(values.next().copied().unwrap_or_default());
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_key_is_translated() {
        for &key in Key::ALL {
            let en = tr(Lang::En, key);
            let fr = tr(Lang::Fr, key);
            assert!(!en.trim().is_empty(), "{key:?}: empty English text");
            assert!(!fr.trim().is_empty(), "{key:?}: empty French text");
            assert_eq!(
                en.matches("{}").count(),
                fr.matches("{}").count(),
                "{key:?}: placeholders differ"
            );
            // French typography: a non-breaking space before ":".
            assert!(!fr.contains(" :"), "{key:?}: breaking space before ':'");
        }
        // Everything but the brand-only strings is actually translated.
        let same: Vec<_> = Key::ALL
            .iter()
            .filter(|&&k| tr(Lang::En, k) == tr(Lang::Fr, k))
            .collect();
        assert_eq!(same, [&Key::TooltipLive]);
    }

    #[test]
    fn settings_and_locales() {
        assert_eq!(Lang::from_setting("en"), Lang::En);
        assert_eq!(Lang::from_setting("fr"), Lang::Fr);
        assert_eq!(Lang::from_setting("system"), Lang::system());
        assert_eq!(Lang::from_setting("klingon"), Lang::system());
        assert_eq!(Lang::from_locale("fr"), Lang::Fr);
        assert_eq!(Lang::from_locale("fr-CA"), Lang::Fr);
        assert_eq!(Lang::from_locale("fr_FR.UTF-8"), Lang::Fr);
        assert_eq!(Lang::from_locale("FR_be"), Lang::Fr);
        assert_eq!(Lang::from_locale("en-GB"), Lang::En);
        assert_eq!(Lang::from_locale("de-DE"), Lang::En);
        assert_eq!(Lang::from_locale("fy"), Lang::En);
        assert_eq!(Lang::from_locale(""), Lang::En);
        assert_eq!(Lang::from_locale("C"), Lang::En);
        assert_eq!(Lang::parse("de"), None);
        assert_eq!(Lang::detect_from(["de-DE", "fr-BE"]), Lang::Fr);
        assert_eq!(Lang::detect_from(["en-GB", "fr-FR"]), Lang::En);
        assert_eq!(Lang::detect_from(["de-DE"]), Lang::En);
        assert_eq!(Lang::detect_from(Vec::<String>::new()), Lang::En);
    }

    #[test]
    fn fills_placeholders() {
        assert_eq!(fill(tr(Lang::En, Key::Power), &["23.4 W"]), "Power: 23.4 W");
        assert_eq!(
            fill(tr(Lang::Fr, Key::Power), &["23,4 W"]),
            "Puissance\u{a0}: 23,4 W"
        );
        assert_eq!(fill("{} · {}", &["a"]), "a · ");
        assert_eq!(fill("no value", &["a"]), "no value");
    }
}
