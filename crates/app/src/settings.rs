//! The settings screen — the idle screen's/shelf's "Configurações" button,
//! redrawn on the shelf's own layout (plan revision: "a tela de
//! configurações não está no padrão do resto do app — faça a tv na mesma
//! proporção e coloque o painel"): rows inside the tube, exactly like the
//! shelf's grid area, and a flat side panel carrying the section buttons
//! ("jogo", "vídeo", "sistema", "controles") plus "Voltar". No nesting
//! deeper than a section: click a row to act on it (or gamepad nav +
//! Confirm/Back — plan revision: no keyboard shortcuts). Edits save to
//! `sixteen.cfg` immediately, not on some later "apply" step — there's
//! nothing to lose by backing out.

use std::sync::mpsc::{self, Receiver};
use std::time::Instant;

use anyhow::Result;
#[allow(unused_imports)]
use sixteen_platform::tr;
use sixteen_platform::{
    Cabinet, MenuMode, MenuNav, Osk, OskOutcome, Platform, Screen, SettingsButton,
    SettingsPanelInfo, TextField,
};

use crate::config::Config;
use crate::controls_screen::{self, ControlsScreen};
use crate::core_update::{self, CoreUpdateMsg};
use crate::rom_rename::{self, RenameOutcome};

const BG: (u8, u8, u8) = (18, 18, 20);
const TEXT: (u8, u8, u8) = (232, 232, 232);
const DIM: (u8, u8, u8) = (150, 150, 158);
const HINT: (u8, u8, u8) = (120, 116, 108);

const MARGIN: i32 = 40;
const ROW_H: i32 = 30;
/// Where the row list starts, vertically — right under the title (drawn at
/// scale 2, so `2 * GLYPH_H` tall) plus a little breathing room. Shared by
/// the draw functions and the click hit-test so they can't drift apart.
const LIST_TOP: i32 = MARGIN + 50;

/// The settings sections, in panel order — the tube shows one section's
/// rows at a time; the panel's `Section(i)` buttons switch between them
/// (plan revision: "se for interessante faça secoes na configuração").
/// Labels read the active language on every call (not a `const`: the
/// English base and the pt-BR fallback are picked at runtime).
fn section_names() -> [&'static str; 6] {
    [
        tr!("controls", "controles"),
        tr!("game", "jogo"),
        tr!("picture & sound", "imagem e som"),
        tr!("shelf", "estante"),
        tr!("achievements", "conquistas"),
        tr!("system", "sistema"),
    ]
}
const SEC_CONTROLES: usize = 0;
const SEC_JOGO: usize = 1;
const SEC_IMAGEM: usize = 2;
const SEC_ESTANTE: usize = 3;
const SEC_CONQUISTAS: usize = 4;
const SEC_SISTEMA: usize = 5;

/// Which "conquistas" row the text-entry editor is attached to.
const RA_ROW_USER: usize = 0;
const RA_ROW_TOKEN: usize = 1;
const RA_ROW_SENHA: usize = 2;

/// LB/RB (ou LT/RT): a seção vizinha, com volta nas pontas — todas as
/// seções ficam a um aperto de ombro, sem precisar do mouse no painel.
fn step_section(sec: usize, forward: bool) -> usize {
    let n = section_names().len();
    if forward {
        (sec + 1) % n
    } else {
        (sec + n - 1) % n
    }
}

/// Quantos caracteres cabem nos campos das conquistas.
const RA_FIELD_LIMIT: usize = 40;

/// Abre o editor de um campo das conquistas (usuário, chave, senha): o
/// usuário parte do valor atual; chave e senha começam vazios (a senha é
/// mascarada). `paste` já entra colado (botão direito). O teclado na tela
/// abre quando a última entrada foi do controle.
fn begin_ra_edit(
    plat: &Platform,
    cab: &Cabinet,
    cfg: &Config,
    row: usize,
    paste: Option<String>,
    field: &mut TextField,
    osk: &mut Osk,
) {
    let initial = if row == RA_ROW_USER {
        cfg.ra_user.as_str()
    } else {
        ""
    };
    *field = TextField::new(initial, RA_FIELD_LIMIT, row == RA_ROW_SENHA);
    if let Some(p) = paste {
        field.insert_str(&p);
    }
    osk.begin(plat.last_input_was_pad());
    plat.start_text_input(cab);
}

/// O título do teclado na tela de cada campo.
fn ra_field_title(row: usize) -> &'static str {
    match row {
        RA_ROW_USER => tr!("RetroAchievements username", "usuário do RetroAchievements"),
        RA_ROW_TOKEN => tr!(
            "RetroAchievements web API key",
            "chave da web API do RetroAchievements"
        ),
        _ => tr!("RetroAchievements password", "senha do RetroAchievements"),
    }
}

/// How many rows a section shows — shared by the nav clamps and the click
/// hit-test so they can't drift apart.
fn row_count(sec: usize) -> usize {
    match sec {
        SEC_CONTROLES => controls_screen::ROWS,
        // mau contato / soprar
        SEC_JOGO => 2,
        // tela cheia / som do jogo / chiado / flyback / burn-in / visual
        SEC_IMAGEM => 7,
        // tema / sugestão da noite / renomear ROMs
        SEC_ESTANTE => 3,
        // usuário / token / senha / testar login / hardcore
        SEC_CONQUISTAS => 5,
        // núcleo / checar updates / idioma
        SEC_SISTEMA => 3,
        _ => 0,
    }
}

/// Which row (if any) a screen-local click y lands in, given how many rows
/// are actually on screen right now — settings rows span the full width, so
/// only y matters. Shared by the draw functions and the click handling in
/// `run`, so they can't drift apart.
fn row_at(sec: usize, y: i32) -> Option<usize> {
    if sec == SEC_CONTROLES {
        return ControlsScreen::row_at(y, LIST_TOP);
    }
    let count = row_count(sec);
    if y < LIST_TOP {
        return None;
    }
    let i = ((y - LIST_TOP) / ROW_H) as usize;
    (i < count).then_some(i)
}

