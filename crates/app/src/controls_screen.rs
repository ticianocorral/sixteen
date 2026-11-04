//! A seção "controles" das configurações: o controle de Super Nintendo
//! desenhado na TV, a lista dos 12 botões para o aparelho escolhido
//! (teclado, controle 1 ou controle 2 — cada um com o seu mapa), o
//! "configurar tudo" em sequência, o teste de botões ao vivo e o restaurar
//! padrão.
//!
//! Trocar um botão que já era de outra ação **troca** as duas (ninguém fica
//! sem botão) e avisa embaixo do desenho.

use std::time::{Duration, Instant};

#[allow(unused_imports)]
use sixteen_platform::tr;
use sixteen_platform::{
    GamepadBtn, GamepadInfo, KeyMap, MenuInput, PadButton, PadFamily, PadMap, Platform, Screen,
    MAX_PORTS,
};

use crate::config::Config;

const TEXT: (u8, u8, u8) = (232, 232, 232);
const DIM: (u8, u8, u8) = (150, 150, 158);
const ACCENT: (u8, u8, u8) = (255, 214, 90);
const WARN: (u8, u8, u8) = (240, 150, 110);

/// Altura de uma linha desta seção — menor que a das outras: são 16 linhas.
pub(crate) const ROW_H: i32 = 24;

/// A ordem em que os botões aparecem (a de quem olha o controle, não a do
/// libretro).
const ORDER: [PadButton; 12] = [
    PadButton::Up,
    PadButton::Down,
    PadButton::Left,
    PadButton::Right,
    PadButton::A,
    PadButton::B,
    PadButton::X,
    PadButton::Y,
    PadButton::L,
    PadButton::R,
    PadButton::Start,
    PadButton::Select,
];

const ROW_DEVICE: usize = 0;
const ROW_FIRST_BUTTON: usize = 1;
const ROW_WIZARD: usize = ROW_FIRST_BUTTON + ORDER.len();
const ROW_TEST: usize = ROW_WIZARD + 1;
const ROW_RESET: usize = ROW_TEST + 1;
pub(crate) const ROWS: usize = ROW_RESET + 1;

/// Quanto tempo o aviso embaixo do controle fica na tela.
const NOTICE_FOR: Duration = Duration::from_secs(4);

/// Segurar B (East) por este tempo cancela a captura — quem só tem controle
/// não tem Esc.
const HOLD_CANCEL: Duration = Duration::from_millis(1000);
/// Sem nenhum botão em tanto tempo, a captura se cancela sozinha.
const CAPTURE_TIMEOUT: Duration = Duration::from_secs(5);

/// O que o B do controle fez durante a captura (lógica pura, testada): como
/// B também é um botão do SNES que se pode querer gravar, ele só vale ao
/// SOLTAR — rápido grava; segurado até `HOLD_CANCEL` cancela.
#[derive(Debug, Default)]
struct BHold {
    since: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BHoldAct {
    /// Nada a fazer (B não está sendo segurado).
    None,
    /// Segurando, ainda sem chegar ao fim.
    Holding,
    /// Soltou cedo: grava o B.
    Tap,
    /// Segurou o bastante: cancela a captura.
    Cancel,
}

impl BHold {
    /// `pressed`: B desceu neste quadro; `held`: B está para baixo agora.
    fn step(&mut self, pressed: bool, held: bool, now: Instant) -> BHoldAct {
        match self.since {
            None => {
                if pressed {
                    self.since = Some(now);
                    BHoldAct::Holding
                } else {
                    BHoldAct::None
                }
            }
            Some(t) => {
                if !held {
                    self.since = None;
                    BHoldAct::Tap
                } else if now.duration_since(t) >= HOLD_CANCEL {
                    self.since = None;
                    BHoldAct::Cancel
                } else {
                    BHoldAct::Holding
                }
            }
        }
    }

