//! Platform layer: everything OS- and SDL-specific. Nothing above this line in
//! the architecture knows SDL exists.

mod audio;
mod cabinet;
mod input;

pub use audio::AudioOut;
pub use cabinet::{
    Cabinet, FrameRef, PanelButton, PanelSection, PixelFormat, RaStatus, Screen, SettingsButton,
    SettingsPanelInfo, ShelfButton, ShelfPanelInfo, UpdateArrow, BRAND, DEMO_BADGE_IMG,
    RA_LOGO_IMG,
};
pub use input::{Input, KeyMap, PadButton, PadMap, UiEvent, MAX_PORTS};
// The SDL gamepad button enum, for callers that hold a captured press
// (`MenuInput::captured_pad`) — the app crate doesn't link SDL itself.
pub use sdl3::gamepad::Button as GamepadBtn;

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use sdl3::event::Event;
use sdl3::gamepad::{Button as PadBtn, Gamepad};
use sdl3::joystick::JoystickId;
use sdl3::mouse::MouseButton;
use thiserror::Error;

/// Diagnóstico de engasgo, gateado por `XPERIENCE_TRACE=1` na hora de rodar.
/// Com ele ligado, warns apontam qual fase estourou o budget do frame:
/// `poll_*` (fila de eventos SDL + gamepad), `present` (submissão/vsync de
/// GPU — estourar aqui aponta para compositor/tela, não para o app) e o
/// atraso de frame no `pace_frame` (loop não fechou o budget por qualquer
/// motivo). Sem a variável, o custo é um `OnceLock` lido uma vez.
pub fn trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("XPERIENCE_TRACE").is_some_and(|v| v != "0"))
}

/// Cronometra o escopo onde vive e, com o trace ligado, avisa se passar de
/// ~2 ms. `None` (sem custo) quando desligado.
pub(crate) struct TraceSpan {
    name: &'static str,
    start: Instant,
}

impl TraceSpan {
    pub(crate) fn new(name: &'static str) -> Option<Self> {
        trace_enabled().then_some(Self {
            name,
            start: Instant::now(),
        })
    }
}

impl Drop for TraceSpan {
    fn drop(&mut self) {
        let took = self.start.elapsed();
        if took > Duration::from_millis(2) {
            log::warn!(
                "trace: {} levou {:.1} ms",
                self.name,
                took.as_secs_f64() * 1e3
            );
        }
    }
}

/// Left-button-down position, in window coordinates, or `None` for anything
/// else. Shared between `poll` and `poll_menu` so the SDL event match isn't
/// duplicated.
fn left_click_at(event: &Event) -> Option<(i32, i32)> {
    match event {
        Event::MouseButtonDown {
            mouse_btn: MouseButton::Left,
            x,
            y,
            ..
        } => Some((*x as i32, *y as i32)),
        _ => None,
    }
}

/// Right-button-down position, in window coordinates — the text fields'
/// "colar aqui" gesture, mirroring `left_click_at`.
fn right_click_at(event: &Event) -> Option<(i32, i32)> {
    match event {
        Event::MouseButtonDown {
            mouse_btn: MouseButton::Right,
            x,
            y,
            ..
        } => Some((*x as i32, *y as i32)),
        _ => None,
    }
}

/// What `poll_menu` should do with keydowns this call. Mouse click + gamepad
/// nav (`MENU_PAD_MAP`) drive `Nav` regardless — no keyboard shortcuts left
/// for navigation itself. `CaptureKey` is the one deliberate exception: its
/// whole job is recording a keyboard key to bind for gameplay input, so it's
/// the only mode `poll_menu` still reads keydowns for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuMode {
    /// Grid/list browsing: mouse click, gamepad d-pad/buttons, and the mouse
    /// wheel (mapped to `Up`/`Down`) are the only inputs.
    Nav,
    /// Rebinding a control: the next key pressed OR gamepad button pressed
    /// comes back raw in `captured_key`/`captured_pad`; Escape cancels instead
    /// of being captured.
    CaptureKey,
}

