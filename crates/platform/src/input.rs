//! Keyboard + gamepad, folded into one SNES-shaped button matrix.
//!
//! Gamepad navigation is the primary path (plan §3.1); the keyboard binds come
//! from a [`KeyMap`] the app builds (defaults plus a config file).

use std::str::FromStr;

use sdl3::gamepad::Button as PadBtn;
use sdl3::keyboard::Keycode;

/// SNES / libretro joypad buttons, in libretro id order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub enum PadButton {
    B = 0,
    Y = 1,
    Select = 2,
    Start = 3,
    Up = 4,
    Down = 5,
    Left = 6,
    Right = 7,
    A = 8,
    X = 9,
    L = 10,
    R = 11,
}

impl PadButton {
    pub const ALL: [PadButton; 12] = [
        PadButton::B,
        PadButton::Y,
        PadButton::Select,
        PadButton::Start,
        PadButton::Up,
        PadButton::Down,
        PadButton::Left,
        PadButton::Right,
        PadButton::A,
        PadButton::X,
        PadButton::L,
        PadButton::R,
    ];

    /// Config token, e.g. `"start"`, `"l"`.
    pub fn token(self) -> &'static str {
        match self {
            PadButton::B => "b",
            PadButton::Y => "y",
            PadButton::Select => "select",
            PadButton::Start => "start",
            PadButton::Up => "up",
            PadButton::Down => "down",
            PadButton::Left => "left",
            PadButton::Right => "right",
            PadButton::A => "a",
            PadButton::X => "x",
            PadButton::L => "l",
            PadButton::R => "r",
        }
    }
}

impl FromStr for PadButton {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        PadButton::ALL
            .into_iter()
            .find(|b| b.token() == s)
            .ok_or(())
    }
}

/// Controller ports. The SNES has more with a multitap; two covers the plan.
pub const MAX_PORTS: usize = 2;