    /// 0.0..=1.0 do tempo segurado, para a barrinha.
    fn progress(&self, now: Instant) -> Option<f32> {
        self.since
            .map(|t| (now.duration_since(t).as_secs_f32() / HOLD_CANCEL.as_secs_f32()).min(1.0))
    }
}

/// O aparelho sendo mapeado: 0 = teclado, 1.. = controle `n`.
const DEVICES: usize = 1 + MAX_PORTS;

pub(crate) struct ControlsScreen {
    pub sel: usize,
    device: usize,
    /// Esperando o botão para esta ação.
    awaiting: Option<PadButton>,
    /// O "configurar tudo": ao gravar um botão, segue para o próximo.
    wizard: bool,
    /// Testando: todo botão só acende no desenho; nada navega.
    testing: bool,
    notice: Option<(String, Instant)>,
    /// O primeiro toque em "restaurar padrão" só arma; o segundo restaura.
    reset_armed: Option<Instant>,
    born: Instant,
    /// B segurado para cancelar a captura (controle).
    b_hold: BHold,
    /// Desde quando a captura espera um botão (o timeout de 5 s).
    awaiting_since: Option<Instant>,
}

impl ControlsScreen {
    pub fn new() -> Self {
        Self {
            sel: 0,
            device: 0,
            awaiting: None,
            wizard: false,
            testing: false,
            notice: None,
            reset_armed: None,
            born: Instant::now(),
            b_hold: BHold::default(),
            awaiting_since: None,
        }
    }

    /// Capturando (um botão a gravar ou o teste): a tela pede ao `poll_menu`
    /// as teclas cruas e ignora a navegação.
    pub fn capturing(&self) -> bool {
        self.awaiting.is_some() || self.testing
    }

    /// Larga qualquer captura (ao sair da seção).
    pub fn cancel(&mut self) {
        self.awaiting = None;
        self.awaiting_since = None;
        self.b_hold = BHold::default();
        self.wizard = false;
        self.testing = false;
    }

    fn say(&mut self, s: impl Into<String>) {
        self.notice = Some((s.into(), Instant::now()));
    }

    /// Esquerda/direita: só a linha do aparelho tem o que ajustar.
    pub fn adjust(&mut self, right: bool) {
        if self.sel == ROW_DEVICE {
            self.device = if right {
                (self.device + 1) % DEVICES
            } else {
                (self.device + DEVICES - 1) % DEVICES
            };
            self.reset_armed = None;
        }
    }

    /// Confirmar (ou clicar) na linha `sel`.
    pub fn activate(&mut self, plat: &mut Platform, cfg: &mut Config) {
        let slots = plat.gamepad_slots();
        match self.sel {
            ROW_DEVICE => self.adjust(true),
            ROW_TEST => {
                self.testing = true;
            }
            ROW_RESET => {
                if self
                    .reset_armed
                    .is_some_and(|t| t.elapsed() < Duration::from_secs(3))
                {
                    self.reset_armed = None;
                    match self.device {
                        0 => cfg.keymap = KeyMap::defaults(),
                        n => {
                            *pad_map_mut(cfg, n) = PadMap::defaults();
                            plat.set_pad_map(n - 1, pad_map(cfg, n).clone());
                        }
                    }
                    self.say(tr!("defaults restored", "padrão restaurado"));
                } else {
                    self.reset_armed = Some(Instant::now());
                    self.say(tr!(
                        "press again to restore the defaults",
                        "aperte de novo para restaurar o padrão"
                    ));
                }
            }
            row => {
                if self.device > 0 && slots.len() < self.device {
                    self.say(tr!(
                        format!("plug in controller {} first", self.device),
                        format!("conecte o controle {} antes", self.device)
                    ));
                    return;
                }
                if row == ROW_WIZARD {
                    self.wizard = true;
                    self.sel = ROW_FIRST_BUTTON;
                    self.awaiting = Some(ORDER[0]);
                } else {
                    self.awaiting = Some(ORDER[row - ROW_FIRST_BUTTON]);
                }
                self.awaiting_since = Some(Instant::now());
                self.b_hold = BHold::default();
            }
        }
    }

