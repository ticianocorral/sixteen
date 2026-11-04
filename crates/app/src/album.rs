//! Álbum de prints — todas as capturas de tela que o jogador guardou, de
//! todos os jogos, num álbum de fotos (página de couro escuro, fotos com
//! moldura de polaroide, legenda à mão).
//!
//! Os prints vivem em `notes/<jogo>/NN.png` (até 15 por jogo, ver
//! `runner::note_slot_path`); a legenda de cada um fica em
//! `notes/<jogo>/slots.json` (`slots.<n>.label`). Esta tela só LÊ — nada
//! aqui grava no disco.
//!
//! Controles (100% no controle, pensado no Steam Deck — nunca Guide, L4/R4/
//! L5/R5, trackpads, nem L3/R3): d-pad move, A abre a foto grande, B volta
//! (foto grande -> grade -> estante), L/R trocam o filtro por jogo, na foto
//! grande esquerda/direita (ou L/R) passam as fotos. Mouse e toque: clicar
//! numa foto abre, setas na tela e o chip do filtro respondem a clique.
//!
//! As imagens são decodificadas numa thread de fundo e sobem para a GPU no
//! máximo [`UPLOADS_PER_FRAME`] por frame; só as fotos visíveis (e as
//! vizinhas da foto grande) são pedidas — álbum com centenas de prints não
//! engasga o quadro.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, UNIX_EPOCH};

use anyhow::Result;
use sixteen_platform::tr;
use sixteen_platform::{Cabinet, MenuMode, MenuNav, PanelSection, Platform, Screen, ShelfButton};

use crate::shelf::{self, Pick};

/// Fundo da página: couro escuro.
const LEATHER: (u8, u8, u8) = (38, 29, 25);
const STITCH: (u8, u8, u8, u8) = (96, 72, 54, 255);
const PAPER: (u8, u8, u8, u8) = (244, 240, 228, 255);
const INK: (u8, u8, u8) = (38, 40, 70);
const INK_DIM: (u8, u8, u8) = (110, 104, 96);
const DIM: (u8, u8, u8) = (170, 150, 130);
const TEXT: (u8, u8, u8) = (236, 226, 210);
const HILITE: (u8, u8, u8, u8) = (240, 200, 80, 255);
const FOSFORO: (u8, u8, u8) = (186, 138, 244);
const FOSFORO_A: (u8, u8, u8, u8) = (186, 138, 244, 255);

const MARGIN: i32 = 28;
const HEADER_H: i32 = 28;
/// Altura reservada para a legenda dos controles, no pé da tela.
const LEGEND_H: i32 = 30;
/// Borda branca da polaroide e faixa de legenda embaixo da foto.
const BORDER: i32 = 7;
const STRIP_H: i32 = 38;
/// Texturas subidas por frame (o upload também custa).
const UPLOADS_PER_FRAME: usize = 4;
/// Prefixo dos ids de textura do álbum — longe dos ids de capa (SHA1).
const ID_BASE: u64 = 0xA1B0_0000_0000_0000;

fn print_id(index: usize) -> u64 {
    ID_BASE | index as u64
}

// ---------------------------------------------------------------------
// Dados: varredura da pasta de notas
// ---------------------------------------------------------------------

/// Um print guardado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Print {
    /// Nome da pasta do jogo (o título já saneado).
    pub game: String,
    pub slot: u8,
    pub path: PathBuf,
    /// Legenda (vazia = sem legenda).
    pub caption: String,
    /// Segundos desde a época (mtime do arquivo).
    pub mtime: u64,
}

/// `"07.png"` -> `Some(7)`; qualquer outra coisa (ou fora de 1..=15) `None`.
fn parse_slot_file(name: &str) -> Option<u8> {
    let stem = name.strip_suffix(".png")?;
    if stem.len() != 2 || !stem.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    stem.parse().ok().filter(|n| (1..=15).contains(n))
}

/// As legendas de `slots.json` (`{"slots": {"3": {"label": "chefe"}}}`),
/// tolerante como o resto: JSON ruim = sem legendas.
fn parse_captions(json: &str) -> BTreeMap<u8, String> {
    let mut out = BTreeMap::new();
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else {
        return out;
    };
    if let Some(slots) = v.get("slots").and_then(|s| s.as_object()) {
        for (k, meta) in slots {
            let label = meta.get("label").and_then(|l| l.as_str()).unwrap_or("");
            if let (Ok(n), false) = (k.parse::<u8>(), label.trim().is_empty()) {
                out.insert(n, label.trim().to_string());
            }
        }
    }
    out
}

