//! O balcão da locadora (docs/design-modo-locadora.md §3/§7/§8): prateleira
//! em grade com as capas e os estados físicos dos cartuchos, cerimônia de
//! compra (label descascando), contracapa e manual destravados na compra,
//! cartaz da semana, parede de troféus e o sorteio de boas-vindas.
//! Mouse e gamepad, sem teclado — como o resto da casa.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::Result;
use sixteen_locadora::{
    check_trophies, draw_trio, poster_of_week, ElectError, HourlyError, InsertCheck, Money,
    Persisted, Prices, RentMode, StoreError, TrophyId,
};
use sixteen_domain::catalog::{Catalog, CatalogEntry, Order};
use sixteen_platform::{Cabinet, MenuMode, MenuNav, Platform};

use crate::dirs;
use crate::shelf::{decode_art_scaled, find_local_art};

/// Para onde o balcão manda o jogador.
pub enum Pick {
    /// Cartucho validado pela tela — o runner decide o resto.
    Play { rom: PathBuf, sha: String, mode: RentMode },
    /// Voltou à tela inicial (TV no CH 3).
    Back,
    Quit,
}

/// Verificação headless (dev/testes): captura a tela no frame N e devolve.
pub struct LocaDebug {
    pub shot: PathBuf,
    pub frame: u64,
}

type RectT = (i32, i32, u32, u32);

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Insert,
    Hourly,
    Ticket,
    ReturnTicket,
    Elect,
    Buy,
    ViewBackcover,
    ViewManual,
    View(u8),
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Intro,
    Balcao,
    Loja,
    Trofeus,
}

const GLYPH: i32 = 9; // cell do texto na escala 1 (cabinet.rs:222)
const TEXT: (u8, u8, u8) = (232, 226, 210);
const DIM: (u8, u8, u8) = (150, 142, 128);
const ACCENT: (u8, u8, u8) = (214, 78, 58);
const GOLD: (u8, u8, u8) = (214, 178, 84);
const PANEL: (u8, u8, u8, u8) = (30, 24, 26, 235);
const EDGE: (u8, u8, u8, u8) = (70, 60, 55, 255);
const EDGE_HOT: (u8, u8, u8, u8) = (214, 78, 58, 255);
const EDGE_GOLD: (u8, u8, u8, u8) = (214, 178, 84, 255);

const CEREMONY_PHASE: Duration = Duration::from_millis(700);
const TAKEBACK_TIME: Duration = Duration::from_millis(1700);
const OSD_TROPHY_ID: u64 = 0x1D0C_E000_0000_0001;

/// Cortinas do balcão: a compra descasca a etiqueta (§7); a devolução
/// automática mostra a locadora levando o cartucho de volta (§6).
enum Ceremony {
    Peel { sha: String, started: Instant },
    TakeBack { started: Instant, fined: i64 },
}

impl Ceremony {
    fn sha(&self) -> Option<&str> {
        match self {
            Self::Peel { sha, .. } => Some(sha),
            _ => None,
        }
    }
    fn done(&self) -> bool {
        match self {
            Self::Peel { started, .. } => started.elapsed() > CEREMONY_PHASE * 3,
            Self::TakeBack { started, .. } => started.elapsed() > TAKEBACK_TIME,
        }
    }
}