    /// Um quadro capturando: grava o que chegou para o aparelho escolhido,
    /// ou sai do teste.
    pub fn on_capture(&mut self, plat: &mut Platform, cfg: &mut Config, m: &MenuInput) {
        if m.capture_cancelled || m.click.is_some() {
            self.cancel();
            return;
        }
        if self.testing {
            // Start + Select juntos no aparelho em teste também saem — quem
            // está só no controle não tem Esc.
            let live = plat.live_buttons(&cfg.keymap)[self.device];
            if live[PadButton::Start as usize] && live[PadButton::Select as usize] {
                self.testing = false;
            }
            return;
        }
        let Some(b) = self.awaiting else { return };
        // Sem botão nenhum por 5 s a captura desiste (ninguém fica preso).
        if self
            .awaiting_since
            .is_some_and(|t| t.elapsed() >= CAPTURE_TIMEOUT)
        {
            self.cancel();
            self.say(tr!(
                "no button pressed -- capture cancelled",
                "nenhum botão apertado -- captura cancelada"
            ));
            return;
        }
        // B do controle: soltar rápido grava, segurar ~1 s cancela.
        let mut cap_pad = m.captured_pad;
        let mut cap_port = m.captured_pad_port;
        if self.device > 0 {
            let port = self.device - 1;
            let east_down = plat.gamepad_button_held(port, GamepadBtn::East);
            let pressed =
                m.captured_pad == Some(GamepadBtn::East) && m.captured_pad_port == Some(port);
            match self.b_hold.step(pressed, east_down, Instant::now()) {
                BHoldAct::Holding => return,
                BHoldAct::Cancel => {
                    self.cancel();
                    self.say(tr!("capture cancelled", "captura cancelada"));
                    return;
                }
                BHoldAct::Tap => {
                    cap_pad = Some(GamepadBtn::East);
                    cap_port = Some(port);
                }
                BHoldAct::None => {}
            }
        }
        let (moved, input) = match self.device {
            0 => {
                let Some(name) = &m.captured_key else { return };
                match cfg.keymap.bind_pad(name, b) {
                    Ok(moved) => (moved, key_label(name)),
                    Err(e) => {
                        log::warn!("rebind {} -> {name}: {e}", b.token());
                        return;
                    }
                }
            }
            n => {
                let (Some(btn), Some(port)) = (cap_pad, cap_port) else {
                    return;
                };
                // Guide/Steam fica do sistema (no Deck abre o menu do Steam):
                // nunca vira botão do jogo.
                if btn == GamepadBtn::Guide {
                    self.say(tr!(
                        "the Guide/Steam button can't be bound",
                        "o botão Guide/Steam não pode ser mapeado"
                    ));
                    return;
                }
                if port != n - 1 {
                    self.say(tr!(
                        format!(
                            "that was controller {} -- press it on controller {n}",
                            port + 1
                        ),
                        format!("esse foi o controle {} -- aperte no controle {n}", port + 1)
                    ));
                    return;
                }
                let family = family_of(&plat.gamepad_slots(), n);
                match pad_map_mut(cfg, n).bind(&btn.string(), b) {
                    Ok(moved) => {
                        plat.set_pad_map(n - 1, pad_map(cfg, n).clone());
                        (moved, pad_label(btn, family))
                    }
                    Err(e) => {
                        log::warn!("rebind {} -> {}: {e}", b.token(), btn.string());
                        return;
                    }
                }
            }
        };
        if let Some(other) = moved {
            let now = self.binding_label(plat, cfg, other);
            self.say(tr!(
                format!(
                    "{input} was {} -- {} moved to {now}",
                    action_name(other),
                    action_name(other)
                ),
                format!(
                    "{input} era do {} -- o {} foi para {now}",
                    action_name(other),
                    action_name(other)
                )
            ));
        }
        let i = ORDER.iter().position(|&o| o == b).unwrap_or(0);
        if self.wizard && i + 1 < ORDER.len() {
            self.sel = ROW_FIRST_BUTTON + i + 1;
            self.awaiting = Some(ORDER[i + 1]);
            self.awaiting_since = Some(Instant::now());
        } else {
            if self.wizard {
                self.say(tr!("all set!", "pronto, tudo configurado!"));
            }
            self.awaiting = None;
            self.awaiting_since = None;
            self.wizard = false;
        }
    }

