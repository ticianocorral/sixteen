//! The idle/root screen: TV off, no game loaded — the app's home state, and
//! also literally the "cartridge ejected" screen (plan revision: one screen,
//! not two — startup, backing out of the shelf, and ejecting a game all land
//! here). In place of a game's logo/cartridge art there's the console's own
//! brand logo (`assets/console.png`, optional) and an "Inserir cartucho"
//! button; "Configurações" sits in the panel's footer. This is the outermost
//! screen now: a gamepad's Back button here, same as closing the window,
//! ends the app (plan revision: mouse/gamepad only, no keyboard shortcuts).
//!
//! First-run setup (plan revision: "ao abrir pela primeira vez e não ter o
//! DAT, o snes9x — abrir dentro da tv o aviso para baixar os dois arquivos,
//! com o botão para baixar"): when either the snes9x core or the
//! `nointro.dat` is missing, the TV shows a full setup screen with a
//! download button for each, before the usual idle panel. Updates (plan
//! revision: "quando tiver update do app ou do snes9x não mostrar mais a
//! tela cheia e sim um icone verde no nameplate do lado de cada um") are no
//! longer a modal — the startup check's result lights a green dot in the
//! chin, next to the app's and the core's version lines.

use std::path::Path;
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use std::sync::mpsc;

use anyhow::Result;
#[allow(unused_imports)]
use sixteen_platform::tr;
use sixteen_platform::{
    Cabinet, MenuMode, MenuNav, PadFocus, PanelButton, Platform, Screen, TvControl, UpdateArrow,
};

use crate::core_update::{self, CoreUpdateMsg};
use crate::update_check::UpdateNotice;

const SETUP_BG: (u8, u8, u8) = (18, 18, 20);
const SETUP_TEXT: (u8, u8, u8) = (232, 232, 232);
const SETUP_DIM: (u8, u8, u8) = (150, 150, 158);
const SETUP_GREEN: (u8, u8, u8) = (60, 230, 70);

/// O manual do console (plan revision: "fazer um botao em cima das
/// configurações na tela inicial para visualizar esse pdf"), um por
/// idioma — gerados por
/// `packaging/manual-console/gerar-manual.py`.
const CONSOLE_MANUAL_EN: &[u8] = include_bytes!("../assets/console-manual-en.pdf");
const CONSOLE_MANUAL_PT: &[u8] = include_bytes!("../assets/console-manual-pt.pdf");

/// O leitor do manual do console aberto no tubo — o mesmo leitor dos
/// manuais da estante (`shelf::draw_manual_view`): páginas renderizadas sob
/// demanda por um worker, que vivem como texturas sob `key`.
struct ConsoleManual {
    /// A chave das texturas das páginas (o "sha1" do leitor da estante) —
    /// uma por idioma, para a troca de idioma não reaproveitar páginas.
    key: &'static str,
    pages: usize,
    page: usize,
    /// O worker não conseguiu abrir o PDF (morreu sem dizer quantas
    /// páginas tem) — a tela avisa em vez de ficar "abrindo" para sempre.
    failed: bool,
    requested: std::collections::HashSet<usize>,
    rx: Receiver<crate::manual::ManualEv>,
    tx: mpsc::Sender<usize>,
    rects: (
        crate::shelf::ViewBtn,
        crate::shelf::ViewBtn,
        crate::shelf::ViewBtn,
    ),
}

impl ConsoleManual {
    /// Grava o PDF do idioma corrente na pasta temporária (o renderizador
    /// lê de arquivo) e sobe o worker. `None` = não deu para gravar.
    fn open() -> Option<Self> {
        let (key, bytes) = tr!(
            ("console-manual-en", CONSOLE_MANUAL_EN),
            ("console-manual-pt", CONSOLE_MANUAL_PT)
        );
        // Na pasta do app, não no temp: no Linux o `/tmp` é compartilhado e
        // um arquivo de mesmo nome e tamanho deixado por outro usuário seria
        // aberto pelo renderizador de PDF.
        let dir = crate::dirs::app_root().join("cache");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{key}.pdf"));
        // Já gravado numa abertura anterior (mesmo tamanho): não regrava os
        // ~1,6 MB na thread de UI a cada clique.
        let fresh = std::fs::metadata(&path).is_ok_and(|m| m.len() == bytes.len() as u64);
        if !fresh {
            if let Err(e) = std::fs::write(&path, bytes) {
                log::warn!("console manual: writing {}: {e}", path.display());
                return None;
            }
        }
        let (ev_tx, rx) = mpsc::channel();
        let (tx, req_rx) = mpsc::channel::<usize>();
        std::thread::spawn(move || {
            let pages = match crate::manual::page_count(&path) {
                Ok(n) => n,
                Err(e) => {
                    log::warn!("console manual: {e}");
                    return;
                }
            };
            let _ = ev_tx.send(crate::manual::ManualEv::Ready {
                sha1: key.to_string(),
                pages,
            });
            for page in req_rx {
                let (w, h, rgba) = match crate::manual::render_page(&path, page, 2048) {
                    Ok(Some(img)) => (img.w, img.h, img.rgba),
                    _ => (0, 0, Vec::new()),
                };
                let _ = ev_tx.send(crate::manual::ManualEv::Page {
                    sha1: key.to_string(),
                    page,
                    w,
                    h,
                    rgba,
                });
            }
        });
        Some(Self {
            key,
            pages: 0,
            page: 1,
            failed: false,
            requested: std::collections::HashSet::new(),
            rx,
            tx,
            rects: ((0, 0, 1, 1), (0, 0, 1, 1), (0, 0, 1, 1)),
        })
    }

    /// Eventos do worker (contagem e páginas viram texturas) e o pedido da
    /// página corrente e da seguinte assim que faltam.
    fn poll(&mut self, cab: &mut Cabinet) {
        loop {
            let ev = match self.rx.try_recv() {
                Ok(ev) => ev,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.failed = self.pages == 0;
                    break;
                }
            };
            match ev {
                crate::manual::ManualEv::Ready { pages, .. } => self.pages = pages,
                crate::manual::ManualEv::Page {
                    page, w, h, rgba, ..
                } => {
                    if w > 0 {
                        cab.set_image(crate::shelf::manual_page_id(self.key, page), w, h, &rgba);
                    } else {
                        log::warn!("console manual: page {page} didn't render");
                    }
                }
            }
        }
        for page in [self.page, self.page + 1] {
            // Página já em textura (de uma abertura anterior): não renderiza
            // de novo.
            if self.pages > 0
                && page <= self.pages
                && !cab.has_image(crate::shelf::manual_page_id(self.key, page))
                && self.requested.insert(page)
            {
                let _ = self.tx.send(page);
            }
        }
    }

    fn prev(&mut self) {
        self.page = self.page.saturating_sub(1).max(1);
    }

    fn next(&mut self) {
        if self.page < self.pages {
            self.page += 1;
        }
    }
}