/// Where the "Núcleo" row's background download stands right now — drives
/// both its label and whether `Confirm` on it starts a new one.
enum CoreStatus {
    Idle,
    Downloading { downloaded: u64, total: Option<u64> },
    Done,
    Failed(String),
}

/// The "testar login" row's state — the same Idle/busy/ok/fail shape the
/// core row uses, minus progress (one small request).
enum LoginStatus {
    Idle,
    Checking,
    Ok(String),
    Failed(String),
}

/// Run the settings screen until the player backs all the way out. Returns
/// `true` if the whole app should quit (window closed / Cmd-Q) instead of
/// returning to the shelf.
/// A tela de configurações. Os ajustes valem na hora (idioma, tela cheia,
/// hiss...), mas o `sixteen.cfg` é gravado UMA vez, ao sair, se algo mudou
/// — segurar uma seta num ajuste gerava uma gravação por passo. Login e
/// token do RetroAchievements continuam gravando na hora (credencial não
/// espera a saída da tela).
pub fn run(plat: &mut Platform, cab: &mut Cabinet, cfg: &mut Config) -> Result<bool> {
    // O menu de imagem da TV pode ter mudado desde a última gravação:
    // entra no `cfg` antes, para esta gravação não salvar o valor velho.
    crate::sync_tv_picture(cfg, cab);
    let before = cfg.to_toml();
    let result = run_screen(plat, cab, cfg);
    plat.set_hold_pad_order(false);
    if cfg.to_toml() != before {
        if let Err(e) = cfg.save() {
            log::warn!("settings: saving config: {e}");
        }
    }
    result
}

