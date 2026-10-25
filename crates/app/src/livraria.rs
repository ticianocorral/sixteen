//! A Livraria Retro (plan revision: "adicionar um novo botão para uma nova
//! funcionalidade - Livraria Retro - pode baixar revistas antigas em pdf e
//! criar um revistoteca para leitura - coloca o pdf em uma pasta (magazine)
//! e mostra um livreiro na tv"). Os PDFs moram em `magazine/` na raiz do app
//! ([`crate::dirs::magazine_dir`]): chegam por download (URL colada na tela)
//! ou largados à mão na pasta — as duas portas abertas. A TV mostra o
//! livreiro: prateleiras com a capa de cada revista, onde a capa é a
//! PÁGINA 1 do próprio PDF (scan de revista = uma imagem grande por página,
//! exatamente o que [`crate::manual`] já desenha para os manuais); o leitor
//! folheia as páginas dentro do tubo (‹/›, PgUp/PgDn e cliques nos botões).

use std::collections::HashSet;
use std::io::Read as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use anyhow::Result;
use sixteen_platform::tr;
use sixteen_platform::{Cabinet, MenuMode, MenuNav, PanelButton, Platform, Screen, TvControl};

use crate::dirs::magazine_dir;
use crate::dirs::museu_dir;
use crate::runner::pace_frame;

const BG: (u8, u8, u8) = (18, 18, 20);
const TEXT: (u8, u8, u8) = (232, 232, 232);
const DIM: (u8, u8, u8) = (150, 150, 158);
const GREEN: (u8, u8, u8) = (64, 184, 92);
const RED: (u8, u8, u8) = (224, 84, 74);
const TILE_BG: (u8, u8, u8, u8) = (34, 34, 40, 255);
/// A madeira das prateleiras — face frontal, o lip iluminado e a sombra por
/// baixo, como um móvel de verdade visto de frente.
const PLANK: (u8, u8, u8, u8) = (86, 58, 34, 255);
const PLANK_LIT: (u8, u8, u8, u8) = (128, 88, 52, 255);
const PLANK_EDGE: (u8, u8, u8, u8) = (44, 28, 16, 255);

const MARGIN: i32 = 28;
/// Célula do livreiro (capa em pé na prateleira) e o vão entre vizinhas.
const TILE_W: u32 = 190;
const GAP: u32 = 16;
/// Prateleiras visíveis de uma vez — a rolagem anda em páginas de prateleira.
const ROWS: usize = 2;
/// Espaço reservado para o rodapé (botões + status), abaixo das prateleiras.
const FOOT_H: i32 = 240;
/// O piscar do caret do campo de URL, em frames.
const CARET_FRAMES: u32 = 30;

/// `(x, y, w, h)` dentro do buffer do tubo — o mesmo espaço de
/// `window_to_screen`; o draw e o hit-test partem das mesmas fórmulas.
type ScrRect = (i32, i32, u32, u32);

/// Qual acervo a Livraria abre — o loop é o mesmo, muda a pasta (e o
/// rótulo do rodapé de download).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LivKind {
    /// As revistas da época (pasta `magazine`).
    Revistas,
    /// A documentação do console: manual, poster, garantia etc. (pasta
    /// `museu`) — o lado museu do app.
    Museu,
}

impl LivKind {
    fn dir(self) -> PathBuf {
        match self {
            LivKind::Revistas => magazine_dir(),
            LivKind::Museu => museu_dir(),
        }
    }

    /// O título do acervo, no cabeçalho da estante.
    fn title(self) -> &'static str {
        match self {
            LivKind::Revistas => tr!("RETRO LIBRARY", "LIVRARIA RETRO"),
            LivKind::Museu => tr!("CONSOLE MUSEUM", "MUSEU DO CONSOLE"),
        }
    }

    /// A palavra do acervo no plural ("5 revistas" / "3 documentos").
    fn item_word(self) -> &'static str {
        match self {
            LivKind::Revistas => tr!("magazines", "revistas"),
            LivKind::Museu => tr!("documents", "documentos"),
        }
    }

    /// A primeira linha da gaveta vazia.
    fn empty_intro(self) -> &'static str {
        match self {
            LivKind::Revistas => {
                tr!(
                    "Old PDF magazines get onto the shelf in two ways:",
                    "Revistas antigas em PDF entram na prateleira de duas maneiras:"
                )
            }
            LivKind::Museu => {
                tr!(
                    "The documentation PDFs (manual, poster, warranty etc.) get into the museum in two ways:",
                    "Os PDFs da documentação (manual, poster, garantia etc.) entram no museu de duas maneiras:"
                )
            }
        }
    }

    /// A dica do rodapé da estante. Os nomes entre parênteses são as pastas
    /// no disco (`dirs::magazine_dir`/`museu_dir`) — em inglês sempre, em
    /// qualquer idioma.
    fn footer_hint(self) -> &'static str {
        match self {
            LivKind::Revistas => {
                tr!(
                    "magazines via download or dropped into the app folder (docs/magazines)",
                    "revistas por download ou largadas na pasta do app (docs/magazines)"
                )
            }
            LivKind::Museu => {
                tr!(
                    "documents via download or dropped into the app folder (docs/console-docs)",
                    "documentos por download ou largados na pasta do app (docs/console-docs)"
                )
            }
        }
    }

    /// O nome do item no singular ("abrindo a revista…" / "abrindo o
    /// documento…", "revista adicionada!" / "documento adicionado!").
    fn item_singular(self) -> &'static str {
        match self {
            LivKind::Revistas => tr!("magazine", "revista"),
            LivKind::Museu => tr!("document", "documento"),
        }
    }
}

pub enum LivPick {
    /// Window closed / Cmd-Q — tear the app down.
    Quit,
    /// "Voltar" (clique, Esc ou Back do pad) — de volta à tela inicial.
    Back,
}

/// Uma revista da pasta `magazine/` — o título vem do nome do arquivo e o
/// caminho alimenta a capa (página 1) e as páginas do leitor.
struct Book {
    path: PathBuf,
    title: String,
}

/// Evento do worker do LEITOR (um por revista aberta).
enum PageEv {
    Ready {
        pages: usize,
    },
    Fail(String),
    Page {
        page: usize,
        w: u32,
        h: u32,
        rgba: Vec<u8>,
    },
}

/// O leitor, enquanto uma revista está aberta: a página só pede render ao
/// worker uma vez (o `requested` impede pedidos repetidos); a textura fica
/// viva pelo id.
struct Read {
    path: PathBuf,
    title: String,
    page: usize,
    pages: usize,
    ready: bool,
    requested: HashSet<usize>,
    /// Páginas que o worker devolveu `w == 0` — sem imagem embutida no
    /// scan (capa de propaganda, página de anúncios). O leitor mostra
    /// "sem imagem" em vez de "renderizando…" para sempre.
    empty: HashSet<usize>,
    req_tx: Sender<usize>,
    ev_rx: Receiver<PageEv>,
    /// MODO ZOOM do leitor (clique na página): a página estica para caber
    /// na ALTURA da área toda e ‹/› arrastam a leitura (pan 0..1).
    zoom: bool,
    pan: f32,
}