/// High-level events the app acts on. Meaning (e.g. "power off") is decided a
/// layer up; the platform only reports the intent. None of these are
/// keyboard-bindable any more (plan revision: mouse/gamepad only for every
/// console/UI command) — they're only ever produced by a panel-button
/// `Click` being resolved by the caller, or (`CloseRequested`) the OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiEvent {
    /// Power toggle — off saves/signals-off (console dark, cartridge still in
    /// the slot, plan §3.3), on again resumes exactly where it was. Always
    /// mouse-driven now (the panel's Power button); a click while already
    /// mid-transition does nothing new.
    Quit,
    /// The OS asked the window to close (red button, Cmd-Q, `SIGTERM`). Always
    /// means "tear the whole app down", never "go back".
    CloseRequested,
    /// **Ejetar** — only takes effect once the console is off; while it's
    /// still on the lock resists (plan §3.3).
    Eject,
    Reset,
    /// Also what the panel's "Anotacoes" button fires (plan revision — it
    /// replaced "Pausar": clicking it pauses *and* lands straight on the
    /// notebook, one action instead of two).
    TogglePause,
    /// Open the save-state slot picker — a modal, not an immediate save
    /// (plan revision). The game freezes while it's open, same as the
    /// notebook; picking a slot (`ModalPick`) is what actually saves.
    OpenSaveModal,
    /// Open the load-state slot picker, mirroring `OpenSaveModal`.
    OpenLoadModal,
    /// Open the "which slot for this print" picker (plan revision — replaces
    /// the old direct-capture "Nota" button): the frame to save is grabbed
    /// the moment this fires, so whichever slot the player ends up picking
    /// (after the game has frozen) gets the frame from the instant they
    /// clicked, not whatever's on screen once they're done choosing.
    OpenPrintModal,
    /// A slot row clicked inside whichever modal (`OpenSaveModal`/
    /// `OpenLoadModal`/`OpenPrintModal`) is open right now — only reachable
    /// during the row-picking step; the naming step that can follow
    /// (`OpenPrintModal`) is polled via `Platform::poll_text_entry` instead,
    /// same as the notebook's own text editor, so it never produces a
    /// `Click`/`UiEvent` at all (see `PanelButton::ModalConfirm`).
    ModalPick(u16),
    /// Back out of whichever modal is open, discarding any choice so far —
    /// only reachable during the row-picking step, same caveat as
    /// `ModalPick`.
    ModalCancel,
    /// Scroll whichever modal's row grid is open one page up/down — only
    /// reachable (and only drawn at all) when there are more rows than fit
    /// in the card at once (a small window).
    ModalScrollUp,
    ModalScrollDown,
    OpenAchievementsModal,
    /// Pause book: step the right page (the photo album) to an earlier/
    /// later print slot.
    NotePrev,
    NoteNext,
    /// Pause book: open the editor for the left page's currently-shown
    /// text-note slot (plan revision — repurposed from "append a new,
    /// always-blank free-text page": now targets whichever of the 15 slots
    /// `TextPrev`/`TextNext` left showing, pre-filled with what's already
    /// there so editing doesn't start by erasing it) — the one deliberate
    /// keyboard-typing exception, see `Platform::poll_text_entry`.
    NoteWriteStart,
    /// Pause book: toggle whether the right page's shown print slot
    /// resists a future "Printscreen" overwrite (plan revision).
    NotePinToggle,
    /// Pause book: open the editor for the shown print slot's caption
    /// (plan revision) — same keyboard-typing exception as `NoteWriteStart`.
    NoteNameStart,
    /// Pause book: step the left page (the 15 text-note slots, plan
    /// revision) to an earlier/later one — independent of `NotePrev`/
    /// `NoteNext`, which move the *right* page's print slot instead.
    TextPrev,
    TextNext,
    /// Pause book: toggle whether the left page's shown text-note slot
    /// resists a future overwrite *or delete* (plan revision — pin means
    /// "protected" the same way it already does for a print slot).
    TextPinToggle,
    /// Pause book: clear the left page's shown text-note slot — refused
    /// while it's pinned, same protection `TextPinToggle` grants a print
    /// slot against being overwritten.
    TextDelete,
    /// Left mouse button went down, in **window** coordinates — the caller
    /// (which owns the `Cabinet`) converts to output/canvas space via
    /// `Cabinet::window_to_output` before hit-testing anything.
    Click(i32, i32),
}

/// Keyboard bindings for actual SNES gameplay input (D-pad/face buttons) —
/// the one thing that's still keyboard by default, since a gamepad isn't
/// guaranteed to be plugged in. Built from [`KeyMap::default`] then
/// overridden per the app's config. Gamepad bindings stay fixed (SDL's
/// controller DB already normalises devices). Every console/UI command
/// (eject, reset, pause, save state, ...) used to have a rebindable key here
/// too; it doesn't any more — those are mouse/gamepad-menu only (plan
/// revision), so there's nothing left to bind for them.
#[derive(Debug, Clone, Default)]
pub struct KeyMap {
    pad: Vec<(Keycode, PadButton)>,
}

impl KeyMap {
    /// The built-in layout (arrows = dpad, Z/X = B/A, …).
    pub fn defaults() -> Self {
        let pad = [
            ("Up", PadButton::Up),
            ("Down", PadButton::Down),
            ("Left", PadButton::Left),
            ("Right", PadButton::Right),
            ("Z", PadButton::B),
            ("X", PadButton::A),
            ("A", PadButton::Y),
            ("S", PadButton::X),
            ("Q", PadButton::L),
            ("W", PadButton::R),
            ("Return", PadButton::Start),
            ("Right Shift", PadButton::Select),
        ];
        let mut m = KeyMap::default();
        for (name, b) in pad {
            m.bind_pad(name, b).expect("valid default key name");
        }
        m
    }

    /// Parse an SDL key name (`"Left Shift"`, `"F2"`, `"]"`). Case-sensitive as
    /// SDL defines it.
    pub fn parse_key(name: &str) -> Option<Keycode> {
        Keycode::from_name(name)
    }

