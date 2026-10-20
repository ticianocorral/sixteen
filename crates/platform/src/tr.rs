//! Two-language UI strings. The panel draw code says its words through the
//! [`tr!`] macro — `tr!("english text", "texto em português")` — which picks
//! the Brazilian Portuguese side when the process runs in a pt locale and
//! the English side everywhere else. English is the base because Flathub
//! requires apps to be usable in English; Portuguese stays as the language
//! the app speaks by default in a pt-BR system.
//!
//! Selection is decided once per process: `SIXTEEN_LANG` (`pt`/`en`) wins,
//! then `LC_ALL`/`LC_MESSAGES`/`LANG`. There is no `.mo` machinery — the
//! panels draw fixed strings, and both sides are literals at each call
//! site, so a string never falls through "missing key".

use std::sync::OnceLock;

/// The language the UI strings speak this run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    /// English — the base, used when nothing announces Portuguese.
    #[default]
    En,
    /// Brazilian Portuguese — `LC_ALL`/`LC_MESSAGES`/`LANG` starting in `pt`.
    Pt,
}

/// The language for this run, decided once on first call.
pub fn lang() -> Lang {
    static LANG: OnceLock<Lang> = OnceLock::new();
    *LANG.get_or_init(|| detect_from(env_value("SIXTEEN_LANG"), env_value("LC_ALL"), env_value("LC_MESSAGES"), env_value("LANG")))
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Pure detection over injected values so tests can drive it directly.
fn detect_from(
    sixteen_lang: Option<String>,
    lc_all: Option<String>,
    lc_messages: Option<String>,
    lang_env: Option<String>,
) -> Lang {
    // The override speaks the loudest (tests, reports, screenshots).
    if let Some(v) = sixteen_lang {
        match v.trim().to_ascii_lowercase().as_str() {
            "pt" | "pt-br" | "pt_br" => return Lang::Pt,
            "en" | "en-us" | "en_us" => return Lang::En,
            _ => {}
        }
    }
    if [lc_all, lc_messages, lang_env]
        .into_iter()
        .flatten()
        .any(|v| v.trim().to_ascii_lowercase().starts_with("pt"))
    {
        return Lang::Pt;
    }
    Lang::En
}

/// Picks the string for the active language: English first, Portuguese
/// second. Both sides are `'static` literals, so the result is always a
/// `&'static str` — length-driven draw math keeps working on the same call.
#[macro_export]
macro_rules! tr {
    ( $en:expr, $pt:expr $(,)? ) => {
        match $crate::tr::lang() {
            $crate::tr::Lang::Pt => $pt,
            $crate::tr::Lang::En => $en,
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins() {
        assert_eq!(
            detect_from(Some("pt-BR".into()), Some("en_US".into()), None, None),
            Lang::Pt
        );
        assert_eq!(
            detect_from(Some("en".into()), Some("pt_BR".into()), None, None),
            Lang::En
        );
    }

    #[test]
    fn locale_picks_portuguese() {
        assert_eq!(detect_from(None, None, Some("pt_BR.UTF-8".into()), None), Lang::Pt);
        assert_eq!(detect_from(None, None, None, Some("pt".into())), Lang::Pt);
    }

    #[test]
    fn anything_else_is_english() {
        assert_eq!(detect_from(None, Some("en_US.UTF-8".into()), None, None), Lang::En);
        assert_eq!(
            detect_from(None, Some("C.UTF-8".into()), Some("es_ES".into()), None),
            Lang::En
        );
        assert_eq!(detect_from(None, None, None, None), Lang::En);
    }
}