/// A directional / confirm / back intent from the selector's controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuNav {
    Up,
    Down,
    Left,
    Right,
    Confirm,
    Back,
    PageUp,
    PageDown,
    Home,
    End,
}

/// One frame's worth of menu input.
#[derive(Default)]
pub struct MenuInput {
    pub quit: bool,
    pub nav: Vec<MenuNav>,
    /// Set only when `poll_menu` was called with `capture_key: true` and a
    /// key went down this frame: its raw SDL name, for rebinding a control.
    pub captured_key: Option<String>,
    /// Same capture mode, gamepad side: a gamepad button went down this
    /// frame — the settings screen binds whichever arrives first, key or
    /// pad button.
    pub captured_pad: Option<GamepadBtn>,
    /// Set only in capture mode: Escape cancels the capture instead of being
    /// captured as the new binding.
    pub capture_cancelled: bool,
    /// Left click this frame, in window coordinates (see `UiEvent::Click`).
    pub click: Option<(i32, i32)>,
    /// Right click this frame, in window coordinates — the screens use it
    /// only where right-click means something (the text fields' "colar").
    pub right_click: Option<(i32, i32)>,
}

/// One frame's worth of input while writing a free-text note (`Platform::
/// poll_text_entry`) — the pause book's note editor, the one deliberate
/// keyboard-typing exception (plan revision: everything else is
/// mouse/gamepad only).
#[derive(Default)]
pub struct TextEntryInput {
    pub quit: bool,
    /// Composed text typed this frame — usually one character, sometimes
    /// more (IME, paste-like input methods), sometimes empty.
    pub typed: String,
    pub backspace: bool,
    /// Cmd/Ctrl+V this frame: the clipboard's text, sanitized for a
    /// single-line field (control characters dropped), or `None` when no
    /// paste happened / the clipboard couldn't be read.
    pub paste: Option<String>,
    /// Cmd/Ctrl+C this frame — the caller copies the field's whole content
    /// (these drafts have no selection to honor).
    pub copy: bool,
    /// Return/Enter — commit the draft.
    pub commit: bool,
    /// Escape — discard the draft.
    pub cancel: bool,
    /// Left click this frame, in window coordinates.
    pub click: Option<(i32, i32)>,
}

/// Drops control characters from pasted text. Every text field in the app
/// is single-line, and a token copied with a trailing newline (the usual
/// way to get one) would otherwise smuggle it into the draft.
fn sanitize_paste(text: &str) -> String {
    text.chars().filter(|c| !c.is_control()).collect()
}

/// The clipboard's text, sanitized for a single-line field. `None` when the
/// clipboard can't be read or has nothing left worth pasting. Free function
/// because it's called mid-`poll_iter`, with `event_pump` still borrowed.
fn clipboard_paste(video: &sdl3::VideoSubsystem) -> Option<String> {
    let text = video.clipboard().clipboard_text().ok()?;
    let clean = sanitize_paste(&text);
    (!clean.is_empty()).then_some(clean)
}

#[derive(Debug, Error)]
pub enum PlatformError {
    #[error("SDL error: {0}")]
    Sdl(String),
}

impl From<sdl3::Error> for PlatformError {
    fn from(e: sdl3::Error) -> Self {
        PlatformError::Sdl(e.to_string())
    }
}

impl From<String> for PlatformError {
    fn from(e: String) -> Self {
        PlatformError::Sdl(e)
    }
}

