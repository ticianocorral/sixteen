//! `sixteen.cfg`: run-ahead default, start-fullscreen, and keyboard binds —
//! the same file the in-app settings screen (`O` on the shelf) edits and
//! saves. TOML syntax under the hood (same as before); only the name and
//! location changed — it now sits in the app's root (`sixteen_app::dirs`,
//! next to the executable on Windows/Linux, `~/Documents/SixteeN` on
//! macOS), not under `~/.config` (plan: app portátil).
//!
//! Lookup order: `--config PATH`, then `$SIXTEEN_CONFIG`, then
//! `sixteen_app::dirs::config_path()`. When the last one is used and is
//! missing, a default file is written so the user has something to edit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sixteen_platform::{KeyMap, PadButton, PadMap};

pub struct Config {
    pub fullscreen: bool,
    /// UI language (plan revision: "traduzir o app 100% para inglês, pt-BR
    /// como opção na configuração") — English is the default, no matter the
    /// system locale; `pt` opts the UI into Brazilian Portuguese. Chosen on
    /// the settings screen's system section ("Language"), saved here, and
    /// applied at startup plus live on every toggle.
    pub lang: LangOpt,
    /// Check GitHub for a newer app release, and the buildbot for a fresher
    /// snes9x core, once at startup (plan revision) — a settings-screen
    /// toggle, on by default. Either check that finds something newer shows
    /// a one-time notice on the idle screen; a network failure just means no
    /// notice, never an error.
    pub check_updates_on_start: bool,
    /// Ambient white-noise hiss while the TV shows static (idle, power-off)
    /// — plan revision: "som de chiado de tv fora do ar ... colocar na
    /// configuração para tocar ou não. por padrão vem desligado".
    pub hiss_on_static: bool,
    /// O móvel da estante (plan revision: "poder escolher um tema de
    /// expositor, igual era em locadoras de games") — a estante de madeira
    /// (padrão) ou o expositor de metal da locadora. Linha "Estante" na
    /// seção de vídeo das configurações.
    pub shelf_theme: ShelfTheme,
    /// RetroAchievements account (plan: `docs/plano-retroachievements.md`,
    /// fase 1) — the username and the web-API token the user generates on
    /// retroachievements.org (Settings → Web API). Both empty (the default)
    /// keeps every RA feature off: no network call, no UI beyond the
    /// settings rows themselves.
    pub ra_user: String,
    pub ra_token: String,
    /// Hardcore mode for achievements (default on once an account exists) —
    /// cheats/savestates/run-ahead lock during play. Meaningful from fase 3
    /// on; persisted now so the setting doesn't move later.
    pub ra_hardcore: bool,
    /// Connect login token (plan revision — a web API key não serve mais a
    /// lógica das conquistas: o campo `MemAddr` da rota velha chega
    /// hashado). Nasce do `r=login2` com usuário+senha na tela de
    /// configurações; a senha em si nunca é gravada. `None` = conta ainda
    /// não conectada — a estante não identifica nada até conectar.
    pub ra_connect: Option<String>,
    pub keymap: KeyMap,
    /// Gamepad-button layout (`[gamepad]`): which SDL gamepad button drives
    /// each SNES button. Rebindable in the settings screen alongside the
    /// keyboard — clones and native-driver paths can land a press in the
    /// wrong slot, so the layout is the user's to fix.
    pub padmap: PadMap,
    /// centavos, moeda, mínimo de jogos e o dobradouro de fim de semana.
    /// Where it was read from (or freshly written), for logging and for
    /// `save()`.
    pub source: Option<PathBuf>,
}

/// The settings-screen language choice — English is the default (the app
/// reads as English-first); `pt` opts the UI into Brazilian Portuguese.
/// Serialized as the plain word in `sixteen.cfg`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LangOpt {
    #[default]
    En,
    Pt,
}

