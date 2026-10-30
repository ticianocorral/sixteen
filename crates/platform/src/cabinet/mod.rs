//! The one window: a static dark cabinet with the screen recessed into it. Both
//! the running game and the selector draw into that screen area — the game as a
//! frame through a barrel-distorted CRT mesh, the selector as flat 2D (rects,
//! bitmap text, letterboxed images). The cabinet furniture (the chamfer ring
//! from the window edge down to the glass) is redrawn every frame so nothing
//! ever recreates the window. NTSC colour bleed is applied upstream
//! (`sixteen-ntsc`).

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

use noto_sans_mono_bitmap::{get_raster, FontWeight, RasterHeight};
use sdl3::pixels::{Color, FColor, PixelFormat as SdlFormat};
use sdl3::rect::Rect;
use sdl3::render::{
    BlendMode, ClippingRect, ScaleMode as SdlScaleMode, Texture, Vertex, WindowCanvas,
};
use sdl3::{AudioSubsystem, VideoSubsystem};

use crate::tr;
use crate::{audio::AudioOut, PlatformError};

mod chin;
mod chrome;
mod controls;
mod geometry;
mod panel;
mod panel_art;
mod pause;
mod text;

use chin::*;
use chrome::*;
use controls::*;
use geometry::*;
use panel::*;
use panel_art::*;
use pause::*;
use text::*;

/// CRT tube shape. `WARP` is how hard the edges bow (0 = flat); `VIGNETTE` is
/// how much the corners darken; `GRID` is the mesh resolution.
const CRT_WARP: f32 = 0.06;
const CRT_VIGNETTE: f32 = 0.22;
const CRT_GRID: usize = 32;

/// Cabinet around the tube. The screen is inset from the window by these
/// fractions (a bit more at the bottom for the "chin"); everything outside is
/// the cabinet face, chamfered down to a near-black recess at the screen edge.
const BEZEL_SIDE: f32 = 0.070;
const BEZEL_TOP: f32 = 0.050;
const BEZEL_CHIN: f32 = 0.095;
/// Cabinet face — dark warm-grey plastic. Reads as a surface, still far darker
/// than a lit game screen (plan §3.2).
const CABINET: (u8, u8, u8) = (40, 37, 33);
/// The lip right against the glass, in shadow.
const RECESS: (u8, u8, u8) = (4, 4, 5);
/// Reserved image-cache key for the side panel's logo art.
const PANEL_LOGO_IMG: u64 = u64::MAX - 1;
/// Reserved image-cache key for the panel's most-recent note thumbnail.
const PANEL_NOTE_IMG: u64 = u64::MAX - 2;
/// Reserved image-cache key for the pause book's right-hand page.
const PAUSE_THUMB_IMG: u64 = u64::MAX - 3;
/// Reserved image-cache key for the side panel's cartridge art.
const PANEL_CARTRIDGE_IMG: u64 = u64::MAX - 4;
/// Reserved image-cache key for the idle screen's console-brand logo (plan
/// revision — `assets/console.png`, set once via `Cabinet::set_console_logo`,
/// not per-game).
const PANEL_CONSOLE_LOGO_IMG: u64 = u64::MAX - 5;
/// Reserved image-cache key for the console tag wordmark printed on the
/// slot's loading base (plan revision — `assets/console-tag.png`, set once
/// via `Cabinet::set_slot_tag`, not per-game).
const SLOT_TAG_IMG: u64 = u64::MAX - 6;

/// The pause book: two pages, not warped by the tube — a dedicated screen
/// (plan §3.2/§3.4), not cabinet furniture, so it replaces the whole window
/// rather than sharing space with the panel/cartridge/brand.
const PAUSE_MARGIN: f32 = 0.06;
const PAUSE_GAP: f32 = 0.02;
const PAUSE_TOP: f32 = 0.08;

/// The side panel: a column of plain widgets beside the tube during play —
/// not warped, drawn straight on the window (plan §2's presentation order,
/// §3.2). A fixed fraction of the window, clamped so it neither disappears on
/// a small window nor swallows a huge one.
const PANEL_FRAC: f32 = 0.25;
const PANEL_MIN: u32 = 260;
const PANEL_MAX: u32 = 520;
const PANEL_BG: (u8, u8, u8) = (16, 15, 14);
const PANEL_TEXT: (u8, u8, u8) = (225, 220, 210);
const PANEL_DIM: (u8, u8, u8) = (140, 134, 124);
/// Warm amber for the idle screen's core warning (plan revision) — reads as
/// "atenção" without shouting over the panel's own palette.
const PANEL_WARN: (u8, u8, u8) = (228, 180, 90);
/// Fill for a clickable panel button (`draw_button`) — a shade lighter than
/// `PANEL_BG` so it reads as its own control, not flat background text (the
/// in-TV shelf screen's flat buttons in `frame_image_zoom` use it too; the
/// console's own three controls wear `draw_flat_key`'s plastic keys instead).
const PANEL_BTN_BG: (u8, u8, u8) = (34, 32, 29);

/// The console's own controls — Eject (`draw_eject_button`) and the
/// Power/Reset thumbs (`draw_rocker`) — wear the flat matte key
/// (`draw_flat_key`): a plain rectangle with softly rounded corners and no
/// gloss anywhere, like the real hardware's plastic (plan revision). The
/// palettes carry the key's body/outline plus the debossed-oval shades —
/// `draw_rocker_thumb` presses one into the Power/Reset faces; Eject's key
/// is plain and never reads them. `*_LIT` is a control that would do
/// something right now, `*_DIM` inert grey for one that wouldn't. Every
/// other panel button stays a plain flat `draw_button`.
struct FlatKeyPalette {
    /// Dark rim where the key meets the panel — the button's "seating".
    outline: (u8, u8, u8),
    /// The key's flat face colour.
    body: (u8, u8, u8),
    /// The oval's recessed floor.
    floor: (u8, u8, u8),
    /// The oval's shadowed top wall.
    wall_dark: (u8, u8, u8),
    /// The oval's bottom wall, catching a little of the top-down light.
    wall_lit: (u8, u8, u8),
    /// Label colour, tuned to hold up on the flat face.
    text: (u8, u8, u8),
}
/// Power/Reset: the switch violet, same as ever.
const ROCKER_LIT: FlatKeyPalette = FlatKeyPalette {
    outline: (24, 16, 42),
    body: SWITCH_PURPLE,
    floor: SWITCH_THUMB_FLOOR,
    wall_dark: SWITCH_THUMB_WALL_DARK,
    wall_lit: SWITCH_THUMB_WALL_LIT,
    text: (243, 240, 232),
};
/// Power/Reset inert: the same key in grey plastic.
const ROCKER_DIM: FlatKeyPalette = FlatKeyPalette {
    outline: (10, 10, 9),
    body: (60, 57, 53),
    floor: SWITCH_THUMB_FLOOR_DIM,
    wall_dark: SWITCH_THUMB_WALL_DARK_DIM,
    wall_lit: SWITCH_THUMB_WALL_LIT_DIM,
    text: PANEL_DIM,
};
/// Eject: the cartridge's own grey plastic (plan revision: "o cinza do
/// cartucho (mais escuro)") — sampled off the real scans the app shows in
/// the slot (`assets/cartridge/*.png` cluster around RGB 138,132,138: a
/// cool mid-dark grey, a step below the console's light shell). The deboss
/// shades sit unused; the oval lives on the Power/Reset keys only.
const EJECT_LIT: FlatKeyPalette = FlatKeyPalette {
    outline: (62, 59, 66),
    body: (138, 132, 138),
    floor: (118, 112, 118),
    wall_dark: (98, 93, 99),
    wall_lit: (126, 120, 126),
    text: (236, 233, 228),
};
/// Eject inert: a step darker so the live/inert distinction survives.
const EJECT_DIM: FlatKeyPalette = FlatKeyPalette {
    outline: (26, 25, 28),
    body: (95, 91, 99),
    floor: (80, 77, 84),
    wall_dark: (64, 61, 67),
    wall_lit: (88, 84, 92),
    text: PANEL_DIM,
};

/// Power/Reset rocker-switch colours, styled after the reference photo of
/// the real console's controls — a mid violet key riding in a dark
/// rectangular pocket whose grey rim is visibly lighter than the panel
/// around it.
const SWITCH_PURPLE: (u8, u8, u8) = (107, 70, 168);
const SWITCH_TRACK_BG: (u8, u8, u8) = (24, 22, 26);
const SWITCH_TRACK_BORDER: (u8, u8, u8) = (76, 74, 82);
/// The track's inner shadow along its top edge — light comes from above, so
/// the recess the thumb rides in is darkest right under its lip.
const SWITCH_TRACK_SHADOW: (u8, u8, u8) = (12, 11, 14);

/// The thumb's debossed oval (plan revision: "um leve oval em baixo relevo
/// no meio... nao deve ter brilho") — flat violet shades darker than the
/// key's body: the recess floor, its shadowed top wall and the bottom wall
/// that catches a little of the same top-down light. No gradient anywhere —
/// the key itself stays the flat `SWITCH_PURPLE`. The `_DIM` set does the
/// same in inert grey for a control that wouldn't do anything right now.
const SWITCH_THUMB_FLOOR: (u8, u8, u8) = (81, 52, 128);
const SWITCH_THUMB_WALL_DARK: (u8, u8, u8) = (60, 37, 99);
const SWITCH_THUMB_WALL_LIT: (u8, u8, u8) = (95, 62, 148);
const SWITCH_THUMB_FLOOR_DIM: (u8, u8, u8) = (44, 41, 38);
const SWITCH_THUMB_WALL_DARK_DIM: (u8, u8, u8) = (33, 31, 29);
const SWITCH_THUMB_WALL_LIT_DIM: (u8, u8, u8) = (52, 49, 45);

/// Power LED (plan revision: "luz vermelha led indicando o power... igual o
/// console original") — a real SNES has one lit red next to its switches
/// whenever the console is on. `LED_ON_HI` is a small glossy highlight dot
/// drawn on top when lit.
const LED_ON: (u8, u8, u8) = (214, 44, 40);
const LED_ON_HI: (u8, u8, u8) = (255, 150, 140);
/// O LED verde do queixo da TV (a foto do painel real: pontinho verde
/// ao lado do POWER).
const LED_TV_ON: (u8, u8, u8) = (64, 220, 96);
const LED_TV_ON_HI: (u8, u8, u8) = (170, 255, 190);
const LED_OFF: (u8, u8, u8) = (56, 26, 26);

/// O prateado das TVs antigas (plan revision: "botoes de volume e power
/// da tv ... tom prateado como tv antiga") — alumínio pintado, cinza
/// claro e frio, a família oposta do cinza quente do cartucho
/// (`EJECT_LIT.body`). O estado liga/desliga mora no quão acesa a chapa
/// está (`PRATA` / `PRATA_DIM`), com a gravação escura como tinta dos
/// rótulos (`PRATA_INK`) — como os dials antigos marcavam POWER/VOL.
const PRATA: (u8, u8, u8) = (184, 187, 195);
/// O brilho fosco do topo da chapa (metal, não plástico).
const PRATA_HI: (u8, u8, u8) = (234, 236, 242);
/// Desligada: a chapa passa a aço no escuro, um passo acima do `CABINET`.
const PRATA_DIM: (u8, u8, u8) = (104, 106, 116);
/// A tinta gravada dos rótulos/indicadores sobre a chapa acesa.
const PRATA_INK: (u8, u8, u8) = (34, 36, 44);
/// A gravação apagada sobre a chapa desligada.
const PRATA_INK_DIM: (u8, u8, u8) = (168, 170, 178);

/// A linguagem fósforo dos botões que vivem dentro da TV (plan revision) —
/// o violeta da chave Power/Reset (`SWITCH_PURPLE`) clareado para ler contra
/// o fundo escuro do tubo; a estante a chama de `FOSFORO` também.
const FOSFORO: (u8, u8, u8) = (186, 138, 244);

/// The panel's cartridge slot (plan revision: the reference photo — a
/// cartridge standing upright, plugged into the console's loading base) —
/// the in-game panel's cartridge block drawn as the console seen from the
/// front: a solid light-grey base slab across the block's bottom with the
/// dark slot mouth across its top edge, the cartridge standing in it.
/// Colours are the light-grey console plastic this panel already used, plus
/// a darker grounding edge and one groove line for the dust-shield strip
/// (see `draw_panel_slot`).
const SLOT_SHELL: (u8, u8, u8) = (188, 182, 170);
const SLOT_SHELL_EDGE: (u8, u8, u8) = (146, 140, 128);
const SLOT_BEZEL: (u8, u8, u8) = (134, 128, 117);
const SLOT_MOUTH: (u8, u8, u8) = (16, 16, 18);
const SLOT_RIDGE: (u8, u8, u8) = (120, 114, 104);

/// The cartridge body's width as a fraction of the panel block's width —
/// the loading base spans it all and the cart sits a shade inside it, per
/// the user's annotated reference (the vertical red lines).
const CART_WIDTH_FRAC: f32 = 0.98;
/// How much of the cartridge's body stays below the slot mouth's lip when
/// seated — a third of it, hidden inside the console, so the cart visibly
/// *enters* the slot instead of standing on it (the horizontal red line in
/// the same reference marks the lip where the body crosses into the slot).
const SEAT_HIDDEN_FRAC: f32 = 0.50;

/// The set's own nameplate: a small wordmark printed into the chin, left of
/// the cartridge — a touch lighter than the cabinet plastic, like an embossed
/// badge rather than a lit label.
/// The console's own brand name — the nameplate's default text before the
/// app calls `Cabinet::set_nameplate` with its own version appended, and
/// what the idle screen's panel falls back to in plain text when there's no
/// logo image loaded (`draw_panel`'s idle branch).
pub const BRAND: &str = "SixteeN";
const BRAND_TEXT: (u8, u8, u8) = (92, 86, 78);

/// Glyph cell (Noto Sans Mono, anti-aliased, rasterized once at boot into an
/// atlas texture — see `build_font_atlas`), before scaling: 20px tall gives
/// noticeably bigger, smoother UI text than the old 8x8 bitmap font while
/// staying monospace, so every existing cell-based layout calculation below
/// keeps working unchanged.
const FONT_HEIGHT: RasterHeight = RasterHeight::Size20;
const FONT_WEIGHT: FontWeight = FontWeight::Regular;
/// Advance width and line height of one glyph cell, before scaling.
const GLYPH_W: u32 = 9;
const GLYPH_H: u32 = 20;
/// Atlas covers one contiguous Unicode range: printable Basic Latin through
/// Latin-1 Supplement (space through `ÿ`) — plain ASCII plus the accented
/// letters Portuguese needs (á, ã, ç, é, õ, ...), in one indexable block.
const GLYPH_FIRST: u32 = 0x20;
const GLYPH_LAST: u32 = 0xFF;
const GLYPH_COLS: u32 = GLYPH_LAST - GLYPH_FIRST + 1;

/// Pixel layout of a core framebuffer. Mirrors `sixteen_emulation::PixelFormat`
/// so the platform layer stays independent of the emulation crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Rgb1555,
    Xrgb8888,
    Rgb565,
}

impl PixelFormat {
    fn sdl(self) -> SdlFormat {
        match self {
            PixelFormat::Rgb1555 => SdlFormat::XRGB1555,
            PixelFormat::Xrgb8888 => SdlFormat::XRGB8888,
            PixelFormat::Rgb565 => SdlFormat::RGB565,
        }
    }
    fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Rgb1555 | PixelFormat::Rgb565 => 2,
            PixelFormat::Xrgb8888 => 4,
        }
    }
}

/// A borrowed core frame ready to upload.
pub struct FrameRef<'a> {
    pub width: u32,
    pub height: u32,
    pub pitch: usize,
    pub format: PixelFormat,
    pub pixels: &'a [u8],
}

pub struct Cabinet {
    canvas: WindowCanvas,
    /// Streaming texture at the incoming frame's resolution (game path).
    src: Option<SrcTexture>,
    /// Cached CRT mesh; rebuilt only when the game rect resizes.
    mesh: Option<CrtMesh>,
    /// Cópia do `mesh` para os fades de sinal (estante/2D surgindo da
    /// estática): só o alpha dos vértices é reescrito a cada frame, em vez
    /// de reconstruir duas malhas (~1.100 vértices cada) por frame.
    fade_mesh: Option<CrtMesh>,
    /// Cached cabinet mesh; rebuilt only when the window or screen rect changes.
    bezel: Option<BezelMesh>,
    /// Render target the selector draws its flat 2D into, then composited
    /// through the tube like a game frame.
    screen_tex: Option<SizedTex>,
    /// Small streaming texture for the signal-off snow.
    noise_tex: Option<SizedTex>,
    noise: Vec<u8>,
    /// When the static noise was last regenerated (and at what level) —
    /// `update_noise_tex` caps regeneration at ~30Hz.
    noise_stamp: Option<(std::time::Instant, f32)>,
    rng: u32,
    /// 128 glyphs laid out horizontally, white on transparent (2D path).
    font: Texture,
    images: HashMap<u64, ImgTex>,
    /// The current inner-screen rect (the tube opening).
    screen: Rect,
    /// The side panel content (game path only, plan §3.2). `None` = no game
    /// loaded — the idle/root screen (§3.2 item 1's "Inserir cartucho"
    /// button) draws there instead.
    panel: Option<PanelInfo>,
    /// Elapsed time to show at the bottom of the panel; the caller updates
    /// this once a frame (`set_session_time`).
    session: Duration,
    /// The pause book's content (plan §3.2/§3.4), set once on entering pause
    /// and read every frame while `present_pause` is what's on screen.
    pause: Option<PauseNote>,
    /// A save/load-state or print slot picker (plan revision), set on
    /// opening one and read every frame while `present_modal` is on screen.
    modal: Option<ModalInfo>,
    fullscreen: bool,
    /// Clickable panel buttons drawn last frame, in output/canvas coordinates
    /// — `hit_panel_button` scans this. Repopulated by `present_frame`/
    /// `present_static` right after `draw_panel`.
    panel_buttons: Vec<(PanelButton, Rect)>,
    /// Clickable buttons on the pause book screen ("Continuar"/"Avancar
    /// quadro") drawn last frame — `hit_pause_button` scans this. The pause
    /// book replaces the whole window (no side panel drawn alongside it), so
    /// it needs its own list rather than sharing `panel_buttons`.
    pause_buttons: Vec<(PanelButton, Rect)>,
    /// Clickable buttons on the modal dialog drawn last frame —
    /// `hit_modal_button` scans this, same pattern as `pause_buttons`.
    modal_buttons: Vec<(PanelButton, Rect)>,
    /// The shelf's own flat side panel content (plan revision) — `None`
    /// outside the shelf (game/idle screens keep using `panel` instead).
    shelf_panel: Option<ShelfPanelInfo>,
    /// Clickable buttons on the shelf's flat panel drawn last frame —
    /// `hit_shelf_button` scans this, same pattern as `panel_buttons`.
    shelf_buttons: Vec<(ShelfButton, Rect)>,
    /// The settings screen's own flat panel content (plan revision) — `None`
    /// outside settings.
    settings_panel: Option<SettingsPanelInfo>,
    /// Clickable buttons on the settings panel drawn last frame —
    /// `hit_settings_button` scans this.
    settings_buttons: Vec<(SettingsButton, Rect)>,
    /// The audio subsystem handle, for the ambient static hiss (plan
    /// revision: "som de chiado de tv fora do ar") — the hiss engine itself
    /// lives here because `present_static` is where "TV fora do ar" happens.
    audio: AudioSubsystem,
    /// Whether the ambient hiss should play at all — the settings toggle,
    /// off by default. App-side gate; `present_static` only queues while
    /// this is set.
    static_hiss: bool,
    /// The open hiss stream, kept across frames (idle loops at 60fps);
    /// `None` until the first hissing static frame after the toggle turns
    /// on.
    hiss: Option<AudioOut>,
    /// XORSHIFT state for the hiss samples — same generator the power
    /// bursts use, so the texture of the noise matches.
    hiss_rng: u32,
    /// O estado da TV (plan revision: "ao abrir o app a tv estiver desligada
    /// e fazer um botao de power") — desligada, aquecendo (o warm-up do
    /// tubo: ponto -> linha -> raster abrindo), ligada, ou colapsando (o
    /// desligar: imagem -> linha -> ponto -> afterglow). `None` = ligada,
    /// o estado de sempre de todo o app antes desta revisão.
    tv_power: Option<TvPower>,
    /// Volume da TV, 0..=16 — o OSD de barrinhas e o `AudioOut` do jogo
    /// leem daqui (`volume()`). Persistência fica com o caller via
    /// `set_volume`.
    volume: u8,
    /// Até quando o OSD de volume fica na tela — a cada mudança o deadline
    /// renova; expirado, as barrinhas somem (como o CH 3 dos TVs reais).
    volume_osd_until: Option<Instant>,
    /// Os rects dos controles do queixo desenhados no último frame de
    /// estático — `(power, vol_down, vol_up)`, em coordenadas de output;
    /// `hit_tv_power`/`hit_volume` consultam.
    tv_power_buttons: Option<TvChinControls>,