pub fn run(
    plat: &mut Platform,
    cab: &mut Cabinet,
    catalog: &Catalog,
    loc: &mut Persisted,
    prices: &Prices,
    currency: &str,
    debug: Option<LocaDebug>,
) -> Result<Pick> {
    // Reconciliação do acervo (design §16): o que sumiu de roms/ sai da
    // coleção e encerra ticket/sessão/em serviço — sem reembolso.
    let entries: Vec<CatalogEntry> = catalog.list(Order::Shelf)?;
    let sha1s: Vec<String> = entries.iter().map(|e| e.rom.sha1.clone()).collect();
    let removidos = loc.reconcile_acervo(&sha1s);
    if !removidos.is_empty() {
        loc.save(&dirs::locadora_path())?;
        cab.push_osd(
            &[&format!(
                "acervo mudou: {} cartucho(s) saíram da estante — sem reembolso, freguês",
                removidos.len()
            )],
            None,
            Duration::from_secs(5),
        );
    }

    let poster = poster_of_week(&sha1s, sixteen_locadora::rules::now_unix());
    let mut view = if loc.intro_done { View::Balcao } else { View::Intro };
    let mut selected: usize = 0;
    let mut scroll: usize = 0;
    let mut cols: usize = 4; // recomputado a cada frame; o input usa o anterior
    // Overlays de vitrine (compra/contracapa/manual) e filas de decode.
    // Devolução automática aconteceu fora? O balcão abre com a cortina.
    let mut ceremony: Option<Ceremony> = if let Some(fined) = loc.takeback_notice.take() {
        let _ = loc.save(&dirs::locadora_path());
        Some(Ceremony::TakeBack { started: Instant::now(), fined })
    } else {
        None
    };
    let mut zoom: Option<String> = None; // sha com a contracapa aberta
    let mut manual: Option<(String, Vec<PathBuf>, usize)> = None; // sha, páginas, atual
    let mut art_cursor: usize = 0;
    let mut art_tried: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut dec_tried: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Geometria do frame anterior (o clique chega antes do desenho; as rects
    // são estáveis entre frames, então um frame de atraso é invisível).
    let mut hot: Vec<(Action, RectT)> = Vec::new();
    let mut rows: Vec<(usize, RectT)> = Vec::new();
    let mut intro: Vec<(usize, RectT)> = Vec::new();

    let frame = Duration::from_millis(16);
    let mut next = Instant::now() + frame;
    let mut frame_no: u64 = 0;
    let mut picked_auto: bool = false;
                    #[allow(unused_assignments)]


    loop {
        let m = plat.poll_menu(MenuMode::Nav);
        if m.quit {
            return Ok(Pick::Quit);
        }
        // Fechar/minimizar moram no espaço de SAÍDA (fora do tubo).
        let click_out = m.click.map(|(x, y)| cab.window_to_output(x, y));
        if let Some((ox, oy)) = click_out {
            if cab.hit_close_button(ox, oy) {
                return Ok(Pick::Quit);
            }
            if cab.hit_minimize_button(ox, oy) {
                cab.minimize();
            }
        }
        // O balcão é desenhado no espaço do TUBO (o buffer do CRT) — cliques
        // e hover passam por hit_screen_point, igual ao zoom da estante.
        let click = click_out.and_then(|(ox, oy)| cab.hit_screen_point(ox, oy));
        let pointer = m.pointer.and_then(|(x, y)| {
            let (ox, oy) = cab.window_to_output(x, y);
            cab.hit_screen_point(ox, oy)
        });
        let nav: Vec<MenuNav> = m.nav.clone();

        // Cortina ativa: o mundo espera (§6/§7).
        if ceremony.as_ref().is_some_and(|c| c.done()) {
            ceremony = None;
        }
        let busy = ceremony.is_some();

        // Zoom de contracapa e manual consomem o input antes do balcão.
        if !busy {
            if zoom.is_some() {
                let close = click.is_some()
                    || nav.iter().any(|n| matches!(n, MenuNav::Back | MenuNav::Confirm));
                if close {
                    zoom = None;
                }
            } else if let Some((sha, pages, page)) = &mut manual {
                let mut next_page = *page;
                for n in &nav {
                    match n {
                        MenuNav::Left | MenuNav::PageUp => {
                            next_page = next_page.saturating_sub(1)
                        }
                        MenuNav::Right | MenuNav::PageDown => {
                            if *page + 1 < pages.len() {
                                next_page = *page + 1;
                            }
                        }
                        MenuNav::Back | MenuNav::Confirm => next_page = pages.len(),
                        _ => {}
                    }
                }
                if let Some((ox, _)) = click {
                    if ox < 200 {
                        next_page = next_page.saturating_sub(1);
                    } else {
                        next_page += 1;
                    }
                }
                if next_page >= pages.len() {
                    manual = None;
                } else {
                    *page = next_page;
                    // A página entra como textura na primeira vez que abre.
                    let key = format!("{}:{sha}", pages[*page].display());
                    if dec_tried.insert(key.clone()) {
                        if let Ok((w, h, rgba)) = decode_art_scaled(&pages[*page], 2048) {
                            cab.set_image(manual_page_id(&key), w, h, &rgba);
                        }
                    }
                }
            }
        }

        let mut acted: Option<Action> = None;
        if !busy && zoom.is_none() && manual.is_none() {
            if let Some((ox, oy)) = click {
                for (a, r) in &hot {
                    if hit(r, ox, oy) {
                        acted = Some(*a);
                        break;
                    }
                }
                if acted.is_none() && view == View::Intro && !loc.intro_done {
                    for (i, r) in &intro {
                        if hit(r, ox, oy) {
                            let trio = draw_trio(&sha1s, sixteen_locadora::rules::now_unix() as u64);
                            if let Some(sha) = trio.get(*i).cloned() {
                                let _ = loc.apply_intro(&trio, &sha);
                                loc.save(&dirs::locadora_path())?;
                                // A primeira lição com cerimônia: a etiqueta
                                // velha descasca e o jogo nasce na coleção.
                                ceremony = Some(Ceremony::Peel {
                                    sha: sha.clone(),
                                    started: Instant::now(),
                                });
                                announce(
                                    cab,
                                    &["este cartucho é SEU — foi isso que comprar te dá"],
                                );
                                view = View::Balcao;
                                selected = 0;
                            }
                            break;
                        }
                    }
                }
                if acted.is_none() {
                    for (i, r) in &rows {
                        if hit(r, ox, oy) {
                            selected = *i;
                            break;
                        }
                    }
                }
            }
            for n in &nav {
                match n {
                    MenuNav::Up if list_len(&entries, view) > 0 => {
                        selected = selected.saturating_sub(cols);
                    }
                    MenuNav::Down if list_len(&entries, view) > 0 => {
                        selected = (selected + cols).min(list_len(&entries, view) - 1);
                    }
                    MenuNav::Left if list_len(&entries, view) > 0 => {
                        selected = selected.saturating_sub(1);
                    }
                    MenuNav::Right if list_len(&entries, view) > 0 => {
                        selected = (selected + 1).min(list_len(&entries, view) - 1);
                    }
                    MenuNav::Back => {
                        if view == View::Balcao {
                            loc.save(&dirs::locadora_path())?;
                            return Ok(Pick::Back);
                        }
                        view = View::Balcao;
                    }
                    MenuNav::Confirm
                        if acted.is_none() && (view == View::Balcao || view == View::Loja) =>
                    {
                        acted = match view {
                            View::Loja => Some(Action::Buy),
                            _ => Some(primary_action(loc, entries.get(selected))),
                        };
                    }
                    _ => {}
                }
            }
        }
        if let Some(a) = acted {
            if let Some(p) = act(
                cab,
                loc,
                prices,
                currency,
                &entries,
                &poster,
                a,
                selected,
                &mut view,
                &mut ceremony,
                &mut zoom,
                &mut manual,
            )? {
                return Ok(p);
            }
        }

        // --- desenho -----------------------------------------------------
        selected = selected.min(entries.len().saturating_sub(1));
        let mut frame_hot: Vec<(Action, RectT)> = Vec::new();
        let mut frame_rows: Vec<(usize, RectT)> = Vec::new();
        let mut frame_intro: Vec<(usize, RectT)> = Vec::new();

        let mut ui = Ui {
            loc: &*loc,
            prices,
            currency,
            entries: &entries,
            poster: &poster,
            view,
            selected,
            pointer,
            ceremony: ceremony.as_ref(),
            zoom: zoom.as_deref(),
            manual: manual
                .as_ref()
                .map(|(sha, pages, page)| (sha.as_str(), pages.as_slice(), *page)),
        };
        cab.frame_idle_2d((16, 13, 15), |d| {
            draw_ui(d, &mut ui, &mut scroll, &mut cols, &mut frame_hot, &mut frame_rows, &mut frame_intro);
        });

        hot = frame_hot;
        rows = frame_rows;
        intro = frame_intro;

        // Orçamento de decode: uma capa por frame, fila circular (mesma
        // dieta da estante) — a grade enche sozinha em ~1 frame por jogo.
        if !entries.is_empty() {
            let i = art_cursor % entries.len();
            art_cursor += 1;
            let e = &entries[i];
            if art_tried.insert(e.rom.sha1.clone()) {
                ensure_cover(cab, &e.rom.path, &e.rom.sha1);
            }
        }
        // Decodes pontuais: contracapa do zoom e cartucho da cerimônia.
        if let Some(sha) = &zoom {
            if let Some(e) = entries.iter().find(|e| &e.rom.sha1 == sha) {
                let key = format!("back:{}", e.rom.sha1);
                if dec_tried.insert(key.clone()) {
                    ensure_art_at(cab, art_back_id(&e.rom.sha1), "backcover", &e.rom.path);
                }
            }
        }
        if let Some(sha) = ceremony.as_ref().and_then(|c| c.sha()) {
            if let Some(e) = entries.iter().find(|e| &e.rom.sha1 == sha) {
                let key = format!("cart:{}", e.rom.sha1);
                if dec_tried.insert(key.clone()) {
                    ensure_art_at(cab, art_cart_id(&e.rom.sha1), "cartridge", &e.rom.path);
                }
            }
        }

        // Verificação headless: captura no frame N e devolve (Pick::Back).
        if let Some(dbg) = &debug {
            if frame_no + 1 >= dbg.frame {
                // O sorteio se resolve sozinho pra mostra o balcão de fato.
                if !loc.intro_done && !picked_auto {
                    let trio = draw_trio(&sha1s, sixteen_locadora::rules::now_unix() as u64);
                    if let Some(sha) = trio.first().cloned() {
                        let _ = loc.apply_intro(&trio, &sha);
                    }
                    picked_auto = true;
                    view = View::Balcao;
                    #[allow(unused_assignments)]
                    {}
                }
                let mut ui = Ui {
                    loc: &*loc,
                    prices,
                    currency,
                    entries: &entries,
                    poster: &poster,
                    view,
                    selected,
                    pointer: None,
                    ceremony: None,
                    zoom: None,
                    manual: None,
                };
                let mut fh = Vec::new();
                let mut fr = Vec::new();
                let mut fi = Vec::new();
                let mut dscroll = scroll;
                let mut dcols = cols;
                cab.capture_idle_2d((16, 13, 15), |d| {
                    draw_ui(d, &mut ui, &mut dscroll, &mut dcols, &mut fh, &mut fr, &mut fi);
                }, &dbg.shot)?;
                return Ok(Pick::Back);
            }
        }

        frame_no += 1;
        // Pacing do balcão (o idle paca em 16ms; dormimos o resto do tick).
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        }
        next = Instant::now() + frame;
    }
}