    fn binding_label(&self, plat: &Platform, cfg: &Config, b: PadButton) -> String {
        match self.device {
            0 => cfg
                .keymap
                .key_for(b)
                .map(|k| key_label(&k.name()))
                .unwrap_or_else(|| "-".to_string()),
            n => {
                let family = family_of(&plat.gamepad_slots(), n);
                pad_map(cfg, n)
                    .button_for(b)
                    .map(|btn| pad_label(btn, family))
                    .unwrap_or_else(|| "-".to_string())
            }
        }
    }

    pub fn draw(&self, d: &mut Screen, plat: &Platform, cfg: &Config, top: i32, margin: i32) {
        let slots = plat.gamepad_slots();
        let live = plat.live_buttons(&cfg.keymap)[self.device];
        let blink = (self.born.elapsed().as_millis() / 400).is_multiple_of(2);
        let w = d.size().0 as i32;
        let x = margin;
        let col = x + 20 + 11 * 9;

        // A lista.
        let mut y = top;
        let device_label = match self.device {
            0 => tr!("keyboard", "teclado").to_string(),
            n => match slots.get(n - 1) {
                Some(info) => tr!(
                    format!("controller {n} ({})", short(&info.name, 22)),
                    format!("controle {n} ({})", short(&info.name, 22))
                ),
                None => tr!(
                    format!("controller {n} (not connected)"),
                    format!("controle {n} (não conectado)")
                ),
            },
        };
        row(
            d,
            x,
            y,
            self.sel == ROW_DEVICE,
            &tr!(
                format!("Map: < {device_label} >"),
                format!("Mapear: < {device_label} >")
            ),
        );
        y += ROW_H + 6;
        for (i, &b) in ORDER.iter().enumerate() {
            let r = ROW_FIRST_BUTTON + i;
            let selected = self.sel == r;
            row(d, x, y, selected, action_name(b));
            let (value, color) = if self.awaiting == Some(b) {
                (
                    if blink {
                        tr!("press...", "aperte...").to_string()
                    } else {
                        String::new()
                    },
                    ACCENT,
                )
            } else if live[b as usize] {
                (self.binding_label(plat, cfg, b), ACCENT)
            } else {
                (
                    self.binding_label(plat, cfg, b),
                    if selected { TEXT } else { DIM },
                )
            };
            d.text(col, y, 1, color, &value);
            y += ROW_H;
        }
        y += 6;
        let extra = [
            (
                ROW_WIZARD,
                tr!("Set up all, one by one", "Configurar tudo em sequência"),
            ),
            (
                ROW_TEST,
                if self.testing {
                    tr!("Testing buttons...", "Testando botões...")
                } else {
                    tr!("Test buttons", "Testar botões")
                },
            ),
            (
                ROW_RESET,
                tr!(
                    "Restore defaults (this device)",
                    "Restaurar padrão (deste aparelho)"
                ),
            ),
        ];
        for (r, label) in extra {
            row(d, x, y, self.sel == r, label);
            y += ROW_H;
        }

        // O controle desenhado, à direita da lista (ou embaixo dela, numa
        // TV estreita).
        let list_right = col + 22 * 9;
        let (px, py, pw) = if w - list_right - margin >= 260 {
            let pw = (w - list_right - margin).min(540);
            (
                list_right + (w - list_right - margin - pw) / 2,
                top + 50,
                pw,
            )
        } else {
            let pw = (w - 2 * margin).min(360);
            (x + (w - 2 * margin - pw) / 2, y + 10, pw)
        };
        let highlight = self.awaiting.or_else(|| {
            (ROW_FIRST_BUTTON..ROW_WIZARD)
                .contains(&self.sel)
                .then(|| ORDER[self.sel - ROW_FIRST_BUTTON])
        });
        let ph = draw_snes_pad(
            d,
            px,
            py,
            pw,
            highlight,
            self.awaiting.is_none() || blink,
            &live,
        );

        // Embaixo do controle: o que está acontecendo.
        let ty = py + ph + 24;
        let status = if self.testing {
            Some((
                tr!(
                    "press any button -- esc, a click or start+select leave",
                    "aperte qualquer botão -- esc, um clique ou start+select saem"
                )
                .to_string(),
                ACCENT,
            ))
        } else if let Some(b) = self.awaiting {
            let on = match self.device {
                0 => tr!("on the keyboard", "no teclado").to_string(),
                n => tr!(format!("on controller {n}"), format!("no controle {n}")),
            };
            let left = self
                .awaiting_since
                .map(|t| CAPTURE_TIMEOUT.saturating_sub(t.elapsed()).as_secs() + 1)
                .unwrap_or(0);
            let cancel_hint = match (self.device, self.b_hold.progress(Instant::now())) {
                (0, _) => tr!(
                    format!("esc cancels, or wait {left}s"),
                    format!("esc cancela, ou espere {left}s")
                ),
                (_, Some(p)) => tr!(
                    format!("keep holding B to cancel... {}%", (p * 100.0) as u32),
                    format!("segure o B para cancelar... {}%", (p * 100.0) as u32)
                ),
                (_, None) => tr!(
                    format!("hold B for 1s to cancel, or wait {left}s"),
                    format!("segure o B por 1 s para cancelar, ou espere {left}s")
                ),
            };
            Some((
                tr!(
                    format!(
                        "press the button for {} {on} ({cancel_hint})",
                        action_name(b)
                    ),
                    format!("aperte o botão do {} {on} ({cancel_hint})", action_name(b))
                ),
                ACCENT,
            ))
        } else {
            self.notice
                .as_ref()
                .filter(|(_, at)| at.elapsed() < NOTICE_FOR)
                .map(|(s, _)| (s.clone(), WARN))
        };
        if let Some((s, c)) = status {
            d.text_wrapped(px, ty, pw.max(200) as u32, 1, c, &s);
        }
    }

