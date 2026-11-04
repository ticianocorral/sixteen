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
use sixteen_platform::{KeyMap, PadButton, PadMap, ScreenLines, ScreenLook};

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
    /// "Sugestão da noite": botão na estante que sorteia um jogo (de
    /// preferência nunca aberto ou parado há muito tempo). Desligado por
    /// padrão. Linha na seção "estante" das configurações.
    pub night_pick: bool,
    /// Mau contato do cartucho (soprar o cartucho): com que frequência o
    /// primeiro Power depois de inserir não sobe o jogo — tela preta, lixo
    /// de tiles, abertura travada ou gráficos embaralhados — até ejetar,
    /// soprar e inserir de novo.
    /// Desligado por padrão. Linha na seção "jogo" das configurações.
    pub bad_contact: BadContact,
    /// Soprar no microfone de verdade na cena do sopro (Steam Deck, ROG
    /// Ally, notebook) — além do clique/Y. Desligado por padrão: o
    /// microfone só abre durante a cena, e só com isto ligado.
    pub blow_mic: bool,
    /// O som do jogo pelo alto-falante da própria TV (mono, abafado) em
    /// vez do 3 em 1 estéreo (o padrão). Seção de vídeo.
    pub tv_speaker: bool,
    /// O chiado agudo do flyback (~15 kHz) com a TV ligada. Desligado por
    /// padrão — tem quem nem escute, tem quem não aguente.
    pub flyback_whine: bool,
    /// Burn-in leve: horas no mesmo jogo marcam o HUD no tubo, e a marca
    /// some com o tempo. Desligado por padrão.
    pub burn_in: bool,
    /// O menu OSD da TV: brilho, contraste, cor e nitidez (0..100, 50 =
    /// neutro), ajustados pelo MENU do queixo e salvos aqui ao sair.
    pub tv_picture: [u8; 4],
    /// O visual da tela (`screen_look`): sinal + scanlines/grade/brilho.
    pub screen_look: ScreenLook,
    /// As linhas e a máscara (`screen_lines`), à parte do sinal.
    pub screen_lines: ScreenLines,
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
    /// O mesmo, para o controle 2 (`[gamepad2]`) — cada controle tem o seu
    /// mapa, já que dois controles diferentes raramente batem.
    pub padmap2: PadMap,
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
/// (`shelf_theme = "wood"` / `"metal"` / `"hifi"` / `"memphis"` /
/// `"arcade"`).
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
    /// O rack de som preto da sala: laminado "black ash", frisos
    /// prateados e prateleiras de vidro fumê.
    HiFi,
    /// O quarto dos anos 90: papel de parede Memphis, móvel laqueado
    /// branco com fita colorida.
    Memphis,
    /// O fliperama: carpete com desenho fluorescente, neon rosa nos
    /// montantes e neon azul nas prateleiras.
    Arcade,
}

impl ShelfTheme {
    pub fn as_str(self) -> &'static str {
        match self {
            ShelfTheme::Wood => "wood",
            ShelfTheme::Metal => "metal",
            ShelfTheme::HiFi => "hifi",
            ShelfTheme::Memphis => "memphis",
            ShelfTheme::Arcade => "arcade",
        }
    }

    /// Click / left-right on the settings row: the next furniture, in a loop.
    pub fn cycle(self) -> Self {
        match self {
            ShelfTheme::Wood => ShelfTheme::Metal,
            ShelfTheme::Metal => ShelfTheme::HiFi,
            ShelfTheme::HiFi => ShelfTheme::Memphis,
            ShelfTheme::Memphis => ShelfTheme::Arcade,
            ShelfTheme::Arcade => ShelfTheme::Wood,
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "wood" | "madeira" | "" => Some(ShelfTheme::Wood),
            "metal" => Some(ShelfTheme::Metal),
            "hifi" | "rack" => Some(ShelfTheme::HiFi),
            "memphis" | "quarto" => Some(ShelfTheme::Memphis),
            "arcade" | "fliperama" => Some(ShelfTheme::Arcade),
            _ => None,
        }
    }
}

/// Com que frequência o cartucho dá mau contato no primeiro Power —
/// gravado como a palavra no `sixteen.cfg` (`bad_contact = "off"` /
/// `"rare"` / `"90s"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BadContact {
    /// Nunca falha (padrão).
    #[default]
    Off,
    /// Mais ou menos 1 encaixe em 15.
    Rare,
    /// Mais ou menos 1 encaixe em 5 — os anos 90 de verdade.
    Nineties,
}