/// Evento do worker de DOWNLOAD (um por URL colado).
enum DlEv {
    Progress { bytes: u64, total: Option<u64> },
    Done(Result<u64, String>),
}

// --- chaves de textura ------------------------------------------------------
// As famílias de id do app: as artes do jogo não têm bit alto nenhum, a
// estante usa o bit 62 (páginas de manual + badges). A livraria pega o bit
// 61 — capas e páginas nunca colidem com o resto.

fn hash64(salt: &str) -> u64 {
    salt.bytes().fold(0x9E37_79B9_7F4A_7C15u64, |mut acc, b| {
        acc ^= b as u64;
        acc = acc.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
        acc ^= acc >> 33;
        acc
    })
}

fn cover_id(path: &Path) -> u64 {
    hash64(&format!("{}:capa", path.display())) | (1 << 61)
}

fn page_id(path: &Path, page: usize) -> u64 {
    hash64(&format!("{}:pg{page}", path.display())) | (1 << 61)
}

// --- a pasta e seus nomes ---------------------------------------------------

/// Escaneia `magazine/` por PDFs, ordenados por título. A pasta nasce sob
/// demanda (como as irmãs de `dirs.rs`) — abrir a Livraria sem ela mostra
/// "estante vazia", com instruções.
fn scan_books(kind: LivKind) -> Vec<Book> {
    let dir = kind.dir();
    let mut books = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path
                .extension()
                .and_then(|e| e.to_str())
                .is_none_or(|e| !e.eq_ignore_ascii_case("pdf"))
            {
                continue;
            }
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(tr!("magazine", "revista"))
                .to_string();
            books.push(Book {
                title: pretty_title(&stem),
                path,
            });
        }
    }
    books.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
    books
}

/// "Super_Game_World-1994" → "Super Game World-1994" — os underscores viram
/// espaço; o resto do nome fica como o usuário guardou (é o título real).
fn pretty_title(stem: &str) -> String {
    let mut out = String::with_capacity(stem.len());
    for ch in stem.chars() {
        if ch == '_' {
            out.push(' ');
        } else {
            out.push(ch);
        }
    }
    out.trim().to_string()
}

/// O nome do arquivo no destino: o último segmento do URL (sem query),
/// saneado — e sempre `.pdf`, porque alguém colou `…/revista?id=x`.
/// `None` quando o URL não tem segmento nenhum.
fn url_filename(url: &str) -> Option<String> {
    let bare = url.split(['?', '#']).next()?;
    // Sem esquema (`http://…`)? Nem é URL — o chamador reclama.
    if !bare.contains("://") {
        return None;
    }
    // Terminou na barra? Não há arquivo nenhum no URL — o chamador reclama.
    if bare.ends_with('/') {
        return None;
    }
    let name = bare.rsplit('/').find(|s| !s.is_empty())?;
    let mut name = name.replace("%20", " ").replace(['\\', ':'], "-");
    while name.starts_with('.') {
        name.remove(0);
    }
    if name.trim().is_empty() {
        return None;
    }
    if name
        .rsplit('.')
        .next()
        .is_none_or(|ext| !ext.eq_ignore_ascii_case("pdf"))
    {
        name.push_str(".pdf");
    }
    Some(name)
}

/// O nome no destino, sem colisão — `revista.pdf` atrás de `revista (2).pdf`.
fn free_dest(dir: &Path, name: &str) -> PathBuf {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("revista")
        .to_string();
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_else(|| ".pdf".to_string());
    let mut n = 1u32;
    loop {
        let candidate = if n == 1 {
            dir.join(format!("{stem}{ext}"))
        } else {
            dir.join(format!("{stem} ({n}){ext}"))
        };
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Bytes humanos — "3,2 MB" — para a linha de status do download.
fn fmt_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if n >= MB {
        format!("{:.1} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.0} KB", n as f64 / KB as f64)
    } else {
        format!("{n} B")
    }
}

/// O caminho do `.part` — o mesmo cálculo do worker; `download_pdf`
/// ([abaixo]) escreve nele e renomeia só quando o PDF está inteiro.
fn part_path(dest: &Path) -> PathBuf {
    dest.with_extension(format!(
        "{}part",
        dest.extension()
            .and_then(|e| e.to_str().map(|s| format!("{s}.")))
            .unwrap_or_default()
    ))
}

/// O worker de download: `GET`, o `%PDF-` de cabeça confere (não-PDF é
/// erro e não fica), escreve num `.part` e renomeia no fim — a estante
/// jamais lista um PDF quebrado. Progresso vai no canal em passos
/// discretos (1% quando o servidor dá Content-Length, senão a cada 256 KB).
fn download_pdf(url: String, dest: PathBuf, ev_tx: Sender<DlEv>) {
    let result = (|| -> Result<u64, String> {
        // http:// só para servidor local: o PDF é baixado e depois parseado
        // pelo app — tráfego sem TLS numa rede hostil seria a via de
        // injeção mais fácil. localhost é o caso legítimo (servidor na
        // própria máquina, ex. calibre-web local) e segue permitido.
        let scheme_ok = url.starts_with("https://")
            || url.starts_with("http://localhost")
            || url.starts_with("http://127.0.0.1")
            || url.starts_with("http://[::1]");
        if !scheme_ok {
            return Err(tr!(
                "use an https:// URL (http only for localhost)",
                "use um URL https:// (http só para localhost)"
            )
            .to_string());
        }
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(15))
            .timeout(Duration::from_secs(300))
            .user_agent("Mozilla/5.0 (compatible; SixteeN Livraria)")
            .build();
        let resp = agent.get(&url).call().map_err(|e| {
            tr!(
                format!("the URL didn't respond: {e}"),
                format!("o URL não respondeu: {e}")
            )
        })?;
        let total = resp
            .header("Content-Length")
            .and_then(|s| s.parse::<u64>().ok());
        let mut reader = resp.into_reader();

        let part = part_path(&dest);
        let mut file = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 64 * 1024];
        // Revistas escaneadas chegam às dezenas de MB; 256 MB é teto
        // generoso que ainda corta um servidor ruim enchendo o disco.
        const MAX_PDF_DOWNLOAD: u64 = 256 * 1024 * 1024;
        let mut bytes = 0u64;
        let mut head = Vec::new();
        let mut last_pct = 0u32;
        loop {
            let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            if head.len() < 5 {
                head.extend_from_slice(&buf[..n.min(5 - head.len())]);
            }
            file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            bytes += n as u64;
            if bytes > MAX_PDF_DOWNLOAD {
                let _ = std::fs::remove_file(&part);
                return Err(tr!(
                    "PDF larger than 256 MB — aborted",
                    "PDF maior que 256 MB — abortado"
                )
                .to_string());
            }
            if total.is_some() {
                let pct = (bytes * 100 / total.unwrap_or(1)).min(100) as u32;
                if pct > last_pct {
                    last_pct = pct;
                    let _ = ev_tx.send(DlEv::Progress { bytes, total });
                }
            } else if bytes / (256 * 1024) != (bytes - n as u64) / (256 * 1024) {
                let _ = ev_tx.send(DlEv::Progress { bytes, total });
            }
        }
        if head.len() >= 5 && &head[..5] != b"%PDF-" {
            let _ = std::fs::remove_file(&part);
            return Err(tr!(
                "the downloaded file isn't a PDF",
                "o arquivo baixado não é um PDF"
            )
            .to_string());
        }
        std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
        Ok(bytes)
    })();

    // O arquivo de mentirinha não fica para trás — nem o `.part` parcial.
    if result.is_err() {
        let _ = std::fs::remove_file(&part_path(&dest));
    }
    let _ = ev_tx.send(DlEv::Done(result));
}