    /// A linha sob um clique (y local da tela), dado o topo da lista.
    pub fn row_at(y: i32, top: i32) -> Option<usize> {
        if y < top {
            return None;
        }
        // A lista tem dois respiros de 6 px: depois do aparelho e antes das
        // ações extras.
        let mut ry = top;
        for r in 0..ROWS {
            if r == ROW_FIRST_BUTTON || r == ROW_WIZARD {
                ry += 6;
            }
            if y >= ry && y < ry + ROW_H {
                return Some(r);
            }
            ry += ROW_H;
        }
        None
    }
}

fn row(d: &mut Screen, x: i32, y: i32, selected: bool, label: &str) {
    let (cursor, color) = if selected { ("> ", TEXT) } else { ("  ", DIM) };
    d.text(x, y, 1, color, &format!("{cursor}{label}"));
}

fn short(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max - 1).collect();
        t.push('.');
        t
    }
}

fn pad_map(cfg: &Config, device: usize) -> &PadMap {
    if device == 2 {
        &cfg.padmap2
    } else {
        &cfg.padmap
    }
}

fn pad_map_mut(cfg: &mut Config, device: usize) -> &mut PadMap {
    if device == 2 {
        &mut cfg.padmap2
    } else {
        &mut cfg.padmap
    }
}

fn family_of(slots: &[GamepadInfo], device: usize) -> PadFamily {
    slots
        .get(device.wrapping_sub(1))
        .map(|i| i.family)
        .unwrap_or(PadFamily::Generic)
}

/// O nome da ação na lista.
fn action_name(b: PadButton) -> &'static str {
    match b {
        PadButton::Up => tr!("Up", "Cima"),
        PadButton::Down => tr!("Down", "Baixo"),
        PadButton::Left => tr!("Left", "Esquerda"),
        PadButton::Right => tr!("Right", "Direita"),
        PadButton::A => "A",
        PadButton::B => "B",
        PadButton::X => "X",
        PadButton::Y => "Y",
        PadButton::L => "L",
        PadButton::R => "R",
        PadButton::Start => "Start",
        PadButton::Select => "Select",
    }
}