impl BadContact {
    pub fn as_str(self) -> &'static str {
        match self {
            BadContact::Off => "off",
            BadContact::Rare => "rare",
            BadContact::Nineties => "90s",
        }
    }

    /// Click / left-right on the settings row: desligado → raro → anos 90.
    pub fn cycle(self) -> Self {
        match self {
            BadContact::Off => BadContact::Rare,
            BadContact::Rare => BadContact::Nineties,
            BadContact::Nineties => BadContact::Off,
        }
    }

    /// Seta para a esquerda na linha: o caminho de volta do `cycle`.
    pub fn cycle_back(self) -> Self {
        match self {
            BadContact::Off => BadContact::Nineties,
            BadContact::Rare => BadContact::Off,
            BadContact::Nineties => BadContact::Rare,
        }
    }

    /// A chance de um encaixe recém-feito dar mau contato.
    pub fn chance(self) -> f32 {
        match self {
            BadContact::Off => 0.0,
            BadContact::Rare => 1.0 / 15.0,
            BadContact::Nineties => 1.0 / 5.0,
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "" => Some(BadContact::Off),
            "rare" | "raro" => Some(BadContact::Rare),
            "90s" | "nineties" | "anos90" => Some(BadContact::Nineties),
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
    night_pick: Option<bool>,
    #[serde(default)]
    bad_contact: Option<String>,
    #[serde(default)]
    blow_mic: Option<bool>,
    #[serde(default)]
    tv_speaker: Option<bool>,
    #[serde(default)]
    flyback_whine: Option<bool>,
    #[serde(default)]
    burn_in: Option<bool>,
    #[serde(default)]
    tv_picture: Option<Vec<u8>>,
    #[serde(default)]
    screen_look: Option<String>,
    screen_lines: Option<String>,
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
    #[serde(default)]
    gamepad2: BTreeMap<String, String>,
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
            night_pick: false,
            bad_contact: BadContact::Off,
            blow_mic: false,
            tv_speaker: false,
            flyback_whine: false,
            burn_in: false,
            tv_picture: [50; 4],
            screen_look: ScreenLook::Rf,
            screen_lines: ScreenLines::Auto,
            ra_user: String::new(),
            ra_token: String::new(),
            ra_hardcore: true,
            ra_connect: None,
            keymap: KeyMap::defaults(),
            padmap: PadMap::defaults(),
            padmap2: PadMap::defaults(),
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
        if let Some(n) = raw.night_pick {
            self.night_pick = n;
        }
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
                    "[config] shelf_theme = {t:?}: not a theme (want wood|metal|hifi|memphis|arcade), keeping {}",
                    self.shelf_theme.as_str()
                ),
            }
        }
        if let Some(b) = raw.bad_contact {
            match BadContact::parse(&b) {
                Some(v) => self.bad_contact = v,
                None => log::warn!(
                    "[config] bad_contact = {b:?}: not a level (want off|rare|90s), keeping {}",
                    self.bad_contact.as_str()
                ),
            }
        }
        if let Some(m) = raw.blow_mic {
            self.blow_mic = m;
        }
        if let Some(v) = raw.tv_speaker {
            self.tv_speaker = v;
        }
        if let Some(v) = raw.flyback_whine {
            self.flyback_whine = v;
        }
        if let Some(v) = raw.burn_in {
            self.burn_in = v;
        }
        if let Some(p) = raw.tv_picture {
            match <[u8; 4]>::try_from(p.as_slice()) {
                Ok(p) => self.tv_picture = p.map(|v| v.min(100)),
                Err(_) => log::warn!(
                    "[config] tv_picture = {p:?}: want 4 values (brightness, contrast, color, sharpness)"
                ),
            }
        }
        if let Some(l) = raw.screen_look {
            match ScreenLook::parse(&l) {
                Some(v) => self.screen_look = v,
                None => log::warn!(
                    "[config] screen_look = {l:?}: not a look (want rf|composite|svideo|pvm|trinitron|lcd|mono), keeping {}",
                    self.screen_look.as_str()
                ),
            }
        }
        if let Some(l) = raw.screen_lines {
            match ScreenLines::parse(&l) {
                Some(v) => self.screen_lines = v,
                None => log::warn!(
                    "[config] screen_lines = {l:?}: not an option (want auto|off|light|pvm|grille|grid), keeping {}",
                    self.screen_lines.as_str()
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
            if key.is_empty() {
                continue;
            }
            self.keymap
                .bind_pad(key, b)
                .map_err(anyhow::Error::msg)
                .with_context(|| format!("[keyboard] {action}"))?;
        }
        for (section, map, table) in [
            ("gamepad", &mut self.padmap, &raw.gamepad),
            ("gamepad2", &mut self.padmap2, &raw.gamepad2),
        ] {
            for (action, button) in table {
                let Ok(b) = action.parse::<PadButton>() else {
                    log::warn!("[{section}] {action}: not a bindable action, ignoring");
                    continue;
                };
                if button.is_empty() {
                    // Um arquivo antigo, de quando roubar um botão deixava a
                    // outra ação vazia — nada a ligar aqui.
                    continue;
                }
                map.bind(button, b)
                    .map_err(anyhow::Error::msg)
                    .with_context(|| format!("[{section}] {action}"))?;
            }
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
             # shelf_theme: the shelf furniture — wood (default), metal (rental-store rack),\n\
             # hifi (black stereo rack), memphis (90s bedroom) or arcade.\n\
             # night_pick: show a \"tonight's pick\" button on the shelf (off by default).\n\
             # bad_contact: cartridge bad contact on first power-on — off (default), rare or 90s.\n\
             # blow_mic: blow into the real microphone to clean the cartridge (off by default).\n\
             # tv_speaker: game sound through the TV's own small mono speaker (off = 3-in-1 stereo).\n\
             # flyback_whine: the ~15 kHz flyback whine while the TV is on (off by default).\n\
             # burn_in: hours on the same game lightly mark the HUD on the tube (off by default).\n\
             # tv_picture: TV menu [brightness, contrast, color, sharpness], 0-100, 50 = neutral.\n\
             # screen_look: the picture style — rf (default), composite, svideo, pvm, trinitron, lcd or mono.\n\
             # screen_lines: scanlines/mask over any look — auto (the look's own), off, light, pvm, grille or grid.\n\
             # RetroAchievements: ra_user/ra_token from retroachievements.org\n\
             # (Settings -> Web API); empty = the whole feature stays off.\n\
             # ra_hardcore: no cheats/savestates while earning achievements.\n\
             # [keyboard]: action = \"SDL key name\" (e.g. \"Left Shift\", \"F2\", \"]\").\n\
             # [gamepad]: action = \"SDL gamepad button name\" (e.g. \"south\",\n\
             # \"dpup\", \"leftshoulder\") — which pad button drives each action.\n\
             # [gamepad2]: the same, for the second controller.\n\n",
        );
        s.push_str(&format!("fullscreen = {}\n", self.fullscreen));
        s.push_str(&format!("lang = {:?}\n", self.lang.as_str()));
        s.push_str(&format!(
            "check_updates_on_start = {}\n",
            self.check_updates_on_start
        ));
        s.push_str(&format!("hiss_on_static = {}\n", self.hiss_on_static));
        s.push_str(&format!("shelf_theme = {:?}\n", self.shelf_theme.as_str()));
        s.push_str(&format!("night_pick = {}\n", self.night_pick));
        s.push_str(&format!("bad_contact = {:?}\n", self.bad_contact.as_str()));
        s.push_str(&format!("blow_mic = {}\n", self.blow_mic));
        s.push_str(&format!("tv_speaker = {}\n", self.tv_speaker));
        s.push_str(&format!("flyback_whine = {}\n", self.flyback_whine));
        s.push_str(&format!("burn_in = {}\n", self.burn_in));
        s.push_str(&format!("tv_picture = {:?}\n", self.tv_picture));
        s.push_str(&format!("screen_look = {:?}\n", self.screen_look.as_str()));
        s.push_str(&format!(
            "screen_lines = {:?}\n\n",
            self.screen_lines.as_str()
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
        s.push_str("\n[gamepad2]\n");
        for (action, button) in self.padmap2.describe() {
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
            night_pick: false,
            bad_contact: BadContact::Off,
            blow_mic: false,
            tv_speaker: false,
            flyback_whine: false,
            burn_in: false,
            tv_picture: [50; 4],
            screen_look: ScreenLook::Rf,
            screen_lines: ScreenLines::Auto,
            ra_user: String::new(),
            ra_token: String::new(),
            ra_hardcore: true,
            ra_connect: None,
            keymap: KeyMap::defaults(),
            padmap: PadMap::defaults(),
            padmap2: PadMap::defaults(),
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
    fn night_pick_round_trips_and_defaults_off() {
        let mut cfg = defaults();
        assert!(!cfg.night_pick);
        cfg.apply(toml::from_str("night_pick = true").unwrap())
            .unwrap();
        assert!(cfg.night_pick);
        let round = defaults();
        let mut round = round;
        round
            .apply(toml::from_str(&cfg.to_toml()).unwrap())
            .unwrap();
        assert!(round.night_pick);
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
        // Every theme survives the round trip, and cycling visits them all.
        let mut t = ShelfTheme::Wood;
        let mut seen = Vec::new();
        loop {
            assert_eq!(ShelfTheme::parse(t.as_str()), Some(t));
            seen.push(t);
            t = t.cycle();
            if t == ShelfTheme::Wood {
                break;
            }
        }
        assert_eq!(seen.len(), 5);
        assert_eq!(ShelfTheme::parse("fliperama"), Some(ShelfTheme::Arcade));
    }

    #[test]
    fn bad_contact_round_trips_and_rejects_junk() {
        let mut cfg = defaults();
        assert_eq!(cfg.bad_contact, BadContact::Off);
        assert_eq!(BadContact::Off.chance(), 0.0);
        cfg.apply(toml::from_str("bad_contact = \"90s\"").unwrap())
            .unwrap();
        assert_eq!(cfg.bad_contact, BadContact::Nineties);
        cfg.apply(toml::from_str("bad_contact = \"sempre\"").unwrap())
            .unwrap();
        assert_eq!(cfg.bad_contact, BadContact::Nineties);
        let raw: Raw = toml::from_str(&cfg.to_toml()).unwrap();
        let mut round = defaults();
        round.apply(raw).unwrap();
        assert_eq!(round.bad_contact, BadContact::Nineties);
        assert_eq!(BadContact::Nineties.cycle(), BadContact::Off);
        for b in [BadContact::Off, BadContact::Rare, BadContact::Nineties] {
            assert_eq!(b.cycle().cycle_back(), b);
        }
        assert!(!round.blow_mic);
        cfg.apply(toml::from_str("blow_mic = true").unwrap())
            .unwrap();
        let mut round = defaults();
        round
            .apply(toml::from_str(&cfg.to_toml()).unwrap())
            .unwrap();
        assert!(round.blow_mic);
    }

    #[test]
    fn tv_room_settings_round_trip() {
        let mut cfg = defaults();
        assert!(!cfg.tv_speaker && !cfg.flyback_whine && !cfg.burn_in);
        assert_eq!(cfg.tv_picture, [50; 4]);
        cfg.apply(
            toml::from_str(
                "tv_speaker = true\nflyback_whine = true\nburn_in = true\n\
                 tv_picture = [70, 40, 55, 200]",
            )
            .unwrap(),
        )
        .unwrap();
        // Valor fora de 0..100 encosta no máximo.
        assert_eq!(cfg.tv_picture, [70, 40, 55, 100]);
        // Lista do tamanho errado é ignorada.
        cfg.apply(toml::from_str("tv_picture = [1, 2]").unwrap())
            .unwrap();
        assert_eq!(cfg.tv_picture, [70, 40, 55, 100]);
        let mut round = defaults();
        round
            .apply(toml::from_str(&cfg.to_toml()).unwrap())
            .unwrap();
        assert!(round.tv_speaker && round.flyback_whine && round.burn_in);
        assert_eq!(round.tv_picture, [70, 40, 55, 100]);
    }

    #[test]
    fn screen_look_round_trips_and_rejects_junk() {
        let mut cfg = defaults();
        assert_eq!(cfg.screen_look, ScreenLook::Rf);
        cfg.apply(toml::from_str("screen_look = \"trinitron\"").unwrap())
            .unwrap();
        assert_eq!(cfg.screen_look, ScreenLook::Trinitron);
        cfg.apply(toml::from_str("screen_look = \"holograma\"").unwrap())
            .unwrap();
        assert_eq!(cfg.screen_look, ScreenLook::Trinitron);
        let mut round = defaults();
        round
            .apply(toml::from_str(&cfg.to_toml()).unwrap())
            .unwrap();
        assert_eq!(round.screen_look, ScreenLook::Trinitron);
        cfg.apply(toml::from_str("screen_lines = \"pvm\"").unwrap())
            .unwrap();
        assert_eq!(cfg.screen_lines, ScreenLines::Pvm);
        cfg.apply(toml::from_str("screen_lines = \"xx\"").unwrap())
            .unwrap();
        assert_eq!(cfg.screen_lines, ScreenLines::Pvm);
    }

    #[test]
    fn old_cheat_genius_key_is_ignored() {
        // A chave saiu (o Cheat Genius é o único jeito de usar cheats);
        // um config.toml antigo com ela continua carregando.
        let raw: Raw = toml::from_str("cheat_genius = false").unwrap();
        let mut cfg = defaults();
        cfg.apply(raw).unwrap();
        assert!(!cfg.to_toml().contains("cheat_genius"));
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
        // B now lives on dpup; Up took B's old button in the swap (one
        // action per button, one button per action — nothing left unbound).
        let south = PadMap::defaults().describe()[0].1.clone();
        assert!(d.contains(&("b".to_string(), "dpup".to_string())));
        assert!(d.contains(&("up".to_string(), south)));
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

    #[test]
    fn second_gamepad_has_its_own_map() {
        let mut cfg = defaults();
        let raw: Raw = toml::from_str("[gamepad2]\nb = \"b\"\na = \"a\"").unwrap();
        cfg.apply(raw).unwrap();
        assert_eq!(cfg.padmap, PadMap::defaults());
        assert_ne!(cfg.padmap2, PadMap::defaults());
        let raw: Raw = toml::from_str(&cfg.to_toml()).unwrap();
        let mut round = defaults();
        round.apply(raw).unwrap();
        assert_eq!(round.padmap2, cfg.padmap2);
    }
}
