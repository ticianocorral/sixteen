//! The selector shelf, factored out of the `selector` binary so `sixteen` can
//! show it between games: a scrollable grid of covers (or a multicart-style
//! list when no cover art is around) with a details panel, mouse and gamepad
//! navigation only (plan revision: no keyboard shortcuts, so type-to-search
//! is gone too — except for the shelf's own title filter, the one deliberate
//! exception, same as the pause book's free-text note). Cover/logo art is
//! local — dropped by hand into `assets/cover/`/`assets/logo/` (plan §4.3) —
//! matched by the ROM's file name (peeling variant tags down to the base
//! name) and decoded lazily as tiles scroll into view. The details panel itself is drawn flat by
//! `Cabinet` (plan revision: "o painel nao pode estar dentro da TV") —
//! outside the tube's warp, like the in-game side panel — so this module only
//! ever lays out the grid inside `Cabinet::shelf_screen_size`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use anyhow::Result;
use sixteen_domain::{Catalog, CatalogEntry, Order};
use sixteen_platform::tr;
use sixteen_platform::{
    Cabinet, MenuInput, MenuMode, MenuNav, PanelSection, Platform, Screen, ShelfButton,
    ShelfPanelInfo,
};

use crate::idle;

// Landscape, not portrait: the cover art dropped into `assets/cover/` is
// horizontal (box-front scans, not spine-style portrait covers), so the tile
// itself is wide-short rather than tall-narrow — `image_fit` would otherwise
// letterbox a landscape image down to a sliver inside a portrait frame. Sized
// up from the original 200x150 (same 4:3 tile shape, ~30% larger) so covers
// read clearly on the shelf.
const TILE_W: u32 = 260;
const TILE_H: u32 = 195;
const GAP: u32 = 18;
const MARGIN: i32 = 28;
/// Room reserved at the header row for the "N games" label and the filter
/// box — both sit above the favorites/grid, inside the tube like the rest
/// of the shelf's own content.
const HEADER_H: i32 = 28;
/// Margem interna do móvel: as capas das pontas não encostam nos montantes.
const SHELF_INSET: i32 = 14;
/// Thin strip reserved at the grid's right edge for the scroll indicator
/// (plan revision: "mostrar rolagem na tv para visualizar os jogos") —
/// scrolling itself already worked (wheel/d-pad/page keys); this just makes
/// it visible, and how much more there is to see.
const SCROLLBAR_W: u32 = 6;
const SCROLLBAR_RESERVE: u32 = 16;

/// Row height when the shelf falls back to a plain text list (no cover art
/// loaded at all, plan §3.1) — a multicart-style menu instead of a grid of
/// empty tiles.
const LIST_ROW_H: u32 = 22;
const LIST_GAP: u32 = 6;

/// A faixa de favoritos acima da grade (plan revision: "adicionar marcador
/// de favorito nos jogos; mostrar em uma categoria acima") — uma fileira só,
/// com rolagem horizontal: mais favoritos do que cabem ficam a um movimento
/// do cursor em vez de cortados. ("Jogados recentemente" saiu: o Histórico,
/// no cabeçalho, já lista os últimos jogados.)
const STRIP_TILE_W: u32 = 160;
const STRIP_TILE_H: u32 = 120;
const STRIP_GAP: u32 = 14;
const FAV_LABEL_H: i32 = 24;
/// O rótulo da faixa de favoritos — função porque `tr!` não cabe em `const`.
fn fav_title() -> &'static str {
    tr!("favorites", "favoritos")
}
/// Teto bem acima do que cabe na tela de uma vez — a faixa rola, mas
/// milhares de favoritos não devem crescê-la sem limite.
const STRIP_CAP: usize = 60;

/// O título da listagem geral (plan revision: "colocar um titulo (Todos os
/// jogos)"), na mesma altura do rótulo dos favoritos.
const ALL_GAMES_LABEL_H: i32 = 36;
/// O rótulo da seção principal — função porque `tr!` não cabe em `const`.
fn all_games_title() -> &'static str {
    tr!("all games", "todos os jogos")
}

/// The title filter box (plan revision: "colocar filtro para facilitar o
/// encontro dos games na lista") — the shelf's own one deliberate keyboard
/// exception, same pattern as the pause book's free-text note.
const FILTER_BOX_W: u32 = 240;
const FILTER_BOX_H: u32 = 24;
const FILTER_LIMIT: usize = 40;

/// "Histórico" button (plan revision: "colocar um botão para histórico
/// listando os jogos mais jogados") — drawn in the header, left of the
/// filter box, same row/height.
const HISTORY_BTN_W: u32 = 110;

/// "Atualizar" button (plan revision: "colocar botão de atualizar estante
/// do lado do histórico") — same header row, to the "histórico" button's
/// left; re-scans roms/ without leaving the shelf.
const REFRESH_BTN_W: u32 = 110;

const BG: (u8, u8, u8) = (18, 18, 20);
const HILITE: (u8, u8, u8, u8) = (240, 200, 80, 255);
const TEXT: (u8, u8, u8) = (232, 232, 232);
const DIM: (u8, u8, u8) = (150, 150, 158);
/// A linguagem "fósforo" dos botões que vivem dentro da TV (plan revision):
/// sem caixa, texto na cor da chave Power/Reset do console (o violeta
/// `SWITCH_PURPLE` do cabinet, aqui clareado para ler contra o fundo escuro
/// do tubo) com um sublinhado no ativo, dim no inativo — o mesmo jeito do
/// OSD "CH 3", só que na cor do console em vez do verde.
const FOSFORO: (u8, u8, u8) = (186, 138, 244);
/// Text color on top of the `HILITE` selection bar in list mode — dark, for
/// contrast against the bright fill.
const HILITE_TEXT: (u8, u8, u8) = (24, 20, 12);
/// How opaque the shelf sits over the resting TV static (plan §3.3) — era
/// 0.92 (a neve sangrava de leve pelo fundo), mas com as capas 3D vazadas
/// o ruído cinça aparecia nas margens transparentes, embaixo das caixas —
/// uma "borda cinza" contra a madeira. A estante agora assenta opaca; a
/// neve segue viva na tela idle e na entrada (fade-in), que rampa até 1.0.
const SHELF_ALPHA: f32 = 1.0;

/// What the player did on the shelf.
pub enum Pick {
    /// Launch this ROM (already marked played in the catalogue).
    Play {
        rom: PathBuf,
        /// Local logo art (`assets/logo/<rom stem>.*`), if one exists — for
        /// the side panel during play (plan §3.2).
        wheel: Option<PathBuf>,
        /// Local cartridge art (`assets/cartridge/<rom stem>.*`), if one
        /// exists — shown in the panel alongside the logo (plan revision).
        cartridge: Option<PathBuf>,
    },
    /// Clicked "Voltar", or a gamepad's Back button — back out to the
    /// idle/root screen. The app keeps running.
    Back,
    /// Window closed / Cmd-Q — tear the app down.
    Quit,
    /// Clicked "Configurações" — open the settings screen, then come back to
    /// the shelf.
    Settings,
    /// Clicked "Histórico" — open the all-time most-played list (plan
    /// revision), then come back to the shelf.
    History,
    /// Clicked "Atualizar" (plan revision: "adicionar opção de atualizar a
    /// estante para buscar jogos novos sem precisar abrir e fechar o app")
    /// — the caller re-scans `roms/` and reopens the shelf with the fresh
    /// catalog.
    Refresh,
}

/// Knobs for [`run`].
pub struct ShelfOpts {
    pub order: Order,
    /// Headless smoke test: stop after N frames and return [`Pick::Quit`].
    pub max_frames: Option<u64>,
    /// Headless: on the last frame, save the shelf (through the tube) here.
    pub shot: Option<PathBuf>,
    /// Ease in from residual signal-off static instead of cutting in cold —
    /// the static level to fade from (plan §3.3). `None` draws from frame one.
    pub fade_in: Option<f32>,
    /// Headless only: pre-apply this as the title filter from frame one,
    /// as if the player had already typed and committed it — there's no way
    /// to simulate real typing in a headless run, so this is how `--shot`
    /// verifies the filtered view (plan revision).
    pub preset_filter: Option<String>,
    /// RetroAchievements account (plan: `docs/plano-retroachievements.md`,
    /// fase 2) — `Some((user, web_key, connect_token))` enables the per-game
    /// identification
    /// that shows "conquistas: N" in the panel; `None` keeps the shelf
    /// entirely RA-free.
    pub ra: Option<(String, String, String)>,
}

impl Default for ShelfOpts {
    fn default() -> Self {
        Self {
            order: Order::Shelf,
            max_frames: None,
            shot: None,
            fade_in: None,
            preset_filter: None,
            ra: None,
        }
    }
}

/// Texture key for a game's cover — first 64 bits of the SHA1.
fn cover_id(sha1: &str) -> u64 {
    u64::from_str_radix(sha1.get(..16).unwrap_or("0"), 16).unwrap_or(0)
}

/// Texture key for a game's wheel logo — the *next* 64 bits, so it can't
/// collide with any `cover_id`.
fn wheel_id(sha1: &str) -> u64 {
    u64::from_str_radix(sha1.get(16..32).unwrap_or("0"), 16).unwrap_or(0)
}

/// Texture key for a game's cartridge art (plan revision — the shelf panel
/// now shows it alongside the logo, same as the in-game panel does): derived
/// from `wheel_id` with a fixed splitmix64-style salt rather than a third,
/// thinner slice of the sha1, so it keeps the full 64 bits of entropy and
/// can't collide with `cover_id`/`wheel_id`.
fn cartridge_id(sha1: &str) -> u64 {
    wheel_id(sha1) ^ 0x9E37_79B9_7F4A_7C15
}

/// Texture key de uma página do manual em PDF (plan revision: "colocar
/// suporte para abrir manual pdf") — hash do sha1 + página com o bit alto
/// da família badge (nunca colide com as chaves de arte do jogo).
fn manual_page_id(sha1: &str, page: usize) -> u64 {
    format!("{sha1}:{page}")
        .bytes()
        .fold(0x9E37_79B9_7F4A_7C15u64, |mut acc, b| {
            acc ^= b as u64;
            acc = acc.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
            acc ^= acc >> 33;
            acc
        })
        | (1 << 62)
}