/// O nome de uma tecla do SDL como as pessoas falam.
fn key_label(sdl: &str) -> String {
    let s = match sdl {
        "Up" => tr!("Up arrow", "Seta cima"),
        "Down" => tr!("Down arrow", "Seta baixo"),
        "Left" => tr!("Left arrow", "Seta esquerda"),
        "Right" => tr!("Right arrow", "Seta direita"),
        "Return" => "Enter",
        "Space" => tr!("Space", "Espaço"),
        "Left Shift" => tr!("Left Shift", "Shift esquerdo"),
        "Right Shift" => tr!("Right Shift", "Shift direito"),
        "Left Ctrl" => tr!("Left Ctrl", "Ctrl esquerdo"),
        "Right Ctrl" => tr!("Right Ctrl", "Ctrl direito"),
        "Left Alt" => tr!("Left Option", "Option esquerdo"),
        "Right Alt" => tr!("Right Option", "Option direito"),
        "Left GUI" => tr!("Left Cmd", "Cmd esquerdo"),
        "Right GUI" => tr!("Right Cmd", "Cmd direito"),
        other => return format!("{} {other}", tr!("Key", "Tecla")),
    };
    s.to_string()
}

/// O botão do controle com o nome impresso nele, pelo fabricante.
fn pad_label(btn: GamepadBtn, family: PadFamily) -> String {
    use PadFamily::*;
    let face = |xbox: &'static str, ps: &'static str, nin: &'static str| match family {
        PlayStation => ps,
        Nintendo => nin,
        Xbox | Generic => xbox,
    };
    let s: &str = match btn {
        GamepadBtn::South => face("A", tr!("Cross", "Xis"), "B"),
        GamepadBtn::East => face("B", tr!("Circle", "Bola"), "A"),
        GamepadBtn::West => face("X", tr!("Square", "Quadrado"), "Y"),
        GamepadBtn::North => face("Y", tr!("Triangle", "Triângulo"), "X"),
        GamepadBtn::LeftShoulder => face("LB", "L1", "L"),
        GamepadBtn::RightShoulder => face("RB", "R1", "R"),
        GamepadBtn::Back => face("View", "Share", "-"),
        GamepadBtn::Start => face("Menu", "Options", "+"),
        GamepadBtn::Guide => face("Xbox", "PS", "Home"),
        GamepadBtn::LeftStick => face("L3", "L3", tr!("Left stick", "Analógico esq.")),
        GamepadBtn::RightStick => face("R3", "R3", tr!("Right stick", "Analógico dir.")),
        GamepadBtn::DPadUp => tr!("D-pad up", "Direcional cima"),
        GamepadBtn::DPadDown => tr!("D-pad down", "Direcional baixo"),
        GamepadBtn::DPadLeft => tr!("D-pad left", "Direcional esquerda"),
        GamepadBtn::DPadRight => tr!("D-pad right", "Direcional direita"),
        other => return other.string(),
    };
    s.to_string()
}

