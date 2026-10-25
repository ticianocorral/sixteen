//! Two-language UI strings. The panel draw code says its words through the
//! [`tr!`] macro — `tr!("english text", "texto em português")` — which picks
//! the side the app is currently speaking. English is the default (Flathub
//! requires apps to be usable in English, and it reads as the base language
//! of every screen); Brazilian Portuguese is the opt-in, chosen on the
//! settings screen's system section and saved to `sixteen.cfg` as
//! `lang = "pt"`.
//!
//! Selection, loudest first: `SIXTEEN_LANG` (`pt`/`en` — the dev/test/
//! screenshot override), then the config pin applied via [`set_lang`] right
//! after config load and live on every settings change, then
//! `LC_ALL`/`LC_MESSAGES`/`LANG` starting in `pt` (belt-and-suspenders for a
//! config that never pinned), then English. There is no `.mo` machinery —
//! the panels draw fixed strings, and both sides are literals at each call
//! site, so a string never falls through "missing key".

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// The language the UI strings speak this run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lang {
    /// English — the default.
    #[default]
    En,
    /// Brazilian Portuguese — the settings opt-in (`lang = "pt"`).
    Pt,
}

/// 0 = no pin (fall through to the locale), 1 = English, 2 = Portuguese.
/// Relaxed is fine: the only writer is the settings screen (and startup,
/// before any draw), and a frame drawing one label in the old language
/// until the next draw is harmless.
static PIN: AtomicU8 = AtomicU8::new(0);

/// `SIXTEEN_LANG`, read once — the dev/test/screenshot override that
/// outranks even the config pin.
static ENV: OnceLock<Option<Lang>> = OnceLock::new();

/// The language for this run: `SIXTEEN_LANG`, then the config pin, then the
/// system locale, then English.
pub fn lang() -> Lang {
    if let Some(l) = *ENV.get_or_init(|| parse_lang(env_value("SIXTEEN_LANG"))) {
        return l;
    }
    match PIN.load(Ordering::Relaxed) {
        1 => return Lang::En,
        2 => return Lang::Pt,
        _ => {}
    }
    *AUTO.get_or_init(|| {
        detect_from(
            None,
            env_value("LC_ALL"),
            env_value("LC_MESSAGES"),
            env_value("LANG"),
        )
    })
}

static AUTO: OnceLock<Lang> = OnceLock::new();

/// Pin the UI language from the config (settings screen / `sixteen.cfg`).
/// `None` drops the pin and goes back to the automatic detection — the
/// fallback when no config exists. Takes effect on the next `tr!` call, so
/// the settings screen itself flips language mid-run with no restart.
pub fn set_lang(lang: Option<Lang>) {
    let v = match lang {
        Some(Lang::En) => 1,
        Some(Lang::Pt) => 2,
        None => 0,
    };
    PIN.store(v, Ordering::Relaxed);
}

fn env_value(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// `SIXTEEN_LANG`'s accepted spellings — `None` means "not set (or junk):
/// fall through".
fn parse_lang(value: Option<String>) -> Option<Lang> {
    match value?.trim().to_ascii_lowercase().as_str() {
        "pt" | "pt-br" | "pt_br" => Some(Lang::Pt),
        "en" | "en-us" | "en_us" => Some(Lang::En),
        _ => None,
    }
}

/// Pure detection over injected values so tests can drive it directly.
fn detect_from(
    sixteen_lang: Option<String>,
    lc_all: Option<String>,
    lc_messages: Option<String>,
    lang_env: Option<String>,
) -> Lang {
    // The override speaks the loudest (tests, reports, screenshots).
    if let Some(l) = parse_lang(sixteen_lang) {
        return l;
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
    fn sixteeen_lang_env_wins() {
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

    #[test]
    fn parse_lang_spellings() {
        assert_eq!(parse_lang(Some("pt".into())), Some(Lang::Pt));
        assert_eq!(parse_lang(Some("PT-BR".into())), Some(Lang::Pt));
        assert_eq!(parse_lang(Some(" en_US ".into())), Some(Lang::En));
        assert_eq!(parse_lang(Some("fr".into())), None);
        assert_eq!(parse_lang(None), None);
    }

    #[test]
    fn config_pin_overrides_and_unpins() {
        // A pt locale underneath: with no pin and no SIXTEEN_LANG, the env
        // vars injected in the other tests don't reach `lang()` (it reads
        // the process env) — so drive the pin directly and just check it
        // wins and unpins.
        set_lang(Some(Lang::Pt));
        assert_eq!(lang(), Lang::Pt);
        set_lang(Some(Lang::En));
        assert_eq!(lang(), Lang::En);
        set_lang(None);
    }
}