/// Owns the SDL context and its subsystems. Create one, keep it alive for the
/// whole process.
pub struct Platform {
    pub sdl: sdl3::Sdl,
    pub video_subsystem: sdl3::VideoSubsystem,
    pub audio_subsystem: sdl3::AudioSubsystem,
    event_pump: sdl3::EventPump,
    gamepad_subsystem: sdl3::GamepadSubsystem,
    /// Open gamepads, in order — index n drives port n. Paired with the
    /// `JoystickId` each was opened from, so a device change only touches the
    /// pads that actually left or arrived (see `refresh_gamepads_if_changed`).
    gamepads: Vec<(JoystickId, Gamepad)>,
    /// The full id list SDL reported on the last gamepad sync — the diff
    /// basis that keeps a connect/disconnect flap from reopening anything.
    seen_ids: Vec<JoystickId>,
    /// A gamepad appeared or left since the last sync; the next `poll*` call
    /// resolves it against `seen_ids` (cheap check, zero cost while idle).
    devices_pending: bool,
    /// Rising-edge tracking for `poll_menu`'s gamepad buttons.
    menu_prev: [bool; MENU_PAD_MAP.len()],
    /// SNES-button layout of the gamepads — the pair table `sample_gamepads`
    /// walks. Defaults to the factory layout; the settings screen swaps pairs
    /// through `set_pad_map` (persisted by the app).
    pad_map: PadMap,
}

const MENU_PAD_MAP: [(PadBtn, MenuNav); 8] = [
    (PadBtn::DPadUp, MenuNav::Up),
    (PadBtn::DPadDown, MenuNav::Down),
    (PadBtn::DPadLeft, MenuNav::Left),
    (PadBtn::DPadRight, MenuNav::Right),
    (PadBtn::South, MenuNav::Confirm),
    (PadBtn::East, MenuNav::Back),
    (PadBtn::LeftShoulder, MenuNav::PageUp),
    (PadBtn::RightShoulder, MenuNav::PageDown),
];

impl Platform {
    pub fn new() -> Result<Self, PlatformError> {
        // Pads clones de "Switch Pro Controller" (p.ex. RetroFlag de SNES,
        // VID/PID 057e:2009) fazem o driver hidapi do SDL reivindicá-los com
        // handshake e um watchdog que, sem pacote de input por 100 ms, envia
        // um comando ForceUSB SÍNCRONO na thread principal
        // (SDL_hidapi_switch.c, UpdateDevice) — cada write trava ~1 s até o
        // timeout do USB, e o resultado é a interface engasgando em ondas
        // enquanto o pad estiver plugado. No backend nativo (GCController/
        // IOKit) o mesmo pad é atendido sem nenhum write síncrono; para SNES
        // não perdemos nada que importe (não usamos rumble nem giro).
        sdl3::hint::set("SDL_JOYSTICK_HIDAPI_SWITCH", "0");
        // GCController (o "MFI" da Apple) também engole clones 057e:2009 — e
        // fica esperando o protocolo Switch que eles não falam: o pad abre e
        // nenhum botão chega. Sem MFI, o pad cai no backend IOKit bruto, que
        // lê os reports de joystick genérico que ele manda de verdade (o
        // mesmo caminho que o kernel HID alimenta no Raspberry).
        sdl3::hint::set("SDL_JOYSTICK_MFI", "0");
        // Fullscreen sem os Spaces do macOS: a transição nativa de zoom lia
        // como "abre de um tamanho e dá uma aumentada" — com Spaces fora, a
        // janela cobre a tela na hora, sem animação. Vale para janelas
        // criadas depois do hint (o init abaixo).
        sdl3::hint::set("SDL_VIDEO_MAC_FULLSCREEN_SPACES", "0");
        let sdl = sdl3::init()?;
        let video_subsystem = sdl.video()?;
        let audio_subsystem = sdl.audio()?;
        let gamepad_subsystem = sdl.gamepad()?;
        let event_pump = sdl.event_pump()?;
        let mut me = Self {
            sdl,
            video_subsystem,
            audio_subsystem,
            event_pump,
            gamepad_subsystem,
            gamepads: Vec::new(),
            seen_ids: Vec::new(),
            devices_pending: false,
            menu_prev: [false; MENU_PAD_MAP.len()],
            pad_map: PadMap::defaults(),
        };
        me.sync_gamepads();
        Ok(me)
    }

