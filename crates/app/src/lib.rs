//! Shared bits for the `sixteen-app` binaries.
//!
//! - [`config`] — `sixteen.cfg` (run-ahead, fullscreen, key binds).
//! - [`core_update`] — download/update the snes9x core from the libretro buildbot.
//! - [`dirs`] — the portable app layout (one root: next to the executable
//!   on Windows/Linux, `~/Documents/SixteeN` on macOS).
//! - [`devmenu`] — the dev-mode menu (Konami-code secret; blank for now).
//! - [`exemplo`] — the built-in example art seeded into the data folder.
//! - [`idle`] — the idle/root screen (`sixteen`'s home: TV off, "Inserir cartucho").
//! - [`rom_rename`] — rename ROMs to their canonical No-Intro name (settings-screen action).
//! - [`runner`] — the emulator run-loop (`emu-run`, and `sixteen` between games).
//! - [`settings`] — the settings screen (`sixteen` only, opened with `O` on the shelf).
//! - [`sfx`] — the console's embedded foley sounds (insert/eject/power/reset).
//! - [`shelf`] — the selector grid (`selector`, and `sixteen` between games).
//! - [`update_check`] — startup checks for a newer release/snes9x core.

pub mod config;
pub mod console_art;
mod contato;
pub mod core_update;
pub mod dat_update;
pub mod devmenu;
pub mod dirs;
pub mod exemplo;
pub mod idle;
pub mod manual;
pub mod ra;
pub mod rom_rename;
pub mod runner;
pub mod settings;
pub mod sfx;
pub mod shelf;
pub mod update_check;

use sixteen_platform::{Cabinet, Picture, TvSound};

/// Passa as opções da TV e do quarto para o gabinete — na abertura e a
/// cada mudança na tela de configurações.
pub fn apply_tv_settings(cfg: &config::Config, cab: &mut Cabinet) {
    cab.set_tv_sound(if cfg.tv_speaker {
        TvSound::Speaker
    } else {
        TvSound::Stereo
    });
    cab.set_flyback(cfg.flyback_whine);
    cab.set_burn_in(cfg.burn_in);
    cab.set_room_light(cfg.room_light);
    let [brilho, contraste, cor, nitidez] = cfg.tv_picture;
    cab.set_picture(Picture {
        brilho,
        contraste,
        cor,
        nitidez,
    });
}

/// Traz o que o menu de imagem da TV ajustou para o `cfg`; `true` se
/// mudou (aí vale gravar).
pub fn sync_tv_picture(cfg: &mut config::Config, cab: &Cabinet) -> bool {
    let p = cab.picture();
    let now = [p.brilho, p.contraste, p.cor, p.nitidez];
    let changed = cfg.tv_picture != now;
    cfg.tv_picture = now;
    changed
}

/// O menu de imagem da TV em ajustes do filtro NTSC: 50 = o preset puro,
/// e os extremos vão até onde a imagem ainda é imagem.
pub fn ntsc_picture(p: Picture) -> sixteen_ntsc::Picture {
    let k = |v: u8, span: f64| (v as f64 - 50.0) / 50.0 * span;
    sixteen_ntsc::Picture {
        brightness: k(p.brilho, 0.45),
        contrast: k(p.contraste, 0.45),
        saturation: k(p.cor, 0.9),
        sharpness: k(p.nitidez, 0.8),
    }
}

/// A logo oficial do RetroAchievements (o favicon do site, embutida) — o
/// runner/bin registra no `Cabinet` com [`sixteen_platform::RA_LOGO_IMG`]
/// para o badge do queixo. Vermelho/dourado/azul: a marca lida na própria
/// TV.
pub const RA_ICON_PNG: &[u8] = include_bytes!("../assets/ra-icon.png");

/// O "H" do HowLongToBeat (o favicon do site, embutida) — a estante registra
/// no `Cabinet` ao lado do badge do RA para o cabeçalho do bloco de tempos
/// (as barras "história / + extras / 100% / todos" da aba informações).
pub const HLTB_ICON_PNG: &[u8] = include_bytes!("../assets/hltb-icon.png");

/// A logo da IGDB (o quadrado roxo do press kit, embutida) — a estante
/// registra no `Cabinet` para o cabeçalho da seção de nota.
pub const IGDB_ICON_PNG: &[u8] = include_bytes!("../assets/igdb-icon.png");