    /// Rebind a pad button. One key per action and one action per key: if
    /// the key already drove another action, the two **swap** — that action
    /// takes over the key `button` had — so nothing is ever left unbound by a
    /// rebind. Returns the action that moved, if any.
    pub fn bind_pad(
        &mut self,
        key_name: &str,
        button: PadButton,
    ) -> Result<Option<PadButton>, String> {
        let k = Self::parse_key(key_name).ok_or_else(|| format!("unknown key {key_name:?}"))?;
        Ok(swap_bind(&mut self.pad, k, button))
    }

    /// `(action token, SDL key name)` for every SNES button, in libretro
    /// order — an unbound action comes back with an empty name.
    pub fn describe(&self) -> Vec<(String, String)> {
        PadButton::ALL
            .iter()
            .map(|&b| {
                let name = self.key_for(b).map(|k| k.name()).unwrap_or_default();
                (b.token().to_string(), name)
            })
            .collect()
    }

    /// The key that drives `b`, if any.
    pub fn key_for(&self, b: PadButton) -> Option<Keycode> {
        self.pad.iter().find(|&&(_, bb)| bb == b).map(|&(k, _)| k)
    }

    pub(crate) fn pairs(&self) -> &[(Keycode, PadButton)] {
        &self.pad
    }

    pub(crate) fn pad_for(&self, k: Keycode) -> Option<PadButton> {
        self.pad.iter().find(|&&(kk, _)| kk == k).map(|&(_, b)| b)
    }
}

/// Gamepad-button layout: which SDL gamepad button drives each SNES button —
/// the gamepad counterpart of [`KeyMap`]. SDL's controller DB normalizes the
/// button *names* across devices, but clones can still land a press in the
/// wrong slot — so the layout is rebindable in the settings screen, same as
/// the keyboard's, and persists in the config's `[gamepad]` section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PadMap {
    pad: Vec<(PadBtn, PadButton)>,
}

impl PadMap {
    /// The built-in layout (d-pad, B/A/Y/X on South/East/West/North, L/R on
    /// the shoulders, select/start on back/start).
    pub fn defaults() -> Self {
        let mut m = PadMap::default();
        for (btn, b) in DEFAULT_PAD_MAP {
            m.pad.push((btn, b));
        }
        m
    }

    /// Rebind a SNES button to an SDL gamepad-button name (`"south"`,
    /// `"dpad_up"`, …). One button per action and one action per button —
    /// a taken button swaps, like [`KeyMap::bind_pad`]. Returns the action
    /// that moved, if any.
    pub fn bind(&mut self, button_name: &str, b: PadButton) -> Result<Option<PadButton>, String> {
        let Some(btn) = PadBtn::from_string(button_name) else {
            return Err(format!("unknown gamepad button {button_name:?}"));
        };
        Ok(swap_bind(&mut self.pad, btn, b))
    }

    /// The gamepad button that drives `b`, if any.
    pub fn button_for(&self, b: PadButton) -> Option<PadBtn> {
        self.pad.iter().find(|&&(_, pb)| pb == b).map(|&(bb, _)| bb)
    }

    /// `(action token, SDL gamepad button name)` for every SNES button, in
    /// libretro order — the settings rows' and the config file's source.
    /// An unbound action comes back with an empty name.
    pub fn describe(&self) -> Vec<(String, String)> {
        PadButton::ALL
            .iter()
            .map(|&b| {
                let name = self
                    .pad
                    .iter()
                    .find(|&&(_, pb)| pb == b)
                    .map(|&(bb, _)| bb.string())
                    .unwrap_or_default();
                (b.token().to_string(), name)
            })
            .collect()
    }

    pub(crate) fn pairs(&self) -> &[(PadBtn, PadButton)] {
        &self.pad
    }
}