// --- a geometria (fonte única para draw E hit-test) -------------------------

fn inner_w(scr_w: u32) -> u32 {
    scr_w.saturating_sub(2 * MARGIN as u32)
}

/// Quantas revistas por prateleira — o tamanho do buffer manda.
fn per_row(scr_w: u32) -> usize {
    (inner_w(scr_w) / (TILE_W + GAP)).max(2) as usize
}

fn rack_top(_scr_h: u32) -> i32 {
    MARGIN + 66
}

/// Onde a área livre do rodapé começa (abaixo das duas prateleiras).
fn foot_top(scr_h: u32) -> i32 {
    scr_h as i32 - FOOT_H
}

/// O retângulo (no buffer do tubo) da capa `i` dado o `top` corrente — a
/// MESMA fórmula do desenho usa, então clique e visual nunca divergem.
/// `None` = o índice não está visível agora.
fn tile_rect(scr: (u32, u32), top: usize, i: usize) -> Option<ScrRect> {
    let (w, h) = scr;
    let per = per_row(w);
    let rel = i.checked_sub(top)?;
    if rel >= per * ROWS {
        return None;
    }
    let row = rel / per;
    let col = rel % per;
    let rows_h = foot_top(h) - rack_top(h);
    let row_h = rows_h / (ROWS as i32).max(1);
    let box_h = (row_h - 24).max(100) as u32;
    let x = MARGIN + (col as i32) * (TILE_W + GAP) as i32;
    let plank_y = rack_top(h) + (row as i32 + 1) * row_h - 26;
    Some((x, plank_y - box_h as i32, TILE_W, box_h))
}

/// Os dois botões do rodapé da estante: BAIXAR (à esquerda) e VOLTAR.
fn foot_rects(scr: (u32, u32)) -> (ScrRect, ScrRect) {
    let (w, h) = scr;
    let y = h as i32 - 96;
    if w < 700 {
        return ((MARGIN, y, 170, 40), (MARGIN + 190, y, 120, 40));
    }
    let url = (MARGIN, y, 260u32, 44u32);
    let voltar = (MARGIN + 280, y, 150u32, 44u32);
    (url, voltar)
}

/// Os três botões do leitor: VOLTAR à esquerda e ‹ / › centrados.
fn reader_rects(scr: (u32, u32)) -> (ScrRect, ScrRect, ScrRect) {
    let (w, h) = scr;
    let y = h as i32 - 96;
    let cx = w as i32 / 2;
    let voltar = (MARGIN, y, 150, 44);
    let prev = (cx - 250, y, 150, 44);
    let next = (cx + 100, y, 150, 44);
    (voltar, prev, next)
}

fn in_rect(x: i32, y: i32, r: ScrRect) -> bool {
    x >= r.0 && y >= r.1 && (x - r.0) < r.2 as i32 && (y - r.1) < r.3 as i32
}

// --- o loop da tela ---------------------------------------------------------