    /// The idle screen's core-download prompt (plan revision): `Some(label)`
    /// draws a warning plus this button in the idle panel — the label is the
    /// app's own live status ("Baixar núcleo" / progress text). `None`
    /// restores the plain idle panel.
    idle_core_prompt: Option<String>,
    /// O modo dev (plan revision: "konami code na tela inicial do app ...
    /// habilitar botão em cima da configuração") — desligado até o app
    /// religar; ligado, o painel da tela inicial desenha um botão "Dev"
    /// logo acima do "Configurações". Session-only: nunca é persistido, o
    /// segredo não vaza para arquivo nenhum.
    dev_mode: bool,
    /// O painel de desempenho do modo dev ("infos de desempenho" no menu
    /// dev): "N FPS  CPU T  RAM M" no topo central da TV durante o jogo. Os
    /// números chegam por `set_perf_readout` (runner, 1×/s); desligado,
    /// nada é desenhado.
    perf_overlay: bool,
    perf_fps: u32,
    perf_cpu: Option<u32>,
    perf_ram_mb: Option<u32>,
    /// Linha de status do download do núcleo no painel da tela inicial
    /// (plan revision: "cliquei no icone verde de update do nucleo mas nao
    /// abriu nada") — o download é em segundo plano e o rótulo dele só
    /// existia na tela de setup; aqui o idle mostra "baixando… X MB",
    /// "núcleo atualizado!" ou o motivo da falha.
    core_status: Option<String>,
    /// The foley stream (app-supplied one-shots: insert/eject/power/reset,
    /// plan revision) — opened lazily on the first sound and kept alive for
    /// the process, because a stream dropped right after `queue` destroys
    /// the queued audio before the device ever plays it (the bug that made
    /// the first foley attempt silent).
    foley: Option<AudioOut>,
    /// The "CH 3" channel banner's deadline during gameplay (plan revision:
    /// "quando ligar o console, mostrar por 3 segundos e remover da tela")
    /// — `None` means not showing. The static path (`present_static`)
    /// always draws it instead: no signal, same as an old TV parked on
    /// channel 3.
    ch3_until: Option<Instant>,
    /// O canal em que a TV está — só 3 ou 4, como TV de tubo de verdade
    /// com o console no 3. CH-/CH+ do queixo alternam.
    tv_channel: u8,
    /// Where the cabinet actually drew last frame, in real window/output
    /// pixels — native for displays in the 16:10..16:9 band, else a centered
    /// 16:9 letterbox/pillarbox (plan: don't distort on an ultrawide).
    /// Every other stored rect (`screen`, `panel_buttons`, …) lives in this
    /// rect's own local space; `window_to_output` subtracts its offset
    /// before any hit-test runs.
    canvas_rect: Rect,
    /// The cabinet's own nameplate, printed into the chin by `draw_brand` on
    /// every screen (plan revision: "mostrar versao do app e do snes9x, onde
    /// esta o nome do app na tv") — starts as just `BRAND`, but the app sets
    /// it once at startup (and again after a core swap) to also carry the
    /// app/core version, via `set_nameplate`.
    nameplate: String,
    /// MOCK (design preview for the achievements notification — RetroAchievements
    /// plan): lines drawn right-aligned in the chin, mirroring `set_nameplate`'s
    /// block on the left. Throwaway scaffolding for `examples/ra_osd_mock.rs`;
    /// the real feature (phase 4) replaces it with a timed OSD queue.
    /// Per-nameplate-line "tem update" flags (plan revision: "quando tiver
    /// update do app ou do snes9x não mostrar mais a tela cheia e sim um
    /// icone verde no nameplate do lado de cada um") — `(app, core)`. When
    /// set, `draw_brand` prints a small green dot right after that line
    /// (line 0 = app version, line 1 = snes9x version); the app flips them
    /// via `set_nameplate_updates` once its startup check reports something.
    nameplate_updates: (bool, bool),
    /// Os rects das setas verdes de update no nameplate (app, core) —
    /// preenchidos a cada frame do caminho estático; o idle usa para
    /// abrir os fluxos de atualização (plan revision: "ao clicar no icone
    /// verde de atualizar, atualizar").
    update_arrows: (Option<Rect>, Option<Rect>),
    /// The chin's OSD queue (plan: `docs/plano-retroachievements.md`,
    /// fase 4) — "CONQUISTA DESBLOQUEADA" blocks drawn right-aligned in the
    /// chin, front entry only, expiring by time; the next one slides in
    /// after. Pushed by the runner on an unlock event.
    osd_queue: VecDeque<OsdEntry>,
    /// The chin's persistent RetroAchievements badge: when the account is
    /// on, the chin shows the RA mark + "ATIVADO" + the mode whenever the
    /// OSD queue is empty — an unlock notification still takes the spot
    /// while it lives, and the badge comes back when it expires. `None`
    /// draws nothing (account off, or no cartridge in).
    ra_status: Option<RaStatus>,
    /// Whether the top-left "fechar app" button is drawn/clickable this
    /// screen (plan revision: "criar botao de fechar app no canto superior
    /// esquerdo") — the idle/shelf/settings/history screens turn it on;
    /// gameplay never does (Desligar/Ejetar already cover backing out of a
    /// running game, and a stray click there quitting the whole app outright
    /// would be a much bigger surprise than on a menu screen). Explicitly
    /// set on every screen's own entry, not just left over from whatever ran
    /// before — same pattern `clear_panel`/`set_powered` already follow.

    /// Where the close button drew last frame, in output/canvas coordinates
    /// — `hit_close_button` scans this, same pattern as `panel_buttons`.
    /// Drawn on every screen now (plan revision: "mostrar o fechar e
    /// minimizar em todas as telas"), next to its minimize sibling.
    close_button: Rect,
    /// The top-left minimize button's rect drawn last frame —
    /// `hit_minimize_button` scans this.
    minimize_button: Rect,
}

/// One queued chin OSD block (plan fase 4) — the unlock notification with
/// its expiry.
struct OsdEntry {
    lines: Vec<String>,
    badge: Option<u64>,
    until: Instant,
}

/// The persistent RetroAchievements chin badge's state — just the session
/// mode; the drawing itself is fixed (`draw_chin_ra`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RaStatus {
    pub hardcore: bool,
}

/// What the chin draws this frame: a queued notification wins over the
/// persistent RA badge — which is what makes the badge "cede the spot" to
/// each unlock and come back when the notification expires.
enum ChinOsd {
    Notify {
        lines: Vec<String>,
        badge: Option<u64>,
    },
    Ra {
        hardcore: bool,
    },
    None,
}

/// Um controle físico do queixo da TV (ver `Cabinet::hit_tv_controls`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvControl {
    Power,
    VolUp,
    VolDown,
    Channel,
}

/// Os rects clicáveis do queixo da TV, registrados a cada frame (ver
/// `Cabinet::hit_tv_controls`): VOL-/VOL+, o botão CH (3↔4) e POWER —
/// da esquerda para a direita.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TvChinControls {
    pub vol_down: Rect,
    pub vol_up: Rect,
    pub ch: Rect,
    pub power: Rect,
}

impl TvChinControls {
    /// Todos fora da tela — o "sem controles" (queixo curto demais).
    pub fn offscreen() -> TvChinControls {
        let no = Rect::new(-100, -100, 1, 1);
        TvChinControls {
            vol_down: no,
            vol_up: no,
            ch: no,
            power: no,
        }
    }
}

/// O estado da animação da TV ligando/desligando (ver `Cabinet::tv_power`).
/// `frames` conta os frames desde o início da fase; cada fase sabe quantos
/// dura (60fps: o warm-up leva ~2s como um tubo de verdade, o colapso é
/// rápido — ~0.4s — e o afterglow apaga sozinho).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TvPower {
    /// Filamento acordando -> ponto -> linha -> raster abrindo. No fim,
    /// vira `None` (ligada) e a neve normal assume.
    Warming { frames: u32 },
    /// Imagem -> linha -> ponto -> afterglow decaindo. No fim, vira
    /// `Off`.
    Collapsing { frames: u32 },
    /// Desligada: vidro quase preto com reflexo, controles do queixo vivos.
    Off,
}

/// O que a fase do tubo manda desenhar NO VIDRO neste frame — a leitura
/// única que o caminho estático (neve) e o caminho de gameplay (imagem do
/// jogo) fazem, para o tubo se comportar EXATAMENTE igual com qualquer
/// sinal (plan revision: desligar a TV com o jogo rodando mata a imagem,
/// não o jogo). `Raster` carrega a máscara de transição: `open` 0..1 é a
/// abertura vertical do raster, `hot` o brilho do fósforo e `ponto` o
/// afterglow final (o único ponto quente no centro do tubo morto).
pub enum TvPhase {
    /// Ligada: o sinal que o caminho já desenha.
    On,
    /// Desligada: o vidro morto por cima de tudo.
    Dead,
    /// Transição (warm-up abrindo / colapso fechando) por cima do sinal.
    Raster { open: f32, hot: f32, ponto: bool },
}

/// O comprimento das fases do tubo em frames (60fps: o warm-up leva ~2s
/// como um tubo de verdade, o colapso é rápido — ~0.4s — e o afterglow
/// apaga sozinho). Compartilhadas pelo frame vivo (`tv_phase`) e pelos
/// captures (`tv_phase_snapshot`).
const TV_WARM_FRAMES: u32 = 120;
const TV_COLLAPSE_FRAMES: u32 = 26;
/// O flash do canal ao LIGAR a TV (plan revision: "quando religar a tv,
/// mostrar o CH 3 novamente por alguns segundos (até a tv entender que
/// esta dando sinal)") — o mesmo 3s do ligar o console (`CH3_FLASH` no
/// runner); some sozinho, o sinal fica limpo.
const TV_CH3_FLASH: Duration = Duration::from_secs(3);

/// A clickable spot in the side panel: the idle screen's "Inserir cartucho"
/// button, or one of the in-game console commands. `hit_panel_button` turns a
/// click into one of these; the caller (idle screen / `run_game`) decides
/// what each one does. `Hash` is for `runner`'s brief "done!" flash on
/// silent actions (screenshot, note capture, save/load) — keyed by button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelButton {
    Insert,
    Settings,
    /// The idle screen's dev-mode button (plan revision: "habilitar botão em
    /// cima da configuração") — only drawn while the app's dev mode is on,
    /// which only the Konami code on the idle screen can turn on. The dev
    /// menu itself lives app-side.
    Dev,
    /// The idle screen's "Baixar núcleo" button (plan revision: "ao iniciar
    /// o app pela primeira vez e/ou nao tiver o core na pasta, mostrar
    /// botão para baixar") — only drawn while the core is missing; the
    /// download itself lives app-side.
    CoreDownload,
    Power,
    Eject,
    Reset,
    /// Pauses and opens the notebook in one click (plan revision — replaces
    /// the old separate "Pausar" button; "Nota" moved to `PrintScreen`).
    Notebook,
    /// Opens the achievements list modal (plan:
    /// `docs/plano-retroachievements.md`, fase 4) — only drawn while an RA
    /// session is active for the loaded game.
    Achievements,
    /// Opens the achievements list on the shelf (plan fase 2/4) — inside
    /// the tube, like the back-cover zoom. Only drawn while the focused
    /// game has an identified RA set.
    ShelfAchievements,
    /// Grabs the current frame and opens a modal to pick which of the 15
    /// note slots to save it into, with a name (plan revision — replaces
    /// the old direct-capture "Nota" button and its slot cycler).
    PrintScreen,
    /// Opens a modal to pick which of the save-state slots to write into
    /// (plan revision — used to save straight into a pre-cycled slot).
    SaveState,
    /// Opens a modal to pick which save-state slot to load, mirroring
    /// `SaveState`.
    LoadState,
    /// Opens the Cheats modal (plan revision — split out of the notebook,
    /// which had no natural home for a checklist next to the photo album).
    /// Absent from the panel entirely for a cartridge with no curated
    /// cheats, same "hide rather than show an empty screen" rule as the
    /// other modal-opening buttons.
    Cheats,
    /// One slot row inside whichever modal (`SaveState`/`LoadState`/
    /// `PrintScreen`) is open right now, or one cheat row inside the Cheats
    /// modal (plan revision — same click addresses both a pick and a
    /// toggle; which it means depends on which modal is open). `u16`, not
    /// `u8` (plan revision): a database-heavy game's Cheats list can run
    /// well past 255 rows.
    ModalSlot(u16),
    /// Confirm the modal's text step (naming a print).
    ModalConfirm,
    /// Back out of whichever modal is open, discarding any choice so far.
    ModalCancel,
    /// Scroll the modal's row grid up/down by one page — only drawn (and
    /// so only clickable) once the row count overflows the card.
    ModalScrollUp,
    ModalScrollDown,
    /// Click the search box on a searchable modal (plan revision — today
    /// only Cheats) to start typing a filter.
    ModalSearchStart,
    /// On a searchable modal (today only Cheats): show every row, only the
    /// ones checked on, or only the ones off (plan revision) — a segmented
    /// three-way control, one of the three always "active" (`lit`).
    ModalFilterAll,
    ModalFilterOn,
    ModalFilterOff,
    /// Drawn only on the pause book screen (not the side panel): resume play.
    PauseContinue,
    /// Pause book: step the right page (the photo album) to an earlier/
    /// later print slot.
    PauseNotePrev,
    PauseNoteNext,
    /// Pause book: open the editor for the left page's currently-shown
    /// text-note slot (plan revision — used to always append a new, blank
    /// free-text page; now edits whichever of the 15 slots is showing,
    /// pre-filled with its saved content).
    PauseWrite,
    /// Pause book, while writing: commit the draft to the notebook.
    PauseDraftSave,
    /// Pause book, while writing: discard the draft, back to the status view.
    PauseDraftCancel,
    /// Pause book: toggle whether the right page's shown print slot is
    /// protected from being overwritten by a future Printscreen capture.
    PauseNotePin,
    /// Pause book: open the editor for the shown print slot's caption
    /// (plan revision) — same text-editor UI as the text-note slots, a
    /// shorter limit and a different destination.
    PauseNoteName,
    /// Pause book: step the left page (the 15 text-note slots, plan
    /// revision) to an earlier/later one.
    PauseTextPrev,
    PauseTextNext,
    /// Pause book: toggle whether the left page's shown text-note slot is
    /// protected from a future overwrite *or delete* (plan revision).
    PauseTextPin,
    /// Pause book: clear the left page's shown text-note slot (plan
    /// revision) — dimmed/unclickable while it's pinned.
    PauseTextDelete,
}

/// A clickable spot on the shelf's own flat info panel (plan revision: the
/// shelf used to draw its details column *inside* the warped screen buffer
/// alongside the grid — "o painel nao pode estar dentro da TV" — so it now
/// sits flat next to the tube, like the in-game side panel, with its own
/// tiny button set). Separate from `PanelButton`: the shelf isn't a loaded
/// game, so none of that enum's meaning applies here.
/// As seções do corpo rolável do painel da estante (plan revision: "o cima
/// e baixo no painel faça assim os botões: capa traseira, cartucho e
/// informações") — cada botão de salto leva o corpo ao primeiro bloco da
/// seção.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PanelSection {
    CapaTraseira,
    Cartucho,
    Informacoes,
    /// A primeira página do manual (plan revision: "mostrar primeira
    /// pagina do manual igual mostra o back cover e cartucho / ao clicar
    /// abrir nele visualização").
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShelfButton {
    Back,
    Settings,
    /// Pula o corpo do painel para uma seção (plan revision: no lugar das
    /// antigas setas "^ Cima"/"v Baixo", três botões nomeados) — desenhados
    /// só enquanto a arte/info do jogo focused transborda o painel; o índice
    /// de destino o caller calcula (ele tem o [`ShelfPanelInfo`]).
    PanelJump(PanelSection),
    /// The focused game's back cover, drawn in the panel — clicking it opens
    /// the enlarged view (plan revision: "ao clicar no back cover
    /// possibilitar mostrar em tamanho maior, com botão de fechar").
    Backcover,
    /// O cartucho desenhado na aba dele (plan revision: "colocar zoom igual
    /// ao backcover no cartucho") — mesmo comportamento: clique abre a
    /// arte ampliada, "voltar"/Back fecha.
    Cartridge,
    /// Abrir o manual em PDF do jogo (plan revision: "criar botão no painel
    /// para abrir o manual") — só desenhado quando há PDF na pasta de
    /// manuais; o leitor vive no tubo, lado estante.
    Manual,
    /// Toggle the focused game's favorite marker (plan revision: "adicionar
    /// marcador de favorito nos jogos") — drawn by `draw_shelf_panel` when
    /// `ShelfPanelInfo::favorite` is `Some`.
    ToggleFavorite,
    /// Open the achievements list view (plan: `docs/plano-retroachievements
    /// .md`, fase 2/4) — drawn by `draw_shelf_panel` when
    /// `ShelfPanelInfo::achievements` is set.
    ShelfAchievements,
    /// "Atualizar" (plan revision: "adicionar opção de atualizar a estante
    /// para buscar jogos novos sem precisar abrir e fechar o app") —
    /// re-scan `roms/` and rebuild the shelf.
    Refresh,
}

/// A clickable spot on the settings screen's flat panel (plan revision: "a
/// tela de configurações não está no padrão do resto do app — faça a tv na
/// mesma proporção e coloque o painel") — the sections live in the panel,
/// like the shelf's own buttons do. `Section(i)` switches the settings list
/// shown inside the tube; `Back` leaves settings entirely.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsButton {
    Section(usize),
    Back,
}

/// Qual seta de update do nameplate foi clicada (plan revision: "ao
/// clicar no icone verde de atualizar (tanto do app quanto o nucleo),
/// atualizar").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UpdateArrow {
    /// A seta da linha do próprio app — abre o changelog + botão atualizar.
    App,
    /// A seta da linha do snes9x — baixa e instala o core na hora.
    Core,
}

/// The settings screen's flat panel content — one button per section plus
/// the always-present "Voltar". Drawn by `draw_settings_panel`, hit-tested
/// through `hit_settings_button`; settings.rs sets it every frame like the
/// shelf does with [`ShelfPanelInfo`].
pub struct SettingsPanelInfo {
    pub title: String,
    /// Section labels, top to bottom — `Section(i)` indexes into this.
    pub sections: Vec<String>,
    /// Which section is currently shown in the tube (button drawn lit).
    pub selected: usize,
}

/// The pause book: `captures` is the fixed slot count (plan revision, 15),
/// shared by both pages — the right page's photo album and the left page's
/// text notes (plan revision: the left page used to be a status line plus
/// an always-blank "write a new page" button over an unbounded, unreadable
/// append log; now it's a slot viewer just like the right page, since
/// "I wrote something and can't see it again" was a real complaint).
struct PauseNote {
    title: String,
    /// Total slots per page (plan revision: a fixed 1..=15 for both the
    /// photo album and the text notes) — the pagination bound both share.
    captures: usize,
    /// 0-based index of the print slot currently shown on the right page.
    page: usize,
    has_thumb: bool,
    /// Whether the shown print slot is protected from a future
    /// Printscreen overwrite (plan revision), and its caption if one was
    /// given — both empty/false for a slot nobody's touched yet.
    pinned: bool,
    slot_label: String,
    /// 0-based index of the text-note slot currently shown on the left
    /// page (plan revision) — independent of `page`, the right page's own
    /// cursor.
    text_page: usize,
    /// Whether the shown text-note slot is protected from a future
    /// overwrite *or delete* (plan revision) — same meaning `pinned` has
    /// for a print slot.
    text_pinned: bool,
    /// The shown text-note slot's saved content, or `None` for an empty
    /// one (plan revision) — this is the whole point of the left page now:
    /// showing back what was actually written there.
    text_content: Option<String>,
    /// `Some(text)` while the player is editing something (plan revision:
    /// either a text-note slot or a print's caption — `draft_heading`
    /// says which): replaces the left page's content with a live, editable
    /// draft. `draft_limit` is the max character count `text` may reach —
    /// shown as a counter alongside it.
    draft: Option<String>,
    draft_limit: usize,
    draft_heading: String,
}

/// One clickable row in a `ModalInfo`'s slot grid — `enabled` false dims it
/// the same way `draw_button` dims anything that wouldn't do anything right
/// now (e.g. "Carregar" on an empty slot, or a pinned print slot).
struct ModalRow {
    label: String,
    enabled: bool,
}

/// A modal dialog (plan revision) — save/load-state, "which slot for this
/// print", and the Cheats checklist: a title, a grid of rows, and a Cancel
/// button; lighter than the two-page pause book, since picking (or, for
/// Cheats, toggling) a row is the whole job here. Once a print's slot is
/// picked, `draft` replaces the row grid with a text field (name it) plus
/// Confirm/Cancel — same idea as the pause book's own draft mode
/// (`PauseNote::draft`), just a lighter home for it.
struct ModalInfo {
    title: String,
    rows: Vec<ModalRow>,
    /// Index of the first visible row-of-columns (plan revision — a
    /// database-heavy Cheats list can run to thousands of rows, so
    /// `draw_modal` only ever renders a page of them at a time). Preserved
    /// across a `set_modal` call that keeps the dialog open (Cheats
    /// refreshing its own checkmarks after a toggle) so a click deep in a
    /// long list doesn't jump the view back to the top; reset to 0 only on
    /// a fresh open (see `Cabinet::set_modal`) or a new search filter (see
    /// `Cabinet::set_modal_search`).
    scroll: usize,
    /// Whether this modal offers a search box at all (plan revision — only
    /// Cheats does; Save/Load/Print's lists are always small enough that
    /// hunting for one by eye is fine). Set once by whoever opens the
    /// modal, alongside `set_modal`.
    searchable: bool,
    /// Current filter text (plan revision) — empty means "show every row".
    /// A row matches when its label contains this, case-insensitively.
    /// Preserved across `set_modal` the same way `scroll` is.
    search_query: String,
    /// On/off state filter (plan revision, Cheats-only like `search_query`
    /// — drawn only when `searchable`): `None` shows every row, `Some(true)`
    /// only the ones checked on, `Some(false)` only the ones off. Derived
    /// per-row from the `[x] `/`[ ] ` prefix `cheat_modal_rows` already
    /// encodes into the label for display — see `row_checked` — rather
    /// than threading a second parallel array through `set_modal` just for
    /// this. Preserved across `set_modal` the same way `search_query` is.
    cheat_filter: Option<bool>,
    draft: Option<String>,
    draft_limit: usize,
    draft_heading: String,
}