impl LangOpt {
    pub fn as_str(self) -> &'static str {
        match self {
            LangOpt::En => "en",
            LangOpt::Pt => "pt",
        }
    }

    /// Click / left-right on the settings row: english ↔ português.
    pub fn cycle(self) -> Self {
        match self {
            LangOpt::En => LangOpt::Pt,
            LangOpt::Pt => LangOpt::En,
        }
    }

    /// The string picker equivalent, for `tr::set_lang` at startup and on
    /// every settings toggle.
    pub fn tr_lang(self) -> sixteen_platform::tr::Lang {
        match self {
            LangOpt::En => sixteen_platform::tr::Lang::En,
            LangOpt::Pt => sixteen_platform::tr::Lang::Pt,
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "en" | "en-us" | "en_us" => Some(LangOpt::En),
            // "auto" was the locale-driven default of the brief middle
            // revision — English is the default now, so it folds into `en`.
            "auto" | "" => Some(LangOpt::En),
            "pt" | "pt-br" | "pt_br" => Some(LangOpt::Pt),
            _ => None,
        }
    }
}

/// O tema do móvel da estante — gravado como a palavra no `sixteen.cfg`
/// (`shelf_theme = "wood"` / `"metal"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShelfTheme {
    /// A estante de madeira: fundo com veios, prateleiras de tábua e
    /// suportes de acrílico.
    #[default]
    Wood,
    /// O expositor de metal da locadora: chapa perfurada no fundo,
    /// montantes com cremalheira, prateleiras de aço com canaleta de preço
    /// e um arame cromado segurando as caixas.
    Metal,
}

impl ShelfTheme {
    pub fn as_str(self) -> &'static str {
        match self {
            ShelfTheme::Wood => "wood",
            ShelfTheme::Metal => "metal",
        }
    }

    /// Click / left-right on the settings row: madeira ↔ metal.
    pub fn cycle(self) -> Self {
        match self {
            ShelfTheme::Wood => ShelfTheme::Metal,
            ShelfTheme::Metal => ShelfTheme::Wood,
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "wood" | "madeira" | "" => Some(ShelfTheme::Wood),
            "metal" => Some(ShelfTheme::Metal),
            _ => None,
        }
    }
}

#[derive(Deserialize, Default)]
struct Raw {
    #[serde(default)]
    fullscreen: Option<bool>,
    #[serde(default)]
    lang: Option<String>,
    #[serde(default)]
    check_updates_on_start: Option<bool>,
    #[serde(default)]
    hiss_on_static: Option<bool>,
    #[serde(default)]
    shelf_theme: Option<String>,
    #[serde(default)]
    ra_user: Option<String>,
    #[serde(default)]
    ra_token: Option<String>,
    #[serde(default)]
    ra_hardcore: Option<bool>,
    #[serde(default)]
    ra_connect: Option<String>,
    #[serde(default)]
    keyboard: BTreeMap<String, String>,
    #[serde(default)]
    gamepad: BTreeMap<String, String>,
}