/// O controle de Super Nintendo, desenhado em (x, y) com largura `w`:
/// `highlight` ganha um aro claro (piscando quando `ring_on` alterna) e o
/// que está em `pressed` acende. Devolve a altura.
fn draw_snes_pad(
    d: &mut Screen,
    x: i32,
    y: i32,
    w: i32,
    highlight: Option<PadButton>,
    ring_on: bool,
    pressed: &[bool; 12],
) -> i32 {
    const BODY: (u8, u8, u8, u8) = (206, 206, 214, 255);
    const EDGE: (u8, u8, u8, u8) = (176, 176, 186, 255);
    const SHADOW: (u8, u8, u8, u8) = (0, 0, 0, 90);
    const SHOULDER: (u8, u8, u8, u8) = (148, 148, 158, 255);
    const DARK: (u8, u8, u8, u8) = (46, 46, 52, 255);
    const RECESS: (u8, u8, u8, u8) = (188, 188, 198, 255);
    const GROOVE: (u8, u8, u8, u8) = (150, 146, 170, 255);
    const LIT: (u8, u8, u8, u8) = (255, 214, 90, 255);
    const RING: (u8, u8, u8, u8) = (255, 255, 255, 255);

    let wf = w as f32;
    // Cada ponta é um círculo de raio `r`; as duas ficam a `wf - 2r` uma da
    // outra e o meio entre elas é mais estreito — o "osso" do SNES.
    let r = wf * 0.215;
    let xf = x as f32;
    let yf = y as f32 + r * 0.30;
    let (lcx, rcx, cy) = (xf + r, xf + wf - r, yf + r);
    let is = |b: PadButton| pressed[b as usize];
    let ring = |b: PadButton| ring_on && highlight == Some(b);
    let fill = |b: PadButton, base: (u8, u8, u8, u8)| if is(b) { LIT } else { base };

    // Ombros: atrás do corpo, uma faixa grossa aparece por cima de cada
    // ponta, seguindo a curva, com o nome escrito nela.
    for (b, sx) in [
        (PadButton::L, lcx + r * 0.18),
        (PadButton::R, rcx - r * 0.18),
    ] {
        let sy = cy - r * 0.82;
        let (rx, ry) = (r * 0.78, r * 0.44);
        if ring(b) {
            d.ellipse(sx, sy - 2.0, rx + 4.0, ry + 4.0, RING);
        }
        d.ellipse(sx, sy, rx, ry, fill(b, SHOULDER));
        // Um friso mais claro na crista, como o plástico do botão.
        d.ellipse(
            sx,
            sy - ry * 0.55,
            rx * 0.70,
            ry * 0.30,
            (255, 255, 255, 40),
        );
    }
    let shoulder_labels = [
        (PadButton::L, lcx + r * 0.18),
        (PadButton::R, rcx - r * 0.18),
    ];

    // O corpo: as duas pontas e a ponte do meio, mais baixa embaixo.
    let bridge = |d: &mut Screen, dy: f32, grow: f32, c| {
        d.ellipse(lcx, cy + dy, r + grow, r + grow, c);
        d.ellipse(rcx, cy + dy, r + grow, r + grow, c);
        d.quad(
            [
                (lcx, cy - r * 0.90 - grow + dy),
                (rcx, cy - r * 0.90 - grow + dy),
                (rcx, cy + r * 0.66 + grow + dy),
                (lcx, cy + r * 0.66 + grow + dy),
            ],
            c,
        );
    };
    bridge(d, 5.0, 0.0, SHADOW);
    bridge(d, 0.0, 1.5, EDGE);
    bridge(d, 0.0, 0.0, BODY);

    let label = |d: &mut Screen, cx: f32, cy: f32, s: &str, c: (u8, u8, u8)| {
        let tw = s.chars().count() as f32 * 9.0;
        d.text((cx - tw / 2.0) as i32, (cy - 10.0) as i32, 1, c, s);
    };
    // O nome do ombro fica na faixa visível, entre a crista e o corpo.
    for (b, sx) in shoulder_labels {
        let ink = if is(b) { (40, 30, 0) } else { (44, 44, 52) };
        label(d, sx, cy - r * 1.105, action_name(b), ink);
    }

    // Direcional: uma cruz fina num rebaixo redondo; cada braço acende sozinho.
    let a = r * 0.50;
    let t = r * 0.30;
    let arms = [
        (PadButton::Up, lcx - t / 2.0, cy - a, t, a),
        (PadButton::Down, lcx - t / 2.0, cy, t, a),
        (PadButton::Left, lcx - a, cy - t / 2.0, a, t),
        (PadButton::Right, lcx, cy - t / 2.0, a, t),
    ];
    d.ellipse(lcx, cy, r * 0.66, r * 0.66, RECESS);
    for (b, ax, ay, aw, ah) in arms {
        if ring(b) {
            d.fill(
                ax as i32 - 3,
                ay as i32 - 3,
                aw as u32 + 6,
                ah as u32 + 6,
                RING,
            );
        }
    }
    for (_, ax, ay, aw, ah) in arms {
        d.fill(ax as i32, ay as i32 + 3, aw as u32, ah as u32, SHADOW);
    }
    for (b, ax, ay, aw, ah) in arms {
        d.fill(ax as i32, ay as i32, aw as u32, ah as u32, fill(b, DARK));
    }
    d.ellipse(lcx, cy, t * 0.22, t * 0.22, (70, 70, 78, 255));

    // Os quatro botões em losango, em dois sulcos diagonais (Y-X e B-A),
    // nas cores do Super Famicom.
    let br = r * 0.20;
    let dd = r * 0.42;
    let (fcx, fcy) = (rcx - r * 0.02, cy + r * 0.02);
    for (p, q) in [
        ((fcx - dd, fcy), (fcx, fcy - dd)),
        ((fcx, fcy + dd), (fcx + dd, fcy)),
    ] {
        pill(d, p, q, br + r * 0.07, GROOVE);
    }
    let faces = [
        (PadButton::X, fcx, fcy - dd, (64, 92, 200, 255)),
        (PadButton::A, fcx + dd, fcy, (200, 52, 52, 255)),
        (PadButton::B, fcx, fcy + dd, (226, 186, 40, 255)),
        (PadButton::Y, fcx - dd, fcy, (44, 146, 72, 255)),
    ];
    for (b, fx, fy, c) in faces {
        if ring(b) {
            d.ellipse(fx, fy, br + 4.0, br + 4.0, RING);
        }
        d.ellipse(fx, fy + 2.5, br, br, SHADOW);
        d.ellipse(fx, fy, br, br, fill(b, c));
        let ink = if is(b) { (40, 30, 0) } else { (250, 250, 250) };
        label(d, fx, fy, action_name(b), ink);
    }

    // Select e Start: duas pílulas inclinadas no meio, nome embaixo.
    let mid = xf + wf / 2.0;
    let half = wf * 0.035;
    let (ux, uy) = (0.80_f32, -0.60_f32); // sobem para a direita
    for (b, sx) in [
        (PadButton::Select, mid - wf * 0.065),
        (PadButton::Start, mid + wf * 0.065),
    ] {
        let sy = cy + r * 0.12;
        let p = (sx - ux * half, sy - uy * half);
        let q = (sx + ux * half, sy + uy * half);
        let pr = r * 0.075;
        if ring(b) {
            pill(d, p, q, pr + 3.5, RING);
        }
        pill(d, (p.0, p.1 + 2.0), (q.0, q.1 + 2.0), pr, SHADOW);
        pill(d, p, q, pr, fill(b, (92, 92, 100, 255)));
        label(d, sx, sy + r * 0.36, action_name(b), (96, 96, 108));
    }

    (r * 2.40 + 8.0) as i32
}