fn run_screen(plat: &mut Platform, cab: &mut Cabinet, cfg: &mut Config) -> Result<bool> {
    let mut sec = SEC_CONTROLES;
    let mut sel: usize = 0;
    let mut controls = ControlsScreen::new();
    let mut core_status = CoreStatus::Idle;
    let mut core_worker: Option<Receiver<CoreUpdateMsg>> = None;
    // The last "Renomear ROMs" result, shown as that row's own label until
    // the next click (plan revision) — synchronous (plain disk renames, no
    // network), so unlike the core download there's no worker/progress to
    // track, just a one-shot summary.
    let mut rename_status: Option<String> = None;
    // RA account rows (plan: `docs/plano-retroachievements.md`, fase 1):
    // the free-text editor (same deliberate keyboard exception as the
    // shelf's filter box) and the login-test worker.
    let mut ra_editing: Option<usize> = None;
    let mut ra_draft = TextField::new("", RA_FIELD_LIMIT, false);
    let mut ra_osk = Osk::new();
    let mut login_status = LoginStatus::Idle;
    let mut login_worker: Option<Receiver<Result<String, String>>> = None;
    // O worker em andamento é o login Connect (senha → token)? Se sim, o
    // `Ok` guarda o token no cfg em vez de só exibir.
    let mut login_connect = false;
    let frame_time = crate::runner::UI_FRAME;
    let mut next = Instant::now();
    cab.set_close_button(true);

    loop {
        if let Some(rx) = &login_worker {
            match rx.try_recv() {
                Ok(Ok(token)) => {
                    if login_connect {
                        cfg.ra_connect = Some(token);
                        let _ = cfg.save();
                        login_status = tr!(
                            LoginStatus::Ok("account linked — achievements unlocked".to_string()),
                            LoginStatus::Ok("conta conectada — conquistas liberadas".to_string())
                        );
                        login_connect = false;
                    } else {
                        login_status = LoginStatus::Ok(token);
                    }
                    login_worker = None;
                }
                Ok(Err(e)) => {
                    login_status = LoginStatus::Failed(e);
                    login_worker = None;
                    login_connect = false;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => login_worker = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &core_worker {
            while let Ok(msg) = rx.try_recv() {
                match msg {
                    CoreUpdateMsg::Progress { downloaded, total } => {
                        core_status = CoreStatus::Downloading { downloaded, total };
                    }
                    CoreUpdateMsg::Done => {
                        core_status = CoreStatus::Done;
                        core_worker = None;
                        break;
                    }
                    CoreUpdateMsg::Failed(e) => {
                        core_status = CoreStatus::Failed(e);
                        core_worker = None;
                        break;
                    }
                }
            }
        }

        // While editing an RA field, ONLY the text-entry side is polled —
        // a `poll_menu` first would drain (and discard) the TextInput
        // events before `poll_text_entry` ever sees them.
        if let Some(row) = ra_editing {
            let te = plat.poll_text_entry();
            let outcome = ra_osk.update(&te, &mut ra_draft, plat);
            if outcome == OskOutcome::Quit {
                return Ok(true);
            }
            let committed = outcome == OskOutcome::Commit;
            let cancelled = outcome == OskOutcome::Cancel;
            if committed {
                let typed = ra_draft.text().trim().to_string();
                if row == RA_ROW_USER {
                    cfg.ra_user = typed;
                } else if row == RA_ROW_SENHA {
                    // A senha nunca é gravada: vira token Connect por
                    // login2 — ele que autentica a lógica das conquistas e
                    // a submissão (plan revision: a web key ficou readonly).
                    let (user, senha) = (cfg.ra_user.clone(), typed);
                    if user.is_empty() || senha.is_empty() {
                        login_status = tr!(
                            LoginStatus::Failed("fill in the username and password".to_string()),
                            LoginStatus::Failed("preencha usuário e senha".to_string())
                        );
                    } else {
                        login_status = LoginStatus::Checking;
                        let (tx, rx) = mpsc::channel();
                        std::thread::spawn(move || {
                            let _ = tx.send(crate::ra::connect_login(&user, &senha));
                        });
                        login_worker = Some(rx);
                        login_connect = true;
                    }
                } else {
                    cfg.ra_token = typed;
                }
                let _ = cfg.save();
                if !matches!(login_status, LoginStatus::Checking) {
                    login_status = LoginStatus::Idle;
                }
                ra_editing = None;
                plat.stop_text_input(cab);
            } else if cancelled {
                ra_editing = None;
                plat.stop_text_input(cab);
            }
            // Clicks during editing do nothing (same rule as the shelf's
            // filter box): finish or cancel the field first.
            cab.set_settings_panel(SettingsPanelInfo {
                title: tr!("settings", "configurações").to_string(),
                sections: section_names().iter().map(|s| s.to_string()).collect(),
                selected: sec,
            });
            let draft_shown = ra_draft.display();
            let view = View {
                sec,
                sel,
                core_status: &core_status,
                rename_status: rename_status.as_deref(),
                login_status: &login_status,
                ra_editing,
                ra_draft: &draft_shown,
                osk: ra_editing.map(|row| (&ra_osk, &ra_draft, ra_field_title(row))),
                controls: Some((&controls, &*plat)),
            };
            cab.frame_settings(BG, |d| draw_section(d, cfg, &view));
            crate::runner::pace_ui(&mut next, frame_time, plat, cab);
            continue;
        }

        plat.set_hold_pad_order(sec == SEC_CONTROLES);
        let poll_mode = if sec == SEC_CONTROLES && controls.capturing() {
            MenuMode::CaptureKey
        } else {
            MenuMode::Nav
        };
        let m = plat.poll_menu(poll_mode);
        if m.quit {
            return Ok(true);
        }

        if sec == SEC_CONTROLES && controls.capturing() {
            // Teclado no teclado, botão no controle escolhido: a tela de
            // controles decide o que vale (e o Esc / clique cancelam).
            controls.on_capture(plat, cfg, &m);
        } else {
            // Mouse click — the panel's section buttons (and "Voltar") come
            // first, then the rows inside the tube. Every action here also
            // has a gamepad-nav path below (`m.nav`), this just adds the
            // click-driven one (plan revision: no keyboard shortcuts).
            if let Some((cx, cy)) = m.click {
                let (ox, oy) = cab.window_to_output(cx, cy);
                if cab.hit_close_button(ox, oy) {
                    return Ok(true);
                }
                if cab.hit_minimize_button(ox, oy) {
                    cab.minimize();
                    continue;
                }
                match cab.hit_settings_button(ox, oy) {
                    Some(SettingsButton::Back) => return Ok(false),
                    Some(SettingsButton::Section(i)) => {
                        sec = i.min(section_names().len() - 1);
                        sel = 0;
                        controls.cancel();
                    }
                    None => {
                        if let Some((_, ly)) = cab.hit_screen_point(ox, oy) {
                            if let Some(i) = row_at(sec, ly) {
                                if sec == SEC_CONTROLES {
                                    controls.sel = i;
                                    controls.activate(plat, cfg);
                                    continue;
                                }
                                sel = i;
                                activate_row(
                                    cab,
                                    cfg,
                                    &mut sec,
                                    &mut ra_editing,
                                    i,
                                    &mut core_status,
                                    &mut core_worker,
                                    &mut rename_status,
                                    &mut login_status,
                                    &mut login_worker,
                                );
                                if let Some(row) = ra_editing {
                                    begin_ra_edit(
                                        plat,
                                        cab,
                                        cfg,
                                        row,
                                        None,
                                        &mut ra_draft,
                                        &mut ra_osk,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            // Botão direito no usuário/token: abre o campo já colando — o
            // caminho curto do token copiado direto no browser. Só os campos
            // de texto: as outras linhas fazem coisa demais para disparar no
            // botão "errado".
            if let Some((cx, cy)) = m.right_click {
                let (ox, oy) = cab.window_to_output(cx, cy);
                if let Some((_, ly)) = cab.hit_screen_point(ox, oy) {
                    if let Some(i) = row_at(sec, ly) {
                        if sec == SEC_CONQUISTAS
                            && matches!(i, RA_ROW_USER | RA_ROW_TOKEN | RA_ROW_SENHA)
                        {
                            sel = i;
                            ra_editing = Some(i);
                            let paste = plat.paste_from_clipboard();
                            begin_ra_edit(plat, cab, cfg, i, paste, &mut ra_draft, &mut ra_osk);
                        }
                    }
                }
            }
            for nav in &m.nav {
                // LB/RB (ou LT/RT): seção anterior/próxima — todas ao alcance
                // do controle.
                if matches!(
                    nav,
                    MenuNav::PageUp | MenuNav::PageDown | MenuNav::TabPrev | MenuNav::TabNext
                ) {
                    sec = step_section(sec, matches!(nav, MenuNav::PageDown | MenuNav::TabNext));
                    sel = 0;
                    controls.cancel();
                    continue;
                }
                if sec == SEC_CONTROLES {
                    match nav {
                        MenuNav::Up => controls.sel = controls.sel.saturating_sub(1),
                        MenuNav::Down => {
                            controls.sel = (controls.sel + 1).min(controls_screen::ROWS - 1)
                        }
                        MenuNav::Left => controls.adjust(false),
                        MenuNav::Right => controls.adjust(true),
                        MenuNav::Confirm => controls.activate(plat, cfg),
                        MenuNav::Back => return Ok(false),
                        _ => {}
                    }
                    if controls.capturing() {
                        // O resto deste quadro já é captura.
                        break;
                    }
                    continue;
                }
                match nav {
                    MenuNav::Up => sel = sel.saturating_sub(1),
                    MenuNav::Down => {
                        sel = (sel + 1).min(row_count(sec).saturating_sub(1));
                    }
                    MenuNav::Back => return Ok(false),
                    MenuNav::Confirm => {
                        let cur = sel;
                        let was_editing = ra_editing.is_some();
                        activate_row(
                            cab,
                            cfg,
                            &mut sec,
                            &mut ra_editing,
                            cur,
                            &mut core_status,
                            &mut core_worker,
                            &mut rename_status,
                            &mut login_status,
                            &mut login_worker,
                        );
                        if let (false, Some(row)) = (was_editing, ra_editing) {
                            begin_ra_edit(plat, cab, cfg, row, None, &mut ra_draft, &mut ra_osk);
                        }
                    }
                    // The toggle/slider rows keep their old left/right
                    // feel; everything else ignores them.
                    MenuNav::Left => adjust_row(cfg, sec, sel, cab, false),
                    MenuNav::Right => adjust_row(cfg, sec, sel, cab, true),
                    _ => {}
                }
            }
        }

        // The flat side panel — section buttons + Voltar, rebuilt every
        // frame like the shelf's own panel.
        cab.set_settings_panel(SettingsPanelInfo {
            title: tr!("settings", "configurações").to_string(),
            sections: section_names().iter().map(|s| s.to_string()).collect(),
            selected: sec,
        });

        let draft_shown = ra_draft.display();
        let view = View {
            sec,
            sel,
            core_status: &core_status,
            rename_status: rename_status.as_deref(),
            login_status: &login_status,
            ra_editing,
            ra_draft: &draft_shown,
            osk: None,
            controls: Some((&controls, &*plat)),
        };
        cab.frame_settings(BG, |d| draw_section(d, cfg, &view));

        crate::runner::pace_ui(&mut next, frame_time, plat, cab);
    }
}

/// Act on row `i` of the current section — the click path and the gamepad's
/// Confirm both land here, so they can't drift apart.
// One row action touches every screen knob at once; splitting it into
// parameter structs just to appease the arity lint would obscure the
// actual wiring.
#[allow(clippy::too_many_arguments)]
fn activate_row(
    cab: &mut Cabinet,
    cfg: &mut Config,
    sec: &mut usize,
    ra_editing: &mut Option<usize>,
    i: usize,
    core_status: &mut CoreStatus,
    core_worker: &mut Option<Receiver<CoreUpdateMsg>>,
    rename_status: &mut Option<String>,
    login_status: &mut LoginStatus,
    login_worker: &mut Option<Receiver<Result<String, String>>>,
) {
    match *sec {
        SEC_JOGO => match i {
            0 => cfg.bad_contact = cfg.bad_contact.cycle(),
            _ => cfg.blow_mic = !cfg.blow_mic,
        },
        SEC_IMAGEM => toggle_picture_row(cfg, i, cab),
        SEC_ESTANTE => match i {
            0 => cfg.shelf_theme = cfg.shelf_theme.cycle(),
            1 => cfg.night_pick = !cfg.night_pick,
            _ => *rename_status = Some(run_rom_rename()),
        },
        SEC_SISTEMA => match i {
            0 => start_core_download(core_status, core_worker),
            1 => {
                cfg.check_updates_on_start = !cfg.check_updates_on_start;
            }
            // The language row: english ↔ português, applied live — the
            // very next frame's `tr!` calls read the new one, so this
            // screen re-renders in the chosen language at once.
            _ => {
                cfg.lang = cfg.lang.cycle();
                apply_lang(cfg);
            }
        },
        SEC_CONQUISTAS => match i {
            RA_ROW_USER | RA_ROW_TOKEN | RA_ROW_SENHA => *ra_editing = Some(i),
            3 => start_login_test(cfg, login_status, login_worker),
            _ => {
                cfg.ra_hardcore = !cfg.ra_hardcore;
            }
        },
        _ => {}
    }
}

/// Kick off the login test on a worker thread (plan fase 1) — one small
/// request, but network is network: the row shows "testando..." until the
/// `mpsc` reply lands.
fn start_login_test(
    cfg: &Config,
    status: &mut LoginStatus,
    worker: &mut Option<Receiver<Result<String, String>>>,
) {
    if matches!(status, LoginStatus::Checking) {
        return;
    }
    if cfg.ra_user.is_empty() || cfg.ra_token.is_empty() {
        *status = tr!(
            LoginStatus::Failed("fill in the username and token first".to_string()),
            LoginStatus::Failed("preencha usuário e token antes".to_string())
        );
        return;
    }
    let (tx, rx) = mpsc::channel();
    let (user, token) = (cfg.ra_user.clone(), cfg.ra_token.clone());
    std::thread::spawn(move || {
        let _ = tx.send(crate::ra::test_login(&user, &token));
    });
    *worker = Some(rx);
    *status = LoginStatus::Checking;
}

/// The left/right adjustments — toggles flip, multi-value rows step in the
/// pressed direction. Rows without a horizontal feel ignore them.
fn adjust_row(cfg: &mut Config, sec: usize, sel: usize, cab: &mut Cabinet, right: bool) {
    match (sec, sel) {
        (SEC_JOGO, 0) => {
            cfg.bad_contact = if right {
                cfg.bad_contact.cycle()
            } else {
                cfg.bad_contact.cycle_back()
            };
        }
        (SEC_JOGO, 1) => cfg.blow_mic = !cfg.blow_mic,
        (SEC_IMAGEM, 5) => {
            cfg.screen_look = if right {
                cfg.screen_look.cycle()
            } else {
                cfg.screen_look.cycle_back()
            };
            crate::apply_tv_settings(cfg, cab);
        }
        (SEC_IMAGEM, 6) => {
            cfg.screen_lines = if right {
                cfg.screen_lines.cycle()
            } else {
                cfg.screen_lines.cycle_back()
            };
            crate::apply_tv_settings(cfg, cab);
        }
        (SEC_IMAGEM, i) => toggle_picture_row(cfg, i, cab),
        (SEC_ESTANTE, 0) => cfg.shelf_theme = cfg.shelf_theme.cycle(),
        (SEC_ESTANTE, 1) => cfg.night_pick = !cfg.night_pick,
        (SEC_SISTEMA, 1) => {
            cfg.check_updates_on_start = !cfg.check_updates_on_start;
        }
        (SEC_SISTEMA, 2) => {
            cfg.lang = cfg.lang.cycle();
            apply_lang(cfg);
        }
        (SEC_CONQUISTAS, 4) => {
            cfg.ra_hardcore = !cfg.ra_hardcore;
        }
        _ => {}
    }
}

/// As linhas de "imagem e som": alternam e já valem na hora.
fn toggle_picture_row(cfg: &mut Config, row: usize, cab: &mut Cabinet) {
    match row {
        0 => {
            cfg.fullscreen = !cfg.fullscreen;
            cab.toggle_fullscreen();
        }
        1 => cfg.tv_speaker = !cfg.tv_speaker,
        2 => {
            cfg.hiss_on_static = !cfg.hiss_on_static;
            cab.set_static_hiss(cfg.hiss_on_static);
        }
        3 => cfg.flyback_whine = !cfg.flyback_whine,
        4 => cfg.burn_in = !cfg.burn_in,
        5 => cfg.screen_look = cfg.screen_look.cycle(),
        6 => cfg.screen_lines = cfg.screen_lines.cycle(),
        _ => return,
    }
    crate::apply_tv_settings(cfg, cab);
}

/// Push the config's language into the string picker — at startup the bins
/// apply the same value through `tr::set_lang`.
fn apply_lang(cfg: &Config) {
    sixteen_platform::tr::set_lang(Some(cfg.lang.tr_lang()));
}

/// The language row's value. "English"/"Português" are proper names — each
/// shown in its own language on both sides; only the row word translates.
fn lang_value_name(lang: crate::config::LangOpt) -> &'static str {
    match lang {
        crate::config::LangOpt::En => tr!("English", "Inglês"),
        crate::config::LangOpt::Pt => "Português (Brasil)",
    }
}

/// The bad-contact row's value — how often the first Power after inserting
/// a cartridge fails until you blow on it.
fn bad_contact_name(level: crate::config::BadContact) -> &'static str {
    match level {
        crate::config::BadContact::Off => tr!("off", "desligado"),
        crate::config::BadContact::Rare => tr!("rare", "raro"),
        crate::config::BadContact::Nineties => tr!("like the 90s", "anos 90"),
    }
}

/// The shelf row's value — the furniture the games stand on.
fn shelf_theme_name(theme: crate::config::ShelfTheme) -> &'static str {
    match theme {
        crate::config::ShelfTheme::Wood => tr!("wooden bookcase", "móvel de madeira"),
        crate::config::ShelfTheme::Metal => {
            tr!("metal rental-store rack", "expositor de metal da locadora")
        }
        crate::config::ShelfTheme::HiFi => tr!("black stereo rack", "rack de som preto"),
        crate::config::ShelfTheme::Memphis => tr!("90s bedroom", "quarto anos 90"),
        crate::config::ShelfTheme::Arcade => tr!("arcade", "fliperama"),
    }
}

/// Kick off a background download of the snes9x core (plan: "opção pra
/// baixar o snes9x... e opção de update do núcleo" — one action serves both,
/// the buildbot only ever serves "latest"). No-op while one is already in
/// flight. The standard background-thread + `mpsc` shape of this app —
/// `settings::run`'s loop drains it with `try_recv()` every frame.
fn start_core_download(status: &mut CoreStatus, worker: &mut Option<Receiver<CoreUpdateMsg>>) {
    if matches!(status, CoreStatus::Downloading { .. }) {
        return;
    }
    let Some(url) = core_update::core_download_url() else {
        *status = tr!(
            CoreStatus::Failed("no automatic download for this system".to_string()),
            CoreStatus::Failed("sem download automático para este sistema".to_string())
        );
        return;
    };
    let (tx, rx) = mpsc::channel();
    let dest = crate::dirs::core_dir();
    std::thread::spawn(move || core_update::download_and_install(&url, &dest, &tx));
    *worker = Some(rx);
    *status = CoreStatus::Downloading {
        downloaded: 0,
        total: None,
    };
}

/// How long since the installed core was written, in the coarse terms
/// `draw_sistema`'s row wants — `None` if there's no core installed at all.
///
/// O desenho chama a cada quadro: o `stat()` do arquivo fica em cache por
/// alguns segundos (o rótulo só muda quando um download termina, e aí a
/// linha mostra "atualizado!" antes de voltar a ele).
fn core_installed_label() -> Option<String> {
    use std::cell::RefCell;
    use std::time::Instant;
    thread_local! {
        static CACHE: RefCell<Option<(Instant, Option<String>)>> = const { RefCell::new(None) };
    }
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some((at, label)) = c.as_ref() {
            if at.elapsed() < std::time::Duration::from_secs(5) {
                return label.clone();
            }
        }
        let label = core_installed_label_uncached();
        *c = Some((Instant::now(), label.clone()));
        label
    })
}

fn core_installed_label_uncached() -> Option<String> {
    let path = crate::dirs::core_dir().join(core_update::core_file_name());
    let modified = std::fs::metadata(&path).ok()?.modified().ok()?;
    let days = modified.elapsed().unwrap_or_default().as_secs() / 86_400;
    Some(match days {
        0 => tr!("installed today", "instalado hoje").to_string(),
        1 => tr!("installed 1 day ago", "instalado há 1 dia").to_string(),
        n => tr!(
            format!("installed {n} days ago"),
            format!("instalado há {n} dias")
        ),
    })
}

/// Run the rename synchronously (plain disk renames — fast enough even for
/// a few hundred ROMs that there's no need for `start_core_download`'s
/// background-thread treatment) and turn the result into the row's next
/// label (plan: "renomear automaticamente no padrao no-intro").
fn run_rom_rename() -> String {
    let outcome = rom_rename::rename_to_nointro(
        &crate::dirs::roms_dir(),
        &crate::dirs::nointro_dat_path(),
        &crate::dirs::saves_dir(),
        &crate::dirs::notes_dir(),
        &crate::dirs::assets_dir(),
    );
    match outcome {
        RenameOutcome::Renamed(list) => {
            for r in &list {
                log::info!("rom rename: {} -> {}", r.old, r.new);
            }
            match list.len() {
                1 => tr!("1 ROM renamed", "1 ROM renomeada").to_string(),
                n => tr!(format!("{n} ROMs renamed"), format!("{n} ROMs renomeadas")),
            }
        }
        RenameOutcome::NothingToDo => tr!(
            "every ROM already has its proper name",
            "todas já têm o nome certinho"
        )
        .to_string(),
        RenameOutcome::NoDat => {
            tr!("nointro.dat not found", "nointro.dat não encontrado").to_string()
        }
    }
}

fn core_row_label(status: &CoreStatus) -> String {
    match status {
        CoreStatus::Downloading { downloaded, total } => {
            let mb = *downloaded as f64 / 1_048_576.0;
            match total {
                Some(t) => tr!(
                    format!(
                        "Core: downloading... {mb:.1}/{:.1} MB",
                        *t as f64 / 1_048_576.0
                    ),
                    format!(
                        "Núcleo: baixando... {mb:.1}/{:.1} MB",
                        *t as f64 / 1_048_576.0
                    )
                ),
                None => tr!(
                    format!("Core: downloading... {mb:.1} MB"),
                    format!("Núcleo: baixando... {mb:.1} MB")
                ),
            }
        }
        CoreStatus::Done => tr!("Core: updated!", "Núcleo: atualizado!").to_string(),
        CoreStatus::Failed(e) => tr!(
            format!("Core: update failed — {e}"),
            format!("Núcleo: falha na atualização — {e}")
        ),
        CoreStatus::Idle => match core_installed_label() {
            Some(installed) => tr!(
                format!("Core: update ({installed})"),
                format!("Núcleo: atualizar ({installed})")
            ),
            None => tr!("Core: download", "Núcleo: baixar").to_string(),
        },
    }
}

/// Tudo o que o desenho de uma seção lê num quadro.
struct View<'a> {
    sec: usize,
    sel: usize,
    core_status: &'a CoreStatus,
    rename_status: Option<&'a str>,
    login_status: &'a LoginStatus,
    ra_editing: Option<usize>,
    ra_draft: &'a str,
    /// O teclado na tela do campo em edição: estado, campo e título.
    osk: Option<(&'a Osk, &'a TextField, &'static str)>,
    controls: Option<(&'a ControlsScreen, &'a Platform)>,
}

fn draw_section(d: &mut Screen, cfg: &Config, v: &View) {
    draw_section_body(d, cfg, v);
    // A legenda dos botões do controle (todas as seções) e, em edição, o
    // teclado na tela por cima.
    if v.osk.is_none() {
        draw_pad_legend(d, v.sec);
    }
    if let Some((osk, field, title)) = v.osk {
        osk.draw(d, field, title);
    }
}

/// A legenda de botões, uma linha acima da dica de cada seção — o mesmo
/// estilo (`HINT`, escala 1). LB/RB trocam de seção; o resto é por linha.
fn draw_pad_legend(d: &mut Screen, sec: usize) {
    let text = match sec {
        SEC_CONTROLES => tr!(
            "Up/Down row  Left/Right adjust  A change  LB/RB section  B back",
            "Cima/Baixo linha  Esq./Dir. ajusta  A muda  LB/RB seção  B volta"
        ),
        _ => tr!(
            "Up/Down row  Left/Right adjust  A change/open  LB/RB section  B back",
            "Cima/Baixo linha  Esq./Dir. ajusta  A muda/abre  LB/RB seção  B volta"
        ),
    };
    d.text(MARGIN, d.size().1 as i32 - MARGIN - 22, 1, DIM, text);
}

fn draw_section_body(d: &mut Screen, cfg: &Config, v: &View) {
    match v.sec {
        SEC_CONTROLES => {
            d.text(MARGIN, MARGIN, 2, TEXT, tr!("controls", "controles"));
            if let Some((controls, plat)) = v.controls {
                controls.draw(d, plat, cfg, LIST_TOP, MARGIN);
            }
            draw_hint(
                d,
                tr!(
                    "pick keyboard or a controller in the first row -- click a button to change it",
                    "escolha teclado ou controle na primeira linha -- clique num botão pra trocar"
                ),
            );
        }
        SEC_IMAGEM => draw_imagem(d, cfg, v.sel),
        SEC_ESTANTE => draw_estante(d, cfg, v.sel, v.rename_status),
        SEC_SISTEMA => draw_sistema(d, cfg, v.sel, v.core_status),
        SEC_CONQUISTAS => draw_conquistas(d, cfg, v.sel, v.login_status, v.ra_editing, v.ra_draft),
        _ => draw_jogo(d, cfg, v.sel),
    }
}

/// O título e as linhas de uma seção simples.
fn draw_rows(d: &mut Screen, title: &str, rows: &[String], sel: usize) {
    d.text(MARGIN, MARGIN, 2, TEXT, title);
    let mut y = LIST_TOP;
    for (i, row) in rows.iter().enumerate() {
        draw_row(d, MARGIN, y, row, i == sel);
        y += ROW_H;
    }
}

fn draw_jogo(d: &mut Screen, cfg: &Config, sel: usize) {
    let rows = [
        tr!(
            format!(
                "Cartridge bad contact: {}",
                bad_contact_name(cfg.bad_contact)
            ),
            format!(
                "Mau contato do cartucho: {}",
                bad_contact_name(cfg.bad_contact)
            ),
        ),
        tr!(
            format!("Blow into the microphone: {}", on_off(cfg.blow_mic)),
            format!("Soprar no microfone: {}", on_off(cfg.blow_mic)),
        ),
    ];
    draw_rows(d, tr!("game", "jogo"), &rows, sel);
    draw_hint(
        d,
        tr!(
            "click an option to change it -- gamepad arrows adjust, A confirms",
            "clique numa opção pra mudar -- setas do gamepad ajustam, A confirma"
        ),
    );
}

/// "ligado"/"desligado" (ou on/off) para as linhas de liga-desliga.
fn on_off(v: bool) -> &'static str {
    if v {
        tr!("on", "ligado")
    } else {
        tr!("off", "desligado")
    }
}

fn draw_imagem(d: &mut Screen, cfg: &Config, sel: usize) {
    let rows = [
        tr!(
            format!("Fullscreen: {}", if cfg.fullscreen { "on" } else { "off" }),
            format!(
                "Tela cheia: {}",
                if cfg.fullscreen {
                    "ligada"
                } else {
                    "desligada"
                }
            )
        ),
        tr!(
            format!(
                "Game sound: {}",
                if cfg.tv_speaker {
                    "TV speaker (mono)"
                } else {
                    "3-in-1 stereo"
                }
            ),
            format!(
                "Som do jogo: {}",
                if cfg.tv_speaker {
                    "alto-falante da TV (mono)"
                } else {
                    "3 em 1 estéreo"
                }
            )
        ),
        tr!(
            format!("TV static hiss: {}", on_off(cfg.hiss_on_static)),
            format!("Chiado da TV fora do ar: {}", on_off(cfg.hiss_on_static))
        ),
        tr!(
            format!("Flyback whine (~15 kHz): {}", on_off(cfg.flyback_whine)),
            format!("Chiado do flyback (~15 kHz): {}", on_off(cfg.flyback_whine))
        ),
        tr!(
            format!("Tube burn-in: {}", on_off(cfg.burn_in)),
            format!("Burn-in do tubo: {}", on_off(cfg.burn_in))
        ),
        tr!(
            format!("Screen look: < {} >", cfg.screen_look.label()),
            format!("Visual da tela: < {} >", cfg.screen_look.label())
        ),
        tr!(
            format!("Lines & mask: < {} >", cfg.screen_lines.label()),
            format!("Linhas e máscara: < {} >", cfg.screen_lines.label())
        ),
    ];
    draw_rows(d, tr!("picture & sound", "imagem e som"), &rows, sel);
    draw_hint(
        d,
        tr!(
            "brightness, contrast, color and sharpness: the TV's MENU button",
            "brilho, contraste, cor e nitidez: no botão MENU da TV"
        ),
    );
}

fn draw_estante(d: &mut Screen, cfg: &Config, sel: usize, rename_status: Option<&str>) {
    let rows = [
        tr!(
            format!("Theme: {}", shelf_theme_name(cfg.shelf_theme)),
            format!("Tema: {}", shelf_theme_name(cfg.shelf_theme)),
        ),
        tr!(
            format!("Tonight's pick: {}", on_off(cfg.night_pick)),
            format!("Sugestão da noite: {}", on_off(cfg.night_pick)),
        ),
        rename_status
            .unwrap_or(tr!(
                "Rename ROMs to No-Intro names",
                "Renomear ROMs para nomes No-Intro"
            ))
            .to_string(),
    ];
    draw_rows(d, tr!("shelf", "estante"), &rows, sel);
    draw_hint(
        d,
        tr!(
            "click an option to change or run it",
            "clique numa opção pra mudar ou executar"
        ),
    );
}

fn draw_sistema(d: &mut Screen, cfg: &Config, sel: usize, core_status: &CoreStatus) {
    let rows = [
        core_row_label(core_status),
        tr!(
            format!(
                "Check for updates on launch: {}",
                if cfg.check_updates_on_start {
                    "yes"
                } else {
                    "no"
                }
            ),
            format!(
                "Verificar atualizações ao abrir: {}",
                if cfg.check_updates_on_start {
                    "sim"
                } else {
                    "não"
                }
            )
        ),
        tr!(
            format!("Language: {}", lang_value_name(cfg.lang)),
            format!("Idioma: {}", lang_value_name(cfg.lang)),
        ),
    ];
    draw_rows(d, tr!("system", "sistema"), &rows, sel);
    draw_hint(
        d,
        tr!(
            "click an option to run or toggle it",
            "clique numa opção pra executar ou alternar"
        ),
    );
}

fn draw_conquistas(
    d: &mut Screen,
    cfg: &Config,
    sel: usize,
    login: &LoginStatus,
    editing: Option<usize>,
    draft: &str,
) {
    let x = MARGIN;
    d.text(x, MARGIN, 2, TEXT, tr!("achievements", "conquistas"));
    let field = |row: usize, label: &str, value: &str| {
        if editing == Some(row) {
            format!("{label}: {draft}_")
        } else if value.is_empty() {
            tr!(format!("{label}: (empty)"), format!("{label}: (vazio)"))
        } else {
            // Segredos (token, senha) não voltam para a tela, nem
            // mascarados — só a certeza de que estão lá.
            if row == RA_ROW_TOKEN {
                tr!(format!("{label}: (set)"), format!("{label}: (configurado)"))
            } else {
                format!("{label}: {value}")
            }
        }
    };
    let senha_label = if cfg.ra_connect.is_some() {
        tr!("Password: (connected)", "Senha: (conectado)").to_string()
    } else {
        tr!(
            "Password: (empty — achievements stay off)",
            "Senha: (vazio — conquistas ficam desligadas)"
        )
        .to_string()
    };
    let login_label = match login {
        LoginStatus::Idle => {
            if cfg.ra_user.is_empty() && cfg.ra_token.is_empty() {
                tr!("Test login", "Testar login").to_string()
            } else {
                tr!("Test login (not tested yet)", "Testar login (não testado)").to_string()
            }
        }
        LoginStatus::Checking => {
            tr!("Test login: testing...", "Testar login: testando...").to_string()
        }
        LoginStatus::Ok(l) => tr!(
            format!("Test login: ok — {l}"),
            format!("Testar login: ok — {l}")
        ),
        LoginStatus::Failed(e) => tr!(format!("Test login: {e}"), format!("Testar login: {e}")),
    };
    let rows = [
        field(RA_ROW_USER, tr!("Username", "Usuário"), &cfg.ra_user),
        field(
            RA_ROW_TOKEN,
            tr!("Web API key", "Chave da web API"),
            &cfg.ra_token,
        ),
        if editing == Some(RA_ROW_SENHA) {
            format!("{}: {draft}_", tr!("Password", "Senha"))
        } else {
            senha_label
        },
        login_label,
        tr!(
            format!(
                "Hardcore mode: {}",
                if cfg.ra_hardcore {
                    tr!(
                        "on (no cheats/savestates)",
                        "ligado (sem cheats/savestates)"
                    )
                } else {
                    tr!("off", "desligado")
                }
            ),
            format!(
                "Modo hardcore: {}",
                if cfg.ra_hardcore {
                    "ligado (sem cheats/savestates)"
                } else {
                    "desligado"
                }
            )
        ),
    ];
    let mut y = LIST_TOP;
    for (i, row) in rows.iter().enumerate() {
        draw_row(d, x, y, row, i == sel);
        y += ROW_H;
    }
    d.text(
        x,
        y + 8,
        1,
        DIM,
        "key: retroachievements.org -> settings -> keys",
    );
    d.text(
        x,
        y + 8 + 20,
        1,
        DIM,
        tr!(
            "password: the same one from the site — links your account so achievements unlock and sync",
            "senha: a mesma do site — conecta a conta para desbloquear e sincronizar conquistas"
        ),
    );
    draw_hint(
        d,
        tr!(
            "click to edit, right-click to paste, cmd+c to copy -- enter confirms, esc cancels",
            "clique edita, botão direito cola, cmd+c copia -- enter confirma, esc cancela"
        ),
    );
}

/// The one-line help at the bottom of the tube, inside it like the shelf's
/// own footer text (shared by every section so the sections read the same).
fn draw_hint(d: &mut Screen, s: &str) {
    d.text(MARGIN, d.size().1 as i32 - MARGIN, 1, HINT, s);
}

/// Headless preview of one section (dev/testing) — not part of the
/// interactive `run` loop. Accepts the section names, plus the old
/// "main"/"controls" aliases.
pub fn capture_preview(
    plat: &Platform,
    cab: &mut Cabinet,
    cfg: &Config,
    screen: &str,
    path: &std::path::Path,
) -> Result<()> {
    let sec = match screen {
        "jogo" | "game" | "main" => SEC_JOGO,
        "video" | "imagem" => SEC_IMAGEM,
        "estante" | "shelf" => SEC_ESTANTE,
        "sistema" => SEC_SISTEMA,
        "conquistas" | "conquistas-osk" | "osk" => SEC_CONQUISTAS,
        _ => SEC_CONTROLES,
    };
    // `--debug-settings osk`: a senha sendo digitada no teclado na tela.
    let preview_osk = matches!(screen, "conquistas-osk" | "osk");
    let mut osk = Osk::new();
    osk.begin(true);
    let field = TextField::new("senha123", RA_FIELD_LIMIT, true);
    let shown = field.display();
    cab.set_settings_panel(SettingsPanelInfo {
        title: tr!("settings", "configurações").to_string(),
        sections: section_names().iter().map(|s| s.to_string()).collect(),
        selected: sec,
    });
    let controls = ControlsScreen::new();
    let view = View {
        sec,
        sel: if preview_osk { RA_ROW_SENHA } else { 0 },
        core_status: &CoreStatus::Idle,
        rename_status: None,
        login_status: &LoginStatus::Idle,
        ra_editing: preview_osk.then_some(RA_ROW_SENHA),
        ra_draft: if preview_osk { &shown } else { "" },
        osk: preview_osk.then(|| (&osk, &field, ra_field_title(RA_ROW_SENHA))),
        controls: Some((&controls, plat)),
    };
    cab.set_close_button(true);
    cab.capture_settings(BG, |d| draw_section(d, cfg, &view), path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

fn draw_row(d: &mut Screen, x: i32, y: i32, label: &str, selected: bool) {
    let (cursor, color) = if selected { ("> ", TEXT) } else { ("  ", DIM) };
    d.text(x, y, 1, color, &format!("{cursor}{label}"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shoulders_walk_every_section_and_wrap() {
        let n = section_names().len();
        // RB passa por todas as seções e volta ao começo.
        let mut seen = vec![SEC_CONTROLES];
        let mut sec = SEC_CONTROLES;
        for _ in 0..n {
            sec = step_section(sec, true);
            seen.push(sec);
        }
        assert_eq!(seen.last(), Some(&SEC_CONTROLES));
        for want in [
            SEC_CONTROLES,
            SEC_JOGO,
            SEC_IMAGEM,
            SEC_ESTANTE,
            SEC_CONQUISTAS,
            SEC_SISTEMA,
        ] {
            assert!(seen.contains(&want), "seção {want} inalcançável");
        }
        // LB é o inverso, com volta no início.
        assert_eq!(step_section(SEC_CONTROLES, false), n - 1);
        assert_eq!(step_section(step_section(SEC_JOGO, true), false), SEC_JOGO);
    }

    #[test]
    fn every_ra_row_opens_the_text_editor_by_pad() {
        // O campo de senha é o único mascarado.
        let f = TextField::new("segredo", RA_FIELD_LIMIT, true);
        assert_eq!(f.display(), "*******");
        assert_eq!(row_count(SEC_CONQUISTAS), 5);
    }
}