impl Config {
    pub fn load(explicit: Option<&Path>) -> Result<Self> {
        let (path, required) = match explicit {
            Some(p) => (Some(p.to_path_buf()), true),
            None => match std::env::var_os("SIXTEEN_CONFIG") {
                Some(p) => (Some(PathBuf::from(p)), true),
                None => (Some(crate::dirs::config_path()), false),
            },
        };

        let mut cfg = Config {
            // Plan revision: "sempre abrir em fullscreen como padrao" — a
            // fresh install (no `sixteen.cfg` yet) starts fullscreen; the
            // settings screen's own toggle still turns it off from there.
            fullscreen: true,
            lang: LangOpt::En,
            check_updates_on_start: true,
            hiss_on_static: false,
            shelf_theme: ShelfTheme::Wood,
            ra_user: String::new(),
            ra_token: String::new(),
            ra_hardcore: true,
            ra_connect: None,
            keymap: KeyMap::defaults(),
            padmap: PadMap::defaults(),
            source: path.clone(),
        };

        let Some(path) = path else {
            return Ok(cfg);
        };

        match std::fs::read_to_string(&path) {
            Ok(text) => {
                let raw: Raw =
                    toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
                cfg.apply(raw)?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !required => {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let _ = write_config_private(&path, &cfg.to_toml());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                bail!("config not found: {}", path.display());
            }
            Err(e) => {
                return Err(e).with_context(|| format!("reading {}", path.display()));
            }
        }
        Ok(cfg)
    }

    fn apply(&mut self, raw: Raw) -> Result<()> {
        if let Some(f) = raw.fullscreen {
            self.fullscreen = f;
        }
        if let Some(l) = raw.lang {
            match LangOpt::parse(&l) {
                Some(v) => self.lang = v,
                None => log::warn!(
                    "[config] lang = {l:?}: not a language (want auto|en|pt), keeping {}",
                    self.lang.as_str()
                ),
            }
        }
        if let Some(c) = raw.check_updates_on_start {
            self.check_updates_on_start = c;
        }
        if let Some(h) = raw.hiss_on_static {
            self.hiss_on_static = h;
        }
        if let Some(t) = raw.shelf_theme {
            match ShelfTheme::parse(&t) {
                Some(v) => self.shelf_theme = v,
                None => log::warn!(
                    "[config] shelf_theme = {t:?}: not a theme (want wood|metal), keeping {}",
                    self.shelf_theme.as_str()
                ),
            }
        }
        if let Some(u) = raw.ra_user {
            self.ra_user = u;
        }
        if let Some(t) = raw.ra_token {
            self.ra_token = t;
        }
        if let Some(h) = raw.ra_hardcore {
            self.ra_hardcore = h;
        }
        // Connect token só chega pelo login da tela de configurações — mas
        // respeita um que tenha sido colado à mão no arquivo.
        if let Some(c) = raw.ra_connect {
            self.ra_connect = (!c.is_empty()).then_some(c);
        }
        for (action, key) in &raw.keyboard {
            let Ok(b) = action.parse::<PadButton>() else {
                // Not a gameplay button — either a typo, or (most likely for
                // anyone upgrading) one of the console/UI commands this
                // section used to rebind (eject, pause, save_state, ...)
                // before those became mouse/gamepad-only. Either way,
                // nothing left to bind it to; warn and move on rather than
                // refusing to start over a stale config line.
                log::warn!("[keyboard] {action}: not a bindable action any more, ignoring");
                continue;
            };
            self.keymap
                .bind_pad(key, b)
                .map_err(anyhow::Error::msg)
                .with_context(|| format!("[keyboard] {action}"))?;
        }
        for (action, button) in &raw.gamepad {
            let Ok(b) = action.parse::<PadButton>() else {
                log::warn!("[gamepad] {action}: not a bindable action, ignoring");
                continue;
            };
            if button.is_empty() {
                // Written for an action whose button another action took —
                // nothing to bind here (the empty state re-emerges from the
                // other action's bind dropping the default).
                continue;
            }
            self.padmap
                .bind(button, b)
                .map_err(anyhow::Error::msg)
                .with_context(|| format!("[gamepad] {action}"))?;
        }
        Ok(())
    }

    /// Serialize the live config back to the `sixteen.cfg` text format.
    pub fn to_toml(&self) -> String {
        let mut s = String::from(
            "# SixteeN configuration.\n\
             # Edited by the in-app settings screen (O on the shelf) — hand edits\n\
             # survive a save, but comments outside a value don't.\n\
             # fullscreen: start in fullscreen.\n\
             # lang: UI language — en (default) or pt (Brazilian Portuguese).\n\
             # check_updates_on_start: look for a newer release/snes9x core at launch.\n\
             # hiss_on_static: white-noise hiss while the TV shows static (off by default).\n\
             # shelf_theme: the shelf furniture — wood (default) or metal (rental-store rack).\n\
             # RetroAchievements: ra_user/ra_token from retroachievements.org\n\
             # (Settings -> Web API); empty = the whole feature stays off.\n\
             # ra_hardcore: no cheats/savestates while earning achievements.\n\
             # [keyboard]: action = \"SDL key name\" (e.g. \"Left Shift\", \"F2\", \"]\").\n\
             # [gamepad]: action = \"SDL gamepad button name\" (e.g. \"south\",\n\
             # \"dpup\", \"leftshoulder\") — which pad button drives each action.\n\n",
        );
        s.push_str(&format!("fullscreen = {}\n", self.fullscreen));
        s.push_str(&format!("lang = {:?}\n", self.lang.as_str()));
        s.push_str(&format!(
            "check_updates_on_start = {}\n",
            self.check_updates_on_start
        ));
        s.push_str(&format!("hiss_on_static = {}\n", self.hiss_on_static));
        s.push_str(&format!(
            "shelf_theme = {:?}\n\n",
            self.shelf_theme.as_str()
        ));
        s.push_str("# RetroAchievements\n");
        s.push_str(&format!("ra_user = {:?}\n", self.ra_user));
        s.push_str(&format!("ra_token = {:?}\n", self.ra_token));
        s.push_str(&format!("ra_hardcore = {}\n\n", self.ra_hardcore));
        if let Some(connect) = &self.ra_connect {
            s.push_str(&format!("ra_connect = {connect:?}\n\n"));
        }
        s.push_str("[keyboard]\n");
        for (action, key) in self.keymap.describe() {
            s.push_str(&format!("{action} = {key:?}\n"));
        }
        s.push_str("\n[gamepad]\n");
        for (action, button) in self.padmap.describe() {
            s.push_str(&format!("{action} = {button:?}\n"));
        }
        s
    }

    /// Write the live config back to `source`. Errors if this `Config` was
    /// never tied to a path.
    pub fn save(&self) -> Result<()> {
        let path = self
            .source
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("no config path to save to"))?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        write_config_private(path, &self.to_toml())
            .with_context(|| format!("writing {}", path.display()))
    }
}