// --- estado de desenho compartilhado (loop vivo e captura headless) --------

struct Ui<'a> {
    loc: &'a Persisted,
    prices: &'a Prices,
    currency: &'a str,
    entries: &'a [CatalogEntry],
    poster: &'a str,
    view: View,
    selected: usize,
    pointer: Option<(i32, i32)>,
    ceremony: Option<&'a Ceremony>,
    zoom: Option<&'a str>,
    manual: Option<(&'a str, &'a [PathBuf], usize)>,
}

fn draw_ui(
    d: &mut sixteen_platform::Screen,
    ui: &mut Ui,
    scroll: &mut usize,
    cols: &mut usize,
    hot: &mut Vec<(Action, RectT)>,
    rows: &mut Vec<(usize, RectT)>,
    intro: &mut Vec<(usize, RectT)>,
) {
    let (w, h) = d.size();
    let (w, h) = (w as i32, h as i32);
    let pad = 28;
    let wallet = Money(ui.loc.wallet_cents).fmt(ui.currency);

    // Cabeçalho: letreiro, caixa registradora e o cargo de serviço.
    d.fill(0, 0, w as u32, 64, (12, 9, 11, 255));
    d.text(pad, 14, 2, ACCENT, "LOCADORA SIXTEEN");
    let service = ui
        .loc
        .service_game
        .as_ref()
        .and_then(|sha| ui.entries.iter().find(|e| &e.rom.sha1 == sha))
        .map(|e| e.title().into_owned())
        .unwrap_or_else(|| "— vago —".into());
    let status = format!("EM SERVIÇO: {service} · CAIXA {wallet}");
    d.text(w - pad - status.len() as i32 * GLYPH, 44, 1, DIM, &status);

    // Abas.
    let tabs: [(&str, u8); 3] = [("BALCÃO", 0), ("LOJA", 1), ("TROFÉUS", 2)];
    let mut tx = w - pad;
    for (label, id) in tabs.iter().rev() {
        let tw = label.len() as i32 * GLYPH + 24;
        tx -= tw + 8;
        let r = (tx, 12, tw as u32, 34);
        let on = *id == match ui.view {
            View::Balcao | View::Intro => 0,
            View::Loja => 1,
            View::Trofeus => 2,
        };
        d.fill(r.0, r.1, r.2, r.3, if on { (70, 30, 26, 255) } else { PANEL });
        d.outline(r.0, r.1, r.2, r.3, 1, if on { EDGE_HOT } else { EDGE });
        d.text(r.0 + 12, r.1 + 10, 1, if on { TEXT } else { DIM }, label);
        hot.push((Action::View(*id), r));
    }

    let top = 84;
    match ui.view {
        View::Intro => {
            d.text(pad, top, 2, TEXT, "BEM-VINDO À FICHA — escolha SEU jogo");
            d.text(
                pad,
                top + 36,
                1,
                DIM,
                "sorteio de boas-vindas: três cartuchos; um é seu de graça e vira o cartucho em serviço",
            );
            let trio = draw_trio(
                &ui.entries.iter().map(|e| e.rom.sha1.clone()).collect::<Vec<_>>(),
                sixteen_locadora::rules::now_unix() as u64,
            );
            let bw = 240i32;
            let bh = 190i32;
            let bx0 = (w - bw * 3 - 40) / 2;
            for (i, sha) in trio.iter().enumerate() {
                let r = (bx0 + i as i32 * (bw + 20), top + 90, bw as u32, bh as u32);
                let hov = ui.pointer.is_some_and(|(px, py)| hit(&r, px, py));
                d.fill(r.0, r.1, r.2, r.3, PANEL);
                d.outline(r.0, r.1, r.2, r.3, if hov { 3 } else { 1 }, if hov { EDGE_HOT } else { EDGE });
                // Etiqueta gasta de locadora sobre o exemplar (§7).
                d.fill(r.0 + 8, r.1 + 8, r.2 - 16, r.3 - 16, worn_alpha(ui.loc, sha));
                let cover = art_id(sha);
                if d.has_image(cover) {
                    d.image_fit(cover, r.0 + 12, r.1 + 12, r.2 - 24, r.3 - 56);
                }
                d.text(r.0 + 14, r.1 + r.3 as i32 - 36, 1, DIM, "EXEMPLAR DA LOCADORA");
                if ui.loc.owns(sha) {
                    d.text(r.0 + r.2 as i32 - 46, r.1 + 14, 1, GOLD, "SEU");
                }
                intro.push((i, r));
            }
        }
        View::Balcao | View::Loja => {
            let mut list_top = top;
            if ui.view == View::Balcao {
                // Cartaz da semana (§17).
                if let Some(p) = ui.entries.iter().find(|e| e.rom.sha1 == ui.poster) {
                    d.fill(pad, top, (w - pad * 2) as u32, 30, (46, 24, 20, 255));
                    d.text(
                        pad + 10,
                        top + 9,
                        1,
                        GOLD,
                        &format!("CARTAZ DA SEMANA: {} — locação pela metade", p.title()),
                    );
                }
                list_top = top + 42;
            }

            // Grade de caixas (a prateleira da locadora).
            let (tw, th, gap) = (206i32, 158i32, 16i32);
            let cols_now = (((w - pad * 2 + gap) / (tw + gap)) as usize).max(1);
            *cols = cols_now;
            let vis_rows = (((h - 150 - list_top) / (th + gap)) as usize).max(1);
            let len = ui.entries.len();
            ui.selected = ui.selected.min(len.saturating_sub(1));
            let sel_row = ui.selected / cols_now;
            if sel_row < *scroll {
                *scroll = sel_row;
            }
            if sel_row >= *scroll + vis_rows {
                *scroll = sel_row + 1 - vis_rows;
            }
            let first = *scroll * cols_now;
            for (k, e) in ui.entries.iter().enumerate().skip(first).take(vis_rows * cols_now) {
                let col = (k % cols_now) as i32;
                let row = (k / cols_now - *scroll) as i32;
                let r = (
                    pad + col * (tw + gap),
                    list_top + row * (th + gap),
                    tw as u32,
                    th as u32,
                );
                let sel = k == ui.selected;
                let owned = ui.loc.owns(&e.rom.sha1);
                let is_service = ui.loc.service_game.as_deref() == Some(e.rom.sha1.as_str());
                let is_poster = e.rom.sha1 == ui.poster;

                d.fill(r.0, r.1, r.2, r.3, PANEL);
                if owned {
                    // Caixa nova: contorno vivo, sem desgaste.
                    d.outline(r.0, r.1, r.2, r.3, if sel { 3 } else { 2 }, (ACCENT.0, ACCENT.1, ACCENT.2, 255));
                } else {
                    // Exemplar da locadora: cicatriz por uso (§17).
                    d.fill(r.0, r.1, r.2, r.3, worn_alpha(ui.loc, &e.rom.sha1));
                    d.outline(r.0, r.1, r.2, r.3, if sel { 3 } else { 1 }, if sel { EDGE_HOT } else { EDGE });
                }
                if is_poster && !owned {
                    d.fill(r.0, r.1, r.2, 22, (46, 24, 20, 255));
                    d.text(r.0 + 8, r.1 + 5, 1, GOLD, "CARTAZ DA SEMANA");
                }
                let cover = art_id(&e.rom.sha1);
                if d.has_image(cover) {
                    d.image_fit(cover, r.0 + 10, r.1 + 10 + if is_poster && !owned { 20 } else { 0 }, r.2 - 20, r.3 - 52 - if is_poster && !owned { 20 } else { 0 });
                } else {
                    d.text_wrapped(r.0 + 12, r.1 + 14, (r.2 as i32 - 24) as u32, 1, if sel { TEXT } else { DIM }, &e.title());
                }
                // Faixa inferior: título curto à esquerda, estado à direita.
                d.fill(r.0, r.1 + r.3 as i32 - 34, r.2, 34, (12, 9, 11, 220));
                let tag = row_tag(ui.loc, e);
                let tag_w = tag.len() as i32 * GLYPH;
                let title_max = ((r.2 as i32 - 20 - tag_w - 12) / GLYPH).max(6);
                d.text(r.0 + 10, r.1 + r.3 as i32 - 26, 1, if sel { TEXT } else { DIM }, &short_title(&e.title(), title_max));
                d.text(r.0 + r.2 as i32 - tag_w - 10, r.1 + r.3 as i32 - 26, 1, if sel { GOLD } else { DIM }, &tag);
                if is_service {
                    d.fill(r.0, r.1, r.2, 20, (40, 28, 14, 235));
                    d.text(r.0 + 10, r.1 + 4, 1, GOLD, "EM SERVIÇO — paga por hora");
                }
                rows.push((k, r));
            }

            // Barra de ações do jogo selecionado.
            let bar_y = h - 70;
            d.fill(pad, bar_y, (w - pad * 2) as u32, 58, (12, 9, 11, 255));
            if let Some(e) = ui.entries.get(ui.selected) {
                let sha = e.rom.sha1.as_str();
                let owned = ui.loc.owns(sha);
                let half = ui.view == View::Balcao && sha == ui.poster && !owned;
                let price = |base: i64| Money(if half { base / 2 } else { base }).fmt(ui.currency);
                let mut labels: Vec<(String, Action)> = Vec::new();
                if ui.view == View::Loja {
                    if owned {
                        labels.push(("NA COLEÇÃO — contracapa e manual destravados no BALCÃO".into(), Action::View(0)));
                    } else {
                        labels.push((format!("COMPRAR {}", Money(ui.prices.purchase).fmt(ui.currency)), Action::Buy));
                    }
                } else {
                    match ui.loc.insert_check(sha) {
                        InsertCheck::Ok => {
                            labels.push(("INSERIR CARTUCHO".into(), Action::Insert));
                            if owned && ui.loc.service_game.as_deref() != Some(sha) {
                                labels.push(("ELEGER EM SERVIÇO".into(), Action::Elect));
                            }
                            // §7: comprado destrava contracapa e manual.
                            if owned {
                                labels.push(("VER CONTRACAPA".into(), Action::ViewBackcover));
                                if has_manual(&e.rom.path) {
                                    labels.push(("FOLHEAR MANUAL".into(), Action::ViewManual));
                                }
                            }
                        }
                        InsertCheck::TicketExpired => {
                            labels.push(("DEVOLVER".into(), Action::ReturnTicket));
                        }
                        InsertCheck::NotRented => {
                            labels.push((format!("LOCAR AVULSA {}/h", price(ui.prices.hourly_per_hour)), Action::Hourly));
                            labels.push((format!("TICKET 48H {}", price(ui.prices.ticket_48h)), Action::Ticket));
                        }
                    }
                }
                let mut bx = pad + 14;
                for (label, a) in &labels {
                    let bw = label.len() as i32 * GLYPH + 28;
                    let r = (bx, bar_y + 11, bw as u32, 36);
                    let hov = ui.pointer.is_some_and(|(px, py)| hit(&r, px, py));
                    d.fill(r.0, r.1, r.2, r.3, if hov { (70, 30, 26, 255) } else { (40, 30, 30, 255) });
                    d.outline(r.0, r.1, r.2, r.3, 1, (120, 80, 70, 255));
                    d.text(r.0 + 14, r.1 + 12, 1, TEXT, label);
                    hot.push((*a, r));
                    bx += bw + 12;
                }
                if half {
                    d.text(bx + 8, bar_y + 22, 1, GOLD, "CARTAZ DA SEMANA — metade do preço");
                }
            }
        }
        View::Trofeus => {
            d.text(pad, top, 2, TEXT, "PAREDE DE TROFÉUS");
            let (cw, ch) = (300i32, 96i32);
            for (i, t) in TrophyId::ALL.iter().enumerate() {
                let col = (i % 3) as i32;
                let row = (i / 3) as i32;
                let r = (
                    pad + col * (cw + 14),
                    top + 48 + row * (ch + 14),
                    cw as u32,
                    ch as u32,
                );
                let rec = ui.loc.trophies.iter().find(|rec| rec.id == t.key());
                d.fill(r.0, r.1, r.2, r.3, PANEL);
                d.outline(
                    r.0,
                    r.1,
                    r.2,
                    r.3,
                    if rec.is_some() { 2 } else { 1 },
                    if rec.is_some() { EDGE_GOLD } else { EDGE },
                );
                if let Some(rec) = rec {
                    d.fill(r.0 + 16, r.1 + 20, 24, 26, (28, 24, 22, 255));
                    d.fill(r.0 + 12, r.1 + 48, 32, 8, (28, 24, 22, 255));
                    draw_trophy_sprite(d, *t, r.0 + 16, r.1 + 16, true);
                    d.text(r.0 + 54, r.1 + 16, 1, GOLD, t.title());
                    d.text(
                        r.0 + 54,
                        r.1 + 38,
                        1,
                        DIM,
                        &format!("desde {}", fmt_deadline(rec.unlocked_unix)),
                    );
                } else {
                    d.fill(r.0 + 16, r.1 + 20, 24, 26, (24, 21, 20, 255));
                    d.fill(r.0 + 12, r.1 + 48, 32, 8, (24, 21, 20, 255));
                    draw_trophy_sprite(d, *t, r.0 + 16, r.1 + 16, false);
                    d.text(r.0 + 54, r.1 + 16, 1, DIM, "???");
                }
                d.text(r.0 + 16, r.1 + r.3 as i32 - 28, 1, DIM, t.hint());
            }
        }
    }

    // --- overlays por cima de tudo --------------------------------------
    if let Some(Ceremony::Peel { sha, started }) = ui.ceremony {
        // Cerimônia de compra (§7): gasto → faixa descasca → caixa nova.
        let elapsed = started.elapsed().as_millis() as i64;
        let phase = (elapsed / CEREMONY_PHASE.as_millis() as i64).min(2);
        let t_in_phase = elapsed % CEREMONY_PHASE.as_millis() as i64;
        d.fill(0, 0, w as u32, h as u32, (8, 6, 8, 200));
        let (bw, bh) = (300i32, 400i32);
        let r = ((w - bw) / 2, (h - bh) / 2, bw as u32, bh as u32);
        d.fill(r.0, r.1, r.2, r.3, (22, 18, 20, 255));
        d.outline(r.0, r.1, r.2, r.3, 2, EDGE_GOLD);
        let cart = art_cart_id(sha);
        if d.has_image(cart) {
            d.image_fit(cart, r.0 + 20, r.1 + 24, r.2 - 40, r.3 - 120);
        }
        if phase == 0 {
            d.fill(r.0 + 20, r.1 + 60, r.2 - 40, 30, (232, 222, 198, 220));
            d.text(r.0 + 34, r.1 + 68, 1, (60, 40, 30), "PROPRIEDADE DA LOCADORA");
            d.text(r.0 + 20, r.1 + r.3 as i32 - 60, 1, DIM, "pagando no balcão...");
        } else if phase == 1 {
            // A faixa velha descasca pro lado, com o cantinho dobrado.
            let slide = ((t_in_phase * i64::from(r.2 - 20))
                / CEREMONY_PHASE.as_millis() as i64) as i32;
            d.fill(r.0 + 20 + slide, r.1 + 60, r.2 - 40, 30, (232, 222, 198, 220));
            d.fill(r.0 + 20 + slide, r.1 + 90, 14, 30, (232, 222, 198, 240));
            d.text(r.0 + 20, r.1 + r.3 as i32 - 60, 1, GOLD, "descascando a etiqueta...");
        } else {
            d.outline(r.0 + 16, r.1 + 20, r.2 - 32, r.3 - 96, 2, EDGE_GOLD);
            d.star(r.0 + 40, r.1 + 40, 9, GOLD);
            d.star(r.0 + r.2 as i32 - 44, r.1 + r.3 as i32 - 120, 7, GOLD);
            d.text(r.0 + 20, r.1 + r.3 as i32 - 60, 1, GOLD, "SEU — COLEÇÃO, CAIXA NOVA!");
        }
    }
    if let Some(Ceremony::TakeBack { started, fined }) = ui.ceremony {
        // Devolução automática (§6): o exemplar sobe e sai da tela — a
        // locadora busca o cartucho que você não devolveu.
        let t = started.elapsed().as_millis() as i64;
        let total = TAKEBACK_TIME.as_millis() as i64;
        let rise = (t * i64::from(h)) / total;
        d.fill(0, 0, w as u32, h as u32, (8, 6, 8, 190));
        let (bw, bh) = (240i32, 180i32);
        let r = ((w - bw) / 2, (h - bh) / 2 - rise as i32, bw as u32, bh as u32);
        // Cartucho genérico desenhado (sem arte: quem busca é a locadora).
        d.fill(r.0, r.1, r.2, r.3, (36, 32, 30, 255));
        d.fill(r.0 + 30, r.1, r.2 - 60, 26, (52, 46, 42, 255));
        d.fill(r.0 + 24, r.1 + 34, r.2 - 48, r.3 - 60, (232, 222, 198, 230));
        d.text(r.0 + 34, r.1 + r.3 as i32 - 40, 1, (60, 40, 30), "PROPRIEDADE DA LOCADORA");
        d.outline(r.0, r.1, r.2, r.3, 2, EDGE);
        let title = "DEVOLUÇÃO AUTOMÁTICA";
        d.text((w - title.len() as i32 * GLYPH * 2) / 2, (h / 2 - 150).max(30), 2, ACCENT, title);
        let line = format!(
            "a locadora levou o cartucho de volta — multa: {}",
            Money(*fined).fmt(ui.currency)
        );
        d.text((w - line.len() as i32 * GLYPH) / 2, (h / 2 + 130).min(h - 56), 1, TEXT, &line);
    }
    if let Some(sha) = ui.zoom {
        // Contracapa destravada pela compra (§7): zoom no tubo.
        d.fill(0, 0, w as u32, h as u32, (8, 6, 8, 235));
        let back = art_back_id(sha);
        if d.has_image(back) {
            d.image_fit(back, 120, 70, (w - 240) as u32, (h - 150) as u32);
        } else {
            d.text(w / 2 - 180, h / 2, 1, DIM, "sem contracapa digitalizada — coloque em assets/backcover/");
        }
        d.text(16, h - 30, 1, DIM, "clique ou ESC fecha");
    }
    if let Some((sha, pages, page)) = ui.manual {
        // Manual folheado (§7): páginas de assets/manual/<jogo>/.
        d.fill(0, 0, w as u32, h as u32, (8, 6, 8, 245));
        let key = format!("{}:{sha}", pages[page].display());
        let id = manual_page_id(&key);
        if d.has_image(id) {
            d.image_fit(id, 60, 40, (w - 120) as u32, (h - 120) as u32);
        } else {
            d.text(w / 2 - 100, h / 2, 1, DIM, "decodificando página...");
        }
        d.text(
            16,
            h - 30,
            1,
            DIM,
            &format!("página {} de {} — clique esquerda/direita folheia, ESC fecha", page + 1, pages.len()),
        );
    }
}

