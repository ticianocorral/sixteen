//! The emulator run-loop, factored out of the `emu-run` binary so the unified
//! `sixteen` binary can call it between selector visits. Presentation is fixed:
//! RF NTSC + CRT-tube warp (see docs/fase-0.md).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sixteen_domain::fsutil::write_atomic;

use anyhow::{anyhow, Context, Result};
use sixteen_emulation::{Button, Core, Frame as EmuFrame, PixelFormat as EmuFormat};
use sixteen_ntsc::NtscFilter;
use sixteen_platform::tr;
use sixteen_platform::{
    Cabinet, FrameRef, Osk, OskOutcome, PanelButton, PixelFormat as PlatFormat, Platform,
    TextField, UiEvent, MAX_PORTS,
};

use crate::config::Config;

/// How often to flush battery SRAM to disk while playing (frames ≈ 10 s).
const SRAM_FLUSH_FRAMES: u32 = 600;
/// Save-state slots, `0`..`9`.
const SLOTS: u8 = 10;
/// Max characters a pause-book free-text note may hold (plan revision) —
/// enforced live, as it's typed, not just on save.
const NOTE_CHAR_LIMIT: usize = 240;
/// The dim, near-still hiss the screen idles at once the console is off
/// (plan §3.3) — also what the next screen fades in from, and the idle/root
/// screen's resting level (`crate::idle`).
pub(crate) const OFF_STATIC_LEVEL: f32 = 0.12;

/// Why the run-loop returned.
/// How long an unlock notification stays up (plan fase 4).
const OSD_UNLOCK_TTL: Duration = Duration::from_secs(6);

/// Stable image id for a badge name — distinct namespace from the art
/// hashes (their ids are sha1-derived u64s; this flips a high bit).
fn osd_badge_id(badge: &str) -> u64 {
    // FNV-1a: stable, dependency-free, distinct namespace from the art ids
    // (which are sha1-derived; this flips the top bit).
    let mut v: u64 = 0xcbf2_9ce4_8422_2325;
    for b in badge.as_bytes() {
        v ^= *b as u64;
        v = v.wrapping_mul(0x0000_0100_0000_01b3);
    }
    v | 0x8000_0000_0000_0000
}

pub enum GameExit {
    /// Ejected — the caller should show the idle/root screen again (not the
    /// shelf directly). Carries the signal-off static level the screen
    /// settled on, so the next screen can fade in over it instead of a fresh
    /// burst (plan §3.3, "a estante entra por cima").
    Ejected { static_level: f32 },
    /// Window close / Cmd-Q / headless self-check done — tear the app down.
    Quit,
}

/// Everything [`run_game`] needs for one session.
pub struct GameSpec {
    pub core: PathBuf,
    pub rom: PathBuf,
    pub system_dir: PathBuf,
    pub save_dir: PathBuf,
    /// Where per-game notebooks live: `<title>/01.png`..`15.png` for the
    /// photo slots, `<title>/01.txt`..`15.txt` for the text-note slots
    /// (both fixed at 15, plan revision — numbered independently of each
    /// other) (plan §3.4).
    pub notes_dir: PathBuf,
    /// Headless self-check: `(path, frame)` — run to `frame`, dump a BMP, exit.
    pub shot: Option<(PathBuf, u32)>,
    /// Local logo art (`assets/logo/<rom>.*`) for the top of the side panel.
    /// `None` shows the ROM's name instead (plan §3.2, item 1).
    pub logo: Option<PathBuf>,
    /// Local cartridge art (`assets/cartridge/<rom>.*`), shown in the panel
    /// alongside the logo when present (plan revision) — `None` just skips
    /// that block, no fallback needed.
    pub cartridge: Option<PathBuf>,
    /// Headless self-check: skip straight to the console-off signal-off snow
    /// and save `shot` there, instead of running the game to `shot`'s frame
    /// count.
    pub shot_off: bool,
    /// Headless self-check: force one `NoteCapture` at `shot`'s frame (or
    /// frame 1 without one), exactly like clicking "Nota" live — so `--shot`
    /// can prove out the panel's notebook block without a window.
    pub debug_note_capture: bool,
    /// Headless self-check: skip straight to the pause book (plan §3.2/§3.4)
    /// instead of gameplay, so `--shot` can prove it out against whatever
    /// notes already exist on disk for this ROM.
    pub debug_shot_pause: bool,
    /// Headless self-check: skip straight to one of the save/load-state or
    /// print slot pickers (plan revision) instead of gameplay — `"save"`,
    /// `"load"`, or `"print"`; anything else is ignored (no modal shown).
    pub debug_shot_modal: Option<String>,
    /// Dev/testing: `Some(dir)` salva frames da animação de inserção do
    /// cartucho em `dir` (o gif do site é gerado com isto) e sai, sem
    /// live loop.
    pub debug_cart_anim: Option<std::path::PathBuf>,
    /// `emu-run --debug-blow DIR`: as telas de mau contato e a cena do
    /// sopro em BMPs (headless), no lugar do jogo.
    pub debug_blow: Option<std::path::PathBuf>,
}

pub(crate) const PAD: [(Button, sixteen_platform::PadButton); 12] = {
    use sixteen_platform::PadButton as P;
    [
        (Button::B, P::B),
        (Button::Y, P::Y),
        (Button::Select, P::Select),
        (Button::Start, P::Start),
        (Button::Up, P::Up),
        (Button::Down, P::Down),
        (Button::Left, P::Left),
        (Button::Right, P::Right),
        (Button::A, P::A),
        (Button::X, P::X),
        (Button::L, P::L),
        (Button::R, P::R),
    ]
};

fn map_format(f: EmuFormat) -> PlatFormat {
    match f {
        EmuFormat::Rgb1555 => PlatFormat::Rgb1555,
        EmuFormat::Xrgb8888 => PlatFormat::Xrgb8888,
        EmuFormat::Rgb565 => PlatFormat::Rgb565,
    }
}

/// A path-hostile character in a game title, replaced with `_` so it can't
/// escape its parent directory or fail to create — shared by every per-game
/// folder (`saves/<title>/`, `notes/<title>/`), and by `rom_rename` (plan
/// revision) when it turns a No-Intro name into a ROM file name.
pub(crate) fn sanitize_dir_name(title: &str) -> String {
    let safe: String = title
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_control()
                || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '_'
            } else {
                c
            }
        })
        .collect::<String>();
    // Pontos e espaços no fim somem (o Windows os descarta, e "x." viraria
    // outra pasta que "x"); um nome feito só de pontos ("." / "..") apontaria
    // para a pasta atual ou a de cima — vira "___" como o nome vazio. Teto
    // de 200 bytes, abaixo do limite de nome de arquivo dos sistemas comuns.
    let mut safe = safe.trim_end_matches(['.', ' ']).to_string();
    if safe.len() > 200 {
        let mut cut = 200;
        while !safe.is_char_boundary(cut) {
            cut -= 1;
        }
        safe.truncate(cut);
        safe = safe.trim_end_matches(['.', ' ']).to_string();
    }
    if safe.is_empty() {
        "___".to_string()
    } else {
        safe
    }
}

/// `save_dir/<game title>/` — one folder per game, named for a human
/// browsing it rather than the ROM hash (plan revision — mirrors
/// `note_dir`: readability over rename-proofing nobody asked for here).
fn game_dir(save_dir: &Path, title: &str) -> PathBuf {
    save_dir.join(sanitize_dir_name(title))
}

/// `save_dir/<title>/<slot>.state` — plain slot number, no hash prefix
/// (plan revision).
fn state_file(save_dir: &Path, title: &str, slot: u8) -> PathBuf {
    game_dir(save_dir, title).join(format!("{slot}.state"))
}

/// `save_dir/<title>/sram.srm` — battery SRAM, one file per game (plan
/// revision — used to be `<hash>.srm` directly under `save_dir`).
fn sram_file(save_dir: &Path, title: &str) -> PathBuf {
    game_dir(save_dir, title).join("sram.srm")
}

/// Lê a SRAM do disco: `Ok(None)` quando ainda não existe (jogo novo);
/// `Err` para qualquer outra falha — EIO no microSD, permissão, arquivo
/// travado por antivírus/OneDrive. Separar os dois é o que impede uma
/// leitura que falhou de virar "SRAM vazia" e ser gravada por cima do save.
fn read_sram(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Para onde a SRAM da sessão vai quando o `sram.srm` existe mas não pôde
/// ser lido: um arquivo ao lado, para o save original nunca ser tocado.
fn sram_aside_path(sram: &Path) -> PathBuf {
    sram.with_file_name("sram-sessao-sem-leitura.srm")
}

/// `save_dir/<title>/cheats.txt` — plain text either way, so the extension
/// might as well say so (plan revision — used to be `<hash>.cheats`).
fn cheat_state_path(save_dir: &Path, title: &str) -> PathBuf {
    game_dir(save_dir, title).join("cheats.txt")
}

/// `save_dir/<title>/playtime.txt` — total seconds ever spent powered on,
/// plain text like `cheats.txt` (plan revision: "mostrar tempo total de
/// jogo do game").
fn playtime_path(save_dir: &Path, title: &str) -> PathBuf {
    game_dir(save_dir, title).join("playtime.txt")
}

/// The per-game folder name `run_game` itself uses — the ROM's file stem,
/// not the shelf's `CatalogEntry::title()` (which prefers a No-Intro/
/// internal name when one exists). Exposed so the shelf can read
/// `total_playtime_secs` for the exact folder a session actually wrote to,
/// instead of guessing at a name that might not match.
pub fn rom_title(rom_path: &Path) -> String {
    rom_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "???".to_string())
}

/// Total time this game has spent powered on, across every session ever
/// played — `0` for a game never played (no file yet) or a corrupt one
/// (never worth failing the panel over one bad number).
pub fn total_playtime_secs(save_dir: &Path, title: &str) -> u64 {
    fs::read_to_string(playtime_path(save_dir, title))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

/// Add this session's powered-on seconds to the running total and save it
/// back — best-effort, same as the other per-game sidecars; a `0` (the
/// console was never turned on this session) skips the write entirely.
fn add_playtime(save_dir: &Path, title: &str, secs: u64) {
    if secs == 0 {
        return;
    }
    let total = total_playtime_secs(save_dir, title) + secs;
    let _ = fs::create_dir_all(game_dir(save_dir, title));
    let _ = write_atomic(&playtime_path(save_dir, title), total.to_string());
}

/// Rows for the "Salvar" modal (plan revision — replaces the old slot
/// cycler): one per save-state slot, always clickable (saving overwrites
/// whatever was there), labelled "(vazio)" for slots with nothing in them
/// yet purely as information.
fn save_slot_rows(save_dir: &Path, title: &str) -> Vec<(String, bool)> {
    (0..SLOTS)
        .map(|s| {
            let filled = state_file(save_dir, title, s).exists();
            let label = if filled {
                format!("Slot {s}")
            } else {
                tr!(format!("Slot {s} (empty)"), format!("Slot {s} (vazio)"))
            };
            (label, true)
        })
        .collect()
}

/// Rows for the "Carregar" modal, mirroring `save_slot_rows` — an empty
/// slot is disabled instead of just noted, since there's nothing to load.
fn load_slot_rows(save_dir: &Path, title: &str) -> Vec<(String, bool)> {
    (0..SLOTS)
        .map(|s| {
            let filled = state_file(save_dir, title, s).exists();
            let label = if filled {
                format!("Slot {s}")
            } else {
                tr!(format!("Slot {s} (empty)"), format!("Slot {s} (vazio)"))
            };
            (label, filled)
        })
        .collect()
}

/// How long a silent action's button shows "feito!" after firing (plan
/// revision, fixing a real report: clicking Nota gave zero on-screen
/// feedback, so a player clicked it seven times thinking nothing happened —
/// it had, every time). Long enough to register as intentional, short
/// enough to not look stuck.
const FLASH_DURATION: Duration = Duration::from_millis(900);

/// O quadro das telas de menu e animações: 60 Hz de verdade. Era 16 ms
/// (62,5 Hz) — 2,5 quadros por segundo desenhados a mais que a tela de
/// 60 Hz mostra (energia à toa no Steam Deck), e as animações que enfileiram
/// `RATE / 60` amostras por quadro empilhavam ~4% de áudio a mais.
pub const UI_FRAME: Duration = Duration::from_nanos(16_666_667);

/// Sem input do jogador por este tempo (e sem transição do tubo), as telas
/// paradas — ociosa, estante, configurações, livro de pausa, console
/// desligado — caem para meio quadro (~30 Hz). A neve da TV já só muda a
/// ~33 Hz (`update_noise_tex`), então visualmente é igual; metade dos
/// quadros a menos é CPU/GPU dormindo mais no Steam Deck. O primeiro input
/// volta aos 60 Hz no quadro seguinte.
const IDLE_AFTER: Duration = Duration::from_secs(2);

/// O quadro de uma tela parada: `frame`, ou o dobro dele com o jogador
/// parado (ver [`IDLE_AFTER`]).
pub(crate) fn idle_paced(frame: Duration, plat: &Platform, cab: &Cabinet) -> Duration {
    if plat.window_hidden() {
        // Ninguém vendo: ~10 Hz basta para ouvir os eventos da volta.
        frame * 6
    } else if plat.idle_for() >= IDLE_AFTER && !cab.animating() {
        frame * 2
    } else {
        frame
    }
}

/// O fim de quadro das telas de menu: ritmo ocioso ([`idle_paced`]) e só
/// `sleep` — o spin final do [`pace_frame`] é precisão que menu não usa
/// (CPU acordada à toa). Também conta ao gabinete se a janela está
/// escondida (o chiado e o som do tubo calam).
pub fn pace_ui(next: &mut Instant, frame: Duration, plat: &Platform, cab: &mut Cabinet) {
    cab.set_window_hidden(plat.window_hidden());
    *next += idle_paced(frame, plat, cab);
    let now = Instant::now();
    if *next <= now {
        *next = now; // atrasou: ressincroniza, sem espiral
        return;
    }
    std::thread::sleep(*next - now);
}

/// Advance `next` by one frame and wait for it: sleep the bulk of the wait,
/// then spin the last ~0.5ms (a plain `thread::sleep` can overshoot by more
/// than a frame's worth of jitter). The spin window is a CPU-vs-precision
/// trade: 2ms of spinning burned ~12% of a core at 60 fps (hot on a Steam
/// Deck), 0.5ms keeps the overshoot small for a fraction of the cost.
/// Falls back to resyncing when the loop fell behind, so it never spirals.
/// Shared by runner/shelf/idle/settings.
pub(crate) fn pace_frame(next: &mut Instant, frame_time: Duration) {
    *next += frame_time;
    let now = Instant::now();
    if *next <= now {
        if sixteen_platform::trace_enabled() {
            let late = now - *next;
            if late > Duration::from_millis(2) {
                log::warn!(
                    "trace: frame atrasado {:.1} ms (loop não fechou o budget)",
                    late.as_secs_f64() * 1e3
                );
            }
        }
        *next = now; // fell behind; resync so we don't spiral
        return;
    }
    let wait = *next - now;
    // Linux (Steam Deck): o `nanosleep` com hrtimers erra por ~50–100 µs
    // (timer slack padrão), então 0,5 ms de spin era CPU acordada à toa a
    // cada frame — o núcleo nunca descia de C-state nesse trecho. 150 µs
    // cobre o erro típico; nos outros sistemas o sleep é mais grosso.
    #[cfg(target_os = "linux")]
    const SPIN_WINDOW: Duration = Duration::from_micros(150);
    #[cfg(not(target_os = "linux"))]
    const SPIN_WINDOW: Duration = Duration::from_micros(500);
    if wait > SPIN_WINDOW {
        std::thread::sleep(wait - SPIN_WINDOW);
    }
    while Instant::now() < *next {
        std::thread::yield_now();
    }
}

/// Whether `b`'s flash is still showing — `flash` maps a button to when it
/// last fired, only ever holding entries for buttons that flash at all.
fn flashed(flash: &HashMap<PanelButton, Instant>, b: PanelButton) -> bool {
    flash.get(&b).is_some_and(|t| t.elapsed() < FLASH_DURATION)
}

/// How long Reset's rocker stays "up" after a click before springing back
/// down on its own (plan revision — a momentary switch, not a toggle like
/// Power's). Shorter than `FLASH_DURATION`: a real spring-back reads as
/// snappy, not as a held state to notice.
const RESET_SPRING: Duration = Duration::from_millis(220);

/// How long the "CH 3" channel banner stays over the picture after power-on
/// (plan revision: "quando ligar o console, mostrar por 3 segundos e remover
/// da tela"). While the console is off, the static path draws it constantly
/// instead — see `Cabinet::present_static`.
const CH3_FLASH: Duration = Duration::from_secs(3);

/// Whether Reset's rocker should be drawn up right now — same `flash` map
/// the "feito!" labels use, just a different (shorter) window.
fn reset_pressed(flash: &HashMap<PanelButton, Instant>) -> bool {
    flash
        .get(&PanelButton::Reset)
        .is_some_and(|t| t.elapsed() < RESET_SPRING)
}

/// The side panel's command legend (plan §3.2, item 3; plan revision:
/// mouse/gamepad only, so labels don't carry a key name any more) — some
/// live-refreshed every frame via `Cabinet::set_commands` since their text
/// depends on state (`flash`), not just the console being on. Power/Eject/
/// Reset aren't in here: they're drawn as their own rocker-switch widgets
/// straight off `panel.powered`/`panel.reset_pressed` (see `draw_panel`/
/// `draw_rocker`). Salvar/Carregar/Printscreen no longer carry a slot
/// number in their label (plan revision — confusing next to the old
/// cyclers): clicking any of the three now opens a modal to pick one, so
/// the label just names the action. Cheats have no button here: they're
/// picked in the Cheat Genius at power-on.
fn command_rows(
    flash: &HashMap<PanelButton, Instant>,
    has_achievements: bool,
    all_slots_pinned: bool,
) -> Vec<(PanelButton, String)> {
    let label = |b: PanelButton, base: &str| -> String {
        if flashed(flash, b) {
            tr!(format!("{base} (done!)"), format!("{base} (feito!)"))
        } else {
            base.to_string()
        }
    };
    let mut rows = vec![(PanelButton::Notebook, tr!("Notes", "Anotações").to_string())];
    if has_achievements {
        rows.push((
            PanelButton::Achievements,
            tr!("Achievements", "Conquistas").to_string(),
        ));
    }
    rows.push((
        PanelButton::PrintScreen,
        if all_slots_pinned {
            // Every slot resists overwrite — a click here would have
            // nowhere to land, so say so instead of the normal label
            // (plan revision: "avisar quando 15 fixados").
            tr!(
                "Screenshot: no free slot (15 pinned)",
                "Print: sem slot livre (15 fixados)"
            )
            .to_string()
        } else {
            label(PanelButton::PrintScreen, tr!("Screenshot", "Print"))
        },
    ));
    rows.push((
        PanelButton::SaveState,
        label(PanelButton::SaveState, tr!("Save", "Salvar")),
    ));
    rows.push((
        PanelButton::LoadState,
        label(PanelButton::LoadState, tr!("Load", "Carregar")),
    ));
    rows
}

/// O painel de desempenho do modo dev: fps medido em janelas de 1 s;
/// temperatura/RAM lidas no máximo a cada 2 s, e só com o painel ligado.
struct FpsMeter {
    window_start: Instant,
    window_frames: u32,
    sys_next: Instant,
}

impl FpsMeter {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
            window_frames: 0,
            sys_next: Instant::now(),
        }
    }

    /// Conta um frame apresentado; fecha a janela de 1 s e alimenta o
    /// painel quando ela vence.
    fn frame(&mut self, cab: &mut Cabinet) {
        self.window_frames += 1;
        let window = self.window_start.elapsed();
        if window < Duration::from_secs(1) {
            return;
        }
        let fps = self.window_frames as f64 / window.as_secs_f64();
        self.window_start = Instant::now();
        self.window_frames = 0;
        if cab.perf_overlay() && Instant::now() >= self.sys_next {
            self.sys_next = Instant::now() + Duration::from_secs(2);
            cab.set_perf_readout(
                fps.round() as u32,
                sixteen_platform::sysinfo::cpu_temp_celsius(),
                sixteen_platform::sysinfo::process_ram_mb(),
            );
        }
    }
}