/// Grava o config com permissão 0600 no Unix — ele carrega os tokens do
/// RetroAchievements — e de forma atômica (tmp + rename): um travamento no
/// meio não deixa o cfg pela metade.
pub(crate) fn write_config_private(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    sixteen_domain::fsutil::write_atomic_private(path, contents)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn defaults() -> Config {
        Config {
            fullscreen: false,
            lang: LangOpt::En,
            check_updates_on_start: true,
            hiss_on_static: false,
            shelf_theme: ShelfTheme::Wood,
            ra_user: String::new(),
            ra_token: String::new(),
            ra_hardcore: true,
            ra_connect: None,
            keymap: KeyMap::defaults(),
            padmap: PadMap::defaults(),
            source: None,
        }
    }

    #[test]
    fn empty_config_keeps_defaults() {
        let mut cfg = defaults();
        cfg.apply(toml::from_str("").unwrap()).unwrap();
        assert!(!cfg.fullscreen);
        assert!(cfg.check_updates_on_start);
    }

    #[test]
    fn check_updates_on_start_can_be_turned_off() {
        let mut cfg = defaults();
        cfg.apply(toml::from_str("check_updates_on_start = false").unwrap())
            .unwrap();
        assert!(!cfg.check_updates_on_start);
    }

    #[test]
    fn lang_round_trips_and_rejects_junk() {
        let mut cfg = defaults();
        assert_eq!(cfg.lang, LangOpt::En);
        cfg.apply(toml::from_str("lang = \"pt\"").unwrap()).unwrap();
        assert_eq!(cfg.lang, LangOpt::Pt);
        cfg.apply(toml::from_str("lang = \"EN\"").unwrap()).unwrap();
        assert_eq!(cfg.lang, LangOpt::En);
        // "auto" (the locale-driven default of the brief middle revision)
        // folds into the new default instead of erroring.
        cfg.apply(toml::from_str("lang = \"auto\"").unwrap())
            .unwrap();
        assert_eq!(cfg.lang, LangOpt::En);
        // Unknown value keeps the current one (warn, not an error — same rule
        // as the stale keyboard binds).
        cfg.apply(toml::from_str("lang = \"es\"").unwrap()).unwrap();
        assert_eq!(cfg.lang, LangOpt::En);
        // And the text form survives a save/load round trip.
        cfg.lang = LangOpt::Pt;
        let raw: Raw = toml::from_str(&cfg.to_toml()).unwrap();
        let mut round = defaults();
        round.apply(raw).unwrap();
        assert_eq!(round.lang, LangOpt::Pt);
    }

    #[test]
    fn overrides_and_rebinds_apply() {
        let mut cfg = defaults();
        // "pause" is a pre-revision console-command binding — ignored now,
        // not an error (see `unknown_action_is_ignored`).
        let raw: Raw =
            toml::from_str("fullscreen = true\n[keyboard]\nb = \"Space\"\npause = \"Escape\"")
                .unwrap();
        cfg.apply(raw).unwrap();
        assert!(cfg.fullscreen);
        // Rebound key resolves; describe() reflects it.
        let d = cfg.keymap.describe();
        assert!(d.contains(&("b".to_string(), "Space".to_string())));
    }

    #[test]
    fn unknown_action_is_ignored() {
        // Not an error: a typo, or (more likely) a pre-revision config with a
        // now-removed console-command binding (eject, pause, ...) — either
        // way there's nothing to bind it to, so `apply` warns and moves on
        // rather than refusing to start over one stale line.
        let mut cfg = defaults();
        let raw: Raw = toml::from_str("[keyboard]\nwarp = \"W\"\nb = \"Space\"").unwrap();
        cfg.apply(raw).unwrap();
        assert!(cfg
            .keymap
            .describe()
            .contains(&("b".to_string(), "Space".to_string())));
    }

    #[test]
    fn shelf_theme_round_trips_and_rejects_junk() {
        let mut cfg = defaults();
        assert_eq!(cfg.shelf_theme, ShelfTheme::Wood);
        cfg.apply(toml::from_str("shelf_theme = \"metal\"").unwrap())
            .unwrap();
        assert_eq!(cfg.shelf_theme, ShelfTheme::Metal);
        cfg.apply(toml::from_str("shelf_theme = \"vidro\"").unwrap())
            .unwrap();
        assert_eq!(cfg.shelf_theme, ShelfTheme::Metal);
        let raw: Raw = toml::from_str(&cfg.to_toml()).unwrap();
        let mut round = defaults();
        round.apply(raw).unwrap();
        assert_eq!(round.shelf_theme, ShelfTheme::Metal);
    }

    #[test]
    fn ra_fields_round_trip_and_default_off() {
        let mut cfg = defaults();
        assert!(cfg.ra_user.is_empty() && cfg.ra_token.is_empty());
        assert!(cfg.ra_hardcore);
        cfg.ra_user = "player1".into();
        cfg.ra_token = "abc123".into();
        cfg.ra_hardcore = false;
        let raw: Raw = toml::from_str(&cfg.to_toml()).unwrap();
        let mut round = defaults();
        round.apply(raw).unwrap();
        assert_eq!(round.ra_user, "player1");
        assert_eq!(round.ra_token, "abc123");
        assert!(!round.ra_hardcore);
    }

    #[test]
    fn to_toml_round_trips() {
        let mut cfg = defaults();
        cfg.fullscreen = true;
        cfg.check_updates_on_start = false;
        let text = cfg.to_toml();
        let raw: Raw = toml::from_str(&text).unwrap();
        let mut round = defaults();
        round.apply(raw).unwrap();
        assert!(round.fullscreen);
        assert!(!round.check_updates_on_start);
        assert_eq!(round.keymap.describe(), cfg.keymap.describe());
        assert_eq!(round.padmap.describe(), cfg.padmap.describe());
    }

    #[test]
    fn gamepad_rebind_applies_and_round_trips() {
        let mut cfg = defaults();
        let raw: Raw = toml::from_str("[gamepad]\nb = \"dpup\"").unwrap();
        cfg.apply(raw).unwrap();
        let d = cfg.padmap.describe();
        // B now lives on dpup; Up lost its button to it (one action per
        // button, one button per action — same rule as the keyboard side).
        assert!(d.contains(&("b".to_string(), "dpup".to_string())));
        assert!(d.contains(&("up".to_string(), String::new())));
        // And the text form survives a save/load round trip.
        let raw: Raw = toml::from_str(&cfg.to_toml()).unwrap();
        let mut round = defaults();
        round.apply(raw).unwrap();
        assert_eq!(round.padmap.describe(), cfg.padmap.describe());
    }

    #[test]
    fn unknown_gamepad_button_is_an_error() {
        let mut cfg = defaults();
        let raw: Raw = toml::from_str("[gamepad]\nb = \"not_a_button\"").unwrap();
        assert!(cfg.apply(raw).is_err());
    }
}