/// Todos os prints sob `notes_dir`, o mais novo primeiro.
pub fn list_prints(notes_dir: &Path) -> Vec<Print> {
    let mut out = Vec::new();
    let Ok(dirs) = std::fs::read_dir(notes_dir) else {
        return out;
    };
    for d in dirs.flatten() {
        let dir = d.path();
        if !dir.is_dir() {
            continue;
        }
        let game = d.file_name().to_string_lossy().into_owned();
        let captions = std::fs::read_to_string(dir.join("slots.json"))
            .map(|s| parse_captions(&s))
            .unwrap_or_default();
        let Ok(files) = std::fs::read_dir(&dir) else {
            continue;
        };
        for f in files.flatten() {
            let Some(slot) = parse_slot_file(&f.file_name().to_string_lossy()) else {
                continue;
            };
            let mtime = f
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            out.push(Print {
                game: game.clone(),
                slot,
                path: f.path(),
                caption: captions.get(&slot).cloned().unwrap_or_default(),
                mtime,
            });
        }
    }
    out.sort_by(|a, b| {
        b.mtime
            .cmp(&a.mtime)
            .then_with(|| a.game.cmp(&b.game))
            .then(a.slot.cmp(&b.slot))
    });
    out
}

/// Os jogos que têm print, em ordem alfabética (sem diferenciar caixa).
fn games_of(prints: &[Print]) -> Vec<String> {
    let mut g: Vec<String> = prints.iter().map(|p| p.game.clone()).collect();
    g.sort_by_key(|s| s.to_lowercase());
    g.dedup();
    g
}