/// What to draw at the top of the side panel: the `wheel` logo if we have
/// it, else the ROM's title, plus cartridge art right below when there's a
/// local file for it (plan revision — both are optional, independent of
/// each other). `commands` is the button legend (plan §3.2, item 3), one
/// clickable row per entry, rebuilt every frame by the caller since several
/// labels are live state (e.g. a "(feito!)" flash) — see
/// `Cabinet::set_commands`. `cheats` (item 4, plan §4.4) is
/// informational-only here now — `(description, on)` pairs, only the ones
/// that are on get drawn, plain text; toggling lives in its own Cheats modal
/// (plan revision), reachable from a `commands` row like any other.
struct PanelInfo {
    has_logo: bool,
    has_cartridge: bool,
    title: String,
    commands: Vec<(PanelButton, String)>,
    /// Whether the console is on right now — dims/brightens the command
    /// buttons (Eject only clunks while powered, Reset and everything past
    /// it only act while powered), and which way the Power rocker sits. Set
    /// via `Cabinet::set_powered`; starts `false` (plan revision: picking a
    /// game only inserts the cartridge, it doesn't start it — the player
    /// presses Power themselves, same as the real console).
    powered: bool,
    /// Reset's rocker is momentary (plan revision): true for a short spring
    /// window right after a click, then the caller (`runner`) lets it lapse —
    /// see `Cabinet::set_reset_pressed`. Power's rocker has no such flag; its
    /// position is just `powered` itself (a real toggle, stays where left).
    reset_pressed: bool,
    cheats: Vec<(String, bool)>,
    /// Notebook block (item 5, plan §3.4): absent entirely when this is 0 —
    /// no "no notes" filler.
    note_count: usize,
    has_note_thumb: bool,
    /// A pinned text-note slot's content (plan revision — independent of
    /// the photo thumbnail above: either, both, or neither can be showing
    /// at once, "mostrar apenas uma nota e/ou uma imagem"), already
    /// snippet-length by the time it gets here (see `runner::
    /// panel_text_snippet`) — the panel itself doesn't truncate anything.
    text_note: Option<String>,
    /// Insert/eject progress for the panel's cartridge slot (plan revision)
    /// — `None` is the resting seated state every normal frame draws;
    /// `Some((t, ejecting))` while `runner`'s animation drives it, drawn by
    /// `draw_panel_slot`. Reset to `None` by `set_panel`.
    cartridge_motion: Option<(f32, bool)>,
}

/// Uma barra do bloco HowLongToBeat do painel — o rótulo dim em cima
/// ("história principal") e a barra colorida com o tempo dentro, largura
/// proporcional ao duration (`frac` 0..1 contra o maior tempo do jogo; o
/// desenho impõe um mínimo visível).
#[derive(Clone)]
pub struct HltbBar {
    pub label: String,
    /// O tempo formatado ("1h 59min") — escrito dentro da barra quando
    /// cabe, logo depois dela quando não cabe.
    pub text: String,
    pub frac: f32,
    pub color: (u8, u8, u8),
}

/// O bloco HowLongToBeat da aba informações — as 4 barras de tempo (como o
/// card do site: história, + extras, completista, todos os estilos) sob um
/// cabeçalho com o ícone "H". `icon` é a textura registrada pelo caller via
/// `Cabinet::set_image`; sem ela o cabeçalho fica só com o texto.
#[derive(Clone)]
pub struct HltbBars {
    pub icon: u64,
    pub rows: Vec<HltbBar>,
}

/// A seção de nota da IGDB na aba informações — cabeçalho com a logo e o
/// valor centrado. `icon` é a textura registrada pelo caller.
#[derive(Clone)]
pub struct IgdbSection {
    pub icon: u64,
    /// A nota formatada ("68 / 100").
    pub nota: String,
}

/// What the shelf's flat side panel shows for the currently-selected game
/// (plan revision — moved out of the warped screen buffer so its text reads
/// crisp and its logo doesn't bow with the tube curvature, same as the
/// in-game panel already did). `logo_img`/`cartridge_img`/`backcover_img`
/// are texture ids the shelf already cached via `Cabinet::set_image` (its
/// own per-game local art, hashed from the ROM's sha1) — `None` falls back
/// to the title in text for the logo, and simply draws nothing for the
/// other two (plan revision: "abaixo da logo... colocar o back cover
/// tambem" — a second, independent local image under the cartridge art,
/// same `assets/<kind>/<rom>.*` convention, just its own folder,
/// `assets/backcover/`). `release` is the game's release year from the
/// No-Intro DAT, if one was loaded and had it (plan revision — replaces the
/// play count here; `info` is whatever else the DAT carried beyond the bare
/// title, plus the shelf's own file/play facts ("se o DAT tiver
/// informacoes do jogo, preencher no painel") — label/value pairs, empty
/// when the DAT has nothing extra or wasn't loaded at all.
#[derive(Clone)]
pub struct ShelfPanelInfo {
    pub title: String,
    pub logo_img: Option<u64>,
    pub cartridge_img: Option<u64>,
    pub backcover_img: Option<u64>,
    pub release: Option<String>,
    pub info: Vec<(String, String)>,
    /// A seção ativa do corpo (plan revision: "a ideia é mostrar um info
    /// de cada vez mesmo em resolução grande, mudar ao clicar") — o corpo
    /// vira abas: só os blocos da seção escolhida são desenhados, e os três
    /// botões do topo trocam a seção.
    pub section: PanelSection,
    /// How many of the scrollable blocks below the logo/title header
    /// (back cover, cartridge, release, `info`) to skip before drawing —
    /// the caller's own running counter (plan revision: "criar rolagem no
    /// painel quando necessario"), reset to 0 whenever the focused game
    /// changes. `draw_shelf_panel` clamps this itself, so an over-large
    /// value (scrolled past the end) is harmless.
    pub scroll: usize,
    /// The focused game's favorite marker (plan revision: "adicionar
    /// marcador de favorito nos jogos") — `Some(false)` draws a
    /// "Favoritar" button, `Some(true)` a "Remover favorito" one (both
    /// `ShelfButton::ToggleFavorite`), `None` no button at all (history /
    /// empty panel).
    pub favorite: Option<bool>,
    /// The focused game is identified on RetroAchievements (plan fase 2) —
    /// draws a "Conquistas" button (`ShelfButton::ShelfAchievements`)
    /// between "Favoritar" and "Configurações".
    pub achievements: bool,
    /// A primeira página do manual em PDF (plan revision: o corpo da aba
    /// manual a mostra como as artes, e o clique nela abre o leitor).
    /// `None` = sem manual para o jogo.
    pub manual_first_page: Option<u64>,
    /// O motivo pelo qual o botão Conquistas aparece **desativado** (plan
    /// revision: "avisar o usuário com tooltip no botão conquistas,
    /// mostrar mas deixar desativado") — o RA marcou o hash como versão
    /// não suportada, então o botão desenha apagado com o motivo no
    /// rótulo e o clique nele não faz nada. `None` = destravado.
    pub achievements_reason: Option<String>,
    /// A medalha de prêmio do jogo (plan fase 4) — textura já registrada
    /// pelo caller (`set_image`), desenhada à esquerda do número da linha
    /// "conquistas".
    pub award_img: Option<u64>,
    /// O bloco HowLongToBeat (as 4 barras de tempo com o ícone "H") no fim
    /// da aba informações — `None` e o jogo não ganha bloco nenhum (o site
    /// não conhece o jogo; tabela embutida por CRC32, nada de rede).
    pub hltb: Option<HltbBars>,
    /// A seção de nota da IGDB (logo + valor) na aba informações — `None`
    /// e o jogo não ganha seção (sem nota na IGDB para ele).
    pub igdb: Option<IgdbSection>,
}

/// Medalha + respiro antes do número — compartilhados pela medida da
/// altura e pelo desenho de `PanelBlock::RaField`.
const AWARD_MEDAL_W: i32 = 12;
const AWARD_MEDAL_H: i32 = 15;
const AWARD_MEDAL_GAP: i32 = 5;

struct SrcTexture {
    tex: Texture,
    w: u32,
    h: u32,
    format: PixelFormat,
}

/// A plain RGBA texture kept at a known size.
struct SizedTex {
    tex: Texture,
    w: u32,
    h: u32,
}

struct CrtMesh {
    verts: Vec<Vertex>,
    indices: Vec<i32>,
    w: u32,
    h: u32,
}

/// The chamfered ring from the window edge to the recessed screen.
struct BezelMesh {
    verts: Vec<Vertex>,
    indices: Vec<i32>,
    /// Cache key: (window w, window h, screen w, screen h).
    key: (u32, u32, u32, u32),
}

struct ImgTex {
    tex: Texture,
    w: u32,
    h: u32,
    /// The opaque content's bounding box inside the texture, in texture
    /// pixels — the art's transparent margins excluded (`content_bbox`).
    /// The cartridge slot scene fits and seats the cartridge by this box, so
    /// the cart's actual body — not its empty canvas — spans the base and
    /// meets the slot mouth.
    content: (u32, u32, u32, u32),
    /// A arte tem transparência de verdade (algum pixel com alpha abaixo de
    /// 250 — render 3D em ângulo, fundo vazado). Carregada NA textura, não
    /// num registro paralelo: a textura sobrevive entre visitas à estante,
    /// e a flag tem que sobreviver junto.
    vazado: bool,
}