/// O PDF do manual da ROM em `dir` (`assets/manual/<nome da rom>.pdf`),
/// com o mesmo casamento por nome/variantes da arte local.
fn find_local_manual(dir: &Path, rom_path: &str) -> Option<PathBuf> {
    let stem = Path::new(rom_path).file_stem()?.to_str()?;
    let mut candidates = vec![stem.to_string()];
    let mut cur = stem.to_string();
    while cur.ends_with(')') {
        match cur.rfind(" (") {
            Some(idx) => {
                cur.truncate(idx);
                candidates.push(cur.clone());
            }
            None => break,
        }
    }
    for cand in candidates {
        let p = dir.join(format!("{cand}.pdf"));
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

/// `(x, y, w, h)` em coordenadas de tela — o rect de um botão do leitor.
type ViewBtn = (i32, i32, u32, u32);

/// O leitor de manual na TV (plan revision: "abrir na tv como o back cover
/// com opção de ver as paginas") — página ampliada + `‹`/`›` + `voltar`.
/// Devolve os rects dos três botões para o hit-test do frame seguinte.
fn draw_manual_view(
    d: &mut Screen,
    sha1: &str,
    page: usize,
    pages: usize,
    page_ready: bool,
) -> (ViewBtn, ViewBtn, ViewBtn) {
    let (w, h) = d.size();
    let (w, h) = (w as i32, h as i32);
    let btn = (110u32, 34u32);
    let voltar = (w / 2 - btn.0 as i32 / 2, h - 64, btn.0, btn.1);
    let prev = (w / 2 - btn.0 as i32 - 90, h - 64, btn.0, btn.1);
    let next = (w / 2 + 90, h - 64, btn.0, btn.1);

    let pages_label = if pages > 0 {
        pages.to_string()
    } else {
        "…".to_string()
    };
    d.text(
        MARGIN,
        MARGIN,
        2,
        TEXT,
        &tr!(
            format!("manual — page {page} of {pages_label}"),
            format!("manual — página {page} de {pages_label}"),
        ),
    );
    let id = manual_page_id(sha1, page);
    let area = (
        MARGIN,
        MARGIN + 60,
        (w - MARGIN * 2) as u32,
        (h - MARGIN * 2 - 140) as u32,
    );
    if page_ready {
        d.image_fit(id, area.0, area.1, area.2, area.3);
    } else if pages > 0 {
        d.text(
            MARGIN,
            area.1 + 20,
            1,
            DIM,
            tr!("rendering the page...", "renderizando a página..."),
        );
    } else {
        d.text(
            MARGIN,
            area.1 + 20,
            1,
            DIM,
            tr!("opening the manual...", "abrindo o manual..."),
        );
    }

    let mut draw_nav = |r: &(i32, i32, u32, u32), label: &str| {
        draw_fosforo_btn(d, *r, label, true);
    };
    draw_nav(&prev, tr!("< page", "< página"));
    draw_nav(&next, tr!("page >", "página >"));
    draw_nav(&voltar, tr!("back", "voltar"));
    (prev, next, voltar)
}

/// Texture key for an achievement badge — hashed from the badge name with a
/// fixed splitmix64-style mix (the name has no sha1 of its own to slice),
/// ending in a high bit the game-art keys never set.
fn badge_image_id(name: &str) -> u64 {
    name.bytes().fold(0x9E37_79B9_7F4A_7C15u64, |mut acc, b| {
        acc ^= b as u64;
        acc = acc.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
        acc ^= acc >> 33;
        acc
    }) | (1 << 62)
}

/// Texture key for a game's back-cover art (plan revision: "abaixo da
/// logo... colocar o back cover tambem") — same derivation as
/// `cartridge_id`, a different fixed salt so it can't collide with any of
/// the other three.
/// The favorites strip's scroll affordance (plan revision: "inserir
/// rolagem horizontal em favoritos"; depois "nao consigo
/// clicar nas setas") — real buttons in the strip's header row, right
/// side, same outline style as the header's "histórico" button: "<" slides
/// the strip back a tile, ">" forward, each click one step. Drawn whenever
/// the strip overflows (like the panel's own "Cima"/"Baixo" pair), dimmed
/// at the end it can't move past.
/// `(left, right)` header-button rects for one strip, screen-local.
type StripArrows = ((i32, i32, u32, u32), (i32, i32, u32, u32));

pub fn strip_arrow_rects(scr_w: u32, y0: i32) -> StripArrows {
    let bw = 36u32;
    let bh = 22u32;
    let by = y0 - FAV_LABEL_H + 1;
    let right = (scr_w as i32 - MARGIN - bw as i32, by, bw, bh);
    let left = (right.0 - 8 - bw as i32, by, bw, bh);
    (left, right)
}

fn draw_strip_arrows(d: &mut Screen, scr_w: u32, y0: i32, scroll: usize, len: usize, vis: usize) {
    let (left, right) = strip_arrow_rects(scr_w, y0);
    for (rect, glyph, more) in [(left, "<", scroll > 0), (right, ">", scroll + vis < len)] {
        draw_fosforo_btn(d, rect, glyph, more);
    }
}

/// The shelf's achievements view (plan revision: "mostrar a lista de
/// conquistas e pontuação total dentro da tv com botão de voltar, parecido
/// com o back cover") — title, "N/M — X de Y pontos", then the per-
/// achievement rows (earned in green with a star-ish [x], locked dim),
/// scrolled by `top`, with a flat "voltar" button bottom-centre. Returns
/// that button's screen-local rect for the click hit-test.
fn draw_achievements_view(
    d: &mut Screen,
    data: Option<&crate::ra::ShelfAchievements>,
    badges: &std::collections::HashMap<String, u64>,
    top: usize,
) -> (i32, i32, u32, u32) {
    const ROW_H: i32 = 22;
    let (w, h) = d.size();
    let (w, h) = (w as i32, h as i32);
    let x = MARGIN;
    let green = (60, 230, 70);
    let voltar = (w / 2 - 70, h - 64, 140u32, 34u32);

    match data {
        None => {
            d.text(
                x,
                MARGIN,
                2,
                DIM,
                tr!("achievements unavailable", "conquistas indisponíveis"),
            );
        }
        Some(list) => {
            let (all_pts, got_pts) = list.points();
            d.text_wrapped(x, MARGIN, (w - MARGIN * 2) as u32, 2, TEXT, &list.title);
            let earned_n = list.earned.len();
            let total_n = list.achievements.len();
            let hardcore_n = list.earned.values().filter(|&&hc| hc).count();
            d.text(
                x,
                MARGIN + 56,
                1,
                DIM,
                &tr!(
                    format!(
                        "achievements {earned_n}/{total_n} ({hardcore_n} hardcore) — {got_pts} of {all_pts} points"
                    ),
                    format!(
                        "conquistas {earned_n}/{total_n} ({hardcore_n} hardcore) — {got_pts} de {all_pts} pontos"
                    ),
                ),
            );
            let rows = (h - MARGIN * 2 - 130) / ROW_H;
            let visible = rows.max(1) as usize;
            // Hardcore em dourado (a cor do prêmio no RA), softcore em
            // verde — plan revision: "como sei qual tipo ganhei ou já
            // tenho?".
            const GOLD: (u8, u8, u8) = (238, 192, 62);
            for (i, a) in list.achievements.iter().enumerate().skip(top).take(visible) {
                let y = MARGIN + 84 + (i - top) as i32 * ROW_H;
                let (mark, color) = match list.earned.get(&a.id) {
                    Some(true) => ("[HC]", GOLD),
                    Some(false) => ("[SC]", green),
                    None => ("[ ]", DIM),
                };
                let label = format!("{mark} {} ({} pts)", a.title, a.points);
                // Badge 18×18 à esquerda (quando a textura já baixou); o
                // texto abre mão do espaço dele.
                const BADGE: u32 = 18;
                if let Some(id) = badges.get(&a.badge) {
                    d.image_fit(*id, x, y, BADGE, BADGE);
                }
                let text_x = x + 24;
                // Truncate to the tube's width so nothing wraps.
                let max_chars = ((w - MARGIN * 2 - 26) / 9).max(8) as usize;
                let label: String = if label.chars().count() > max_chars {
                    format!(
                        "{}...",
                        label.chars().take(max_chars - 3).collect::<String>()
                    )
                } else {
                    label
                };
                d.text(text_x, y, 1, color, &label);
            }
            if list.achievements.len() > visible {
                let note = if top + visible < list.achievements.len() {
                    tr!("v to scroll", "v para rolar")
                } else {
                    "^ v"
                };
                d.text(w - MARGIN - 90, MARGIN + 56, 1, DIM, note);
            }
        }
    }

    // "voltar" — a linguagem fósforo do resto dos botões dentro da TV.
    draw_fosforo_btn(d, voltar, tr!("back", "voltar"), true);
    voltar
}

fn backcover_id(sha1: &str) -> u64 {
    wheel_id(sha1) ^ 0xC2B2_AE3D_27D4_EB4F
}

/// `dir/<rom's file stem>.{png,jpg,jpeg}`, in that order — the convention for
/// locally-supplied art: `roms/Aladdin.sfc` matches `assets/cover/Aladdin.png`.
/// If the exact stem misses, the trailing "(...)" tags are peeled one group at
/// a time ("X (USA) (Rev 1)" -> "X (USA)" -> "X"), so a single base-named art
/// file serves every variant ROM of the same game.
pub fn find_local_art(dir: &Path, rom_path: &str) -> Option<PathBuf> {
    let stem = Path::new(rom_path).file_stem()?.to_str()?;
    let mut candidates = vec![stem.to_string()];
    let mut cur = stem.to_string();
    while cur.ends_with(')') {
        match cur.rfind(" (") {
            Some(idx) => {
                cur.truncate(idx);
                candidates.push(cur.clone());
            }
            None => break,
        }
    }
    for cand in candidates {
        for ext in ["png", "jpg", "jpeg"] {
            let p = dir.join(format!("{cand}.{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// Mark `entry` played and build its launch — shared by Enter/A confirm and
/// clicking the already-selected tile.
fn pick_play(
    catalog: &Catalog,
    logo_dir: &Path,
    cartridge_dir: &Path,
    entry: &CatalogEntry,
) -> Pick {
    let _ = catalog.mark_played(&entry.rom.sha1);
    let wheel = find_local_art(logo_dir, &entry.rom.path);
    let cartridge = find_local_art(cartridge_dir, &entry.rom.path);
    Pick::Play {
        rom: PathBuf::from(&entry.rom.path),
        wheel,
        cartridge,
    }
}

/// O MÓVEL em volta das capas: o painel de fundo (madeira escura com veios
/// verticais, escurecendo para os cantos como o fundo de um armário),
/// os dois montantes laterais e a travessa do topo — a grade vira o
/// interior de uma estante, em vez de capas soltas no escuro. `top` é onde
/// a travessa assenta (logo abaixo do cabeçalho).
fn draw_cabinet(d: &mut Screen, scr_w: u32, scr_h: u32, top: i32) {
    const POST: i32 = 12;
    let x0 = MARGIN - POST - 4;
    let x1 = scr_w as i32 - MARGIN + 4;
    let bottom = scr_h as i32;
    let inner_w = (x1 - x0 - POST * 2).max(1) as u32;
    let inner_h = (bottom - top).max(1) as u32;
    let ix = x0 + POST;

    // Painel de fundo: tom base + veios verticais em faixas de tons vizinhos.
    d.fill(ix, top, inner_w, inner_h, (20, 16, 14, 255));
    let mut gx = ix;
    let mut k = 0u32;
    while gx < x1 - POST {
        let tone = [(21, 17, 15), (18, 15, 13), (23, 19, 16), (20, 16, 14)][(k % 4) as usize];
        let wv = 9 + (k * 7 % 13) as i32;
        d.fill(
            gx,
            top,
            wv.min(x1 - POST - gx) as u32,
            inner_h,
            (tone.0, tone.1, tone.2, 255),
        );
        gx += wv;
        k += 1;
    }
    // Vinheta: o fundo escurece para as laterais e para baixo.
    for i in 0..10 {
        let a = (60 - i * 6) as u8;
        d.fill(ix + i * 3, top, 3, inner_h, (0, 0, 0, a));
        d.fill(x1 - POST - (i + 1) * 3, top, 3, inner_h, (0, 0, 0, a));
    }
    // Sombra da travessa no fundo, logo abaixo dela.
    for i in 0..8 {
        d.fill(
            ix,
            top + 10 + i * 2,
            inner_w,
            2,
            (0, 0, 0, (90 - i * 11) as u8),
        );
    }

    // Montantes laterais: face, quina iluminada e fio de sombra interno.
    for (px, light_left) in [(x0, true), (x1 - POST, false)] {
        d.fill(px, top - 10, POST as u32, inner_h + 10, (52, 38, 29, 255));
        let edge = if light_left { px } else { px + POST - 2 };
        d.fill(edge, top - 10, 2, inner_h + 10, (84, 64, 50, 255));
        let inner = if light_left { px + POST - 1 } else { px };
        d.fill(inner, top, 1, inner_h, (26, 18, 12, 255));
    }
    // Travessa do topo.
    d.fill(x0, top - 10, (x1 - x0) as u32, 10, (56, 41, 31, 255));
    d.fill(x0, top - 10, (x1 - x0) as u32, 2, (90, 70, 55, 255));
    d.fill(x0, top - 1, (x1 - x0) as u32, 1, (32, 22, 15, 255));
}

/// Whether `entry`'s title matches a filter query — empty matches everything,
/// otherwise a case-insensitive substring test (plan revision: "colocar
/// filtro para facilitar o encontro dos games na lista").
/// The panel's own file facts (plan revision: "o painel... muito vazio") —
/// always available straight from the scan (or, for `playtime_secs`, from
/// the per-game sidecar `runner::total_playtime_secs` already writes), no
/// DAT/local art needed, so a plain title-only game still gets a panel with
/// something in it besides two buttons. Appended after any No-Intro extras.
/// (A "jogado" row lived here once — the last-played date is the
/// Histórico's job, and the row was what overflowed into the panel's
/// buttons.)
/// A prateleira de madeira sob uma fileira de caixas (a estante de
/// verdade, do mock `estante_mock` para o app): fita LED no lábio superior
/// com brilho escorrendo na parede (atrás das capas — as caixas desenhadas
/// por cima recortam o brilho e ficam retroiluminadas), três pranchas de
/// tons vizinhos com veios e juntas, boca escura e mão francesa
/// (quarto-de-círculo) sob as duas pontas — curta, para não invadir a copy
/// da fileira de baixo. Mora no vão de `GAP`/`STRIP_GAP` entre fileiras —
/// nenhuma geometria de clique muda.
fn draw_shelf_board(d: &mut Screen, x: i32, w: u32, y: i32, h: i32) {
    // O tampo visto de cima, entrando para o fundo: uma faixa mais clara
    // acima do lábio, com a quina do fundo escura — a prateleira ganha
    // profundidade e as capas pousam EM CIMA dela.
    const DEPTH: i32 = 9;
    d.fill(x, y - DEPTH, w, DEPTH as u32, (70, 53, 41, 255));
    d.fill(x, y - DEPTH, w, 1, (28, 19, 13, 255));
    d.fill(x, y - DEPTH + 1, w, 2, (56, 42, 33, 255));
    // A quina da frente do tampo pegando a luz.
    d.fill(x, y - 1, w, 1, (112, 90, 72, 255));

    // O tampo: três pranchas de tons vizinhos (madeira de verdade nunca é
    // uma cor só), lábio lavado pela fita, veios e juntas, boca escura.
    let face_h = (h - 6).max(1);
    let prancha = (w as i32 / 3).max(1);
    for (p, tom) in [(0, (62, 47, 36)), (1, (59, 45, 34)), (2, (65, 49, 38))] {
        let px = x + p * prancha;
        let pw = if p == 2 {
            w as i32 - 2 * prancha
        } else {
            prancha
        };
        d.fill(
            px,
            y + 2,
            pw as u32,
            face_h as u32,
            (tom.0, tom.1, tom.2, 255),
        );
    }
    d.fill(x, y, w, 2, (96, 76, 60, 255));
    let veios = [
        (5u32, 8i32, (52, 39, 30u8)),
        (7, 30, (55, 41, 32u8)),
        (9, 14, (50, 38, 29u8)),
    ];
    for (dy, ox, c) in veios {
        if h > dy as i32 + 6 {
            d.fill(
                x + ox,
                y + dy as i32,
                w.saturating_sub(ox as u32 + 24),
                1,
                (c.0, c.1, c.2, 255),
            );
        }
    }
    for p in [1, 2] {
        d.fill(x + p * prancha, y + 2, 1, face_h as u32, (40, 28, 20, 255));
    }
    d.fill(x, y + h - 4, w, 4, (36, 26, 19, 255));

    // Sombra do tampo na parede, embaixo.
    for (i, c) in [(13u8, 11u8, 10u8), (16u8, 14u8, 12u8), (19u8, 17u8, 15u8)]
        .into_iter()
        .enumerate()
    {
        d.fill(
            x + 4,
            y + h + i as i32,
            w.saturating_sub(8),
            1,
            (c.0, c.1, c.2, 255),
        );
    }

    // Mão francesa nos cantos: quarto-de-círculo sob cada ponta — r curto
    // para a ponta não descer na copy da fileira de baixo (o vão entre a
    // prateleira e o que vem depois é de 14 a 18px).
    let r: i32 = 8;
    for i in 0..r {
        let dx = (((r * r - i * i) as f32).sqrt().round() as i32).max(1);
        d.fill(x + 2, y + h + i, dx as u32, 1, (52, 38, 29, 255));
        d.fill(
            x + w as i32 - 2 - dx,
            y + h + i,
            dx as u32,
            1,
            (52, 38, 29, 255),
        );
    }
}

/// Como uma capa aparece na prateleira: selecionada pelo cursor (d-pad
/// ou clique), sob o mouse e/ou marcada como favorita.
#[derive(Clone, Copy)]
struct CoverLook {
    /// Dentro do móvel (a listagem geral): luz de vitrine, sombra no fundo
    /// e o tom abaixo fora da seleção. As faixas de cima ficam fora dele.
    on_shelf: bool,
    selected: bool,
    hovered: bool,
    favorite: bool,
}

/// A cor do filete de seleção no tampo — a luz quente da vitrine.
const SELECT_GLOW: (u8, u8, u8) = (246, 236, 214);

/// Uma capa EM PÉ no slot `(x, y, w, h)`, apoiada no tampo da prateleira
/// (a base do slot). Vale igual para capa chapada (2D) e caixa 3D vazada:
/// as duas encaixam pela ALTURA do conteúdo (as margens transparentes do
/// render 3D não contam) e pousam na madeira, centradas no slot.
///
/// - 2D: vira caixa — lateral escura à direita, fio de luz na quina e no
///   topo, sombra de contato na madeira.
/// - 3D: a arte já tem volume; só a sombra de contato.
/// - Selecionada: sobe alguns pixels, com um halo violeta na parede atrás
///   e um filete aceso no tampo — o cursor do gamepad, igual nos dois
///   tipos de capa.
/// - Sem arte: uma caixa genérica com o título, para a falta de capa
///   parecer de propósito.
fn draw_cover(d: &mut Screen, id: u64, title: &str, slot: (i32, i32, u32, u32), look: CoverLook) {
    let (x, y, w, h) = slot;
    let lift = if look.selected { 6 } else { 0 };
    let base = y + h as i32;
    let vazada = d.image_vazado(id);
    let (sx, sy, sw, sh) = (x + 2, y - lift, w.saturating_sub(4), h.saturating_sub(4));

    // Onde a capa vai cair: pela altura do conteúdo, pousada na base,
    // centrada — calculado antes, para o halo ficar ATRÁS dela.
    let fit = d.image_content_size(id).map(|(iw, ih)| {
        let scale = (sw as f32 / iw as f32).min(sh as f32 / ih as f32);
        let (dw, dh) = (
            ((iw as f32 * scale).round() as u32).max(1),
            ((ih as f32 * scale).round() as u32).max(1),
        );
        (
            sx + (sw as i32 - dw as i32) / 2,
            sy + sh as i32 - dh as i32,
            dw,
            dh,
        )
    });
    let (cx, cy, cw, ch) = fit.unwrap_or_else(|| generic_box_rect((sx, sy, sw, sh)));

    // A luz da vitrine sobre o slot — mais forte na capa selecionada: o
    // cursor é LUZ, como numa estante de verdade.
    if look.on_shelf {
        // A sombra da capa no fundo do móvel: deslocada para cima e para a
        // direita, difusa — é o que descola a capa da parede.
        for (off, a) in [(9, 40u8), (6, 60), (3, 70)] {
            d.fill(
                cx + off,
                cy - off / 2,
                cw,
                ch.saturating_sub(4),
                (0, 0, 0, a),
            );
        }
    }

    // Sombra de contato na madeira (mais difusa com a capa erguida).
    let shadow_a = if look.selected { 70 } else { 120 };
    d.fill(cx + 2, base, cw.saturating_sub(4), 3, (10, 6, 4, shadow_a));
    d.fill(cx - 2, base + 3, cw + 6, 2, (10, 6, 4, shadow_a / 2));

    if fit.is_some() {
        d.image_fit_content_bottom(id, sx, sy, sw, sh);
        if !vazada {
            // Volume de caixa: lateral, quina e topo.
            d.fill(
                cx + cw as i32,
                cy + 3,
                4,
                ch.saturating_sub(3),
                (14, 12, 12, 230),
            );
            d.fill(cx, cy, 1, ch, (255, 255, 255, 40));
            d.fill(cx, cy, cw, 1, (255, 255, 255, 50));
        }
    } else {
        draw_generic_box(d, title, (cx, cy, cw, ch));
    }

    if look.on_shelf {
        draw_stand(d, (cx, cy, cw, ch), base);
    }

    if look.selected {
        // O filete aceso no tampo, sob a capa, e um fio de luz no topo dela.
        d.fill(
            cx - 4,
            base + 1,
            cw + 8,
            2,
            (SELECT_GLOW.0, SELECT_GLOW.1, SELECT_GLOW.2, 255),
        );
        // Contorno de luz quente colado na capa, com um brilho em volta.
        for (pad, a) in [(4, 40u8), (2, 90)] {
            d.outline(
                cx - pad,
                cy - pad,
                cw + pad as u32 * 2,
                ch + pad as u32 * 2,
                2,
                (SELECT_GLOW.0, SELECT_GLOW.1, SELECT_GLOW.2, a),
            );
        }
        d.fill(cx, cy, cw, 2, (255, 250, 240, 150));
    } else if look.on_shelf {
        // Fora da luz: um tom abaixo, para a selecionada saltar.
        d.fill(cx, cy, cw, ch, (0, 0, 0, 70));
    }
    if look.favorite {
        d.star(cx + cw as i32 - 12, cy + 12, 8, (214, 40, 40));
    }
    if look.hovered {
        draw_play_icon(d, cx + cw as i32 - 24, cy + ch as i32 - 24);
    }
}

/// O suporte de acrílico que segura a capa em pé — o expositor de loja:
/// uma base chata transparente no tampo, um pouco mais estreita que a
/// capa, e a aba da frente subindo alguns pixels por cima do pé da capa
/// (é ela que segura). Transparente de verdade: a capa e a madeira
/// aparecem através, só as quinas pegam luz.
fn draw_stand(d: &mut Screen, cover: (i32, i32, u32, u32), base: i32) {
    let (cx, _cy, cw, ch) = cover;
    let sw = ((cw as f32 * 0.62) as u32).clamp(28, cw.saturating_sub(8).max(28));
    let sx = cx + (cw as i32 - sw as i32) / 2;
    let lip = (ch / 14).clamp(4, 9) as i32;
    // A base no tampo: placa fina com a quina da frente acesa.
    d.fill(sx - 3, base - 1, sw + 6, 4, (210, 222, 232, 56));
    d.fill(sx - 3, base + 2, sw + 6, 1, (225, 235, 245, 120));
    // A aba da frente, por cima do pé da capa.
    d.fill(sx, base - lip, sw, lip as u32, (214, 226, 236, 70));
    d.fill(sx, base - lip, sw, 1, (244, 248, 252, 230));
    d.fill(sx, base - lip, 1, lip as u32, (240, 246, 252, 110));
    d.fill(
        sx + sw as i32 - 1,
        base - lip,
        1,
        lip as u32,
        (240, 246, 252, 110),
    );
}

/// O retângulo da caixa genérica dentro do slot: em pé, proporção de
/// caixa de cartucho em retrato, pousada na base.
fn generic_box_rect(slot: (i32, i32, u32, u32)) -> (i32, i32, u32, u32) {
    let (x, y, w, h) = slot;
    let bw = ((h as f32 * 0.72) as u32).min(w);
    (x + (w as i32 - bw as i32) / 2, y, bw, h)
}

/// A capa que falta: caixa escura com lombada, faixa de rodapé e o título
/// escrito — para a falta de arte parecer de propósito.
fn draw_generic_box(d: &mut Screen, title: &str, rect: (i32, i32, u32, u32)) {
    let (bx, by, bw, bh) = rect;
    d.fill(bx, by, bw, bh, (38, 36, 48, 255));
    d.fill(bx, by, 6, bh, (24, 22, 32, 255));
    d.fill(
        bx + 6,
        by + bh as i32 - 18,
        bw.saturating_sub(6),
        18,
        (60, 56, 76, 255),
    );
    d.text_wrapped(
        bx + 12,
        by + 12,
        bw.saturating_sub(20),
        1,
        TEXT,
        &title.to_uppercase(),
    );
}

fn game_info_lines(entry: &CatalogEntry, playtime_secs: u64) -> Vec<(String, String)> {
    vec![
        (
            tr!("cartridge", "cartucho").to_string(),
            cart_size(entry.rom.size),
        ),
        (
            tr!("total time", "tempo total").to_string(),
            format_playtime(playtime_secs),
        ),
    ]
}

/// O id fixo da textura da logo da IGDB — 0x49474442 é "IGDB" em ASCII.
/// Registrada uma vez, lida pelo cabeçalho da seção de nota.
const IGDB_ICON_IMG: u64 = 0x4947_4442;

/// O id fixo da textura do ícone "H" do HowLongToBeat — 0x484C5442 é "HLTB"
/// em ASCII. Registrada uma vez (como as medalhas do RA), lida por todo
/// painel que tiver bloco de tempos.
const HLTB_ICON_IMG: u64 = 0x484C_5442;

/// O bloco HowLongToBeat do painel — as 4 barras do card do site (história
/// principal, + extras, completista, todos os estilos), a largura
/// proporcional contra o maior tempo do jogo. Minuto zero vale como sem
/// voto (arredondamento de jogo curtíssimo), e a cor por estilo segue o
/// site: azul, índigo, vermelho, teal. Tempos da tabela embutida por CRC32
/// (`sixteen_domain::hltb`), nada de rede.
fn hltb_bars(i: &sixteen_domain::InfosData, icon: u64) -> sixteen_platform::HltbBars {
    let estilos = [
        (
            tr!("main story", "história principal"),
            i.hltb_main,
            (52, 124, 246),
        ),
        (
            tr!("main + extras", "principal + extras"),
            i.hltb_extra,
            (86, 88, 204),
        ),
    ];
    let maior = estilos
        .iter()
        .filter_map(|(_, v, _)| v.filter(|minutos| *minutos > 0))
        .max()
        .unwrap_or(1) as f32;
    sixteen_platform::HltbBars {
        icon,
        rows: estilos
            .into_iter()
            .filter_map(|(label, minutos, color)| {
                let minutos = minutos.filter(|minutos| *minutos > 0)?;
                Some(sixteen_platform::HltbBar {
                    label: label.to_string(),
                    text: format_playtime(minutos * 60),
                    frac: minutos as f32 / maior,
                    color,
                })
            })
            .collect(),
    }
}

/// O ícone "H" registrado na `Cabinet` (uma vez só) — o bloco de tempos o
/// usa de cabeçalho, ao lado do "HowLongToBeat" dim.
fn hltb_icon_registered(cab: &mut Cabinet) -> u64 {
    if !cab.has_image(HLTB_ICON_IMG) {
        if let Ok(icon) = image::load_from_memory(crate::HLTB_ICON_PNG) {
            let icon = icon.to_rgba8();
            cab.set_image(HLTB_ICON_IMG, icon.width(), icon.height(), icon.as_raw());
        }
    }
    HLTB_ICON_IMG
}

/// O ícone "IGDB" registrado na `Cabinet` (uma vez só) — o cabeçalho da
/// seção de nota o usa, ao lado do "IGDB" em negrito.
fn igdb_icon_registered(cab: &mut Cabinet) -> u64 {
    if !cab.has_image(IGDB_ICON_IMG) {
        if let Ok(icon) = image::load_from_memory(crate::IGDB_ICON_PNG) {
            let icon = icon.to_rgba8();
            cab.set_image(IGDB_ICON_IMG, icon.width(), icon.height(), icon.as_raw());
        }
    }
    IGDB_ICON_IMG
}

/// The cartridge size the box used to print, in megabits (plan revision:
/// "abaixo do tamanho do jogo ... o tamanho do cartucho (ex: SF Alpha 2 é
/// 32 mega)") — the ROM's size with a 512-byte copier header stripped,
/// rounded up to the mask sizes SNES carts actually came in (a 512 KB game
/// is "4 megas", a 4 MB one "32 megas").
fn cart_size(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    const MASK_SIZES: [u64; 12] = [2, 4, 8, 12, 16, 20, 24, 28, 32, 40, 48, 64];
    let eff = if bytes % 0x2000 == 512 {
        bytes - 512
    } else {
        bytes
    };
    let mbit = (eff * 8).div_ceil(MIB); // round up to whole megabits
    let snapped = MASK_SIZES
        .iter()
        .copied()
        .find(|&s| mbit <= s)
        .unwrap_or(mbit);
    if snapped == 1 {
        tr!("1 meg", "1 mega").to_string()
    } else {
        tr!(format!("{snapped} megs"), format!("{snapped} megas"))
    }
}

/// Total powered-on time for a game (plan revision: "mostrar tempo total de
/// jogo") — `0` (never turned on, or never played) reads as "nunca" rather
/// than a confusing "0min".
fn format_playtime(secs: u64) -> String {
    if secs == 0 {
        return tr!("never", "nunca").to_string();
    }
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    match (h, m) {
        (0, 0) => tr!("under 1 min", "menos de 1min").to_string(),
        (0, m) => format!("{m}min"),
        (h, m) => format!("{h}h {m}min"),
    }
}

/// O botão de play sob o mouse: um CD (disco prateado, furo central) com
/// a seta de play, no canto inferior direito da capa sob o cursor — o
/// convite para o clique que lança o jogo.
fn draw_play_icon(d: &mut Screen, cx: i32, cy: i32) {
    let r = 17;
    for dy in -r..=r {
        let dx = ((r * r - dy * dy) as f32).sqrt().round() as i32;
        d.fill(cx - dx, cy + dy, (dx * 2) as u32, 1, (176, 182, 192, 235));
    }
    let r2 = 4;
    for dy in -r2..=r2 {
        let dx = ((r2 * r2 - dy * dy) as f32).sqrt().round() as i32;
        d.fill(cx - dx, cy + dy, (dx * 2) as u32, 1, (24, 24, 28, 255));
    }
    // a seta: triângulo apontando à direita, a partir do centro
    for t in 0..11 {
        let half = 6 - (t * 6 + 5) / 11;
        d.fill(
            cx - 1 + t,
            cy - half,
            1,
            (half * 2) as u32,
            (20, 20, 26, 255),
        );
    }
}

fn filter_box_rect(scr_w: u32) -> (i32, i32, u32, u32) {
    (
        scr_w as i32 - MARGIN - FILTER_BOX_W as i32,
        MARGIN - 18,
        FILTER_BOX_W,
        FILTER_BOX_H,
    )
}

fn history_button_rect(scr_w: u32) -> (i32, i32, u32, u32) {
    let (fx, fy, _, fh) = filter_box_rect(scr_w);
    (fx - 12 - HISTORY_BTN_W as i32, fy, HISTORY_BTN_W, fh)
}

fn refresh_button_rect(scr_w: u32) -> (i32, i32, u32, u32) {
    let (hx, hy, _, fh) = history_button_rect(scr_w);
    (hx - 8 - REFRESH_BTN_W as i32, hy, REFRESH_BTN_W, fh)
}

/// Geometry of the shelf's item grid — image tiles, or (no cover art loaded
/// at all, plan §3.1) a single-column text list styled like a pirate NES
/// multicart menu. One column of navigation math (`cols`/`vis_rows`) serves
/// both: list mode is just `cols == 1` with a short row instead of a tile.
struct GridLayout {
    x0: i32,
    y0: i32,
    /// Cell stride, including the gap.
    cell_w: i32,
    cell_h: i32,
    /// The clickable item itself, within its cell (no gap).
    item_w: i32,
    item_h: i32,
    cols: usize,
    vis_rows: usize,
    list_mode: bool,
}

impl GridLayout {
    /// `top_y` is where the grid itself starts, below the header row and the
    /// favorites strip above it (plan revision — the panel used
    /// to eat into this width; now the grid gets the whole shelf screen).
    ///
    /// Steam Deck (e qualquer tela onde 260px de tile rendem só 2 colunas):
    /// a grade nunca mostra menos que 3 colunas × 2 linhas — o tile encolhe,
    /// mantendo o 4:3, antes de perder uma coluna ou uma linha.
    fn new(scr_w: u32, scr_h: u32, list_mode: bool, top_y: i32) -> Self {
        let grid_w = scr_w.saturating_sub(MARGIN as u32 * 2 + SCROLLBAR_RESERVE);
        let (cell_w, cell_h, item_w, item_h, cols, x0) = if list_mode {
            let cell_h = (LIST_ROW_H + LIST_GAP) as i32;
            (
                grid_w as i32,
                cell_h,
                grid_w as i32,
                LIST_ROW_H as i32,
                1,
                MARGIN,
            )
        } else {
            // Respiro entre as capas das pontas e os montantes do móvel.
            let grid_w = grid_w.saturating_sub(SHELF_INSET as u32 * 2);
            let gap = GAP as i32;
            let cols = ((grid_w as i32 + gap) / (TILE_W as i32 + gap)).max(3);
            // Encolhe o tile até a fileira caber `cols` colunas com gap.
            let item_w = (TILE_W as i32)
                .min((grid_w as i32 - gap * (cols - 1)) / cols)
                .max(48);
            let mut item_h = item_w * TILE_H as i32 / TILE_W as i32;
            // E até 2 linhas couberem na altura (se a tela for baixa demais
            // para os tiles já encolhidos pela largura).
            let avail_h = scr_h as i32 - top_y - MARGIN;
            let two_rows_h = (avail_h / 2 - gap).max(36);
            if item_h > two_rows_h {
                item_h = two_rows_h;
            }
            // Fileiras que cabem inteiras na altura — e o tile cresce para
            // ocupá-la toda, em vez de sobrar um vão vazio embaixo da última.
            let rows = ((avail_h + gap) / (item_h + gap)).max(1);
            item_h = ((avail_h + gap) / rows - gap).max(item_h);
            let mut item_w = item_h * TILE_W as i32 / TILE_H as i32;
            // Mais alto ficou mais largo: as colunas ainda cabem?
            if cols * (item_w + gap) - gap > grid_w as i32 {
                item_w = (grid_w as i32 - gap * (cols - 1)) / cols;
            }
            let cell_w = item_w + gap;
            let cell_h = item_h + gap;
            // Sobrou largura? A grade fica centrada na área dela.
            let used = cols * cell_w - gap;
            let x0 = MARGIN + SHELF_INSET + (grid_w as i32 - used) / 2;
            (cell_w, cell_h, item_w, item_h, cols as usize, x0)
        };
        // A última fileira não precisa do vão de baixo (a prateleira dela
        // mora nele, mas cabe na margem) — sem o `+ gap`, uma fileira
        // inteira que cabia ficava de fora.
        let tail_gap = if list_mode { 0 } else { GAP as i32 };
        let vis_rows = ((scr_h as i32 - top_y - MARGIN + tail_gap) / cell_h).max(1) as usize;
        Self {
            x0,
            y0: top_y,
            cell_w,
            cell_h,
            item_w,
            item_h,
            cols,
            vis_rows,
            list_mode,
        }
    }

    /// Top-left of item `i`'s cell, relative to the grid's own row window
    /// (`top_row` is the first visible row) — `None` if it's scrolled out of
    /// view.
    fn cell_pos(&self, i: usize, top_row: usize) -> Option<(i32, i32)> {
        let row = i / self.cols;
        if row < top_row || row >= top_row + self.vis_rows {
            return None;
        }
        let col = i % self.cols;
        Some((
            self.x0 + col as i32 * self.cell_w,
            self.y0 + (row - top_row) as i32 * self.cell_h,
        ))
    }

    /// Which visible item (if any) a screen-local point sits inside — `None`
    /// in the gap between cells, past the last column, below the last
    /// visible row, or past the end of `view`.
    fn tile_at(&self, x: i32, y: i32, top_row: usize, view_len: usize) -> Option<usize> {
        let (dx, dy) = (x - self.x0, y - self.y0);
        if dx < 0 || dy < 0 || dx % self.cell_w >= self.item_w || dy % self.cell_h >= self.item_h {
            return None;
        }
        let col = (dx / self.cell_w) as usize;
        let row_in_view = (dy / self.cell_h) as usize;
        if col >= self.cols || row_in_view >= self.vis_rows {
            return None;
        }
        let i = (top_row + row_in_view) * self.cols + col;
        (i < view_len).then_some(i)
    }

    fn total_rows(&self, view_len: usize) -> usize {
        view_len.div_ceil(self.cols)
    }
}

/// Geometry of the "favoritos" strip —
/// always a single row, horizontally scrollable (unlike `GridLayout`): the
/// caller windows it with its own scroll offset.
struct StripLayout {
    x0: i32,
    y0: i32,
    cell_w: i32,
}

impl StripLayout {
    fn new(y0: i32) -> Self {
        Self {
            x0: MARGIN,
            y0,
            cell_w: (STRIP_TILE_W + STRIP_GAP) as i32,
        }
    }

    fn item_pos(&self, i: usize) -> (i32, i32) {
        (self.x0 + i as i32 * self.cell_w, self.y0)
    }

    fn hit(&self, x: i32, y: i32, count: usize) -> Option<usize> {
        let (dx, dy) = (x - self.x0, y - self.y0);
        if dx < 0 || dy < 0 || dy >= STRIP_TILE_H as i32 || dx % self.cell_w >= STRIP_TILE_W as i32
        {
            return None;
        }
        let i = (dx / self.cell_w) as usize;
        (i < count).then_some(i)
    }
}

/// `roms/` has nothing in it — a friendlier landing than a hard error, since
/// an empty ROMs folder is the expected first-launch state for a portable,
/// autoexecutável app, not a misconfiguration.
fn empty_roms_screen(plat: &mut Platform, cab: &mut Cabinet) -> Result<Pick> {
    let frame = Duration::from_millis(16);
    cab.set_shelf_panel(empty_shelf_panel());
    cab.set_close_button(true);
    let refresh_rect = refresh_button_rect(cab.shelf_screen_size().0);
    loop {
        let m = plat.poll_menu(MenuMode::Nav);
        if m.quit {
            return Ok(Pick::Quit);
        }
        if m.nav.contains(&MenuNav::Back) {
            return Ok(Pick::Back);
        }
        if let Some((x, y)) = m.click {
            let (ox, oy) = cab.window_to_output(x, y);
            if cab.hit_close_button(ox, oy) {
                return Ok(Pick::Quit);
            }
            if cab.hit_minimize_button(ox, oy) {
                cab.minimize();
                continue;
            }
            if cab
                .hit_screen_point(ox, oy)
                .is_some_and(|(lx, ly)| in_rect(lx, ly, refresh_rect))
            {
                return Ok(Pick::Refresh);
            }
            match cab.hit_shelf_button(ox, oy) {
                Some(ShelfButton::Back) => return Ok(Pick::Back),
                Some(ShelfButton::Settings) => return Ok(Pick::Settings),
                // No game focused on this screen, so nothing to jump to (and
                // no back cover to enlarge) — "Atualizar" lives in the
                // header now, handled above.
                Some(ShelfButton::PanelJump(_))
                | Some(ShelfButton::Backcover)
                | Some(ShelfButton::Cartridge)
                | Some(ShelfButton::Manual)
                | Some(ShelfButton::ToggleFavorite)
                | Some(ShelfButton::Refresh)
                | Some(ShelfButton::ShelfAchievements)
                | None => {}
            }
        }
        let (scr_w, _) = cab.shelf_screen_size();
        let render = |d: &mut Screen| {
            draw_refresh_button(d, refresh_rect);
            d.text(
                MARGIN,
                MARGIN,
                2,
                TEXT,
                tr!("your shelf is empty", "sua estante está vazia"),
            );
            d.text_wrapped(
                MARGIN,
                MARGIN + 40,
                scr_w.saturating_sub(MARGIN as u32 * 2),
                1,
                DIM,
                tr!(
                    "drop your .sfc/.smc/.zip files into the roms/ folder and \
                     click Refresh (or go back and open the shelf again).",
                    "solte seus .sfc/.smc/.zip na pasta roms/ e clique em \
                     Atualizar (ou volte e abra a estante de novo)."
                ),
            );
        };
        cab.frame_shelf(BG, render);
        std::thread::sleep(frame);
    }
}

/// Onde o cursor da estante está: a grade (`sel`, rolada até `top_row`)
/// ou a faixa de favoritos acima dela, com seu cursor e sua rolagem
/// horizontal (o índice do primeiro tile visível). Mais a rolagem e a aba
/// do painel lateral.
struct ShelfNav {
    sel: usize,
    top_row: usize,
    in_fav: bool,
    fav_idx: usize,
    fav_scroll: usize,
    /// Zerada quando o jogo focado muda (via `last_focus`);
    /// `draw_shelf_panel` limita ao que o conteúdo precisa.
    panel_scroll: usize,
    /// A aba ativa do painel — persiste entre jogos de propósito.
    panel_section: PanelSection,
    last_focus: Option<(u8, usize)>,
}

impl Default for ShelfNav {
    fn default() -> Self {
        Self {
            sel: 0,
            top_row: 0,
            in_fav: false,
            fav_idx: 0,
            fav_scroll: 0,
            panel_scroll: 0,
            // "na estante sempre abrir informações como padrão"
            panel_section: PanelSection::Informacoes,
            last_focus: None,
        }
    }
}

impl ShelfNav {
    /// O jogo sob o cursor: na faixa de favoritos ou na grade.
    fn focused<'a>(
        &self,
        view: &'a [CatalogEntry],
        favorites: &'a [CatalogEntry],
    ) -> Option<&'a CatalogEntry> {
        if self.in_fav {
            favorites.get(self.fav_idx)
        } else {
            view.get(self.sel)
        }
    }

    /// Zona (0 = favoritos, 2 = grade) e índice do cursor — quando muda, o
    /// painel volta ao topo.
    fn focus_key(&self) -> (u8, usize) {
        if self.in_fav {
            (0, self.fav_idx)
        } else {
            (2, self.sel)
        }
    }

    /// Move o cursor com o d-pad/teclado: setas dentro da zona, Cima/Baixo
    /// passando entre a grade e os favoritos, PageUp/PageDown por tela da
    /// grade, Home/End para o começo e o fim. `(cols, vis_rows)` da grade.
    fn step(
        &mut self,
        nav: MenuNav,
        lens: ShelfLens,
        (cols, vis_rows): (usize, usize),
        show_fav: bool,
    ) {
        match nav {
            MenuNav::Left => {
                if self.in_fav {
                    self.fav_idx = self.fav_idx.saturating_sub(1);
                } else {
                    self.sel = self.sel.saturating_sub(1);
                }
            }
            MenuNav::Right => {
                if self.in_fav {
                    if self.fav_idx + 1 < lens.favorites {
                        self.fav_idx += 1;
                    }
                } else if self.sel + 1 < lens.grid {
                    self.sel += 1;
                }
            }
            MenuNav::Up => {
                if self.in_fav {
                    // Já na faixa de cima.
                } else if show_fav && self.sel < cols {
                    self.in_fav = true;
                    self.fav_idx = self.sel.min(lens.favorites.saturating_sub(1));
                } else {
                    self.sel = self.sel.saturating_sub(cols);
                }
            }
            MenuNav::Down => {
                if self.in_fav {
                    self.in_fav = false;
                    self.sel = self.fav_idx.min(lens.grid.saturating_sub(1));
                } else if self.sel + cols < lens.grid {
                    self.sel += cols;
                }
            }
            MenuNav::PageUp => {
                if !self.in_fav {
                    self.sel = self.sel.saturating_sub(cols * vis_rows);
                }
            }
            MenuNav::PageDown => {
                if !self.in_fav {
                    self.sel = (self.sel + cols * vis_rows).min(lens.grid.saturating_sub(1))
                }
            }
            MenuNav::Home => {
                if show_fav {
                    self.in_fav = true;
                    self.fav_idx = 0;
                } else {
                    self.sel = 0;
                }
            }
            MenuNav::End => {
                self.in_fav = false;
                self.sel = lens.grid.saturating_sub(1);
            }
            _ => {}
        }
    }
}

/// Quantos jogos há em cada zona da estante.
#[derive(Clone, Copy)]
struct ShelfLens {
    grid: usize,
    favorites: usize,
}

/// O filtro por título: `query` é o aplicado; `draft`/`editing` só vivem
/// enquanto se digita. As listas são refeitas só quando `query` muda
/// (`applied` guarda a última aplicada) — reordenar ~700 entradas a 60 fps
/// era o maior custo por frame da estante.
struct ShelfFilter {
    query: String,
    draft: String,
    editing: bool,
    applied: Option<String>,
}

impl ShelfFilter {
    fn new(query: String) -> Self {
        Self {
            query,
            draft: String::new(),
            editing: false,
            applied: None,
        }
    }
}

/// O estado do RetroAchievements na estante: identificação por jogo
/// (um hash+fetch em voo por vez, cada jogo tentado uma vez por visita —
/// o cache em disco deixa as próximas visitas instantâneas), a linha
/// "X de Y (Z%)" do painel, o progresso do perfil e os badges.
struct ShelfRa {
    /// sha1 → jogo identificado (`None` = o servidor não conhece / sem set).
    games: HashMap<String, Option<crate::ra::RaGame>>,
    tried: HashSet<String>,
    worker: Option<Receiver<Result<Option<crate::ra::RaGame>, String>>>,
    /// O sha1 do pedido em voo.
    pending: String,
    /// O "X de Y (Z%)" por jogo, calculado uma vez por foco (custa o hash
    /// da ROM) e refeito quando o progresso do perfil chega.
    tally: HashMap<String, Option<(usize, usize, Option<crate::ra::Award>)>>,
    /// sha1 → hash RA, guardado pela identificação para o painel não
    /// refazer o hash da ROM a cada frame.
    hashes: HashMap<String, String>,
    /// Progresso do perfil (o substituto do GetUserUnlocks): uma chamada
    /// por visita traz as conquistas ganhas fora deste app.
    completion: Option<HashMap<u64, crate::ra::CompletionEntry>>,
    completion_tried: bool,
    completion_worker: Option<Receiver<Result<HashMap<u64, crate::ra::CompletionEntry>, String>>>,
    /// Badges já registrados como textura (nome → id da imagem).
    badge_ids: HashMap<String, u64>,
    badge_tried: HashSet<String>,
    /// Próxima varredura do disco atrás de badges que faltam — 4×/s, não a
    /// cada frame (era um stat por badge ausente a 60 Hz).
    badge_scan_next: Instant,
    /// Earned do servidor para os [x] da lista (uma tentativa por jogo por
    /// visita): o worker une no arquivo local e devolve o set resultante.
    earned_worker: Option<Receiver<(String, Option<crate::ra::EarnedMap>)>>,
    earned_tried: HashSet<String>,
}

impl ShelfRa {
    fn new() -> Self {
        Self {
            games: HashMap::new(),
            tried: HashSet::new(),
            worker: None,
            pending: String::new(),
            tally: HashMap::new(),
            hashes: HashMap::new(),
            completion: None,
            completion_tried: false,
            completion_worker: None,
            badge_ids: HashMap::new(),
            badge_tried: HashSet::new(),
            badge_scan_next: Instant::now(),
            earned_worker: None,
            earned_tried: HashSet::new(),
        }
    }

    /// Um frame do RA na estante: recolhe as respostas dos workers
    /// (identificação, progresso do perfil, earned), registra badges que já
    /// estão no disco e dispara a identificação do jogo focado.
    fn poll(
        &mut self,
        cab: &mut Cabinet,
        ach: &mut AchView,
        account: Option<&(String, String, String)>,
        focused: Option<&CatalogEntry>,
    ) {
        // RA identification (plan fase 2): drain the in-flight reply, and
        // kick the next one for the focused game when the account is on.
        if let Some(rx) = &self.worker {
            match rx.try_recv() {
                Ok(Ok(g)) => {
                    self.games.insert(self.pending.clone(), g);
                    self.worker = None;
                }
                Ok(Err(e)) => {
                    log::info!("ra identify {}: {e}", self.pending);
                    self.games.insert(self.pending.clone(), None);
                    self.worker = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.worker = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(rx) = &self.completion_worker {
            match rx.try_recv() {
                Ok(Ok(map)) => {
                    log::info!("ra: progresso do perfil carregado ({} jogos)", map.len());
                    self.completion = Some(map);
                    // Tallys já montados nasceram sem o servidor — refazer.
                    self.tally.clear();
                    self.completion_worker = None;
                }
                Ok(Err(e)) => {
                    log::info!("ra completion: {e}");
                    self.completion_worker = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.completion_worker = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        if let (false, Some((user, web_key, _connect))) = (self.completion_tried, account.cloned())
        {
            self.completion_tried = true;
            let (tx, rx) = mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::ra::fetch_completion(&user, &web_key));
            });
            self.completion_worker = Some(rx);
        }
        // Earned do servidor chegando: o set já foi unido no disco — a lista
        // aberta deste jogo ganha os [x] (e o "X/Y" do cabeçalho) na hora.
        if let Some(rx) = &self.earned_worker {
            match rx.try_recv() {
                Ok((hash, Some(ids))) => {
                    if ach.hash.as_deref() == Some(hash.as_str()) {
                        if let Some(list) = ach.data.as_mut() {
                            list.earned = ids;
                        }
                    }
                    self.earned_worker = None;
                }
                Ok((hash, None)) => {
                    log::info!("ra earned {hash}: sem ids do servidor");
                    self.earned_worker = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => self.earned_worker = None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
        // Badges do jogo aberto que já estão no disco viram texturas — umas
        // poucas por frame para o primeiro carregamento não engasgar. O
        // prefetch continua baixando em background: na medida que os PNGs
        // aparecem, os badges entram na lista (de qualquer jogo aberto).
        if ach.open && Instant::now() >= self.badge_scan_next {
            self.badge_scan_next = Instant::now() + Duration::from_millis(250);
            if let Some(list) = ach.data.as_ref() {
                let mut novos = 0;
                for a in &list.achievements {
                    if novos >= 4 {
                        // Ainda há badges no disco para registrar: a
                        // próxima leva sai no frame seguinte.
                        self.badge_scan_next = Instant::now();
                        break;
                    }
                    if self.badge_ids.contains_key(&a.badge) {
                        continue;
                    }
                    if let Some(path) = crate::ra::badge_path(&a.badge) {
                        if let Ok(img) = image::open(&path).map(|i| i.to_rgba8()) {
                            let id = badge_image_id(&a.badge);
                            cab.set_image(id, img.width(), img.height(), img.as_raw());
                            self.badge_ids.insert(a.badge.clone(), id);
                            novos += 1;
                        }
                    }
                }
            }
        }
        if self.worker.is_none() {
            if let (Some((user, web_key, connect)), Some(e)) = (account, focused) {
                let sha1 = e.rom.sha1.clone();
                if !self.tried.contains(&sha1) {
                    self.tried.insert(sha1.clone());
                    match crate::ra::hash_rom(std::path::Path::new(&e.rom.path)) {
                        Err(err) => {
                            log::info!("ra hash {}: {err}", e.rom.path);
                            self.games.insert(sha1, None);
                        }
                        Ok(hash) => {
                            self.hashes.insert(sha1.clone(), hash.clone());
                            if let Some(g) = crate::ra::cached_game(&hash) {
                                self.games.insert(sha1, Some(g));
                            } else if crate::ra::is_cached_unknown(&hash) {
                                self.games.insert(sha1, None);
                            } else {
                                let (tx, rx) = mpsc::channel();
                                let (user, web_key, connect) =
                                    (user.clone(), web_key.clone(), connect.clone());
                                std::thread::spawn(move || {
                                    let _ = tx.send(crate::ra::fetch_game(
                                        &user, &web_key, &connect, &hash,
                                    ));
                                });
                                self.worker = Some(rx);
                                self.pending = sha1;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A lista de conquistas dentro da TV: aberta ou não, a rolagem, os dados
/// (carregados uma vez por abertura), o hash RA do jogo aberto (para o
/// earned do servidor casar com a lista) e o rect do botão "voltar".
#[derive(Default)]
struct AchView {
    open: bool,
    top: usize,
    data: Option<crate::ra::ShelfAchievements>,
    hash: Option<String>,
    voltar: (i32, i32, u32, u32),
}

impl AchView {
    /// Abre a lista de conquistas de `e` e, com a conta configurada, pede o
    /// earned do servidor (uma vez por jogo por visita) — ele só vale para
    /// esta lista, casada pelo hash RA.
    fn open_for(
        &mut self,
        e: &CatalogEntry,
        ra: &mut ShelfRa,
        account: Option<&(String, String, String)>,
    ) {
        self.data = crate::ra::shelf_achievements(std::path::Path::new(&e.rom.path));
        if self.data.is_some() {
            self.open = true;
            self.top = 0;
        }
        // A lista aberta é deste jogo: o earned do
        // servidor (quando chegar) só vale para ela.
        if let Some((user, web_key, _connect)) = account {
            let hash = match ra.hashes.get(&e.rom.sha1) {
                Some(h) => Some(h.clone()),
                None => crate::ra::hash_rom(std::path::Path::new(&e.rom.path)).ok(),
            };
            if let Some(hash) = hash {
                self.hash = Some(hash.clone());
                if ra.earned_tried.insert(hash.clone()) {
                    ra.earned_worker =
                        Some(crate::ra::fetch_game_earned_worker(user, web_key, &hash));
                }
            }
        }
    }

    /// A lista dona do tubo: d-pad/roda rolam, Back ou "voltar" fecha.
    fn handle_input(&mut self, cab: &mut Cabinet, m: &MenuInput) -> AchInput {
        let (_, sh) = cab.shelf_screen_size();
        let row_h = 22;
        let visible = ((sh as i32 - MARGIN * 2 - HEADER_H - 60) / row_h).max(1) as usize;
        let len = self.data.as_ref().map_or(0, |d| d.achievements.len());
        for nav in &m.nav {
            match nav {
                MenuNav::Up | MenuNav::PageUp => self.top = self.top.saturating_sub(1),
                MenuNav::Down | MenuNav::PageDown => {
                    if self.top + visible < len {
                        self.top += 1;
                    }
                }
                MenuNav::Home => self.top = 0,
                MenuNav::End => self.top = len.saturating_sub(visible),
                MenuNav::Back => self.open = false,
                _ => {}
            }
        }
        if let Some((x, y)) = m.click {
            let (ox, oy) = cab.window_to_output(x, y);
            if cab.hit_close_button(ox, oy) {
                return AchInput::Quit;
            }
            if cab.hit_minimize_button(ox, oy) {
                cab.minimize();
                return AchInput::Minimized;
            }
            if let Some((lx, ly)) = cab.hit_screen_point(ox, oy) {
                if in_rect(lx, ly, self.voltar) {
                    self.open = false;
                }
            }
        }
        AchInput::Stay
    }
}

/// O que o input da lista de conquistas pede ao laço da estante.
enum AchInput {
    Stay,
    Minimized,
    Quit,
}

/// O manual em PDF aberto na TV: o worker decodifica as páginas sob
/// demanda; o estado guarda o pedido em voo para não repetir.
struct ManualState {
    sha1: String,
    pages: usize,
    page: usize,
    requested: HashSet<usize>,
}

/// Os manuais na estante: o leitor aberto, os workers vivos (preview da
/// 1ª página por jogo + o leitor), a contagem de páginas por jogo, os
/// botões do frame anterior e o PDF de cada jogo (resolvido uma vez).
struct ShelfManual {
    reader: Option<ManualState>,
    channels: Vec<Receiver<crate::manual::ManualEv>>,
    tx: Option<mpsc::Sender<usize>>,
    pages: HashMap<String, usize>,
    preview_tried: HashSet<String>,
    rects: (ViewBtn, ViewBtn, ViewBtn),
    dir: PathBuf,
    paths: HashMap<String, Option<PathBuf>>,
}

impl ShelfManual {
    /// O leitor dono do tubo: ‹/› viram página (os rects vieram do frame
    /// anterior), voltar/Back fecha.
    fn handle_input(&mut self, cab: &mut Cabinet, m: &MenuInput) {
        for nav in m.nav.iter() {
            if let Some(m) = self.reader.as_mut() {
                match nav {
                    MenuNav::Left => m.page = m.page.saturating_sub(1).max(1),
                    MenuNav::Right if m.page < m.pages => m.page += 1,
                    MenuNav::Back => {
                        self.reader = None;
                        self.tx = None;
                    }
                    _ => {}
                }
            }
        }
        if self.reader.is_some() {
            if let Some((x, y)) = m.click {
                let (ox, oy) = cab.window_to_output(x, y);
                if let Some((lx, ly)) = cab.hit_screen_point(ox, oy) {
                    let (prev, next, voltar) = self.rects;
                    if in_rect(lx, ly, prev) {
                        if let Some(mm) = self.reader.as_mut() {
                            mm.page = mm.page.saturating_sub(1).max(1);
                        }
                    } else if in_rect(lx, ly, next) {
                        if let Some(mm) = self.reader.as_mut() {
                            if mm.page < mm.pages {
                                mm.page += 1;
                            }
                        }
                    } else if in_rect(lx, ly, voltar) {
                        self.reader = None;
                        self.tx = None;
                    }
                }
            }
        }
    }

    /// Um frame dos manuais: eventos dos workers (contagem ao abrir e
    /// páginas renderizadas viram texturas) e o pedido da página corrente e
    /// da seguinte assim que faltam.
    fn poll(&mut self, cab: &mut Cabinet) {
        // Manual (plan revision): eventos do worker — contagem ao abrir e
        // páginas renderizadas viram texturas; a página corrente (e a
        // seguinte, de brincadeira) são pedidas assim que faltam.
        let mut live_channels = Vec::with_capacity(self.channels.len());
        for rx in self.channels.drain(..) {
            match rx.try_recv() {
                Ok(crate::manual::ManualEv::Ready { sha1, pages }) => {
                    self.pages.insert(sha1.clone(), pages);
                    if let Some(m) = self.reader.as_mut() {
                        if m.sha1 == sha1 {
                            m.pages = pages;
                        }
                    }
                    live_channels.push(rx);
                }
                Ok(crate::manual::ManualEv::Page {
                    sha1,
                    page,
                    w,
                    h,
                    rgba,
                }) => {
                    // A textura é registrada SEMPRE (preview incluído) — o
                    // registro anterior dependia do leitor estar aberto e
                    // descartava a página do preview (plan revision: "a
                    // primeira pgina nao aparece no painel").
                    if w > 0 {
                        cab.set_image(manual_page_id(&sha1, page), w, h, &rgba);
                    }
                    if let Some(m) = self.reader.as_mut() {
                        if m.sha1 == sha1 {
                            m.requested.insert(page);
                        }
                    }
                    live_channels.push(rx);
                }
                // O worker morreu (fechou sem página nenhuma) — sem canal.
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {}
                Err(std::sync::mpsc::TryRecvError::Empty) => live_channels.push(rx),
            }
        }
        self.channels = live_channels;
        if let (Some(m), Some(tx)) = (self.reader.as_mut(), self.tx.as_ref()) {
            if m.pages > 0 && !m.requested.contains(&m.page) {
                m.requested.insert(m.page);
                let _ = tx.send(m.page);
            }
            let next_page = m.page + 1;
            if next_page <= m.pages && !m.requested.contains(&next_page) {
                m.requested.insert(next_page);
                let _ = tx.send(next_page);
            }
        }
    }
}

impl Default for ShelfManual {
    fn default() -> Self {
        Self {
            reader: None,
            channels: Vec::new(),
            tx: None,
            pages: HashMap::new(),
            preview_tried: HashSet::new(),
            rects: ((0, 0, 1, 1), (0, 0, 1, 1), (0, 0, 1, 1)),
            dir: crate::dirs::assets_dir().join("manual"),
            paths: HashMap::new(),
        }
    }
}

/// O painel lateral do jogo focado: logo/cartucho/contracapa já
/// carregados, lançamento e infos da tabela embutida, o "X de Y (Z%)" e a
/// medalha do RA, o tempo de jogo, IGDB, HowLongToBeat e o preview do
/// manual (disparado uma vez por jogo).
fn focused_panel_info(
    cab: &mut Cabinet,
    e: &CatalogEntry,
    ra: &mut ShelfRa,
    man: &mut ShelfManual,
    section: PanelSection,
    scroll: usize,
) -> ShelfPanelInfo {
    let wid = wheel_id(&e.rom.sha1);
    let cid = cartridge_id(&e.rom.sha1);
    let bid = backcover_id(&e.rom.sha1);
    let save_title = crate::runner::rom_title(Path::new(&e.rom.path));
    let playtime = crate::runner::total_playtime_secs(&crate::dirs::saves_dir(), &save_title);
    // The DAT's year, if it had one, moves into its own "release"
    // slot (plan revision) instead of sitting in the generic
    // info list alongside publisher/category/description.
    // A tabela única embutida (crc → tudo): lançamento,
    // editora, região e nota da IGDB — sem DAT, sem rede.
    let infos = sixteen_domain::infos::lookup(&e.rom.crc32);
    let mut info: Vec<(String, String)> = Vec::new();
    if let Some(i) = &infos {
        // o lançamento vai pelo campo `release` do painel
        if let Some(editora) = &i.publisher {
            info.push((tr!("publisher", "editora").to_string(), editora.clone()));
        }
        if let Some(regiao) = &i.region {
            if regiao != "outra" {
                info.push((tr!("region", "região").to_string(), regiao.clone()));
            }
        }
        if let Some(critica) = &i.nota_critica {
            if !critica.is_empty() {
                info.push((tr!("rating", "nota (crítica)").to_string(), critica.clone()));
            }
        }
    }
    let release = infos.as_ref().and_then(|i| i.year.clone());
    let mut award_img: Option<u64> = None;
    if let Some(Some(_)) = ra.games.get(&e.rom.sha1) {
        // "30 de 58 (52%)" — o tally custa um hash de ROM inteiro,
        // então fica cacheado por visita (e refresco quando o
        // completion progress do servidor chega).
        let tally = *ra.tally.entry(e.rom.sha1.clone()).or_insert_with(|| {
            ra.hashes
                .get(&e.rom.sha1)
                .and_then(|h| crate::ra::tally_from_cache(h, ra.completion.as_ref()))
        });
        // A medalha de prêmio do RA desenhada ao lado do número
        // da linha de conquistas (pixel-art gerada por
        // `medal_rgba`, uma textura por variante).
        if let Some((earned, total, award)) = tally {
            let pct = (earned * 100 + total / 2).checked_div(total).unwrap_or(0);
            info.push((
                tr!("achievements", "conquistas").to_string(),
                tr!(
                    format!("{earned} of {total} ({pct}%)"),
                    format!("{earned} de {total} ({pct}%)")
                ),
            ));
            // "zerado (softcore)" / "mastered" — o prêmio em
            // palavras, junto das infos (plan revision).
            if let Some(a) = award {
                info.push(("status".to_string(), a.label().to_string()));
            }
            if let Some(medal) = award.map(|a| a.medal()) {
                let id = crate::ra::medal_image_id(medal);
                if !cab.has_image(id) {
                    let (w, h, rgba) = crate::ra::medal_rgba(medal);
                    cab.set_image(id, w, h, &rgba);
                }
                award_img = Some(id);
            }
        }
        // Os badges do set já baixando em background — a notificação
        // e a lista da estante os encontram em cache na hora.
        if ra.badge_tried.insert(e.rom.sha1.clone()) {
            crate::ra::prefetch_badges(std::path::Path::new(&e.rom.path));
        }
    }
    info.extend(game_info_lines(e, playtime));
    // O bloco HowLongToBeat no fim da aba — as 4 barras do card
    // do site, tempos da tabela embutida por CRC32 (nada de
    // rede); jogo fora da tabela não ganha bloco nenhum.
    // A seção de nota da IGDB + o HowLongToBeat, ambos da
    // tabela única — o HLTB fecha o painel.
    let igdb = infos
        .as_ref()
        .and_then(|i| i.nota.clone())
        .filter(|nota| !nota.is_empty())
        .map(|nota| sixteen_platform::IgdbSection {
            icon: igdb_icon_registered(cab),
            nota: format!("{nota} / 100"),
        });
    let hltb = infos
        .as_ref()
        .map(|i| hltb_bars(i, hltb_icon_registered(cab)));
    let manual_path = man
        .paths
        .entry(e.rom.sha1.clone())
        .or_insert_with(|| find_local_manual(&man.dir, &e.rom.path))
        .clone();
    // Preview: contagem + primeira página decodificadas uma vez
    // por jogo — a aba manual as mostra como as artes mostram
    // capa/cartucho (plan revision).
    if let Some(preview_path) = &manual_path {
        if man.preview_tried.insert(e.rom.sha1.clone()) {
            let (ev_tx, ev_rx) = mpsc::channel();
            let preview_path = preview_path.clone();
            let preview_sha1 = e.rom.sha1.clone();
            std::thread::spawn(move || {
                let pages = crate::manual::page_count(&preview_path).unwrap_or(0);
                let _ = ev_tx.send(crate::manual::ManualEv::Ready {
                    sha1: preview_sha1.clone(),
                    pages,
                });
                if let Ok(Some(img)) = crate::manual::render_page(&preview_path, 1, 512) {
                    let _ = ev_tx.send(crate::manual::ManualEv::Page {
                        sha1: preview_sha1.clone(),
                        page: 1,
                        w: img.w,
                        h: img.h,
                        rgba: img.rgba,
                    });
                }
            });
            man.channels.push(ev_rx);
        }
    }
    let ra_state = ra.games.get(&e.rom.sha1);
    ShelfPanelInfo {
        title: e.title().into_owned(),
        logo_img: cab.has_image(wid).then_some(wid),
        cartridge_img: cab.has_image(cid).then_some(cid),
        backcover_img: cab.has_image(bid).then_some(bid),
        release,
        info,
        section,
        scroll,
        favorite: Some(e.rom.favorite),
        // Identificado = botão desenhado; hash marcado "versão
        // não suportada" = botão desativado com o motivo
        // (plan revision).
        achievements: ra_state.is_some_and(|g| g.is_some()),
        achievements_reason: ra_state
            .and_then(|g| g.as_ref())
            .filter(|g| g.unsupported)
            .map(|_| tr!("unsupported version", "versão sem suporte").to_string()),
        manual_first_page: manual_path
            .is_some()
            .then(|| manual_page_id(&e.rom.sha1, 1)),
        award_img,
        igdb,
        hltb,
    }
}

/// O painel quando não há jogo nenhum na estante.
fn empty_panel_info() -> ShelfPanelInfo {
    ShelfPanelInfo {
        title: tr!("no games found", "nenhum jogo encontrado").to_string(),
        logo_img: None,
        cartridge_img: None,
        backcover_img: None,
        release: None,
        info: Vec::new(),
        section: PanelSection::Informacoes,
        scroll: 0,
        favorite: None,
        achievements: false,
        achievements_reason: None,
        manual_first_page: None,
        award_img: None,
        igdb: None,
        hltb: None,
    }
}

/// Show the shelf in `cab` (the one persistent window) until the player picks a
/// game or cancels. `plat` and `cab` both outlive the call.
pub fn run(
    plat: &mut Platform,
    cab: &mut Cabinet,
    catalog: &Catalog,
    opts: &ShelfOpts,
) -> Result<Pick> {
    let all_scanned = catalog.list(opts.order)?;
    if all_scanned.is_empty() {
        return empty_roms_screen(plat, cab);
    }
    let mut all = all_scanned;

    let mut art = ShelfArt::new();
    // Back cover enlarged (plan revision: "ao clicar no back cover
    // possibilitar mostrar em tamanho maior"): Some = the "voltar" button's
    // screen-local rect — the zoom replaces the tube's content until
    // dismissed (the flat panel stays).
    // A arte ampliada (zoom): o id da imagem no cabinet + o rect do botão
    // "voltar" do frame atual. Antes era só do back cover (plan revision:
    // "colocar zoom igual ao backcover no cartucho" — agora os dois).
    let mut zoom_close: Option<u64> = None;
    let mut zoom_rect: (i32, i32, u32, u32) = (0, 0, 1, 1);
    let mut cur = ShelfNav::default();
    let mut filt = ShelfFilter::new(opts.preset_filter.clone().unwrap_or_default());
    let frame = Duration::from_millis(16);
    let mut next = Instant::now() + frame;
    let mut frame_no = 0u64;
    // The filtered/sorted listings are rebuilt only when the applied filter
    // changes (and once up front) — re-sorting ~700 entries with fresh String
    // allocations at 60fps was the shelf's biggest per-frame cost. While the
    // player is still TYPING, `filter_draft` moves but `filter_query` doesn't,
    // so typing doesn't rebuild either.
    let mut view: Vec<CatalogEntry> = Vec::new();
    let mut favorites: Vec<CatalogEntry> = Vec::new();
    let mut ra = ShelfRa::new();
    let mut ach = AchView::default();
    let mut man = ShelfManual::default();
    cab.set_close_button(true);

    // Frames left in the "entering over the static" ease-in (§3.3), if any.
    const FADE_IN_FRAMES: u32 = 18;
    let mut fade_frame: u32 = 0;

    loop {
        frame_no += 1;
        if opts.max_frames.is_some_and(|n| frame_no > n) {
            return Ok(Pick::Quit);
        }

        // The general listing is always alphabetical (plan revision) — "o
        // que joguei por último" mora no Histórico, então esta é só uma
        // navegação A-Z previsível.
        if filt.applied.as_deref() != Some(filt.query.as_str()) {
            let query_lc = filt.query.to_lowercase();
            view = all
                .iter()
                .filter(|e| e.title().to_lowercase().contains(&query_lc))
                .cloned()
                .collect();
            view.sort_by_key(|e| e.title().to_lowercase());
            // Favorites sit above the grid (plan revision) — only while
            // browsing the unfiltered shelf, alphabetical
            // (plan revision: "organizar favoritos em ordem alfabetica"),
            // so the row reads like a curated mini-shelf.
            favorites = if filt.query.is_empty() {
                let mut f: Vec<CatalogEntry> =
                    all.iter().filter(|e| e.rom.favorite).cloned().collect();
                f.sort_by_key(|e| e.title().to_lowercase());
                f.truncate(STRIP_CAP);
                f
            } else {
                Vec::new()
            };
            filt.applied = Some(filt.query.clone());
        }
        if cur.sel >= view.len() {
            cur.sel = view.len().saturating_sub(1);
        }
        let (scr_w, scr_h) = cab.shelf_screen_size();
        // How many strip tiles fit between the margins — the window the
        // strips' scroll offsets keep the cursor inside.
        let strip_vis = (((scr_w as i32 - MARGIN * 2) as u32)
            .saturating_div(STRIP_TILE_W + STRIP_GAP))
        .max(1) as usize;
        let show_fav = !favorites.is_empty();
        if cur.fav_idx >= favorites.len() {
            cur.fav_idx = favorites.len().saturating_sub(1);
        }
        if !show_fav {
            cur.in_fav = false;
        }
        cur.fav_scroll = cur.fav_scroll.min(cur.fav_idx);
        if cur.fav_idx >= cur.fav_scroll + strip_vis {
            cur.fav_scroll = cur.fav_idx + 1 - strip_vis;
        }

        // No cover art loaded anywhere in view yet: a text list (multicart
        // menu) reads as deliberate, where a grid of empty tiles reads as
        // broken (plan §3.1).
        let list_mode = !view.iter().any(|e| cab.has_image(cover_id(&e.rom.sha1)));
        let fav_block_h = if show_fav {
            FAV_LABEL_H + STRIP_TILE_H as i32 + GAP as i32
        } else {
            0
        };
        let all_games_label_y = MARGIN + HEADER_H + fav_block_h;
        let grid_top_y = all_games_label_y + ALL_GAMES_LABEL_H;
        let grid = GridLayout::new(scr_w, scr_h, list_mode, grid_top_y);
        let fav_layout = StripLayout::new(MARGIN + HEADER_H + FAV_LABEL_H);
        let filter_rect = filter_box_rect(scr_w);
        let history_rect = history_button_rect(scr_w);
        let refresh_rect = refresh_button_rect(scr_w);

        // Input.
        // O cursor em coordenadas de janela — o hover do botão de play lê
        // a cada frame; durante a edição do filtro não há hover.
        let mut pointer = None;
        if filt.editing {
            let te = plat.poll_text_entry();
            if te.quit {
                return Ok(Pick::Quit);
            }
            if te.backspace {
                filt.draft.pop();
            }
            for c in te.typed.chars() {
                if filt.draft.chars().count() < FILTER_LIMIT {
                    filt.draft.push(c);
                }
            }
            // ⌘V/⌘C (Ctrl no resto) — mesmo esquema de clipboard dos outros
            // campos de texto.
            if let Some(paste) = &te.paste {
                for c in paste.chars() {
                    if filt.draft.chars().count() < FILTER_LIMIT {
                        filt.draft.push(c);
                    }
                }
            }
            if te.copy {
                plat.set_clipboard_text(&filt.draft);
            }
            if te.commit {
                filt.query = filt.draft.trim().to_string();
                filt.editing = false;
                plat.stop_text_input(cab);
                cur.sel = 0;
                cur.top_row = 0;
                cur.in_fav = false;
            } else if te.cancel {
                filt.editing = false;
                plat.stop_text_input(cab);
            }
        } else {
            let m = plat.poll_menu(MenuMode::Nav);
            pointer = m.pointer;
            if m.quit {
                return Ok(Pick::Quit);
            }
            if zoom_close.is_some() {
                // The enlarged back cover is up (inside the tube): only the
                // "voltar" button (or Back) dismisses it — clicks elsewhere
                // do nothing. The button lives in the warped screen buffer,
                // so its rect is screen-local and goes through
                // `hit_screen_point`, not raw output coordinates.
                if m.nav.iter().any(|n| matches!(n, MenuNav::Back)) {
                    zoom_close = None;
                }
                if let Some((x, y)) = m.click {
                    let (ox, oy) = cab.window_to_output(x, y);
                    if cab
                        .hit_screen_point(ox, oy)
                        .is_some_and(|(lx, ly)| in_rect(lx, ly, zoom_rect))
                    {
                        zoom_close = None;
                    }
                }
            } else if man.reader.is_some() {
                man.handle_input(cab, &m);
            } else if ach.open {
                match ach.handle_input(cab, &m) {
                    AchInput::Quit => return Ok(Pick::Quit),
                    AchInput::Minimized => continue,
                    AchInput::Stay => {}
                }
            } else {
                for nav in m.nav {
                    match nav {
                        MenuNav::Back => return Ok(Pick::Back),
                        MenuNav::Confirm => {
                            let picked = cur.focused(&view, &favorites);
                            if let Some(e) = picked {
                                return Ok(pick_play(
                                    catalog,
                                    &art.logo_dir,
                                    &art.cartridge_dir,
                                    e,
                                ));
                            }
                        }
                        other => cur.step(
                            other,
                            ShelfLens {
                                grid: view.len(),
                                favorites: favorites.len(),
                            },
                            (grid.cols, grid.vis_rows),
                            show_fav,
                        ),
                    }
                }

                // Botão direito na caixa do filtro: abre já colando — o
                // mesmo caminho curto do token nas configurações.
                if let Some((x, y)) = m.right_click {
                    let (ox, oy) = cab.window_to_output(x, y);
                    if let Some((lx, ly)) = cab.hit_screen_point(ox, oy) {
                        if in_rect(lx, ly, filter_rect) {
                            filt.editing = true;
                            filt.draft = filt.query.clone();
                            if let Some(paste) = plat.paste_from_clipboard() {
                                for c in paste.chars() {
                                    if filt.draft.chars().count() < FILTER_LIMIT {
                                        filt.draft.push(c);
                                    }
                                }
                            }
                            plat.start_text_input(cab);
                        }
                    }
                }

                // Mouse: click a tile to select it, click the already-selected one
                // to launch — the same two-step a controller does (move, then A).
                // The flat panel's "Configurações"/"Voltar" buttons (plan
                // revision: now outside the tube) are the only way into either
                // without a gamepad now.
                if let Some((x, y)) = m.click {
                    let (ox, oy) = cab.window_to_output(x, y);
                    if cab.hit_close_button(ox, oy) {
                        return Ok(Pick::Quit);
                    }
                    if cab.hit_minimize_button(ox, oy) {
                        cab.minimize();
                    }
                    if let Some(hit) = cab.hit_shelf_button(ox, oy) {
                        match hit {
                            ShelfButton::Back => return Ok(Pick::Back),
                            ShelfButton::Settings => return Ok(Pick::Settings),
                            ShelfButton::ShelfAchievements => {
                                if let Some(e) = cur.focused(&view, &favorites) {
                                    ach.open_for(e, &mut ra, opts.ra.as_ref());
                                }
                            }
                            ShelfButton::Refresh => {}
                            // "Atualizar" moved to the header row (plan
                            // revision: "colocar botão de atualizar estante
                            // do lado do histórico").
                            // Salto de seção do painel (plan revision: no
                            // lugar das setas de rolar, capa traseira /
                            // cartucho / informações).
                            ShelfButton::PanelJump(section) => {
                                // Abas (plan revision): clicar troca a seção
                                // visível; clicar de novo em "informações"
                                // avança campo a campo (a única aba que
                                // transborda).
                                if cur.panel_section != section {
                                    cur.panel_section = section;
                                    cur.panel_scroll = 0;
                                } else if section == PanelSection::Informacoes {
                                    cur.panel_scroll += 1;
                                }
                            }
                            ShelfButton::ToggleFavorite => {
                                // Flip the focused game's marker in the
                                // sidecar, then update this visit's own
                                // copy of the catalog and rebuild the
                                // strips — no re-scan involved.
                                let picked = cur.focused(&view, &favorites);
                                if let Some(e) = picked {
                                    let sha1 = e.rom.sha1.clone();
                                    let fav = !e.rom.favorite;
                                    if catalog.set_favorite(&sha1, fav).is_ok() {
                                        if let Some(row) =
                                            all.iter_mut().find(|e| e.rom.sha1 == sha1)
                                        {
                                            row.rom.favorite = fav;
                                        }
                                        filt.applied = None;
                                    }
                                }
                            }
                            ShelfButton::Backcover => {
                                // Open the enlarged view: re-decode the file at
                                // full resolution for a crisp enlargement (the
                                // panel texture is thumbnailed to 512).
                                let picked = cur.focused(&view, &favorites);
                                if let Some(e) = picked {
                                    let bid = backcover_id(&e.rom.sha1);
                                    if cab.has_image(bid) {
                                        // Abre já com a miniatura; a versão
                                        // de 2048 px vem do worker e troca a
                                        // textura quando fica pronta.
                                        zoom_close = Some(bid);
                                        art.request_zoom(bid, ArtKind::Backcover, e);
                                    }
                                }
                            }
                            ShelfButton::Manual => {
                                // Abrir o leitor na TV (plan revision):
                                // worker decodifica páginas sob demanda.
                                let picked = cur.focused(&view, &favorites);
                                if let Some(e) = picked {
                                    if let Some(path) =
                                        man.paths.get(&e.rom.sha1).cloned().flatten()
                                    {
                                        let thread_path = path.clone();
                                        let open_sha1 = e.rom.sha1.clone();
                                        let (ev_tx, ev_rx) = mpsc::channel();
                                        let (req_tx, req_rx) = std::sync::mpsc::channel();
                                        std::thread::spawn(move || {
                                            let path = thread_path;
                                            let Ok(pages) = crate::manual::page_count(&path) else {
                                                return;
                                            };
                                            let _ = ev_tx.send(crate::manual::ManualEv::Ready {
                                                sha1: open_sha1.clone(),
                                                pages,
                                            });
                                            for page in req_rx {
                                                match crate::manual::render_page(&path, page, 2048)
                                                {
                                                    Ok(Some(img)) => {
                                                        let _ = ev_tx.send(
                                                            crate::manual::ManualEv::Page {
                                                                sha1: open_sha1.clone(),
                                                                page,
                                                                w: img.w,
                                                                h: img.h,
                                                                rgba: img.rgba,
                                                            },
                                                        );
                                                    }
                                                    _ => {
                                                        let _ = ev_tx.send(
                                                            crate::manual::ManualEv::Page {
                                                                sha1: open_sha1.clone(),
                                                                page,
                                                                w: 0,
                                                                h: 0,
                                                                rgba: Vec::new(),
                                                            },
                                                        );
                                                    }
                                                }
                                            }
                                        });
                                        man.reader = Some(ManualState {
                                            sha1: e.rom.sha1.clone(),
                                            pages: man.pages.get(&e.rom.sha1).copied().unwrap_or(0),
                                            page: 1,
                                            requested: HashSet::new(),
                                        });
                                        man.channels.push(ev_rx);
                                        man.tx = Some(req_tx);
                                    }
                                }
                            }
                            ShelfButton::Cartridge => {
                                // Zoom do cartucho, igual ao do back cover
                                // (plan revision): re-decodifica a arte em
                                // resolução cheia e abre ampliado no tubo.
                                let picked = cur.focused(&view, &favorites);
                                if let Some(e) = picked {
                                    let cid = cartridge_id(&e.rom.sha1);
                                    if cab.has_image(cid) {
                                        // Abre já com a miniatura; a versão
                                        // de 2048 px vem do worker e troca a
                                        // textura quando fica pronta.
                                        zoom_close = Some(cid);
                                        art.request_zoom(cid, ArtKind::Cartridge, e);
                                    }
                                }
                            }
                        }
                    } else if let Some((lx, ly)) = cab.hit_screen_point(ox, oy) {
                        if in_rect(lx, ly, filter_rect) {
                            filt.editing = true;
                            filt.draft = filt.query.clone();
                            plat.start_text_input(cab);
                        } else if in_rect(lx, ly, history_rect) {
                            return Ok(Pick::History);
                        } else if in_rect(lx, ly, refresh_rect) {
                            return Ok(Pick::Refresh);
                        } else if in_rect(lx, ly, strip_arrow_rects(scr_w, fav_layout.y0).0)
                            && cur.fav_scroll > 0
                        {
                            cur.fav_scroll -= 1;
                            if cur.fav_idx > cur.fav_scroll + strip_vis - 1 {
                                cur.fav_idx = cur.fav_scroll + strip_vis - 1;
                            }
                        } else if in_rect(lx, ly, strip_arrow_rects(scr_w, fav_layout.y0).1)
                            && cur.fav_scroll + strip_vis < favorites.len()
                        {
                            cur.fav_scroll += 1;
                            if cur.fav_idx < cur.fav_scroll {
                                cur.fav_idx = cur.fav_scroll;
                            }
                        } else if let Some(i) = show_fav
                            .then(|| {
                                fav_layout
                                    .hit(lx, ly, strip_vis)
                                    .map(|i| i + cur.fav_scroll)
                                    .filter(|i| *i < favorites.len())
                            })
                            .flatten()
                        {
                            if cur.in_fav && i == cur.fav_idx {
                                if let Some(e) = favorites.get(i) {
                                    return Ok(pick_play(
                                        catalog,
                                        &art.logo_dir,
                                        &art.cartridge_dir,
                                        e,
                                    ));
                                }
                            } else {
                                cur.in_fav = true;
                                cur.fav_idx = i;
                            }
                        } else if let Some(i) = grid.tile_at(lx, ly, cur.top_row, view.len()) {
                            if !cur.in_fav && i == cur.sel {
                                if let Some(e) = view.get(i) {
                                    return Ok(pick_play(
                                        catalog,
                                        &art.logo_dir,
                                        &art.cartridge_dir,
                                        e,
                                    ));
                                }
                            } else {
                                cur.in_fav = false;
                                cur.sel = i;
                            }
                        }
                    }
                }
            }
        }

        // Keep selection visible.
        let sel_row = cur.sel.checked_div(grid.cols).unwrap_or(0);
        if sel_row < cur.top_row {
            cur.top_row = sel_row;
        } else if sel_row >= cur.top_row + grid.vis_rows {
            cur.top_row = sel_row + 1 - grid.vis_rows;
        }

        // Artes do que está à vista: o que o worker já decodificou entra
        // no cabinet; tiles sem imagem fazem o pedido — a grade com teto de
        // 8 pedidos por frame (a fila anda na ordem da tela), as faixas de
        // favoritos logo atrás.
        art.drain(cab);
        let grid_tiles = view
            .iter()
            .enumerate()
            .filter(|(i, _)| grid.cell_pos(*i, cur.top_row).is_some())
            .map(|(_, e)| e);
        art.request_covers(cab, grid_tiles, 8);
        let favs_shown = favorites.iter().skip(cur.fav_scroll).take(strip_vis);
        art.request_covers(cab, favs_shown, usize::MAX);

        let focused = cur.focused(&view, &favorites);
        // A new game focused starts its panel scrolled to the top again —
        // otherwise switching from a long-content game to a short one could
        // leave the scroll position pointing past the end of the new one
        // until `draw_shelf_panel`'s own clamp kicks in.
        let focus_key = focused.is_some().then(|| cur.focus_key());
        if focus_key != cur.last_focus {
            cur.panel_scroll = 0;
            cur.last_focus = focus_key;
        }
        if let Some(e) = focused {
            art.request_focus(cab, e);
        }

        ra.poll(cab, &mut ach, opts.ra.as_ref(), focused);
        man.poll(cab);

        // The flat side panel (plan revision — drawn by `Cabinet` itself,
        // outside the tube): whatever's focused right now, DAT extras and
        // all when the DAT had any ("se o DAT tiver informacoes do jogo,
        // preencher no painel").
        let shelf_panel = match focused {
            Some(e) => focused_panel_info(
                cab,
                e,
                &mut ra,
                &mut man,
                cur.panel_section,
                cur.panel_scroll,
            ),
            None => empty_panel_info(),
        };
        cab.set_shelf_panel(shelf_panel);

        // --- draw (into a screen-sized buffer, then warped through the tube) --
        let page_ready = man
            .reader
            .as_ref()
            .is_some_and(|m| cab.has_image(manual_page_id(&m.sha1, m.page)));
        // O jogo sob o cursor do mouse — o botão de play desenha na
        // capa dele (o clique nela lança; o hover é o convite).
        let pointer = pointer
            .map(|(wx, wy)| cab.window_to_output(wx, wy))
            .and_then(|(ox, oy)| cab.hit_screen_point(ox, oy));
        let hover_grid = pointer.and_then(|(lx, ly)| grid.tile_at(lx, ly, cur.top_row, view.len()));
        let hover_fav = if show_fav {
            pointer.and_then(|(lx, ly)| fav_layout.hit(lx, ly, favorites.len()))
        } else {
            None
        };
        let render = |d: &mut Screen| {
            if let Some(m) = &man.reader {
                man.rects = draw_manual_view(d, &m.sha1, m.page, m.pages, page_ready);
                return;
            }
            if ach.open {
                // The list owns the tube (plan revision: "mostrar a lista de
                // conquistas e pontuação total dentro da tv com botão de
                // voltar, parecido com o back cover"); the button rect comes
                // back for the click hit-test.
                ach.voltar = draw_achievements_view(d, ach.data.as_ref(), &ra.badge_ids, ach.top);
                return;
            }
            let count_label = if filt.query.is_empty() {
                tr!(
                    format!("{} games", view.len()),
                    format!("{} jogos", view.len())
                )
            } else {
                tr!(
                    format!("{} of {} games", view.len(), all.len()),
                    format!("{} de {} jogos", view.len(), all.len())
                )
            };
            d.text(MARGIN, MARGIN - 12, 2, DIM, &count_label);
            draw_filter_box(d, filter_rect, &filt.query, &filt.draft, filt.editing);
            draw_history_button(d, history_rect);
            draw_refresh_button(d, refresh_rect);

            if show_fav {
                d.text(MARGIN, MARGIN + HEADER_H, 1, DIM, fav_title());
                for (i, entry) in favorites
                    .iter()
                    .enumerate()
                    .skip(cur.fav_scroll)
                    .take(strip_vis)
                {
                    // Window-relative position — the strip slides left as it
                    // scrolls, not the absolute slot that would leave a
                    // blank gap at the start.
                    let (x, y) = fav_layout.item_pos(i - cur.fav_scroll);
                    draw_cover(
                        d,
                        cover_id(&entry.rom.sha1),
                        &entry.title(),
                        (x, y, STRIP_TILE_W, STRIP_TILE_H),
                        CoverLook {
                            on_shelf: false,
                            selected: cur.in_fav && i == cur.fav_idx,
                            hovered: hover_fav == Some(i),
                            favorite: true,
                        },
                    );
                }
                draw_strip_arrows(
                    d,
                    scr_w,
                    fav_layout.y0,
                    cur.fav_scroll,
                    favorites.len(),
                    strip_vis,
                );
            }

            if !grid.list_mode {
                // O móvel começa na listagem geral — os favoritos ficam
                // fora dele, soltos acima.
                draw_cabinet(d, scr_w, scr_h, grid.y0 + 1);
            }
            d.text(MARGIN, all_games_label_y, 1, DIM, all_games_title());

            if grid.list_mode {
                // Multicart menu: a plain numbered list, selection as an
                // inverted bar — no tile, no art, nothing pretending there's
                // a cover coming.
                for (i, entry) in view.iter().enumerate() {
                    let Some((x, y)) = grid.cell_pos(i, cur.top_row) else {
                        continue;
                    };
                    let selected = !cur.in_fav && i == cur.sel;
                    if selected {
                        d.fill(x, y, grid.item_w as u32, grid.item_h as u32, HILITE);
                    }
                    let label = format!("{:03}  {}", i + 1, entry.title().to_uppercase());
                    let color = if selected { HILITE_TEXT } else { TEXT };
                    d.text(x + 6, y + 3, 1, color, &label);
                }
            } else {
                // A madeira sob cada fileira visível — as caixas pousam
                // nela (a prateleira mora no vão de GAP, sem mexer nos
                // rects de clique).
                for row in 0..grid.vis_rows {
                    let y = grid.y0 + row as i32 * grid.cell_h + grid.item_h;
                    draw_shelf_board(d, MARGIN, scr_w - MARGIN as u32 * 2, y, 14);
                }
                for (i, entry) in view.iter().enumerate() {
                    let Some((x, y)) = grid.cell_pos(i, cur.top_row) else {
                        continue;
                    };
                    draw_cover(
                        d,
                        cover_id(&entry.rom.sha1),
                        &entry.title(),
                        (x, y, grid.item_w as u32, grid.item_h as u32),
                        CoverLook {
                            on_shelf: true,
                            selected: !cur.in_fav && i == cur.sel,
                            hovered: hover_grid == Some(i),
                            favorite: entry.rom.favorite,
                        },
                    );
                }
            }

            draw_scrollbar(d, &grid, view.len(), cur.top_row, scr_w);
        };

        if let (Some(path), true) = (&opts.shot, opts.max_frames == Some(frame_no)) {
            cab.capture_shelf(BG, render, path)?;
            return Ok(Pick::Quit);
        }
        // The shelf sits on the same tube as the game — the signal-off snow
        // stays faintly visible underneath it the whole time (plan §3.3),
        // not just during the entrance. Easing in from a fresh eject just
        // ramps *toward* that resting `SHELF_ALPHA` instead of straight to
        // fully opaque.
        // Back cover enlarged: it replaces the tube's content while up —
        // drawn through the same warp, on the TV — and the flat panel
        // keeps showing beside it.
        if let Some(img) = zoom_close {
            match focused {
                Some(_) => {
                    let r = cab.frame_image_zoom(img, tr!("back", "voltar"));
                    zoom_rect = (r.x(), r.y(), r.width(), r.height());
                }
                None => zoom_close = None,
            }
        } else {
            match opts.fade_in {
                Some(level) if fade_frame < FADE_IN_FRAMES => {
                    fade_frame += 1;
                    let alpha = (fade_frame as f32 / FADE_IN_FRAMES as f32) * SHELF_ALPHA;
                    cab.frame_shelf_fade_in(BG, render, level, alpha);
                }
                _ => cab.frame_shelf_fade_in(BG, render, idle::RESTING_STATIC, SHELF_ALPHA),
            }
        }

        crate::runner::pace_frame(&mut next, frame);
    }
}

/// A simple, scrollable ranking of every game with any recorded playtime,
/// most time played first (plan revision: "colocar um botao para historico
/// listando os jogos mais jogados em todo periodo") — the shelf's own
/// "Histórico" button. Clicking a row launches it, same as the shelf's own
/// grid; "Voltar" here means "back to the shelf", not all the way to idle
/// (the caller, `sixteen.rs`'s main loop, treats this call's `Pick::Back`
/// that way, same as it already special-cases `Pick::Settings`).
pub fn run_history(plat: &mut Platform, cab: &mut Cabinet, catalog: &Catalog) -> Result<Pick> {
    let logo_dir = crate::dirs::assets_dir().join("logo");
    let cartridge_dir = crate::dirs::assets_dir().join("cartridge");
    let ranked = ranked_by_playtime(catalog)?;

    let mut sel: usize = 0;
    let mut top: usize = 0;
    let frame = Duration::from_millis(16);

    cab.set_shelf_panel(empty_shelf_panel());
    cab.set_close_button(true);

    loop {
        let (scr_w, scr_h) = cab.shelf_screen_size();
        let vis_rows = ((scr_h as i32 - MARGIN * 2 - HEADER_H) / HISTORY_ROW_H).max(1) as usize;
        if sel >= ranked.len() {
            sel = ranked.len().saturating_sub(1);
        }

        let m = plat.poll_menu(MenuMode::Nav);
        if m.quit {
            return Ok(Pick::Quit);
        }
        for nav in &m.nav {
            match nav {
                MenuNav::Up => sel = sel.saturating_sub(1),
                MenuNav::Down if sel + 1 < ranked.len() => sel += 1,
                MenuNav::PageUp => sel = sel.saturating_sub(vis_rows),
                MenuNav::PageDown => sel = (sel + vis_rows).min(ranked.len().saturating_sub(1)),
                MenuNav::Home => sel = 0,
                MenuNav::End => sel = ranked.len().saturating_sub(1),
                MenuNav::Back => return Ok(Pick::Back),
                MenuNav::Confirm => {
                    if let Some((e, _)) = ranked.get(sel) {
                        return Ok(pick_play(catalog, &logo_dir, &cartridge_dir, e));
                    }
                }
                _ => {}
            }
        }
        if sel < top {
            top = sel;
        } else if sel >= top + vis_rows {
            top = sel + 1 - vis_rows;
        }

        if let Some((x, y)) = m.click {
            let (ox, oy) = cab.window_to_output(x, y);
            if cab.hit_close_button(ox, oy) {
                return Ok(Pick::Quit);
            }
            if cab.hit_minimize_button(ox, oy) {
                cab.minimize();
                continue;
            }
            if let Some(hit) = cab.hit_shelf_button(ox, oy) {
                match hit {
                    ShelfButton::Back => return Ok(Pick::Back),
                    ShelfButton::Settings => return Ok(Pick::Settings),
                    ShelfButton::Refresh => return Ok(Pick::Refresh),
                    // The history screen's panel never has any per-game
                    // content, so it never scrolls (and never has a back
                    // cover to enlarge).
                    ShelfButton::Backcover | ShelfButton::Cartridge | ShelfButton::Manual => {}
                    ShelfButton::ToggleFavorite => {}
                    ShelfButton::ShelfAchievements => {}
                    ShelfButton::PanelJump(_) => {}
                }
            } else if let Some((lx, ly)) = cab.hit_screen_point(ox, oy) {
                if let Some(i) = history_row_at(lx, ly, top, ranked.len()) {
                    if i == sel {
                        let (e, _) = &ranked[i];
                        return Ok(pick_play(catalog, &logo_dir, &cartridge_dir, e));
                    }
                    sel = i;
                }
            }
        }

        let render =
            |d: &mut Screen| draw_history_list(d, &ranked, top, vis_rows, Some(sel), scr_w);
        cab.frame_shelf(BG, render);

        std::thread::sleep(frame);
    }
}

/// Headless preview of the history screen (dev/testing) — same setup as
/// `run_history`, one frame captured through the tube instead of a live loop.
pub fn capture_history_preview(cab: &mut Cabinet, catalog: &Catalog, path: &Path) -> Result<()> {
    let ranked = ranked_by_playtime(catalog)?;
    cab.set_shelf_panel(empty_shelf_panel());
    cab.set_close_button(true);
    let (scr_w, scr_h) = cab.shelf_screen_size();
    let vis_rows = ((scr_h as i32 - MARGIN * 2 - HEADER_H) / HISTORY_ROW_H).max(1) as usize;
    let render = |d: &mut Screen| draw_history_list(d, &ranked, 0, vis_rows, None, scr_w);
    cab.capture_shelf(BG, render, path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

/// Every scanned game with `total_playtime_secs > 0`, most time played
/// first — a game never (knowingly) turned on doesn't clutter a "most
/// played" ranking.
fn ranked_by_playtime(catalog: &Catalog) -> Result<Vec<(CatalogEntry, u64)>> {
    let saves_dir = crate::dirs::saves_dir();
    let mut ranked: Vec<(CatalogEntry, u64)> = catalog
        .list(Order::Name)?
        .into_iter()
        .map(|e| {
            let title = crate::runner::rom_title(Path::new(&e.rom.path));
            let secs = crate::runner::total_playtime_secs(&saves_dir, &title);
            (e, secs)
        })
        .filter(|(_, secs)| *secs > 0)
        .collect();
    ranked.sort_by_key(|(_, secs)| std::cmp::Reverse(*secs));
    Ok(ranked)
}

/// A blank flat panel — just the Voltar/Configuracoes buttons, no game
/// focused (the history list isn't a "selection" the way the shelf's grid
/// is; clicking a row launches straight away instead of just selecting it).
fn empty_shelf_panel() -> ShelfPanelInfo {
    ShelfPanelInfo {
        section: PanelSection::Informacoes,
        achievements_reason: None,
        manual_first_page: None,
        title: String::new(),
        logo_img: None,
        cartridge_img: None,
        backcover_img: None,
        release: None,
        info: Vec::new(),
        scroll: 0,
        favorite: None,
        achievements: false,
        award_img: None,
        igdb: None,
        hltb: None,
    }
}

const HISTORY_ROW_H: i32 = 28;

/// Which visible ranked row (if any) a screen-local point sits inside.
fn history_row_at(x: i32, y: i32, top: usize, len: usize) -> Option<usize> {
    let row_top = MARGIN + HEADER_H;
    if x < MARGIN || y < row_top {
        return None;
    }
    let i = top + ((y - row_top) / HISTORY_ROW_H) as usize;
    (i < len).then_some(i)
}

fn draw_history_list(
    d: &mut Screen,
    ranked: &[(CatalogEntry, u64)],
    top: usize,
    vis_rows: usize,
    sel: Option<usize>,
    scr_w: u32,
) {
    d.text(
        MARGIN,
        MARGIN - 12,
        2,
        DIM,
        tr!("history - most played", "histórico - mais jogados"),
    );
    if ranked.is_empty() {
        d.text_wrapped(
            MARGIN,
            MARGIN + 30,
            scr_w.saturating_sub(MARGIN as u32 * 2),
            1,
            DIM,
            tr!(
                "no game has recorded time yet -- play something with the console on.",
                "nenhum jogo tem tempo registrado ainda -- jogue algo com o console ligado."
            ),
        );
        return;
    }
    let row_w = scr_w.saturating_sub(MARGIN as u32 * 2);
    for (i, (entry, secs)) in ranked.iter().enumerate().skip(top).take(vis_rows) {
        let y = MARGIN + HEADER_H + (i - top) as i32 * HISTORY_ROW_H;
        let selected = sel == Some(i);
        if selected {
            d.fill(MARGIN, y, row_w, HISTORY_ROW_H as u32 - 4, HILITE);
        }
        let color = if selected { HILITE_TEXT } else { TEXT };
        let label = format!("{:02}. {}", i + 1, entry.title().to_uppercase());
        d.text(MARGIN + 6, y + 4, 1, color, &label);
        let time_label = format_playtime(*secs);
        let tw = time_label.chars().count() as i32 * 9;
        d.text(MARGIN + row_w as i32 - tw - 6, y + 4, 1, color, &time_label);
    }
}

fn in_rect(x: i32, y: i32, (rx, ry, rw, rh): (i32, i32, u32, u32)) -> bool {
    x >= rx && y >= ry && x < rx + rw as i32 && y < ry + rh as i32
}

/// The title filter box (plan revision): shows the live draft with a text
/// cursor while typing, the applied query otherwise, or a placeholder when
/// empty — long text scrolls from the right so the cursor stays visible.
fn draw_filter_box(
    d: &mut Screen,
    (fx, fy, fw, fh): (i32, i32, u32, u32),
    query: &str,
    draft: &str,
    editing: bool,
) {
    d.outline(fx, fy, fw, fh, 1, (150, 150, 158, 255));
    d.fill(fx + 1, fy + 1, fw - 2, fh - 2, (34, 34, 40, 255));
    let (raw, color) = if editing {
        (format!("{draft}_"), TEXT)
    } else if !query.is_empty() {
        (query.to_string(), TEXT)
    } else {
        (tr!("search...", "buscar...").to_string(), DIM)
    };
    let max_chars = ((fw as i32 - 16) / 9).max(4) as usize;
    let shown: String = if raw.chars().count() > max_chars {
        let tail: Vec<char> = raw.chars().rev().take(max_chars).collect();
        tail.into_iter().rev().collect()
    } else {
        raw
    };
    d.text(fx + 8, fy + (fh as i32 - 16) / 2, 1, color, &shown);
}

/// The "Histórico"/"Atualizar" buttons — a linguagem fósforo (plan
/// revision): sem caixa, o rótulo em `FOSFORO` com sublinhado, dentro do
/// mesmo rect de sempre (o hit-test não muda).
fn draw_history_button(d: &mut Screen, r: (i32, i32, u32, u32)) {
    draw_fosforo_btn(d, r, tr!("history", "histórico"), true);
}

fn draw_refresh_button(d: &mut Screen, r: (i32, i32, u32, u32)) {
    draw_fosforo_btn(d, r, tr!("refresh", "atualizar"), true);
}

/// Um botão da linguagem fósforo: texto centrado no rect, `FOSFORO` com
/// sublinhado quando ativo, `DIM` sem sublinhado quando inativo — sem
/// caixa nenhuma, o OSD é o próprio texto. O rect de clique continua sendo
/// o que o caller guarda (a geometria não muda, só o desenho).
fn draw_fosforo_btn(
    d: &mut Screen,
    (bx, by, bw, bh): (i32, i32, u32, u32),
    label: &str,
    ativo: bool,
) {
    let (cor, sub) = if ativo { (FOSFORO, true) } else { (DIM, false) };
    let tw = label.chars().count() as i32 * 9;
    let tx = bx + (bw as i32 - tw).max(0) / 2;
    let ty = by + (bh as i32 - 20) / 2;
    d.text(tx, ty, 1, cor, label);
    if sub {
        d.fill(
            tx,
            ty + 21,
            tw.max(9) as u32,
            2,
            (FOSFORO.0, FOSFORO.1, FOSFORO.2, 255),
        );
    }
}

/// A thin vertical scroll indicator at the grid's right edge (plan revision:
/// "mostrar rolagem na tv para visualizar os jogos") — only drawn once the
/// list actually overflows the visible rows.
fn draw_scrollbar(d: &mut Screen, grid: &GridLayout, view_len: usize, top_row: usize, scr_w: u32) {
    let total_rows = grid.total_rows(view_len);
    if total_rows <= grid.vis_rows {
        return;
    }
    let track_x = scr_w as i32 - MARGIN - SCROLLBAR_W as i32;
    let track_y = grid.y0;
    let track_h = (grid.vis_rows as i32 * grid.cell_h - GAP as i32).max(SCROLLBAR_W as i32);
    d.fill(
        track_x,
        track_y,
        SCROLLBAR_W,
        track_h as u32,
        (48, 44, 42, 255),
    );

    let thumb_h = ((grid.vis_rows as f32 / total_rows as f32) * track_h as f32)
        .max(16.0)
        .min(track_h as f32) as u32;
    let max_scroll = (total_rows - grid.vis_rows) as f32;
    let scroll_frac = if max_scroll > 0.0 {
        (top_row as f32 / max_scroll).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let thumb_y = track_y + (scroll_frac * (track_h - thumb_h as i32).max(0) as f32) as i32;
    d.fill(track_x, thumb_y, SCROLLBAR_W, thumb_h, (200, 194, 186, 255));
}

/// Decode a cover or logo PNG/JPEG to tightly-packed RGBA, downscaled for
/// memory (covers feed 200x150 tiles, logos a ~340px panel slot — 512 covers
/// both at >2x and keeps alpha for transparent logos).
/// Decodificação das artes da estante fora da thread de UI: um JPEG de
/// capa custa 10–40 ms, mais que o frame inteiro (16,6 ms) — decodificado
/// no laço, cada capa nova engasgava a rolagem. O laço só pede e, a cada
/// frame, registra no cabinet o que já ficou pronto (`drain`). A busca no
/// disco (`find_local_art`) também vai para o worker. A thread termina
/// sozinha quando o `ShelfArt` sai de escopo (o canal fecha).
///
/// Cada arte é tentada no máximo uma vez por visita à estante, achando
/// arquivo ou não — a maioria dos jogos não tem, e disco não é de graça.
struct ShelfArt {
    tx: mpsc::Sender<ArtJob>,
    rx: Receiver<(u64, Option<Decoded>)>,
    cover_dir: PathBuf,
    logo_dir: PathBuf,
    cartridge_dir: PathBuf,
    backcover_dir: PathBuf,
    tried: HashSet<(ArtKind, String)>,
}

/// Uma arte decodificada: largura, altura e os pixels RGBA.
type Decoded = (u32, u32, Vec<u8>);

/// Qual arte de um jogo — cada uma tem sua pasta em `assets/`.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ArtKind {
    Cover,
    Logo,
    Cartridge,
    Backcover,
}

/// Um pedido ao worker: a imagem `id`, a arte de `rom_path` em `dir`,
/// reduzida para caber em `max` px.
struct ArtJob {
    id: u64,
    dir: PathBuf,
    rom_path: String,
    max: u32,
}

impl ShelfArt {
    fn new() -> Self {
        let (tx, jobs) = mpsc::channel::<ArtJob>();
        let (done, rx) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("shelf-art".into())
            .spawn(move || {
                for job in jobs {
                    let img = find_local_art(&job.dir, &job.rom_path)
                        .and_then(|p| decode_art_scaled(&p, job.max).ok());
                    if done.send((job.id, img)).is_err() {
                        break;
                    }
                }
            });
        let assets = crate::dirs::assets_dir();
        Self {
            tx,
            rx,
            cover_dir: assets.join("cover"),
            logo_dir: assets.join("logo"),
            cartridge_dir: assets.join("cartridge"),
            backcover_dir: assets.join("backcover"),
            tried: HashSet::new(),
        }
    }

    fn dir(&self, kind: ArtKind) -> &Path {
        match kind {
            ArtKind::Cover => &self.cover_dir,
            ArtKind::Logo => &self.logo_dir,
            ArtKind::Cartridge => &self.cartridge_dir,
            ArtKind::Backcover => &self.backcover_dir,
        }
    }

    fn send(&self, id: u64, kind: ArtKind, e: &CatalogEntry, max: u32) {
        let _ = self.tx.send(ArtJob {
            id,
            dir: self.dir(kind).to_path_buf(),
            rom_path: e.rom.path.clone(),
            max,
        });
    }

    /// Pede a miniatura (512 px) de `kind` para `e`, se a imagem ainda não
    /// está no cabinet e não foi tentada nesta visita. `true` = pediu.
    fn request(&mut self, cab: &Cabinet, id: u64, kind: ArtKind, e: &CatalogEntry) -> bool {
        if cab.has_image(id) || !self.tried.insert((kind, e.rom.sha1.clone())) {
            return false;
        }
        self.send(id, kind, e, 512);
        true
    }

    /// As capas de `entries`, no máximo `cap` pedidos novos.
    fn request_covers<'a>(
        &mut self,
        cab: &Cabinet,
        entries: impl Iterator<Item = &'a CatalogEntry>,
        cap: usize,
    ) {
        let mut left = cap;
        for e in entries {
            if left == 0 {
                break;
            }
            if self.request(cab, cover_id(&e.rom.sha1), ArtKind::Cover, e) {
                left -= 1;
            }
        }
    }

    /// Logo, cartucho e contracapa do jogo focado (o painel lateral).
    fn request_focus(&mut self, cab: &Cabinet, e: &CatalogEntry) {
        self.request(cab, wheel_id(&e.rom.sha1), ArtKind::Logo, e);
        self.request(cab, cartridge_id(&e.rom.sha1), ArtKind::Cartridge, e);
        self.request(cab, backcover_id(&e.rom.sha1), ArtKind::Backcover, e);
    }

    /// O zoom: a versão de 2048 px troca a miniatura quando chegar.
    fn request_zoom(&self, id: u64, kind: ArtKind, e: &CatalogEntry) {
        self.send(id, kind, e, 2048);
    }

    /// Registra no cabinet o que já foi decodificado — no máximo 4 por
    /// frame (o upload da textura também custa).
    fn drain(&self, cab: &mut Cabinet) {
        for (id, img) in self.rx.try_iter().take(4) {
            if let Some((w, h, rgba)) = img {
                cab.set_image(id, w, h, &rgba);
            }
        }
    }
}

/// Decode one art image, thumbnailed to fit `max` px (512 for the shelf
/// and panel; 2048 for the zoomed view, so the enlargement stays crisp).
pub(crate) fn decode_art_scaled(path: &Path, max: u32) -> Result<Decoded> {
    let img = image::open(path)?.thumbnail(max, max).to_rgba8();
    let (w, h) = img.dimensions();
    Ok((w, h, img.into_raw()))
}

#[cfg(test)]
mod tests {
    use super::find_local_art;

    #[test]
    fn finds_art_for_variant_roms_via_the_base_name() {
        let dir = std::env::temp_dir().join(format!("shelf-art-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("Killer Instinct (USA).jpg");
        std::fs::write(&base, b"jpg").unwrap();

        // Exact stem hits.
        let hit = find_local_art(&dir, "/x/Killer Instinct (USA).sfc").unwrap();
        assert_eq!(hit, base);
        // A variant ROM ("Rev 1") peels tags down to the base-named file.
        let hit = find_local_art(&dir, "/x/Killer Instinct (USA) (Rev 1).sfc").unwrap();
        assert_eq!(hit, base);

        // PNG wins over JPG for the same stem.
        std::fs::write(dir.join("Killer Instinct (USA).png"), b"png").unwrap();
        let hit = find_local_art(&dir, "/x/Killer Instinct (USA).sfc").unwrap();
        assert_eq!(hit.extension().unwrap(), "png");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn nothing_found_leaves_none() {
        let dir = std::env::temp_dir().join(format!("shelf-art-none-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(find_local_art(&dir, "/x/Missing Game (USA).sfc"), None);
        // A stem that only shares a prefix must not match anything.
        std::fs::write(dir.join("Game (USA).jpg"), b"x").unwrap();
        assert_eq!(find_local_art(&dir, "/x/Gam.sfc"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

#[cfg(test)]
mod cart_tests {
    use super::*;

    #[test]
    fn cart_size_matches_the_box_labels() {
        // O rótulo do cartucho agora sai do `tr!` — fixa o inglês para o
        // resultado não depender do locale da máquina.
        tr::set_lang(Some(tr::Lang::En));
        let mib = 1024 * 1024;
        assert_eq!(cart_size(4 * mib), "32 megs"); // SF Alpha 2
        assert_eq!(cart_size(512 * 1024), "4 megs"); // Super Mario World
        assert_eq!(cart_size(3 * mib), "24 megs"); // e.g. Mortal Kombat II? no — 24 Mbit carts exist
        assert_eq!(cart_size(6 * mib), "48 megs"); // Tales of Phantasia
                                                   // Copier headers don't inflate the cartridge.
        assert_eq!(cart_size(512 * 1024 + 512), "4 megs");
        assert_eq!(cart_size(4 * mib + 512), "32 megs");
        // Below the smallest mask: rounds up to it.
        assert_eq!(cart_size(8 * 1024), "2 megs");
    }
}