/// Abre a Livraria (o botão "LIVRARIA RETRO" da tela inicial) — devolve
/// `Quit` (janela fechada) ou `Back` (voltou ao idle).
pub fn run(plat: &mut Platform, cab: &mut Cabinet, kind: LivKind) -> Result<LivPick> {
    let frame = Duration::from_millis(16);
    let mut next = Instant::now() + frame;
    let mut frame_no = 0u64;

    cab.set_close_button(true);

    let mut books = scan_books(kind);
    let mut sel = 0usize;
    let mut top = 0usize;
    let mut on_url = false;

    // O worker das capas (página 1 do PDF, pequena): um pedido atravessa o
    // canal por vez da fila; quem acende é o drain do frame.
    let (cov_req_tx, cov_req_rx) = mpsc::channel::<PathBuf>();
    let (cov_ev_tx, cov_ev_rx) = mpsc::channel::<(PathBuf, u32, u32, Vec<u8>)>();
    std::thread::spawn(move || {
        for path in cov_req_rx {
            match crate::manual::render_page(&path, 1, 512) {
                Ok(Some(img)) => {
                    let _ = cov_ev_tx.send((path, img.w, img.h, img.rgba));
                }
                _ => {
                    let _ = cov_ev_tx.send((path, 0, 0, Vec::new()));
                }
            }
        }
    });
    let mut cov_requested: HashSet<PathBuf> = HashSet::new();
    let mut cov_done: HashSet<PathBuf> = HashSet::new();
    let mut cov_failed: HashSet<PathBuf> = HashSet::new();

    // O leitor (nasce por revista) e o download em andamento.
    let mut read: Option<Read> = None;
    let mut read_err = None::<String>;
    let mut dl_rx = None::<Receiver<DlEv>>;
    let mut dl_label = String::new();
    let mut dl_err = None::<String>;
    let mut dl_ok_until = None::<Instant>;

    // O editor de URL (o mesmo esquema do filtro da estante: o editor é o
    // dono do teclado enquanto está vivo).
    let mut editing_url = false;
    let mut url_draft = String::new();

    // Rescan da pasta de meio em meio segundo — largar um PDF à mão entra
    // sem fechar a tela.
    let mut last_scan = Instant::now();

    loop {
        frame_no += 1;

        // As capas: pedidos (2 por frame, a fila anda) e o drain dos que
        // chegam — textura pelo id por caminho.
        let mut sent = 0;
        for book in &books {
            if sent >= 2 {
                break;
            }
            let p = &book.path;
            if cov_done.contains(p) || cov_failed.contains(p) || cov_requested.contains(p) {
                continue;
            }
            cov_requested.insert(p.clone());
            let _ = cov_req_tx.send(p.clone());
            sent += 1;
        }
        while let Ok((path, w, h, rgba)) = cov_ev_rx.try_recv() {
            if w > 0 {
                cab.set_image(cover_id(&path), w, h, &rgba);
                cov_done.insert(path);
            } else {
                cov_failed.insert(path);
            }
        }

        // O download: progresso e fim — no fim, a rescan coloca a revista
        // nova na prateleira.
        if let Some(rx) = &dl_rx {
            loop {
                match rx.try_recv() {
                    Ok(DlEv::Progress { bytes, total }) => {
                        dl_label = match total {
                            Some(t) => tr!(
                                format!(
                                    "downloading… {}% ({} of {})",
                                    bytes * 100 / t.max(1),
                                    fmt_bytes(bytes),
                                    fmt_bytes(t)
                                ),
                                format!(
                                    "baixando… {}% ({} de {})",
                                    bytes * 100 / t.max(1),
                                    fmt_bytes(bytes),
                                    fmt_bytes(t)
                                )
                            ),
                            None => tr!(
                                format!("downloading… {}", fmt_bytes(bytes)),
                                format!("baixando… {}", fmt_bytes(bytes))
                            ),
                        };
                    }
                    Ok(DlEv::Done(Ok(bytes))) => {
                        dl_label = tr!(
                            format!("{} added ({})!", kind.item_singular(), fmt_bytes(bytes)),
                            match kind {
                                LivKind::Revistas =>
                                    format!("revista adicionada ({})!", fmt_bytes(bytes)),
                                LivKind::Museu =>
                                    format!("documento adicionado ({})!", fmt_bytes(bytes)),
                            }
                        );
                        dl_err = None;
                        dl_ok_until = Some(Instant::now() + Duration::from_secs(5));
                        dl_rx = None;
                        books = scan_books(kind);
                        sel = sel.min(books.len().saturating_sub(1));
                        break;
                    }
                    Ok(DlEv::Done(Err(e))) => {
                        dl_label.clear();
                        dl_err = Some(e);
                        dl_rx = None;
                        break;
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        dl_rx = None;
                        break;
                    }
                }
            }
        }
        if let Some(until) = dl_ok_until {
            if Instant::now() > until {
                dl_ok_until = None;
                dl_label.clear();
            }
        }

        // O leitor: contagem, páginas que viram textura, pedidos em falta.
        if let Some(r) = read.as_mut() {
            loop {
                match r.ev_rx.try_recv() {
                    Ok(PageEv::Ready { pages }) => {
                        r.pages = pages;
                        r.ready = true;
                    }
                    Ok(PageEv::Fail(e)) => {
                        read_err = Some(e);
                        r.ready = true;
                    }
                    Ok(PageEv::Page { page, w, h, rgba }) => {
                        if w > 0 {
                            cab.set_image(page_id(&r.path, page), w, h, &rgba);
                        } else {
                            r.empty.insert(page);
                        }
                        r.requested.insert(page);
                    }
                    Err(_) => break,
                }
            }
            if r.ready && !read_err.is_some() && r.pages > 0 && !r.requested.contains(&r.page) {
                r.requested.insert(r.page);
                let _ = r.req_tx.send(r.page);
            }
        }

        // A pasta mudou? (Largar arquivo à mão entre frames.)
        if last_scan.elapsed() > Duration::from_millis(500) {
            last_scan = Instant::now();
            let fresh = scan_books(kind);
            if fresh.len() != books.len()
                || books
                    .iter()
                    .zip(fresh.iter())
                    .any(|(a, b)| a.path != b.path)
            {
                books = fresh;
                sel = sel.min(books.len().saturating_sub(1));
                top = 0;
            }
        }
        // Encore dos limites de navegação (a rescan pode ter encolhido tudo).
        if sel >= books.len() {
            sel = books.len().saturating_sub(1);
        }
        let visible = per_row(cab.shelf_screen_size().0) * ROWS;
        if top > 0 && top + visible > books.len() {
            top = books.len().saturating_sub(visible.max(1) as usize);
        }

        // ---------- input ----------
        // TV desligada: os workers seguem (as capas/páginas carregam por
        // baixo) e o vidro morto cobre a tela; input nenhum além do queixo
        // — o painel é plástico morto, como no idle.
        // ---------- input ----------
        if editing_url {
            let te = plat.poll_text_entry();
            if te.quit {
                return Ok(LivPick::Quit);
            }
            if te.backspace {
                url_draft.pop();
            }
            for c in te.typed.chars() {
                if url_draft.len() < 512 {
                    url_draft.push(c);
                }
            }
            if let Some(paste) = &te.paste {
                for c in paste.chars() {
                    if url_draft.len() < 512 {
                        url_draft.push(c);
                    }
                }
            }
            if te.copy {
                plat.set_clipboard_text(&url_draft);
            }
            if te.commit {
                editing_url = false;
                plat.stop_text_input(cab);
                let url = url_draft.trim().to_string();
                if !url.is_empty() && dl_rx.is_none() {
                    match url_filename(&url) {
                        Some(name) => {
                            let dest = free_dest(&kind.dir(), &name);
                            let (tx, rx) = mpsc::channel::<DlEv>();
                            std::thread::spawn(move || download_pdf(url, dest, tx));
                            dl_rx = Some(rx);
                            dl_label = tr!("downloading…", "baixando…").to_string();
                            dl_err = None;
                        }
                        None => {
                            dl_err = Some(
                                tr!(
                                    "this URL has no file name",
                                    "esse URL não vem com nome de arquivo"
                                )
                                .to_string(),
                            );
                        }
                    }
                }
            } else if te.cancel {
                editing_url = false;
                plat.stop_text_input(cab);
            }
        } else {
            let m = plat.poll_menu(MenuMode::Nav);
            if m.quit {
                return Ok(LivPick::Quit);
            }
            if let Some(r) = read.as_mut() {
                // O leitor é dono do tubo — ‹/› folheiam, voltar fecha.
                let mut close = false;
                for nav in m.nav.iter() {
                    match nav {
                        // No zoom, a rodinha (Up/Down) rola a página em
                        // passos de ~80% da área visível; ‹/› voltam a
                        // virar página.
                        MenuNav::Up | MenuNav::Down if r.zoom => {
                            // A área da página é a mesma do desenho.
                            let (sw, sh) = cab.shelf_screen_size();
                            let iw2 = inner_w(sw as u32);
                            let ay2 = MARGIN + 46;
                            let aw = iw2 as i32;
                            let ah = (sh as i32 - 110 - ay2) as i32;
                            let aspect = cab
                                .image_size(page_id(&r.path, r.page))
                                .map(|(iw, ih)| iw as f32 / ih.max(1) as f32)
                                .unwrap_or(0.72);
                            let dh = aw as f32 / aspect.max(0.0001);
                            let vis = (ah as f32 / dh).clamp(0.1, 1.0);
                            let dir = if matches!(nav, MenuNav::Down) {
                                1.0
                            } else {
                                -1.0
                            };
                            r.pan = (r.pan + dir * vis * 0.8).clamp(0.0, 1.0);
                        }
                        MenuNav::Left => r.page = r.page.saturating_sub(1).max(1),
                        MenuNav::Right => {
                            if r.pages > 0 && r.page < r.pages {
                                r.page += 1;
                            }
                        }
                        MenuNav::PageUp => r.page = r.page.saturating_sub(5).max(1),
                        MenuNav::PageDown => {
                            if r.pages > 0 {
                                r.page = (r.page + 5).min(r.pages);
                            }
                        }
                        MenuNav::Home => r.page = 1,
                        MenuNav::End => {
                            if r.pages > 0 {
                                r.page = r.pages;
                            }
                        }
                        MenuNav::Back => {
                            close = true;
                            break;
                        }
                        _ => {}
                    }
                }
                if close {
                    read = None;
                }
                if let Some(r) = read.as_mut() {
                    if let Some((x, y)) = m.click {
                        let (ox, oy) = cab.window_to_output(x, y);
                        if cab.hit_close_button(ox, oy) {
                            return Ok(LivPick::Quit);
                        }
                        if cab.hit_minimize_button(ox, oy) {
                            cab.minimize();
                            continue;
                        }
                        if cab.hit_tv_controls(ox, oy).is_some() {
                            // O queixo vive também no leitor (plan
                            // revision: "tem que estar sempre na
                            // tela"); o knob/volume já agiram dentro.
                            continue;
                        }
                        match cab.hit_panel_button(ox, oy) {
                            // O painel lateral segue vivo — clicar na
                            // estante/Configurações devolve ao idle (lá
                            // o clique abre de novo).
                            Some(PanelButton::Insert) | Some(PanelButton::Settings) => {
                                return Ok(LivPick::Back)
                            }
                            _ => {}
                        }
                        let (sx, sy) = cab.window_to_screen(x, y);
                        let (voltar, prev, next) = reader_rects(cab.shelf_screen_size());
                        // O clique na página liga/desliga o MODO ZOOM (a
                        // leitura estica para a altura da área toda).
                        let (sw, sh) = cab.shelf_screen_size();
                        let iw = inner_w(sw as u32);
                        let (ax, ay) = (MARGIN, MARGIN + 46);
                        let (aw, ah) = (iw as i32, (sh as i32 - 110 - ay) as i32);
                        if in_rect(sx, sy, (ax, ay, aw as u32, ah as u32)) {
                            r.zoom = !r.zoom;
                            r.pan = 0.0; // o zoom sempre abre no topo
                        }
                        if in_rect(sx, sy, voltar) {
                            read = None;
                        } else if let Some(dir) = if in_rect(sx, sy, prev) {
                            Some(-1isize)
                        } else if in_rect(sx, sy, next) {
                            Some(1isize)
                        } else {
                            None
                        } {
                            r.page = (r.page as isize + dir).max(1) as usize;
                            if r.pages > 0 {
                                r.page = r.page.min(r.pages);
                            }
                        }
                    }
                }
            } else {
                // A estante: ←/→ (e ↑/↓ cruzando prateleiras) movem o
                // foco; PgUp/PgDn vira a prateleira; Enter abre o
                // leitor; voltar devolve à tela inicial.
                let per = per_row(cab.shelf_screen_size().0);
                let visible = per * ROWS;
                for nav in m.nav.iter() {
                    match nav {
                        MenuNav::Left if on_url => on_url = false,
                        MenuNav::Left if sel > 0 => sel -= 1,
                        MenuNav::Right if !on_url && sel + 1 < books.len() => sel += 1,
                        MenuNav::Right if !on_url && !books.is_empty() => on_url = true,
                        MenuNav::Up | MenuNav::Down if on_url => on_url = false,
                        MenuNav::Up => sel = sel.saturating_sub(per),
                        MenuNav::Down if sel + 1 < books.len() => {
                            sel = (sel + per).min(books.len() - 1)
                        }
                        MenuNav::PageUp if top > 0 => top = top.saturating_sub(visible),
                        MenuNav::PageDown if top + visible < books.len() => {
                            top += visible;
                        }
                        MenuNav::Home => {
                            sel = 0;
                            top = 0;
                        }
                        MenuNav::End => sel = books.len().saturating_sub(1),
                        MenuNav::Confirm if on_url => {
                            editing_url = true;
                            url_draft.clear();
                            plat.start_text_input(cab);
                        }
                        MenuNav::Confirm => {
                            if let Some(book) = books.get(sel) {
                                read = Some(open_reader(book));
                            }
                        }
                        MenuNav::Back => return Ok(LivPick::Back),
                        _ => {}
                    }
                }
                if let Some((x, y)) = m.click {
                    let (ox, oy) = cab.window_to_output(x, y);
                    if cab.hit_close_button(ox, oy) {
                        return Ok(LivPick::Quit);
                    }
                    if cab.hit_minimize_button(ox, oy) {
                        cab.minimize();
                        continue;
                    }
                    if let Some(tv) = cab.hit_tv_controls(ox, oy) {
                        match tv {
                            TvControl::Power => cab.toggle_tv_power(),
                            TvControl::VolUp => cab.nudge_volume(1),
                            TvControl::VolDown => cab.nudge_volume(-1),
                            TvControl::Channel => cab.toggle_channel(),
                        }
                        continue;
                    }
                    match cab.hit_panel_button(ox, oy) {
                        // O painel lateral segue vivo — ESTANTE e
                        // Configurações devolvem ao idle (lá o clique
                        // abre a tela respectiva).
                        Some(PanelButton::Insert) | Some(PanelButton::Settings) => {
                            return Ok(LivPick::Back)
                        }
                        _ => {}
                    }
                    let (sx, sy) = cab.window_to_screen(x, y);
                    let (url_btn, voltar) = foot_rects(cab.shelf_screen_size());
                    if in_rect(sx, sy, voltar) {
                        return Ok(LivPick::Back);
                    }
                    if in_rect(sx, sy, url_btn) {
                        editing_url = true;
                        url_draft.clear();
                        plat.start_text_input(cab);
                        continue;
                    }
                    // Um tile: primeiro clique foca, o segundo no
                    // focado abre o leitor (o mesmo comportamento da
                    // estante: o primeiro Play é o clique).
                    let scr = cab.shelf_screen_size();
                    for i in 0..books.len() {
                        if let Some(r) = tile_rect(scr, top, i) {
                            if in_rect(sx, sy, r) {
                                if sel == i {
                                    read = Some(open_reader(&books[i]));
                                } else {
                                    sel = i;
                                    on_url = false;
                                }
                                break;
                            }
                        }
                    }
                }
            }
        }
        // (roda sempre: a Livraria não depende da TV — sem poll, a janela não responde)
        if read.is_none() {
            read_err = None;
        }

        draw_composite(
            cab,
            kind,
            &books,
            sel,
            top,
            on_url,
            read.as_ref(),
            read_err.as_deref(),
            editing_url,
            url_draft.clone(),
            &dl_label,
            dl_err.as_deref(),
            dl_ok_until.is_some(),
            cov_done.clone(),
            cov_failed.clone(),
            frame_no,
        );
        pace_frame(&mut next, frame);
    }
}