/// The bounding box of pixels with any alpha, in texture pixels — an
/// image-sized box when the art has no transparency at all. Scans at most
/// one pass over the RGBA buffer (art textures are loaded once).
fn content_bbox(w: u32, h: u32, rgba: &[u8]) -> (u32, u32, u32, u32) {
    let (mut min_x, mut min_y) = (w, h);
    let (mut max_x, mut max_y) = (0u32, 0u32);
    let mut seen = false;
    for y in 0..h {
        for x in 0..w {
            if rgba[((y * w + x) * 4 + 3) as usize] > 16 {
                seen = true;
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    if !seen {
        return (0, 0, w, h);
    }
    (min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
}

impl Cabinet {
    pub(crate) fn new(
        video: &VideoSubsystem,
        audio: &AudioSubsystem,
        title: &str,
        width: u32,
        height: u32,
        fullscreen: bool,
    ) -> Result<Self, PlatformError> {
        // Fullscreen from birth, when requested (plan revision: "tem como
        // fazer abrir direto na forma correta?") — e nascendo **oculta**: o
        // macOS ainda mostrava um frame do tamanho bruto 1280×800 antes de
        // aplicar o fullscreen (com a transição nativa de zoom por cima), o
        // que lia como "abre de um tamanho e dá uma aumentada". Oculta +
        // fullscreen desde a criação, a janela só aparece quando já está no
        // formato final.
        let mut builder = video.window(title, width, height);
        builder.position_centered().resizable();
        if fullscreen {
            builder.fullscreen().hidden();
        }
        let window = builder
            .build()
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let mut canvas = window.into_canvas();
        log::info!(
            "video: {} | renderer: {}",
            video.current_video_driver(),
            canvas.renderer_name
        );
        canvas.set_blend_mode(BlendMode::Blend);
        canvas.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        canvas.clear();
        canvas.present();
        if fullscreen {
            // Já preparada (fullscreen aplicado, primeiro frame pronto),
            // aparece sem tamanho intermediário.
            canvas.window_mut().show();
        }

        let font = build_font_atlas(&mut canvas)?;
        let (w, h) = canvas.output_size().unwrap_or((width, height));
        let canvas_rect = cabinet_canvas_rect(w, h);
        Ok(Self {
            canvas,
            src: None,
            mesh: None,
            fade_mesh: None,
            bezel: None,
            screen_tex: None,
            noise_tex: None,
            noise: Vec::new(),
            noise_stamp: None,
            rng: 0x9E37_79B9,
            font,
            images: HashMap::new(),
            screen: screen_area(canvas_rect.width(), canvas_rect.height()),
            panel: None,
            session: Duration::ZERO,
            pause: None,
            modal: None,
            fullscreen: false,
            panel_buttons: Vec::new(),
            pause_buttons: Vec::new(),
            modal_buttons: Vec::new(),
            shelf_panel: None,
            shelf_buttons: Vec::new(),
            settings_panel: None,
            settings_buttons: Vec::new(),
            audio: audio.clone(),
            idle_core_prompt: None,
            dev_mode: false,
            perf_overlay: std::env::var_os("SIXTEEN_PERF").is_some(),
            perf_fps: 0,
            perf_cpu: None,
            perf_ram_mb: None,
            core_status: None,
            static_hiss: false,
            hiss: None,
            hiss_rng: 0x2545_f491,
            tv_power: None, // abre com a TV ligada, como o PolyStnX
            volume: 8,
            volume_osd_until: None,
            tv_power_buttons: None,
            foley: None,
            ch3_until: None,
            tv_channel: 3,
            canvas_rect,
            nameplate: BRAND.to_string(),
            nameplate_updates: (false, false),
            update_arrows: (None, None),
            osd_queue: VecDeque::new(),
            ra_status: None,
            close_button: Rect::new(0, 0, 0, 0),
            minimize_button: Rect::new(0, 0, 0, 0),
        })
    }

    /// Replace the cabinet's nameplate text (plan revision) — the app calls
    /// this once at startup with its own version, and again whenever the
    /// installed snes9x core changes (a download/update via settings).
    pub fn set_nameplate(&mut self, text: &str) {
        self.nameplate = text.to_string();
    }

    /// Which nameplate lines get the green "tem update" dot — `(app, core)`,
    /// matching the block `set_nameplate` stacked (`draw_brand` draws the dot
    /// right after line 0 / line 1, respectively). `false, false` clears both.
    pub fn set_nameplate_updates(&mut self, app: bool, core: bool) {
        self.nameplate_updates = (app, core);
    }

    /// Qual seta verde de update do nameplate um ponto (output) acerta —
    /// rects preenchidos pelo caminho estático (`present_static`), onde o
    /// nameplate é clicável (idle). (plan revision: "ao clicar no icone
    /// verde de atualizar".)
    pub fn hit_update_arrow(&self, ox: i32, oy: i32) -> Option<UpdateArrow> {
        let (app, core) = self.update_arrows;
        if app.is_some_and(|r| r.contains_point((ox, oy))) {
            Some(UpdateArrow::App)
        } else if core.is_some_and(|r| r.contains_point((ox, oy))) {
            Some(UpdateArrow::Core)
        } else {
            None
        }
    }

    /// MOCK (design preview for the achievements notification — see
    /// `demo_osd`): shows a line block in the chin's right side; an empty
    /// slice clears it.
    /// Arm/disarm the persistent RetroAchievements chin badge — the runner
    /// calls it with the live session's mode on cart insert and with `None`
    /// when the cartridge leaves (or the account is off). No visual effect
    /// while the OSD queue has a live notification: that draws first.
    pub fn set_ra_status(&mut self, status: Option<RaStatus>) {
        self.ra_status = status;
    }

    pub fn push_osd(&mut self, lines: &[&str], badge: Option<u64>, ttl: Duration) {
        if lines.is_empty() {
            return;
        }
        self.osd_queue.push_back(OsdEntry {
            lines: lines.iter().map(|l| l.to_string()).collect(),
            badge,
            until: Instant::now() + ttl,
        });
    }

    /// The idle panel's core prompt (plan revision: "avisar que para jogar é
    /// necessário o download do core") — `Some(button_label)` shows the
    /// warning line plus a "CoreDownload" button; `None` hides both. Only
    /// ever rendered on the idle (panel-less) screen.
    pub fn set_idle_core_prompt(&mut self, label: Option<&str>) {
        self.idle_core_prompt = label.map(|l| l.to_string());
    }

    /// O modo dev (plan revision: "konami code na tela inicial do app") —
    /// ligado só pelo app, ao reconhecer o código; enquanto ligado, o painel
    /// da tela inicial desenha o botão "Dev" acima do "Configurações".
    /// Nunca é persistido nem lido de arquivo: o segredo vive só na sessão.
    pub fn set_dev_mode(&mut self, on: bool) {
        self.dev_mode = on;
    }

    /// O estado atual do modo dev — o app consulta para saber se a sessão
    /// já o desbloqueou (o botão em si o Cabinet desenha sozinho).
    pub fn dev_mode(&self) -> bool {
        self.dev_mode
    }

    /// Liga/desliga o painel de desempenho do modo dev (o menu dev alterna;
    /// o estado mora aqui e sobrevive estante ↔ jogo).
    pub fn set_perf_overlay(&mut self, on: bool) {
        self.perf_overlay = on;
    }

    /// O painel de desempenho está ligado?
    pub fn perf_overlay(&self) -> bool {
        self.perf_overlay
    }

    /// Alimenta o painel: fps medido pelo runner (janela de 1 s) e as
    /// leituras de sistema (°C, MB — `None` = indisponível, o segmento sai
    /// da linha).
    pub fn set_perf_readout(&mut self, fps: u32, cpu: Option<f32>, ram_mb: Option<f32>) {
        self.perf_fps = fps;
        self.perf_cpu = cpu.map(|c| c.round() as u32);
        self.perf_ram_mb = ram_mb.map(|m| m.round() as u32);
    }

    /// A linha de status do download do núcleo no painel do idle ("baixando…
    /// X MB" / "núcleo atualizado!" / motivo da falha) — `None` não desenha
    /// nada. O app atualiza a cada frame enquanto o download vive.
    pub fn set_core_status(&mut self, status: Option<&str>) {
        self.core_status = status.map(|s| s.to_string());
    }

    pub fn toggle_fullscreen(&mut self) {
        self.fullscreen = !self.fullscreen;
        let _ = self.canvas.window_mut().set_fullscreen(self.fullscreen);
    }

    /// The underlying window — `Platform::start_text_input`/`stop_text_input`
    /// need it (SDL's text-input API is per-window).
    pub(crate) fn window(&self) -> &sdl3::video::Window {
        self.canvas.window()
    }

    /// Convert a click's window coordinates (what SDL reports) into the
    /// cabinet's own local space — what `panel_rect`/`draw_panel`/`screen`
    /// all lay out in. Two corrections stack here: window → real output
    /// pixels (they differ on a HiDPI display), then output → cabinet-local
    /// (subtracting `canvas_rect`'s offset — on an ultrawide window the
    /// cabinet is letterboxed, not the whole output). A point outside
    /// `canvas_rect` (in the letterbox bars) still comes back as a
    /// coordinate — callers bounds-check against their own rects, e.g.
    /// `hit_panel_button`/`hit_screen_point`.
    pub fn window_to_output(&self, x: i32, y: i32) -> (i32, i32) {
        let (ww, wh) = self.canvas.window().size();
        let (ow, oh) = self.canvas.output_size().unwrap_or((ww, wh));
        let sx = ow as f32 / ww.max(1) as f32;
        let sy = oh as f32 / wh.max(1) as f32;
        let (ox, oy) = ((x as f32 * sx) as i32, (y as f32 * sy) as i32);
        (ox - self.canvas_rect.x(), oy - self.canvas_rect.y())
    }

    /// Like [`Cabinet::window_to_output`], but in the 2D screen buffer's own
    /// coordinate space (what `frame_2d`'s `draw` closure paints in — the
    /// screen is inset by the bezel inside the cabinet canvas). A screen
    /// drawn with hand-laid buttons hit-tests clicks by pairing this with
    /// its `Screen::size` layout.
    pub fn window_to_screen(&self, x: i32, y: i32) -> (i32, i32) {
        let (ox, oy) = self.window_to_output(x, y);
        (ox - self.screen.x(), oy - self.screen.y())
    }

    /// Show/hide the top-left "fechar app" button (plan revision) — call
    /// once on entering a screen that should offer it (idle/shelf/settings/
    /// history), and with `false` on entering one that shouldn't
    /// (gameplay) — the flag has no default that fits every screen, so it's
    /// never implicitly reset between them.
    pub fn set_close_button(&mut self, _show: bool) {
        // The fechar/minimizar pair is drawn on every screen now (plan
        // revision); kept as a no-op so existing callers keep compiling.
    }

    /// O controle do queixo sob um ponto de output — `Power`, `VolUp`,
    /// `VolDown` ou `Channel`. Disponível
    /// enquanto a TV está mostrando estático (o idle); o jogo usa os
    /// controles do painel.
    pub fn hit_tv_controls(&self, ox: i32, oy: i32) -> Option<TvControl> {
        let c = self.tv_power_buttons?;
        if c.power.contains_point((ox, oy)) {
            Some(TvControl::Power)
        } else if c.vol_up.contains_point((ox, oy)) {
            Some(TvControl::VolUp)
        } else if c.vol_down.contains_point((ox, oy)) {
            Some(TvControl::VolDown)
        } else if c.ch.contains_point((ox, oy)) {
            Some(TvControl::Channel)
        } else {
            None
        }
    }

    /// Whether an output-space point lands on the close button drawn last
    /// frame — always `false` while `set_close_button(false)` is in effect,
    /// even if a stale rect from an earlier screen is still sitting in
    /// `close_button`. Coordinates from a click go through
    /// `window_to_output` first.
    pub fn hit_close_button(&self, out_x: i32, out_y: i32) -> bool {
        self.close_button.contains_point((out_x, out_y))
    }

    /// Whether a click (already mapped by `window_to_output`) landed on the
    /// top-left minimize button. Screens call `minimize()` when this hits.
    pub fn hit_minimize_button(&self, out_x: i32, out_y: i32) -> bool {
        self.minimize_button.contains_point((out_x, out_y))
    }

    /// Iconify the window (plan revision: "botão de minimizar ao lado de
    /// fechar") — the app keeps running; restoring is the user's click on
    /// the Dock/taskbar, like any OS window.
    pub fn minimize(&mut self) {
        let _ = self.canvas.window_mut().minimize();
    }

    /// Which panel button, if any, sits under an output-space point — the
    /// idle screen's "Inserir cartucho" or one of the in-game commands drawn
    /// last frame. Coordinates from a click go through `window_to_output`
    /// first.
    pub fn hit_panel_button(&self, out_x: i32, out_y: i32) -> Option<PanelButton> {
        self.panel_buttons
            .iter()
            .find(|(_, r)| r.contains_point((out_x, out_y)))
            .map(|(b, _)| *b)
    }

    /// Same as `hit_panel_button`, for the pause book's own buttons — a
    /// separate list since the pause screen replaces the whole window
    /// instead of sharing it with the side panel (see `pause_buttons`).
    pub fn hit_pause_button(&self, out_x: i32, out_y: i32) -> Option<PanelButton> {
        self.pause_buttons
            .iter()
            .find(|(_, r)| r.contains_point((out_x, out_y)))
            .map(|(b, _)| *b)
    }

    /// Same as `hit_panel_button`, for the shelf's own flat panel (plan
    /// revision) — a separate list since the shelf isn't a loaded game.
    pub fn hit_shelf_button(&self, out_x: i32, out_y: i32) -> Option<ShelfButton> {
        self.shelf_buttons
            .iter()
            .find(|(_, r)| r.contains_point((out_x, out_y)))
            .map(|(b, _)| *b)
    }

    /// Show the shelf's flat side panel content for whatever game is
    /// selected right now (plan revision) — rebuilt every frame the
    /// selection might have changed, same as the shelf already rebuilds its
    /// own grid draw closure; the struct is small enough that this is cheap.
    pub fn set_shelf_panel(&mut self, info: ShelfPanelInfo) {
        self.shelf_panel = Some(info);
    }

    /// Settings screen's flat panel content — see [`SettingsPanelInfo`]
    /// (plan revision: settings now lives on the shelf's layout, tube +
    /// panel, and the panel carries the section buttons).
    pub fn set_settings_panel(&mut self, info: SettingsPanelInfo) {
        self.settings_panel = Some(info);
    }
    pub fn hit_settings_button(&self, out_x: i32, out_y: i32) -> Option<SettingsButton> {
        self.settings_buttons
            .iter()
            .find(|(_, r)| r.contains_point((out_x, out_y)))
            .map(|(b, _)| *b)
    }

    /// Map an output/canvas-space click into the 2D screen buffer's local
    /// coordinates — what `Screen::fill`/`text`/`image_fit` see, e.g. in the
    /// shelf's grid (plan §3.1). `None` outside the tube. Approximate: the
    /// buffer is warped onto the tube by `build_crt_mesh` (a mild barrel bow,
    /// `CRT_WARP`), which this ignores — a click right at the curved edge can
    /// land a few px off, but shelf tiles sit well inside it.
    pub fn hit_screen_point(&self, out_x: i32, out_y: i32) -> Option<(i32, i32)> {
        let (lx, ly) = (out_x - self.screen.x(), out_y - self.screen.y());
        if lx < 0 || ly < 0 || lx >= self.screen.width() as i32 || ly >= self.screen.height() as i32
        {
            return None;
        }
        let (sw, sh) = (self.screen.width() as f32, self.screen.height() as f32);
        // Displaced grid coords the click arrived at (-1..1 per axis).
        let dx = (lx as f32 / sw) * 2.0 - 1.0;
        let dy = (ly as f32 / sh) * 2.0 - 1.0;
        // Recover the undisplaced ones: forward is cx' = cx*(1-W*cy^2),
        // cy' = cy*(1-W*cx^2) — divide out the partner axis' factor until it
        // stops moving (sub-pixel after three rounds; eight for free).
        let (mut cx, mut cy) = (dx, dy);
        for _ in 0..8 {
            cy = dy / (1.0 - CRT_WARP * cx * cx);
            cx = dx / (1.0 - CRT_WARP * cy * cy);
        }
        if !(-1.0..=1.0).contains(&cx) || !(-1.0..=1.0).contains(&cy) {
            // In the recess between the warped picture's corner and the
            // bezel — nothing drawn there to click.
            return None;
        }
        Some((
            (((cx * 0.5 + 0.5) * sw) as i32).clamp(0, sw as i32 - 1),
            (((cy * 0.5 + 0.5) * sh) as i32).clamp(0, sh as i32 - 1),
        ))
    }

    /// Show the side panel during play: `logo` (width, height, RGBA) is the
    /// local art if there is one, else the panel falls back to `title` in
    /// text (plan §3.2, item 1); `cartridge` is a second, independent local
    /// image drawn right below it when present (plan revision — neither
    /// needs the other). `commands` is the initial button legend (item 3,
    /// plan revision: mouse-only) — see `set_commands` for the per-frame
    /// updates that follow (labels like a "(feito!)" flash change live).
    /// Call once per game.
    pub fn set_panel(
        &mut self,
        logo: Option<(u32, u32, &[u8])>,
        cartridge: Option<(u32, u32, &[u8])>,
        title: &str,
        commands: &[(PanelButton, String)],
    ) {
        let has_logo = if let Some((w, h, rgba)) = logo {
            self.set_image(PANEL_LOGO_IMG, w, h, rgba);
            true
        } else {
            false
        };
        let has_cartridge = if let Some((w, h, rgba)) = cartridge {
            self.set_image(PANEL_CARTRIDGE_IMG, w, h, rgba);
            true
        } else {
            false
        };
        self.panel = Some(PanelInfo {
            has_logo,
            has_cartridge,
            title: title.to_string(),
            commands: commands.to_vec(),
            // A freshly inserted cartridge (plan revision): the console
            // doesn't boot itself any more — see `run_game`'s initial
            // `powered = false` — so the rocker starts down, not up.
            powered: false,
            reset_pressed: false,
            cheats: Vec::new(),
            note_count: 0,
            has_note_thumb: false,
            text_note: None,
            // Seated in the slot until `runner`'s insert animation says
            // otherwise (it runs right after this).
            cartridge_motion: None,
        });
    }

    /// Refresh the command legend's labels (plan revision: a "(feito!)"
    /// flash on a silent action is live state, so the caller rebuilds and
    /// passes this every frame instead of once). A no-op before `set_panel`.
    pub fn set_commands(&mut self, commands: &[(PanelButton, String)]) {
        if let Some(panel) = &mut self.panel {
            panel.commands = commands.to_vec();
        }
    }

    /// Dim/brighten the command buttons to match the console's power state
    /// (Eject only clunks while powered, Reset only acts while powered) — see
    /// `PanelInfo::powered`. A no-op before `set_panel`.
    pub fn set_powered(&mut self, powered: bool) {
        if let Some(panel) = &mut self.panel {
            panel.powered = powered;
        }
    }

    /// Reset's rocker springs up for a short moment after a click, then back
    /// down on its own (plan revision) — the caller (`runner`) recomputes
    /// this every frame from its own click timestamp, same pattern as the
    /// "(feito!)" flash on the other buttons.
    pub fn set_reset_pressed(&mut self, pressed: bool) {
        if let Some(panel) = &mut self.panel {
            panel.reset_pressed = pressed;
        }
    }

    /// Drop the last game's panel (logo, commands, cheats, notes, clock) —
    /// call on the way back to the idle/root screen, so `draw_panel` shows
    /// the "Inserir cartucho" button instead of the previous game's stale
    /// info. A no-op if there's nothing set.
    pub fn clear_panel(&mut self) {
        self.panel = None;
    }

    /// The idle screen's console-brand logo (plan revision: the idle/root
    /// screen — startup, backing out of the shelf, or ejecting a game, all
    /// the same screen — shows this where a game's own logo would go).
    /// `assets/console.png`, decoded once by the caller (`idle::run`); a
    /// no-op with `None` (the file doesn't exist) — `draw_panel`'s idle
    /// branch falls back to plain text the same way a missing per-game logo
    /// does.
    pub fn set_console_logo(&mut self, logo: Option<(u32, u32, &[u8])>) {
        if let Some((w, h, rgba)) = logo {
            self.set_image(PANEL_CONSOLE_LOGO_IMG, w, h, rgba);
        }
    }

    /// The console tag wordmark printed on the slot's loading base (plan
    /// revision: "imagem console-tag... na base do cartucho, alinhado a
    /// esquerda") — `assets/console-tag.png`, loaded once by the app and
    /// drawn by `draw_slot_furniture` on every screen that shows the slot.
    /// `None` removes a previously loaded tag (file gone), leaving the bare
    /// groove line.
    pub fn set_slot_tag(&mut self, tag: Option<(u32, u32, &[u8])>) {
        match tag {
            Some((w, h, rgba)) => self.set_image(SLOT_TAG_IMG, w, h, rgba),
            None => {
                self.images.remove(&SLOT_TAG_IMG);
            }
        }
    }

    /// Update the panel's notebook block (plan §3.4, item 5): `count` of the
    /// 15 print slots that are pinned (plan revision — used to be however
    /// many were filled, shown regardless of pin; now the panel only
    /// features what the player deliberately pinned), `thumb` a pinned
    /// slot's image (width, height, RGBA) if there's one to show, `text` a
    /// pinned text-note slot's content if there's one of those to show —
    /// independent of the photo side, so either, both, or neither can be
    /// present at once. Call once at game start and again after every
    /// capture, write, or pin toggle (either kind). A no-op before
    /// `set_panel`.
    pub fn set_notes(
        &mut self,
        count: usize,
        thumb: Option<(u32, u32, &[u8])>,
        text: Option<&str>,
    ) {
        let has_thumb = if let Some((w, h, rgba)) = thumb {
            self.set_image(PANEL_NOTE_IMG, w, h, rgba);
            true
        } else {
            false
        };
        if let Some(panel) = &mut self.panel {
            panel.note_count = count;
            panel.has_note_thumb = has_thumb;
            panel.text_note = text.map(str::to_string);
        }
    }

    /// Update the side panel's cheat list (plan §4.4): `cheats` is
    /// `(description, on)` pairs in the curated order, shown informationally
    /// here (only the ones that are on, plain text) — toggling lives in the
    /// Cheats modal now (plan revision: its own menu, not a page inside the
    /// notebook). Call once at game start and again on every toggle — the
    /// list is always tiny. A no-op before `set_panel`.
    pub fn set_cheats(&mut self, cheats: &[(String, bool)]) {
        if let Some(panel) = &mut self.panel {
            panel.cheats = cheats.to_vec();
        }
    }

    /// Update the session clock shown at the bottom of the panel. Call once a
    /// frame; it just stores the value for the next present/capture.
    pub fn set_session_time(&mut self, elapsed: Duration) {
        self.session = elapsed;
    }

    /// Load the pause book's content (plan §3.2/§3.4): call once when pause
    /// opens, not every frame — `present_pause` just redraws what's already
    /// set. `captures` is the fixed slot count shared by both pages (plan
    /// revision: always 15). Neither page has anything to show yet — follow
    /// with `set_pause_page` and `set_pause_text_page`.
    pub fn set_pause_note(&mut self, title: &str, captures: usize) {
        self.pause = Some(PauseNote {
            title: title.to_string(),
            captures,
            page: captures.saturating_sub(1),
            has_thumb: false,
            pinned: false,
            slot_label: String::new(),
            text_page: captures.saturating_sub(1),
            text_pinned: false,
            text_content: None,
            draft: None,
            draft_limit: 0,
            draft_heading: String::new(),
        });
    }

    /// Show a different capture on the right page (pagination, plan
    /// revision) — call on entering pause (page = the last one) and again on
    /// every Prev/Next/pin/rename change. `pinned`/`label` are that slot's
    /// current protection state and caption. A no-op before `set_pause_note`.
    pub fn set_pause_page(
        &mut self,
        page: usize,
        pinned: bool,
        label: &str,
        thumb: Option<(u32, u32, &[u8])>,
    ) {
        let has_thumb = if let Some((w, h, rgba)) = thumb {
            self.set_image(PAUSE_THUMB_IMG, w, h, rgba);
            true
        } else {
            false
        };
        if let Some(p) = &mut self.pause {
            p.page = page;
            p.has_thumb = has_thumb;
            p.pinned = pinned;
            p.slot_label = label.to_string();
        }
    }

    /// Show a different text-note slot on the left page (plan revision —
    /// mirrors `set_pause_page`, the right page's own pagination): call on
    /// entering pause and again on every Prev/Next/pin/write/delete change.
    /// `content` is `None` for an empty slot. A no-op before
    /// `set_pause_note`.
    pub fn set_pause_text_page(&mut self, page: usize, pinned: bool, content: Option<&str>) {
        if let Some(p) = &mut self.pause {
            p.text_page = page;
            p.text_pinned = pinned;
            p.text_content = content.map(str::to_string);
        }
    }

    /// Enter/update/leave an editor on the left page (plan revision: either
    /// the free-text note or a slot's caption — `heading` names which,
    /// shown above the live text): `Some(text)` shows `text` in place of the
    /// status view, with a `.../limit` counter; `None` leaves editing (any
    /// `heading` passed alongside `None` is ignored). Call on every
    /// keystroke while editing — cheap, at most a few hundred characters.
    /// A no-op before `set_pause_note`.
    pub fn set_pause_draft(&mut self, draft: Option<&str>, limit: usize, heading: &str) {
        if let Some(p) = &mut self.pause {
            p.draft = draft.map(str::to_string);
            p.draft_limit = limit;
            p.draft_heading = heading.to_string();
        }
    }

    /// Open (or replace) a save/load-state, print slot, or Cheats picker
    /// (plan revision): `title` names the dialog, `rows` is one `(label,
    /// enabled)` pair per slot in order — a disabled row (e.g. "Carregar"
    /// on an empty slot, or a pinned print slot) still shows, just dimmed
    /// and unclickable. Scroll position, `searchable`, and any active
    /// search filter all carry over from whatever modal was already open
    /// (Cheats calls this again after every toggle, to redraw its
    /// checkmarks, without wanting the view to jump back to the top or
    /// forget the filter) — only a genuinely fresh open (no modal was
    /// showing) starts scrolled to the top with no filter and not
    /// searchable; call `set_modal_searchable` right after opening one that
    /// should be.
    pub fn set_modal(&mut self, title: &str, rows: &[(String, bool)]) {
        let scroll = self.modal.as_ref().map_or(0, |m| m.scroll);
        let searchable = self.modal.as_ref().is_some_and(|m| m.searchable);
        let search_query = self
            .modal
            .as_ref()
            .map_or_else(String::new, |m| m.search_query.clone());
        let cheat_filter = self.modal.as_ref().and_then(|m| m.cheat_filter);
        self.modal = Some(ModalInfo {
            title: title.to_string(),
            rows: rows
                .iter()
                .map(|(label, enabled)| ModalRow {
                    label: label.clone(),
                    enabled: *enabled,
                })
                .collect(),
            scroll,
            searchable,
            search_query,
            cheat_filter,
            draft: None,
            draft_limit: 0,
            draft_heading: String::new(),
        });
    }

    /// Set the Cheats modal's on/off state filter (plan revision): `None`
    /// shows every row, `Some(true)`/`Some(false)` only the ones checked
    /// on/off. Resets `scroll` to 0, same reasoning as `set_modal_search`.
    /// A no-op before `set_modal`.
    pub fn set_modal_filter(&mut self, filter: Option<bool>) {
        if let Some(m) = &mut self.modal {
            m.cheat_filter = filter;
            m.scroll = 0;
        }
    }

    /// Mark whether the open modal offers a search box (plan revision —
    /// today only the Cheats modal does). Call once, right after opening
    /// it — `set_modal`'s own refreshes (Cheats redrawing its checkmarks
    /// after each toggle) carry this forward on their own.
    pub fn set_modal_searchable(&mut self, searchable: bool) {
        if let Some(m) = &mut self.modal {
            m.searchable = searchable;
        }
    }

    /// Set the modal's search filter (plan revision): empty clears it. A
    /// new filter resets `scroll` to 0 — whatever page was showing under
    /// the old filter is unlikely to mean anything under the new one. A
    /// no-op before `set_modal`.
    pub fn set_modal_search(&mut self, query: &str) {
        if let Some(m) = &mut self.modal {
            m.search_query = query.to_string();
            m.scroll = 0;
        }
    }

    /// The modal's current search filter, e.g. to pre-fill the search
    /// box's text field when the player reopens it to refine a query.
    /// Empty (not `None`) before `set_modal` — same "nothing to filter" as
    /// an explicitly cleared search.
    pub fn modal_search_query(&self) -> &str {
        self.modal.as_ref().map_or("", |m| m.search_query.as_str())
    }

    /// Scroll the modal's row grid by `delta` visual rows (negative = up) —
    /// a no-op before `set_modal`. `draw_modal` clamps the visible result
    /// against however many rows actually overflow the card, so over-
    /// scrolling just stops at the last page rather than needing a bound
    /// here too.
    pub fn scroll_modal(&mut self, delta: i32) {
        if let Some(m) = &mut self.modal {
            m.scroll = (m.scroll as i32 + delta).max(0) as usize;
        }
    }

    /// Enter/update/leave the modal's naming step (plan revision — the print
    /// picker's second step, same idea as `set_pause_draft`): `Some(text)`
    /// replaces the row grid with `text` and a `.../limit` counter. A no-op
    /// before `set_modal`.
    pub fn set_modal_draft(&mut self, draft: Option<&str>, limit: usize, heading: &str) {
        if let Some(m) = &mut self.modal {
            m.draft = draft.map(str::to_string);
            m.draft_limit = limit;
            m.draft_heading = heading.to_string();
        }
    }

    /// Close whichever modal is open — call once the player picks a slot (or
    /// finishes naming a print) and the action's done, or on Cancel.
    pub fn clear_modal(&mut self) {
        self.modal = None;
    }

    /// Same as `hit_panel_button`, for the modal dialog's own buttons — a
    /// separate list since the modal replaces the whole window instead of
    /// sharing it with the side panel (see `modal_buttons`).
    pub fn hit_modal_button(&self, out_x: i32, out_y: i32) -> Option<PanelButton> {
        self.modal_buttons
            .iter()
            .find(|(_, r)| r.contains_point((out_x, out_y)))
            .map(|(b, _)| *b)
    }

    /// Present of every frame, through the trace watchdog (`SIXTEEN_TRACE=
    /// 1`): present is where a GPU/compositor/display stall shows up — telling
    /// it apart from a slow app loop is the whole point of the trace.
    fn present_and_time(&mut self) {
        let _span = super::TraceSpan::new("present");
        self.canvas.present();
    }

    /// Draw the modal dialog to the window, replacing the whole window like
    /// the pause book does (plan revision).
    pub fn present_modal(&mut self) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = rect;
        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(rect));
        self.modal_buttons = draw_modal(
            &mut self.canvas,
            &mut self.font,
            self.modal.as_ref(),
            rect.width(),
            rect.height(),
        );
        self.canvas.set_viewport(None);
        self.present_and_time();
    }

    /// Like [`Cabinet::present_modal`] but composited into an offscreen
    /// target and saved as a BMP (headless preview).
    pub fn capture_modal_bmp(&mut self, path: &std::path::Path) -> Result<(), PlatformError> {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, real_w, real_h)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let modal = self.modal.as_ref();
        let font = &mut self.font;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(rect));
            let _ = draw_modal(c, font, modal, rect.width(), rect.height());
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Draw the pause book to the window: two pages, not warped by the tube,
    /// replacing the whole window rather than sharing it with the cabinet
    /// (plan §3.2/§3.4), plus its own clickable "Continuar" button — mouse/
    /// gamepad only, no keyboard (plan revision: the frame-step button that
    /// used to sit next to it is gone, a dev-only leftover nobody used
    /// through the actual UI).
    pub fn present_pause(&mut self) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = rect;
        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(rect));
        self.pause_buttons = draw_pause_book(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.pause.as_ref(),
            rect.width(),
            rect.height(),
        );
        self.canvas.set_viewport(None);
        self.present_and_time();
    }

    /// Like [`Cabinet::present_pause`] but composited into an offscreen
    /// target and saved as a BMP (headless preview).
    pub fn capture_pause_bmp(&mut self, path: &std::path::Path) -> Result<(), PlatformError> {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, real_w, real_h)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let pause = self.pause.as_ref();
        let font = &mut self.font;
        let images = &self.images;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(rect));
            let _ = draw_pause_book(c, font, images, pause, rect.width(), rect.height());
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Whether image `id` is already decoded and cached — e.g. to decide
    /// whether any cover art exists at all before choosing how to lay out
    /// the shelf (grid vs. list, plan §3.1). Same lookup `Screen::has_image`
    /// does from inside a `frame_2d` closure, exposed here for callers that
    /// need the answer *before* they can build that closure.
    pub fn has_image(&self, id: u64) -> bool {
        self.images.contains_key(&id)
    }

    /// Size of the recessed screen area — what the selector lays itself out in.
    pub fn screen_size(&self) -> (u32, u32) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        let s = screen_area(rect.width(), rect.height());
        (s.width(), s.height())
    }

    /// Like [`Cabinet::screen_size`], but for the shelf specifically (plan
    /// revision): the tube's own area minus the flat side panel, since the
    /// shelf's grid and the game's video now share the exact same split
    /// (`panel_rect` carved out of `cabinet_canvas_rect` first) instead of
    /// the panel living inside the warped buffer.
    pub fn shelf_screen_size(&self) -> (u32, u32) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let rect = cabinet_canvas_rect(real_w, real_h);
        let (out_w, out_h) = (rect.width(), rect.height());
        let cab_w = out_w.saturating_sub(panel_rect(out_w, out_h).width());
        let s = screen_area(cab_w, out_h);
        (s.width(), s.height())
    }

    // --- game path -------------------------------------------------------

    /// Draw one frame to the window. `aspect_ratio <= 0` means "use 4:3".
    pub fn present_frame(&mut self, frame: &FrameRef, aspect_ratio: f32) {
        self.ensure_src(frame.width, frame.height, frame.format);
        self.upload(frame);

        let (real_w, real_h) = self
            .canvas
            .output_size()
            .unwrap_or((frame.width, frame.height));
        let canvas_rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = canvas_rect;
        let (out_w, out_h) = (canvas_rect.width(), canvas_rect.height());
        let panel = panel_rect(out_w, out_h);
        let cab_w = out_w.saturating_sub(panel.width());
        let screen = screen_area(cab_w, out_h);
        self.screen = screen;
        let dst = fit_aspect_in(screen, resolve_aspect(aspect_ratio));
        self.ensure_mesh(dst);
        self.ensure_bezel(cab_w, out_h, dst);

        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(canvas_rect));

        // A fase do tubo avança por frame aqui também (plan revision:
        // "desligar a tv com o jogo rodando, ao ligar mostrar o jogo no
        // tempo correto"): o Warming/Collapsing do caminho estático
        // acontece sobre a IMAGEM que já estava na tela, e desligada o
        // jogo desaparece — mas o jogo NÃO para: o core segue correndo
        // no caller, sem nada pausado.
        let phase = self.tv_phase();
        let src = self.src.take().unwrap();
        let mesh = self.mesh.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        if !matches!(phase, TvPhase::Dead) {
            let _ = self
                .canvas
                .render_geometry(&mesh.verts, Some(&src.tex), &mesh.indices[..]);
            // A interferência do canal 4 (plan revision: "no canal 4, uma
            // interferência na imagem, igual acontecia de verdade") — o
            // sinal do canal errado: estático picado + a barra de hum
            // rolando por cima da imagem. No canal 3, nada.
            if self.tv_on() && self.tv_channel == 4 {
                let specks = self.ch4_specks(dst);
                draw_ch4_layer(&mut self.canvas, dst, &specks);
            }
        }
        // A máscara da fase por cima do que ficou no vidro (mesma do
        // estático: faixas mortas, borda âmbar, ponto do afterglow, vidro
        // morto inteiro).
        self.draw_tv_phase_glass(&phase);
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.src = Some(src);
        self.mesh = Some(mesh);
        self.bezel = Some(bezel);

        self.update_arrows = draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            out_h,
            &self.nameplate,
            self.nameplate_updates,
        );
        // A marca prateada centralizada no queixo (o frame vivo).
        self.ensure_brand_logo();
        draw_brand_logo(&mut self.canvas, &self.images, screen, out_h);
        // A faixa do topo (badge RA + notificações) morre com a TV: vidro
        // morto é vidro — a fila segue (os prazos expiram no religar, sem
        // conteúdo congelado no vidro preto).
        if self.tv_on() {
            self.draw_osd_front();
        }
        // The timed "CH 3" flash (plan revision): power-on shows the channel
        // banner over the picture for a few seconds, then it's gone. Expired
        // deadlines clear here so the banner truly disappears from the frame
        // instead of relying on the next flash.
        if self.ch3_until.is_some_and(|until| Instant::now() >= until) {
            self.ch3_until = None;
        }
        if self.ch3_until.is_some() && self.tv_on() {
            // O canal não existe numa TV desligada: com o tubo morto, nem
            // banner (o queixo embaixo segue vivo, o vidro não).
            let label = self.ch_osd_label();
            draw_ch_osd(&mut self.canvas, &mut self.font, self.screen, &label);
        }
        // The picture is back, so any queued hiss goes — cut, not faded
        // (same rule the power bursts use for their own buzz).
        if let Some(h) = &self.hiss {
            h.clear();
        }
        // O lado TV do frame de gameplay (plan revision: "tem que estar
        // sempre na tela"): o OSD de barrinhas do volume sobre a imagem
        // (a TV não deixa de ser TV com jogo rodando) e os controles
        // físicos do queixo — o mesmo desenho do estático, os rects
        // registrados a cada frame para o hit-test do caller.
        self.draw_volume_osd(screen);
        let tv_on = self.tv_on();
        let chin = draw_tv_chin_controls(&mut self.canvas, &mut self.font, screen, out_h, tv_on);
        self.tv_power_buttons = Some(chin);
        // Fechar/minimizar on every screen, gameplay included (plan
        // revision: "mostrar o fechar e minimizar em todas as telas").
        self.close_button = draw_close_button(&mut self.canvas, &mut self.font);
        self.minimize_button = draw_minimize_button(&mut self.canvas, &mut self.font);
        let panel_tv_on = self.tv_on();
        self.panel_buttons = draw_panel(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.panel.as_ref(),
            panel,
            self.session,
            self.idle_core_prompt.as_deref(),
            self.dev_mode,
            self.core_status.as_deref(),
            panel_tv_on,
        );
        // Painel de desempenho do modo dev: NA MOLDURA, na faixa acima da
        // tela, centralizado sobre a TV — por último, ainda dentro do
        // viewport do gabinete.
        if self.perf_overlay {
            let line = perf_line(self.perf_fps, self.perf_cpu, self.perf_ram_mb);
            let band = (canvas_rect.top(), self.screen.top() - canvas_rect.top());
            draw_perf_overlay(&mut self.canvas, &mut self.font, self.screen, band, &line);
        }
        self.canvas.set_viewport(None);
        self.present_and_time();
    }

    /// Render one frame into an offscreen target and save it as a BMP. Works
    /// headless (a background window never composites on macOS), so the real
    /// output — cabinet, CRT warp and all — can be eyeballed without a window.
    pub fn capture_bmp(
        &mut self,
        frame: &FrameRef,
        aspect_ratio: f32,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        self.ensure_src(frame.width, frame.height, frame.format);
        self.upload(frame);

        let (real_w, real_h) = self
            .canvas
            .output_size()
            .unwrap_or((frame.width, frame.height));
        let canvas_rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = canvas_rect;
        let (out_w, out_h) = (canvas_rect.width(), canvas_rect.height());
        let panel = panel_rect(out_w, out_h);
        let cab_w = out_w.saturating_sub(panel.width());
        let screen = screen_area(cab_w, out_h);
        self.screen = screen;
        let dst = fit_aspect_in(screen, resolve_aspect(aspect_ratio));
        self.ensure_mesh(dst);
        self.ensure_bezel(cab_w, out_h, dst);

        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, real_w, real_h)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;

        let src = self.src.take().unwrap();
        let mesh = self.mesh.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        // A interferência do canal 4 na captura também — o BMP bate com o
        // que a janela real mostra. Pré-computado antes dos empréstimos
        // imutáveis que a closure captura.
        let ch4_specks = self.ch4_specks_for_capture(screen);
        self.ensure_brand_logo();
        let panel_info = self.panel.as_ref();
        let session = self.session;
        let idle_core_prompt = self.idle_core_prompt.as_deref();
        let perf = self
            .perf_overlay
            .then(|| perf_line(self.perf_fps, self.perf_cpu, self.perf_ram_mb));
        let dev_mode = self.dev_mode;
        let core_status = self.core_status.as_deref();
        let osd = self.osd_for_capture();
        // A fase do tubo como FOTO (sem avançar o relógio): o BMP de
        // gameplay bate com o que a janela real mostraria neste instante —
        // TV desligada, vidro morto onde estaria a imagem do jogo.
        let phase = self.tv_phase_snapshot();
        let tv_on = self.tv_on();
        // O flash do canal (plan revision: ligar a TV/console): o que a
        // janela real mostraria neste instante do começo do sinal.
        let ch3_show = tv_on && self.ch3_until.is_some_and(|until| Instant::now() < until);
        let ch_label = self.ch_osd_label();
        // A interferência do canal 4 na captura também — o BMP bate com o
        // que a janela real mostra (o estático picado é pré-computado, a
        // closure não pode tocar em `self`).
        let ch4 = tv_on && self.tv_channel == 4;
        let font = &mut self.font;
        let images = &self.images;
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut chin = TvChinControls::offscreen();
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            // O tubo manda: desligada, a imagem do jogo NÃO entra na
            // captura — o vidro morto é desenhado por cima (como o frame
            // vivo faz no gameplay).
            if !matches!(phase, TvPhase::Dead) {
                let _ = c.render_geometry(&mesh.verts, Some(&src.tex), &mesh.indices[..]);
                if ch4 {
                    draw_ch4_layer(c, screen, &ch4_specks);
                }
            }
            draw_tv_glass_phase(c, screen, &phase);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            let _ = draw_brand(c, font, screen, out_h, nameplate, nameplate_updates);
            // A marca prateada centralizada (a captura bate com a janela).
            draw_brand_logo(c, images, screen, out_h);
            // O banner do canal sobre o sinal (o prazo do religar/ligar):
            // a janela real o mostra por 3s; a captura bate.
            if ch3_show {
                draw_ch_osd(c, font, screen, &ch_label);
            }
            // Os controles do queixo também no BMP de gameplay — o que a
            // janela real mostra (plan revision: "tem que estar sempre
            // na tela"), a captura mostra.
            chin = draw_tv_chin_controls(c, font, screen, out_h, tv_on);
            match &osd {
                ChinOsd::Notify { lines, badge } => {
                    draw_chin_osd(c, font, images, screen, out_h, nameplate, lines, *badge);
                }
                ChinOsd::Ra { hardcore } => {
                    draw_chin_ra(
                        c,
                        font,
                        images,
                        screen,
                        canvas_rect.width() as i32,
                        nameplate,
                        *hardcore,
                    );
                }
                ChinOsd::None => {}
            }
            draw_panel(
                c,
                font,
                images,
                panel_info,
                panel,
                session,
                idle_core_prompt,
                dev_mode,
                core_status,
                tv_on,
            );
            if let Some(line) = &perf {
                let band = (canvas_rect.top(), screen.top() - canvas_rect.top());
                draw_perf_overlay(c, font, screen, band, line);
            }
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        self.src = Some(src);
        self.mesh = Some(mesh);
        self.bezel = Some(bezel);
        self.tv_power_buttons = Some(chin);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    fn upload(&mut self, frame: &FrameRef) {
        let expected_pitch = frame.width as usize * frame.format.bytes_per_pixel();
        let src = self.src.as_mut().unwrap();
        let pitch = if frame.pitch == 0 {
            expected_pitch
        } else {
            frame.pitch
        };
        src.tex
            .update(None, frame.pixels, pitch)
            .expect("upload frame");
    }

    fn ensure_src(&mut self, w: u32, h: u32, format: PixelFormat) {
        let stale = match &self.src {
            Some(s) => s.w != w || s.h != h || s.format != format,
            None => true,
        };
        if stale {
            let mut tex = self
                .canvas
                .create_texture_streaming(format.sdl(), w, h)
                .expect("create streaming texture");
            tex.set_scale_mode(SdlScaleMode::Linear);
            self.src = Some(SrcTexture { tex, w, h, format });
        }
    }

    fn ensure_mesh(&mut self, dst: Rect) {
        let (w, h) = (dst.width(), dst.height());
        if matches!(&self.mesh, Some(m) if m.w == w && m.h == h) {
            return;
        }
        self.mesh = Some(build_crt_mesh(dst, 1.0));
    }

    /// Garante o `mesh` da tela atual e um `fade_mesh` igual a ele com o
    /// alpha pedido — reaproveitados entre os frames do fade.
    fn ensure_fade_meshes(&mut self, alpha: f32) {
        self.ensure_mesh(self.screen);
        let base = self.mesh.as_ref().unwrap();
        if !matches!(&self.fade_mesh, Some(m) if m.w == base.w && m.h == base.h) {
            self.fade_mesh = Some(CrtMesh {
                verts: base.verts.clone(),
                indices: base.indices.clone(),
                w: base.w,
                h: base.h,
            });
        }
        let alpha = alpha.clamp(0.0, 1.0);
        let fade = self.fade_mesh.as_mut().unwrap();
        if fade.verts.first().is_some_and(|v| v.color.a != alpha) {
            for v in &mut fade.verts {
                v.color.a = alpha;
            }
        }
    }

    fn ensure_bezel(&mut self, out_w: u32, out_h: u32, screen: Rect) {
        let key = (out_w, out_h, screen.width(), screen.height());
        if matches!(&self.bezel, Some(b) if b.key == key) {
            return;
        }
        self.bezel = Some(build_bezel_mesh(out_w, out_h, screen, key));
    }

    // --- 2D path (selector) --------------------------------------------
    //
    // The selector draws flat 2D into an offscreen buffer the size of the tube
    // opening, which is then composited through the CRT mesh just like a game
    // frame — so the shelf bulges with the same tube.

    /// Register/replace an image from tightly-packed RGBA8 (call between frames).
    pub fn set_image(&mut self, id: u64, w: u32, h: u32, rgba: &[u8]) {
        if w == 0 || h == 0 || rgba.len() < (w * h * 4) as usize {
            return;
        }
        let mut tex = match self.canvas.create_texture_static(SdlFormat::RGBA32, w, h) {
            Ok(t) => t,
            Err(e) => {
                log::warn!("cabinet: texture {w}x{h}: {e}");
                return;
            }
        };
        tex.set_blend_mode(BlendMode::Blend);
        tex.set_scale_mode(SdlScaleMode::Linear);
        let vazado = rgba.as_chunks::<4>().0.iter().any(|p| p[3] < 250);
        // O content_bbox varre pixel a pixel — só importa para arte VAZADA
        // (a caixa 3D do painel recorta por ele). Para páginas/capas
        // opacas (o PDF do leitor: ~23M pixels), pular a varredura era a
        // diferença entre virar a página fluido e travar um frame.
        let content = if vazado {
            content_bbox(w, h, rgba)
        } else {
            (0, 0, w, h)
        };
        // Arte vazada (render 3D em ângulo): a matte do render —
        // anti-aliasing CINZA em volta da caixa — vira franja cinza quando a
        // textura é reduzida com filtro linear. No upload, limpa: semi cinza
        // sai do todo. Em seguida, SANGRIA DE BORDA: o rgb dos texels
        // vazados herda a cor do conteúdo vizinho — com filtro linear, o
        // contorno mistura caixa com caixa (e não com preto/cinza), e quem
        // faz o fade é só o alpha: transparência de verdade, sem halo
        // escuro na volta nem sombra na base.
        let rgba_limpa;
        let rgba = if vazado {
            let mut limpa = rgba.to_vec();
            for p in limpa.as_chunks_mut::<4>().0 {
                let (r, g, b, a) = (p[0], p[1], p[2], p[3]);
                if a > 0 && a < 250 && r.abs_diff(g) < 24 && g.abs_diff(b) < 24 {
                    p[3] = 0;
                }
            }
            let (wu, hu) = (w as usize, h as usize);
            for _ in 0..2 {
                let fonte = limpa.clone();
                for y in 0..hu {
                    for x in 0..wu {
                        let i = (y * wu + x) * 4;
                        if fonte[i + 3] != 0 {
                            continue;
                        }
                        let vizinho = [
                            x.checked_sub(1).map(|nx| (nx, y)),
                            Some((x + 1, y)).filter(|&(nx, _)| nx < wu),
                            y.checked_sub(1).map(|ny| (x, ny)),
                            Some((x, y + 1)).filter(|&(_, ny)| ny < hu),
                        ]
                        .into_iter()
                        .flatten()
                        .find(|&(nx, ny)| fonte[(ny * wu + nx) * 4 + 3] > 0);
                        if let Some((nx, ny)) = vizinho {
                            let j = (ny * wu + nx) * 4;
                            limpa[i] = fonte[j];
                            limpa[i + 1] = fonte[j + 1];
                            limpa[i + 2] = fonte[j + 2];
                        }
                    }
                }
            }
            rgba_limpa = limpa;
            &rgba_limpa
        } else {
            rgba
        };
        if tex.update(None, rgba, (w * 4) as usize).is_err() {
            return;
        }
        self.images.insert(
            id,
            ImgTex {
                tex,
                w,
                h,
                content,
                vazado,
            },
        );
    }

    /// A imagem `id` tem transparência de verdade (fundo vazado) — ver
    /// [`ImgTex::vazado`]. Imagem desconhecida: `false`.
    pub fn image_vazado(&self, id: u64) -> bool {
        self.images.get(&id).is_some_and(|img| img.vazado)
    }

    /// Draw a 2D frame: `draw` renders into a screen-sized buffer (coords
    /// 0..screen), which is then warped through the tube, framed and presented.
    pub fn frame_2d<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        self.paint_2d(bg, draw);
        self.composite_screen();
        self.present_and_time();
    }

    /// Like [`Cabinet::frame_2d`], but for the shelf specifically (plan
    /// revision): `draw` only ever sees the narrower grid area (the panel
    /// column is reserved first), and the shelf's own info panel is drawn
    /// flat, outside the tube's warp, right after it.
    pub fn frame_shelf<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        self.paint_shelf(bg, draw);
        self.composite_shelf();
        self.present_and_time();
    }

    /// Like [`Cabinet::frame_2d`] but composited into an offscreen target and
    /// saved as a BMP (headless — a background window never composites on macOS).
    pub fn capture_2d<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        self.paint_2d(bg, draw);

        let (ww, wh) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = self.canvas_rect;
        let screen = self.screen;
        let mesh = build_crt_mesh(screen, 1.0);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, ww, wh)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let st = self.screen_tex.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let tv_on = self.tv_on();
        self.ensure_brand_logo(); // a marca do queixo na captura
        let images = &self.images;
        let font = &mut self.font;
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut close_button = self.close_button;
        let mut minimize_button = self.minimize_button;
        let mut chin = TvChinControls::offscreen();
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            let _ = c.render_geometry(&mesh.verts, Some(&st.tex), &mesh.indices[..]);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            draw_brand(
                c,
                font,
                screen,
                canvas_rect.height(),
                nameplate,
                nameplate_updates,
            );
            // A marca prateada centralizada (a captura bate com a janela).
            draw_brand_logo(c, images, screen, canvas_rect.height());
            // Os controles do queixo no BMP — a captura bate com a janela
            // real, que os desenha em toda a moldura.
            chin = draw_tv_chin_controls(c, font, screen, canvas_rect.height(), tv_on);
            close_button = draw_close_button(c, font);
            minimize_button = draw_minimize_button(c, font);
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        self.close_button = close_button;
        self.minimize_button = minimize_button;
        self.tv_power_buttons = Some(chin);
        self.screen_tex = Some(st);
        self.bezel = Some(bezel);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Like [`Cabinet::capture_2d`], but for the shelf specifically (plan
    /// revision) — reserves the panel column like `frame_shelf` does, and
    /// draws the flat info panel into the same offscreen target so the BMP
    /// matches what a real window would show.
    pub fn capture_shelf<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        self.paint_shelf(bg, draw);

        let (ww, wh) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = self.canvas_rect;
        let panel = panel_rect(canvas_rect.width(), canvas_rect.height());
        let screen = self.screen;
        let mesh = build_crt_mesh(screen, 1.0);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, ww, wh)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let st = self.screen_tex.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let tv_on = self.tv_on();
        self.ensure_brand_logo(); // a marca do queixo na captura
        let font = &mut self.font;
        let images = &self.images;
        let shelf_panel = self.shelf_panel.as_ref();
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut close_button = self.close_button;
        let mut minimize_button = self.minimize_button;
        let mut chin = TvChinControls::offscreen();
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            let _ = c.render_geometry(&mesh.verts, Some(&st.tex), &mesh.indices[..]);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            draw_brand(
                c,
                font,
                screen,
                canvas_rect.height(),
                nameplate,
                nameplate_updates,
            );
            // Os controles do queixo no BMP — a captura bate com a janela
            // real, que os desenha em toda a moldura.
            chin = draw_tv_chin_controls(c, font, screen, canvas_rect.height(), tv_on);
            let _ = draw_shelf_panel(c, font, images, shelf_panel, panel);
            close_button = draw_close_button(c, font);
            minimize_button = draw_minimize_button(c, font);
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        self.close_button = close_button;
        self.minimize_button = minimize_button;
        self.tv_power_buttons = Some(chin);
        self.screen_tex = Some(st);
        self.bezel = Some(bezel);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Show the green "CH 3" channel banner over the picture for `dur`
    /// (plan revision: "quando ligar o console, mostrar por 3 segundos e
    /// remover da tela") — the runner calls this on power-on, the way an old
    /// TV flashes the channel number when you tune it. While the tube is on
    /// static instead, `present_static` draws the banner constantly, no
    /// timer involved.
    pub fn flash_ch3(&mut self, dur: Duration) {
        self.ch3_until = Some(Instant::now() + dur);
    }

    /// Settings gate for the ambient static hiss (plan revision: "colocar
    /// na configuração para tocar ou não. por padrão vem desligado") — off
    /// by default; `present_static` queues the white noise only while this
    /// is set. Turning it off drops the stream immediately.
    pub fn set_static_hiss(&mut self, on: bool) {
        self.static_hiss = on;
        if !on {
            self.hiss = None;
        }
    }

    /// Queue a one-shot sound (mono `samples` at `rate`) on the cabinet's
    /// persistent foley stream — the stream outlives the call, so the queued
    /// audio actually plays (a stream dropped immediately after `queue`
    /// takes its audio with it). Returns `false` when no audio device is
    /// available; callers keep their fallback in that case.
    pub fn play_foley(&mut self, samples: &[i16], rate: u32) -> bool {
        if self.foley.is_none() {
            match AudioOut::new(&self.audio, rate) {
                Ok(a) => self.foley = Some(a),
                Err(e) => {
                    log::warn!("foley stream: {e}");
                    return false;
                }
            }
        }
        // Mono → the interleaved stereo `AudioOut` wants. Non-blocking: SDL
        // buffers and plays on its own; overlapping sounds queue in order.
        let mut buf = Vec::with_capacity(samples.len() * 2);
        for &s in samples {
            buf.push(s);
            buf.push(s);
        }
        if let Some(a) = &self.foley {
            a.queue(&buf);
        }
        true
    }

    /// Queue one frame's worth of hiss at `level` (the same 0..1 the visual
    /// snow uses, so blizzards hiss louder than the resting hiss). Opens the
    /// stream lazily on the first hissing frame.
    fn queue_static_hiss(&mut self, level: f32) {
        const RATE: u32 = 22_050;
        if self.hiss.is_none() {
            match AudioOut::new(&self.audio, RATE) {
                Ok(a) => self.hiss = Some(a),
                Err(e) => {
                    log::warn!("static hiss: {e}");
                    self.static_hiss = false;
                    return;
                }
            }
        }
        let n = (RATE / 60) as usize;
        let amp = (level.clamp(0.0, 1.0) * 1800.0) as i32;
        let mut buf = Vec::with_capacity(n * 2);
        for _ in 0..n {
            self.hiss_rng ^= self.hiss_rng << 13;
            self.hiss_rng ^= self.hiss_rng >> 17;
            self.hiss_rng ^= self.hiss_rng << 5;
            let s = (((self.hiss_rng >> 8) & 0xFFFF) as i32 - 0x8000) * amp / 0x8000;
            let v = s.clamp(-32000, 32000) as i16;
            buf.push(v);
            buf.push(v);
        }
        if let Some(a) = &self.hiss {
            a.queue(&buf);
        }
    }

    // --- TV power/volume (plan revision: o conjunto físico do queixo) ---

    /// A TV está ligada (ou virando)? `false` no `Off`/`Collapsing` — o
    /// idle usa para saber o que desenhar (painel vivo ou vidro morto).
    pub fn tv_on(&self) -> bool {
        !matches!(
            self.tv_power,
            Some(TvPower::Off) | Some(TvPower::Collapsing { .. })
        )
    }

    /// Ligar/desligar: dispara o warm-up (ponto -> linha -> raster abrindo,
    /// ~2s) ou o colapso (imagem -> linha -> ponto -> afterglow, ~0.4s).
    /// Chamar de novo no meio de uma animação reinicia a fase oposta — o
    /// knob é físico, cliques rápidos são legítimos.
    pub fn toggle_tv_power(&mut self) {
        // `None` é a TV LIGADA (estado de repouso) — o clique nela desliga;
        // Off/Collapsing/Warming são os estados de desligada/em transição,
        // o clique liga. (O bug antigo: None caía no braço do Warming e
        // "desligar" religava o tubo.)
        self.tv_power = Some(match self.tv_power {
            Some(TvPower::Off) | Some(TvPower::Collapsing { .. }) => {
                // LIGANDO: o canal na tela até a TV entender que está
                // dando sinal (plan revision: "quando religar a tv,
                // mostrar o CH 3 novamente por alguns segundos") — o
                // mesmo 3s do ligar o console (`CH3_FLASH` no runner). No
                // gameplay o banner cobre o warm-up inteiro e depois
                // some; no estático o CH 3 é constante e o prazo não
                // manda em nada — setar por cima é inofensivo.
                self.ch3_until = Some(Instant::now() + TV_CH3_FLASH);
                TvPower::Warming { frames: 0 }
            }
            None | Some(TvPower::Warming { .. }) => TvPower::Collapsing { frames: 0 },
        });
        // A neve volta do zero — sem estoque do estado anterior.
        self.noise_stamp = None;
    }

    /// Volume da TV, 0..=16 — o jogo multiplica a amostra dele por isso.
    pub fn volume(&self) -> u8 {
        self.volume
    }

    /// Ajusta o volume (o queixo: VOL-/VOL+; cada clique 1 de 16) e acende
    /// o OSD de barrinhas por 2s. `changed` diz se houve mudança (os limites
    /// 0/16 param o OSD? não — TVs reais continuam mostrando o MAX/0).
    pub fn nudge_volume(&mut self, delta: i32) {
        let v = self.volume as i32 + delta;
        self.volume = v.clamp(0, 16) as u8;
        self.volume_osd_until = Some(Instant::now() + Duration::from_secs(2));
    }

    /// O canal da TV (o queixo: o botão CH) — só alterna entre 3 (o canal
    /// do console) e 4, acendendo o banner verde por 3s (o mesmo prazo
    /// do ligar).
    pub fn toggle_channel(&mut self) {
        self.tv_channel = if self.tv_channel == 3 { 4 } else { 3 };
        self.ch3_until = Some(Instant::now() + TV_CH3_FLASH);
    }

    /// O texto do banner de canal — dígito duplo, como as TVs de tubo
    /// mostravam.
    pub fn ch_osd_label(&self) -> String {
        format!("CH {:2}", self.tv_channel)
    }

    /// Um frame do tubo: avança as fases (Warming/Collapsing contam em
    /// frames; chamar uma vez por frame apresentado — o estático avança
    /// aqui, o gameplay também) e devolve o que ESTE frame desenha no
    /// vidro. O fim do colapso desliga a TV (e mata o hiss pendente), o
    /// fim do warm-up liga (o `None` do estado ligada).
    fn tv_phase(&mut self) -> TvPhase {
        match self.tv_power {
            Some(TvPower::Warming { frames }) => {
                if frames + 1 >= TV_WARM_FRAMES {
                    self.tv_power = None; // ligada
                } else {
                    self.tv_power = Some(TvPower::Warming { frames: frames + 1 });
                }
            }
            Some(TvPower::Collapsing { frames }) => {
                if frames + 1 >= TV_COLLAPSE_FRAMES {
                    self.tv_power = Some(TvPower::Off);
                    // O hiss morre com a TV.
                    if let Some(h) = &self.hiss {
                        h.clear();
                    }
                } else {
                    self.tv_power = Some(TvPower::Collapsing { frames: frames + 1 });
                }
            }
            _ => {}
        }
        self.tv_phase_snapshot()
    }

    /// A fase do frame ATUAL, sem avançar o relógio — o frame vivo e os
    /// captures (foto do estado, os timed advance só no presente) leem
    /// por aqui.
    fn tv_phase_snapshot(&self) -> TvPhase {
        match self.tv_power {
            Some(TvPower::Warming { frames }) => {
                let t = frames as f32 / TV_WARM_FRAMES as f32;
                // o raster abre: 0 até o meio do warm-up, depois abre até 1
                let open = ((t - 0.35) / 0.5).clamp(0.0, 1.0);
                // o brilho do fósforo: nasce no ponto, pico na abertura,
                // assenta no fim
                let hot = if t < 0.35 {
                    t / 0.35
                } else {
                    (1.0 - (t - 0.35) / 0.65 * 0.8).max(0.2)
                };
                TvPhase::Raster {
                    open,
                    hot,
                    ponto: false,
                }
            }
            Some(TvPower::Collapsing { frames }) => {
                let t = frames as f32 / TV_COLLAPSE_FRAMES as f32;
                TvPhase::Raster {
                    open: (1.0 - t * 1.15).clamp(0.0, 1.0),
                    hot: (0.6 + t * 0.4).min(1.0),
                    ponto: t > 0.85,
                }
            }
            Some(TvPower::Off) => TvPhase::Dead,
            None => TvPhase::On,
        }
    }

    /// Desenha por cima do vidro o que a fase manda (`tv_phase`). Wrapper
    /// do desenho puro — a função livre os captures (que não podem pegar
    /// `&mut self`) também usam.
    fn draw_tv_phase_glass(&mut self, phase: &TvPhase) {
        draw_tv_glass_phase(&mut self.canvas, self.screen, phase);
    }

    /// Um frame do queixo: os controles físicos (Power knob, VOL-/VOL+,
    /// LED) desenhados no plástico abaixo do vidro. Devolve os rects em
    /// coordenadas de output/canvas para o hit-test do caller —
    /// o conjunto `TvChinControls`.
    fn draw_tv_chin(&mut self, screen: Rect, out_h: u32) -> TvChinControls {
        let on = self.tv_on();
        draw_tv_chin_controls(&mut self.canvas, &mut self.font, screen, out_h, on)
    }

    /// O OSD das barrinhas de volume (plan revision: "mostrando no CRT as
    /// barrinhas de volume retro quando aumenta e diminui") — segmentos
    /// Os componentes da interferência do canal 4, pré-computados — as
    /// passadas de captura (a closure do BMP) não podem tocar em `self`.
    fn ch4_specks(&mut self, screen: Rect) -> Vec<(i32, i32, bool)> {
        let mut specks = Vec::new();
        if screen.width() == 0 || screen.height() == 0 {
            return specks;
        }
        let n = ((screen.width() as u64 * screen.height() as u64) / 1400).max(60);
        for _ in 0..n {
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 7;
            self.rng ^= self.rng << 17;
            let x = screen.x() + ((self.rng as u64) % (screen.width() as u64)) as i32;
            let y = screen.y() + (((self.rng as u64) >> 16) % (screen.height() as u64)) as i32;
            specks.push((x, y, self.rng & 1 == 0));
        }
        specks
    }

    /// A variante da captura: vazio a menos que a TV esteja ligada no
    /// canal 4 — a decisão toda num lugar só, antes dos empréstimos.
    fn ch4_specks_for_capture(&mut self, screen: Rect) -> Vec<(i32, i32, bool)> {
        if self.tv_on() && self.tv_channel == 4 {
            self.ch4_specks(screen)
        } else {
            Vec::new()
        }
    }

    /// As dimensões da imagem registrada sob `id` — o input do leitor
    /// usa para centralizar o zoom onde o usuário clicou.
    pub fn image_size(&self, id: u64) -> Option<(u32, u32)> {
        self.images.get(&id).map(|i| (i.w, i.h))
    }

    /// Registra a arte da marca (RGBA embutido) no mapa de imagens, uma
    /// vez só — o queixo a desenha centralizada em todo frame.
    fn ensure_brand_logo(&mut self) {
        if !self.images.contains_key(&BRAND_LOGO_ID) {
            self.set_image(BRAND_LOGO_ID, BRAND_LOGO_W, BRAND_LOGO_H, BRAND_LOGO_RGBA);
        }
    }

    fn draw_volume_osd(&mut self, screen: Rect) {
        let Some(until) = self.volume_osd_until else {
            return;
        };
        if Instant::now() >= until {
            self.volume_osd_until = None;
            return;
        }
        // OSD morre com a TV (plan revision): barrinhas sobre o vidro
        // morto é impossível — e o deadline apaga junto, na re-entrada o
        // OSD recomeça do clique, sem resíduo.
        if !self.tv_on() {
            self.volume_osd_until = None;
            return;
        }
        const TOTAL: u8 = 16;
        let canvas = &mut self.canvas;
        let font = &mut self.font;
        let seg_w = 26i32;
        let n = TOTAL as i32;
        let bars_w = n * seg_w;
        let bx = screen.x() + (screen.width() as i32 - bars_w) / 2;
        let by = screen.bottom() - (screen.height() as i32 / 5);
        // O verde do CH 3 (OSD_GREEN) — o fósforo do OSD da TV é um só,
        // canal e volume na mesma cor E na mesma fonte: escala 3 com a
        // sombra 2px deslocada (plan revision: "aquela fonte retro da tv").
        let label = "VOLUME";
        let lw = (GLYPH_W as i32 * 3) * label.chars().count() as i32;
        draw_text_absolute(
            canvas,
            font,
            bx - lw - 18 + 2,
            by + 2 + 2,
            TextStyle::new(3, (12, 14, 12)),
            label,
            usize::MAX,
        );
        draw_text_absolute(
            canvas,
            font,
            bx - lw - 18,
            by + 2,
            TextStyle::new(3, OSD_GREEN),
            label,
            usize::MAX,
        );
        if self.volume >= TOTAL {
            let mx = bx + bars_w + 18;
            draw_text_absolute(
                canvas,
                font,
                mx + 2,
                by + 2 + 2,
                TextStyle::new(3, (12, 14, 12)),
                "MAX",
                usize::MAX,
            );
            draw_text_absolute(
                canvas,
                font,
                mx,
                by + 2,
                TextStyle::new(3, OSD_GREEN),
                "MAX",
                usize::MAX,
            );
        }
        for i in 0..n {
            let aceso = (i as u8) < self.volume;
            let x = bx + i * seg_w;
            let cor = if aceso { OSD_GREEN } else { (20, 34, 22) };
            canvas.set_draw_color(Color::RGB(cor.0, cor.1, cor.2));
            let _ = canvas.fill_rect(Rect::new(x, by, 9, 34));
            let _ = canvas.fill_rect(Rect::new(x + 13, by, 9, 26));
        }
        // o filete de brilho sob as barras acesas
        if self.volume > 0 {
            let w = seg_w * self.volume as i32 - 6;
            canvas.set_draw_color(Color::RGBA(OSD_GREEN.0, OSD_GREEN.1, OSD_GREEN.2, 150));
            let _ = canvas.fill_rect(Rect::new(bx, by + 40, w.max(4) as u32, 1));
        }
    }

    /// One frame of signal-off snow through the tube, cartridge still visible
    /// in its slot if one is set. `level` 1.0 = a full blizzard, 0.0 = a dim,
    /// near-still hiss. Never a full-screen flash.
    ///
    /// A TV pode estar desligada ou em transição (`tv_power`, plan revision:
    /// "ao abrir o app a tv estiver desligada e fazer um botao de power"):
    /// desligada, o tubo desenha o vidro morto (quase preto + reflexo) no
    /// lugar da neve; aquecendo, a neve entra comprimida e estourada (o
    /// raster abrindo); colapsando, ela esmaga na linha -> ponto ->
    /// afterglow. A neve em si (a textura, o CH 3, o hiss) é intocada —
    /// estas fases são só uma máscara vertical + brilho por cima dela.
    pub fn present_static(&mut self, level: f32) {
        // A fase do tubo avança por frame e sabe o que este frame desenha;
        // a neve: desligada, nível zero (nada para a textura — o vidro morto
        // é desenhado por cima); em transição, o normal (a máscara do vidro
        // faz o resto); ligada, o nível de sempre.
        let phase = self.tv_phase();
        let drawn_level = if matches!(phase, TvPhase::Dead) {
            0.0
        } else {
            level
        };
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = canvas_rect;
        let (ww, wh) = (canvas_rect.width(), canvas_rect.height());
        let panel = panel_rect(ww, wh);
        let cab_w = ww.saturating_sub(panel.width());
        self.screen = screen_area(cab_w, wh);
        self.ensure_bezel(cab_w, wh, self.screen);
        self.ensure_mesh(self.screen);
        self.update_noise_tex(drawn_level);

        // The mesh is cached like the gameplay path's — rebuilding 1,089
        // vertices every frame for a screen that only changes on
        // window-resize was pure waste.
        let mesh = self.mesh.take().unwrap();
        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(canvas_rect));
        let nt = self.noise_tex.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&mesh.verts, Some(&nt.tex), &mesh.indices[..]);
        self.noise_tex = Some(nt);
        // A máscara da fase (raster abrindo/colapsando/vidro morto), por
        // cima da neve — a mesma passada do gameplay.
        self.draw_tv_phase_glass(&phase);
        let bezel = self.bezel.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.bezel = Some(bezel);
        self.mesh = Some(mesh);
        // O OSD de volume + o queixo com os controles físicos — por cima do
        // bezel (o OSD é da TV; o queixo é o plástico dela).
        self.draw_volume_osd(self.screen);
        let chin = self.draw_tv_chin(self.screen, canvas_rect.height());
        self.tv_power_buttons = Some(chin);
        // O retorno PRECISA ser guardado: são os rects clicáveis das setas
        // verdes de update — sem isto o idle desenha as setas mas o clique
        // nunca acerta (hit_update_arrow lia uma lista sempre vazia).
        self.update_arrows = draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            wh,
            &self.nameplate,
            self.nameplate_updates,
        );
        // A marca prateada centralizada no queixo (o estático também).
        self.ensure_brand_logo();
        draw_brand_logo(&mut self.canvas, &self.images, self.screen, wh);
        // Sem sinal (plan revision): the channel banner stays up the whole
        // time the TV is showing snow — game inserted but powered off, the
        // idle screen, all of it — like a real set parked on channel 3.
        // TV DESLIGADA, porém: vidro morto não mostra canal nem badge —
        // uma TV apagada é só vidro (o queixo embaixo segue vivo).
        if self.tv_on() {
            let label = self.ch_osd_label();
            draw_ch_osd(&mut self.canvas, &mut self.font, self.screen, &label);
        }
        // The ambient hiss (opt-in via settings): follows the same level as
        // the visual snow, so a burst hisses loud and the resting hiss stays
        // a quiet background shhh.
        // O chiado é do TUBO: com a TV desligada, silêncio — o buffer
        // pendente é cortado na hora (nada de neve sonora no vidro morto).
        if self.static_hiss && self.tv_on() {
            self.queue_static_hiss(level);
        } else if let Some(h) = &self.hiss {
            h.clear();
        }
        let panel_tv_on = self.tv_on();
        self.panel_buttons = draw_panel(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.panel.as_ref(),
            panel,
            self.session,
            self.idle_core_prompt.as_deref(),
            self.dev_mode,
            self.core_status.as_deref(),
            panel_tv_on,
        );
        // O badge do RA depois do painel (furniture da moldura, como o par
        // fechar/minimizar) — antes disso o painel cobria o badge no idle.
        if self.tv_on() {
            self.draw_ra_badge();
        }
        self.close_button = draw_close_button(&mut self.canvas, &mut self.font);
        self.minimize_button = draw_minimize_button(&mut self.canvas, &mut self.font);
        self.canvas.set_viewport(None);
        self.present_and_time();
    }

    /// Like [`Cabinet::present_static`] but composited into an offscreen
    /// target and saved as a BMP (headless — eyeball the power-off / idle-off
    /// screen, cartridge and all, without a window).
    pub fn capture_static_bmp(
        &mut self,
        level: f32,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = cabinet_canvas_rect(real_w, real_h);
        self.canvas_rect = canvas_rect;
        let (ww, wh) = (canvas_rect.width(), canvas_rect.height());
        let panel = panel_rect(ww, wh);
        let cab_w = ww.saturating_sub(panel.width());
        self.screen = screen_area(cab_w, wh);
        self.ensure_bezel(cab_w, wh, self.screen);
        self.update_noise_tex(level);
        let screen = self.screen;

        let mesh = build_crt_mesh(screen, 1.0);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, real_w, real_h)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let nt = self.noise_tex.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let tv_on = self.tv_on();
        let panel_info = self.panel.as_ref();
        let session = self.session;
        let idle_core_prompt = self.idle_core_prompt.as_deref();
        let dev_mode = self.dev_mode;
        let core_status = self.core_status.as_deref();
        let ra_status = self.ra_status;
        let ch_label = self.ch_osd_label();
        let font = &mut self.font;
        let images = &self.images;
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut close_button = self.close_button;
        let mut minimize_button = self.minimize_button;
        let mut chin = TvChinControls::offscreen();
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            let _ = c.render_geometry(&mesh.verts, Some(&nt.tex), &mesh.indices[..]);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            // TV desligada (mesma regra do present_static): o vidro morto
            // por cima da neve, sem CH 3 nem badge.
            if !tv_on {
                for row in 0..screen.height() as i32 {
                    let t = row as f32 / screen.height() as f32;
                    let v = 5.0 + 3.0 * t;
                    c.set_draw_color(Color::RGB(v as u8, v as u8, (v + 1.0) as u8));
                    let _ = c.fill_rect(Rect::new(screen.x(), screen.y() + row, screen.width(), 1));
                }
            }
            // Os controles do queixo (POWER/VOL) — o frame vivo os desenha
            // sempre; a captura, que existe para bater com o que a janela
            // real mostra, não podia deixar os botões de fora.
            chin = draw_tv_chin_controls(c, font, screen, wh, tv_on);
            draw_brand(c, font, screen, wh, nameplate, nameplate_updates);
            draw_brand_logo(c, images, screen, wh);
            if tv_on {
                draw_ch_osd(c, font, screen, &ch_label);
                if let Some(status) = ra_status {
                    draw_chin_ra(
                        c,
                        font,
                        images,
                        screen,
                        canvas_rect.width() as i32,
                        nameplate,
                        status.hardcore,
                    );
                }
            }
            draw_panel(
                c,
                font,
                images,
                panel_info,
                panel,
                session,
                idle_core_prompt,
                dev_mode,
                core_status,
                tv_on,
            );
            close_button = draw_close_button(c, font);
            minimize_button = draw_minimize_button(c, font);
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        self.close_button = close_button;
        self.minimize_button = minimize_button;
        // Os rects tocam o desenho do frame da captura — na janela real o
        // próximo present_static os refaz; na captura, eles são o que um
        // clique veria neste frame congelado.
        self.tv_power_buttons = Some(chin);
        self.noise_tex = Some(nt);
        self.bezel = Some(bezel);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Drive the panel's cartridge slot animation (plan revision: "a
    /// animação deveria estar onde está o cartucho durante a gameplay, não
    /// uma transição"). `Some((t, ejecting))` animates the cartridge art in
    /// the panel's own cartridge block — `t` 0.0..=1.0 progress, `ejecting`
    /// mirrors the motion (see `draw_panel_slot`); every `present_static`/
    /// gameplay frame while this is set draws that progress. `None` is the
    /// resting state — the cartridge seated in the slot, which is also what
    /// `set_panel` starts with. A no-op before `set_panel`; `runner`'s two
    /// animation functions are the only callers.
    pub fn set_cartridge_motion(&mut self, motion: Option<(f32, bool)>) {
        if let Some(panel) = &mut self.panel {
            panel.cartridge_motion = motion;
        }
    }

    /// Like [`Cabinet::frame_2d`], but blended up from residual signal-off snow
    /// instead of cutting in cold: `static_level` is the snow still showing
    /// behind it, `shelf_alpha` (0..1) how much of the drawn frame shows on top
    /// (plan §3.3, "a estante entra por cima"). Call with `shelf_alpha` ramping
    /// 0.0 -> 1.0 over the first handful of frames after a game closes.
    pub fn frame_2d_fade_in<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        static_level: f32,
        shelf_alpha: f32,
    ) {
        self.paint_2d(bg, draw);
        self.update_noise_tex(static_level);
        let canvas_rect = self.canvas_rect;

        self.ensure_fade_meshes(shelf_alpha);
        let mesh_static = self.mesh.take().unwrap();
        let mesh_shelf = self.fade_mesh.take().unwrap();

        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(canvas_rect));

        let nt = self.noise_tex.take().unwrap();
        let _ = self.canvas.render_geometry(
            &mesh_static.verts,
            Some(&nt.tex),
            &mesh_static.indices[..],
        );
        self.noise_tex = Some(nt);

        let st = self.screen_tex.take().unwrap();
        let _ =
            self.canvas
                .render_geometry(&mesh_shelf.verts, Some(&st.tex), &mesh_shelf.indices[..]);
        self.mesh = Some(mesh_static);
        self.fade_mesh = Some(mesh_shelf);
        self.screen_tex = Some(st);

        let bezel = self.bezel.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.bezel = Some(bezel);
        draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            canvas_rect.height(),
            &self.nameplate,
            self.nameplate_updates,
        );

        self.canvas.set_viewport(None);
        self.present_and_time();
    }

    /// Like [`Cabinet::frame_2d_fade_in`], but for the shelf specifically
    /// (plan revision): reserves the panel column like `frame_shelf` does,
    /// and draws it flat, in the same viewport, right after the blended
    /// grid/static — the panel itself never fades with the signal, same as
    /// the in-game panel doesn't.
    pub fn frame_shelf_fade_in<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        static_level: f32,
        shelf_alpha: f32,
    ) {
        self.paint_shelf(bg, draw);
        self.update_noise_tex(static_level);
        let canvas_rect = self.canvas_rect;
        let panel = panel_rect(canvas_rect.width(), canvas_rect.height());

        self.ensure_fade_meshes(shelf_alpha);
        let mesh_static = self.mesh.take().unwrap();
        let mesh_shelf = self.fade_mesh.take().unwrap();

        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(canvas_rect));

        let nt = self.noise_tex.take().unwrap();
        let _ = self.canvas.render_geometry(
            &mesh_static.verts,
            Some(&nt.tex),
            &mesh_static.indices[..],
        );
        self.noise_tex = Some(nt);

        let st = self.screen_tex.take().unwrap();
        let _ =
            self.canvas
                .render_geometry(&mesh_shelf.verts, Some(&st.tex), &mesh_shelf.indices[..]);
        self.mesh = Some(mesh_static);
        self.fade_mesh = Some(mesh_shelf);
        self.screen_tex = Some(st);

        let bezel = self.bezel.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.bezel = Some(bezel);
        draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            canvas_rect.height(),
            &self.nameplate,
            self.nameplate_updates,
        );
        self.shelf_buttons = draw_shelf_panel(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.shelf_panel.as_ref(),
            panel,
        );
        self.close_button = draw_close_button(&mut self.canvas, &mut self.font);
        self.minimize_button = draw_minimize_button(&mut self.canvas, &mut self.font);

        self.canvas.set_viewport(None);
        self.present_and_time();
    }

    /// Fill the noise texture for the current window size at `level` (1.0 =
    /// full blizzard, 0.0 = a dim near-still hiss); leaves it in `noise_tex`.
    fn update_noise_tex(&mut self, level: f32) {
        const NW: u32 = 320;
        const NH: u32 = 240;
        // The hiss regenerates at ~30Hz (and whenever the level moves) —
        // 60Hz of fresh 307KB noise + texture upload is invisible busywork.
        let now = Instant::now();
        if let Some((t, l)) = self.noise_stamp {
            if now.duration_since(t) < Duration::from_millis(30) && (l - level).abs() < 0.005 {
                return;
            }
        }
        self.noise_stamp = Some((now, level));
        self.ensure_noise_tex(NW, NH);
        let k = level.clamp(0.0, 1.0);
        let hi = (26.0 + 150.0 * k) as u32; // cap well under white
        let lo = (6.0 * k) as u32;
        let span = hi - lo + 1;
        {
            let Self { noise, rng, .. } = &mut *self;
            if noise.len() != (NW * NH * 4) as usize {
                *noise = vec![0u8; (NW * NH * 4) as usize];
            }
            for px in noise.as_chunks_mut::<4>().0 {
                *rng ^= *rng << 13;
                *rng ^= *rng >> 17;
                *rng ^= *rng << 5;
                let v = (lo + *rng % span) as u8;
                *px = [v, v, v, 255];
            }
        }
        let nt = self.noise_tex.as_mut().unwrap();
        let _ = nt.tex.update(None, &self.noise, (NW * 4) as usize);
    }

    /// The back cover enlarged (plan revision: "ao clicar no back cover
    /// possibilitar mostrar em tamanho maior"): the image aspect-preserved as
    /// large as the *tube* allows over a dark backdrop, with a "voltar" button
    /// centred right below it — drawn into the screen buffer and warped
    /// through the CRT mesh like the shelf itself, so the zoom happens on the
    /// TV, not flat over the whole window (plan revision: "abrir ele dentro
    /// da tv"). The flat side panel keeps drawing beside it, untouched.
    /// Returns the button's rect in screen-local coordinates — hit-test it
    /// through `hit_screen_point`, not raw output coordinates.
    pub fn frame_image_zoom(&mut self, id: u64, label: &str) -> Rect {
        let (sw, sh) = self.shelf_screen_size();
        let pad = 24i32;
        let btn_h = GLYPH_H as i32 + 14;
        let btn_w = (GLYPH_W as i32 * label.chars().count() as i32) + 28;
        let btn = Rect::new(
            (sw as i32 - btn_w).max(pad) / 2,
            sh as i32 - pad - btn_h,
            btn_w as u32,
            btn_h as u32,
        );
        let img_w = sw.saturating_sub(pad as u32 * 2);
        let img_h = (sh as i32 - pad * 2 - btn_h - 14).max(60) as u32;
        self.paint_shelf((10, 10, 11), |d| {
            d.image_fit(id, pad, pad, img_w, img_h);
            // O "voltar" na linguagem fósforo (plan revision): sem caixa,
            // o texto na cor da chave do console com sublinhado.
            let text_w = GLYPH_W as i32 * label.chars().count() as i32;
            let tx = btn.x() + (btn.width() as i32 - text_w) / 2;
            let ty = btn.y() + (btn.height() as i32 - GLYPH_H as i32) / 2;
            d.text(tx, ty, 1, FOSFORO, label);
            d.fill(
                tx,
                ty + GLYPH_H as i32 + 1,
                text_w.max(GLYPH_W as i32) as u32,
                2,
                (FOSFORO.0, FOSFORO.1, FOSFORO.2, 255),
            );
        });
        self.composite_shelf();
        self.present_and_time();
        btn
    }

    /// Render `draw` into the screen buffer. Shared by `frame_2d` / `capture_2d`.
    fn paint_2d<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        self.canvas_rect = cabinet_canvas_rect(real_w, real_h);
        let ww = self.canvas_rect.width();
        self.paint_2d_avail(bg, draw, ww);
    }

    /// Like [`Cabinet::paint_2d`], but reserving the flat side-panel column
    /// first (plan revision: "o painel nao pode estar dentro da TV") — the
    /// shelf's own screen buffer only ever spans the narrower grid area.
    fn paint_shelf<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        let (real_w, real_h) = self.canvas.output_size().unwrap_or((1280, 720));
        self.canvas_rect = cabinet_canvas_rect(real_w, real_h);
        let (out_w, out_h) = (self.canvas_rect.width(), self.canvas_rect.height());
        let cab_w = out_w.saturating_sub(panel_rect(out_w, out_h).width());
        self.paint_2d_avail(bg, draw, cab_w);
    }

    /// Shared by `paint_2d`/`paint_shelf`: `self.canvas_rect` must already be
    /// set; `avail_w` is how much of its width the screen buffer gets (all
    /// of it for the plain 2D path, minus the panel for the shelf's).
    fn paint_2d_avail<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F, avail_w: u32) {
        let wh = self.canvas_rect.height();
        self.screen = screen_area(avail_w, wh);
        let (sw, sh) = (self.screen.width(), self.screen.height());
        self.ensure_screen_tex(sw, sh);
        self.ensure_bezel(avail_w, wh, self.screen);

        let Self {
            canvas,
            screen_tex,
            font,
            images,
            ..
        } = self;
        let images = &*images;
        let st = screen_tex.as_mut().unwrap();
        let _ = canvas.with_texture_canvas(&mut st.tex, |c| {
            c.set_draw_color(Color::RGB(bg.0, bg.1, bg.2));
            c.clear();
            let mut s = Screen {
                canvas: c,
                font,
                images,
                w: sw,
                h: sh,
            };
            draw(&mut s);
        });
        // The clip lives on the shared underlying renderer; clear it.
        self.canvas.set_clip_rect(ClippingRect::None);
    }

    /// Warp the screen buffer through the tube into the live window, then frame.
    fn composite_screen(&mut self) {
        // Cached mesh (same one the gameplay path uses): rebuilding 1,089
        // vertices + indices every frame was pure waste for a tube that
        // only changes shape when the window is resized.
        self.ensure_mesh(self.screen);
        let mesh = self.mesh.take().unwrap();
        self.canvas
            .set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
        self.canvas.clear();
        self.canvas.set_viewport(Some(self.canvas_rect));
        let st = self.screen_tex.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&mesh.verts, Some(&st.tex), &mesh.indices[..]);
        self.mesh = Some(mesh);
        self.screen_tex = Some(st);
        let bezel = self.bezel.take().unwrap();
        let _ = self
            .canvas
            .render_geometry(&bezel.verts, None, &bezel.indices[..]);
        self.bezel = Some(bezel);
        draw_brand(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            self.canvas_rect.height(),
            &self.nameplate,
            self.nameplate_updates,
        );
        self.draw_ra_badge();
        // Os controles do queixo em toda a tela que vive dentro da TV
        // (plan revision: "tem que estar sempre na tela" — o queixo não
        // é só do gameplay; shelf/settings/update deslizam pelo mesmo
        // plástico). Mesma função livre do frame vivo.
        let tv_on = self.tv_on();
        let chin = draw_tv_chin_controls(
            &mut self.canvas,
            &mut self.font,
            self.screen,
            self.canvas_rect.height(),
            tv_on,
        );
        self.tv_power_buttons = Some(chin);
        self.close_button = draw_close_button(&mut self.canvas, &mut self.font);
        self.minimize_button = draw_minimize_button(&mut self.canvas, &mut self.font);
        self.canvas.set_viewport(None);
    }

    /// Like [`Cabinet::composite_screen`], but also draws the settings
    /// screen's flat panel (plan revision) right after the warped content —
    /// the section buttons and "Voltar", hit-tested via
    /// `hit_settings_button`.
    fn composite_settings(&mut self) {
        self.composite_screen();
        let panel = panel_rect(self.canvas_rect.width(), self.canvas_rect.height());
        self.canvas.set_viewport(Some(self.canvas_rect));
        self.settings_buttons = draw_settings_panel(
            &mut self.canvas,
            &mut self.font,
            self.settings_panel.as_ref(),
            panel,
        );
        self.canvas.set_viewport(None);
    }

    /// Like [`Cabinet::frame_shelf`], but for the settings screen (plan
    /// revision: "a tela de configurações não está no padrão do resto do
    /// app") — the exact same split as the shelf (warped tube on the left,
    /// flat panel on the right), so the TV keeps the same proportions
    /// everywhere.
    pub fn frame_settings<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        self.paint_shelf(bg, draw);
        self.composite_settings();
        self.present_and_time();
    }

    /// Like [`Cabinet::frame_2d`], but in the idle screen's own layout (plan
    /// revision: "na tela de download fazer igual o padrão das outras telas,
    /// não esticar a tv e mostrar o painel lateral") — the screen buffer
    /// only spans the cabinet column (same split as the shelf/settings), so
    /// the TV keeps its normal proportions, and the idle side panel draws
    /// and stays clickable around it. Hit-testing inside `draw` pairs with
    /// `window_to_screen`.
    pub fn frame_idle_2d<F: FnOnce(&mut Screen)>(&mut self, bg: (u8, u8, u8), draw: F) {
        self.paint_shelf(bg, draw);
        self.composite_screen();
        let panel = panel_rect(self.canvas_rect.width(), self.canvas_rect.height());
        self.canvas.set_viewport(Some(self.canvas_rect));
        let panel_tv_on = self.tv_on();
        self.panel_buttons = draw_panel(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.panel.as_ref(),
            panel,
            self.session,
            self.idle_core_prompt.as_deref(),
            self.dev_mode,
            self.core_status.as_deref(),
            panel_tv_on,
        );
        self.canvas.set_viewport(None);
        self.present_and_time();
    }

    /// Like [`Cabinet::capture_shelf`], but drawing the settings panel
    /// (headless preview — the BMP matches what a real window would show).
    pub fn capture_settings<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        self.paint_shelf(bg, draw);

        let (ww, wh) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = self.canvas_rect;
        let panel = panel_rect(canvas_rect.width(), canvas_rect.height());
        let screen = self.screen;
        let mesh = build_crt_mesh(screen, 1.0);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, ww, wh)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let st = self.screen_tex.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        let font = &mut self.font;
        let settings_panel = self.settings_panel.as_ref();
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut close_button = self.close_button;
        let mut minimize_button = self.minimize_button;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            let _ = c.render_geometry(&mesh.verts, Some(&st.tex), &mesh.indices[..]);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            draw_brand(
                c,
                font,
                screen,
                canvas_rect.height(),
                nameplate,
                nameplate_updates,
            );
            let _ = draw_settings_panel(c, font, settings_panel, panel);
            close_button = draw_close_button(c, font);
            minimize_button = draw_minimize_button(c, font);
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        self.close_button = close_button;
        self.minimize_button = minimize_button;
        self.screen_tex = Some(st);
        self.bezel = Some(bezel);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Like [`Cabinet::frame_idle_2d`], but composited into an offscreen
    /// target and saved as a BMP (headless preview — the BMP matches what a
    /// real window would show, idle panel included).
    pub fn capture_idle_2d<F: FnOnce(&mut Screen)>(
        &mut self,
        bg: (u8, u8, u8),
        draw: F,
        path: &std::path::Path,
    ) -> Result<(), PlatformError> {
        self.paint_shelf(bg, draw);

        let (ww, wh) = self.canvas.output_size().unwrap_or((1280, 720));
        let canvas_rect = self.canvas_rect;
        let panel = panel_rect(canvas_rect.width(), canvas_rect.height());
        let screen = self.screen;
        let mesh = build_crt_mesh(screen, 1.0);
        let mut target = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, ww, wh)
            .map_err(|e| PlatformError::Sdl(e.to_string()))?;
        let st = self.screen_tex.take().unwrap();
        let bezel = self.bezel.take().unwrap();
        self.ensure_brand_logo(); // a marca do queixo na captura
        let tv_on = self.tv_on();
        let font = &mut self.font;
        let images = &self.images;
        let panel_info = self.panel.as_ref();
        let session = self.session;
        let idle_core_prompt = self.idle_core_prompt.as_deref();
        let dev_mode = self.dev_mode;
        let core_status = self.core_status.as_deref();
        let nameplate = self.nameplate.as_str();
        let nameplate_updates = self.nameplate_updates;
        let mut close_button = self.close_button;
        let mut minimize_button = self.minimize_button;
        let mut saved: Result<(), PlatformError> = Ok(());
        let outcome = self.canvas.with_texture_canvas(&mut target, |c| {
            c.set_draw_color(Color::RGB(RECESS.0, RECESS.1, RECESS.2));
            c.clear();
            c.set_viewport(Some(canvas_rect));
            let _ = c.render_geometry(&mesh.verts, Some(&st.tex), &mesh.indices[..]);
            let _ = c.render_geometry(&bezel.verts, None, &bezel.indices[..]);
            draw_brand(
                c,
                font,
                screen,
                canvas_rect.height(),
                nameplate,
                nameplate_updates,
            );
            draw_brand_logo(c, images, screen, canvas_rect.height());
            let _ = draw_panel(
                c,
                font,
                images,
                panel_info,
                panel,
                session,
                idle_core_prompt,
                dev_mode,
                core_status,
                tv_on,
            );
            close_button = draw_close_button(c, font);
            minimize_button = draw_minimize_button(c, font);
            c.set_viewport(None);
            saved = c
                .read_pixels(None::<Rect>)
                .and_then(|s| s.save_bmp(path))
                .map_err(|e| PlatformError::Sdl(e.to_string()));
        });
        self.close_button = close_button;
        self.minimize_button = minimize_button;
        self.screen_tex = Some(st);
        self.bezel = Some(bezel);
        outcome.map_err(|e| PlatformError::Sdl(e.to_string()))?;
        saved
    }

    /// Like [`Cabinet::composite_screen`], but also draws the shelf's flat
    /// side panel (plan revision) right after the warped grid, in the same
    /// viewport — undistorted, like the in-game panel already is.
    fn composite_shelf(&mut self) {
        self.composite_screen();
        let panel = panel_rect(self.canvas_rect.width(), self.canvas_rect.height());
        self.canvas.set_viewport(Some(self.canvas_rect));
        self.shelf_buttons = draw_shelf_panel(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.shelf_panel.as_ref(),
            panel,
        );
        self.canvas.set_viewport(None);
    }

    fn ensure_screen_tex(&mut self, w: u32, h: u32) {
        if matches!(&self.screen_tex, Some(s) if s.w == w && s.h == h) {
            return;
        }
        let mut tex = self
            .canvas
            .create_texture_target(SdlFormat::RGBA32, w, h)
            .expect("create screen target");
        tex.set_scale_mode(SdlScaleMode::Linear);
        // So `frame_2d_fade_in`'s vertex alpha can blend it over the snow.
        tex.set_blend_mode(BlendMode::Blend);
        self.screen_tex = Some(SizedTex { tex, w, h });
    }

    fn ensure_noise_tex(&mut self, w: u32, h: u32) {
        if matches!(&self.noise_tex, Some(s) if s.w == w && s.h == h) {
            return;
        }
        let mut tex = self
            .canvas
            .create_texture_streaming(SdlFormat::RGBA32, w, h)
            .expect("create noise texture");
        tex.set_scale_mode(SdlScaleMode::Linear);
        self.noise_tex = Some(SizedTex { tex, w, h });
    }
}

