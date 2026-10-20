//! O balcão da locadora (docs/design-modo-locadora.md §3/§8): prateleira com
//! os estados físicos dos cartuchos, ações de locação, loja, parede de
//! troféus e o sorteio de boas-vindas. Mouse e gamepad, sem teclado — como
//! o resto da casa.

use std::path::PathBuf;
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

type RectT = (i32, i32, u32, u32);

#[derive(Clone, Copy, PartialEq)]
enum Action {
    Insert,
    Hourly,
    Ticket,
    ReturnTicket,
    Elect,
    Buy,
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
const WORN: (u8, u8, u8, u8) = (60, 52, 38, 70);
const EDGE: (u8, u8, u8, u8) = (70, 60, 55, 255);
const EDGE_HOT: (u8, u8, u8, u8) = (214, 78, 58, 255);

pub fn run(
    plat: &mut Platform,
    cab: &mut Cabinet,
    catalog: &Catalog,
    loc: &mut Persisted,
    prices: &Prices,
    currency: &str,
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
    // Arte do jogo em foco: uma tentativa por jogo por visita.
    let mut art_tried: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Geometria do frame anterior (o clique chega antes do desenho; as rects
    // são estáveis entre frames, então um frame de atraso é invisível).
    let mut hot: Vec<(Action, RectT)> = Vec::new();
    let mut rows: Vec<(usize, RectT)> = Vec::new();
    let mut intro: Vec<(usize, RectT)> = Vec::new();

    let frame = Duration::from_millis(16);
    let mut next = Instant::now() + frame;

    loop {
        let m = plat.poll_menu(MenuMode::Nav);
        if m.quit {
            return Ok(Pick::Quit);
        }
        let click = m.click.map(|(x, y)| cab.window_to_output(x, y));
        if let Some((ox, oy)) = click {
            if cab.hit_close_button(ox, oy) {
                return Ok(Pick::Quit);
            }
            if cab.hit_minimize_button(ox, oy) {
                cab.minimize();
            }
        }
        let pointer = m.pointer.map(|(x, y)| cab.window_to_output(x, y));
        let list_len = entries.len();

        // Clique: abas e botões de ação primeiro, depois linhas/sorteio.
        let mut acted: Option<Action> = None;
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
        // Navegação por d-pad/setas.
        for nav in &m.nav {
            match nav {
                MenuNav::Up if list_len > 0 => {
                    selected = selected.saturating_sub(1);
                }
                MenuNav::Down if list_len > 0 => {
                    selected = (selected + 1).min(list_len - 1);
                }
                MenuNav::Back => {
                    if view == View::Balcao {
                        loc.save(&dirs::locadora_path())?;
                        return Ok(Pick::Back);
                    }
                    view = View::Balcao;
                }
                MenuNav::Confirm if acted.is_none() && view != View::Trofeus => {
                    // Confirm dispara a ação principal do jogo selecionado.
                    acted = match view {
                        View::Loja => Some(Action::Buy),
                        _ => Some(primary_action(loc, selected_entry(&entries, selected))),
                    };
                }
                _ => {}
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
            )? {
                return Ok(p);
            }
        }

        // --- desenho -----------------------------------------------------
        selected = selected.min(entries.len().saturating_sub(1));
        let mut frame_hot: Vec<(Action, RectT)> = Vec::new();
        let mut frame_rows: Vec<(usize, RectT)> = Vec::new();
        let mut frame_intro: Vec<(usize, RectT)> = Vec::new();

        cab.frame_idle_2d((16, 13, 15), |d| {
            let (w, h) = d.size();
            let (w, h) = (w as i32, h as i32);
            let pad = 28;
            let wallet = Money(loc.wallet_cents).fmt(currency);

            // Cabeçalho: letreiro, caixa registradora e o cargo de serviço.
            d.fill(0, 0, w as u32, 64, (12, 9, 11, 255));
            d.text(pad, 14, 2, ACCENT, "LOCADORA SIXTEEN");
            let wallet_line = format!("CAIXA {wallet}");
            d.text(w - pad - wallet_line.len() as i32 * GLYPH, 12, 1, GOLD, &wallet_line);
            let service = loc
                .service_game
                .as_ref()
                .and_then(|sha| entries.iter().find(|e| &e.rom.sha1 == sha))
                .map(|e| e.title().into_owned())
                .unwrap_or_else(|| "— vago —".into());
            d.text(
                pad,
                42,
                1,
                DIM,
                &format!("EM SERVIÇO: {service} (só com o console ligado)"),
            );

            // Abas.
            let tabs: [(&str, u8); 3] = [("BALCÃO", 0), ("LOJA", 1), ("TROFÉUS", 2)];
            let mut tx = w - pad;
            for (label, id) in tabs.iter().rev() {
                let tw = label.len() as i32 * GLYPH + 24;
                tx -= tw + 8;
                let r = (tx, 12, tw as u32, 34);
                let on = *id == match view {
                    View::Balcao | View::Intro => 0,
                    View::Loja => 1,
                    View::Trofeus => 2,
                };
                d.fill(r.0, r.1, r.2, r.3, if on { (70, 30, 26, 255) } else { PANEL });
                d.outline(r.0, r.1, r.2, r.3, 1, if on { EDGE_HOT } else { EDGE });
                d.text(r.0 + 12, r.1 + 10, 1, if on { TEXT } else { DIM }, label);
                frame_hot.push((Action::View(*id), r));
            }

            let top = 84;
            match view {
                View::Intro => {
                    d.text(pad, top, 2, TEXT, "BEM-VINDO À FICHA — escolha SEU jogo");
                    d.text(
                        pad,
                        top + 36,
                        1,
                        DIM,
                        "sorteio de boas-vindas: três cartuchos; um é seu de graça e vira o cartucho em serviço",
                    );
                    let trio = draw_trio(&sha1s, sixteen_locadora::rules::now_unix() as u64);
                    let bw = 240i32;
                    let bh = 170i32;
                    let bx0 = (w - bw * 3 - 40) / 2;
                    for (i, sha) in trio.iter().enumerate() {
                        let r = (bx0 + i as i32 * (bw + 20), top + 80, bw as u32, bh as u32);
                        let hov = pointer.is_some_and(|(px, py)| hit(&r, px, py));
                        d.fill(r.0, r.1, r.2, r.3, PANEL);
                        d.outline(r.0, r.1, r.2, r.3, if hov { 3 } else { 1 }, if hov { EDGE_HOT } else { EDGE });
                        // Etiqueta gasta de locadora sobre o nome (§7).
                        d.fill(r.0 + 8, r.1 + 8, r.2 - 16, r.3 - 16, WORN);
                        d.text(r.0 + 10, r.1 + r.3 as i32 - 34, 1, DIM, "EXEMPLAR DA LOCADORA");
                        if let Some(e) = entries.iter().find(|e| &e.rom.sha1 == sha) {
                            d.text_wrapped(
                                r.0 + 18,
                                r.1 + 22,
                                (r.2 as i32 - 36).max(40) as u32,
                                1,
                                TEXT,
                                &e.title(),
                            );
                        }
                        if loc.owns(sha) {
                            d.text(r.0 + 10, r.1 + 10, 1, GOLD, "SEU");
                        }
                        frame_intro.push((i, r));
                    }
                }
                View::Balcao | View::Loja => {
                    let mut list_top = top;
                    if view == View::Balcao {
                        // Cartaz da semana (§17).
                        if let Some(p) = entries.iter().find(|e| e.rom.sha1 == poster) {
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
                    let row_h = 44i32;
                    let visible = (((h - 150 - list_top) / row_h) as usize).max(1);
                    if selected < scroll {
                        scroll = selected;
                    }
                    if selected >= scroll + visible {
                        scroll = selected + 1 - visible;
                    }
                    for (i, e) in entries.iter().enumerate().skip(scroll).take(visible) {
                        let r = (
                            pad,
                            list_top + (i - scroll) as i32 * row_h,
                            (w - pad * 2) as u32,
                            (row_h - 6) as u32,
                        );
                        let sel = i == selected;
                        let owned = loc.owns(&e.rom.sha1);
                        d.fill(r.0, r.1, r.2, r.3, if sel { (48, 34, 32, 255) } else { PANEL });
                        if owned {
                            // Coleção: barra viva na lateral, caixa nova.
                            d.fill(r.0, r.1, 6, r.3, (ACCENT.0, ACCENT.1, ACCENT.2, 255));
                        } else {
                            // Cartucho da casa: amarelecimento (§7).
                            d.fill(r.0 + 6, r.1, r.2 - 6, r.3, WORN);
                        }
                        d.text(r.0 + 20, r.1 + 14, 1, if sel { TEXT } else { DIM }, &e.title());
                        let tag = row_tag(loc, e);
                        let tw = tag.len() as i32 * GLYPH;
                        d.text(
                            r.0 + r.2 as i32 - tw - 14,
                            r.1 + 14,
                            1,
                            if sel { GOLD } else { DIM },
                            &tag,
                        );
                        frame_rows.push((i, r));
                    }

                    // Barra de ações do jogo selecionado.
                    let bar_y = h - 70;
                    d.fill(pad, bar_y, (w - pad * 2) as u32, 58, (12, 9, 11, 255));
                    if let Some(e) = entries.get(selected) {
                        let sha = e.rom.sha1.as_str();
                        let owned = loc.owns(sha);
                        let half = view == View::Balcao && sha == poster && !owned;
                        let price = |base: i64| Money(if half { base / 2 } else { base }).fmt(currency);
                        let mut labels: Vec<(String, Action)> = Vec::new();
                        if view == View::Loja {
                            if owned {
                                labels.push(("NA COLEÇÃO".into(), Action::View(1)));
                            } else {
                                labels.push((
                                    format!("COMPRAR {}", Money(prices.purchase).fmt(currency)),
                                    Action::Buy,
                                ));
                            }
                        } else {
                            match loc.insert_check(sha) {
                                InsertCheck::Ok => {
                                    labels.push(("INSERIR CARTUCHO".into(), Action::Insert));
                                    if owned && loc.service_game.as_deref() != Some(sha) {
                                        labels.push(("ELEGER EM SERVIÇO".into(), Action::Elect));
                                    }
                                }
                                InsertCheck::TicketExpired => {
                                    labels.push(("DEVOLVER".into(), Action::ReturnTicket));
                                }
                                InsertCheck::NotRented => {
                                    labels.push((
                                        format!("LOCAR AVULSA {}/h", price(prices.hourly_per_hour)),
                                        Action::Hourly,
                                    ));
                                    labels.push((
                                        format!("TICKET 48H {}", price(prices.ticket_48h)),
                                        Action::Ticket,
                                    ));
                                }
                            }
                        }
                        let mut bx = pad + 14;
                        for (label, a) in &labels {
                            let bw = label.len() as i32 * GLYPH + 28;
                            let r = (bx, bar_y + 11, bw as u32, 36);
                            let hov = pointer.is_some_and(|(px, py)| hit(&r, px, py));
                            d.fill(r.0, r.1, r.2, r.3, if hov { (70, 30, 26, 255) } else { (40, 30, 30, 255) });
                            d.outline(r.0, r.1, r.2, r.3, 1, (120, 80, 70, 255));
                            d.text(r.0 + 14, r.1 + 12, 1, TEXT, label);
                            frame_hot.push((*a, r));
                            bx += bw + 12;
                        }
                        if half {
                            d.text(bx + 8, bar_y + 22, 1, GOLD, "CARTAZ DA SEMANA — metade do preço");
                        }
                        // Vitrine: a arte do cartucho do selecionado (quando
                        // existir), no canto direito da barra.
                        let art = art_id(&e.rom.sha1);
                        if d.has_image(art) {
                            d.image_fit(art, w - pad - 260, bar_y - 120, 260, 130);
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
                        let rec = loc.trophies.iter().find(|rec| rec.id == t.key());
                        d.fill(r.0, r.1, r.2, r.3, PANEL);
                        d.outline(
                            r.0,
                            r.1,
                            r.2,
                            r.3,
                            if rec.is_some() { 2 } else { 1 },
                            if rec.is_some() { (GOLD.0, GOLD.1, GOLD.2, 255) } else { EDGE },
                        );
                        if let Some(rec) = rec {
                            // Pedestal dourado + título + data.
                            d.fill(r.0 + 16, r.1 + 18, 20, 30, (GOLD.0, GOLD.1, GOLD.2, 255));
                            d.fill(r.0 + 12, r.1 + 48, 28, 9, (GOLD.0, GOLD.1, GOLD.2, 200));
                            d.text(r.0 + 54, r.1 + 16, 1, GOLD, t.title());
                            d.text(
                                r.0 + 54,
                                r.1 + 38,
                                1,
                                DIM,
                                &format!("desde {}", fmt_deadline(rec.unlocked_unix)),
                            );
                        } else {
                            d.fill(r.0 + 16, r.1 + 18, 20, 30, (40, 36, 34, 255));
                            d.fill(r.0 + 12, r.1 + 48, 28, 9, (40, 36, 34, 255));
                            d.text(r.0 + 54, r.1 + 16, 1, DIM, "???");
                        }
                        d.text(r.0 + 16, r.1 + r.3 as i32 - 28, 1, DIM, t.hint());
                    }
                }
            }
        });

        hot = frame_hot;
        rows = frame_rows;
        intro = frame_intro;

        // A arte do selecionado entra no fundo (um arquivo por tentativa,
        // mesmo orçamento de decode por frame que a estante usa).
        if let Some(e) = entries.get(selected) {
            if art_tried.insert(e.rom.sha1.clone()) {
                ensure_art(cab, &e.rom.path, &e.rom.sha1);
            }
        }

        // Pacing do balcão (o idle paca em 16ms; dormimos o resto do tick).
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        }
        next = Instant::now() + frame;
    }
}

// --- helpers ---------------------------------------------------------------

fn hit(r: &RectT, x: i32, y: i32) -> bool {
    x >= r.0 && y >= r.1 && (x - r.0) < r.2 as i32 && (y - r.1) < r.3 as i32
}

fn selected_entry<'a>(entries: &'a [CatalogEntry], selected: usize) -> Option<&'a CatalogEntry> {
    entries.get(selected)
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

/// Etiqueta à direita da linha, contando o estado físico do exemplar (§7).
fn row_tag(loc: &Persisted, e: &CatalogEntry) -> String {
    let sha = &e.rom.sha1;
    if loc.service_game.as_deref() == Some(sha.as_str()) {
        return "EM SERVIÇO — paga por hora".into();
    }
    if loc.owns(sha) {
        return "COLEÇÃO".into();
    }
    match loc.insert_check(sha) {
        InsertCheck::Ok if loc.hourly.as_ref().map(|h| h.game == *sha).unwrap_or(false) => format!(
            "AVULSA — trocas usadas: {}",
            loc.hourly.as_ref().map(|h| h.swaps_used).unwrap_or(0)
        ),
        InsertCheck::Ok => format!(
            "TICKET até {}",
            fmt_deadline(loc.active_ticket.as_ref().map(|t| t.deadline_unix).unwrap_or(0))
        ),
        InsertCheck::TicketExpired => "ATRASADO — devolva!".into(),
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
                    let _ = loc.buy(&e.rom.sha1, prices);
                    loc.save(&dirs::locadora_path())?;
                    announce(cab, &["comprado! caixa nova, contracapa e manual destravados"]);
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
    }
    // Troféus pipocam depois de qualquer ação (design §9).
    let novos = check_trophies(loc, sixteen_locadora::rules::now_unix());
    if !novos.is_empty() {
        loc.save(&dirs::locadora_path())?;
        for t in &novos {
            announce(cab, &[&format!("🏆 TROFÉU: {}", t.title())]);
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

/// Decodifica a arte do cartucho (ou da capa) e registra a textura sob um
/// id derivado do sha1 — uma tentativa por jogo por visita.
fn ensure_art(cab: &mut Cabinet, rom_path: &str, sha: &str) {
    let stem_art = |kind: &str| find_local_art(&dirs::assets_dir().join(kind), rom_path);
    let Some(path) = stem_art("cartridge").or_else(|| stem_art("cover")) else {
        return;
    };
    let Ok((w, h, rgba)) = decode_art_scaled(&path, 512) else {
        return;
    };
    cab.set_image(art_id(sha), w, h, &rgba);
}

/// Namespace próprio das texturas da locadora (FNV-1a do sha1 + marca).
pub fn art_id(sha: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in sha.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    0x1D0C_A000_0000_0000 | (hash & 0x0000_FFFF_FFFF_FFFF)
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