// --- helpers ---------------------------------------------------------------


fn hit(r: &RectT, x: i32, y: i32) -> bool {
    x >= r.0 && y >= r.1 && (x - r.0) < r.2 as i32 && (y - r.1) < r.3 as i32
}

fn short_title(t: &str, max_chars: i32) -> String {
    let max = max_chars.max(4) as usize;
    if t.chars().count() <= max {
        t.to_string()
    } else {
        let cut: String = t.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

/// Cicatriz da estante (§17): a etiqueta gasta engorda com as locações do
/// exemplar — o queridão da prateleira é o mais sofrido.
fn worn_alpha(loc: &Persisted, sha: &str) -> (u8, u8, u8, u8) {
    let rentals = loc.rentals_per_game.get(sha).copied().unwrap_or(0) as i32;
    let alpha = (70 + rentals * 18).min(210) as u8;
    (60, 52, 38, alpha)
}

fn list_len(entries: &[CatalogEntry], view: View) -> usize {
    match view {
        View::Balcao | View::Loja => entries.len(),
        _ => 0,
    }
}

/// A ação do Confirm: inserir quando dá; senão locar (avulsa primeiro).
fn primary_action(loc: &Persisted, e: Option<&CatalogEntry>) -> Action {
    let Some(e) = e else { return Action::Insert };
    match loc.insert_check(&e.rom.sha1) {
        InsertCheck::Ok => Action::Insert,
        InsertCheck::TicketExpired => Action::ReturnTicket,
        InsertCheck::NotRented => Action::Hourly,
    }
}

/// Etiqueta da caixa, contando o estado físico do exemplar (§7).
fn row_tag(loc: &Persisted, e: &CatalogEntry) -> String {
    let sha = &e.rom.sha1;
    if loc.service_game.as_deref() == Some(sha.as_str()) {
        return "+R$/h".into();
    }
    if loc.owns(sha) {
        return "COLEÇÃO".into();
    }
    match loc.insert_check(sha) {
        InsertCheck::Ok if loc.hourly.as_ref().map(|h| h.game == *sha).unwrap_or(false) => format!(
            "AVULSA ({} trocas)",
            loc.hourly.as_ref().map(|h| h.swaps_used).unwrap_or(0)
        ),
        InsertCheck::Ok => format!(
            "TICKET até {}",
            fmt_deadline(loc.active_ticket.as_ref().map(|t| t.deadline_unix).unwrap_or(0))
        ),
        InsertCheck::TicketExpired => "ATRASADA!".into(),
        InsertCheck::NotRented => "para locar".into(),
    }
}

/// Aplica uma ação de botão sobre o jogo selecionado. `Some(pick)` encerra.
#[allow(clippy::too_many_arguments)]
fn act(
    cab: &mut Cabinet,
    loc: &mut Persisted,
    prices: &Prices,
    currency: &str,
    entries: &[CatalogEntry],
    poster: &str,
    a: Action,
    selected: usize,
    view: &mut View,
    ceremony: &mut Option<Ceremony>,
    zoom: &mut Option<String>,
    manual: &mut Option<(String, Vec<PathBuf>, usize)>,
) -> Result<Option<Pick>> {
    let mut pick: Option<Pick> = None;
    let entry = entries.get(selected);
    match a {
        Action::View(id) => {
            *view = match id {
                1 => View::Loja,
                2 => View::Trofeus,
                _ => View::Balcao,
            };
        }
        Action::Insert => {
            if let Some(e) = entry {
                if loc.insert_check(&e.rom.sha1) == InsertCheck::Ok {
                    let sha = e.rom.sha1.clone();
                    let mode = if loc.owns(&sha) {
                        RentMode::Service
                    } else if loc.hourly.as_ref().map(|h| h.game == sha).unwrap_or(false) {
                        RentMode::Hourly
                    } else {
                        RentMode::Ticket
                    };
                    loc.save(&dirs::locadora_path())?;
                    pick = Some(Pick::Play { rom: PathBuf::from(&e.rom.path), sha, mode });
                }
            }
        }
        Action::Elect => {
            if let Some(e) = entry {
                match loc.elect_service(&e.rom.sha1) {
                    Ok(()) => announce(cab, &["cartucho em serviço — agora a gaveta paga por hora"]),
                    Err(ElectError::NotOwned) => {}
                }
            }
        }
        Action::Hourly => {
            if let Some(e) = entry {
                let sha = e.rom.sha1.clone();
                let p = if sha == poster && !loc.owns(&sha) { half_price(prices) } else { *prices };
                match loc.start_hourly(&sha, &p) {
                    Ok(()) => {
                        loc.save(&dirs::locadora_path())?;
                        pick = Some(Pick::Play { rom: PathBuf::from(&e.rom.path), sha, mode: RentMode::Hourly });
                    }
                    Err(HourlyError::EmptyWallet) => {
                        announce(cab, &["grana insuficiente — jogue o cartucho em serviço"]);
                    }
                    Err(_) => {}
                }
            }
        }
        Action::Ticket => {
            if let Some(e) = entry {
                let sha = e.rom.sha1.clone();
                let p = if sha == poster && !loc.owns(&sha) { half_price(prices) } else { *prices };
                match loc.start_ticket(&sha, &p) {
                    Ok(start) => {
                        loc.save(&dirs::locadora_path())?;
                        announce(
                            cab,
                            &[&format!(
                                "ticket de 48h — devolver até {}",
                                fmt_deadline(start + 48 * 3600)
                            )],
                        );
                        pick = Some(Pick::Play { rom: PathBuf::from(&e.rom.path), sha, mode: RentMode::Ticket });
                    }
                    Err(StoreError::InsufficientFunds) => {
                        announce(cab, &["grana insuficiente para o ticket"]);
                    }
                }
            }
        }
        Action::ReturnTicket => {
            let late = loc.return_ticket();
            loc.save(&dirs::locadora_path())?;
            announce(
                cab,
                &[if late {
                    "devolvido ATRASADO — a multa já caiu, freguês"
                } else {
                    "devolvido no prazo — carimbo no cartão de fidelidade"
                }],
            );
        }
        Action::Buy => {
            if let Some(e) = entry {
                if loc.can_afford_purchase(prices) {
                    if loc.buy(&e.rom.sha1, prices).is_ok() {
                        loc.save(&dirs::locadora_path())?;
                        announce(cab, &["comprado! caixa nova, contracapa e manual destravados"]);
                        // Cerimônia do label novo (§7) — o cartucho entra na
                        // fila de decode do frame e a cortina bloqueia 2.1s.
                        ensure_art_at(cab, art_cart_id(&e.rom.sha1), "cartridge", &e.rom.path);
                        *ceremony = Some(Ceremony::Peel {
                            sha: e.rom.sha1.clone(),
                            started: Instant::now(),
                        });
                    }
                } else {
                    announce(
                        cab,
                        &[&format!(
                            "grana insuficiente — faltam {} para a caixa nova",
                            Money(prices.purchase.saturating_sub(loc.wallet_cents)).fmt(currency)
                        )],
                    );
                }
            }
        }
        Action::ViewBackcover => {
            if let Some(e) = entry {
                if loc.owns(&e.rom.sha1) {
                    *zoom = Some(e.rom.sha1.clone());
                }
            }
        }
        Action::ViewManual => {
            if let Some(e) = entry {
                if loc.owns(&e.rom.sha1) {
                    match manual_pages(&e.rom.path) {
                        Some(pages) if !pages.is_empty() => {
                            *manual = Some((e.rom.sha1.clone(), pages, 0));
                        }
                        _ => announce(
                            cab,
                            &["sem manual digitalizado — coloque as páginas em assets/manual/<jogo>/"],
                        ),
                    }
                }
            }
        }
    }
    // Troféus pipocam depois de qualquer ação (design §9).
    let novos = check_trophies(loc, sixteen_locadora::rules::now_unix());
    if !novos.is_empty() {
        loc.save(&dirs::locadora_path())?;
        for t in &novos {
            let (bw, bh, rgba) = badge_rgba(*t);
            cab.set_image(OSD_TROPHY_ID, bw, bh, &rgba);
            cab.push_osd(
                &[&format!("🏆 TROFÉU: {}", t.title())],
                Some(OSD_TROPHY_ID),
                Duration::from_secs(6),
            );
            crate::sfx::play(cab, crate::sfx::Sfx::Achievement);
        }
    }
    Ok(pick)
}

fn announce(cab: &mut Cabinet, lines: &[&str]) {
    cab.push_osd(lines, None, Duration::from_secs(5));
    crate::sfx::play(cab, crate::sfx::Sfx::Insert);
}

fn half_price(p: &Prices) -> Prices {
    Prices {
        salary_per_hour: p.salary_per_hour,
        hourly_per_hour: (p.hourly_per_hour / 2).max(1),
        ticket_48h: (p.ticket_48h / 2).max(1),
        purchase: p.purchase,
        weekend_double: p.weekend_double,
    }
}

fn stem_of(rom_path: &str) -> Option<String> {
    Path::new(rom_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}

/// Páginas do manual digitalizado: assets/manual/<jogo>/*.{png,jpg,jpeg},
/// em ordem de nome.
fn manual_pages(rom_path: &str) -> Option<Vec<PathBuf>> {
    let dir = dirs::assets_dir().join("manual").join(stem_of(rom_path)?);
    let mut pages: Vec<PathBuf> = std::fs::read_dir(&dir)
        .ok()?
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            matches!(
                p.extension().and_then(|e| e.to_str()),
                Some("png") | Some("jpg") | Some("jpeg")
            )
        })
        .collect();
    pages.sort();
    Some(pages)
}

fn has_manual(rom_path: &str) -> bool {
    manual_pages(rom_path).is_some_and(|p| !p.is_empty())
}

/// unix → "DD/MM hh:MM" (algoritmo civil de Howard Hinnant, std only).
pub fn fmt_deadline(unix: i64) -> String {
    let days = unix.div_euclid(86_400);
    let secs = unix.rem_euclid(86_400);
    let (h, mi) = (secs / 3600, (secs % 3600) / 60);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let _y = if m <= 2 { y + 1 } else { y };
    format!("{d:02}/{m:02} {h:02}:{mi:02}")
}

/// Capa do jogo (grade e sorteio) sob o id derivado do sha1.
fn ensure_cover(cab: &mut Cabinet, rom_path: &str, sha: &str) {
    let Some(path) = find_local_art(&dirs::assets_dir().join("cover"), rom_path) else {
        return;
    };
    let Ok((w, h, rgba)) = decode_art_scaled(&path, 512) else {
        return;
    };
    cab.set_image(art_id(sha), w, h, &rgba);
}

/// Arte de um kind (cartridge/backcover) sob um id explícito.
fn ensure_art_at(cab: &mut Cabinet, id: u64, kind: &str, rom_path: &str) {
    let Some(path) = find_local_art(&dirs::assets_dir().join(kind), rom_path) else {
        return;
    };
    let Ok((w, h, rgba)) = decode_art_scaled(&path, 512) else {
        return;
    };
    cab.set_image(id, w, h, &rgba);
}

/// Namespace próprio das texturas da locadora (FNV-1a + marca por tipo).
fn fnv(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in s.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash & 0x0000_FFFF_FFFF_FFFF
}

pub fn art_id(sha: &str) -> u64 {
    0x1D0C_A000_0000_0000 | fnv(sha)
}

fn art_cart_id(sha: &str) -> u64 {
    0x1D0C_B000_0000_0000 | fnv(sha)
}

fn art_back_id(sha: &str) -> u64 {
    0x1D0C_C000_0000_0000 | fnv(sha)
}

fn manual_page_id(key: &str) -> u64 {
    0x1D0C_D000_0000_0000 | fnv(key)
}

// --- trofeuzinhos pixel-art (design §9) -------------------------------------
// 12×12, '.' vazio, 'X' ouro, 'o' accent — desenhados na parede e no badge
// do OSD. Personagens-tema de locadora, não personagens licenciados. kkk

fn trophy_sprite(t: TrophyId) -> [&'static str; 12] {
    match t {
        // Ficha de catálogo: o primeiro dia na locadora.
        TrophyId::BemVindoAFicha => [
            "............",
            ".oooooooooo.",
            ".oXXXXXXXXo.",
            ".oX......Xo.",
            ".oX.oooo.Xo.",
            ".oX......Xo.",
            ".oX.ooo..Xo.",
            ".oX......Xo.",
            ".oX.oooo.Xo.",
            ".oXXXXXXXXo.",
            ".oooooooooo.",
            "............",
        ],
        // Cartão de ponto: uma hora de trampo.
        TrophyId::PrimeiroTurno => [
            "....XXXX....",
            "..XX....XX..",
            ".X........X.",
            ".X...oo...X.",
            ".X...oo...X.",
            ".X...ooo..X.",
            ".X........X.",
            ".X..oooo..X.",
            ".X........X.",
            "..XX....XX..",
            "....XXXX....",
            "............",
        ],
        // Cartucho: o vizinho do acervo.
        TrophyId::VizinhoDoAcervo => [
            ".XXXXXXXXXX.",
            ".XooooooooX.",
            ".XoXXXXXXoX.",
            ".XoX....XoX.",
            ".XoX.XX.XoX.",
            ".XoX.XX.XoX.",
            ".XoX....XoX.",
            ".XoXXXXXXoX.",
            ".XooooooooX.",
            ".XXXXXXXXXX.",
            "..XX....XX..",
            "............",
        ],
        // Setas cruzadas: a troca de cartucho.
        TrophyId::TrocaTroca => [
            "....X.......",
            "...XX...X...",
            "..XXX..XX...",
            ".XXXX.XXX...",
            "...XXXXXXX..",
            "....XXXXXX..",
            "...XXXXXXXX.",
            ".XXX..XXXX..",
            "..XX...XXX..",
            "...X....XX..",
            "........X...",
            "............",
        ],
        // Relógio carimbado: prazo cumprido.
        TrophyId::PrazoCumprido => [
            "....XXXX....",
            "..XX.oo.XX..",
            ".X...oo..oX.",
            "X....oo...oX",
            "X..........X",
            "X....oo....X",
            "X.....oooo.X",
            ".X........X.",
            "..XX....XX..",
            "....XXXX....",
            "............",
            "............",
        ],
        // Caixas empilhadas: a coleção crescendo.
        TrophyId::Colegao => [
            "..XX....XX..",
            "..XXXX..XXXX",
            "..XooX..XooX",
            "..XX....XX..",
            "XXXXXXXXXXXX",
            "XoXXXXXXXXoX",
            "..XX....XX..",
            "..XXXX..XXXX",
            "..XooX..XooX",
            "..XX....XX..",
            "XXXXXXXXXXXX",
            "............",
        ],
        // Balcão com toldo: a loja.
        TrophyId::Lojista => [
            ".oXoXoXoXoX.",
            "XXXXXXXXXXXX",
            "XXXXXXXXXXXX",
            "....XXXX....",
            "....X..X....",
            "....X..X....",
            "....XooX....",
            "....X..X....",
            "....X..X....",
            "....XXXX....",
            "............",
            "............",
        ],
        // Coração com ficha: freguês fiel.
        TrophyId::FreguesFiel => [
            ".XX....XX...",
            "XXXX..XXXX..",
            "XXXX..XXXX..",
            "XXXXXXXXXX..",
            "XooXXXXXXX..",
            ".XoXXXXXX...",
            "..XXXXXX....",
            "...XXXX.....",
            "....XX......",
            "............",
            "............",
            "............",
        ],
        // TV com estática: sobrevivente do corte.
        TrophyId::SobreviventeDaEstatica => [
            "XX........XX",
            "X.XXoXoXXX.X",
            "X.XoXoXoXX.X",
            "X.XXoXoXXX.X",
            "X.XoXoXoXX.X",
            "X.XXoXoXXX.X",
            "X..........X",
            "XX........XX",
            ".X..X..X..X.",
            "............",
            "............",
            "............",
        ],
        // Caracol: todo mundo devolve tarde uma vez. kkk
        TrophyId::DevolucaoAtrasada => [
            "......XXXX..",
            "....XXooooX.",
            "...X..XXXooX",
            "..X..X....oX",
            ".X..X.....oX",
            ".X..X.....oX",
            "..X..X....oX",
            "...X..XXXooX",
            ".XXXXXXXXXXX",
            "X..........X",
            "XXXXXXXXXXXX",
            "............",
        ],
    }
}

/// Desenha o sprite na parede (2px por pixel) — apagado quando não ganho.
fn draw_trophy_sprite(d: &mut sixteen_platform::Screen, t: TrophyId, x: i32, y: i32, lit: bool) {
    let s = 2i32;
    for (row, line) in trophy_sprite(t).iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            let c = match ch {
                'X' => {
                    if lit {
                        GOLD
                    } else {
                        (70, 64, 60)
                    }
                }
                'o' => {
                    if lit {
                        ACCENT
                    } else {
                        (54, 48, 46)
                    }
                }
                _ => continue,
            };
            d.fill(
                x + col as i32 * s,
                y + row as i32 * s,
                s as u32,
                s as u32,
                (c.0, c.1, c.2, 255),
            );
        }
    }
}