/// A 2D drawing surface, screen-local coordinates (0,0 = top-left of the tube
/// opening). Handed to the `frame_2d` / `capture_2d` closure.
pub struct Screen<'a> {
    canvas: &'a mut WindowCanvas,
    font: &'a mut Texture,
    images: &'a HashMap<u64, ImgTex>,
    w: u32,
    h: u32,
}

impl Screen<'_> {
    /// Size of the drawing surface.
    pub fn size(&self) -> (u32, u32) {
        (self.w, self.h)
    }

    /// As dimensões da imagem registrada sob `id` — o tile da capa usa
    /// para abraçar a proporção real da página 1 do PDF.
    pub fn image_size(&self, id: u64) -> Option<(u32, u32)> {
        self.images.get(&id).map(|i| (i.w, i.h))
    }

    pub fn fill(&mut self, x: i32, y: i32, w: u32, h: u32, c: (u8, u8, u8, u8)) {
        self.canvas.set_draw_color(Color::RGBA(c.0, c.1, c.2, c.3));
        let _ = self.canvas.fill_rect(Rect::new(x, y, w, h));
    }

    /// A small filled five-point star centred on (cx, cy) — the favorite
    /// marker (plan revision: "o icone de favorito coloca uma estrela
    /// vermelha"). The font atlas has no star glyph, so it's a polygon: ten
    /// points alternating outer/inner radius, filled as a fan.
    pub fn star(&mut self, cx: i32, cy: i32, r: u32, c: (u8, u8, u8)) {
        let (cxf, cyf, rf) = (cx as f32, cy as f32, r as f32);
        let point = |k: usize| {
            let radius = if k.is_multiple_of(2) { rf } else { rf * 0.45 };
            let angle = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
            sdl3::render::FPoint::new(cxf + radius * angle.cos(), cyf + radius * angle.sin())
        };
        let vtx = |p: sdl3::render::FPoint| Vertex {
            position: p,
            color: FColor::RGBA(
                c.0 as f32 / 255.0,
                c.1 as f32 / 255.0,
                c.2 as f32 / 255.0,
                1.0,
            ),
            tex_coord: sdl3::render::FPoint::new(0.0, 0.0),
        };
        let mut verts = vec![vtx(sdl3::render::FPoint::new(cxf, cyf))];
        for k in 0..10 {
            verts.push(vtx(point(k)));
        }
        let mut indices = Vec::with_capacity(30);
        for k in 1..=10 {
            indices.extend_from_slice(&[0, k, k % 10 + 1]);
        }
        let _ = self
            .canvas
            .render_geometry(&verts, None::<&Texture>, &indices[..]);
    }

    pub fn outline(&mut self, x: i32, y: i32, w: u32, h: u32, thick: u32, c: (u8, u8, u8, u8)) {
        let t = thick as i32;
        self.fill(x, y, w, thick, c);
        self.fill(x, y + h as i32 - t, w, thick, c);
        self.fill(x, y, thick, h, c);
        self.fill(x + w as i32 - t, y, thick, h, c);
    }

    /// Draw `s` at `(x, y)`, `scale`x the glyph cell. Returns the advance width.
    pub fn text(&mut self, x: i32, y: i32, scale: u32, c: (u8, u8, u8), s: &str) -> i32 {
        self.font.set_color_mod(c.0, c.1, c.2);
        let cell = (GLYPH_W * scale) as i32;
        let mut pen = x;
        for ch in s.chars() {
            let idx = glyph_index(ch);
            if ch != ' ' {
                let src = Rect::new(idx as i32 * GLYPH_W as i32, 0, GLYPH_W, GLYPH_H);
                let dst = Rect::new(pen, y, GLYPH_W * scale, GLYPH_H * scale);
                let _ = self.canvas.copy(self.font, src, dst);
            }
            pen += cell;
        }
        pen - x
    }

    /// Word-wrap `s` into `max_w`, returning the y past the last line.
    pub fn text_wrapped(
        &mut self,
        x: i32,
        y: i32,
        max_w: u32,
        scale: u32,
        c: (u8, u8, u8),
        s: &str,
    ) -> i32 {
        let cols = (max_w / (GLYPH_W * scale)).max(1) as usize;
        let row = (GLYPH_H * scale) as i32;
        let mut line = String::new();
        let mut line_len = 0usize;
        let mut cy = y;
        for word in s.split_whitespace() {
            let word_len = word.chars().count();
            if !line.is_empty() && line_len + 1 + word_len > cols {
                self.text(x, cy, scale, c, &line);
                cy += row + 2;
                line.clear();
                line_len = 0;
            }
            if !line.is_empty() {
                line.push(' ');
                line_len += 1;
            }
            line.push_str(word);
            line_len += word_len;
            // `.chars()` (not `split_at`, byte-indexed): a wrap point that
            // fell mid-character would panic on any accented/multi-byte word.
            while line_len > cols {
                let head: String = line.chars().take(cols).collect();
                let tail: String = line.chars().skip(cols).collect();
                self.text(x, cy, scale, c, &head);
                cy += row + 2;
                line = tail;
                line_len = line.chars().count();
            }
        }
        if !line.is_empty() {
            self.text(x, cy, scale, c, &line);
            cy += row + 2;
        }
        cy
    }

    /// Height [`Screen::text_wrapped`] would take for `s`, without drawing.
    pub fn wrapped_height(&self, max_w: u32, scale: u32, s: &str) -> i32 {
        wrapped_height(max_w, scale, s)
    }

    /// Clip drawing to `rect`; `None` clears the clip.
    pub fn clip(&mut self, rect: Option<(i32, i32, u32, u32)>) {
        self.canvas.set_clip_rect(match rect {
            Some((x, y, w, h)) => ClippingRect::Some(Rect::new(x, y, w, h)),
            None => ClippingRect::None,
        });
    }

    pub fn has_image(&self, id: u64) -> bool {
        self.images.contains_key(&id)
    }

    /// A imagem `id` tem transparência de verdade (fundo vazado) — o gêmeo
    /// do [`Cabinet::image_vazado`] para o desenho da estante.
    pub fn image_vazado(&self, id: u64) -> bool {
        self.images.get(&id).is_some_and(|img| img.vazado)
    }

    /// Draw image `id` letterboxed inside the box, centered. No-op if unknown.
    pub fn image_fit(&mut self, id: u64, x: i32, y: i32, bw: u32, bh: u32) {
        let Some(img) = self.images.get(&id) else {
            return;
        };
        let (iw, ih) = (img.w as f32, img.h as f32);
        let scale = (bw as f32 / iw).min(bh as f32 / ih);
        let dw = (iw * scale).round() as i32;
        let dh = (ih * scale).round() as i32;
        let dx = x + (bw as i32 - dw) / 2;
        let dy = y + (bh as i32 - dh) / 2;
        let _ = self.canvas.copy(
            &img.tex,
            None::<sdl3::render::FRect>,
            Rect::new(dx, dy, dw.max(1) as u32, dh.max(1) as u32),
        );
    }

    /// A arte preenchendo 100% do rect (`object-fit: cover`): escala pelo
    /// maior fator e recorta o excesso, centrado na horizontal e com a
    /// base no fundo do rect — a capa da estante sempre cheia no tile.
    pub fn image_fill(&mut self, id: u64, x: i32, y: i32, bw: u32, bh: u32) {
        let Some(img) = self.images.get(&id) else {
            return;
        };
        let (iw, ih) = (img.w as f32, img.h as f32);
        let scale = (bw as f32 / iw).max(bh as f32 / ih);
        let dw = (iw * scale).round() as i32;
        let dh = (ih * scale).round() as i32;
        let dx = x + ((bw as i32 - dw) / 2);
        let dy = y + (bh as i32 - dh); // base ancorada no fundo
        self.canvas.set_clip_rect(Some(Rect::new(x, y, bw, bh)));
        let _ = self.canvas.copy(
            &img.tex,
            None::<sdl3::render::FRect>,
            Rect::new(dx, dy, dw.max(1) as u32, dh.max(1) as u32),
        );
        self.canvas.set_clip_rect(None);
    }

    /// A página do leitor em MODO ZOOM: a página cabe na LARGURA do rect
    /// (muito maior que o fit da área) e rola na VERTICAL com `pan`
    /// (0.0 = topo, 1.0 = fim) — leitura de revista de verdade, roda da
    /// roda do mouse para cima e para baixo.
    pub fn image_scroll_v(&mut self, id: u64, rect: (i32, i32, u32, u32), pan: f32) {
        let Some(img) = self.images.get(&id) else {
            return;
        };
        let (rw, rh) = (rect.2 as f32, rect.3 as f32);
        let aspect = img.w as f32 / img.h.max(1) as f32;
        let dw = rw;
        let dh = rw / aspect.max(0.0001);
        let max_off = (dh - rh).max(0.0);
        let dx = rect.0 as f32;
        let dy = rect.1 as f32 - max_off * pan.clamp(0.0, 1.0);
        self.canvas
            .set_clip_rect(Some(Rect::new(rect.0, rect.1, rect.2, rect.3)));
        let _ = self.canvas.copy(
            &img.tex,
            None::<sdl3::render::FRect>,
            Rect::new(
                dx.round() as i32,
                dy.round() as i32,
                dw.max(1.0) as u32,
                dh.max(1.0) as u32,
            ),
        );
        self.canvas.set_clip_rect(None);
    }

    /// Como [`Screen::image_fit`], com a arte ancorada na BASE do rect —
    /// para capa vazada (render 3D em ângulo, fundo transparente): a caixa
    /// pousa com a base na prateleira em vez de flutuar centrada.
    /// O tamanho do conteúdo da imagem `id` (sem as margens transparentes
    /// da arte vazada) — a estante calcula o retângulo da capa antes de
    /// desenhá-la, para o halo de seleção abraçar a capa.
    pub fn image_content_size(&self, id: u64) -> Option<(u32, u32)> {
        self.images
            .get(&id)
            .map(|i| (i.content.2.max(1), i.content.3.max(1)))
    }

    /// Como [`Screen::image_fit_bottom`], mas recortando as margens
    /// transparentes da arte (o `content` da textura): a caixa 3D vazada
    /// ocupa a mesma altura que uma capa chapada, em vez de encolher pelo
    /// canvas vazio em volta. Devolve o retângulo efetivamente desenhado
    /// (`None` se a imagem não existe) — a estante veste a capa por ele.
    pub fn image_fit_content_bottom(
        &mut self,
        id: u64,
        x: i32,
        y: i32,
        bw: u32,
        bh: u32,
    ) -> Option<Rect> {
        let img = self.images.get(&id)?;
        let (cx, cy, cw, ch) = img.content;
        let (iw, ih) = (cw.max(1) as f32, ch.max(1) as f32);
        let scale = (bw as f32 / iw).min(bh as f32 / ih);
        let dw = ((iw * scale).round() as i32).max(1);
        let dh = ((ih * scale).round() as i32).max(1);
        let dst = Rect::new(
            x + (bw as i32 - dw) / 2,
            y + bh as i32 - dh,
            dw as u32,
            dh as u32,
        );
        let src = Rect::new(cx as i32, cy as i32, cw.max(1), ch.max(1));
        let _ = self.canvas.copy(&img.tex, src, dst);
        Some(dst)
    }

    pub fn image_fit_bottom(&mut self, id: u64, x: i32, y: i32, bw: u32, bh: u32) {
        let Some(img) = self.images.get(&id) else {
            return;
        };
        let (iw, ih) = (img.w as f32, img.h as f32);
        let scale = (bw as f32 / iw).min(bh as f32 / ih);
        let dw = (iw * scale).round() as i32;
        let dh = (ih * scale).round() as i32;
        let dx = x + (bw as i32 - dw) / 2;
        let dy = y + bh as i32 - dh;
        let _ = self.canvas.copy(
            &img.tex,
            None::<sdl3::render::FRect>,
            Rect::new(dx, dy, dw.max(1) as u32, dh.max(1) as u32),
        );
    }
}