/// Uma pílula de `p` a `q` com raio `rad`: dois círculos e o retângulo
/// girado entre eles.
fn pill(d: &mut Screen, p: (f32, f32), q: (f32, f32), rad: f32, c: (u8, u8, u8, u8)) {
    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
    let len = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (nx, ny) = (-dy / len * rad, dx / len * rad);
    d.ellipse(p.0, p.1, rad, rad, c);
    d.ellipse(q.0, q.1, rad, rad, c);
    d.quad(
        [
            (p.0 + nx, p.1 + ny),
            (q.0 + nx, q.1 + ny),
            (q.0 - nx, q.1 - ny),
            (p.0 - nx, p.1 - ny),
        ],
        c,
    );
}

#[cfg(test)]
mod hold_tests {
    use super::*;

    #[test]
    fn a_quick_tap_of_b_binds_it_on_release() {
        let t0 = Instant::now();
        let mut h = BHold::default();
        assert_eq!(h.step(false, false, t0), BHoldAct::None);
        assert_eq!(h.step(true, true, t0), BHoldAct::Holding);
        assert_eq!(
            h.step(false, true, t0 + Duration::from_millis(300)),
            BHoldAct::Holding
        );
        assert_eq!(
            h.step(false, false, t0 + Duration::from_millis(400)),
            BHoldAct::Tap
        );
        assert_eq!(h.progress(t0), None);
    }

    #[test]
    fn holding_b_for_a_second_cancels() {
        let t0 = Instant::now();
        let mut h = BHold::default();
        h.step(true, true, t0);
        let p = h.progress(t0 + Duration::from_millis(500)).unwrap();
        assert!((p - 0.5).abs() < 0.01);
        assert_eq!(
            h.step(false, true, t0 + Duration::from_millis(999)),
            BHoldAct::Holding
        );
        assert_eq!(
            h.step(false, true, t0 + Duration::from_millis(1000)),
            BHoldAct::Cancel
        );
        // Depois de cancelar, o B segurado não vira outro "tap" ao soltar.
        assert_eq!(
            h.step(false, false, t0 + Duration::from_millis(1100)),
            BHoldAct::None
        );
    }
}