/// RGBA 24×24 do sprite, pro badge do OSD de desbloqueio.
fn badge_rgba(t: TrophyId) -> (u32, u32, Vec<u8>) {
    let (w, h) = (24u32, 24u32);
    let mut out = vec![0u8; (w * h * 4) as usize];
    for (row, line) in trophy_sprite(t).iter().enumerate() {
        for (col, ch) in line.chars().enumerate() {
            let (r, g, b, a) = match ch {
                'X' => (GOLD.0, GOLD.1, GOLD.2, 255),
                'o' => (ACCENT.0, ACCENT.1, ACCENT.2, 255),
                _ => continue,
            };
            let (px, py) = (col * 2, row * 2);
            for dy in 0..2 {
                for dx in 0..2 {
                    let i = (((py + dy) as u32 * w + (px + dx) as u32) * 4) as usize;
                    out[i] = r;
                    out[i + 1] = g;
                    out[i + 2] = b;
                    out[i + 3] = a;
                }
            }
        }
    }
    (w, h, out)
}

#[cfg(test)]
mod tests {
    use super::fmt_deadline;

    #[test]
    fn deadline_format() {
        // 2026-10-24 12:30 UTC
        assert_eq!(fmt_deadline(1_792_845_000), "24/10 12:30");
        assert_eq!(fmt_deadline(0), "01/01 00:00");
    }
}