/// Image id the RA mock example registers its badge under.
pub const DEMO_BADGE_IMG: u64 = u64::MAX;

/// Image id for the RetroAchievements logo the app registers (the favicon,
/// embedded in the app crate) — the persistent chin badge draws it instead
/// of the pixel gamepad when it's there.
pub const RA_LOGO_IMG: u64 = u64::MAX - 7;

/// A marca "SixteeN" (arte prateada pronta, RGBA vazado) — embutida na
/// crate e registrada no mapa de imagens no primeiro frame.
const BRAND_LOGO_ID: u64 = u64::MAX - 8;
const BRAND_LOGO_W: u32 = 1724;
const BRAND_LOGO_H: u32 = 281;
const BRAND_LOGO_RGBA: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/assets/brand_sixteen.rgba"
));

impl Cabinet {
    /// Drop expired fronts, then draw the front block — or, with the queue
    /// empty, the persistent RetroAchievements badge (when armed).
    fn draw_osd_front(&mut self) {
        let now = Instant::now();
        while self.osd_queue.front().is_some_and(|e| e.until <= now) {
            self.osd_queue.pop_front();
        }
        let (_, out_h) = self.canvas.output_size().unwrap_or((1280, 720));
        let cw = self.canvas_rect.width() as i32;
        let Some(entry) = self.osd_queue.front() else {
            if let Some(status) = self.ra_status {
                draw_chin_ra(
                    &mut self.canvas,
                    &mut self.font,
                    &self.images,
                    self.screen,
                    cw,
                    &self.nameplate,
                    status.hardcore,
                );
            }
            return;
        };
        draw_chin_osd(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.screen,
            out_h,
            &self.nameplate,
            &entry.lines,
            entry.badge,
        );
    }