/// O frame inteiro: estante OU leitor no tubo + rodapé — por `frame_idle_2d`
/// (o painel lateral segue vivo ao lado, o vidro morre com a TV por cima).
#[allow(clippy::too_many_arguments)]
fn draw_composite(
    cab: &mut Cabinet,
    kind: LivKind,
    books: &[Book],
    sel: usize,
    top: usize,
    on_url: bool,
    read: Option<&Read>,
    read_err: Option<&str>,
    editing_url: bool,
    url_draft: String,
    dl_label: &str,
    dl_err: Option<&str>,
    dl_ok: bool,
    cov_done: HashSet<PathBuf>,
    cov_failed: HashSet<PathBuf>,
    frame_no: u64,
) {
    let caret_on = frame_no % ((CARET_FRAMES * 2) as u64) < CARET_FRAMES as u64;
    let render = |d: &mut Screen| {
        if let Some(r) = read {
            draw_reader(d, kind, r, read_err);
        } else {
            draw_shelf(d, kind, books, sel, top, on_url, &cov_done, &cov_failed);
        }
        draw_foot(
            d,
            kind,
            editing_url,
            &url_draft,
            caret_on,
            dl_label,
            dl_err,
            dl_ok,
        );
    };
    cab.frame_idle_2d(BG, render);
}