    /// (Re)open the connected gamepads (up to `MAX_PORTS`), keeping every pad
    /// that's already open: `SDL_OpenGamepad` runs on the main thread (IOKit/
    /// GCController binding, easily tens of ms), so a USB controller whose
    /// connection flaps must not reopen the *stable* pads — each reopen lands
    /// right in the UI's frame budget and reads as stutter. Ports follow SDL's
    /// enumeration order, same as before.
    fn sync_gamepads(&mut self) {
        let Ok(ids) = self.gamepad_subsystem.gamepads() else {
            return;
        };
        self.seen_ids = ids.clone();
        self.seen_ids.sort_unstable();
        let mut leftovers = std::mem::take(&mut self.gamepads);
        let mut gamepads = Vec::with_capacity(leftovers.len());
        for id in ids.into_iter().take(input::MAX_PORTS) {
            if let Some(pos) = leftovers.iter().position(|(pid, _)| *pid == id) {
                gamepads.push(leftovers.swap_remove(pos));
            } else if let Ok(pad) = self.gamepad_subsystem.open(id) {
                log::info!(
                    "gamepad port {}: {}",
                    gamepads.len(),
                    pad.name().unwrap_or_default()
                );
                gamepads.push((id, pad));
            }
        }
        // Whatever `leftovers` still holds was unplugged; dropping it closes
        // the pads.
        self.gamepads = gamepads;
    }

    /// Resolve a pending hotplug: only when the connected-id set actually
    /// differs from the last sync does anything reopen. A flap that comes and
    /// goes between two frames (or removes and re-adds before the next poll
    /// sees both events) collapses into a no-op; a real change syncs once.
    fn refresh_gamepads_if_changed(&mut self) {
        if !self.devices_pending {
            return;
        }
        self.devices_pending = false;
        let Ok(ids) = self.gamepad_subsystem.gamepads() else {
            return;
        };
        let mut now = ids;
        now.sort_unstable();
        if now != self.seen_ids {
            self.sync_gamepads();
        }
    }

    /// The one window: a dark cabinet with the screen recessed into it. Both the
    /// game and the selector draw into that screen area; nothing recreates it.
    pub fn create_cabinet(
        &self,
        title: &str,
        width: u32,
        height: u32,
        fullscreen: bool,
    ) -> Result<Cabinet, PlatformError> {
        Cabinet::new(
            &self.video_subsystem,
            &self.audio_subsystem,
            title,
            width,
            height,
            fullscreen,
        )
    }

    /// Testing hook (example harnesses): push a synthetic left-button
    /// down+up at window coordinates into the event queue, indistinguishable
    /// from a real click to `poll`/`poll_menu`.
    pub fn push_synthetic_click(&self, x: i32, y: i32) {
        for down in [true, false] {
            let ev = if down {
                Event::MouseButtonDown {
                    timestamp: 0,
                    window_id: 0,
                    which: 0,
                    mouse_btn: MouseButton::Left,
                    clicks: 1,
                    x: x as f32,
                    y: y as f32,
                }
            } else {
                Event::MouseButtonUp {
                    timestamp: 0,
                    window_id: 0,
                    which: 0,
                    mouse_btn: MouseButton::Left,
                    clicks: 1,
                    x: x as f32,
                    y: y as f32,
                }
            };
            let _ = self.sdl.event().expect("event subsystem").push_event(ev);
        }
    }