    /// `draw_osd_front`'s decision, for the offscreen capture path (whose
    /// closure can't take `&mut self`) — expire and decide instead.
    fn osd_for_capture(&self) -> ChinOsd {
        let now = Instant::now();
        if let Some(e) = self.osd_queue.front().filter(|e| e.until > now) {
            return ChinOsd::Notify {
                lines: e.lines.clone(),
                badge: e.badge,
            };
        }
        match self.ra_status {
            Some(s) => ChinOsd::Ra {
                hardcore: s.hardcore,
            },
            None => ChinOsd::None,
        }
    }

    /// The persistent RA badge, for every chin that isn't the gameplay one
    /// (`composite_screen` covers início/estante/configurações; the idle's
    /// `present_static` and its capture call it directly).
    fn draw_ra_badge(&mut self) {
        let Some(status) = self.ra_status else {
            return;
        };
        let cw = self.canvas_rect.width() as i32;
        draw_chin_ra(
            &mut self.canvas,
            &mut self.font,
            &self.images,
            self.screen,
            cw,
            &self.nameplate,
            status.hardcore,
        );
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn perf_line_omits_unavailable_segments_and_stays_ascii() {
        use super::perf_line;
        assert_eq!(
            perf_line(34, Some(52), Some(84)),
            "34 FPS  CPU 52C  RAM 84 MB"
        );
        assert_eq!(perf_line(60, None, None), "60 FPS");
        assert_eq!(perf_line(60, Some(61), None), "60 FPS  CPU 61C");
    }

    use super::{
        fit_aspect_in, panel_cartridge_rects, panel_slot_base, panel_slot_mouth, ra_badge_rows,
        screen_area, wrapped_height, CART_WIDTH_FRAC, GLYPH_H, OSD_GREEN, SEAT_HIDDEN_FRAC,
    };
    use sdl3::rect::Rect;

    #[test]
    fn ra_badge_rows_pin_the_ativado_contract() {
        let hardcore = ra_badge_rows(true);
        let softcore = ra_badge_rows(false);
        // A primeira linha é sempre o "ativado" verde do OSD; o modo muda
        // de texto e de cor (âmbar no hardcore, cinza claro no softcore).
        // O texto fala `tr!`, então o pin cobre os dois idiomas.
        let ativado = crate::tr!("RA ENABLED", "RA ATIVADO");
        assert_eq!(hardcore[0], (ativado, OSD_GREEN));
        assert_eq!(softcore[0], (ativado, OSD_GREEN));
        assert_eq!(hardcore[1].0, "HARDCORE");
        assert_eq!(softcore[1].0, "SOFTCORE");
        assert_ne!(hardcore[1].1, softcore[1].1);
    }

    #[test]
    fn panel_slot_base_and_mouth_nest_in_the_block() {
        let block = Rect::new(40, 100, 320, 210);
        let base = panel_slot_base(block);
        let mouth = panel_slot_mouth(block, base);
        // The base is a full-width slab at the block's bottom edge; the
        // mouth sits inside it, near the base's top.
        assert_eq!(base.width(), block.width());
        assert_eq!(base.bottom(), block.bottom());
        assert!(base.y() > block.y());
        assert!(mouth.x() >= base.x() && mouth.right() <= base.right());
        assert!(mouth.y() >= base.y() && mouth.bottom() <= base.bottom());
        // Never collapses on a sliver of a block.
        let b = panel_slot_base(Rect::new(0, 0, 4, 4));
        let m = panel_slot_mouth(Rect::new(0, 0, 4, 4), b);
        assert!(b.width() >= 1 && m.width() >= 1 && m.height() >= 1);
    }

    #[test]
    fn panel_cartridge_enters_from_above_and_seats_in_the_slot() {
        let block = Rect::new(40, 100, 320, 270);
        let base = panel_slot_base(block);
        let mouth = panel_slot_mouth(block, base);
        // t=0: entirely above the block — the clip (block top .. mouth top)
        // shows nothing yet, the slot sits empty.
        let (r0, c0) = panel_cartridge_rects(0.0, false, block, mouth, 700, 500, (0, 0, 700, 500));
        assert!(r0.bottom() <= block.y());
        assert_eq!(c0.y(), block.y());
        assert_eq!(c0.height() as i32, mouth.y() - block.y());
        // t=1: seated — the content's base is past the mouth's top edge
        // (hidden by the clip) while the label stays inside the block,
        // standing proud of the console.
        let (r1, _) = panel_cartridge_rects(1.0, false, block, mouth, 700, 500, (0, 0, 700, 500));
        assert!(r1.bottom() >= mouth.y());
        assert!(r1.top() >= block.y());
    }

    #[test]
    fn panel_cartridge_seats_by_content_not_canvas() {
        let block = Rect::new(40, 100, 320, 270);
        let base = panel_slot_base(block);
        let mouth = panel_slot_mouth(block, base);
        // Art with fat transparent margins (like the real scans): the fit
        // and the seating key off the opaque body, not the canvas — the
        // body spans the annotated width inside the base and its base meets
        // the mouth, with no float gap.
        let content = (100u32, 50u32, 500u32, 400u32);
        let (r1, _) = panel_cartridge_rects(1.0, false, block, mouth, 700, 500, content);
        let scale = (block.width() as f32 * CART_WIDTH_FRAC / 500.0)
            .min(((mouth.y() - block.y() - 2) as f32) / (400.0 * (1.0 - SEAT_HIDDEN_FRAC)));
        // The content's centre sits on the block's centre.
        let content_cx = r1.x() as f32 + (100.0 + 250.0) * scale;
        assert!((content_cx - (block.x() + block.width() as i32 / 2) as f32).abs() <= 1.0);
        // Seated: the content's base is just past the lip, its top inside
        // the block.
        let content_bottom = r1.y() as f32 + (50.0 + 400.0) * scale;
        let hidden = SEAT_HIDDEN_FRAC * 400.0 * scale;
        assert!((content_bottom - (mouth.y() as f32 + hidden)).abs() <= 1.0);
        assert!(r1.y() as f32 + 50.0 * scale >= block.y() as f32);
        // Entering: the content is entirely above the block.
        let (r0, _) = panel_cartridge_rects(0.0, false, block, mouth, 700, 500, content);
        assert!(r0.y() as f32 + 450.0 * scale <= block.y() as f32 + 0.5);
    }

    #[test]
    fn content_bbox_finds_the_opaque_region() {
        // 4x3 image: only the middle row's middle two pixels are opaque.
        let mut rgba = vec![0u8; 4 * 3 * 4];
        for (x, y) in [(1usize, 1usize), (2, 1)] {
            rgba[(y * 4 + x) * 4 + 3] = 255;
        }
        assert_eq!(super::content_bbox(4, 3, &rgba), (1, 1, 2, 1));
        // A fully opaque image gets the full canvas; a fully transparent
        // one falls back to it too (no empty box to fit).
        let solid = vec![255u8; 2 * 2 * 4];
        assert_eq!(super::content_bbox(2, 2, &solid), (0, 0, 2, 2));
        assert_eq!(super::content_bbox(2, 2, &[0u8; 2 * 2 * 4]), (0, 0, 2, 2));
    }

    #[test]
    fn panel_cartridge_eject_mirrors_insert_exactly() {
        let block = Rect::new(40, 100, 320, 270);
        let base = panel_slot_base(block);
        let mouth = panel_slot_mouth(block, base);
        // Eject at t=0 continues from insert's t=1 spot (seated either way),
        // and ends where insert began (entirely above the block, gone).
        let (seated_insert, _) =
            panel_cartridge_rects(1.0, false, block, mouth, 700, 500, (0, 0, 700, 500));
        let (seated_eject, _) =
            panel_cartridge_rects(0.0, true, block, mouth, 700, 500, (0, 0, 700, 500));
        assert_eq!(seated_insert, seated_eject);
        let (risen, _) = panel_cartridge_rects(1.0, true, block, mouth, 700, 500, (0, 0, 700, 500));
        assert!(risen.bottom() <= block.y());
    }

    #[test]
    fn screen_area_insets_with_a_wider_chin() {
        let s = screen_area(1000, 1000);
        assert_eq!(s.x(), 70); // 7% side
        assert_eq!(s.y(), 50); // 5% top (plan revision: moldura superior fina)
        assert_eq!(s.width(), 860); // 1000 - 2*70
        assert_eq!(s.height(), 855); // 1000 - 50 top - 95 chin (9.5%)
                                     // Never collapses to nothing on a tiny window.
        assert!(screen_area(4, 4).width() >= 16);
    }

    #[test]
    fn fit_aspect_in_centers_a_43_rect() {
        // A wide area: 4:3 fills the height, centered horizontally.
        let r = fit_aspect_in(Rect::new(0, 0, 800, 300), 4.0 / 3.0);
        assert_eq!(r.height(), 300);
        assert_eq!(r.width(), 400);
        assert_eq!(r.x(), 200);
        assert_eq!(r.y(), 0);
        // A tall area: 4:3 fills the width instead.
        let r = fit_aspect_in(Rect::new(0, 0, 400, 900), 4.0 / 3.0);
        assert_eq!(r.width(), 400);
        assert_eq!(r.height(), 300);
        assert_eq!(r.y(), 300);
    }

    #[test]
    fn wrapped_height_counts_lines() {
        let row = GLYPH_H as i32 + 2; // one line's advance at scale 1
        assert_eq!(wrapped_height(90, 1, ""), 0);
        // 90px / 9px advance = 10 cols. "hello world" -> "hello" + " world"
        // = 11 > 10, so two lines.
        assert_eq!(wrapped_height(90, 1, "hello world"), 2 * row);
        // Fits on one line.
        assert_eq!(wrapped_height(90, 1, "hello you"), row);
        // A single word longer than the line is hard-split.
        assert_eq!(wrapped_height(90, 1, &"x".repeat(25)), 3 * row);
    }

    #[test]
    fn atlas_char_traduz_o_que_o_atlas_nao_tem() {
        use super::{atlas_char, glyph_index};
        // pontuação tipográfica e setas viram o ASCII mais próximo — nada de
        // "?" na copy (plan revision: "corrigir copy com ?")
        assert_eq!(atlas_char('—'), '-');
        assert_eq!(atlas_char('–'), '-');
        assert_eq!(atlas_char('•'), '-');
        assert_eq!(atlas_char('…'), '.');
        assert_eq!(atlas_char('←'), '<');
        assert_eq!(atlas_char('→'), '>');
        assert_eq!(atlas_char('↑'), '^');
        assert_eq!(atlas_char('↓'), 'v');
        assert_eq!(atlas_char('‘'), '\'');
        assert_eq!(atlas_char('“'), '"');
        assert_eq!(atlas_char('\u{3000}'), ' ');
        // full-width é o espelho ASCII do bloco U+FF01..U+FF5E
        assert_eq!(atlas_char('Ａ'), 'A');
        assert_eq!(atlas_char('１'), '1');
        // o Latin-1 que o atlas TEM passa intocado — acento usa o glifo real
        assert_eq!(atlas_char('ç'), 'ç');
        assert_eq!(atlas_char('Õ'), 'Õ');
        // e o que não tem tradução cai no '?' de sempre, desenhável
        assert_eq!(glyph_index('—'), glyph_index('-'));
        assert_ne!(glyph_index('—'), glyph_index('?'));
        assert_eq!(glyph_index('⌘'), glyph_index('?'));
    }
}

#[cfg(test)]
mod canvas_rect_tests {
    use super::cabinet_canvas_rect;