/// The screen's resting static level — startup and "Esc on the shelf" have no
/// prior game to inherit a level from, so they use this (same dim hiss the
/// console settles on after powering off, `runner::OFF_STATIC_LEVEL`).
pub const RESTING_STATIC: f32 = crate::runner::OFF_STATIC_LEVEL;

/// What the player did on the idle screen.
pub enum IdleExit {
    /// Window closed / Cmd-Q, or Esc — this is the root screen, so both end
    /// the app.
    Quit,
    /// "Inserir cartucho" confirmed (Enter/gamepad) or clicked.
    OpenShelf,
    /// "Configurações" clicked — the only way into settings from here (no
    /// keyboard shortcut, mouse/gamepad only).
    OpenSettings,
    /// "Dev" clicked — the dev-mode button that only exists once the Konami
    /// code has been entered on this screen (plan revision: "ultra secreto").
    OpenDev,
}

/// One clickable region of the setup screen, in output coordinates —
/// `(x, y, w, h)` in the same space `Cabinet::window_to_output` maps clicks
/// into, so `hit_setup_button` and the draw closure stay in sync by both
/// coming from `setup_rects`.
type RectT = (i32, i32, u32, u32);

/// What the player can click on the setup screen.
enum SetupButton {
    Core,
    Dat,
    Continue,
}

/// Button/status geometry for the setup screen, derived from the output
/// size — the single source both `draw_setup` and `hit_setup_button` use.
fn setup_rects(w: u32, h: u32) -> (RectT, RectT, RectT) {
    let bw = 640.min(w.saturating_sub(160)).max(280);
    let bh = 64u32;
    let x = (w as i32 - bw as i32) / 2;
    let core_y = h as i32 * 34 / 100;
    let dat_y = core_y + bh as i32 + 44;
    let cw = 280u32;
    let ch = 56u32;
    let cont = (
        (w as i32 - cw as i32) / 2,
        h as i32 - ch as i32 - 72,
        cw,
        ch,
    );
    ((x, core_y, bw, bh), (x, dat_y, bw, bh), cont)
}

fn hit_setup_button(w: u32, h: u32, x: i32, y: i32) -> Option<SetupButton> {
    let (core, dat, cont) = setup_rects(w, h);
    let inside =
        |r: &RectT| x >= r.0 && y >= r.1 && (x - r.0) < r.2 as i32 && (y - r.1) < r.3 as i32;
    if inside(&core) {
        Some(SetupButton::Core)
    } else if inside(&dat) {
        Some(SetupButton::Dat)
    } else if inside(&cont) {
        Some(SetupButton::Continue)
    } else {
        None
    }
}

/// Download state for one setup button — `label` carries the live progress
/// /failure text, `rx` is `Some` while the worker thread runs.
struct Download {
    label: String,
    rx: Option<mpsc::Receiver<CoreUpdateMsg>>,
    done: bool,
    /// The last run failed (`label` tem o motivo) — o idle usa para manter
    /// o status visível e a seta verde acesa para tentar de novo.
    failed: bool,
}

impl Download {
    fn idle(label: &str) -> Self {
        Self {
            label: label.to_string(),
            rx: None,
            done: false,
            failed: false,
        }
    }

    /// Drain the worker's channel, if one is wired up — the same pump
    /// `idle::run` used to inline for the core alone.
    fn pump(&mut self) {
        let Some(rx) = &self.rx else { return };
        match rx.try_recv() {
            Ok(CoreUpdateMsg::Progress { downloaded, total }) => {
                let mb = downloaded as f64 / 1_048_576.0;
                self.label = match total {
                    Some(t) => tr!(
                        format!("downloading... {mb:.1}/{:.1} MB", t as f64 / 1_048_576.0),
                        format!("baixando... {mb:.1}/{:.1} MB", t as f64 / 1_048_576.0)
                    ),
                    None => tr!(
                        format!("downloading... {mb:.1} MB"),
                        format!("baixando... {mb:.1} MB")
                    ),
                };
            }
            Ok(CoreUpdateMsg::Done) => {
                self.done = true;
                self.rx = None;
                self.failed = false;
                self.label = tr!("installed".to_string(), "instalado".to_string());
            }
            Ok(CoreUpdateMsg::Failed(e)) => {
                self.label = tr!(
                    format!("failed to update the core: {e}"),
                    format!("falha ao atualizar o núcleo: {e}")
                );
                self.rx = None;
                self.failed = true;
            }
            Err(TryRecvError::Disconnected) => self.rx = None,
            Err(TryRecvError::Empty) => {}
        }
    }
}

impl Download {
    /// Começa o download do núcleo (o caminho do controle — o do mouse tem a
    /// mesma lógica inline). `panel`: o botão/seta do painel, que também
    /// reabre o estado "baixando" mesmo depois de instalado.
    fn start_core(&mut self, panel: bool) {
        if self.rx.is_some() {
            return;
        }
        match core_update::core_download_url() {
            Some(url) => {
                let (tx, rx) = mpsc::channel();
                let dest = crate::dirs::core_dir();
                std::thread::spawn(move || core_update::download_and_install(&url, &dest, &tx));
                self.rx = Some(rx);
                self.failed = false;
                if panel {
                    self.done = false;
                    self.label = tr!("downloading...".to_string(), "baixando...".to_string());
                }
            }
            None => {
                self.label = tr!(
                    "not available for this system — check settings".to_string(),
                    "indisponível neste sistema — veja as configurações".to_string()
                );
            }
        }
    }

    /// Começa o download do `nointro.dat`.
    fn start_dat(&mut self) {
        if self.rx.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let dest = crate::dat_update::dat_path();
        std::thread::spawn(move || crate::dat_update::download_and_install(&dest, &tx));
        self.rx = Some(rx);
    }
}

/// Um alvo do foco do controle na tela inicial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdleFocus {
    Panel(PanelButton),
    Arrow(UpdateArrow),
    /// O botão POWER da TV — sem ele, quem só tem controle não liga a TV.
    Power,
}

/// Os alvos que o controle percorre, em ordem: botões do painel (em ordem
/// de leitura), as setas verdes de update e, por último, o POWER da TV.
/// `drawn` é a lista de botões que o painel desenhou no último quadro.
fn idle_items(drawn: &[PanelButton], app_update: bool, core_stale: bool) -> Vec<IdleFocus> {
    let mut v: Vec<IdleFocus> = drawn
        .iter()
        .filter(|b| {
            matches!(
                b,
                PanelButton::Insert
                    | PanelButton::Settings
                    | PanelButton::ConsoleManual
                    | PanelButton::Dev
                    | PanelButton::CoreDownload
            )
        })
        .map(|b| IdleFocus::Panel(*b))
        .collect();
    if app_update {
        v.push(IdleFocus::Arrow(UpdateArrow::App));
    }
    if core_stale {
        v.push(IdleFocus::Arrow(UpdateArrow::Core));
    }
    v.push(IdleFocus::Power);
    v
}