/// Write battery SRAM to `path` if it changed since the last flush.
fn flush_sram(path: &Path, last: &mut Option<Vec<u8>>, cur: Option<Vec<u8>>) {
    if let Some(cur) = cur {
        if last.as_ref() != Some(&cur) {
            // Só marca como gravado quando gravou: com disco cheio ou SD
            // somente leitura o próximo flush (e o da saída) tenta de novo.
            match write_atomic(path, &cur) {
                Ok(_) => {
                    log::info!("SRAM flushed -> {}", path.display());
                    *last = Some(cur);
                }
                Err(e) => log::warn!("SRAM flush failed: {e}"),
            }
        }
    }
}

/// Desligar (plan §3.3): a short burst of RF snow through the tube with a
/// decaying buzz, settling to a dim near-still hiss — never a full-screen
/// flash, and the noise cuts rather than lingers.
fn power_off_burst(plat: &Platform, cab: &mut Cabinet) -> f32 {
    const RATE: u32 = 22_050;
    const SPAN: Duration = Duration::from_millis(650);
    // The real power-switch foley replaces the synthesized buzz (plan
    // revision); the visual burst plays either way.
    let has_sfx = crate::sfx::play(cab, crate::sfx::Sfx::PowerOff);
    let audio = (!has_sfx).then(|| plat.open_audio(RATE).ok()).flatten();
    let frame = UI_FRAME;
    let mut rng = crate::contato::Rng::new(0x1234_5678);
    let mut buf = Vec::new();
    let start = Instant::now();

    while start.elapsed() < SPAN {
        let t = (start.elapsed().as_secs_f32() / SPAN.as_secs_f32()).min(1.0);
        // Strong for the first half, then settle toward the idle-off hiss.
        let level = if t < 0.5 {
            1.0 - 0.5 * t
        } else {
            (0.9 - t).max(OFF_STATIC_LEVEL)
        };
        cab.present_static(level);

        if let Some(a) = &audio {
            let n = (RATE / 60) as usize;
            let amp = ((1.0 - t) * 8000.0) as i32;
            noise_frame(&mut rng, n, amp, &mut buf);
            a.queue(&buf);
        }
        std::thread::sleep(frame);
    }
    if let Some(a) = &audio {
        a.clear(); // buzz cut, not fade-out tail
    }
    OFF_STATIC_LEVEL
}

/// Ligar de novo: the mirror of `power_off_burst` — snow clears from a dim
/// hiss back up to a brief bright burst, then cuts to the game resuming
/// exactly where it was paused.
fn power_on_burst(plat: &Platform, cab: &mut Cabinet) {
    const RATE: u32 = 22_050;
    const SPAN: Duration = Duration::from_millis(450);
    let has_sfx = crate::sfx::play(cab, crate::sfx::Sfx::PowerOn);
    let audio = (!has_sfx).then(|| plat.open_audio(RATE).ok()).flatten();
    let frame = UI_FRAME;
    let mut rng = crate::contato::Rng::new(0x8765_4321);
    let mut buf = Vec::new();
    let start = Instant::now();

    while start.elapsed() < SPAN {
        let t = (start.elapsed().as_secs_f32() / SPAN.as_secs_f32()).min(1.0);
        let level = OFF_STATIC_LEVEL + (1.0 - OFF_STATIC_LEVEL) * t;
        cab.present_static(level);

        if let Some(a) = &audio {
            let n = (RATE / 60) as usize;
            let amp = (t * 8000.0) as i32;
            noise_frame(&mut rng, n, amp, &mut buf);
            a.queue(&buf);
        }
        std::thread::sleep(frame);
    }
    if let Some(a) = &audio {
        a.clear();
    }
}

/// O relógio da sessão ("no tempo da sessão considerar o tempo que o jogo
/// está rodando, com o power ligado"): conta só com o console ligado, não o
/// tempo desde que o cartucho entrou. `banked` guarda os trechos ligados já
/// encerrados; `since` é quando o trecho atual começou (`None` desligado).
/// O total da sessão também entra no tempo de jogo de todos os tempos.
struct SessionClock {
    banked: Duration,
    since: Option<Instant>,
}

impl SessionClock {
    fn new(powered: bool) -> Self {
        Self {
            banked: Duration::ZERO,
            since: powered.then(Instant::now),
        }
    }

    /// Ligou: começa um trecho.
    fn start(&mut self) {
        self.since = Some(Instant::now());
    }

    /// Desligou: fecha o trecho em curso e devolve o total da sessão.
    fn stop(&mut self) -> Duration {
        if let Some(t) = self.since.take() {
            self.banked += t.elapsed();
        }
        self.banked
    }

    /// O total da sessão agora, contando o trecho em curso.
    fn now(&self) -> Duration {
        self.banked + self.since.map(|t| t.elapsed()).unwrap_or_default()
    }
}

/// Um quadro RGB565 do core passado pelo filtro NTSC (RF), pronto para o
/// tubo — o caminho do jogo e o das falhas de mau contato.
pub(crate) fn ntsc_frame<'a>(
    ntsc: &'a mut NtscFilter,
    pixels: &[u8],
    width: u32,
    height: u32,
    pitch: usize,
) -> FrameRef<'a> {
    let (out, ow, oh) = ntsc.process(pixels, width, height, pitch);
    // SAFETY: `out` é um `&[u16]` vivo enquanto `ntsc` estiver emprestado
    // ('a); ver os mesmos bytes como `u8` (o dobro do comprimento) é sempre
    // válido — u8 não tem exigência de alinhamento nem valores inválidos.
    let pixels = unsafe { std::slice::from_raw_parts(out.as_ptr() as *const u8, out.len() * 2) };
    FrameRef {
        width: ow,
        height: oh,
        pitch: ow as usize * 2,
        format: PlatFormat::Rgb565,
        pixels,
    }
}

/// Sai de uma falha de mau contato: reset no core e a SRAM do encaixe de
/// volta — o que a abertura travada tenha escrito nunca vira save.
fn restore_after_fault(core: &mut Core, sram: Option<&[u8]>, last: &mut Option<Vec<u8>>) {
    core.reset();
    if let Some(bytes) = sram {
        core.load_sram(bytes);
    }
    *last = core.sram();
}

/// Um frame de chiado branco (estéreo, `n` amostras por canal) com
/// amplitude `amp` em `buf` — o som sintetizado dos bursts de power e do
/// cartucho entrando/saindo quando não há foley.
fn noise_frame(rng: &mut crate::contato::Rng, n: usize, amp: i32, buf: &mut Vec<i16>) {
    buf.clear();
    for _ in 0..n {
        let s = (((rng.next_u32() >> 8) & 0xFFFF) as i32 - 0x8000) * amp / 0x8000;
        let v = s.clamp(-32000, 32000) as i16;
        buf.push(v);
        buf.push(v);
    }
}

/// Ejetar with the console still on: the lock resists — a short mechanical
/// thump, nothing else (plan §3.3, "a alavanca resiste, com um clunk seco").
fn eject_clunk(plat: &Platform) {
    tone_click(plat, 90.0);
}

/// A short damped tone burst at `freq` Hz — the shared shape behind every
/// mechanical "click" in this file (`eject_clunk`'s resist-thump, and the
/// insert/eject animations' seat/unseat clicks below): a plain sine ramping
/// from full volume down to silence over ~90ms, via a continuous phase
/// accumulator so it doesn't pop at the start. A no-op if no audio device is
/// available.
fn tone_click(plat: &Platform, freq: f32) {
    const RATE: u32 = 22_050;
    let Some(audio) = plat.open_audio(RATE).ok() else {
        return;
    };
    let n = (RATE as f32 * 0.09) as usize;
    let mut buf = Vec::with_capacity(n * 2);
    let mut phase = 0f32;
    for i in 0..n {
        let env = 1.0 - i as f32 / n as f32;
        phase += freq / RATE as f32;
        let s = (phase * std::f32::consts::TAU).sin() * env * env;
        let v = (s * 12000.0) as i16;
        buf.push(v);
        buf.push(v);
    }
    audio.queue(&buf);
    std::thread::sleep(Duration::from_millis(100));
}

/// The cartridge sliding into the console's slot (plan revision: "a animação
/// deveria estar onde está o cartucho durante a gameplay, não uma
/// transição") — played in the panel's own cartridge block (`Cabinet::
/// set_cartridge_motion` driving `draw_panel_slot`), right where the
/// cartridge lives for the rest of the session: the game's art drops into
/// the slot's dark mouth with a smoothstep ease until it seats, over the
/// signal-off CRT — a quiet sliding hiss swelling and fading underneath and
/// a firm seat-in click right at the end. A no-op with no cartridge art to
/// animate — the slot would sit empty. Skipped for a headless `--shot`
/// capture by the caller (`spec.shot.is_none()`, same reasoning
/// `power_on_burst`/`power_off_burst` don't need — there's no button to
/// click there, so this has no live trigger to skip in the first place;
/// the guard is really about the "plain `--shot`" mode that still runs the
/// live loop for a few frames).
fn cartridge_insert_animation(plat: &Platform, cab: &mut Cabinet) {
    const RATE: u32 = 22_050;
    const SPAN: Duration = Duration::from_millis(520);
    // Real foley (plan revision: pixabay sounds for insert/eject/power/reset)
    // — when it plays, the synthesized sliding hiss and seat click stay out
    // of the way; the foley already carries the whole motion.
    let has_sfx = crate::sfx::play(cab, crate::sfx::Sfx::Insert);
    let audio = (!has_sfx).then(|| plat.open_audio(RATE).ok()).flatten();
    let frame = UI_FRAME;
    let mut rng = crate::contato::Rng::new(0x2468_ace0);
    let mut buf = Vec::new();
    let start = Instant::now();

    while start.elapsed() < SPAN {
        let t = (start.elapsed().as_secs_f32() / SPAN.as_secs_f32()).min(1.0);
        cab.set_cartridge_motion(Some((t, false)));
        cab.present_static(OFF_STATIC_LEVEL);

        if let Some(a) = &audio {
            let n = (RATE / 60) as usize;
            let amp = (2200.0 * (std::f32::consts::PI * t).sin()) as i32;
            noise_frame(&mut rng, n, amp, &mut buf);
            a.queue(&buf);
        }
        std::thread::sleep(frame);
    }
    cab.set_cartridge_motion(None);
    cab.present_static(OFF_STATIC_LEVEL);
    if !has_sfx {
        tone_click(plat, 180.0);
    }
}

/// The mirror of `cartridge_insert_animation`, played right as an
/// already-off cartridge actually leaves (`UiEvent::Eject`'s second branch,
/// plan revision: "criar animacao de... ejetar cartucho") — an unseat click
/// first, then the same panel slot in reverse: the cartridge pops up out of
/// the mouth and rises clear of the block, easing out as it goes. Also a
/// no-op with no cartridge art. `cab.clear_panel()` on the way to the idle
/// screen right after drops the panel entirely, so there's no stuck
/// mid-motion state left over to reset here.
fn cartridge_eject_animation(plat: &Platform, cab: &mut Cabinet) {
    // Real foley for the whole leave-the-slot motion when it's available;
    // the synthesized unseat click + hiss are the fallback.
    let has_sfx = crate::sfx::play(cab, crate::sfx::Sfx::Eject);
    if !has_sfx {
        tone_click(plat, 130.0);
    }
    const RATE: u32 = 22_050;
    const SPAN: Duration = Duration::from_millis(430);
    let audio = (!has_sfx).then(|| plat.open_audio(RATE).ok()).flatten();
    let frame = UI_FRAME;
    let mut rng = crate::contato::Rng::new(0x0ff1_ce00);
    let mut buf = Vec::new();
    let start = Instant::now();

    while start.elapsed() < SPAN {
        let t = (start.elapsed().as_secs_f32() / SPAN.as_secs_f32()).min(1.0);
        cab.set_cartridge_motion(Some((t, true)));
        cab.present_static(OFF_STATIC_LEVEL);

        if let Some(a) = &audio {
            let n = (RATE / 60) as usize;
            let amp = (2000.0 * (std::f32::consts::PI * t).sin()) as i32;
            noise_frame(&mut rng, n, amp, &mut buf);
            a.queue(&buf);
        }
        std::thread::sleep(frame);
    }
}

/// One `0`/`1` per line, in the curated list's order. Missing/short/garbled
/// files just mean "start with everything off" — nothing to migrate.
fn load_cheat_state(path: &Path, len: usize) -> Vec<bool> {
    let mut state = vec![false; len];
    if let Ok(text) = fs::read_to_string(path) {
        for (slot, line) in state.iter_mut().zip(text.lines()) {
            *slot = line.trim() == "1";
        }
    }
    state
}

fn save_cheat_state(path: &Path, state: &[bool]) {
    let text: String = state
        .iter()
        .map(|&on| if on { "1\n" } else { "0\n" })
        .collect();
    if let Err(e) = write_atomic(path, text) {
        log::warn!("cheat state flush failed: {e}");
    }
}

/// Reaplica ao core o estado de todos os cheats — os do banco e os que o
/// jogador digitou no Cheat Genius. Reset + set de tudo, a mesma sequência
/// "cinto e suspensório" do carregamento.
fn apply_cheats(
    core: &mut Core,
    defs: &[sixteen_domain::CheatDef],
    state: &[bool],
    customs: &[crate::genius::Custom],
) {
    core.cheat_reset();
    let mut i = 0u32;
    for (def, &on) in defs.iter().zip(state) {
        core.cheat_set(i, on, def.code);
        i += 1;
    }
    for c in customs {
        core.cheat_set(i, c.on, &c.code);
        i += 1;
    }
}

/// `cheat_rows` + os códigos próprios (a descrição de um é o próprio código).
fn all_cheat_rows(
    defs: &[sixteen_domain::CheatDef],
    state: &[bool],
    customs: &[crate::genius::Custom],
) -> Vec<(String, bool)> {
    let mut rows = cheat_rows(defs, state);
    rows.extend(customs.iter().map(|c| (c.code.clone(), c.on)));
    rows
}