    #[test]
    fn deck_and_16x9_displays_fill_natively() {
        // Steam Deck 1280x800 (16:10): the canvas fills the display — no
        // letterbox bars (plan revision: melhor experiencia no Deck).
        assert_eq!(
            cabinet_canvas_rect(1280, 800),
            sdl3::rect::Rect::new(0, 0, 1280, 800)
        );
        // 16:9 TVs/monitors: unchanged, fills exactly.
        assert_eq!(
            cabinet_canvas_rect(1920, 1080),
            sdl3::rect::Rect::new(0, 0, 1920, 1080)
        );
        assert_eq!(
            cabinet_canvas_rect(1280, 720),
            sdl3::rect::Rect::new(0, 0, 1280, 720)
        );
    }

    #[test]
    fn ultrawide_keeps_the_16x9_letterbox() {
        // 3440x1440 (21:9): 16:9 canvas centered, side bars.
        let r = cabinet_canvas_rect(3440, 1440);
        assert_eq!((r.width(), r.height()), (2560, 1440));
        assert_eq!(r.x(), 440);
        assert_eq!(r.y(), 0);
    }

    #[test]
    fn shapes_outside_the_band_clamp_to_it() {
        // 4:3 display: clamps to 16:10 (narrower pillarbox than 16:9).
        let r = cabinet_canvas_rect(1280, 1024);
        assert_eq!((r.width(), r.height()), (1280, 800));
        assert_eq!(r.y(), 112);
        // 3:2 (Surface): clamps to 16:10 as well.
        let r = cabinet_canvas_rect(1500, 1000);
        assert_eq!((r.width(), r.height()), (1500, 938));
        assert_eq!(r.y(), 31);
    }
}