/// O foco efetivo: o escolhido, se ainda existe; senão o padrão — "Inserir
/// cartucho", ou o download do núcleo quando falta, ou o primeiro da lista.
fn resolve_focus(items: &[IdleFocus], chosen: Option<IdleFocus>) -> IdleFocus {
    if let Some(f) = chosen.filter(|f| items.contains(f)) {
        return f;
    }
    [
        IdleFocus::Panel(PanelButton::Insert),
        IdleFocus::Panel(PanelButton::CoreDownload),
    ]
    .into_iter()
    .find(|f| items.contains(f))
    .or_else(|| items.first().copied())
    .unwrap_or(IdleFocus::Power)
}

/// Passo do foco: Cima/Esquerda para trás, Baixo/Direita para frente, com
/// volta nas pontas.
fn step_focus(items: &[IdleFocus], cur: IdleFocus, forward: bool) -> IdleFocus {
    let n = items.len().max(1);
    let at = items.iter().position(|f| *f == cur).unwrap_or(0);
    items[if forward {
        (at + 1) % n
    } else {
        (at + n - 1) % n
    }]
}

/// Run the idle screen until the player opens the shelf or quits.
/// `static_level` is the signal-off snow to show — the steady dim hiss at
/// startup or after Esc on the shelf, or whatever level a just-ejected game
/// settled on (already steady by the time Eject fires, so no extra fade is
/// needed here either way). `notice_rx` is the startup update-check's
/// channel — `None` once the check has reported (its result went straight
/// to the nameplate's green dots) or found nothing.
pub fn run(
    plat: &mut Platform,
    cab: &mut Cabinet,
    static_level: f32,
    notice_rx: &mut Option<Receiver<UpdateNotice>>,
    core_installed: bool,
    dat_installed: bool,
    _catalog_counts: usize,
) -> Result<IdleExit> {
    // Whatever game was loaded before (if any) is gone now — without this the
    // panel keeps showing its stale logo/commands instead of the "Inserir
    // cartucho" button, and there'd be nothing to click.
    cab.clear_panel();
    cab.set_close_button(true);
    crate::console_art::load_brand_images(cab);
    // The idle screen is where this app spends most of its life — pace it
    // (it used to spin unthrottled, redrawing 60fps+ of static forever).
    let frame = crate::runner::UI_FRAME;
    let mut next = Instant::now() + frame;

    // First-run setup (plan revision: see module docs): the TV shows the
    // two download buttons until the player hits "continuar". After that
    // the plain idle panel takes over (with the old core prompt if the
    // core is still missing).
    let mut setup = core_missing_at_start(core_installed, dat_installed);
    let mut core = Download::idle(tr!("download the snes9x core", "baixar núcleo snes9x"));
    if core_installed {
        core.done = true;
        core.label = tr!("installed".to_string(), "instalado".to_string());
    }
    let mut dat = Download::idle(tr!("download nointro.dat", "baixar nointro.dat"));
    if dat_installed {
        dat.done = true;
        dat.label = tr!("installed".to_string(), "instalado".to_string());
    }
    let mut core_missing = !core_installed;
    // Feedback do download do núcleo na seta verde (plan revision: "cliquei
    // no icone verde de update do nucleo mas nao abriu nada"): a confirmação
    // fica alguns segundos no painel depois que instala.
    let mut core_ok_until: Option<Instant> = None;
    // Updates (plan revision: "ao clicar no icone verde de atualizar"):
    // o notice é GUARDADO (o changelog/asset o leitor da tela usa), a
    // aba de update do app abre a tela com changelog + botão, e o core
    // baixa na hora. `core_stale` controla a seta até o download valer.
    let mut app_update: Option<crate::update_check::AppUpdate> = None;
    let mut core_stale = false;
    let mut update_view: Option<crate::update_check::AppUpdate> = None;
    let mut update_rx: Option<Receiver<Result<String, String>>> = None;
    // Aplicação do pacote já baixado (1.1.4: na hora, não no arranque
    // seguinte) — `update_applied` liga o botão "reiniciar agora".
    let mut apply_rx: Option<Receiver<Result<(), String>>> = None;
    let mut update_applied = false;
    let mut update_progress_rx: Option<Receiver<u32>> = None;
    let mut update_pct: Option<u32> = None;
    let mut update_status = String::new();
    let mut update_rects: (UpdateBtn, UpdateBtn) = ((0, 0, 1, 1), (0, 0, 1, 1));
    let mut console_manual: Option<ConsoleManual> = None;
    // Jogar só com o controle: o foco da tela de setup (0 núcleo, 1 DAT,
    // 2 continuar), o da tela inicial, a confirmação de sair (B) e se a
    // legenda dos botões já apareceu nesta visita.
    let mut setup_focus: usize = if !core_installed {
        0
    } else if !dat_installed {
        1
    } else {
        2
    };
    let mut idle_focus: Option<IdleFocus> = None;
    let mut confirm_quit = false;
    let mut legend_shown = false;

    loop {
        let core_was_done = core.done;
        core.pump();
        dat.pump();
        // A core download just finished (setup screen or panel button) —
        // rebuild the nameplate right away, so the snes9x line appears
        // without waiting for the player to leave this screen.
        if core.done && !core_was_done {
            let p = core_update::default_core_path();
            cab.set_nameplate(&core_update::nameplate_text(p.as_deref()));
            // O app reconhece a nova versão na hora — e a seta do core
            // apaga (o meta do install já vale para o próximo arranque).
            if core_stale {
                core_stale = false;
                cab.set_nameplate_updates(app_update.is_some(), false);
            }
            core_ok_until = Some(Instant::now() + Duration::from_secs(4));
        }
        if core.rx.is_none() && core.done {
            core_missing = false;
        }
        if let Some(rx) = notice_rx.as_ref() {
            match rx.try_recv() {
                // Updates aren't a modal anymore (plan revision) — the
                // result just lights the green dots in the nameplate: line
                // 0 for the app, line 1 for the core.
                Ok(n) => {
                    cab.set_nameplate_updates(n.app_update.is_some(), n.core_stale);
                    app_update = n.app_update;
                    core_stale = n.core_stale;
                    *notice_rx = None;
                }
                Err(TryRecvError::Disconnected) => *notice_rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }

        // Download do update do app em andamento: percentual vivo e o
        // final ("será atualizado ao reiniciar") no rótulo da tela.
        while let Some(rx) = update_progress_rx.take() {
            match rx.try_recv() {
                Ok(pct) => {
                    update_pct = Some(pct);
                    update_status = tr!(
                        format!("downloading... {pct}%"),
                        format!("baixando... {pct}%")
                    );
                    update_progress_rx = Some(rx);
                    break;
                }
                Err(TryRecvError::Empty) => {
                    update_progress_rx = Some(rx);
                    break;
                }
                Err(TryRecvError::Disconnected) => {}
            }
        }
        if let Some(rx) = &update_rx {
            match rx.try_recv() {
                Ok(Ok(file)) => {
                    // Baixou: aplica na hora (o apply é seguro com o app
                    // rodando — macOS troca o bundle, o inode vivo segue) e
                    // oferece o relançamento; falha cai no apply do
                    // arranque seguinte, que continua no lugar.
                    update_status = tr!(
                        format!("{file} downloaded — applying..."),
                        format!("{file} baixado — aplicando...")
                    );
                    update_pct = None;
                    update_rx = None;
                    let (tx, rx) = mpsc::channel();
                    let path = crate::dirs::update_dir().join(&file);
                    std::thread::spawn(move || {
                        let _ = tx.send(crate::update_check::apply_update_file(&path));
                    });
                    apply_rx = Some(rx);
                }
                Ok(Err(e)) => {
                    update_status = tr!(format!("failed: {e}"), format!("falhou: {e}"));
                    update_rx = None;
                }
                Err(TryRecvError::Disconnected) => update_rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &apply_rx {
            match rx.try_recv() {
                Ok(Ok(())) => {
                    let tag = update_view
                        .as_ref()
                        .map(|u| u.tag.clone())
                        .unwrap_or_default();
                    update_status = tr!(
                        format!("updated to {tag} — restart to apply"),
                        format!("atualizado para {tag} — reinicie para aplicar")
                    );
                    update_applied = true;
                    apply_rx = None;
                }
                Ok(Err(e)) => {
                    update_status = tr!(
                        format!(
                            "downloaded, but couldn't apply it ({e}) — will try again on restart"
                        ),
                        format!("baixado, mas não aplicou ({e}) — tenta de novo ao reiniciar")
                    );
                    apply_rx = None;
                }
                Err(TryRecvError::Disconnected) => apply_rx = None,
                Err(TryRecvError::Empty) => {}
            }
        }

        let m = plat.poll_menu(MenuMode::Nav);
        if m.quit {
            return Ok(IdleExit::Quit);
        }
        // O código Konami (↑↑↓↓←→←→BA — teclado ou controle) completa aqui:
        // religa o devmode da sessão e o painel passa a mostrar o botão "Dev"
        // acima do "Configurações" (plan revision: "ultra secreto"). Nada na
        // tela denuncia a escuta — sem flash, sem som; o log só sai no
        // stderr de quem rodou pelo terminal.
        if m.konami {
            cab.set_dev_mode(true);
            log::info!("devmode: on");
        }

        // "Sair do SixteeN?": B na tela inicial não sai de uma vez (um B de
        // sobra no controle fecharia o app). A/Start confirma, B volta.
        if confirm_quit {
            for nav in &m.nav {
                match nav {
                    MenuNav::Confirm | MenuNav::Start => return Ok(IdleExit::Quit),
                    MenuNav::Back => confirm_quit = false,
                    _ => {}
                }
            }
            if let Some((x, y)) = m.click {
                let (ox, oy) = cab.window_to_output(x, y);
                if cab.hit_close_button(ox, oy) {
                    return Ok(IdleExit::Quit);
                }
            }
            cab.frame_idle_2d(SETUP_BG, draw_quit_confirm);
            crate::runner::pace_ui(&mut next, frame, plat, cab);
            continue;
        }

        if setup {
            let (w, h) = cab.screen_size();
            let mut dismiss = false;
            if let Some((x, y)) = m.click {
                // The close/minimize pair lives in cabinet-canvas space; the
                // setup buttons in the screen buffer's own space (the screen
                // is inset by the bezel) — each with its own mapping.
                let (ox, oy) = cab.window_to_output(x, y);
                if cab.hit_close_button(ox, oy) {
                    return Ok(IdleExit::Quit);
                }
                if cab.hit_minimize_button(ox, oy) {
                    cab.minimize();
                    continue;
                }
                let (sx, sy) = cab.window_to_screen(x, y);
                match hit_setup_button(w, h, sx, sy) {
                    Some(SetupButton::Core) if core.rx.is_none() && !core.done => {
                        if let Some(url) = core_update::core_download_url() {
                            let (tx, rx) = mpsc::channel();
                            let dest = crate::dirs::core_dir();
                            std::thread::spawn(move || {
                                core_update::download_and_install(&url, &dest, &tx)
                            });
                            core.rx = Some(rx);
                            core.failed = false;
                        } else {
                            core.label = tr!(
                                "not available for this system — check settings".to_string(),
                                "indisponível neste sistema — veja as configurações".to_string()
                            );
                        }
                    }
                    Some(SetupButton::Dat) if dat.rx.is_none() && !dat.done => {
                        let (tx, rx) = mpsc::channel();
                        let dest = crate::dat_update::dat_path();
                        std::thread::spawn(move || {
                            crate::dat_update::download_and_install(&dest, &tx)
                        });
                        dat.rx = Some(rx);
                    }
                    Some(SetupButton::Continue) => dismiss = true,
                    _ => {}
                }
            }
            // Confirm (Enter/gamepad A) on the setup screen means
            // "continuar" — Back still quits.
            for nav in &m.nav {
                match nav {
                    MenuNav::Up => setup_focus = setup_focus.saturating_sub(1),
                    MenuNav::Down => setup_focus = (setup_focus + 1).min(2),
                    // A no botão em foco: baixa o núcleo/DAT ou continua.
                    MenuNav::Confirm => match setup_focus {
                        0 if !core.done => core.start_core(false),
                        1 if !dat.done => dat.start_dat(),
                        2 => dismiss = true,
                        _ => {}
                    },
                    // X: baixa o que falta, na ordem (núcleo primeiro).
                    MenuNav::Alt => {
                        if !core.done {
                            core.start_core(false);
                        } else if !dat.done {
                            dat.start_dat();
                        }
                    }
                    MenuNav::Start => dismiss = true,
                    MenuNav::Back => confirm_quit = true,
                    _ => {}
                }
            }
            if dismiss {
                setup = false;
            } else {
                let core_label = core.label.clone();
                let dat_label = dat.label.clone();
                let core_done = core.done;
                let dat_done = dat.done;
                let core_busy = core.rx.is_some();
                let dat_busy = dat.rx.is_some();
                let render = |d: &mut Screen| {
                    draw_setup(
                        d,
                        &core_label,
                        core_done,
                        core_busy,
                        &dat_label,
                        dat_done,
                        dat_busy,
                        setup_focus,
                    )
                };
                cab.frame_idle_2d(SETUP_BG, render);
                crate::runner::pace_ui(&mut next, frame, plat, cab);
                continue;
            }
            // Just dismissed: the same click/Confirm that dismissed the
            // setup screen must not also land on the idle panel below —
            // give it a frame of its own.
            cab.set_idle_core_prompt(
                core_missing.then_some(tr!("Download the snes9x core", "Baixar núcleo snes9x")),
            );
            crate::runner::pace_ui(&mut next, frame, plat, cab);
            continue;
        }

        cab.set_idle_core_prompt(
            core_missing.then_some(tr!("Download the snes9x core", "Baixar núcleo snes9x")),
        );

        // Manual do console aberto: dono do tubo (‹/› viram página,
        // voltar/Back fecha) — com a TV ligada ou não, como as
        // Configurações.
        if let Some(man) = console_manual.as_mut() {
            let mut close = false;
            for nav in &m.nav {
                match nav {
                    MenuNav::Left | MenuNav::PageUp => man.prev(),
                    MenuNav::Right | MenuNav::PageDown => man.next(),
                    MenuNav::Back => close = true,
                    _ => {}
                }
            }
            if let Some((x, y)) = m.click {
                let (ox, oy) = cab.window_to_output(x, y);
                if cab.hit_close_button(ox, oy) {
                    return Ok(IdleExit::Quit);
                }
                if cab.hit_panel_button(ox, oy) == Some(PanelButton::ConsoleManual) {
                    close = true;
                }
                if let Some((sx, sy)) = cab.hit_screen_point(ox, oy) {
                    let (prev, next, voltar) = man.rects;
                    if in_update_rect(sx, sy, prev) {
                        man.prev();
                    } else if in_update_rect(sx, sy, next) {
                        man.next();
                    } else if in_update_rect(sx, sy, voltar) {
                        close = true;
                    }
                }
            }
            if close {
                console_manual = None;
            } else {
                man.poll(cab);
                let ready = cab.has_image(crate::shelf::manual_page_id(man.key, man.page));
                let (key, page, pages, failed) = (man.key, man.page, man.pages, man.failed);
                let mut rects = man.rects;
                let render = |d: &mut Screen| {
                    rects = crate::shelf::draw_manual_view(d, key, page, pages, ready, failed);
                };
                cab.frame_idle_2d(SETUP_BG, render);
                man.rects = rects;
                crate::runner::pace_ui(&mut next, frame, plat, cab);
                continue;
            }
        }

        // Tela de update do app aberta: dona do tubo (changelog + botão).
        if update_view.is_some() {
            let (upt_tag, upt_body, asset_url) = {
                let u = update_view.as_ref().expect("checado acima");
                (u.tag.clone(), u.changelog.clone(), u.asset_url.clone())
            };
            // Os dois botões da tela — pelo mouse ou pelo controle (A age no
            // botão principal, B volta).
            let mut press_voltar = false;
            let mut press_btn = false;
            for nav in &m.nav {
                match nav {
                    MenuNav::Back if update_rx.is_none() => press_voltar = true,
                    MenuNav::Confirm => press_btn = true,
                    _ => {}
                }
            }
            if let Some((x, y)) = m.click {
                let (ox, oy) = cab.window_to_output(x, y);
                if cab.hit_close_button(ox, oy) {
                    return Ok(IdleExit::Quit);
                }
                if let Some((sx, sy)) = cab.hit_screen_point(ox, oy) {
                    let (btn, voltar) = update_rects;
                    if in_update_rect(sx, sy, voltar) {
                        press_voltar = true;
                    } else if in_update_rect(sx, sy, btn) {
                        press_btn = true;
                    }
                }
            }
            if press_voltar {
                update_view = None;
            } else if press_btn && update_applied {
                // "reiniciar agora": o pacote já está aplicado no
                // disco — relança e encerra a sessão atual.
                match crate::update_check::relaunch() {
                    Ok(()) => return Ok(IdleExit::Quit),
                    Err(e) => {
                        update_status = tr!(
                            format!("couldn't relaunch ({e}) — open the app again"),
                            format!("não consegui relançar ({e}) — abra o app de novo")
                        );
                    }
                }
            } else if let (true, Some(asset)) = (
                press_btn && update_rx.is_none() && apply_rx.is_none(),
                asset_url.as_ref(),
            ) {
                // "Atualizar": baixa o asset da plataforma para o
                // diretório de updates e aplica em seguida (plan
                // revision: "ao clicar em atualizar o app, ele faz
                // o download mas nao atualiza nada" — o apply só
                // acontecia no arranque seguinte, sem sinal nenhum).
                let (tx, rx) = mpsc::channel();
                let (ptx, prx) = mpsc::channel();
                let asset = asset.clone();
                let dest = crate::dirs::update_dir();
                std::thread::spawn(move || {
                    let result =
                        crate::update_check::download_asset(&asset, &dest, &ptx).map(|p| {
                            p.file_name()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .into_owned()
                        });
                    let _ = tx.send(result);
                });
                update_rx = Some(rx);
                update_progress_rx = Some(prx);
                update_pct = None;
                update_status = tr!("downloading...".to_string(), "baixando...".to_string());
            }
            let busy = update_rx.is_some() || apply_rx.is_some();
            let status = update_status.clone();
            let render = |d: &mut Screen| {
                update_rects = draw_update_view(
                    d,
                    &upt_tag,
                    &upt_body,
                    &status,
                    busy,
                    update_pct,
                    update_applied,
                );
            };
            cab.frame_idle_2d(SETUP_BG, render);
            crate::runner::pace_ui(&mut next, frame, plat, cab);
            continue;
        }

        if let Some((x, y)) = m.click {
            let (ox, oy) = cab.window_to_output(x, y);
            if cab.hit_close_button(ox, oy) {
                return Ok(IdleExit::Quit);
            }
            if cab.hit_minimize_button(ox, oy) {
                cab.minimize();
                continue;
            }
            // Os controles físicos da TV (plan revision): o knob POWER
            // liga/desliga (com o warm-up/colapso do tubo), VOL-/VOL+ mudam
            // o volume e acendem o OSD de barrinhas.
            if let Some(tv) = cab.hit_tv_controls(ox, oy) {
                cab.apply_tv_control(tv);
            }
            // Setas verdes do nameplate (plan revision): app abre a tela
            // de changelog; core baixa e instala na hora.
            match cab.hit_update_arrow(ox, oy) {
                Some(UpdateArrow::App) if app_update.is_some() => {
                    update_view = app_update.clone();
                    update_status.clear();
                }
                Some(UpdateArrow::Core) if core_stale && core.rx.is_none() => {
                    match core_update::core_download_url() {
                        Some(url) => {
                            let (tx, rx) = mpsc::channel();
                            let dest = crate::dirs::core_dir();
                            std::thread::spawn(move || {
                                core_update::download_and_install(&url, &dest, &tx)
                            });
                            core.rx = Some(rx);
                            core.done = false;
                            core.failed = false;
                            core.label =
                                tr!("downloading...".to_string(), "baixando...".to_string());
                        }
                        None => {
                            core.label = tr!(
                                "not available for this system — check settings".into(),
                                "indisponível neste sistema — veja as configurações".into()
                            );
                        }
                    }
                }
                _ => {}
            }
            // TV desligada: a estante precisa do tubo aceso, mas as
            // Configurações não dependem dela — respondem normalmente; clicar na estante acende o aviso no
            // OSD do queixo.
            if !cab.tv_on() {
                match cab.hit_panel_button(ox, oy) {
                    Some(PanelButton::Settings) if !core_missing => {
                        return Ok(IdleExit::OpenSettings)
                    }
                    Some(PanelButton::Dev) if cab.dev_mode() => return Ok(IdleExit::OpenDev),
                    Some(PanelButton::ConsoleManual) => console_manual = ConsoleManual::open(),
                    Some(_) => {
                        cab.push_osd(
                            &[tr!(
                                "turn the TV on to open the shelf",
                                "ligue a TV para abrir a estante"
                            )],
                            None,
                            Duration::from_secs(3),
                        );
                    }
                    None => {}
                }
                cab.present_static(static_level);
                crate::runner::pace_ui(&mut next, frame, plat, cab);
                continue;
            }
            match cab.hit_panel_button(ox, oy) {
                // Sem o snes9x instalado (plan revision: "só desative o
                // botão de config e de inserir cartucho se o usuário não
                // baixou o snes9x") — os dois ficam inertes; o painel
                // oferece o download do core no lugar.
                Some(PanelButton::Insert) if !core_missing => return Ok(IdleExit::OpenShelf),
                Some(PanelButton::Settings) if !core_missing => return Ok(IdleExit::OpenSettings),
                // O botão só existe com o devmode ligado (só o painel o
                // desenha), mas o gate fica explícito: sem devmode, cliques
                // fantasmas não abrem nada.
                Some(PanelButton::Dev) if cab.dev_mode() => return Ok(IdleExit::OpenDev),
                // O manual do console não depende do núcleo — é só leitura.
                Some(PanelButton::ConsoleManual) => console_manual = ConsoleManual::open(),
                Some(PanelButton::CoreDownload) if core.rx.is_none() => {
                    match core_update::core_download_url() {
                        Some(url) => {
                            let (tx, rx) = mpsc::channel();
                            let dest = crate::dirs::core_dir();
                            std::thread::spawn(move || {
                                core_update::download_and_install(&url, &dest, &tx)
                            });
                            core.rx = Some(rx);
                            core.done = false;
                            core.failed = false;
                            core.label =
                                tr!("downloading...".to_string(), "baixando...".to_string());
                        }
                        None => {
                            core.label = tr!(
                                "not available for this system — check settings".to_string(),
                                "indisponível neste sistema — veja as configurações".to_string()
                            );
                        }
                    }
                }
                // Already downloading? The click is ignored until the
                // worker reports Done/Failed.
                Some(PanelButton::CoreDownload) => {}
                _ => {}
            }
        }
        // O controle na tela inicial: d-pad passeia pelos botões do painel,
        // pelas setas de update e pelo POWER da TV (anel lavanda); A ativa o
        // foco; Start abre as configurações; X baixa o núcleo quando falta;
        // B pergunta se quer sair.
        let items = idle_items(&cab.panel_button_list(), app_update.is_some(), core_stale);
        let mut focus = resolve_focus(&items, idle_focus);
        for nav in m.nav {
            match nav {
                MenuNav::Up | MenuNav::Left => {
                    focus = step_focus(&items, focus, false);
                    idle_focus = Some(focus);
                }
                MenuNav::Down | MenuNav::Right => {
                    focus = step_focus(&items, focus, true);
                    idle_focus = Some(focus);
                }
                MenuNav::Confirm => match focus {
                    IdleFocus::Power => cab.apply_tv_control(TvControl::Power),
                    IdleFocus::Panel(PanelButton::Insert) if !core_missing => {
                        if cab.tv_on() {
                            return Ok(IdleExit::OpenShelf);
                        }
                        // Sem TV ligada a estante não abre: o A liga a TV
                        // (o mouse teria o botão POWER para isso).
                        cab.apply_tv_control(TvControl::Power);
                    }
                    IdleFocus::Panel(PanelButton::Settings) if !core_missing => {
                        return Ok(IdleExit::OpenSettings)
                    }
                    IdleFocus::Panel(PanelButton::Dev) if cab.dev_mode() => {
                        return Ok(IdleExit::OpenDev)
                    }
                    IdleFocus::Panel(PanelButton::ConsoleManual) => {
                        console_manual = ConsoleManual::open()
                    }
                    IdleFocus::Panel(PanelButton::CoreDownload) => core.start_core(true),
                    IdleFocus::Arrow(UpdateArrow::App) if app_update.is_some() => {
                        update_view = app_update.clone();
                        update_status.clear();
                    }
                    IdleFocus::Arrow(UpdateArrow::Core) if core_stale => core.start_core(true),
                    _ => {}
                },
                MenuNav::Start if !core_missing => return Ok(IdleExit::OpenSettings),
                MenuNav::Alt if core_missing => core.start_core(true),
                MenuNav::Back => confirm_quit = true,
                _ => {}
            }
        }
        // O anel só aparece para quem está usando o controle; a primeira vez
        // acende a legenda dos botões no OSD do queixo.
        if plat.last_input_was_pad() {
            if let Some(f) = items.contains(&focus).then_some(focus) {
                cab.set_pad_focus(Some(match f {
                    IdleFocus::Panel(b) => PadFocus::Panel(b),
                    IdleFocus::Arrow(a) => PadFocus::Arrow(a),
                    IdleFocus::Power => PadFocus::Tv(TvControl::Power),
                }));
            }
            if !legend_shown {
                legend_shown = true;
                cab.push_osd(
                    &[
                        tr!(
                            "D-pad choose   A confirm   Start settings",
                            "D-pad escolhe   A confirma   Start configurações"
                        ),
                        tr!("B quit", "B sair"),
                    ],
                    None,
                    Duration::from_secs(6),
                );
            }
        }
        // A seta verde do núcleo não abre tela nenhuma: o download roda em
        // segundo plano, e o painel mostra o andamento (plan revision:
        // "cliquei no icone verde de update do nucleo mas nao abriu nada").
        let core_status = if core.rx.is_some() || core.failed {
            Some(core.label.clone())
        } else if let Some(until) = core_ok_until {
            (Instant::now() < until).then(|| {
                tr!(
                    "core updated!".to_string(),
                    "núcleo atualizado!".to_string()
                )
            })
        } else {
            None
        };
        cab.set_core_status(core_status.as_deref());
        cab.present_static(static_level);
        crate::runner::pace_ui(&mut next, frame, plat, cab);
    }
}

/// `(x, y, w, h)` em coordenadas de tela — rect de botão da tela de update.
type UpdateBtn = (i32, i32, u32, u32);

const UPDATE_MARGIN: i32 = 40;

fn in_update_rect(x: i32, y: i32, r: UpdateBtn) -> bool {
    x >= r.0 && y >= r.1 && (x - r.0) < r.2 as i32 && (y - r.1) < r.3 as i32
}

/// A tela de update do app (plan revision: "quando for update do app
/// mostrar o changelog e o botão de atualizar" — e, desde a 1.1.4, o apply
/// acontece logo após o download e o botão vira "reiniciar agora") —
/// título, changelog do release, status e os botões. Devolve os rects.
fn draw_update_view(
    d: &mut Screen,
    tag: &str,
    changelog: &str,
    status: &str,
    busy: bool,
    _pct: Option<u32>,
    applied: bool,
) -> (UpdateBtn, UpdateBtn) {
    let (w, h) = d.size();
    let (w, h) = (w as i32, h as i32);
    let x = UPDATE_MARGIN;

    d.text(
        x,
        UPDATE_MARGIN,
        2,
        SETUP_TEXT,
        &tr!(format!("update {tag}"), format!("atualização {tag}")),
    );
    // Changelog em texto — o body vem em markdown do GitHub; as marcas
    // (#, *, `) são ruído no fonte de pixel.
    let clean: String = changelog
        .replace('\r', "")
        .chars()
        .filter(|c| !matches!(c, '#' | '*' | '`'))
        .collect();
    let lines: Vec<&str> = clean.lines().take(24).collect();
    let mut y = UPDATE_MARGIN + 60;
    for line in &lines {
        if y > h - 150 {
            d.text(x, y, 1, SETUP_DIM, "…");
            break;
        }
        d.text_wrapped(x, y, (w - UPDATE_MARGIN * 2) as u32, 1, SETUP_TEXT, line);
        y += 22;
    }

    let btn = (w - UPDATE_MARGIN * 2 - 220, h - 96, 220u32, 44u32);
    let voltar = (x, h - 96, 140u32, 44u32);
    d.text(x, h - 130, 1, SETUP_DIM, status);
    // Enquanto baixa/aplica o botão dorme (dim, sem hit) — o status ocupa
    // o lugar. Aplicado, ele acorda verde como "reiniciar agora" (o clique
    // relança pelo que está no disco e encerra a sessão).
    let can_update = !busy && !applied && !status.contains(tr!("downloaded", "baixado"));
    let label = if applied {
        tr!("restart now", "reiniciar agora")
    } else if can_update {
        tr!("update", "atualizar")
    } else {
        tr!("updated", "atualizado")
    };
    let active = can_update || applied;
    if active {
        d.outline(
            btn.0,
            btn.1,
            btn.2,
            btn.3,
            2,
            (SETUP_GREEN.0, SETUP_GREEN.1, SETUP_GREEN.2, 255),
        );
        d.text(
            btn.0 + (btn.2 as i32 - label.chars().count() as i32 * 18) / 2,
            btn.1 + 12,
            2,
            SETUP_GREEN,
            label,
        );
    } else {
        d.outline(btn.0, btn.1, btn.2, btn.3, 2, (90, 90, 96, 255));
        d.text(
            btn.0 + (btn.2 as i32 - label.chars().count() as i32 * 18) / 2,
            btn.1 + 12,
            2,
            SETUP_DIM,
            label,
        );
    }
    d.outline(
        voltar.0,
        voltar.1,
        voltar.2,
        voltar.3,
        2,
        (SETUP_TEXT.0, SETUP_TEXT.1, SETUP_TEXT.2, 255),
    );
    let back_label = tr!("back", "voltar");
    d.text(
        voltar.0 + (voltar.2 as i32 - back_label.chars().count() as i32 * 18) / 2,
        voltar.1 + 12,
        2,
        SETUP_TEXT,
        back_label,
    );
    (btn, voltar)
}

/// Whether the setup screen should come up — either file the player needs
/// is missing (plan revision: "não tiver o DAT, o snes9x").
fn core_missing_at_start(core_installed: bool, dat_installed: bool) -> bool {
    !core_installed || !dat_installed
}

/// The first-run setup screen, drawn inside the TV: a download button per
/// missing file, each with its own live status line underneath, plus
/// "continuar" to get to the regular console. Done buttons restyle to a
/// green "instalado" instead of disappearing, so the layout never jumps.
#[allow(clippy::too_many_arguments)]
fn draw_setup(
    d: &mut Screen,
    core_label: &str,
    core_done: bool,
    core_busy: bool,
    dat_label: &str,
    dat_done: bool,
    dat_busy: bool,
    focus: usize,
) {
    /// Same glyph metrics the platform font uses — `Screen::text` advances
    /// `GLYPH_W * scale` per char, `GLYPH_H * scale` per line.
    const CELL: i32 = 9;
    let (w, h) = d.size();
    let (w, h) = (w as i32, h as i32);
    let (core_r, dat_r, cont_r) = setup_rects(w as u32, h as u32);
    let center_x = |s: &str, scale: u32| (w - s.chars().count() as i32 * CELL * scale as i32) / 2;

    d.text(
        center_x(tr!("welcome to SixteeN", "bem-vindo ao sixteen"), 3),
        h * 12 / 100,
        3,
        SETUP_TEXT,
        tr!("welcome to SixteeN", "bem-vindo ao sixteen"),
    );
    d.text(
        center_x(
            tr!(
                "two downloads and you're ready to play",
                "dois downloads e você já pode jogar"
            ),
            1,
        ),
        h * 12 / 100 + 74,
        1,
        SETUP_DIM,
        tr!(
            "two downloads and you're ready to play",
            "dois downloads e você já pode jogar"
        ),
    );

    fn draw_button(d: &mut Screen, r: &RectT, label: &str, color: (u8, u8, u8)) {
        const CELL2: i32 = 18; // CELL * scale 2, for centering the label
        let (cr, cg, cb) = color;
        d.outline(r.0, r.1, r.2, r.3, 2, (cr, cg, cb, 255));
        d.text(
            r.0 + (r.2 as i32 - label.chars().count() as i32 * CELL2) / 2,
            r.1 + (r.3 as i32 - 40) / 2,
            2,
            color,
            label,
        );
    }
    let (mut core_color, mut dat_color) = (SETUP_TEXT, SETUP_TEXT);
    if core_busy {
        core_color = SETUP_DIM;
    } else if core_done {
        core_color = SETUP_GREEN;
    }
    if dat_busy {
        dat_color = SETUP_DIM;
    } else if dat_done {
        dat_color = SETUP_GREEN;
    }
    draw_button(d, &core_r, core_label, core_color);
    draw_button(d, &dat_r, dat_label, dat_color);
    d.text(
        core_r.0 + 4,
        core_r.1 + core_r.3 as i32 + 8,
        1,
        SETUP_DIM,
        tr!(
            "the engine that runs the games",
            "o motor que roda os jogos"
        ),
    );
    d.text(
        dat_r.0 + 4,
        dat_r.1 + dat_r.3 as i32 + 8,
        1,
        SETUP_DIM,
        tr!(
            "optional: proper game names, year and publisher",
            "opcional: nomes certinhos dos jogos, ano e editora"
        ),
    );

    draw_button(d, &cont_r, tr!("let's play", "bora jogar"), SETUP_TEXT);
    // O foco do controle: moldura lavanda em volta do botão escolhido.
    let [core_f, dat_f, cont_f] = [core_r, dat_r, cont_r];
    let fr = [core_f, dat_f, cont_f][focus.min(2)];
    d.outline(
        fr.0 - 6,
        fr.1 - 6,
        fr.2 + 12,
        fr.3 + 12,
        3,
        (186, 138, 244, 255),
    );
    let legend = tr!(
        "Up/Down choose  A confirm  Start let's play  B quit",
        "Cima/Baixo escolhe  A confirma  Start bora jogar  B sair"
    );
    d.text(
        center_x(legend, 1),
        cont_r.1 + cont_r.3 as i32 + 10,
        1,
        SETUP_DIM,
        legend,
    );
}

/// A pergunta de sair (B na tela inicial): o tubo inteiro vira o aviso,
/// como o leitor de manual — A confirma, B volta.
fn draw_quit_confirm(d: &mut Screen) {
    const CELL: i32 = 9;
    let (w, h) = d.size();
    let (w, h) = (w as i32, h as i32);
    let title = tr!("Quit SixteeN?", "Sair do SixteeN?");
    let hint = tr!("A confirms, B goes back", "A confirma, B volta");
    d.text(
        (w - title.chars().count() as i32 * CELL * 3) / 2,
        h * 38 / 100,
        3,
        SETUP_TEXT,
        title,
    );
    d.text(
        (w - hint.chars().count() as i32 * CELL * 2) / 2,
        h * 38 / 100 + 90,
        2,
        SETUP_DIM,
        hint,
    );
}

/// Headless preview of the idle screen (dev/testing) — same setup as `run`,
/// one frame captured through the tube instead of a live loop. Shows the
/// first-run setup screen when either download is missing, the plain idle
/// panel otherwise.
pub fn capture_preview(
    cab: &mut Cabinet,
    static_level: f32,
    core_installed: bool,
    dat_installed: bool,
    path: &Path,
) -> Result<()> {
    cab.clear_panel();
    if core_missing_at_start(core_installed, dat_installed) {
        let render = |d: &mut Screen| {
            draw_setup(
                d,
                if core_installed {
                    tr!("installed", "instalado")
                } else {
                    tr!("download the snes9x core", "baixar núcleo snes9x")
                },
                core_installed,
                false,
                if dat_installed {
                    tr!("installed", "instalado")
                } else {
                    tr!("download nointro.dat", "baixar nointro.dat")
                },
                dat_installed,
                false,
                2,
            )
        };
        cab.capture_idle_2d(SETUP_BG, render, path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    } else {
        crate::console_art::load_brand_images(cab);
        cab.capture_static_bmp(static_level, path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    }
}

/// Headless preview of the console manual reader on `page` (dev/testing) —
/// renders the page synchronously instead of through the worker.
pub fn capture_console_manual_preview(cab: &mut Cabinet, page: usize, path: &Path) -> Result<()> {
    let man = ConsoleManual::open().ok_or_else(|| anyhow::anyhow!("couldn't write the PDF"))?;
    let pdf = std::env::temp_dir().join(format!("sixteen-{}.pdf", man.key));
    let pages = crate::manual::page_count(&pdf)?;
    if let Some(img) = crate::manual::render_page(&pdf, page, 2048)? {
        cab.set_image(
            crate::shelf::manual_page_id(man.key, page),
            img.w,
            img.h,
            &img.rgba,
        );
    }
    crate::console_art::load_brand_images(cab);
    let ready = cab.has_image(crate::shelf::manual_page_id(man.key, page));
    let render = |d: &mut Screen| {
        crate::shelf::draw_manual_view(d, man.key, page, pages, ready, false);
    };
    cab.capture_idle_2d(SETUP_BG, render, path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_follow_the_panel_then_arrows_then_power() {
        let drawn = [
            PanelButton::Insert,
            PanelButton::Settings,
            PanelButton::ConsoleManual,
            PanelButton::Eject,
        ];
        let v = idle_items(&drawn, true, true);
        assert_eq!(
            v,
            vec![
                IdleFocus::Panel(PanelButton::Insert),
                IdleFocus::Panel(PanelButton::Settings),
                IdleFocus::Panel(PanelButton::ConsoleManual),
                IdleFocus::Arrow(UpdateArrow::App),
                IdleFocus::Arrow(UpdateArrow::Core),
                IdleFocus::Power,
            ]
        );
    }

    #[test]
    fn the_tv_can_always_be_powered_by_pad() {
        // Nem o painel desenhou ainda: o POWER segue alcançável.
        let v = idle_items(&[], false, false);
        assert_eq!(v, vec![IdleFocus::Power]);
        assert_eq!(resolve_focus(&v, None), IdleFocus::Power);
    }

    #[test]
    fn default_focus_is_insert_or_the_core_download() {
        let with_insert = idle_items(&[PanelButton::Settings, PanelButton::Insert], false, false);
        assert_eq!(
            resolve_focus(&with_insert, None),
            IdleFocus::Panel(PanelButton::Insert)
        );
        let no_core = idle_items(
            &[PanelButton::ConsoleManual, PanelButton::CoreDownload],
            false,
            false,
        );
        assert_eq!(
            resolve_focus(&no_core, None),
            IdleFocus::Panel(PanelButton::CoreDownload)
        );
        // Um foco que sumiu da lista cai no padrão.
        assert_eq!(
            resolve_focus(&no_core, Some(IdleFocus::Arrow(UpdateArrow::App))),
            IdleFocus::Panel(PanelButton::CoreDownload)
        );
    }

    #[test]
    fn focus_steps_wrap_both_ways() {
        let v = idle_items(&[PanelButton::Insert, PanelButton::Settings], false, false);
        let last = *v.last().unwrap();
        assert_eq!(step_focus(&v, last, true), v[0]);
        assert_eq!(step_focus(&v, v[0], false), last);
    }
}