/// Cheat Genius: troca a ROM do core pela do adaptador e entrega a lista.
/// `Some(n)` com o nº de entradas enviadas; em qualquer falha o jogo volta
/// ao core e é `None` (o jogador segue sem a tela, nunca fica sem jogo).
#[allow(clippy::too_many_arguments)]
fn launch_genius(
    core: &mut Core,
    rom: &Path,
    rom_bytes: &[u8],
    sram_path: &Path,
    last_sram: &mut Option<Vec<u8>>,
    held_sram: &mut Option<Vec<u8>>,
    title: &str,
    defs: &[sixteen_domain::CheatDef],
    state: &[bool],
    customs: &[crate::genius::Custom],
) -> Option<usize> {
    // A SRAM do jogo vai para o disco antes de a ROM sair do core — e fica
    // guardada em memória: se o flush falhou (disco cheio, SD só leitura), a
    // volta do jogo usa ESTA cópia, não a versão velha do disco.
    let game_sram = core.sram();
    flush_sram(sram_path, last_sram, game_sram.clone());
    *held_sram = game_sram;
    core.unload_game();
    let entries = crate::genius::build_entries(defs, state, customs);
    let ok = core
        .load_game(Path::new("cheat_genius.sfc"), crate::genius::ROM)
        .is_ok()
        && crate::genius::start(core, title, &entries);
    if ok {
        log::info!("cheat genius: {} entradas", entries.len());
        Some(entries.len())
    } else {
        log::warn!("cheat genius: a ROM não carregou, seguindo sem ela");
        restore_game(core, rom, rom_bytes, sram_path, last_sram, held_sram);
        None
    }
}

/// Devolve o jogo ao core (depois do Cheat Genius) com a SRAM que ele tinha
/// ao sair (`held_sram`, de [`launch_genius`]) — ou, sem ela, a do disco.
/// `last_sram` (o último flush BEM-sucedido) não muda aqui: se o flush da
/// ida falhou, o próximo vê a diferença e tenta gravar de novo.
fn restore_game(
    core: &mut Core,
    rom: &Path,
    rom_bytes: &[u8],
    sram_path: &Path,
    last_sram: &mut Option<Vec<u8>>,
    held_sram: &mut Option<Vec<u8>>,
) {
    core.unload_game();
    if let Err(e) = core.load_game(rom, rom_bytes) {
        log::error!("recarregar o jogo falhou: {e}");
        return;
    }
    if let Some(bytes) = held_sram.take() {
        core.load_sram(&bytes);
        return;
    }
    match read_sram(sram_path) {
        Ok(Some(bytes)) => {
            core.load_sram(&bytes);
        }
        Ok(None) => {}
        // A releitura falhou: a SRAM que acabou de ser gravada ainda está
        // na memória — volta ela, e não uma SRAM zerada que o próximo
        // flush gravaria por cima do save.
        Err(e) => {
            log::warn!("SRAM: releitura falhou ({e}); usando a cópia em memória");
            if let Some(bytes) = last_sram.as_deref() {
                core.load_sram(bytes);
            }
        }
    }
    *last_sram = core.sram();
}

/// `(description, on)` pairs for the panel — cheap enough to rebuild on every
/// toggle/navigate, there are only ever a handful.
fn cheat_rows(defs: &[sixteen_domain::CheatDef], state: &[bool]) -> Vec<(String, bool)> {
    defs.iter()
        .zip(state)
        .map(|(d, &on)| (d.desc.to_string(), on))
        .collect()
}

/// Decode scraped art (panel logo, note thumbnail, …) small enough for its
/// slot, keeping alpha for transparent logos.
fn decode_art(path: &Path, max: u32) -> Result<(u32, u32, Vec<u8>)> {
    let img = crate::imgutil::open_limited(path, crate::imgutil::ART_MAX_SIDE)?
        .thumbnail(max, max)
        .to_rgba8();
    let (w, h) = img.dimensions();
    Ok((w, h, img.into_raw()))
}

/// Decode a core frame's raw pixels into plain RGB8 — the un-warped, un-NTSC'd
/// picture, not what's on screen, so a captured password stays legible on the
/// page (plan §3.4: "é como se fazia no papel").
fn frame_to_rgb8(frame: &EmuFrame) -> image::RgbImage {
    let (w, h) = (frame.width as usize, frame.height as usize);
    let mut img = image::RgbImage::new(frame.width, frame.height);
    for y in 0..h {
        let row = &frame.pixels[y * frame.pitch..];
        for x in 0..w {
            let rgb = match frame.format {
                EmuFormat::Rgb565 => {
                    let px = u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]);
                    let (r5, g6, b5) = ((px >> 11) & 0x1F, (px >> 5) & 0x3F, px & 0x1F);
                    [
                        ((r5 << 3) | (r5 >> 2)) as u8,
                        ((g6 << 2) | (g6 >> 4)) as u8,
                        ((b5 << 3) | (b5 >> 2)) as u8,
                    ]
                }
                EmuFormat::Rgb1555 => {
                    let px = u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]);
                    let (r5, g5, b5) = ((px >> 10) & 0x1F, (px >> 5) & 0x1F, px & 0x1F);
                    [
                        ((r5 << 3) | (r5 >> 2)) as u8,
                        ((g5 << 3) | (g5 >> 2)) as u8,
                        ((b5 << 3) | (b5 >> 2)) as u8,
                    ]
                }
                // XRGB8888, little-endian bytes: B, G, R, X.
                EmuFormat::Xrgb8888 => {
                    let o = x * 4;
                    [row[o + 2], row[o + 1], row[o]]
                }
            };
            img.put_pixel(x as u32, y as u32, image::Rgb(rgb));
        }
    }
    img
}

/// Fixed note slots per game (plan revision, replacing an open-ended
/// timestamped list): a screenshot into the notebook always lands in one of
/// these, chosen by the player, same shape as the save-state slots.
const NOTE_SLOTS: u8 = 15;
/// Max characters a slot's caption may hold — short, it's a label, not a
/// page (contrast `NOTE_CHAR_LIMIT` for the free-text notebook page).
const SLOT_NAME_LIMIT: usize = 40;

/// Which of three things the app's one text editor is currently editing
/// (plan revision) — they share all the input-polling plumbing, just write
/// to a different place, have a different character limit, and (for
/// `PrintName`) render in the modal instead of the notebook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NoteEdit {
    None,
    /// Editing the notebook's currently-shown text-note slot (plan
    /// revision — used to always append a fresh, blank page to an
    /// unbounded, unreadable log; now edits whichever of the 15 slots the
    /// left page is showing, pre-filled with its saved content). Saving an
    /// empty draft deletes the slot instead of writing one.
    Text,
    /// Renaming the notebook's currently-shown print slot's caption (from
    /// "Nomear print" on the notebook's right page).
    SlotName,
    /// Naming the print just captured into this slot (plan revision — the
    /// Printscreen modal's second step): unlike `SlotName`, committing this
    /// one also writes `runner::print_capture`'s image to disk for the
    /// first time — the capture and the name land together.
    PrintName(u8),
}

impl NoteEdit {
    fn limit(self) -> usize {
        match self {
            NoteEdit::None => 0,
            NoteEdit::Text => NOTE_CHAR_LIMIT,
            NoteEdit::SlotName | NoteEdit::PrintName(_) => SLOT_NAME_LIMIT,
        }
    }

    fn heading(self) -> &'static str {
        match self {
            NoteEdit::None => "",
            NoteEdit::Text => tr!(
                "editing note (empty deletes, click outside cancels)",
                "editando anotação (vazio apaga, clique fora cancela)"
            ),
            NoteEdit::SlotName => tr!(
                "renaming the screenshot (click outside cancels)",
                "renomeando o print (clique fora cancela)"
            ),
            NoteEdit::PrintName(_) => tr!("screenshot name (optional)", "nome do print (opcional)"),
        }
    }
}

/// A save/load-state or print slot picker, or the achievements list (plan
/// revision) — mutually exclusive with `paused` (the notebook has no
/// slot-picking or cheats of its own any more) and gates gameplay stepping
/// the same way `paused` does, so the frozen frame behind the dialog doesn't
/// keep moving while a choice is pending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modal {
    None,
    SaveSlot,
    LoadSlot,
    /// Picking which of the 15 note slots to save the just-grabbed print
    /// into — see `runner::print_capture`.
    PrintSlot,
    /// The achievements list (plan: `docs/plano-retroachievements.md`,
    /// fase 4) — read-only rows with the earned state; a pick just closes.
    Achievements,
    /// O "Menu do jogo" aberto pelo acorde Select+Start do gamepad — as
    /// linhas estão em `menu_acts` (ver `game_menu_rows`).
    GameMenu,
}

/// Ação de uma linha do Menu do jogo; cada uma dispara o `UiEvent` que o
/// painel (mouse) já dispara.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuAct {
    Continue,
    Save,
    Load,
    Print,
    Notes,
    Achievements,
    Reset,
    Power,
    Eject,
    QuitApp,
}

/// As linhas do Menu do jogo para o estado atual: ligado, desligado (Ligar
/// no lugar de Desligar) e com o adaptador do Cheat Genius no ar (só
/// Desligar/Ejetar/Sair valem lá — o mesmo filtro dos cliques).
fn game_menu_rows(powered: bool, genius: bool, has_ra: bool) -> Vec<(MenuAct, String)> {
    let mut v = vec![(MenuAct::Continue, tr!("Resume", "Continuar").to_string())];
    if powered && !genius {
        v.push((
            MenuAct::Save,
            tr!("Save state", "Salvar estado").to_string(),
        ));
        v.push((
            MenuAct::Load,
            tr!("Load state", "Carregar estado").to_string(),
        ));
        v.push((MenuAct::Print, tr!("Screenshot", "Print").to_string()));
        v.push((MenuAct::Notes, tr!("Notes", "Anotações").to_string()));
        if has_ra {
            v.push((
                MenuAct::Achievements,
                tr!("Achievements", "Conquistas").to_string(),
            ));
        }
        v.push((MenuAct::Reset, "Reset".to_string()));
    }
    v.push((
        MenuAct::Power,
        if powered {
            tr!("Power off", "Desligar")
        } else {
            tr!("Power on", "Ligar")
        }
        .to_string(),
    ));
    v.push((
        MenuAct::Eject,
        tr!("Eject (back to shelf)", "Ejetar (voltar à estante)").to_string(),
    ));
    v.push((MenuAct::QuitApp, tr!("Quit app", "Sair do app").to_string()));
    v
}

/// Os `UiEvent`s que uma linha do Menu do jogo dispara. Ejetar com o
/// console ligado desliga antes (o trinco não cede ligado).
fn menu_act_events(act: MenuAct, powered: bool) -> Vec<UiEvent> {
    match act {
        MenuAct::Continue => vec![],
        MenuAct::Save => vec![UiEvent::OpenSaveModal],
        MenuAct::Load => vec![UiEvent::OpenLoadModal],
        MenuAct::Print => vec![UiEvent::OpenPrintModal],
        MenuAct::Notes => vec![UiEvent::TogglePause],
        MenuAct::Achievements => vec![UiEvent::OpenAchievementsModal],
        MenuAct::Reset => vec![UiEvent::Reset],
        MenuAct::Power => vec![UiEvent::Quit],
        MenuAct::Eject if powered => vec![UiEvent::Quit, UiEvent::Eject],
        MenuAct::Eject => vec![UiEvent::Eject],
        MenuAct::QuitApp => vec![UiEvent::CloseRequested],
    }
}

/// Botões do pad durante o livro de pausa → o `UiEvent` do botão equivalente
/// do mouse. Escrever/nomear abrem o editor de texto (ver o ponto de
/// encaixe do teclado na tela no ramo `note_edit` do loop).
fn book_pad_events(edges: u16) -> Vec<UiEvent> {
    use sixteen_platform::padnav::bit;
    let map = [
        (bit::LEFT, UiEvent::NotePrev),
        (bit::RIGHT, UiEvent::NoteNext),
        (bit::LB, UiEvent::TextPrev),
        (bit::RB, UiEvent::TextNext),
        (bit::X, UiEvent::NotePinToggle),
        (bit::Y, UiEvent::NoteNameStart),
        (bit::START, UiEvent::NoteWriteStart),
        (bit::SELECT, UiEvent::TextPinToggle),
        (bit::B, UiEvent::TogglePause),
    ];
    map.into_iter()
        .filter(|(b, _)| edges & b != 0)
        .map(|(_, e)| e)
        .collect()
}

/// One note slot's protection/caption (plan revision) — everything defaults
/// to "untouched" (not pinned, no caption) for a slot nobody's set either on.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct SlotMeta {
    #[serde(default)]
    pinned: bool,
    #[serde(default)]
    label: String,
}

/// `notes_dir/<title>/slots.json`'s shape: which of the 15 photo slots and
/// which of the 15 text-note slots (plan revision — the two are numbered
/// independently, slot 3's photo and slot 3's text share nothing but a
/// number) are pinned or captioned. Absent entries mean the default
/// (`SlotMeta::default()`) — most slots never need a real entry.
#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
struct NotesMeta {
    #[serde(default)]
    slots: std::collections::BTreeMap<u8, SlotMeta>,
    #[serde(default)]
    text_slots: std::collections::BTreeMap<u8, SlotMeta>,
}

impl NotesMeta {
    fn slot(&self, slot: u8) -> SlotMeta {
        self.slots.get(&slot).cloned().unwrap_or_default()
    }

    fn text_slot(&self, slot: u8) -> SlotMeta {
        self.text_slots.get(&slot).cloned().unwrap_or_default()
    }
}

/// Every one of the 15 slots resists overwrite — Printscreen has nowhere
/// left to put a new capture (plan revision: "avisar quando 15 fixados").
fn all_slots_pinned(meta: &NotesMeta) -> bool {
    (1..=NOTE_SLOTS).all(|s| meta.slot(s).pinned)
}

/// Rows for the Printscreen modal's slot-picking step (plan revision): one
/// per note slot, in order, labelled with its number plus whether it's
/// filled/pinned; a pinned slot is disabled, same protection `NoteCapture`
/// used to enforce by auto-redirecting instead.
fn print_slot_rows(notes_dir: &Path, title: &str, meta: &NotesMeta) -> Vec<(String, bool)> {
    (1..=NOTE_SLOTS)
        .map(|s| {
            let m = meta.slot(s);
            let filled = note_slot_path(notes_dir, title, s).exists();
            let label = if m.pinned {
                tr!(format!("Slot {s} (pinned)"), format!("Slot {s} (fixado)"))
            } else if filled {
                format!("Slot {s}")
            } else {
                tr!(format!("Slot {s} (empty)"), format!("Slot {s} (vazio)"))
            };
            (label, !m.pinned)
        })
        .collect()
}

fn notes_meta_path(notes_dir: &Path, title: &str) -> PathBuf {
    note_dir(notes_dir, title).join("slots.json")
}