    /// Drain events for a menu screen: mouse click, mouse wheel (-> `Up`/
    /// `Down`), and gamepad d-pad/buttons (rising edge only, `MENU_PAD_MAP`)
    /// always feed `nav`. In `CaptureKey` mode only, a keydown is captured
    /// raw instead — see [`MenuMode`].
    pub fn poll_menu(&mut self, mode: MenuMode) -> MenuInput {
        use sdl3::keyboard::Keycode;
        let _span = TraceSpan::new("poll_menu");
        let mut out = MenuInput::default();
        for event in self.event_pump.poll_iter() {
            if let Some(pos) = left_click_at(&event) {
                out.click = Some(pos);
                continue;
            }
            if let Some(pos) = right_click_at(&event) {
                out.right_click = Some(pos);
                continue;
            }
            match event {
                Event::Quit { .. } => out.quit = true,
                Event::GamepadAdded { .. } | Event::GamepadRemoved { .. } => {
                    self.devices_pending = true
                }
                Event::MouseWheel { y, .. } => {
                    if y > 0.0 {
                        out.nav.push(MenuNav::Up);
                    } else if y < 0.0 {
                        out.nav.push(MenuNav::Down);
                    }
                }
                Event::KeyDown {
                    keycode: Some(k),
                    repeat,
                    ..
                } if mode == MenuMode::CaptureKey => {
                    if repeat {
                        continue;
                    }
                    if k == Keycode::Escape {
                        out.capture_cancelled = true;
                    } else if out.captured_key.is_none() {
                        out.captured_key = Some(k.name());
                    }
                }
                Event::GamepadButtonDown { button, .. } if mode == MenuMode::CaptureKey => {
                    out.captured_pad = Some(button);
                }
                _ => {}
            }
        }
        self.refresh_gamepads_if_changed();
        // Gamepad: rising edges only.
        let pad = self.gamepads.first().map(|(_, pad)| pad);
        for (i, (btn, nav)) in MENU_PAD_MAP.iter().enumerate() {
            let down = pad.map(|p| p.button(*btn)).unwrap_or(false);
            if down && !self.menu_prev[i] {
                out.nav.push(*nav);
            }
            self.menu_prev[i] = down;
        }
        out
    }

    /// Turn on OS text composition (IME, dead keys, the works) for `cab`'s
    /// window — call before the first `poll_text_entry` of a writing session
    /// (plan revision: the pause book's free-text note is the one deliberate
    /// keyboard-typing exception to "mouse/gamepad only").
    pub fn start_text_input(&self, cab: &Cabinet) {
        self.video_subsystem.text_input().start(cab.window());
    }

    /// Turn text composition back off — call once the draft is saved or
    /// cancelled, so a stray keypress elsewhere doesn't get eaten as text.
    pub fn stop_text_input(&self, cab: &Cabinet) {
        self.video_subsystem.text_input().stop(cab.window());
    }