/// Abre o leitor para uma revista: um worker por abertura, que manda a
/// contagem (`Ready`) e depois as páginas conforme pedidas.
fn open_reader(book: &Book) -> Read {
    let (req_tx, req_rx) = mpsc::channel::<usize>();
    let (ev_tx, ev_rx) = mpsc::channel::<PageEv>();
    let path = book.path.clone();
    std::thread::spawn(move || {
        let Ok(pages) = crate::manual::page_count(&path) else {
            let _ = ev_tx.send(PageEv::Fail(
                tr!("the PDF didn't open", "o PDF não abriu").to_string(),
            ));
            return;
        };
        let _ = ev_tx.send(PageEv::Ready { pages });
        for page in req_rx {
            match crate::manual::render_page(&path, page, 2048) {
                Ok(Some(img)) => {
                    let _ = ev_tx.send(PageEv::Page {
                        page,
                        w: img.w,
                        h: img.h,
                        rgba: img.rgba,
                    });
                }
                _ => {
                    let _ = ev_tx.send(PageEv::Page {
                        page,
                        w: 0,
                        h: 0,
                        rgba: Vec::new(),
                    });
                }
            }
        }
    });
    Read {
        path: book.path.clone(),
        title: book.title.clone(),
        page: 1,
        pages: 0,
        ready: false,
        requested: HashSet::new(),
        empty: HashSet::new(),
        req_tx,
        ev_rx,
        zoom: false,
        pan: 0.5,
    }
}

// --- desenho ----------------------------------------------------------------

/// Botão flat do app (o mesmo estilo dos botões da tela de update):
/// contorno de 2px + rótulo centrado, ativos em branco e dormentes em cinza.
/// O violeta fósforo da TV (o mesmo da estante de jogos).
const FOSFORO: (u8, u8, u8) = (186, 138, 244);