/// Missing or unreadable is just "nothing pinned or captioned yet", not an
/// error — same tolerant read as the rest of this portable app's sidecars.
fn load_notes_meta(notes_dir: &Path, title: &str) -> NotesMeta {
    fs::read_to_string(notes_meta_path(notes_dir, title))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_notes_meta(notes_dir: &Path, title: &str, meta: &NotesMeta) {
    let dir = note_dir(notes_dir, title);
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    if let Ok(s) = serde_json::to_string_pretty(meta) {
        if let Err(e) = write_atomic(&notes_meta_path(notes_dir, title), s) {
            log::warn!("saving note slot metadata failed: {e}");
        }
    }
}

/// `notes_dir/<game title>/` — one folder per game, named for a human
/// browsing it rather than the ROM hash (plan revision: readability over
/// rename-proofing, since the player picked this explicitly). Mirrors
/// `game_dir` (`saves/<title>/`) — same sanitizing, same tradeoff.
fn note_dir(notes_dir: &Path, title: &str) -> PathBuf {
    notes_dir.join(sanitize_dir_name(title))
}

/// `notes_dir/<title>/01.png` .. `15.png` — 1-indexed to match the slot
/// numbers shown on screen.
fn note_slot_path(notes_dir: &Path, title: &str, slot: u8) -> PathBuf {
    note_dir(notes_dir, title).join(format!("{slot:02}.png"))
}

/// `notes_dir/<title>/notas.txt` — the old, pre-revision unbounded
/// free-text append log. Nothing writes here any more (see
/// `note_text_slot_path`, the 15-slot replacement) — this path only still
/// exists so `migrate_legacy_text_notes` can find and split up whatever an
/// earlier version of the app already wrote there.
fn legacy_note_text_path(notes_dir: &Path, title: &str) -> PathBuf {
    note_dir(notes_dir, title).join("notas.txt")
}

/// `notes_dir/<title>/01.txt` .. `15.txt` — 1-indexed to match the slot
/// numbers shown on screen, numbered independently of the photo slots
/// (`note_slot_path`) even though they share the same range: a `.png` and
/// a `.txt` never collide, so slot 3's photo and slot 3's text are just two
/// unrelated files that happen to both say "3".
fn note_text_slot_path(notes_dir: &Path, title: &str, slot: u8) -> PathBuf {
    note_dir(notes_dir, title).join(format!("{slot:02}.txt"))
}

/// Save the current frame into note `slot` (1..=15), overwriting whatever
/// was there before — same "pick a slot, it replaces what's in it" model as
/// save states.
fn save_note_image(notes_dir: &Path, title: &str, slot: u8, frame: &EmuFrame) -> Result<()> {
    let dir = note_dir(notes_dir, title);
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = note_slot_path(notes_dir, title, slot);
    frame_to_rgb8(frame)
        .save(&path)
        .with_context(|| format!("saving note image {}", path.display()))
}

/// Text-note `slot`'s saved content (plan revision — one of the 15,
/// mirroring the photo slots), or `None` for an empty slot — a
/// whitespace-only file counts as empty too, same as an empty draft never
/// got saved as one to begin with.
fn read_text_slot(notes_dir: &Path, title: &str, slot: u8) -> Option<String> {
    fs::read_to_string(note_text_slot_path(notes_dir, title, slot))
        .ok()
        .filter(|s| !s.trim().is_empty())
}

/// Save `text` into note-text `slot` (1..=15), overwriting whatever was
/// there before — same "pick a slot, it replaces what's in it" model
/// `save_note_image` already uses for the photo slots.
fn save_text_slot(notes_dir: &Path, title: &str, slot: u8, text: &str) -> Result<()> {
    let dir = note_dir(notes_dir, title);
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = note_text_slot_path(notes_dir, title, slot);
    write_atomic(&path, text).with_context(|| format!("saving text note {}", path.display()))
}

/// Clear note-text `slot` — a missing file already means "empty", so this
/// is not an error either way.
fn delete_text_slot(notes_dir: &Path, title: &str, slot: u8) {
    let _ = fs::remove_file(note_text_slot_path(notes_dir, title, slot));
}

/// One-time upgrade from the old unbounded `notas.txt` append log into the
/// new 15 numbered slots (plan revision — "I wrote something and can't see
/// it again" was a real complaint: the old log had no viewer at all, just
/// a button that always appended a blank page). A no-op the moment any
/// text slot already exists, so a game already using the new format is
/// never re-split or overwritten. Entries in the old log were separated by
/// a blank line (`append_note_text` used to write `"{text}\n\n"`); the
/// first 15 non-empty ones become slots 1..=15, in the order they were
/// written. `notas.txt` itself is left in place afterward rather than
/// deleted — cheap insurance against a bug here losing anyone's notes
/// outright — but nothing reads it again once this has run once.
fn migrate_legacy_text_notes(notes_dir: &Path, title: &str) {
    if (1..=NOTE_SLOTS).any(|s| note_text_slot_path(notes_dir, title, s).is_file()) {
        return;
    }
    let Ok(contents) = fs::read_to_string(legacy_note_text_path(notes_dir, title)) else {
        return;
    };
    let pages: Vec<&str> = contents
        .split("\n\n")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if pages.is_empty() {
        return;
    }
    let mut migrated = 0;
    for (slot, page) in (1..=NOTE_SLOTS).zip(pages) {
        match save_text_slot(notes_dir, title, slot, page) {
            Ok(()) => migrated += 1,
            Err(e) => log::warn!("migrating legacy note into slot {slot} failed: {e}"),
        }
    }
    log::info!("notas.txt: migrated {migrated} page(s) into numbered text-note slots");
}

/// Recompute the panel's notebook block from what's actually on disk: a
/// pinned print (with its caption) if there's one to show, and a pinned
/// text note's full content if there's one of those too — independent of
/// the photo side ("mostrar apenas uma nota e/ou uma imagem"), so either,
/// both, or neither can end up in the panel. Only pinned slots ever show
/// there (plan revision: a slot nobody deliberately featured showing up in
/// the always-visible panel was the original complaint). Both sides pick
/// the same way: the currently-viewed slot if it's itself pinned, else the
/// lowest-numbered pinned one, else none. Call at game start, after every
/// capture/write, after cycling either slot, and after every pin toggle or
/// rename — cheap, at most 30 file-exists checks.
fn refresh_notes(
    cab: &mut Cabinet,
    notes_dir: &Path,
    title: &str,
    slot: u8,
    text_slot: u8,
    meta: &NotesMeta,
) {
    let thumb_slot = if meta.slot(slot).pinned {
        Some(slot)
    } else {
        (1..=NOTE_SLOTS).find(|&s| meta.slot(s).pinned)
    };
    // Big enough to stay sharp at the panel's full column width.
    let thumb = thumb_slot.and_then(|s| note_thumb(notes_dir, title, s, 512));
    let caption = thumb_slot.map(|s| meta.slot(s).label).unwrap_or_default();

    let text_pin_slot = if meta.text_slot(text_slot).pinned {
        Some(text_slot)
    } else {
        (1..=NOTE_SLOTS).find(|&s| meta.text_slot(s).pinned)
    };
    let text = text_pin_slot.and_then(|s| read_text_slot(notes_dir, title, s));

    cab.set_notes(
        thumb.as_ref().map(|(w, h, d)| (*w, *h, d.as_slice())),
        &caption,
        text.as_deref(),
    );
}

/// Decode note `slot`'s image if it's filled, `max` pixels on the long
/// side — `None` for an empty slot or a decode failure either way.
fn note_thumb(notes_dir: &Path, title: &str, slot: u8, max: u32) -> Option<(u32, u32, Vec<u8>)> {
    let path = note_slot_path(notes_dir, title, slot);
    path.is_file()
        .then(|| decode_art(&path, max))
        .and_then(Result::ok)
}

/// Push note `slot`'s current image, pin state, and caption onto the pause
/// book's right page — the `set_pause_page` call every slot-change (or
/// pin/rename) site needs.
fn show_note_slot(cab: &mut Cabinet, notes_dir: &Path, title: &str, slot: u8, meta: &NotesMeta) {
    let thumb = note_thumb(notes_dir, title, slot, 900);
    let m = meta.slot(slot);
    cab.set_pause_page(
        (slot - 1) as usize,
        m.pinned,
        &m.label,
        thumb.as_ref().map(|(w, h, d)| (*w, *h, d.as_slice())),
    );
}

/// Push text-note `slot`'s saved content and pin state onto the pause
/// book's left page (plan revision, mirrors `show_note_slot`) — the
/// `set_pause_text_page` call every slot-change (or pin/write/delete)
/// site needs.
fn show_text_slot(cab: &mut Cabinet, notes_dir: &Path, title: &str, slot: u8, meta: &NotesMeta) {
    let content = read_text_slot(notes_dir, title, slot);
    let pinned = meta.text_slot(slot).pinned;
    cab.set_pause_text_page((slot - 1) as usize, pinned, content.as_deref());
}

/// Headless `--debug-shot-modal`: monta um dos modais (salvar, carregar,
/// print e o nome do print) em vez do gameplay, para a captura.
fn preview_modal(
    cab: &mut Cabinet,
    kind: &str,
    spec: &GameSpec,
    title: &str,
    notes_meta: &NotesMeta,
) {
    match kind {
        "save" => cab.set_modal(
            tr!("Save state", "Salvar estado"),
            &save_slot_rows(&spec.save_dir, title),
        ),
        "load" => cab.set_modal(
            tr!("Load state", "Carregar estado"),
            &load_slot_rows(&spec.save_dir, title),
        ),
        "print" => cab.set_modal(
            tr!("Where to save the screenshot?", "Onde salvar o print?"),
            &print_slot_rows(&spec.notes_dir, title, notes_meta),
        ),
        // Not a real step on its own (there's no row list once
        // naming starts) — previews the draft view `NoteEdit::
        // PrintName` switches to after a slot's picked.
        "print-name" => {
            cab.set_modal(
                tr!("Where to save the screenshot?", "Onde salvar o print?"),
                &[],
            );
            cab.set_modal_draft(
                Some(""),
                NoteEdit::PrintName(1).limit(),
                NoteEdit::PrintName(1).heading(),
            );
        }
        // O Menu do jogo (acorde Select+Start) com o destaque do pad na 3a
        // linha, e o modal de print (2 colunas) com o destaque na 10a.
        "menu" => {
            let rows: Vec<(String, bool)> = game_menu_rows(true, false, true)
                .into_iter()
                .map(|(_, l)| (l, true))
                .collect();
            cab.set_modal(tr!("Game menu", "Menu do jogo"), &rows);
            cab.move_modal_cursor(0, 2, 0);
        }
        "print-pad" => {
            cab.set_modal(
                tr!("Where to save the screenshot?", "Onde salvar o print?"),
                &print_slot_rows(&spec.notes_dir, title, notes_meta),
            );
            cab.move_modal_cursor(0, 9, 0);
        }
        other => {
            log::warn!(
                "--debug-shot-modal {other}: unknown, expected save/load/print/print-name/menu/print-pad"
            )
        }
    }
}

/// A fila de áudio do jogo, com o volume da TV (VOL-/VOL+ do queixo,
/// 0..16, 8 = padrão). TV desligada com o jogo rodando: o jogo segue SEM
/// som pelo alto-falante dela — a fila morre na hora (o corte, não o fade,
/// é a mesma regra do hiss); ao religar, o áudio volta no tempo do jogo.
/// No canal 4 o som vem como a imagem: abafado, com chiado e o zumbido do
/// vertical ([`RfAudio`](sixteen_platform::RfAudio)); com o alto-falante da
/// TV escolhido (em vez do 3 em 1), sai mono e abafado.
pub(crate) fn queue_game_audio(
    audio: &sixteen_platform::AudioOut,
    cab: &Cabinet,
    samples: &[i16],
    vol_buf: &mut Vec<i16>,
    rf: &mut sixteen_platform::RfAudio,
) {
    if !cab.tv_on() {
        audio.clear();
        return;
    }
    let vol = cab.volume() as i32;
    if vol == 0 {
        return; // mudo, nada entra na fila
    }
    let speaker = cab.tv_sound() == sixteen_platform::TvSound::Speaker;
    if let Some(signal) = cab.ch4_signal() {
        rf.process(samples, audio.source_rate, signal, cab.volume(), vol_buf);
    } else if vol == 16 && !speaker {
        audio.queue(samples);
        return;
    } else {
        vol_buf.clear();
        vol_buf.extend(samples.as_chunks::<2>().0.iter().flat_map(|p| {
            let l = (p[0] as i32 * vol / 16).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            let r = (p[1] as i32 * vol / 16).clamp(i16::MIN as i32, i16::MAX as i32) as i16;
            [l, r]
        }));
    }
    // O alto-falante da TV em vez do 3 em 1: mono, sem grave nem agudo.
    if speaker {
        rf.speaker.process(vol_buf, audio.source_rate);
    }
    audio.queue(vol_buf);
}

/// Badges decodificados fora do laço do jogo: (id da imagem, (w, h, rgba)).
type BadgeTx = std::sync::mpsc::Sender<(u64, (u32, u32, Vec<u8>))>;

/// Uma conquista desbloqueada na TV: o "plim", o badge (do cache, ou
/// baixado na hora se o prefetch ainda não trouxe) e o aviso no queixo.
fn notify_unlock(cab: &mut Cabinet, unlock: &crate::ra::Unlock, badges: &BadgeTx) {
    // O "plim" da conquista (plan revision: "coloque uma
    // notificação sonora ao ganhar uma conquista") — sobre o
    // áudio do jogo, pelo stream de foley.
    crate::sfx::play(cab, crate::sfx::Sfx::Achievement);
    // O badge: do cache em disco ou baixado — numa thread, nunca no laço
    // do jogo (o download segurava o frame por centenas de ms). O aviso
    // sai na hora; a imagem entra no queixo quando chegar (o laço drena
    // `BadgeRx` a cada frame).
    let badge = unlock.badge.clone();
    if !badge.is_empty() {
        let tx = badges.clone();
        std::thread::spawn(move || {
            let path = crate::ra::badge_path(&badge).or_else(|| crate::ra::download_badge(&badge));
            if let Some(img) = path.and_then(|p| decode_art(&p, 512).ok()) {
                let _ = tx.send((osd_badge_id(&badge), img));
            }
        });
    }
    cab.push_osd(
        &[
            tr!("ACHIEVEMENT UNLOCKED", "CONQUISTA DESBLOQUEADA"),
            &unlock.title,
            &tr!(
                format!("+{} points", unlock.points),
                format!("+{} pontos", unlock.points)
            ),
            if unlock.hardcore {
                tr!("hardcore mode", "modo hardcore")
            } else {
                tr!("softcore mode", "modo softcore")
            },
        ],
        Some(osd_badge_id(&unlock.badge)),
        OSD_UNLOCK_TTL,
    );
}

/// Load the core + ROM and run until the player leaves, drawing into `cab` (the
/// one persistent window). `plat` and `cab` both outlive the call so `sixteen`
/// can reuse them for the next screen.
pub fn run_game(
    plat: &mut Platform,
    cab: &mut Cabinet,
    spec: &GameSpec,
    cfg: &Config,
) -> Result<GameExit> {
    // --- load + identify -------------------------------------------------
    let mut core =
        Core::load(&spec.core).with_context(|| format!("loading core {}", spec.core.display()))?;
    log::info!("core: {} {}", core.system_name(), core.system_version());
    core.set_directories(&spec.system_dir, &spec.save_dir);
    core.init();

    // `load_rom` transparently extracts the ROM inside a .zip (plan
    // revision: "add suporte a roms em formato zip") — the core gets raw
    // ROM bytes either way.
    let rom_bytes = sixteen_domain::library::load_rom(&spec.rom)
        .with_context(|| format!("reading ROM {}", spec.rom.display()))?;
    match sixteen_domain::RomId::from_bytes(&rom_bytes) {
        Ok(id) => log::info!(
            "rom: {} bytes (+{} header), crc32={} sha1={} name={:?} {:?}",
            id.rom_len,
            id.header_len,
            id.crc32,
            id.sha1,
            id.internal_name,
            id.mapper
        ),
        Err(e) => log::warn!("rom id failed: {e}"),
    }

    core.load_game(&spec.rom, &rom_bytes)
        .context("core rejected the ROM")?;

    let title = rom_title(&spec.rom);

    // Per-game persistence — one folder per game, named for the title like
    // notes already are (plan revision — used to be a flat file per kind,
    // keyed by ROM hash: unreadable next to a folder a player might actually
    // open, and the hash bought rename-proofing nobody asked for here).
    fs::create_dir_all(game_dir(&spec.save_dir, &title)).ok();
    let mut sram_path = sram_file(&spec.save_dir, &title);
    // Note slot (fixed 1..=15, not save-state's 0..=9) — which of the 15
    // the notebook's right page shows; Prev/Next move it while paused
    // there, and picking a print's destination in the Printscreen modal
    // moves it too, so the notebook opens on whatever was captured last.
    let mut note_slot: u8 = 1;
    // Text-note slot (plan revision) — same idea as `note_slot`, but for
    // the left page's 15 text notes; the two cursors are independent, so
    // paging through prints never moves which text slot is shown, or the
    // other way around.
    let mut text_slot: u8 = 1;

    let aside = sram_aside_path(&sram_path);
    match read_sram(&sram_path) {
        Ok(Some(bytes)) => {
            let n = core.load_sram(&bytes);
            log::info!("SRAM: loaded {n} bytes from {}", sram_path.display());
            // O original voltou a ler, mas uma sessão anterior gravou ao lado:
            // os dois ficam no disco — o jogador decide qual vale.
            if aside.exists() {
                log::warn!("SRAM: existe também {}", aside.display());
                cab.push_osd(
                    &[
                        tr!(
                            "This game has a second save beside the original:",
                            "Este jogo tem um segundo save ao lado do original:"
                        ),
                        "sram-sessao-sem-leitura.srm",
                    ],
                    None,
                    Duration::from_secs(10),
                );
            }
        }
        Ok(None) => {}
        Err(e) => {
            // O save existe mas não leu: o jogo roda, mas grava AO LADO —
            // nunca por cima do `sram.srm` que pode estar bom. Se uma sessão
            // anterior já gravou ao lado, o jogo continua DELE (senão o
            // primeiro flush desta sessão apagaria aquele progresso).
            if let Ok(Some(bytes)) = read_sram(&aside) {
                let n = core.load_sram(&bytes);
                log::info!("SRAM: loaded {n} bytes from {}", aside.display());
            }
            log::error!(
                "SRAM: {} não pôde ser lido ({e}); esta sessão grava em {}",
                sram_path.display(),
                aside.display()
            );
            cab.push_osd(
                &[
                    tr!(
                        "Could not read this game's save.",
                        "Não deu para ler o save deste jogo."
                    ),
                    tr!(
                        "Your old save is untouched; this session saves beside it.",
                        "O save antigo fica intacto; esta sessão grava ao lado."
                    ),
                ],
                None,
                Duration::from_secs(10),
            );
            sram_path = aside;
        }
    }

    // We do our own NTSC (vendored blargg snes_ntsc, RF preset) on the raw
    // frame, so keep the core's built-in filter off.
    core.set_variable("snes9x_blargg", "disabled");
    // O visual da tela da configuração (a tela de ajustes troca depois).
    cab.set_screen_look(cfg.screen_look);
    cab.set_screen_lines(cfg.screen_lines);
    let mut ntsc = NtscFilter::new(crate::look_preset(cfg.screen_look));

    let av = core.av_info();
    log::info!(
        "av: {}x{} (max {}x{}) aspect={:.3} fps={:.3} sr={:.0}",
        av.base_width,
        av.base_height,
        av.max_width,
        av.max_height,
        av.aspect_ratio,
        av.fps,
        av.sample_rate
    );

    // Which note slots are pinned/captioned (plan revision) — loaded once,
    // mutated and re-saved in place on every pin toggle or rename.
    let mut notes_meta = load_notes_meta(&spec.notes_dir, &title);
    // One-time, no-op after the first run for this game — see the
    // function's own doc comment.
    migrate_legacy_text_notes(&spec.notes_dir, &title);

    // --- cheats: the full libretro-database slice for this title (plan
    // §4.4, revision) --- Matched by the ROM's own title (same string
    // saves/notes are keyed by), not the cartridge header any more — see
    // `sixteen_domain::cheats`'s doc comment for why. Empty if nothing in
    // the database lines up with it. Choosing them happens in the Cheat
    // Genius at power-on; the side panel only counts the ones on.
    let cheat_defs = sixteen_domain::cheats_for_title(&title);
    let cheat_path = cheat_state_path(&spec.save_dir, &title);
    let mut cheat_state = load_cheat_state(&cheat_path, cheat_defs.len());
    // RetroAchievements hardcore (plan fase 3): cheats stay off entirely.
    // A sessão exige a conta conectada (token Connect): sem ela não há
    // lógica de conquista para ativar nem envio para o servidor.
    let ra_on = !cfg.ra_user.is_empty() && cfg.ra_connect.is_some();
    let ra_hardcore_active = ra_on && cfg.ra_hardcore;
    if ra_hardcore_active && !cheat_defs.is_empty() {
        log::info!("ra hardcore: cheats disabled for this session");
        cheat_state = vec![false; cheat_defs.len()];
    }
    // Cheat Genius: a ROM do adaptador roda no power-on e decide o que liga
    // — é o único jeito de usar cheats. Fora no hardcore do RA (nada a
    // escolher) e na captura headless (sem controle; aí o jogo roda sem
    // cheats).
    // `SIXTEEN_DEBUG_GENIUS=1` (dev): com `--shot` arma o Genius já ligado,
    // para capturar a tela dele pelo mesmo caminho do jogo.
    let debug_genius = std::env::var_os("SIXTEEN_DEBUG_GENIUS").is_some();
    let genius_on = !ra_hardcore_active && (spec.shot.is_none() || debug_genius);
    let custom_path = game_dir(&spec.save_dir, &title).join("custom_cheats.txt");
    let mut customs: Vec<crate::genius::Custom> = if genius_on {
        fs::read_to_string(&custom_path)
            .map(|t| crate::genius::parse_customs(&t))
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    // Quem aplica os cheats é o veredito do Genius, depois do power-on.
    // --- side panel: logo, cartridge art, command legend, session timer
    // (plan §3.2) — cartridge art is new (plan revision): a second, optional
    // image alongside the logo, same local-file convention.
    // RetroAchievements session (plan: `docs/plano-retroachievements.md`,
    // fase 3) — armed only when the account is configured; identification
    // and cache reads happen here, on cart insert.
    let mut ra_session = if ra_on {
        match crate::ra::Active::start(
            &spec.rom,
            &cfg.ra_user,
            cfg.ra_connect.as_deref().unwrap_or(""),
            cfg.ra_hardcore,
        ) {
            Ok(Some(a)) => {
                log::info!(
                    "ra: {} — {} conquista(s), hardcore {}",
                    a.game_title,
                    a.achievements().len(),
                    if a.hardcore { "on" } else { "off" }
                );
                Some(a)
            }
            Ok(None) => None,
            Err(e) => {
                log::info!("ra: {e}");
                None
            }
        }
    } else {
        None
    };
    let ra_hardcore_active = ra_session.as_ref().is_some_and(|a| a.hardcore);
    // Earned do servidor para a sessão (uma rede só na primeira vez — o
    // disco cacheia por game id): conquistas ganhas fora deste app entram
    // no set da sessão quando o worker chega, e a modal in-game as marca.
    let mut ra_earned_worker = match (&ra_session, ra_on) {
        (Some(a), true) => Some(crate::ra::fetch_game_earned_worker(
            &cfg.ra_user,
            &cfg.ra_token,
            &a.hash,
        )),
        _ => None,
    };
    // Os badges do set em cache antes da primeira notificação (worker) — o
    // bloco "CONQUISTA DESBLOQUEADA" sai com a imagem de primeira.
    if ra_session.is_some() {
        crate::ra::prefetch_badges(&spec.rom);
    }

    // "Done!" flash for otherwise-silent actions (Nota/Salvar/Carregar) —
    // see `flashed`/`FLASH_DURATION`.
    let mut flash: HashMap<PanelButton, Instant> = HashMap::new();
    // The command legend's last drawn signature (a "(feito!)" flash active?
    // all print slots pinned?) — `None` forces the first frame to draw it.
    let mut prev_sig: Option<(bool, bool)> = None;
    let commands = command_rows(&flash, ra_session.is_some(), all_slots_pinned(&notes_meta));
    let decode_panel_art = |path: &Option<PathBuf>, kind: &str| {
        path.as_ref().and_then(|p| match decode_art(p, 640) {
            Ok(img) => Some(img),
            Err(e) => {
                log::warn!("{kind} {}: {e}", p.display());
                None
            }
        })
    };
    let logo_img = decode_panel_art(&spec.logo, "logo");
    // Console tag wordmark on the slot's base (plan revision) — reloaded per
    // game launch so a direct `emu-run` shows it too; the baked-in image is
    // the fallback (see `console_art`).
    crate::console_art::load_slot_tag(cab);
    let cartridge_img = decode_panel_art(&spec.cartridge, "cartridge");
    let has_cartridge_art = cartridge_img.is_some();
    cab.set_panel(
        logo_img.as_ref().map(|(w, h, d)| (*w, *h, d.as_slice())),
        cartridge_img
            .as_ref()
            .map(|(w, h, d)| (*w, *h, d.as_slice())),
        &title,
        &commands,
    );
    // Never inherited from whatever screen ran before (idle/shelf/settings
    // all turn it on) — see `Cabinet::show_close`'s own doc comment for why
    // gameplay doesn't get one.
    cab.set_close_button(false);
    // `set_cheats` has to come *after* `set_panel` — `set_panel` replaces
    // the whole `PanelInfo` (fresh `cheats: Vec::new()` included), so
    // calling this first, as an earlier revision did when the cheats-
    // loading block moved up ahead of the panel block, silently wiped out
    // the count before it ever
    // reached the screen — a real bug, caught by testing the panel's
    // "N cheats ativados" line and finding it never showed up at all.
    cab.set_cheats(&all_cheat_rows(&cheat_defs, &cheat_state, &customs));

    // --- notes: the notebook block, empty until the first capture (§3.4) --
    fs::create_dir_all(&spec.notes_dir).ok();
    refresh_notes(
        cab,
        &spec.notes_dir,
        &title,
        note_slot,
        text_slot,
        &notes_meta,
    );

    // Headless self-check: skip straight to the idle "console off" screen.
    if spec.shot_off {
        if let Some((path, _)) = &spec.shot {
            cab.capture_static_bmp(OFF_STATIC_LEVEL, path)
                .map_err(|e| anyhow!(e.to_string()))?;
            log::info!("wrote {} (idle-off preview)", path.display());
        }
        return Ok(GameExit::Quit);
    }

    // Headless self-check: skip straight to the pause book, against whatever
    // notes already exist on disk for this ROM (build them with a separate
    // --debug-note-capture run first).
    if spec.debug_shot_pause {
        if let Some((path, _)) = &spec.shot {
            cab.set_pause_note(&title, NOTE_SLOTS as usize);
            show_note_slot(cab, &spec.notes_dir, &title, note_slot, &notes_meta);
            show_text_slot(cab, &spec.notes_dir, &title, text_slot, &notes_meta);
            cab.capture_pause_bmp(path)
                .map_err(|e| anyhow!(e.to_string()))?;
            log::info!("wrote {} (pause book preview)", path.display());
        }
        return Ok(GameExit::Quit);
    }

    // Headless self-check: skip straight to one of the modals (plan
    // revision) instead of gameplay — same "build the rows, capture" shape
    // as the pause book above, just picking which modal by name.
    if let Some(kind) = &spec.debug_shot_modal {
        if let Some((path, _)) = &spec.shot {
            preview_modal(cab, kind, spec, &title, &notes_meta);
            cab.capture_modal_bmp(path)
                .map_err(|e| anyhow!(e.to_string()))?;
            log::info!("wrote {} (modal preview: {kind})", path.display());
        }
        return Ok(GameExit::Quit);
    }

    // The cartridge visibly sliding into the console's slot (plan revision),
    // right as the console goes from "nothing loaded" to "off, waiting for
    // Ligar" — skipped for a `--shot` capture (dev/testing; no button was
    // clicked to trigger it in the first place) and with no cartridge art to
    // animate.
    if spec.shot.is_none() && has_cartridge_art {
        cartridge_insert_animation(plat, cab);
    }
    if let Some(dir) = &spec.debug_cart_anim {
        std::fs::create_dir_all(dir).map_err(|e| anyhow!(e.to_string()))?;
        for (i, t) in [0.0f32, 0.12, 0.25, 0.4, 0.55, 0.7, 0.85, 1.0]
            .iter()
            .enumerate()
        {
            cab.set_cartridge_motion(Some((*t, false)));
            cab.capture_static_bmp(OFF_STATIC_LEVEL, &dir.join(format!("cart_{i:02}.bmp")))
                .map_err(|e| anyhow!(e.to_string()))?;
        }
        cab.set_cartridge_motion(None);
        cab.capture_static_bmp(OFF_STATIC_LEVEL, &dir.join("cart_08.bmp"))
            .map_err(|e| anyhow!(e.to_string()))?;
        return Ok(GameExit::Quit);
    }
    if let Some(dir) = &spec.debug_blow {
        crate::contato::debug_shots(&mut core, cab, &mut ntsc, &rom_bytes, &title, dir)?;
        return Ok(GameExit::Quit);
    }

    // --- audio -----------------------------------------------------------
    // Sem dispositivo de áudio o jogo roda mudo em vez de não abrir.
    let audio_rate = av.sample_rate.round().max(8000.0) as u32;
    let audio = plat.open_audio(audio_rate).unwrap_or_else(|e| {
        log::warn!("áudio do jogo não abriu, seguindo mudo: {e}");
        sixteen_platform::AudioOut::muted(audio_rate)
    });
    let mut input = plat.new_input();

    let mut paused = false;
    let frame_time = Duration::from_secs_f64(1.0 / av.fps.max(1.0));
    let mut next = Instant::now();
    let mut frames: u32 = 0;
    let mut fps_meter = FpsMeter::new();
    let (badge_tx, badge_rx): (BadgeTx, _) = std::sync::mpsc::channel();
    let mut last_dims = (0u32, 0u32);
    // Don't let the audio queue run more than ~0.15 s ahead (latency creep).
    let audio_cap = (av.sample_rate / 6.0) as usize;
    // Scratch buffer for per-frame volume scaling — reused instead of a
    // fresh Vec every frame (the default volume of 8 always takes this
    // path, so it used to allocate ~2–4 KB at 60 Hz).
    let mut vol_buf: Vec<i16> = Vec::new();
    let mut rf_audio = sixteen_platform::RfAudio::default();

    // O último SRAM gravado: o flush periódico só escreve quando mudar.
    let mut last_sram = core.sram();
    // A SRAM do jogo enquanto o Cheat Genius ocupa o core (ver
    // `launch_genius`/`restore_game`).
    let mut genius_sram: Option<Vec<u8>> = None;
    // Which note slot to save into once the next frame is ready — set at
    // click time, consumed after `core.run()` produces a real frame.
    let mut note_request: Option<u8> = None;
    // Debug capture lines up with --shot-frame (default: the very first
    // frame) so the saved page actually shows whatever --shot is inspecting,
    // not just a black boot frame.
    let debug_note_frame = spec.shot.as_ref().map_or(1, |(_, f)| *f).max(1);
    // The console: off until the player presses Power (plan revision —
    // picking a game from the shelf only inserts the cartridge, same as
    // real hardware, it doesn't boot itself); off again after a later
    // "Desligar", idling on snow until Eject either way (plan §3.3).
    // Exception: a plain `--shot` dev capture (not `--shot-off`/
    // `--debug-shot-pause`, both of which already returned above) exists to
    // inspect live gameplay, so it starts powered — there's no Power click
    // to send it in headless mode.
    let mut powered = spec.shot.is_some();
    cab.set_powered(powered);
    let mut session = SessionClock::new(powered);
    // Mau contato do cartucho (opcional): o encaixe é sorteado agora; um
    // encaixe ruim faz o próximo Power cair numa `FaultRun` em vez do jogo.
    // Durante a falha `powered` segue falso (nada de save, notas, RA ou
    // tempo de jogo) — só o painel mostra o console ligado.
    let mut contact = crate::contato::Contact::inserted(cfg.bad_contact, spec.shot.is_none());
    let mut fault: Option<crate::contato::FaultRun> = None;
    // A SRAM como estava logo depois de carregar — do disco ou zerada, num
    // jogo que ainda não tem `.srm` —, para a falha nunca chegar ao save.
    // `None` só num jogo sem SRAM nenhuma.
    let sram_at_insert = last_sram.clone();
    let mut static_level = OFF_STATIC_LEVEL;
    // Drives the one deliberate keyboard-typing exception (plan revision) —
    // the free-text note, a slot's caption, or a just-captured print's name
    // (see `NoteEdit`).
    let mut note_edit = NoteEdit::None;
    // Teclado na tela (gamepad) do editor de notas/legendas: nasce quando o
    // editor abre (`note_field` = None) e some quando fecha.
    let mut note_field: Option<(TextField, Osk)> = None;
    let mut note_draft = String::new();
    // A save/load-state or print slot picker (plan revision) — see `Modal`.
    let mut modal = Modal::None;
    // Gamepad dentro do jogo: o acorde Select+Start abre o Menu do jogo
    // (`Modal::GameMenu`); com um overlay aberto os botões físicos viram
    // navegação (`padnav`).
    let mut chord = sixteen_platform::padnav::Chord::default();
    let mut edges = sixteen_platform::padnav::EdgeTracker::default();
    let mut menu_acts: Vec<MenuAct> = Vec::new();
    // O pad (não o mouse) está conduzindo: liga o destaque nos modais.
    let mut pad_flow = false;
    // Overlay recém-fechado com botão ainda apertado: o jogo só volta a
    // ver o pad depois de soltar tudo (o B que fechou não vaza).
    let mut input_gate = false;
    let mut was_overlay = false;
    let mut hint_shown = false;
    // Set the instant "Printscreen" is clicked; the frame that's live once
    // `core.run()` next produces one gets cloned into `print_capture` below
    // and held there until the player finishes naming it (or cancels).
    let mut print_pending = false;
    let mut print_capture: Option<EmuFrame> = None;
    log::info!("running: rf ntsc + crt tube");
    // Cheat Genius: `genius_boot` fica armado até o primeiro power-on (e é
    // rearmado pelo Reset, como num adaptador de verdade no slot);
    // `genius_run` guarda quantas entradas a ROM em execução recebeu.
    let mut genius_boot = genius_on;
    let mut genius_run: Option<usize> = None;
    if debug_genius && genius_on && spec.shot.is_some() {
        genius_boot = false;
        genius_run = launch_genius(
            &mut core,
            &spec.rom,
            &rom_bytes,
            &sram_path,
            &mut last_sram,
            &mut genius_sram,
            &title,
            &cheat_defs,
            &cheat_state,
            &customs,
        );
    }

    let exit = 'run: loop {
        use sixteen_platform::padnav::bit;
        let pad_bits = plat.pad_bits();
        let overlay_open = paused || modal != Modal::None || note_edit != NoteEdit::None;
        // Com overlay aberto (ou o acorde em curso) o pad não troca de
        // porta: quem aperta Select+Start continua sendo o mesmo jogador.
        plat.set_hold_pad_order(overlay_open || chord.progress() > 0.0);
        if was_overlay && !overlay_open && pad_bits != 0 {
            input_gate = true;
        }
        was_overlay = overlay_open;
        let nav_edges = edges.update(pad_bits, frame_time);
        // Editing (text or a caption) is the one deliberate keyboard-typing
        // exception (plan revision) — while it's open, poll for composed
        // text/backspace/commit/cancel instead of gameplay input, so a key
        // meant for the editor doesn't also twitch the D-pad underneath it.
        if note_edit != NoteEdit::None {
            // Naming a fresh print (plan revision) renders in the modal, not
            // the notebook — everything else about polling/typing is shared.
            let in_modal = matches!(note_edit, NoteEdit::PrintName(_));
            let te = plat.poll_text_entry();
            let (field, osk) = note_field.get_or_insert_with(|| {
                let mut osk = Osk::new();
                osk.begin(plat.last_input_was_pad());
                (TextField::new(&note_draft, note_edit.limit(), false), osk)
            });
            let outcome = osk.update(&te, field, plat);
            if matches!(outcome, OskOutcome::Quit) {
                break 'run GameExit::Quit;
            }
            if field.text() != note_draft {
                note_draft = field.text().to_string();
                if in_modal {
                    cab.set_modal_draft(Some(&note_draft), note_edit.limit(), note_edit.heading());
                } else {
                    cab.set_pause_draft(Some(&note_draft), note_edit.limit(), note_edit.heading());
                }
            }
            let mut save = matches!(outcome, OskOutcome::Commit);
            let mut cancel = matches!(outcome, OskOutcome::Cancel);
            if let Some((x, y)) = te.click {
                let (ox, oy) = cab.window_to_output(x, y);
                let hit = if in_modal {
                    cab.hit_modal_button(ox, oy)
                } else {
                    cab.hit_pause_button(ox, oy)
                };
                match hit {
                    Some(PanelButton::PauseDraftSave) | Some(PanelButton::ModalConfirm) => {
                        save = true
                    }
                    Some(PanelButton::PauseDraftCancel) | Some(PanelButton::ModalCancel) => {
                        cancel = true
                    }
                    _ => {}
                }
            }
            if save {
                match note_edit {
                    NoteEdit::Text => {
                        let trimmed = note_draft.trim();
                        if trimmed.is_empty() {
                            delete_text_slot(&spec.notes_dir, &title, text_slot);
                            log::info!("text slot {text_slot}: cleared (saved empty)");
                        } else {
                            match save_text_slot(&spec.notes_dir, &title, text_slot, trimmed) {
                                Ok(()) => log::info!("text slot {text_slot}: saved"),
                                Err(e) => log::warn!("text slot {text_slot}: save failed: {e}"),
                            }
                        }
                        show_text_slot(cab, &spec.notes_dir, &title, text_slot, &notes_meta);
                    }
                    NoteEdit::SlotName => {
                        notes_meta.slots.entry(note_slot).or_default().label =
                            note_draft.trim().to_string();
                        save_notes_meta(&spec.notes_dir, &title, &notes_meta);
                        show_note_slot(cab, &spec.notes_dir, &title, note_slot, &notes_meta);
                        // A pinned print shows its caption in the panel too.
                        refresh_notes(
                            cab,
                            &spec.notes_dir,
                            &title,
                            note_slot,
                            text_slot,
                            &notes_meta,
                        );
                        log::info!("note slot {note_slot}: renamed");
                    }
                    NoteEdit::PrintName(print_slot) => {
                        if let Some(frame) = &print_capture {
                            match save_note_image(&spec.notes_dir, &title, print_slot, frame) {
                                Ok(_) => {
                                    notes_meta.slots.entry(print_slot).or_default().label =
                                        note_draft.trim().to_string();
                                    save_notes_meta(&spec.notes_dir, &title, &notes_meta);
                                    note_slot = print_slot;
                                    refresh_notes(
                                        cab,
                                        &spec.notes_dir,
                                        &title,
                                        note_slot,
                                        text_slot,
                                        &notes_meta,
                                    );
                                    log::info!("print: captured into slot {print_slot}");
                                }
                                Err(e) => log::warn!("print capture failed: {e}"),
                            }
                        }
                    }
                    _ => {}
                }
            }
            if save || cancel {
                if in_modal {
                    cab.set_modal_draft(None, 0, "");
                    modal = Modal::None;
                    print_capture = None;
                    cab.clear_modal();
                } else {
                    cab.set_pause_draft(None, 0, "");
                }
                note_edit = NoteEdit::None;
                note_draft.clear();
                note_field = None;
                plat.stop_text_input(cab);
            }
            match note_field.as_ref().filter(|(_, o)| o.shown()) {
                // Com o controle: o teclado na tela ocupa o lugar do livro.
                Some((field, osk)) => {
                    let heading = note_edit.heading();
                    cab.frame_2d((18, 18, 20), |d| osk.draw(d, field, heading));
                }
                None if in_modal => cab.present_modal(),
                None => cab.present_pause(),
            }
            pace_ui(&mut next, frame_time, plat, cab);
            continue;
        }

        // A clicked panel/pause-book button becomes exactly the `UiEvent` its
        // key used to send — the match below doesn't need to know clicks
        // exist at all. Which set of buttons a click can land on depends on
        // `paused`: the pause book replaces the whole window (no side panel
        // drawn alongside it), so it has its own hit-test.
        let events: Vec<UiEvent> = plat
            .poll(&mut input, &cfg.keymap)
            .into_iter()
            .filter_map(|ev| match ev {
                UiEvent::Click(x, y) => {
                    let (ox, oy) = cab.window_to_output(x, y);
                    // The titlebar pair is drawn on every screen now —
                    // gameplay included (plan revision: "mostrar o fechar e
                    // minimizar em todas as telas"). Checked before every
                    // in-screen hit-test.
                    if cab.hit_close_button(ox, oy) {
                        Some(UiEvent::CloseRequested)
                    } else if cab.hit_minimize_button(ox, oy) {
                        cab.minimize();
                        None
                    } else if modal != Modal::None {
                        // Not reachable during the naming step — that's
                        // `NoteEdit::PrintName`, polled
                        // via `poll_text_entry` instead (above), so `Click`
                        // never comes through `plat.poll()` while it's open.
                        match cab.hit_modal_button(ox, oy) {
                            Some(PanelButton::ModalSlot(i)) => Some(UiEvent::ModalPick(i)),
                            Some(PanelButton::ModalCancel) => Some(UiEvent::ModalCancel),
                            Some(PanelButton::ModalScrollUp) => Some(UiEvent::ModalScrollUp),
                            Some(PanelButton::ModalScrollDown) => Some(UiEvent::ModalScrollDown),
                            _ => None,
                        }
                    } else if paused {
                        // Not reachable here while `note_edit != None` — that
                        // branch polls via `poll_text_entry` instead (below
                        // the main match), so `Click` never comes through
                        // `plat.poll()` during it.
                        match cab.hit_pause_button(ox, oy) {
                            Some(PanelButton::PauseContinue) => Some(UiEvent::TogglePause),
                            Some(PanelButton::PauseNotePrev) => Some(UiEvent::NotePrev),
                            Some(PanelButton::PauseNoteNext) => Some(UiEvent::NoteNext),
                            Some(PanelButton::PauseWrite) => Some(UiEvent::NoteWriteStart),
                            Some(PanelButton::PauseNotePin) => Some(UiEvent::NotePinToggle),
                            Some(PanelButton::PauseNoteName) => Some(UiEvent::NoteNameStart),
                            Some(PanelButton::PauseTextPrev) => Some(UiEvent::TextPrev),
                            Some(PanelButton::PauseTextNext) => Some(UiEvent::TextNext),
                            Some(PanelButton::PauseTextPin) => Some(UiEvent::TextPinToggle),
                            Some(PanelButton::PauseTextDelete) => Some(UiEvent::TextDelete),
                            _ => None,
                        }
                    } else if let Some(tv) = cab.hit_tv_controls(ox, oy) {
                        // Os controles físicos da TV no gameplay (plan
                        // revision: "tem que estar sempre na tela" +
                        // "possível desligar a tv com o jogo rodando") —
                        // o mesmo par do idle: o knob POWER liga/desliga
                        // o tubo (o jogo segue correndo por baixo), VOL
                        // muda o volume e acende o OSD de barrinhas.
                        cab.apply_tv_control(tv);
                        None
                    } else {
                        cab.hit_panel_button(ox, oy).and_then(|b| match b {
                            PanelButton::Power => Some(UiEvent::Quit),
                            PanelButton::Eject => Some(UiEvent::Eject),
                            PanelButton::Reset => Some(UiEvent::Reset),
                            PanelButton::Notebook => Some(UiEvent::TogglePause),
                            PanelButton::Achievements => Some(UiEvent::OpenAchievementsModal),
                            // The shelf's own list button is shelf-side.
                            PanelButton::ShelfAchievements => None,
                            PanelButton::PrintScreen => Some(UiEvent::OpenPrintModal),
                            PanelButton::SaveState => Some(UiEvent::OpenSaveModal),
                            PanelButton::LoadState => Some(UiEvent::OpenLoadModal),
                            // The rest are all idle-screen-, pause-book- or
                            // modal-only, never shown alongside the panel
                            // that's up now. `Dev` and `ConsoleManual` are
                            // idle-screen-only too (their screens live
                            // outside a game). Same for
                            // `Insert` — the idle screen owns it.
                            PanelButton::Insert
                            | PanelButton::Settings
                            | PanelButton::Dev
                            | PanelButton::ConsoleManual
                            | PanelButton::CoreDownload
                            | PanelButton::PauseContinue
                            | PanelButton::PauseNotePrev
                            | PanelButton::PauseNoteNext
                            | PanelButton::PauseWrite
                            | PanelButton::PauseNotePin
                            | PanelButton::PauseNoteName
                            | PanelButton::PauseTextPrev
                            | PanelButton::PauseTextNext
                            | PanelButton::PauseTextPin
                            | PanelButton::PauseTextDelete
                            | PanelButton::PauseDraftSave
                            | PanelButton::PauseDraftCancel
                            | PanelButton::ModalSlot(_)
                            | PanelButton::ModalConfirm
                            | PanelButton::ModalCancel
                            | PanelButton::ModalScrollUp
                            | PanelButton::ModalScrollDown => None,
                        })
                    }
                }
                other => Some(other),
            })
            .collect();
        // Com a tela do Cheat Genius no ar só valem Desligar, Ejetar e fechar:
        // nada de notas, estados ou modais por cima da ROM do adaptador.
        let events: Vec<UiEvent> = if genius_run.is_some() {
            events
                .into_iter()
                .filter(|e| matches!(e, UiEvent::Quit | UiEvent::Eject | UiEvent::CloseRequested))
                .collect()
        } else {
            events
        };
        let mut events: std::collections::VecDeque<UiEvent> = events.into();
        plat.set_playing(powered && !paused && modal == Modal::None && !plat.window_hidden());
        // Janela minimizada com o jogo correndo: pausa — emular, filtrar e
        // desenhar o que ninguém vê é bateria jogada fora. Na volta o livro
        // de pausa está esperando, como se o jogador tivesse pausado.
        if plat.window_hidden()
            && powered
            && !paused
            && modal == Modal::None
            && note_edit == NoteEdit::None
            && genius_run.is_none()
            && fault.is_none()
            && !events.iter().any(|e| matches!(e, UiEvent::TogglePause))
        {
            log::info!("janela escondida: pausando o jogo");
            events.push_back(UiEvent::TogglePause);
        }
        // Mouse na jogada: o destaque do pad sai dos modais.
        if events.iter().any(|e| matches!(e, UiEvent::Click(..))) {
            pad_flow = false;
        }
        // --- Gamepad dentro do jogo ---
        let overlay_now = paused || modal != Modal::None;
        let mut swallow_menu_btns = false;
        if overlay_now {
            // Overlay aberto: os botões físicos navegam (o jogo está parado).
            chord.reset_latched();
            if nav_edges != 0 {
                pad_flow = true;
            }
            if modal != Modal::None {
                let d = |neg: u16, pos: u16| {
                    (nav_edges & pos != 0) as i32 - (nav_edges & neg != 0) as i32
                };
                let (dx, dy) = (d(bit::LEFT, bit::RIGHT), d(bit::UP, bit::DOWN));
                let page = d(bit::LB, bit::RB) * 5;
                if dx != 0 || dy != 0 || page != 0 {
                    cab.move_modal_cursor(dx, dy, page);
                }
                if nav_edges & bit::A != 0 {
                    match cab.modal_cursor() {
                        Some(i) => events.push_back(UiEvent::ModalPick(i as u16)),
                        // Primeiro toque só mostra o destaque.
                        None => cab.show_modal_cursor(),
                    }
                }
                if nav_edges & bit::B != 0 {
                    events.push_back(UiEvent::ModalCancel);
                }
            } else {
                events.extend(book_pad_events(nav_edges));
            }
        } else {
            let out = chord.update(pad_bits, frame_time);
            swallow_menu_btns = out.swallow;
            if out.fire {
                // Abre o Menu do jogo (acorde Select+Start segurado).
                let rows = game_menu_rows(powered, genius_run.is_some(), ra_session.is_some());
                menu_acts = rows.iter().map(|(a, _)| *a).collect();
                let labels: Vec<(String, bool)> =
                    rows.into_iter().map(|(_, l)| (l, true)).collect();
                cab.clear_modal();
                modal = Modal::GameMenu;
                cab.set_modal(tr!("Game menu", "Menu do jogo"), &labels);
                pad_flow = true;
                edges.swallow_held(pad_bits);
                swallow_menu_btns = true;
            }
        }
        if pad_flow && modal != Modal::None {
            cab.show_modal_cursor();
        }
        // Acorde segurado ou botão que fechou um overlay: o jogo não vê.
        let hide_pad = swallow_menu_btns;
        // Dica única ~2 s depois do jogo começar (o acorde não tem outra
        // pista na tela): só com um pad conectado.
        if !hint_shown && powered && frames >= 120 {
            hint_shown = true;
            if !plat.gamepad_slots().is_empty() {
                cab.push_osd(
                    &[tr!(
                        "hold Select+Start = menu",
                        "segure Select+Start = menu"
                    )],
                    None,
                    Duration::from_secs(4),
                );
            }
        }
        if input_gate && pad_bits == 0 {
            input_gate = false;
        }
        while let Some(ev) = events.pop_front() {
            match ev {
                UiEvent::Quit if fault.is_some() => {
                    // Desligar na falha: o core volta ao encaixe (a falha
                    // pode ter rodado a abertura) e ligar de novo às vezes
                    // resolve sozinho.
                    static_level = power_off_burst(plat, cab);
                    fault = None;
                    restore_after_fault(&mut core, sram_at_insert.as_deref(), &mut last_sram);
                    audio.clear();
                    contact.power_cycled();
                    cab.set_powered(false);
                    log::info!("power off on a bad contact");
                }
                UiEvent::Quit => {
                    if powered {
                        // Desligar (plan §3.3): flush the cart, then the
                        // signal-off ritual. A second click while already off
                        // does nothing on purpose — it's Ligar (below) now.
                        flush_sram(&sram_path, &mut last_sram, core.sram());
                        cab.set_session_time(session.stop());
                        static_level = power_off_burst(plat, cab);
                        powered = false;
                        cab.set_powered(false);
                        log::info!("power off — eject to leave, click power to resume");
                    } else {
                        // Ligar de novo: same button as power off, now
                        // toggling back on — the game resumes exactly where
                        // it was, no reload. The "CH 3" banner flashes for a
                        // few seconds over the picture, the way a TV shows
                        // the channel when you tune it (plan revision).
                        power_on_burst(plat, cab);
                        if let Some(kind) = contact.power_on() {
                            log::info!("mau contato: {kind:?}");
                            fault = Some(crate::contato::FaultRun::new(kind));
                            cab.set_powered(true);
                            cab.flash_ch3(CH3_FLASH);
                            continue;
                        }
                        powered = true;
                        session.start();
                        cab.set_powered(true);
                        cab.flash_ch3(CH3_FLASH);
                        // Console power-on re-arms the RA hit counts (plan
                        // fase 5) — earned stays authoritative locally, and
                        // the server deduplicates.
                        if let Some(ra) = &mut ra_session {
                            ra.reset();
                        }
                        log::info!("power on — resuming");
                        if genius_boot {
                            genius_boot = false;
                            genius_run = launch_genius(
                                &mut core,
                                &spec.rom,
                                &rom_bytes,
                                &sram_path,
                                &mut last_sram,
                                &mut genius_sram,
                                &title,
                                &cheat_defs,
                                &cheat_state,
                                &customs,
                            );
                        }
                    }
                }
                UiEvent::Eject => {
                    if powered || fault.is_some() {
                        eject_clunk(plat); // lock resists while it's still on
                    } else if contact.needs_ritual() {
                        // Falhou desde o encaixe: o cartucho sai e vai para a
                        // frente da TV, para soprar.
                        audio.clear();
                        if has_cartridge_art {
                            cartridge_eject_animation(plat, cab);
                        }
                        match crate::contato::blow_ritual(
                            plat,
                            cab,
                            &mut input,
                            &cfg.keymap,
                            &title,
                            cfg.blow_mic,
                        ) {
                            crate::contato::RitualExit::Reinsert { clean, fog } => {
                                contact.reinserted(clean, fog);
                                cab.set_cartridge_motion(None);
                                if has_cartridge_art {
                                    cartridge_insert_animation(plat, cab);
                                }
                                log::info!("reinserted after blowing (clean {clean:.2})");
                            }
                            crate::contato::RitualExit::Shelf => {
                                if let Some(ra) = &mut ra_session {
                                    ra.save_progress();
                                }
                                break 'run GameExit::Ejected { static_level };
                            }
                            crate::contato::RitualExit::Quit => break 'run GameExit::Quit,
                        }
                    } else {
                        if let Some(ra) = &mut ra_session {
                            ra.save_progress();
                        }
                        if has_cartridge_art {
                            cartridge_eject_animation(plat, cab);
                        }
                        break 'run GameExit::Ejected { static_level };
                    }
                }
                UiEvent::CloseRequested => break 'run GameExit::Quit,
                UiEvent::Reset => {
                    if powered && genius_on && genius_run.is_none() {
                        // O adaptador está no slot: reiniciar passa por ele de
                        // novo, e é aqui que se muda os cheats.
                        genius_run = launch_genius(
                            &mut core,
                            &spec.rom,
                            &rom_bytes,
                            &sram_path,
                            &mut last_sram,
                            &mut genius_sram,
                            &title,
                            &cheat_defs,
                            &cheat_state,
                            &customs,
                        );
                        crate::sfx::play(cab, crate::sfx::Sfx::Reset);
                        flash.insert(PanelButton::Reset, Instant::now());
                    } else if powered {
                        core.reset();
                        crate::sfx::play(cab, crate::sfx::Sfx::Reset);
                        // Momentary rocker (plan revision) — springs back on
                        // its own next frame via `reset_pressed`/`RESET_SPRING`,
                        // same clock as the "feito!" flashes.
                        flash.insert(PanelButton::Reset, Instant::now());
                    }
                }
                UiEvent::TogglePause if powered => {
                    paused = !paused;
                    if paused {
                        // Load once on the way in; present_pause just
                        // redraws it every frame (plan §3.2/§3.4) — on
                        // whichever slots were last selected.
                        cab.set_pause_note(&title, NOTE_SLOTS as usize);
                        show_note_slot(cab, &spec.notes_dir, &title, note_slot, &notes_meta);
                        show_text_slot(cab, &spec.notes_dir, &title, text_slot, &notes_meta);
                    }
                    log::info!("{}", if paused { "paused" } else { "resumed" });
                }
                UiEvent::NotePrev if paused && note_slot > 1 => {
                    note_slot -= 1;
                    show_note_slot(cab, &spec.notes_dir, &title, note_slot, &notes_meta);
                }
                UiEvent::NoteNext if paused && note_slot < NOTE_SLOTS => {
                    note_slot += 1;
                    show_note_slot(cab, &spec.notes_dir, &title, note_slot, &notes_meta);
                }
                UiEvent::NoteWriteStart if paused && !notes_meta.text_slot(text_slot).pinned => {
                    note_edit = NoteEdit::Text;
                    note_draft =
                        read_text_slot(&spec.notes_dir, &title, text_slot).unwrap_or_default();
                    cab.set_pause_draft(
                        Some(&note_draft),
                        NoteEdit::Text.limit(),
                        NoteEdit::Text.heading(),
                    );
                    plat.start_text_input(cab);
                }
                UiEvent::NotePinToggle if paused => {
                    let m = notes_meta.slots.entry(note_slot).or_default();
                    m.pinned = !m.pinned;
                    let now_pinned = m.pinned;
                    save_notes_meta(&spec.notes_dir, &title, &notes_meta);
                    show_note_slot(cab, &spec.notes_dir, &title, note_slot, &notes_meta);
                    // The panel's own notes block only features pinned slots
                    // now (plan revision) — recompute it so the change is
                    // already reflected once the player resumes, not stale
                    // until the next capture.
                    refresh_notes(
                        cab,
                        &spec.notes_dir,
                        &title,
                        note_slot,
                        text_slot,
                        &notes_meta,
                    );
                    log::info!(
                        "note slot {note_slot}: {}",
                        if now_pinned { "pinned" } else { "unpinned" }
                    );
                }
                UiEvent::NoteNameStart if paused => {
                    note_edit = NoteEdit::SlotName;
                    note_draft = notes_meta.slot(note_slot).label;
                    cab.set_pause_draft(
                        Some(&note_draft),
                        NoteEdit::SlotName.limit(),
                        NoteEdit::SlotName.heading(),
                    );
                    plat.start_text_input(cab);
                }
                UiEvent::TextPrev if paused && text_slot > 1 => {
                    text_slot -= 1;
                    show_text_slot(cab, &spec.notes_dir, &title, text_slot, &notes_meta);
                }
                UiEvent::TextNext if paused && text_slot < NOTE_SLOTS => {
                    text_slot += 1;
                    show_text_slot(cab, &spec.notes_dir, &title, text_slot, &notes_meta);
                }
                UiEvent::TextPinToggle if paused => {
                    let m = notes_meta.text_slots.entry(text_slot).or_default();
                    m.pinned = !m.pinned;
                    let now_pinned = m.pinned;
                    save_notes_meta(&spec.notes_dir, &title, &notes_meta);
                    show_text_slot(cab, &spec.notes_dir, &title, text_slot, &notes_meta);
                    // Same reasoning as `NotePinToggle`'s own refresh: the
                    // panel only features pinned slots, so a text pin
                    // toggle needs to reach it right away too.
                    refresh_notes(
                        cab,
                        &spec.notes_dir,
                        &title,
                        note_slot,
                        text_slot,
                        &notes_meta,
                    );
                    log::info!(
                        "text slot {text_slot}: {}",
                        if now_pinned { "pinned" } else { "unpinned" }
                    );
                }
                UiEvent::TextDelete if paused && !notes_meta.text_slot(text_slot).pinned => {
                    delete_text_slot(&spec.notes_dir, &title, text_slot);
                    show_text_slot(cab, &spec.notes_dir, &title, text_slot, &notes_meta);
                    log::info!("text slot {text_slot}: deleted");
                }
                UiEvent::OpenSaveModal if powered => {
                    if ra_hardcore_active {
                        cab.push_osd(
                            &[
                                tr!("HARDCORE MODE", "MODO HARDCORE"),
                                tr!("savestates blocked", "savestates bloqueados"),
                            ],
                            None,
                            Duration::from_secs(3),
                        );
                        continue;
                    }
                    modal = Modal::SaveSlot;
                    cab.set_modal(
                        tr!("Save state", "Salvar estado"),
                        &save_slot_rows(&spec.save_dir, &title),
                    );
                }
                UiEvent::OpenLoadModal if powered => {
                    if ra_hardcore_active {
                        cab.push_osd(
                            &[
                                tr!("HARDCORE MODE", "MODO HARDCORE"),
                                tr!("loadstates blocked", "loadstates bloqueados"),
                            ],
                            None,
                            Duration::from_secs(3),
                        );
                        continue;
                    }
                    modal = Modal::LoadSlot;
                    cab.set_modal(
                        tr!("Load state", "Carregar estado"),
                        &load_slot_rows(&spec.save_dir, &title),
                    );
                }
                UiEvent::OpenAchievementsModal if powered => {
                    let Some(ra) = &ra_session else { continue };
                    let (total, earned) = ra.earned_snapshot();
                    // Cada linha diz o modo do ganho: [HC] hardcore,
                    // [SC] softcore, [ ] não ganha (plan revision: "como
                    // sei qual tipo ganhei ou já tenho?").
                    let rows: Vec<(String, bool)> = ra
                        .achievements()
                        .iter()
                        .map(|a| {
                            let modo = match earned.get(&a.id) {
                                Some(true) => " [HC]",
                                Some(false) => " [SC]",
                                None => "",
                            };
                            (
                                format!("{} ({} pts){}", a.title, a.points, modo),
                                earned.contains_key(&a.id),
                            )
                        })
                        .collect();
                    modal = Modal::Achievements;
                    cab.set_modal(
                        &tr!(
                            format!("Achievements {}/{}", earned.len(), total),
                            format!("Conquistas {}/{}", earned.len(), total)
                        ),
                        &rows,
                    );
                }
                UiEvent::OpenPrintModal if powered => {
                    if all_slots_pinned(&notes_meta) {
                        log::warn!("print skipped: all {NOTE_SLOTS} slots pinned");
                    } else {
                        // The actual capture happens once `core.run()` next
                        // produces a frame, below — same "wait for a real
                        // frame" pattern `--debug-note-capture` already uses.
                        print_pending = true;
                    }
                }
                UiEvent::ModalPick(i) => match modal {
                    // Save/load/print never have more than 15 rows — safe
                    // to narrow back to `u8` (`ModalPick` is `u16` only
                    // because the achievements list can be longer).
                    Modal::SaveSlot => {
                        let path = state_file(&spec.save_dir, &title, i as u8);
                        match core.save_state() {
                            Some(s) => match write_atomic(&path, &s) {
                                Ok(_) => {
                                    log::info!("slot {i}: saved ({} KiB)", s.len() / 1024);
                                    flash.insert(PanelButton::SaveState, Instant::now());
                                    modal = Modal::None;
                                    cab.clear_modal();
                                }
                                Err(e) => {
                                    // Disco cheio/SD só leitura: o jogador
                                    // precisa saber — o modal fecha e o
                                    // aviso sai sobre o jogo.
                                    log::warn!("slot {i}: save failed: {e}");
                                    modal = Modal::None;
                                    cab.clear_modal();
                                    cab.push_osd(
                                        &[tr!(
                                            "Could not save the state (disk full or read-only?)",
                                            "Não deu para salvar o estado (disco cheio ou só leitura?)"
                                        )],
                                        None,
                                        Duration::from_secs(5),
                                    );
                                }
                            },
                            None => log::warn!("core doesn't support save states"),
                        }
                    }
                    Modal::LoadSlot => {
                        let path = state_file(&spec.save_dir, &title, i as u8);
                        match fs::read(&path) {
                            Ok(s) if core.load_state(&s) => {
                                log::info!("slot {i}: loaded");
                                flash.insert(PanelButton::LoadState, Instant::now());
                                modal = Modal::None;
                                cab.clear_modal();
                            }
                            // An empty/rejected slot just stays disabled in the
                            // modal — nothing to load, nothing changes.
                            Ok(_) => {
                                // Estado de outra versão do núcleo, ou
                                // corrompido: o jogo segue como estava.
                                log::warn!("slot {i}: core rejected the state");
                                modal = Modal::None;
                                cab.clear_modal();
                                cab.push_osd(
                                    &[tr!(
                                        "This state could not be loaded (made by another core version?)",
                                        "Este estado não carregou (feito com outra versão do núcleo?)"
                                    )],
                                    None,
                                    Duration::from_secs(5),
                                );
                            }
                            Err(_) => log::warn!("slot {i}: empty"),
                        }
                    }
                    Modal::PrintSlot => {
                        let picked = i as u8 + 1; // modal rows are 0-based, slots 1..=15
                        if !notes_meta.slot(picked).pinned {
                            note_edit = NoteEdit::PrintName(picked);
                            note_draft.clear();
                            cab.set_modal_draft(
                                Some(""),
                                NoteEdit::PrintName(picked).limit(),
                                NoteEdit::PrintName(picked).heading(),
                            );
                            plat.start_text_input(cab);
                        }
                    }
                    // Read-only list (plan fase 4): a pick closes, nothing
                    // toggles.
                    Modal::Achievements => {
                        modal = Modal::None;
                        cab.clear_modal();
                    }
                    Modal::GameMenu => {
                        let act = menu_acts.get(i as usize).copied();
                        modal = Modal::None;
                        cab.clear_modal();
                        if let Some(act) = act {
                            // Fica no começo da fila: roda já, na ordem.
                            for e in menu_act_events(act, powered).into_iter().rev() {
                                events.push_front(e);
                            }
                        }
                    }
                    Modal::None => {}
                },
                UiEvent::ModalCancel => {
                    modal = Modal::None;
                    print_capture = None;
                    cab.clear_modal();
                }
                UiEvent::ModalScrollUp if modal != Modal::None => cab.scroll_modal(-1),
                UiEvent::ModalScrollDown if modal != Modal::None => cab.scroll_modal(1),
                // The rest only make sense with the console on (or, for the
                // pause-book trio, only while actually paused); ignored
                // otherwise. `Click` never reaches this match — it's already
                // resolved into one of the arms above (or dropped) before
                // the loop.
                UiEvent::TogglePause
                | UiEvent::OpenSaveModal
                | UiEvent::OpenLoadModal
                | UiEvent::OpenAchievementsModal
                | UiEvent::OpenPrintModal
                | UiEvent::ModalScrollUp
                | UiEvent::ModalScrollDown
                | UiEvent::NotePrev
                | UiEvent::NoteNext
                | UiEvent::NoteWriteStart
                | UiEvent::NotePinToggle
                | UiEvent::NoteNameStart
                | UiEvent::TextPrev
                | UiEvent::TextNext
                | UiEvent::TextPinToggle
                | UiEvent::TextDelete
                | UiEvent::Click(..) => {}
            }
        }
        // The command legend only changes when a "(feito!)" flash starts or
        // ends, or the print-slots-pinned state flips — rebuilding its
        // Strings at 60fps for an unchanged panel was churn (perf pass).
        let flashing_now = flash.values().any(|t| t.elapsed() < FLASH_DURATION);
        let all_pinned = all_slots_pinned(&notes_meta);
        let sig = (flashing_now, all_pinned);
        if Some(sig) != prev_sig {
            cab.set_commands(&command_rows(&flash, ra_session.is_some(), all_pinned));
            prev_sig = Some(sig);
        }
        cab.set_reset_pressed(reset_pressed(&flash));

        if let (Some(_), true) = (&fault, modal != Modal::None) {
            // O Menu do jogo aberto sobre a falha: só o modal.
            cab.present_modal();
            pace_ui(&mut next, frame_time, plat, cab);
            continue;
        }
        if let Some(f) = &mut fault {
            f.frame(
                &mut core,
                &mut ntsc,
                cab,
                &audio,
                audio_cap,
                &rom_bytes,
                &mut vol_buf,
                &mut rf_audio,
            );
            pace_frame(&mut next, frame_time);
            continue;
        }

        if !powered {
            cab.set_session_time(session.now());
            if modal != Modal::None {
                // Menu do jogo com o console desligado (Ligar/Ejetar/Sair).
                cab.present_modal();
                pace_ui(&mut next, frame_time, plat, cab);
                continue;
            }
            cab.present_static(OFF_STATIC_LEVEL);
            pace_ui(&mut next, frame_time, plat, cab);
            continue;
        }

        if !paused && modal == Modal::None {
            use sixteen_platform::PadButton as P;
            for port in 0..MAX_PORTS {
                for (rb, pb) in PAD {
                    // Select/Start engolidos enquanto o acorde está em curso;
                    // `input_gate` zera tudo até soltar o botão que fechou
                    // o overlay.
                    let hidden = input_gate || (hide_pad && matches!(pb, P::Select | P::Start));
                    core.set_button(port, rb, !hidden && input.held(port, pb));
                }
            }
            core.run();
            frames += 1;
            // Cheat Genius decidiu? Devolve o jogo ao core e aplica a escolha.
            if let Some(sent) = genius_run {
                if let Some(v) = crate::genius::poll(&mut core, sent) {
                    genius_run = None;
                    restore_game(
                        &mut core,
                        &spec.rom,
                        &rom_bytes,
                        &sram_path,
                        &mut last_sram,
                        &mut genius_sram,
                    );
                    audio.clear();
                    let rows = if v.play {
                        crate::genius::apply_verdict(&v, &mut cheat_state, &mut customs);
                        save_cheat_state(&cheat_path, &cheat_state);
                        if let Err(e) =
                            write_atomic(&custom_path, crate::genius::serialize_customs(&customs))
                        {
                            log::warn!("custom cheats flush failed: {e}");
                        }
                        apply_cheats(&mut core, &cheat_defs, &cheat_state, &customs);
                        log::info!(
                            "cheat genius: jogar — {} do banco e {} próprio(s) ligados",
                            cheat_state.iter().filter(|&&b| b).count(),
                            customs.iter().filter(|c| c.on).count()
                        );
                        all_cheat_rows(&cheat_defs, &cheat_state, &customs)
                    } else {
                        // B: este jogo roda sem nenhum, o que estava salvo
                        // continua salvo para a próxima vez.
                        core.cheat_reset();
                        log::info!("cheat genius: cancelado, jogando sem cheats");
                        let off: Vec<crate::genius::Custom> = customs
                            .iter()
                            .map(|c| crate::genius::Custom {
                                code: c.code.clone(),
                                on: false,
                            })
                            .collect();
                        all_cheat_rows(&cheat_defs, &vec![false; cheat_defs.len()], &off)
                    };
                    cab.set_cheats(&rows);
                    continue;
                }
            }
            if spec.debug_note_capture && frames == debug_note_frame {
                note_request = Some(note_slot);
            }
            if audio.queued_frames() < audio_cap {
                queue_game_audio(&audio, cab, core.audio(), &mut vol_buf, &mut rf_audio);
            }

            // Earned do servidor chegando: une no set da sessão (memória e
            // disco) — a modal de conquistas passa a marcar o que foi
            // ganho fora deste app, e nunca re-submete.
            if let Some(rx) = &ra_earned_worker {
                match rx.try_recv() {
                    Ok((_, Some(ids))) => {
                        if let Some(ra) = &mut ra_session {
                            ra.absorb_earned(&ids);
                        }
                        ra_earned_worker = None;
                    }
                    Ok((_, None)) => ra_earned_worker = None,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => ra_earned_worker = None,
                    Err(std::sync::mpsc::TryRecvError::Empty) => {}
                }
            }

            for (id, (w, h, rgba)) in badge_rx.try_iter() {
                cab.set_image(id, w, h, &rgba);
            }

            // RA evaluation (plan fase 3) — the real frame's RAM, before any
            // speculative run-ahead frames mutate the state further.
            if let (true, Some(ra)) = (powered && genius_run.is_none(), &mut ra_session) {
                let mut unlocks = Vec::new();
                core.with_memory(sixteen_emulation::MEMORY_SYSTEM_RAM, |ram| {
                    unlocks = ra.tick(ram);
                });
                for unlock in unlocks {
                    log::info!(
                        "ra: CONQUISTA DESBLOQUEADA — {} (+{} pts)",
                        unlock.title,
                        unlock.points
                    );
                    ra_session
                        .as_ref()
                        .expect("checked above")
                        .submit_unlock(&unlock);
                    notify_unlock(cab, &unlock, &badge_tx);
                }
            }

            if let Some(frame) = core.take_frame() {
                let dims = (frame.width, frame.height);
                if dims != last_dims {
                    log::info!(
                        "core framebuffer: {}x{} ({:?})",
                        dims.0,
                        dims.1,
                        frame.format
                    );
                    last_dims = dims;
                }
                // O menu de imagem da TV mexe no filtro de verdade (só refaz
                // a tabela quando o valor muda).
                ntsc.set_picture(crate::ntsc_picture(cab.picture()));
                ntsc.set_preset(crate::look_preset(cab.screen_look()));
                // RF NTSC on RGB565 frames; anything else passes straight through.
                // TV desligada (vidro morto, sem transição): a imagem nem é
                // desenhada — o quadro cru basta e o blit do NTSC fica de fora.
                let tv_dark = !cab.tv_on() && !cab.animating();
                let fref = if frame.format == EmuFormat::Rgb565 && !tv_dark {
                    ntsc_frame(
                        &mut ntsc,
                        &frame.pixels,
                        frame.width,
                        frame.height,
                        frame.pitch,
                    )
                } else {
                    FrameRef {
                        width: frame.width,
                        height: frame.height,
                        pitch: frame.pitch,
                        format: map_format(frame.format),
                        pixels: &frame.pixels,
                    }
                };
                let aspect = core.av_info().aspect_ratio;
                cab.set_session_time(session.now());
                cab.present_frame(&fref, aspect);
                fps_meter.frame(cab);

                if let Some(slot) = note_request.take() {
                    match save_note_image(&spec.notes_dir, &title, slot, &frame) {
                        Ok(_) => {
                            refresh_notes(
                                cab,
                                &spec.notes_dir,
                                &title,
                                note_slot,
                                text_slot,
                                &notes_meta,
                            );
                            log::info!("note: captured into slot {slot}");
                        }
                        Err(e) => log::warn!("note capture failed: {e}"),
                    }
                }

                if print_pending {
                    // Grabbed now, held until the player finishes picking a
                    // slot and naming it (or cancels) — see `NoteEdit::
                    // PrintName`. Not written to disk yet: nothing's final
                    // until they confirm the name.
                    print_pending = false;
                    print_capture = Some(frame.clone());
                    modal = Modal::PrintSlot;
                    cab.set_modal(
                        tr!("Where to save the screenshot?", "Onde salvar o print?"),
                        &print_slot_rows(&spec.notes_dir, &title, &notes_meta),
                    );
                }

                if let Some((path, at)) = &spec.shot {
                    if frames >= *at {
                        cab.capture_bmp(&fref, aspect, path)
                            .map_err(|e| anyhow!(e.to_string()))?;
                        log::info!("wrote {} after {} frames", path.display(), frames);
                        break 'run GameExit::Quit;
                    }
                }

                core.recycle_frame(frame);
            }

            // Periodically flush battery SRAM if it changed.
            if frames.is_multiple_of(SRAM_FLUSH_FRAMES) {
                flush_sram(&sram_path, &mut last_sram, core.sram());
            }
        } else if paused {
            // Paused, not stepping: the book, not a frozen game frame.
            cab.present_pause();
        } else {
            // A save/load-state or print slot picker is open (plan
            // revision): the modal, frozen same as the book is.
            cab.present_modal();
        }

        // Jogo rodando: o ritmo do core, sempre. Pausado ou com o Menu do
        // jogo aberto, a tela está parada — o ritmo ocioso vale.
        if paused || modal != Modal::None {
            pace_ui(&mut next, frame_time, plat, cab);
        } else {
            pace_frame(&mut next, frame_time);
        }
    };

    // Saiu no meio de uma falha (janela fechada): o core volta ao encaixe
    // antes do flush, para a abertura que rodou nunca chegar ao save.
    if fault.is_some() {
        restore_after_fault(&mut core, sram_at_insert.as_deref(), &mut last_sram);
    }
    // Final SRAM flush on the way out (either exit path). Saindo com o
    // Cheat Genius no core, a SRAM do jogo é a guardada na ida.
    let final_sram = genius_sram.take().or_else(|| core.sram());
    flush_sram(&sram_path, &mut last_sram, final_sram);

    // Fecha o trecho ligado que ainda corria (saiu com o console ligado,
    // ex.: janela fechada) e soma ao total de todos os tempos.
    add_playtime(&spec.save_dir, &title, session.stop().as_secs());

    if let Some(ra) = &mut ra_session {
        ra.save_progress();
    }
    log::info!("game loop done");
    Ok(exit)
}

#[cfg(test)]
mod tests {
    #[test]
    fn game_menu_rows_follow_the_console_state() {
        use super::{game_menu_rows, menu_act_events, MenuAct};
        use sixteen_platform::UiEvent;
        let acts = |p, g, r| -> Vec<MenuAct> {
            game_menu_rows(p, g, r)
                .into_iter()
                .map(|(a, _)| a)
                .collect()
        };
        let on = acts(true, false, false);
        assert_eq!(on.first(), Some(&MenuAct::Continue));
        assert!(on.contains(&MenuAct::Notes) && !on.contains(&MenuAct::Achievements));
        assert!(acts(true, false, true).contains(&MenuAct::Achievements));
        // Desligado ou com o Genius no ar: só Continuar/Ligar/Ejetar/Sair.
        for v in [acts(false, false, true), acts(true, true, true)] {
            assert!(!v.contains(&MenuAct::Save) && !v.contains(&MenuAct::Reset));
            assert!(v.contains(&MenuAct::Power) && v.contains(&MenuAct::Eject));
        }
        // Ejetar ligado desliga primeiro.
        assert_eq!(
            menu_act_events(MenuAct::Eject, true),
            vec![UiEvent::Quit, UiEvent::Eject]
        );
        assert_eq!(menu_act_events(MenuAct::Eject, false), vec![UiEvent::Eject]);
        assert!(menu_act_events(MenuAct::Continue, true).is_empty());
    }

    #[test]
    fn book_pad_buttons_map_to_the_panel_events() {
        use super::book_pad_events;
        use sixteen_platform::padnav::bit;
        use sixteen_platform::UiEvent;
        assert_eq!(book_pad_events(bit::LEFT), vec![UiEvent::NotePrev]);
        assert_eq!(book_pad_events(bit::B), vec![UiEvent::TogglePause]);
        assert_eq!(book_pad_events(bit::A | bit::DOWN), vec![]);
        assert_eq!(
            book_pad_events(bit::X | bit::START),
            vec![UiEvent::NotePinToggle, UiEvent::NoteWriteStart]
        );
    }

    #[test]
    fn sanitize_dir_name_never_escapes_the_parent() {
        use super::sanitize_dir_name;
        assert_eq!(sanitize_dir_name(".."), "___");
        assert_eq!(sanitize_dir_name("."), "___");
        assert_eq!(sanitize_dir_name(" ... "), "___");
        assert_eq!(sanitize_dir_name("../etc"), ".._etc");
        assert_eq!(sanitize_dir_name("Super Game (USA)."), "Super Game (USA)");
        assert!(sanitize_dir_name(&"ã".repeat(300)).len() <= 200);
    }

    use super::{
        add_playtime, cheat_state_path, delete_text_slot, frame_to_rgb8, game_dir,
        legacy_note_text_path, load_cheat_state, migrate_legacy_text_notes, note_dir,
        note_slot_path, note_text_slot_path, read_text_slot, rom_title, save_cheat_state,
        save_note_image, save_text_slot, sram_file, state_file, total_playtime_secs, EmuFrame,
    };
    use sixteen_emulation::PixelFormat as EmuFormat;

    fn scratch_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("sixteen-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn cheat_state_round_trips_through_disk() {
        let dir = scratch_dir("cheats");
        let path = dir.join("test.cheats");

        save_cheat_state(&path, &[true, false, true]);
        assert_eq!(load_cheat_state(&path, 3), vec![true, false, true]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_defaults_everything_off() {
        let path = std::env::temp_dir().join("sixteen-cheat-test-missing.cheats");
        assert_eq!(load_cheat_state(&path, 2), vec![false, false]);
    }

    #[test]
    fn playtime_accumulates_across_sessions() {
        let dir = scratch_dir("playtime");
        assert_eq!(total_playtime_secs(&dir, "Game"), 0);

        add_playtime(&dir, "Game", 90);
        assert_eq!(total_playtime_secs(&dir, "Game"), 90);

        add_playtime(&dir, "Game", 30);
        assert_eq!(total_playtime_secs(&dir, "Game"), 120);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn playtime_zero_seconds_skips_the_write() {
        let dir = scratch_dir("playtime-zero");
        add_playtime(&dir, "Game", 0);
        assert!(!game_dir(&dir, "Game").join("playtime.txt").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rom_title_is_the_file_stem() {
        assert_eq!(
            rom_title(std::path::Path::new("/roms/Super Mario World (USA).sfc")),
            "Super Mario World (USA)"
        );
    }

    #[test]
    fn frame_to_rgb8_decodes_rgb565_bit_layout() {
        // Pure red, green, blue, white — 5-6-5 packed little-endian.
        let px: [u16; 4] = [0xF800, 0x07E0, 0x001F, 0xFFFF];
        let mut pixels = Vec::with_capacity(8);
        for p in px {
            pixels.extend_from_slice(&p.to_le_bytes());
        }
        let frame = EmuFrame {
            width: 4,
            height: 1,
            pitch: 8,
            format: EmuFormat::Rgb565,
            pixels,
        };
        let img = frame_to_rgb8(&frame);
        assert_eq!(img.get_pixel(0, 0).0, [255, 0, 0]);
        assert_eq!(img.get_pixel(1, 0).0, [0, 255, 0]);
        assert_eq!(img.get_pixel(2, 0).0, [0, 0, 255]);
        assert_eq!(img.get_pixel(3, 0).0, [255, 255, 255]);
    }

    #[test]
    fn note_slots_save_independently() {
        let dir = scratch_dir("notes");
        let frame = EmuFrame {
            width: 2,
            height: 2,
            pitch: 4,
            format: EmuFormat::Rgb565,
            pixels: vec![0u8; 4 * 2],
        };

        assert!(!note_slot_path(&dir, "Aladdin", 1).is_file());
        save_note_image(&dir, "Aladdin", 1, &frame).unwrap();
        save_note_image(&dir, "Aladdin", 15, &frame).unwrap();
        assert!(note_dir(&dir, "Aladdin").join("01.png").is_file());
        assert!(note_dir(&dir, "Aladdin").join("15.png").is_file());
        // Slot 2 is untouched — the two saves above didn't spill into it.
        assert!(!note_slot_path(&dir, "Aladdin", 2).is_file());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn text_slot_round_trips_and_deletes() {
        let dir = scratch_dir("notes-text-slot");

        assert_eq!(read_text_slot(&dir, "Aladdin", 1), None);
        save_text_slot(&dir, "Aladdin", 1, "primeira nota").unwrap();
        save_text_slot(&dir, "Aladdin", 15, "outra nota").unwrap();
        assert_eq!(
            read_text_slot(&dir, "Aladdin", 1),
            Some("primeira nota".to_string())
        );
        assert_eq!(
            read_text_slot(&dir, "Aladdin", 15),
            Some("outra nota".to_string())
        );
        // Slot 2 was never written — reading it back is empty, not an error.
        assert_eq!(read_text_slot(&dir, "Aladdin", 2), None);

        // Overwriting a slot replaces its content, doesn't append to it.
        save_text_slot(&dir, "Aladdin", 1, "nota substituida").unwrap();
        assert_eq!(
            read_text_slot(&dir, "Aladdin", 1),
            Some("nota substituida".to_string())
        );

        delete_text_slot(&dir, "Aladdin", 1);
        assert_eq!(read_text_slot(&dir, "Aladdin", 1), None);
        assert!(!note_text_slot_path(&dir, "Aladdin", 1).is_file());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn whitespace_only_text_slot_reads_back_as_empty() {
        let dir = scratch_dir("notes-text-blank");
        save_text_slot(&dir, "Aladdin", 1, "   \n  ").unwrap();
        assert_eq!(read_text_slot(&dir, "Aladdin", 1), None);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn legacy_notas_txt_migrates_into_numbered_slots() {
        let dir = scratch_dir("notes-migrate");
        let legacy_dir = note_dir(&dir, "Aladdin");
        std::fs::create_dir_all(&legacy_dir).unwrap();
        std::fs::write(
            legacy_note_text_path(&dir, "Aladdin"),
            "primeira pagina\n\nsegunda pagina\n\n",
        )
        .unwrap();

        migrate_legacy_text_notes(&dir, "Aladdin");

        assert_eq!(
            read_text_slot(&dir, "Aladdin", 1),
            Some("primeira pagina".to_string())
        );
        assert_eq!(
            read_text_slot(&dir, "Aladdin", 2),
            Some("segunda pagina".to_string())
        );
        // The original file survives the migration untouched.
        assert!(legacy_note_text_path(&dir, "Aladdin").is_file());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn migration_is_a_noop_once_any_text_slot_exists() {
        let dir = scratch_dir("notes-migrate-noop");
        std::fs::create_dir_all(note_dir(&dir, "Aladdin")).unwrap();
        std::fs::write(legacy_note_text_path(&dir, "Aladdin"), "pagina antiga\n\n").unwrap();
        save_text_slot(&dir, "Aladdin", 1, "ja no formato novo").unwrap();

        migrate_legacy_text_notes(&dir, "Aladdin");

        // Migration must not have touched slot 1 (already occupied) or
        // spilled the legacy page into slot 2.
        assert_eq!(
            read_text_slot(&dir, "Aladdin", 1),
            Some("ja no formato novo".to_string())
        );
        assert_eq!(read_text_slot(&dir, "Aladdin", 2), None);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn note_dir_sanitizes_path_hostile_titles() {
        let dir = scratch_dir("notes-sanitize");
        let d = note_dir(&dir, "Foo/Bar: The \"Game\"?");
        // A single direct child of `dir` — no path traversal, no nested
        // directories from the slashes/colons in the title.
        assert_eq!(d.parent(), Some(dir.as_path()));
        assert!(!d.file_name().unwrap().to_string_lossy().contains('/'));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_paths_are_grouped_by_game_with_plain_names() {
        let dir = scratch_dir("saves-layout");
        let title = "Aladdin";

        // All under saves_dir/<title>/, not a flat file per kind at the
        // root — same folder-per-game shape as notes.
        assert_eq!(
            state_file(&dir, title, 3),
            game_dir(&dir, title).join("3.state")
        );
        assert_eq!(
            sram_file(&dir, title),
            game_dir(&dir, title).join("sram.srm")
        );
        assert_eq!(
            cheat_state_path(&dir, title),
            game_dir(&dir, title).join("cheats.txt")
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn game_dir_sanitizes_path_hostile_titles() {
        let dir = scratch_dir("saves-sanitize");
        let d = game_dir(&dir, "Foo/Bar: The \"Game\"?");
        assert_eq!(d.parent(), Some(dir.as_path()));
        assert!(!d.file_name().unwrap().to_string_lossy().contains('/'));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn read_sram_tells_missing_from_unreadable() {
        use crate::runner::{read_sram, sram_aside_path};
        let dir = std::env::temp_dir().join(format!("sixteen-sram-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let srm = dir.join("sram.srm");
        assert!(
            matches!(read_sram(&srm), Ok(None)),
            "ainda não existe = jogo novo"
        );
        std::fs::write(&srm, [1u8, 2, 3]).unwrap();
        assert_eq!(read_sram(&srm).unwrap(), Some(vec![1, 2, 3]));
        // Um diretório no lugar do arquivo: existe e não lê — é erro, nunca
        // "SRAM vazia".
        let as_dir = dir.join("pasta.srm");
        std::fs::create_dir_all(&as_dir).unwrap();
        assert!(read_sram(&as_dir).is_err());
        assert_ne!(sram_aside_path(&srm), srm);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