    /// Drain events while writing free text: composed text (`Event::
    /// TextInput`, which handles layout/IME properly — no hand-rolled
    /// shift/keycode mapping), Backspace, Return (commit), Escape (cancel),
    /// a click (to hit "Salvar"/"Cancelar" or click away) and the field
    /// copy/paste shortcuts (⌘C/⌘V on macOS, Ctrl elsewhere — the one
    /// thing TextInput events don't carry, and what makes the RA token
    /// pasteable straight from the browser). Nothing else is read —
    /// gameplay input stays untouched while a note is open.
    pub fn poll_text_entry(&mut self) -> TextEntryInput {
        use sdl3::keyboard::{Keycode, Mod};
        let _span = TraceSpan::new("poll_text_entry");
        let mut out = TextEntryInput::default();
        for event in self.event_pump.poll_iter() {
            if let Some(pos) = left_click_at(&event) {
                out.click = Some(pos);
                continue;
            }
            // Botão direito enquanto digita: colar — o mesmo `paste` do
            // atalho de teclado, sem tirar a mão do mouse.
            if matches!(
                event,
                Event::MouseButtonDown {
                    mouse_btn: MouseButton::Right,
                    ..
                }
            ) {
                out.paste = clipboard_paste(&self.video_subsystem);
                continue;
            }
            match event {
                Event::Quit { .. } => out.quit = true,
                Event::TextInput { text, .. } => out.typed.push_str(&text),
                // Hotplug durante a digitação não é lido aqui, mas o evento
                // seria engolido (e o controle ficaria morto até o próximo
                // flap) — marca para o sync resolver antes do próximo frame.
                Event::GamepadAdded { .. } | Event::GamepadRemoved { .. } => {
                    self.devices_pending = true
                }
                Event::KeyDown {
                    keycode: Some(k),
                    keymod,
                    repeat: false,
                    ..
                } => {
                    let shortcut = keymod
                        .intersects(Mod::LCTRLMOD | Mod::RCTRLMOD | Mod::LGUIMOD | Mod::RGUIMOD);
                    if shortcut {
                        match k {
                            Keycode::C => out.copy = true,
                            Keycode::V => out.paste = clipboard_paste(&self.video_subsystem),
                            _ => {}
                        }
                    }
                    match k {
                        Keycode::Backspace => out.backspace = true,
                        Keycode::Return | Keycode::KpEnter => out.commit = true,
                        Keycode::Escape => out.cancel = true,
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        self.refresh_gamepads_if_changed();
        out
    }

    /// Copy `text` to the OS clipboard — the text fields' Cmd/Ctrl+C. The
    /// drafts have no selection, so it's always the whole field. `false`
    /// when the OS refuses; there's nothing better to do than ignore it.
    pub fn set_clipboard_text(&self, text: &str) -> bool {
        self.video_subsystem
            .clipboard()
            .set_clipboard_text(text)
            .is_ok()
    }

    /// The clipboard's text, sanitized for a single-line field — what a
    /// text field pastes (right-clicking a closed field opens it already
    /// pasting). `None` when the clipboard can't be read or has nothing
    /// left worth pasting.
    pub fn paste_from_clipboard(&self) -> Option<String> {
        clipboard_paste(&self.video_subsystem)
    }

    pub fn open_audio(&self, sample_rate: u32) -> Result<AudioOut, PlatformError> {
        AudioOut::new(&self.audio_subsystem, sample_rate)
    }

    pub fn new_input(&self) -> Input {
        Input::new()
    }

    /// Drain the event queue, update `input` via `keymap` (gameplay D-pad/
    /// buttons only — every console/UI command is mouse-only now, reported
    /// as [`UiEvent::Click`] for the caller to resolve via
    /// `Cabinet::hit_panel_button`), and return the click plus an OS close
    /// request ([`UiEvent::CloseRequested`]), if either happened this frame.
    pub fn poll(&mut self, input: &mut Input, keymap: &KeyMap) -> Vec<UiEvent> {
        let _span = TraceSpan::new("poll");
        let mut out = Vec::new();
        for event in self.event_pump.poll_iter() {
            if let Some((x, y)) = left_click_at(&event) {
                out.push(UiEvent::Click(x, y));
                continue;
            }
            match event {
                Event::Quit { .. } => out.push(UiEvent::CloseRequested),
                Event::GamepadAdded { .. } | Event::GamepadRemoved { .. } => {
                    self.devices_pending = true
                }
                Event::KeyDown {
                    keycode: Some(k),
                    repeat: false,
                    ..
                } => {
                    if let Some(b) = keymap.pad_for(k) {
                        input.set_key(b, true);
                    }
                }
                Event::KeyUp {
                    keycode: Some(k), ..
                } => {
                    if let Some(b) = keymap.pad_for(k) {
                        input.set_key(b, false);
                    }
                }
                _ => {}
            }
        }
        self.refresh_gamepads_if_changed();
        self.sample_gamepads(input);
        out
    }

    /// Swap the gamepad SNES-button layout (settings screen); takes effect on
    /// the next `poll`.
    pub fn set_pad_map(&mut self, map: PadMap) {
        self.pad_map = map;
    }

    fn sample_gamepads(&self, input: &mut Input) {
        input.clear_pads();
        for (port, (_, pad)) in self.gamepads.iter().enumerate() {
            for (btn, mapped) in self.pad_map.pairs() {
                if pad.button(*btn) {
                    input.set_pad(port, *mapped, true);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sanitize_paste;

    #[test]
    fn sanitize_paste_drops_control_chars_and_keeps_the_rest() {
        assert_eq!(sanitize_paste("abc123-_."), "abc123-_.");
        // A token copied from the browser usually comes with a newline.
        assert_eq!(sanitize_paste("tok3n\n"), "tok3n");
        assert_eq!(sanitize_paste("li\nne\rtab\there"), "linetabhere");
        assert_eq!(sanitize_paste("são çedilha ✨"), "são çedilha ✨");
        assert_eq!(sanitize_paste("\n\r\t"), "");
    }
}