/// Binds `input` to `b` in a one-to-one pair table. If `input` already drove
/// another action, that action takes `b`'s previous input (or is left
/// unbound when `b` had none). Returns the action that moved.
fn swap_bind<T: Copy + PartialEq>(
    pairs: &mut Vec<(T, PadButton)>,
    input: T,
    b: PadButton,
) -> Option<PadButton> {
    let previous = pairs.iter().find(|&&(_, pb)| pb == b).map(|&(i, _)| i);
    if previous == Some(input) {
        return None;
    }
    pairs.retain(|&(_, pb)| pb != b);
    let mut moved = None;
    if let Some(pos) = pairs.iter().position(|&(i, _)| i == input) {
        let other = pairs[pos].1;
        moved = Some(other);
        match previous {
            Some(prev) => pairs[pos] = (prev, other),
            None => {
                pairs.remove(pos);
            }
        }
    }
    pairs.push((input, b));
    moved
}

/// The factory layout every X-input-style pad ships with.
const DEFAULT_PAD_MAP: [(PadBtn, PadButton); 12] = [
    (PadBtn::DPadUp, PadButton::Up),
    (PadBtn::DPadDown, PadButton::Down),
    (PadBtn::DPadLeft, PadButton::Left),
    (PadBtn::DPadRight, PadButton::Right),
    (PadBtn::South, PadButton::B),
    (PadBtn::East, PadButton::A),
    (PadBtn::West, PadButton::Y),
    (PadBtn::North, PadButton::X),
    (PadBtn::LeftShoulder, PadButton::L),
    (PadBtn::RightShoulder, PadButton::R),
    (PadBtn::Back, PadButton::Select),
    (PadBtn::Start, PadButton::Start),
];

/// Per-frame button state. Filled by `Platform::poll`. Keyboard drives port 0;
/// gamepad *n* drives port *n*.
#[derive(Default)]
pub struct Input {
    keys: [bool; 12],
    pads: [[bool; 12]; MAX_PORTS],
}

impl Input {
    pub fn new() -> Self {
        Self::default()
    }

    /// Button `b` held on `port` this frame (keyboard counts for port 0).
    pub fn held(&self, port: usize, b: PadButton) -> bool {
        let i = b as usize;
        (port == 0 && self.keys[i]) || (port < MAX_PORTS && self.pads[port][i])
    }

    pub(crate) fn set_key(&mut self, b: PadButton, down: bool) {
        self.keys[b as usize] = down;
    }

    pub(crate) fn set_pad(&mut self, port: usize, b: PadButton, down: bool) {
        if port < MAX_PORTS {
            self.pads[port][b as usize] = down;
        }
    }

    pub(crate) fn clear_pads(&mut self) {
        self.pads = [[false; 12]; MAX_PORTS];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taking_a_bound_key_swaps_instead_of_unbinding() {
        let mut m = KeyMap::defaults();
        // X drives A; give it to B (which had Z): A must end up on Z.
        let moved = m.bind_pad("X", PadButton::B).unwrap();
        assert_eq!(moved, Some(PadButton::A));
        assert_eq!(m.key_for(PadButton::B).unwrap().name(), "X");
        assert_eq!(m.key_for(PadButton::A).unwrap().name(), "Z");
        // Every action still listed, in a stable order.
        let d = m.describe();
        assert_eq!(d.len(), 12);
        assert!(d.iter().all(|(_, k)| !k.is_empty()));
    }

    #[test]
    fn rebinding_to_the_same_key_is_a_no_op() {
        let mut m = KeyMap::defaults();
        assert_eq!(m.bind_pad("Z", PadButton::B).unwrap(), None);
        assert_eq!(m.key_for(PadButton::B).unwrap().name(), "Z");
    }

    #[test]
    fn pad_buttons_swap_too() {
        let mut m = PadMap::defaults();
        let moved = m.bind(&PadBtn::East.string(), PadButton::B).unwrap();
        assert_eq!(moved, Some(PadButton::A));
        assert_eq!(m.button_for(PadButton::B), Some(PadBtn::East));
        assert_eq!(m.button_for(PadButton::A), Some(PadBtn::South));
    }
}