/// Botão na linguagem FÓSFORO da TV (a mesma da estante de jogos e do
/// OSD): texto em violeta com sublinhado quando ativo, `DIM` sem caixa
/// quando inativo — o rect de clique é o de sempre.
fn draw_flat_button(d: &mut Screen, r: ScrRect, label: &str, active: bool) {
    let (cor, sub) = if active {
        (FOSFORO, true)
    } else {
        ((96, 96, 104), false)
    };
    let tw = label.chars().count() as i32 * 9;
    let tx = r.0 + (r.2 as i32 - tw).max(0) / 2;
    let ty = r.1 + (r.3 as i32 - 20) / 2;
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

/// A estante: o cabeçalho, as prateleiras com as capas e o rodapé. Devolve
/// os rects dos dois botões do rodapé (BAIXAR e VOLTAR) para o hit-test.
#[allow(clippy::too_many_arguments)]
fn draw_shelf(
    d: &mut Screen,
    kind: LivKind,
    books: &[Book],
    sel: usize,
    top: usize,
    on_url: bool,
    _cov_done: &HashSet<PathBuf>,
    cov_failed: &HashSet<PathBuf>,
) -> (ScrRect, ScrRect) {
    let (w, h) = d.size();
    let scr = (w, h);
    let (w, h) = (w as i32, h as i32);
    let iw = inner_w(w as u32);

    d.text(MARGIN, MARGIN, 2, TEXT, kind.title());
    let count = format!("{} {}", books.len(), kind.item_word());
    d.text(
        w - MARGIN - (count.chars().count() as i32) * 9,
        MARGIN + 20,
        1,
        DIM,
        &count,
    );

    let per = per_row(w as u32);
    let rows_h = foot_top(h as u32) - rack_top(h as u32);
    let row_h = rows_h / ROWS as i32;

    if books.is_empty() {
        // A gaveta vazia explica as duas portas (plan revision: PDFs por
        // download OU largados à mão na pasta).
        let rt = rack_top(h as u32);
        d.text_wrapped(
            MARGIN,
            rt + 10,
            iw,
            1,
            TEXT,
            tr!("The shelf is empty.", "A estante está vazia."),
        );
        d.text_wrapped(MARGIN, rt + 42, iw, 1, DIM, kind.empty_intro());
        d.text_wrapped(
            MARGIN,
            rt + 68,
            iw,
            1,
            TEXT,
            tr!(
                "1. drop the files straight into the app folder:",
                "1. largue os arquivos direto na pasta do app:"
            ),
        );
        d.text_wrapped(
            MARGIN,
            rt + 92,
            iw,
            1,
            DIM,
            &LivKind::Revistas.dir().display().to_string(),
        );
        d.text_wrapped(
            MARGIN,
            rt + 122,
            iw,
            1,
            TEXT,
            tr!(
                "2. paste the PDF URL into the DOWNLOAD URL button, below.",
                "2. cole o URL do PDF no botão BAIXAR URL, abaixo."
            ),
        );
    } else {
        // Duas prateleiras: face frontal, lip iluminado e sombra por baixo.
        for row in 0..ROWS {
            let plank_y = rack_top(h as u32) + row as i32 * row_h + row_h - 26;
            d.fill(MARGIN, plank_y, iw, 12, PLANK);
            d.fill(MARGIN, plank_y, iw, 3, PLANK_LIT);
            d.fill(MARGIN, plank_y + 12, iw, 2, PLANK_EDGE);
            for c in 0..per {
                let i = top + row * per + c;
                let Some(book) = books.get(i) else { continue };
                let Some(r) = tile_rect(scr, top, i) else {
                    continue;
                };
                // A capa SEM ESTICAR e SEM CORTAR: a caixa tem a
                // proporção exata da página 1 do PDF (retrato de revista
                // vira tile alto; capa dupla/paisagem vira tile deitado),
                // centrada no slot e pousada na prateleira.
                let cover_h = r.3 as i32;
                let aspect = d
                    .image_size(cover_id(&book.path))
                    .map(|(iw, ih)| iw as f32 / ih.max(1) as f32)
                    .unwrap_or(0.72);
                let mut w = (cover_h as f32 * aspect).round() as i32;
                let mut h = cover_h;
                if w > r.2 as i32 {
                    w = r.2 as i32;
                    h = (w as f32 / aspect.max(0.0001)).round() as i32;
                }
                let cr = (
                    r.0 + ((r.2 as i32 - w) / 2),
                    r.1 + (cover_h - h),
                    w as u32,
                    h as u32,
                );
                d.fill(cr.0, cr.1, cr.2, cr.3, TILE_BG);
                let cid = cover_id(&book.path);
                if d.has_image(cid) {
                    d.image_fit(cid, cr.0, cr.1, cr.2, cr.3);
                } else if cov_failed.contains(&book.path) {
                    // O worker já tentou esta capa: o scan não tem imagem
                    // na página 1 — fala isso, sem "sempre carregando".
                    d.text(
                        cr.0 + 6,
                        cr.1 + cr.3 as i32 / 2 - 10,
                        1,
                        DIM,
                        tr!("no cover", "sem capa"),
                    );
                } else {
                    // A capa ainda vem: o título carrega o tile.
                    d.text_wrapped(cr.0 + 6, cr.1 + 10, cr.2 - 12, 1, DIM, &book.title);
                }
                if sel == i && !on_url {
                    let line = (232, 232, 232, 255);
                    d.fill(cr.0 - 2, cr.1 - 2, cr.2 + 4, 2, line);
                    d.fill(cr.0 - 2, cr.1 + cr.3 as i32, cr.2 + 4, 2, line);
                    d.fill(cr.0 - 2, cr.1, 2, cr.3, line);
                    d.fill(cr.0 + cr.2 as i32, cr.1, 2, cr.3, line);
                }
            }
        }
        // Página de prateleira: «/» no alto de cada lado quando tem mais.
        if top > 0 {
            d.text(MARGIN, MARGIN + 58, 1, DIM, "« in flashes");
        }
        if top + per * ROWS < books.len() {
            d.text(
                w - MARGIN - 36,
                MARGIN + 58,
                1,
                DIM,
                tr!("more »", "tem mais »"),
            );
        }
    }

    // O rodapé: BAIXAR + VOLTAR (o foco da URL acende o BAIXAR).
    let (url_btn, voltar) = foot_rects(scr);
    draw_flat_button(d, url_btn, tr!("DOWNLOAD URL", "BAIXAR URL"), on_url);
    draw_flat_button(d, voltar, tr!("BACK", "VOLTAR"), false);
    (url_btn, voltar)
}

/// O rodapé do download/dicas — desenhado por cima de todo o resto quando
/// há algo acontecendo (status em verde/vermelho/cinza) ou o campo de URL
/// aberto.
fn draw_foot(
    d: &mut Screen,
    kind: LivKind,
    editing_url: bool,
    url_draft: &str,
    caret_on: bool,
    dl_label: &str,
    dl_err: Option<&str>,
    dl_ok: bool,
) {
    let (w, h) = d.size();
    let (w, h) = (w as i32, h as i32);
    let iw = inner_w(w as u32);
    if editing_url {
        // O campo ocupa a faixa do meio: texto + caret piscando, a dica logo
        // abaixo DENTRO da caixa. Os botões de baixo ficam fora do alcance
        // (o editor é o dono do teclado).
        let by = h - 186;
        d.fill(MARGIN, by, iw, 56, TILE_BG);
        d.outline(MARGIN, by, iw, 56, 2, (232, 232, 232, 255));
        // Mostra o rabo da URL — URLs grandes estouram a linha.
        let capacity = ((iw as u32 - 40) / 9) as usize;
        let shown: String = url_draft
            .chars()
            .rev()
            .take(capacity)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let caret = if caret_on { "|" } else { " " };
        d.text(MARGIN + 12, by + 8, 1, TEXT, &format!("{}{caret}", shown));
        d.text(
            MARGIN + 12,
            by + 36,
            1,
            DIM,
            tr!(
                "enter to download · esc to cancel",
                "enter baixa · esc cancela"
            ),
        );
        return;
    }
    // O status mora na linha acima dos botões; sem download nenhum, a
    // dica da pasta fica no lugar.
    let sy = h - 40;
    if let Some(e) = dl_err {
        d.text(MARGIN, sy, 1, RED, e);
    } else if dl_ok {
        d.text(MARGIN, sy, 1, GREEN, dl_label);
    } else if !dl_label.is_empty() {
        d.text(MARGIN, sy, 1, DIM, dl_label);
    } else {
        d.text(MARGIN, sy, 1, DIM, kind.footer_hint());
    }
}

/// O leitor: cabeçalho com a página corrente, a página do scan no vão
/// (placeholder enquanto renderiza) e os três botões. Devolve os rects
/// (voltar, ant, próx) para o hit-test.
fn draw_reader(
    d: &mut Screen,
    kind: LivKind,
    read: &Read,
    err: Option<&str>,
) -> (ScrRect, ScrRect, ScrRect) {
    let (w, h) = d.size();
    let scr = (w, h);
    let (w, h) = (w as i32, h as i32);
    let iw = inner_w(w as u32);

    d.text_wrapped(MARGIN, MARGIN, iw - 200, 1, TEXT, &read.title);
    let pages_label = if read.ready {
        tr!(
            format!("page {} of {}", read.page, read.pages),
            format!("página {} de {}", read.page, read.pages)
        )
    } else {
        tr!("page …", "página …").to_string()
    };
    d.text(
        w - MARGIN - (pages_label.chars().count() as i32) * 9,
        MARGIN,
        1,
        DIM,
        &pages_label,
    );

    // A área da página: abaixo do cabeçalho, até pouco acima dos botões.
    let (ax, ay) = (MARGIN, MARGIN + 46);
    let (aw, ah) = (iw as u32, (h - 110 - ay) as u32);
    let pid = page_id(&read.path, read.page);
    if read.zoom && d.has_image(pid) {
        // MODO ZOOM: a página estica para a altura da área toda (cabe
        // muito mais texto do scan) e ‹/› arrastam; clicar de novo volta.
        d.fill(ax, ay, aw, ah, TILE_BG);
        d.image_scroll_v(pid, (ax, ay, aw, ah), read.pan);
    } else if d.has_image(pid) {
        d.image_fit(pid, ax, ay, aw, ah);
    } else {
        d.fill(ax, ay, aw, ah, TILE_BG);
        let msg = if let Some(e) = err {
            tr!(
                format!("the PDF didn't open: {e}"),
                format!("o PDF não abriu: {e}")
            )
        } else if !read.ready {
            tr!(
                format!("opening the {}…", kind.item_singular()),
                match kind {
                    LivKind::Revistas => "abrindo a revista…".to_string(),
                    LivKind::Museu => "abrindo o documento…".to_string(),
                }
            )
        } else if read.empty.contains(&read.page) {
            tr!(
                "this page has no image in the scan",
                "esta página não tem imagem no scan"
            )
            .to_string()
        } else {
            tr!("rendering the page…", "renderizando a página…").to_string()
        };
        let x = ax + ((aw as i32 - msg.chars().count() as i32 * 9) / 2);
        let y = ay + ah as i32 / 2 - 10;
        d.text(x, y, 1, DIM, &msg);
    }

    let (voltar, prev, next) = reader_rects(scr);
    let can_prev = read.page > 1;
    let can_next = read.ready && read.pages > 0 && read.page < read.pages;
    draw_flat_button(d, voltar, tr!("BACK", "VOLTAR"), true);
    draw_flat_button(d, prev, tr!("« prev", "« pág"), can_prev);
    draw_flat_button(d, next, tr!("next »", "pág »"), can_next);
    (voltar, prev, next)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_names() {
        // O segmento final manda; query some; `.pdf` garantido.
        assert_eq!(
            url_filename("https://example.com/scans/edicao-01.pdf?url=seed").as_deref(),
            Some("edicao-01.pdf")
        );
        assert_eq!(
            url_filename("https://x.y/revista%20z").as_deref(),
            Some("revista z.pdf")
        );
        // Segmento sumido (URL até a barra) → None, e o chamador reclama.
        assert_eq!(url_filename("https://x.y/"), None);
        assert_eq!(url_filename("nada"), None);
    }

    #[test]
    fn dest_without_collision() {
        let dir = std::env::temp_dir();
        let name = unique_tmp_name();
        let a = free_dest(&dir, &name);
        std::fs::write(&a, b"x").unwrap();
        let b = free_dest(&dir, &name);
        assert_ne!(a, b);
        assert!(b.to_string_lossy().contains("(2)"));
        let _ = std::fs::remove_file(a);
    }

    fn unique_tmp_name() -> String {
        use std::time::{SystemTime, UNIX_EPOCH};
        format!(
            "livraria-{}.pdf",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        )
    }

    #[test]
    fn human_bytes() {
        assert_eq!(fmt_bytes(512), "512 B");
        assert_eq!(fmt_bytes(3 * 1024 * 1024 + 130 * 1024), "3.1 MB");
    }

    #[test]
    fn title_prettify() {
        assert_eq!(
            pretty_title("Super_Game_World_1-1994"),
            "Super Game World 1-1994"
        );
        assert_eq!(pretty_title("  spaços   fora "), "spaços   fora");
    }
}

// --- previews headless (fotos de verificação, o mesmo acordo do capturar_
// history_preview da estante) ------------------------------------------------

/// A estante📸: render sincrono das capas (sem worker/loop) + um frame
/// capturado pelo `capture_shelf`.
pub fn capture_preview(cab: &mut Cabinet, path: &Path) -> Result<()> {
    let books = scan_books(LivKind::Revistas);
    for book in &books {
        if let Ok(Some(img)) = crate::manual::render_page(&book.path, 1, 512) {
            cab.set_image(cover_id(&book.path), img.w, img.h, &img.rgba);
        }
    }
    let render = |d: &mut Screen| {
        draw_shelf(
            d,
            LivKind::Revistas,
            &books,
            0,
            0,
            false,
            &HashSet::new(),
            &HashSet::new(),
        );
        draw_foot(d, LivKind::Revistas, false, "", true, "", None, false);
    };
    cab.capture_shelf(BG, render, path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

/// O leiter📸: abre a primeira da fila, pede a página `page`, ESPERA a
/// contagem + a textura (o worker é assíncrono; o preview não corre 60fps)
/// e captura um frame com o leitor num estado de uso real.
pub fn capture_reader_preview(cab: &mut Cabinet, page: usize, path: &Path) -> Result<()> {
    let books = scan_books(LivKind::Revistas);
    let Some(book) = books.first() else {
        // Sem revista nenhuma, a foto da estante vazia serve.
        return capture_preview(cab, path);
    };
    let mut read = open_reader(book);
    read.requested.insert(page);
    let _ = read.req_tx.send(page);
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        loop {
            match read.ev_rx.try_recv() {
                Ok(PageEv::Ready { pages }) => {
                    read.pages = pages;
                    read.ready = true;
                }
                Ok(PageEv::Fail(_)) => {
                    read.ready = true;
                    break;
                }
                Ok(PageEv::Page {
                    page: p,
                    w,
                    h,
                    rgba,
                }) => {
                    if w > 0 {
                        cab.set_image(page_id(&read.path, p), w, h, &rgba);
                        read.requested.insert(p);
                    } else {
                        read.empty.insert(p);
                        read.requested.insert(p);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
        if read.ready && (cab.has_image(page_id(&book.path, page)) || read.empty.contains(&page)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    read.page = page;
    let render = |d: &mut Screen| {
        draw_reader(d, LivKind::Revistas, &read, None);
        draw_foot(d, LivKind::Revistas, false, "", true, "", None, false);
    };
    cab.capture_shelf(BG, render, path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}

/// O campo de URL📸: a estante com o editor de URL vivo (o draft de https://…),
/// para bater com a foto do download por URL.
pub fn capture_url_preview(cab: &mut Cabinet, path: &Path) -> Result<()> {
    let books = scan_books(LivKind::Revistas);
    for book in &books {
        if let Ok(Some(img)) = crate::manual::render_page(&book.path, 1, 512) {
            cab.set_image(cover_id(&book.path), img.w, img.h, &img.rgba);
        }
    }
    let render = |d: &mut Screen| {
        draw_shelf(
            d,
            LivKind::Revistas,
            &books,
            0,
            0,
            false,
            &HashSet::new(),
            &HashSet::new(),
        );
        draw_foot(
            d,
            LivKind::Revistas,
            true,
            "https://arquivos.revista.br/scans/Super_Jogos_Brazil_1988.pdf",
            true,
            "",
            None,
            false,
        );
    };
    cab.capture_shelf(BG, render, path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))
}