/// `AAAA-MM-DD` de um instante Unix (UTC) — algoritmo de dias civis.
fn civil_date(epoch: u64) -> (i64, u32, u32) {
    let z = (epoch / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

fn format_date(epoch: u64) -> String {
    let (y, m, d) = civil_date(epoch);
    tr!(
        format!("{y:04}-{m:02}-{d:02}"),
        format!("{d:02}/{m:02}/{y:04}")
    )
}

/// `s` cortado em `max` caracteres, com ".." no fim quando sobra.
fn fit_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(2);
    let mut t: String = s.chars().take(keep).collect();
    t.push_str("..");
    t
}

// ---------------------------------------------------------------------
// Estado e navegação (lógica pura)
// ---------------------------------------------------------------------

struct AlbumState {
    prints: Vec<Print>,
    games: Vec<String>,
    /// `None` = todos os jogos; `Some(i)` = só `games[i]`.
    filter: Option<usize>,
    /// Posição na lista visível (não em `prints`).
    cursor: usize,
    /// Primeira linha da grade visível na tela.
    top_row: usize,
    /// Foto grande aberta.
    viewing: bool,
}

impl AlbumState {
    fn new(prints: Vec<Print>) -> Self {
        let games = games_of(&prints);
        Self {
            prints,
            games,
            filter: None,
            cursor: 0,
            top_row: 0,
            viewing: false,
        }
    }

    /// Índices (em `prints`) dos prints que o filtro deixa passar.
    fn visible(&self) -> Vec<usize> {
        self.prints
            .iter()
            .enumerate()
            .filter(|(_, p)| self.filter.is_none_or(|g| self.games[g] == p.game))
            .map(|(i, _)| i)
            .collect()
    }

    /// O print sob o cursor (índice em `prints`).
    fn current(&self) -> Option<usize> {
        self.visible().get(self.cursor).copied()
    }

    /// Anda o filtro `dir` passos pelo ciclo "todos, jogo 1, jogo 2, ..".
    fn cycle_filter(&mut self, dir: i32) {
        let n = self.games.len() as i32 + 1;
        let cur = self.filter.map_or(0, |g| g as i32 + 1);
        let next = (cur + dir).rem_euclid(n);
        self.filter = (next > 0).then(|| (next - 1) as usize);
        self.cursor = 0;
        self.top_row = 0;
        self.viewing = false;
    }

    /// Move o cursor da grade `dx` colunas / `dy` linhas; trava nas pontas
    /// (descer para uma última linha curta cai no último print).
    fn move_cursor(&mut self, dx: i32, dy: i32, cols: usize) {
        let len = self.visible().len();
        if len == 0 {
            return;
        }
        let step = dx + dy * cols as i32;
        let target = (self.cursor as i32 + step).clamp(0, len as i32 - 1);
        self.cursor = target as usize;
    }

    fn home_end(&mut self, end: bool) {
        let len = self.visible().len();
        self.cursor = if end { len.saturating_sub(1) } else { 0 };
    }

    /// Passa a foto grande `dir` posições (dá a volta nas pontas).
    fn step_view(&mut self, dir: i32) {
        let len = self.visible().len() as i32;
        if len > 0 {
            self.cursor = (self.cursor as i32 + dir).rem_euclid(len) as usize;
        }
    }

    /// Mantém a linha do cursor dentro da janela de `vis_rows` linhas.
    fn scroll_to_cursor(&mut self, cols: usize, vis_rows: usize) {
        let row = self.cursor / cols.max(1);
        if row < self.top_row {
            self.top_row = row;
        } else if row >= self.top_row + vis_rows {
            self.top_row = row + 1 - vis_rows;
        }
    }

    fn filter_label(&self) -> String {
        match self.filter {
            None => tr!("all games", "todos os jogos").to_string(),
            Some(g) => self.games[g].clone(),
        }
    }
}

// ---------------------------------------------------------------------
// Geometria
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Layout {
    cols: usize,
    vis_rows: usize,
    x0: i32,
    y0: i32,
    cell_w: i32,
    cell_h: i32,
}

fn layout(w: u32, h: u32) -> Layout {
    let inner_w = (w as i32 - MARGIN * 2).max(100);
    let cols = ((inner_w / 150).clamp(2, 6)) as usize;
    let cell_w = inner_w / cols as i32;
    // Foto 8:7 (a proporção do quadro do SNES) dentro da polaroide (cell_w - 10 de folga entre colunas).
    let photo_w = cell_w - 10 - BORDER * 2;
    let photo_h = photo_w * 7 / 8;
    let cell_h = photo_h + BORDER + STRIP_H + 12;
    let y0 = MARGIN + HEADER_H + 6;
    let area_h = h as i32 - y0 - MARGIN - LEGEND_H + 14;
    Layout {
        cols,
        vis_rows: (area_h / cell_h).max(1) as usize,
        x0: MARGIN,
        y0,
        cell_w,
        cell_h,
    }
}

impl Layout {
    /// Retângulo da polaroide da célula (linha, coluna) visível.
    fn polaroid(&self, row: usize, col: usize) -> (i32, i32, u32, u32) {
        let x = self.x0 + col as i32 * self.cell_w + 5;
        let y = self.y0 + row as i32 * self.cell_h + 4;
        (x, y, (self.cell_w - 10) as u32, (self.cell_h - 12) as u32)
    }

    /// Moldura da foto dentro da polaroide.
    fn photo(&self, row: usize, col: usize) -> (i32, i32, u32, u32) {
        let (x, y, w, h) = self.polaroid(row, col);
        (
            x + BORDER,
            y + BORDER,
            w - BORDER as u32 * 2,
            h - (BORDER + STRIP_H) as u32,
        )
    }

    /// Índice visível sob um ponto da tela, dado `top_row` e `len`.
    fn cell_at(&self, x: i32, y: i32, top_row: usize, len: usize) -> Option<usize> {
        if x < self.x0 || y < self.y0 {
            return None;
        }
        let col = ((x - self.x0) / self.cell_w) as usize;
        let row = ((y - self.y0) / self.cell_h) as usize;
        if col >= self.cols || row >= self.vis_rows {
            return None;
        }
        let i = (top_row + row) * self.cols + col;
        (i < len).then_some(i)
    }
}

type Rect = (i32, i32, u32, u32);

fn in_rect(x: i32, y: i32, (rx, ry, rw, rh): Rect) -> bool {
    x >= rx && y >= ry && x < rx + rw as i32 && y < ry + rh as i32
}

/// Chip do filtro, no canto do cabeçalho: [<] nome [>].
fn filter_rects(w: u32) -> (Rect, Rect) {
    let y = MARGIN - 4;
    let right = w as i32 - MARGIN;
    ((right - 300, y, 36, 28), (right - 36, y, 36, 28))
}

/// Setas e área da foto na visão grande.
fn view_rects(w: u32, h: u32) -> (Rect, Rect, Rect) {
    let arrow_w = 56;
    let top = MARGIN + 6;
    let bottom = h as i32 - MARGIN - LEGEND_H;
    let mid = (top + bottom) / 2 - 28;
    let left = (6, mid, arrow_w as u32, 56);
    let right = (w as i32 - 6 - arrow_w, mid, arrow_w as u32, 56);
    let pol = (
        arrow_w + 14,
        top,
        (w as i32 - 2 * (arrow_w + 14)).max(60) as u32,
        (bottom - top).max(60) as u32,
    );
    (left, right, pol)
}

// ---------------------------------------------------------------------
// Carregador de fundo
// ---------------------------------------------------------------------

struct Loader {
    tx: Sender<(u64, PathBuf)>,
    rx: Receiver<(u64, Option<sixteen_platform::PreparedArt>)>,
    asked: HashSet<u64>,
    failed: HashSet<u64>,
}

impl Loader {
    fn new() -> Self {
        let (tx, jobs) = mpsc::channel::<(u64, PathBuf)>();
        let (done, rx) = mpsc::channel();
        let _ = std::thread::Builder::new()
            .name("album-art".into())
            .spawn(move || {
                for (id, path) in jobs {
                    let img = shelf::decode_art_scaled(&path, 1024)
                        .ok()
                        .map(|(w, h, rgba)| sixteen_platform::prepare_art(w, h, rgba));
                    if done.send((id, img)).is_err() {
                        break;
                    }
                }
            });
        Self {
            tx,
            rx,
            asked: HashSet::new(),
            failed: HashSet::new(),
        }
    }

    fn request(&mut self, cab: &Cabinet, index: usize, p: &Print) {
        let id = print_id(index);
        if cab.has_image(id) || !self.asked.insert(id) {
            return;
        }
        let _ = self.tx.send((id, p.path.clone()));
    }

    fn drain(&mut self, cab: &mut Cabinet) {
        for (id, img) in self.rx.try_iter().take(UPLOADS_PER_FRAME) {
            match img {
                Some(art) => cab.set_prepared_image(id, art),
                None => {
                    self.failed.insert(id);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------
// Desenho
// ---------------------------------------------------------------------

/// Largura aproximada, em caracteres, que `w` px comportam na letra à mão
/// de tamanho `size` (avanço = size * 9/20 * 0.92).
fn hand_cols(w: i32, size: f32) -> usize {
    ((w as f32 / (size * 0.45 * 0.92 * 1.03)) as usize).max(1)
}

fn text_w(s: &str) -> i32 {
    s.chars().count() as i32 * 9
}

fn draw_page(d: &mut Screen) {
    let (w, h) = d.size();
    d.fill(0, 0, w, h, (LEATHER.0, LEATHER.1, LEATHER.2, 255));
    // Costura em volta da página.
    d.outline(8, 8, w - 16, h - 16, 1, STITCH);
}

fn draw_header(d: &mut Screen, st: &AlbumState, shown: usize) {
    let (w, _) = d.size();
    d.text(
        MARGIN,
        MARGIN - 12,
        2,
        DIM,
        tr!("print album", "album de prints"),
    );
    let (prev, next) = filter_rects(w);
    for (r, g) in [(prev, "<"), (next, ">")] {
        d.text(r.0 + 13, r.1 + 4, 1, FOSFORO, g);
    }
    let label = st.filter_label();
    let room = ((next.0 - (prev.0 + prev.2 as i32)) / 9 - 1).max(4) as usize;
    let label = fit_chars(&label.to_uppercase(), room);
    let cx = prev.0 + prev.2 as i32 + ((next.0 - prev.0 - prev.2 as i32) - text_w(&label)) / 2;
    d.text(cx, prev.1 - 8, 1, TEXT, &label);
    let count = tr!(format!("{shown} prints"), format!("{shown} prints"));
    let count = if shown == 1 {
        tr!("1 print".to_string(), "1 print".to_string())
    } else {
        count
    };
    let cx = prev.0 + prev.2 as i32 + ((next.0 - prev.0 - prev.2 as i32) - text_w(&count)) / 2;
    d.text(cx, prev.1 + 12, 1, DIM, &count);
}

fn draw_legend(d: &mut Screen, viewing: bool, empty: bool) {
    let (_, h) = d.size();
    let s = if empty {
        tr!("B: back", "B: voltar")
    } else if viewing {
        tr!(
            "left/right: other print -- B: back",
            "esquerda/direita: outro print -- B: voltar"
        )
    } else {
        tr!(
            "d-pad: move -- A: open -- L/R: game -- B: back",
            "direcional: mover -- A: abrir -- L/R: jogo -- B: voltar"
        )
    };
    d.text(MARGIN, h as i32 - MARGIN, 1, DIM, s);
}

/// A polaroide (moldura + foto + legenda) em `(x, y, w, h)`; `photo` é o
/// retângulo da foto dentro dela; o último argumento é (falhou ao
/// carregar, tipografia grande, selecionada).
fn draw_polaroid(
    d: &mut Screen,
    r: Rect,
    photo: Rect,
    index: usize,
    p: &Print,
    (failed, big, selected): (bool, bool, bool),
) {
    let (x, y, w, h) = r;
    d.fill(x + 3, y + 4, w, h, (0, 0, 0, 110));
    if selected {
        d.fill(x - 3, y - 3, w + 6, h + 6, HILITE);
    }
    d.fill(x, y, w, h, PAPER);
    let (px, py, pw, ph) = photo;
    d.fill(px, py, pw, ph, (16, 16, 20, 255));
    let id = print_id(index);
    if d.has_image(id) {
        d.image_fit(id, px, py, pw, ph);
    } else {
        let msg = if failed { "?" } else { "..." };
        d.text(
            px + (pw as i32 - text_w(msg)) / 2,
            py + ph as i32 / 2 - 10,
            1,
            (90, 90, 100),
            msg,
        );
    }
    // Faixa de baixo: legenda à mão + jogo/data pequenos.
    let sx = x + BORDER + 2;
    let sw = w as i32 - BORDER * 2 - 4;
    let sy = py + ph as i32 + 4;
    let (hand, small_y) = if big {
        (24.0, sy + 34)
    } else {
        (15.0, sy + 18)
    };
    let seed = (p.slot as u32) ^ (p.mtime as u32);
    if !p.caption.is_empty() {
        let cap = fit_chars(&p.caption, hand_cols(sw, hand));
        d.text_hand((sx as f32, sy as f32), -0.01, hand, INK, &cap, seed);
    }
    let meta = if big {
        format!("{} -- {}", p.game, format_date(p.mtime))
    } else {
        p.game.clone()
    };
    let meta = fit_chars(&meta, (sw / 9).max(4) as usize);
    d.text(sx, small_y.min(y + h as i32 - 20), 1, INK_DIM, &meta);
}

fn draw_empty(d: &mut Screen) {
    let (w, h) = d.size();
    let msg = tr!(
        "no prints yet -- take one while playing with the Print button on the panel.",
        "nenhum print ainda -- tire prints durante o jogo com o botão Print do painel."
    );
    let box_w = (w as i32 - MARGIN * 4).clamp(120, 520);
    let x = (w as i32 - box_w) / 2;
    let rows = d.wrapped_height(box_w as u32, 1, msg);
    let y = (h as i32 - rows) / 2 - 10;
    // Uma polaroide vazia de enfeite.
    d.fill(x + (box_w - 70) / 2 + 3, y - 74, 70, 60, (0, 0, 0, 110));
    d.fill(x + (box_w - 70) / 2, y - 78, 70, 60, PAPER);
    d.fill(x + (box_w - 70) / 2 + 6, y - 72, 58, 38, (16, 16, 20, 255));
    d.text_wrapped(x, y, box_w as u32, 1, TEXT, msg);
}

fn draw_grid(d: &mut Screen, st: &AlbumState, lay: &Layout, failed: &HashSet<u64>) {
    let vis = st.visible();
    for (n, &pi) in vis
        .iter()
        .enumerate()
        .skip(st.top_row * lay.cols)
        .take(lay.cols * lay.vis_rows)
    {
        let rel = n - st.top_row * lay.cols;
        let (row, col) = (rel / lay.cols, rel % lay.cols);
        draw_polaroid(
            d,
            lay.polaroid(row, col),
            lay.photo(row, col),
            pi,
            &st.prints[pi],
            (failed.contains(&print_id(pi)), false, n == st.cursor),
        );
    }
    // Barra de rolagem fina quando há mais linhas que cabem.
    let rows = vis.len().div_ceil(lay.cols);
    if rows > lay.vis_rows {
        let (w, _) = d.size();
        let track_h = lay.vis_rows as i32 * lay.cell_h;
        let th = (track_h * lay.vis_rows as i32 / rows as i32).max(16);
        let ty = lay.y0 + (track_h - th) * st.top_row as i32 / (rows - lay.vis_rows).max(1) as i32;
        d.fill(w as i32 - 14, ty, 4, th as u32, STITCH);
    }
}

fn draw_view(d: &mut Screen, st: &AlbumState, failed: &HashSet<u64>) {
    let (w, h) = d.size();
    let vis = st.visible();
    let Some(&pi) = vis.get(st.cursor) else {
        return;
    };
    let p = &st.prints[pi];
    let (left, right, area) = view_rects(w, h);
    // Polaroide grande: a foto toma a maior área 4:3 (ou a proporção real).
    let (ax, ay, aw, ah) = area;
    let (iw, ih) = d.image_size(print_id(pi)).unwrap_or((4, 3));
    let strip = BORDER + 60;
    let max_pw = aw as i32 - BORDER * 2;
    let max_ph = ah as i32 - strip - BORDER;
    let s = (max_pw as f32 / iw as f32).min(max_ph as f32 / ih as f32);
    let (pw, ph) = (
        ((iw as f32 * s) as i32).max(40),
        ((ih as f32 * s) as i32).max(30),
    );
    let (rw, rh) = (pw + BORDER * 2, ph + BORDER + strip);
    let rx = ax + (aw as i32 - rw) / 2;
    let ry = ay + (ah as i32 - rh) / 2;
    draw_polaroid(
        d,
        (rx, ry, rw as u32, rh as u32),
        (rx + BORDER, ry + BORDER, pw as u32, ph as u32),
        pi,
        p,
        (failed.contains(&print_id(pi)), true, false),
    );
    // Posição "n / total" no canto da faixa.
    let pos = format!("{} / {}", st.cursor + 1, vis.len());
    d.text(
        rx + rw - BORDER - 2 - text_w(&pos),
        ry + rh - 22,
        1,
        INK_DIM,
        &pos,
    );
    if vis.len() > 1 {
        for (r, g) in [(left, "<"), (right, ">")] {
            d.fill(r.0, r.1, r.2, r.3, (60, 46, 38, 255));
            d.outline(r.0, r.1, r.2, r.3, 1, FOSFORO_A);
            d.text(r.0 + (r.2 as i32 - 9) / 2, r.1 + 18, 1, FOSFORO, g);
        }
    }
}

fn render(d: &mut Screen, st: &AlbumState, lay: &Layout, failed: &HashSet<u64>) {
    draw_page(d);
    let vis_len = st.visible().len();
    if st.prints.is_empty() {
        d.text(
            MARGIN,
            MARGIN - 12,
            2,
            DIM,
            tr!("print album", "album de prints"),
        );
        draw_empty(d);
    } else if st.viewing {
        draw_view(d, st, failed);
    } else {
        draw_header(d, st, vis_len);
        draw_grid(d, st, lay, failed);
    }
    draw_legend(d, st.viewing, st.prints.is_empty());
}

// ---------------------------------------------------------------------
// Loop interativo e preview
// ---------------------------------------------------------------------

fn empty_panel() -> sixteen_platform::ShelfPanelInfo {
    sixteen_platform::ShelfPanelInfo {
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

/// Pede ao carregador o que a tela mostra agora: a foto sob o cursor
/// primeiro, depois as visíveis (grade) ou as vizinhas (foto grande).
fn request_visible(st: &AlbumState, lay: &Layout, loader: &mut Loader, cab: &Cabinet) {
    let vis = st.visible();
    let mut want: Vec<usize> = Vec::new();
    if let Some(&c) = vis.get(st.cursor) {
        want.push(c);
    }
    if st.viewing {
        for off in [1i32, -1] {
            let n = (st.cursor as i32 + off).rem_euclid(vis.len().max(1) as i32) as usize;
            if let Some(&i) = vis.get(n) {
                want.push(i);
            }
        }
    } else {
        want.extend(
            vis.iter()
                .skip(st.top_row * lay.cols)
                .take(lay.cols * lay.vis_rows)
                .copied(),
        );
    }
    for i in want {
        loader.request(cab, i, &st.prints[i]);
    }
}

/// O álbum interativo. `Pick::Back` = volta à estante.
pub fn run(plat: &mut Platform, cab: &mut Cabinet, notes_dir: &Path) -> Result<Pick> {
    let mut st = AlbumState::new(list_prints(notes_dir));
    let mut loader = Loader::new();
    cab.set_shelf_panel(empty_panel());
    cab.set_close_button(true);
    let frame = Duration::from_millis(16);

    loop {
        let (w, h) = cab.shelf_screen_size();
        let lay = layout(w, h);

        let m = plat.poll_menu(MenuMode::Nav);
        if m.quit {
            return Ok(Pick::Quit);
        }
        let cols = lay.cols;
        for nav in &m.nav {
            if st.viewing {
                match nav {
                    MenuNav::Left | MenuNav::PageUp => st.step_view(-1),
                    MenuNav::Right | MenuNav::PageDown => st.step_view(1),
                    MenuNav::Back => st.viewing = false,
                    _ => {}
                }
                continue;
            }
            match nav {
                MenuNav::Back => return Ok(Pick::Back),
                MenuNav::Left => st.move_cursor(-1, 0, cols),
                MenuNav::Right => st.move_cursor(1, 0, cols),
                MenuNav::Up => st.move_cursor(0, -1, cols),
                MenuNav::Down => st.move_cursor(0, 1, cols),
                MenuNav::Home => st.home_end(false),
                MenuNav::End => st.home_end(true),
                MenuNav::PageUp => st.cycle_filter(-1),
                MenuNav::PageDown => st.cycle_filter(1),
                MenuNav::Confirm => st.viewing = st.current().is_some(),
                // Y também troca o filtro (como LB/RB); Start abre as
                // configurações — o mesmo atalho da estante.
                MenuNav::Extra => st.cycle_filter(1),
                MenuNav::Start => return Ok(Pick::Settings),
                _ => {}
            }
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
                    ShelfButton::Back => {
                        if st.viewing {
                            st.viewing = false;
                        } else {
                            return Ok(Pick::Back);
                        }
                    }
                    ShelfButton::Settings => return Ok(Pick::Settings),
                    _ => {}
                }
            } else if let Some((lx, ly)) = cab.hit_screen_point(ox, oy) {
                click(&mut st, &lay, (w, h), lx, ly);
            }
        }

        st.scroll_to_cursor(lay.cols, lay.vis_rows);
        request_visible(&st, &lay, &mut loader, cab);
        loader.drain(cab);

        let failed = &loader.failed;
        cab.frame_shelf(bg(), |d: &mut Screen| render(d, &st, &lay, failed));
        std::thread::sleep(frame);
    }
}

fn bg() -> (u8, u8, u8) {
    LEATHER
}

/// Um clique (coordenadas da tela) na grade ou na foto grande.
fn click(st: &mut AlbumState, lay: &Layout, (w, h): (u32, u32), x: i32, y: i32) {
    if st.prints.is_empty() {
        return;
    }
    if st.viewing {
        let (left, right, area) = view_rects(w, h);
        if in_rect(x, y, left) {
            st.step_view(-1);
        } else if in_rect(x, y, right) {
            st.step_view(1);
        } else if !in_rect(x, y, area) {
            st.viewing = false;
        }
        return;
    }
    let (prev, next) = filter_rects(w);
    if in_rect(x, y, prev) {
        st.cycle_filter(-1);
    } else if in_rect(x, y, next) {
        st.cycle_filter(1);
    } else if let Some(i) = lay.cell_at(x, y, st.top_row, st.visible().len()) {
        st.cursor = i;
        st.viewing = true;
    }
}

/// Preview headless (dev/teste): uma tela do álbum pelo tubo. `view` abre a
/// foto grande no primeiro print; `filter_game` aplica o primeiro filtro de
/// jogo; `cursor` posiciona o cursor.
pub fn capture_album_preview(
    cab: &mut Cabinet,
    notes_dir: &Path,
    path: &Path,
    view: bool,
    filter_game: bool,
    cursor: usize,
) -> Result<()> {
    let mut st = AlbumState::new(list_prints(notes_dir));
    if filter_game {
        st.cycle_filter(1);
    }
    let vis = st.visible();
    st.cursor = cursor.min(vis.len().saturating_sub(1));
    st.viewing = view && !vis.is_empty();
    cab.set_shelf_panel(empty_panel());
    cab.set_close_button(true);
    let (w, h) = cab.shelf_screen_size();
    let lay = layout(w, h);
    st.scroll_to_cursor(lay.cols, lay.vis_rows);
    // Sem thread: decodifica o que a tela mostra, na hora.
    let mut loader = Loader::new();
    request_visible(&st, &lay, &mut loader, cab);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        loader.drain(cab);
        let needed = loader.asked.len();
        let have = loader.asked.iter().filter(|id| cab.has_image(**id)).count();
        if have + loader.failed.len() >= needed {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let failed = loader.failed.clone();
    cab.capture_shelf(bg(), |d: &mut Screen| render(d, &st, &lay, &failed), path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(game: &str, slot: u8, mtime: u64) -> Print {
        Print {
            game: game.into(),
            slot,
            path: PathBuf::from(format!("{game}/{slot:02}.png")),
            caption: String::new(),
            mtime,
        }
    }

    fn sample() -> AlbumState {
        AlbumState::new(vec![
            pr("Zelda", 1, 50),
            pr("Mario", 2, 40),
            pr("Zelda", 2, 30),
            pr("Mario", 1, 20),
            pr("Metroid", 1, 10),
        ])
    }

    #[test]
    fn slot_file_names_parse_strictly() {
        assert_eq!(parse_slot_file("01.png"), Some(1));
        assert_eq!(parse_slot_file("15.png"), Some(15));
        assert_eq!(parse_slot_file("00.png"), None);
        assert_eq!(parse_slot_file("16.png"), None);
        assert_eq!(parse_slot_file("1.png"), None);
        assert_eq!(parse_slot_file("01.txt"), None);
        assert_eq!(parse_slot_file("slots.json"), None);
    }

    #[test]
    fn captions_are_read_and_bad_json_is_tolerated() {
        let c = parse_captions(
            r#"{"slots":{"3":{"pinned":true,"label":" chefe final "},"4":{"label":""}},"text_slots":{"1":{"label":"x"}}}"#,
        );
        assert_eq!(c.len(), 1);
        assert_eq!(c[&3], "chefe final");
        assert!(parse_captions("not json").is_empty());
    }

    #[test]
    fn games_are_unique_and_sorted() {
        assert_eq!(games_of(&sample().prints), ["Mario", "Metroid", "Zelda"]);
    }

    #[test]
    fn filter_cycles_through_all_and_each_game() {
        let mut s = sample();
        assert_eq!(s.visible().len(), 5);
        s.cursor = 3;
        s.cycle_filter(1);
        assert_eq!(s.filter, Some(0));
        assert_eq!(s.cursor, 0);
        assert_eq!(s.visible(), [1, 3]); // Mario
        s.cycle_filter(1);
        s.cycle_filter(1);
        assert_eq!(s.visible(), [0, 2]); // Zelda
        s.cycle_filter(1);
        assert_eq!(s.filter, None);
        s.cycle_filter(-1);
        assert_eq!(s.filter, Some(2));
    }

    #[test]
    fn cursor_moves_and_clamps() {
        let mut s = sample();
        s.move_cursor(-1, 0, 3);
        assert_eq!(s.cursor, 0);
        s.move_cursor(1, 0, 3);
        assert_eq!(s.cursor, 1);
        s.move_cursor(0, 1, 3); // 1 + 3 = 4
        assert_eq!(s.cursor, 4);
        s.move_cursor(0, 1, 3); // past the end: last print
        assert_eq!(s.cursor, 4);
        s.move_cursor(0, -1, 3);
        assert_eq!(s.cursor, 1);
        s.move_cursor(0, -1, 3);
        assert_eq!(s.cursor, 0);
        s.home_end(true);
        assert_eq!(s.cursor, 4);
        s.home_end(false);
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn two_row_jumps_and_scrolling_follow_the_cursor() {
        let mut s = AlbumState::new((0..20).map(|i| pr("G", 1, i)).collect());
        s.move_cursor(0, 2, 4); // two rows of four
        assert_eq!(s.cursor, 8);
        s.scroll_to_cursor(4, 2);
        assert_eq!(s.top_row, 1); // row 2 must be visible
        s.move_cursor(0, -2, 4);
        s.scroll_to_cursor(4, 2);
        assert_eq!((s.cursor, s.top_row), (0, 0));
    }

    #[test]
    fn big_view_steps_and_wraps() {
        let mut s = sample();
        s.step_view(-1);
        assert_eq!(s.cursor, 4);
        s.step_view(1);
        assert_eq!(s.cursor, 0);
        let mut empty = AlbumState::new(vec![]);
        empty.step_view(1);
        empty.move_cursor(1, 1, 3);
        assert_eq!(empty.cursor, 0);
        assert_eq!(empty.current(), None);
    }

    #[test]
    fn layout_fits_and_hit_tests_cells() {
        let lay = layout(560, 420);
        assert!(lay.cols >= 2 && lay.vis_rows >= 1);
        let (px, py, pw, ph) = lay.polaroid(0, 1);
        assert_eq!(lay.cell_at(px + 3, py + 3, 0, 10), Some(1));
        assert_eq!(
            lay.cell_at(px + pw as i32 / 2, py + ph as i32 / 2, 2, 20),
            Some(2 * lay.cols + 1)
        );
        // Beyond the last print, or above the grid.
        assert_eq!(lay.cell_at(px + 3, py + 3, 0, 1), None);
        assert_eq!(lay.cell_at(px, 0, 0, 10), None);
        // Tiny and huge screens never produce zero columns/rows.
        assert!(layout(100, 100).vis_rows >= 1);
        assert!(layout(3000, 2000).cols <= 6);
    }

    #[test]
    fn clicks_open_close_and_step() {
        let mut s = sample();
        let (w, h) = (560, 420);
        let lay = layout(w, h);
        let (px, py, _, _) = lay.polaroid(0, 1);
        click(&mut s, &lay, (w, h), px + 5, py + 5);
        assert!(s.viewing);
        assert_eq!(s.cursor, 1);
        let (left, right, _) = view_rects(w, h);
        click(&mut s, &lay, (w, h), right.0 + 2, right.1 + 2);
        assert_eq!(s.cursor, 2);
        click(&mut s, &lay, (w, h), left.0 + 2, left.1 + 2);
        assert_eq!(s.cursor, 1);
        click(&mut s, &lay, (w, h), 1, h as i32 - 1); // outside the photo
        assert!(!s.viewing);
        // The filter chip's arrows.
        let (_, next) = filter_rects(w);
        click(&mut s, &lay, (w, h), next.0 + 2, next.1 + 2);
        assert_eq!(s.filter, Some(0));
    }

    #[test]
    fn dates_and_text_fit() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(1_000_000_000), (2001, 9, 9));
        assert_eq!(civil_date(1_709_164_800), (2024, 2, 29));
        assert_eq!(fit_chars("abcdef", 6), "abcdef");
        assert_eq!(fit_chars("abcdefg", 6), "abcd..");
        assert_eq!(fit_chars("ação grande", 5), "açã..");
    }

    #[test]
    fn scan_reads_games_slots_captions_newest_first() {
        let dir = std::env::temp_dir().join(format!("album-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Mario")).unwrap();
        std::fs::create_dir_all(dir.join("Zelda")).unwrap();
        std::fs::write(dir.join("Mario/03.png"), b"x").unwrap();
        std::fs::write(dir.join("Mario/notas.txt"), b"x").unwrap();
        std::fs::write(
            dir.join("Mario/slots.json"),
            r#"{"slots":{"3":{"label":"castelo"}}}"#,
        )
        .unwrap();
        std::fs::write(dir.join("Zelda/01.png"), b"x").unwrap();
        std::fs::write(dir.join("loose.png"), b"x").unwrap();
        let got = list_prints(&dir);
        assert_eq!(got.len(), 2);
        let mario = got.iter().find(|p| p.game == "Mario").unwrap();
        assert_eq!((mario.slot, mario.caption.as_str()), (3, "castelo"));
        assert!(got
            .iter()
            .find(|p| p.game == "Zelda")
            .unwrap()
            .caption
            .is_empty());
        assert!(list_prints(&dir.join("nope")).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
