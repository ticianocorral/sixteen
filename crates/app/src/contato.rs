//! Mau contato do cartucho (soprar o cartucho): opcional, desligado por
//! padrão (`bad_contact` no `sixteen.cfg`). O encaixe é sorteado quando o
//! cartucho entra; se deu mau contato, o primeiro Power não sobe o jogo —
//! tela preta, lixo de tiles, imagem travada ou paleta trocada — até
//! desligar, ejetar, soprar e inserir de novo. Desligar e ligar de novo às
//! vezes resolve sozinho, como na vida real.
//!
//! Regras para não irritar: só no Power depois de um encaixe (retomar com
//! Power nunca falha), no máximo duas falhas seguidas, e o save nunca é
//! tocado — a SRAM do disco volta para o core ao sair da falha.

use std::time::{Duration, Instant};

use sixteen_emulation::{Core, Frame as EmuFrame, PixelFormat as EmuFormat};
use sixteen_ntsc::NtscFilter;
use sixteen_platform::{
    AudioOut, BlowButton, BlowScene, Cabinet, Dust, FrameRef, Input, KeyMap, PadButton,
    PixelFormat as PlatFormat, Platform, RfAudio, UiEvent,
};

use crate::config::BadContact;

/// Falhas seguidas antes de o encaixe pegar com certeza — a terceira
/// tentativa sempre funciona.
const MAX_STREAK: u8 = 2;
/// A tela que o core do SNES desenha.
const SNES_W: u32 = 256;
const SNES_H: u32 = 224;

/// xorshift32 — o sorteio do encaixe e o lixo da tela; nada aqui precisa
/// de qualidade criptográfica.
pub(crate) struct Rng(u32);

impl Rng {
    pub(crate) fn seeded() -> Self {
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() ^ d.as_secs() as u32)
            .unwrap_or(0x9e37_79b9);
        Rng(t | 1)
    }

    pub(crate) fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// 0.0..1.0
    pub(crate) fn next_f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// O jeito que o jogo não sobe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FaultKind {
    /// Tela preta, só o zumbido.
    Black,
    /// Blocos coloridos embaralhados e listras verticais.
    Tiles,
    /// A abertura começa e trava, com o "bzzzt" em loop.
    Freeze,
    /// A abertura com as cores trocadas, travando em seguida.
    Palette,
}

/// O estado do encaixe do cartucho no slot, de um `run_game` inteiro.
pub(crate) struct Contact {
    rng: Rng,
    /// O encaixe atual está ruim: o próximo Power falha.
    bad: bool,
    /// Falhas seguidas desde o último boot bom.
    streak: u8,
    /// Houve falha desde o último boot bom — ejetar abre o sopro.
    failed: bool,
}

impl Contact {
    /// O cartucho acabou de entrar: sorteia o encaixe. `enabled` falso
    /// (captura headless) nunca falha. `SIXTEEN_MAU_CONTATO=1` força o
    /// primeiro encaixe ruim — para testar sem depender da sorte.
    pub(crate) fn inserted(level: BadContact, enabled: bool) -> Self {
        let mut rng = Rng::seeded();
        let forced = std::env::var_os("SIXTEEN_MAU_CONTATO").is_some_and(|v| v != "0");
        let bad = enabled && (forced || rng.next_f32() < level.chance());
        Contact {
            rng,
            bad,
            streak: 0,
            failed: false,
        }
    }

    /// Houve falha desde o último boot bom (ejetar abre o sopro).
    pub(crate) fn needs_ritual(&self) -> bool {
        self.failed
    }

    /// Power ligado depois de um encaixe ou de uma falha: a falha, se o
    /// encaixe está ruim; `None` = o jogo sobe (e a contagem zera).
    pub(crate) fn power_on(&mut self) -> Option<FaultKind> {
        if !self.bad {
            self.streak = 0;
            self.failed = false;
            return None;
        }
        self.streak += 1;
        self.failed = true;
        let r = self.rng.next_f32();
        Some(if r < 0.35 {
            FaultKind::Black
        } else if r < 0.65 {
            FaultKind::Tiles
        } else if r < 0.85 {
            FaultKind::Freeze
        } else {
            FaultKind::Palette
        })
    }

    /// Desligou na falha: ligar de novo às vezes resolve (40%).
    pub(crate) fn power_cycled(&mut self) {
        self.bad = self.roll(0.6);
    }

    /// Inseriu de novo depois do sopro: contatos limpos quase sempre pegam;
    /// sujos (ou sem soprar), metade das vezes; embaçados, pior.
    pub(crate) fn reinserted(&mut self, clean: f32, fog: f32) {
        let base = if clean >= 0.6 {
            0.1
        } else if clean > 0.15 {
            0.35
        } else {
            0.5
        };
        self.bad = self.roll((base + fog * 0.4).min(0.9));
    }

    fn roll(&mut self, fail: f32) -> bool {
        self.streak < MAX_STREAK && self.rng.next_f32() < fail
    }
}

/// Uma falha em andamento na tela (o console "ligado", o jogo não).
pub(crate) struct FaultRun {
    pub(crate) kind: FaultKind,
    frames: u32,
    /// Freeze/Palette: o frame em que a imagem trava.
    stop_at: u32,
    rng: Rng,
    /// O quadro RGB565 da tela (sintetizado, ou o último do core travado).
    screen: Vec<u8>,
    pitch: usize,
    width: u32,
    height: u32,
    frozen: bool,
    /// O som que fica em loop quando a imagem trava.
    stuck_audio: Vec<i16>,
    hum_phase: f32,
}

impl FaultRun {
    pub(crate) fn new(kind: FaultKind) -> Self {
        let mut rng = Rng::seeded();
        let stop_at = match kind {
            FaultKind::Freeze => 40 + rng.next_u32() % 70,
            FaultKind::Palette => 120 + rng.next_u32() % 80,
            _ => 0,
        };
        FaultRun {
            kind,
            frames: 0,
            stop_at,
            rng,
            screen: vec![0; (SNES_W * SNES_H * 2) as usize],
            pitch: (SNES_W * 2) as usize,
            width: SNES_W,
            height: SNES_H,
            frozen: matches!(kind, FaultKind::Black | FaultKind::Tiles),
            stuck_audio: Vec::new(),
            hum_phase: 0.0,
        }
    }

    /// Um frame da falha: roda o core até travar (Freeze/Palette) ou
    /// desenha o lixo, passa pelo NTSC e pelo tubo, e põe o som na fila.
    // A falha mexe em tudo que o laço do jogo mexe (core, NTSC, tubo,
    // áudio): agrupar em struct só para a contagem de argumentos esconderia
    // a fiação.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn frame(
        &mut self,
        core: &mut Core,
        ntsc: &mut NtscFilter,
        cab: &mut Cabinet,
        audio: &AudioOut,
        audio_cap: usize,
        rom: &[u8],
        vol_buf: &mut Vec<i16>,
        rf: &mut RfAudio,
    ) {
        self.frames += 1;
        if !self.frozen {
            for port in 0..sixteen_platform::MAX_PORTS {
                for (rb, _) in crate::runner::PAD {
                    core.set_button(port, rb, false);
                }
            }
            core.run();
            if audio.queued_frames() < audio_cap {
                crate::runner::queue_game_audio(audio, cab, core.audio(), vol_buf, rf);
            }
            // As últimas ~3 frames de som viram o "bzzzt" quando trava.
            self.stuck_audio.extend_from_slice(core.audio());
            let keep = (audio.source_rate as usize / 20) * 2;
            if self.stuck_audio.len() > keep {
                let cut = self.stuck_audio.len() - keep;
                self.stuck_audio.drain(..cut);
            }
            if let Some(frame) = core.take_frame() {
                self.grab(&frame);
                core.recycle_frame(frame);
            }
            if self.frames >= self.stop_at {
                self.frozen = true;
            }
        } else {
            if self.kind == FaultKind::Tiles && (self.frames.is_multiple_of(23) || self.frames == 1)
            {
                self.tiles(rom);
            }
            if audio.queued_frames() < audio_cap {
                let chunk = if matches!(self.kind, FaultKind::Freeze | FaultKind::Palette)
                    && !self.stuck_audio.is_empty()
                {
                    self.stuck_audio.clone()
                } else {
                    self.hum(audio.source_rate)
                };
                crate::runner::queue_game_audio(audio, cab, &chunk, vol_buf, rf);
            }
        }
        let (out, ow, oh) = ntsc.process(&self.screen, self.width, self.height, self.pitch);
        let bytes = unsafe { std::slice::from_raw_parts(out.as_ptr() as *const u8, out.len() * 2) };
        let fref = FrameRef {
            width: ow,
            height: oh,
            pitch: ow as usize * 2,
            format: PlatFormat::Rgb565,
            pixels: bytes,
        };
        cab.present_frame(&fref, 4.0 / 3.0);
    }

    /// A falha num BMP (captura headless): o mesmo quadro do `frame`.
    pub(crate) fn capture(
        &mut self,
        ntsc: &mut NtscFilter,
        cab: &mut Cabinet,
        rom: &[u8],
        path: &std::path::Path,
    ) -> anyhow::Result<()> {
        if self.kind == FaultKind::Tiles {
            self.tiles(rom);
        }
        let (out, ow, oh) = ntsc.process(&self.screen, self.width, self.height, self.pitch);
        let bytes = unsafe { std::slice::from_raw_parts(out.as_ptr() as *const u8, out.len() * 2) };
        let fref = FrameRef {
            width: ow,
            height: oh,
            pitch: ow as usize * 2,
            format: PlatFormat::Rgb565,
            pixels: bytes,
        };
        cab.capture_bmp(&fref, 4.0 / 3.0, path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))
    }

    /// Copia o frame do core para a tela da falha — com as cores trocadas
    /// (vermelho ↔ azul) na falha de paleta.
    fn grab(&mut self, frame: &EmuFrame) {
        if frame.format != EmuFormat::Rgb565 {
            return;
        }
        self.width = frame.width;
        self.height = frame.height;
        self.pitch = frame.pitch;
        self.screen.clear();
        self.screen.extend_from_slice(&frame.pixels);
        if self.kind == FaultKind::Palette {
            for px in self.screen.as_chunks_mut::<2>().0 {
                let p = u16::from_le_bytes([px[0], px[1]]);
                let swapped = ((p & 0x1f) << 11) | (p & 0x07e0) | (p >> 11);
                px.copy_from_slice(&swapped.to_le_bytes());
            }
        }
    }

    /// O lixo de tiles: bytes da própria ROM lidos como tiles 4bpp do SNES
    /// com uma paleta sorteada, mais umas listras verticais.
    fn tiles(&mut self, rom: &[u8]) {
        self.width = SNES_W;
        self.height = SNES_H;
        self.pitch = (SNES_W * 2) as usize;
        self.screen.resize((SNES_W * SNES_H * 2) as usize, 0);
        // Um tilemap corrompido não é ruído: são poucos tiles (2bpp, uma
        // paleta de 4 cores fortes) repetidos em faixas, linhas inteiras
        // duplicadas e trechos vazios.
        const POOL: [u16; 8] = [
            0xf800, 0x07e0, 0x001f, 0xffe0, 0xf81f, 0x07ff, 0xfd20, 0xffff,
        ];
        let mut palette = [0u16; 4];
        for c in palette.iter_mut().skip(1) {
            *c = POOL[self.rng.next_u32() as usize % POOL.len()];
        }
        let span = rom.len().saturating_sub(32).max(1);
        let tile_set: Vec<usize> = (0..10)
            .map(|_| (self.rng.next_u32() as usize % span) & !15)
            .collect();
        let tiles_x = (SNES_W / 8) as usize;
        let tiles_y = (SNES_H / 8) as usize;
        let mut map: Vec<Option<usize>> = vec![None; tiles_x * tiles_y];
        for ty in 0..tiles_y {
            if ty > 0 && self.rng.next_f32() < 0.35 {
                // A linha de cima repetida, como um scroll que não andou.
                let (prev, cur) = map.split_at_mut(ty * tiles_x);
                cur[..tiles_x].copy_from_slice(&prev[(ty - 1) * tiles_x..]);
                continue;
            }
            let mut tx = 0;
            while tx < tiles_x {
                let run = 1 + self.rng.next_u32() as usize % 12;
                let tile = (self.rng.next_f32() > 0.3)
                    .then(|| tile_set[self.rng.next_u32() as usize % tile_set.len()]);
                for x in tx..(tx + run).min(tiles_x) {
                    map[ty * tiles_x + x] = tile;
                }
                tx += run;
            }
        }
        for ty in 0..tiles_y {
            for tx in 0..tiles_x {
                let tile = map[ty * tiles_x + tx];
                for row in 0..8usize {
                    let b = |i: usize| tile.and_then(|o| rom.get(o + i).copied()).unwrap_or(0);
                    let (p0, p1) = (b(row * 2), b(row * 2 + 1));
                    for col in 0..8usize {
                        let bit = 7 - col;
                        let idx = ((p0 >> bit) & 1) | (((p1 >> bit) & 1) << 1);
                        let c = palette[idx as usize];
                        let x = tx * 8 + col;
                        let y = ty * 8 + row;
                        let at = y * self.pitch + x * 2;
                        self.screen[at..at + 2].copy_from_slice(&c.to_le_bytes());
                    }
                }
            }
        }
        // Umas listras verticais de uma cor só.
        for _ in 0..(1 + self.rng.next_u32() % 3) {
            let x0 = (self.rng.next_u32() % SNES_W) as usize;
            let w = 1 + (self.rng.next_u32() % 3) as usize;
            let c = (self.rng.next_u32() & 0xffff) as u16;
            for y in 0..SNES_H as usize {
                for x in x0..(x0 + w).min(SNES_W as usize) {
                    let at = y * self.pitch + x * 2;
                    self.screen[at..at + 2].copy_from_slice(&c.to_le_bytes());
                }
            }
        }
    }

    /// O zumbido de 60 Hz da TV sem imagem (um frame de som, estéreo).
    fn hum(&mut self, rate: u32) -> Vec<i16> {
        let n = (rate / 60).max(1) as usize;
        let mut out = Vec::with_capacity(n * 2);
        for _ in 0..n {
            self.hum_phase = (self.hum_phase + 60.0 / rate as f32).fract();
            let t = self.hum_phase * std::f32::consts::TAU;
            let s =
                (t.sin() * 0.6 + (2.0 * t).sin() * 0.3 + (3.0 * t).sin().signum() * 0.1) * 1600.0;
            let v = s as i16;
            out.push(v);
            out.push(v);
        }
        out
    }
}

/// Como a cena do sopro terminou.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum RitualExit {
    /// Inserir de novo — com o quanto os contatos ficaram limpos e
    /// embaçados.
    Reinsert { clean: f32, fog: f32 },
    /// Desistiu: o cartucho vai para a estante (o ejetar de sempre).
    Shelf,
    /// Fechou o app no meio.
    Quit,
}

/// O estado vivo do sopro (fôlego, limpeza, poeira) — separado do laço
/// para o teste e a captura headless andarem o mesmo relógio.
pub(crate) struct Blow {
    pub(crate) scene: BlowScene,
    rng: Rng,
}

impl Blow {
    pub(crate) fn new(title: &str) -> Self {
        Blow {
            scene: BlowScene {
                title: title.to_string(),
                ..BlowScene::default()
            },
            rng: Rng::seeded(),
        }
    }

    /// Avança `dt` segundos com o botão de soprar `holding` ou não.
    pub(crate) fn step(&mut self, dt: f32, holding: bool) {
        let s = &mut self.scene;
        s.time += dt;
        let ready = s.lift >= 1.0;
        let holding = holding && ready;
        if holding && !s.out_of_breath {
            s.breath = (s.breath + dt * 0.55).min(1.0);
            if s.breath >= 1.0 {
                // Soprou até o fim: sem fôlego, e o bafo embaça os pinos.
                s.out_of_breath = true;
                s.fog = (s.fog + 0.6).min(1.0);
            }
        } else if !holding {
            s.breath = (s.breath - dt * 0.7).max(0.0);
            if s.out_of_breath && s.breath < 0.5 {
                s.out_of_breath = false;
            }
        }
        s.blowing = holding && !s.out_of_breath;
        s.fog = (s.fog - dt * 0.22).max(0.0);
        if s.blowing && s.fog < 0.05 {
            s.clean = (s.clean + dt * 0.42).min(1.0);
        }
        // A poeira: muita no começo, quase nada com os pinos limpos.
        if s.blowing {
            let rate = 90.0 * ((1.0 - s.clean) + 0.08);
            let mut n = rate * dt;
            while n > 0.0 {
                if n < 1.0 && self.rng.next_f32() > n {
                    break;
                }
                n -= 1.0;
                s.dust.push(Dust {
                    x: (self.rng.next_f32() - 0.5) * 0.7,
                    y: 0.0,
                    life: 1.0,
                    size: self.rng.next_f32(),
                });
            }
        }
        let rng = &mut self.rng;
        for d in &mut s.dust {
            let spread = (d.size - 0.5) * 0.6 + (rng.next_f32() - 0.5) * 0.2;
            d.x += spread * dt;
            d.y -= (0.45 + d.size * 0.5) * dt * d.life.max(0.3);
            d.life -= dt * 1.1;
        }
        s.dust.retain(|d| d.life > 0.0);
    }
}

/// A cena do sopro, depois de ejetar um cartucho que falhou: o cartucho
/// voa para a frente da TV com os contatos para quem joga, segurar o clique
/// (ou Y/B) sopra, e "Inserir de novo"/"Guardar na estante" encerram.
pub(crate) fn blow_ritual(
    plat: &mut Platform,
    cab: &mut Cabinet,
    input: &mut Input,
    keymap: &KeyMap,
    title: &str,
) -> RitualExit {
    const FLY_IN: f32 = 0.9;
    const FLY_OUT: f32 = 0.6;
    let frame_time = Duration::from_secs_f64(1.0 / 60.0);
    let audio = plat.open_audio(22_050).ok();
    let mut blow = Blow::new(title);
    let mut last = Instant::now();
    let mut next = Instant::now();
    let mut prev_a = true;
    let mut prev_select = true;
    let mut leaving: Option<RitualExit> = None;
    let mut noise = Rng::seeded();
    let mut lp = 0.0f32;
    cab.set_cartridge_motion(Some((1.0, true)));

    loop {
        let now = Instant::now();
        let dt = (now - last).as_secs_f32().min(0.05);
        last = now;

        let mut clicked: Option<BlowButton> = None;
        for ev in plat.poll(input, keymap) {
            match ev {
                UiEvent::CloseRequested => return RitualExit::Quit,
                UiEvent::Click(x, y) => {
                    let (ox, oy) = cab.window_to_output(x, y);
                    if cab.hit_close_button(ox, oy) {
                        return RitualExit::Quit;
                    } else if cab.hit_minimize_button(ox, oy) {
                        cab.minimize();
                    } else if let Some(b) = cab.hit_blow_button(ox, oy) {
                        clicked = Some(b);
                    } else if let Some(tv) = cab.hit_tv_controls(ox, oy) {
                        match tv {
                            sixteen_platform::TvControl::Power => cab.toggle_tv_power(),
                            sixteen_platform::TvControl::VolUp => cab.nudge_volume(1),
                            sixteen_platform::TvControl::VolDown => cab.nudge_volume(-1),
                            sixteen_platform::TvControl::Channel => cab.toggle_channel(),
                        }
                    }
                }
                _ => {}
            }
        }
        let held = |b: PadButton| (0..sixteen_platform::MAX_PORTS).any(|p| input.held(p, b));
        let a_now = held(PadButton::A) || held(PadButton::Start);
        let select_now = held(PadButton::Select);
        if a_now && !prev_a {
            clicked = Some(BlowButton::Insert);
        }
        if select_now && !prev_select {
            clicked = Some(BlowButton::Shelf);
        }
        prev_a = a_now;
        prev_select = select_now;

        // Segurar o clique no tubo, fora dos botões (ou Y/B no controle), sopra.
        let mouse = plat.left_mouse_held().is_some_and(|(x, y)| {
            let (ox, oy) = cab.window_to_output(x, y);
            cab.hit_screen_point(ox, oy).is_some() && cab.hit_blow_button(ox, oy).is_none()
        });
        let holding = leaving.is_none() && (mouse || held(PadButton::Y) || held(PadButton::B));

        match leaving {
            None => {
                if blow.scene.lift < 1.0 {
                    blow.scene.lift = (blow.scene.lift + dt / FLY_IN).min(1.0);
                } else if let Some(b) = clicked {
                    leaving = Some(match b {
                        BlowButton::Insert => RitualExit::Reinsert {
                            clean: blow.scene.clean,
                            fog: blow.scene.fog,
                        },
                        BlowButton::Shelf => RitualExit::Shelf,
                    });
                }
            }
            Some(exit) => {
                blow.scene.lift = (blow.scene.lift - dt / FLY_OUT).max(0.0);
                if blow.scene.lift <= 0.0 {
                    if let Some(a) = &audio {
                        a.clear();
                    }
                    return exit;
                }
            }
        }
        blow.step(dt, holding);

        // O "ffff": ruído passado num passa-baixa, só enquanto sopra.
        if let Some(a) = &audio {
            if blow.scene.blowing && cab.tv_on() && cab.volume() > 0 {
                if a.queued_frames() < 22_050 / 15 {
                    let n = 22_050 / 60;
                    let amp = 5200.0 * cab.volume() as f32 / 16.0;
                    let mut buf = Vec::with_capacity(n * 2);
                    for _ in 0..n {
                        let w = noise.next_f32() * 2.0 - 1.0;
                        lp += (w - lp) * 0.18;
                        let v = (lp * amp) as i16;
                        buf.push(v);
                        buf.push(v);
                    }
                    a.queue(&buf);
                }
            } else {
                a.clear();
            }
        }

        cab.present_blow(crate::runner::OFF_STATIC_LEVEL, &blow.scene);
        crate::runner::pace_frame(&mut next, frame_time);
    }
}

/// Captura headless (`emu-run --debug-blow DIR`): as quatro falhas e a
/// cena do sopro em vários momentos, em BMP.
pub(crate) fn debug_shots(
    cab: &mut Cabinet,
    ntsc: &mut NtscFilter,
    rom: &[u8],
    title: &str,
    dir: &std::path::Path,
) -> anyhow::Result<()> {
    std::fs::create_dir_all(dir)?;
    for (name, kind) in [
        ("falha_tiles", FaultKind::Tiles),
        ("falha_preta", FaultKind::Black),
    ] {
        let mut f = FaultRun::new(kind);
        f.capture(ntsc, cab, rom, &dir.join(format!("{name}.bmp")))?;
    }
    cab.set_cartridge_motion(Some((1.0, true)));
    let mut blow = Blow::new(title);
    let shots: [(&str, f32, f32, bool); 6] = [
        ("sopro_0_subindo", 0.25, 0.0, false),
        ("sopro_1_virando", 0.6, 0.0, false),
        ("sopro_2_sujo", 1.0, 0.0, false),
        ("sopro_3_soprando", 1.0, 1.2, true),
        ("sopro_4_sem_folego", 1.0, 1.0, true),
        ("sopro_5_limpo", 1.0, 3.0, true),
    ];
    for (name, lift, secs, holding) in shots {
        blow.scene.lift = lift;
        let steps = (secs * 60.0) as usize;
        for i in 0..steps {
            // "limpo": respira entre os sopros para não perder o fôlego.
            let hold = holding && !(name == "sopro_5_limpo" && (i / 50) % 2 == 1);
            blow.step(1.0 / 60.0, hold);
        }
        if name == "sopro_5_limpo" {
            blow.scene.clean = 1.0;
            blow.scene.fog = 0.0;
            blow.scene.out_of_breath = false;
            blow.scene.blowing = false;
        }
        cab.capture_blow_bmp(
            crate::runner::OFF_STATIC_LEVEL,
            &blow.scene,
            &dir.join(format!("{name}.bmp")),
        )
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(bad: bool) -> Contact {
        Contact {
            rng: Rng(12345),
            bad,
            streak: 0,
            failed: false,
        }
    }

    #[test]
    fn good_contact_boots_and_never_asks_for_the_ritual() {
        let mut c = contact(false);
        assert_eq!(c.power_on(), None);
        assert!(!c.needs_ritual());
    }

    #[test]
    fn third_attempt_always_works() {
        let mut c = contact(true);
        assert!(c.power_on().is_some());
        assert!(c.needs_ritual());
        // Mesmo com 100% de chance de falhar, a contagem segura.
        c.bad = true;
        assert!(c.power_on().is_some());
        c.bad = c.roll(1.0);
        assert!(!c.bad, "depois de duas falhas seguidas o encaixe pega");
        assert_eq!(c.power_on(), None);
        assert!(!c.needs_ritual());
    }

    #[test]
    fn clean_contacts_mostly_work() {
        let mut fails = 0;
        for seed in 1..2000u32 {
            let mut c = contact(true);
            c.rng = Rng(seed.wrapping_mul(2_654_435_761) | 1);
            c.reinserted(1.0, 0.0);
            fails += c.bad as u32;
        }
        assert!(fails < 400, "limpos falham pouco ({fails}/2000)");
    }

    #[test]
    fn blowing_cleans_and_running_out_of_breath_fogs_the_pins() {
        let mut b = Blow::new("Jogo");
        b.scene.lift = 1.0;
        for _ in 0..60 {
            b.step(1.0 / 60.0, true);
        }
        assert!(b.scene.clean > 0.3);
        assert!(!b.scene.dust.is_empty());
        for _ in 0..120 {
            b.step(1.0 / 60.0, true);
        }
        assert!(b.scene.out_of_breath);
        assert!(b.scene.fog > 0.0);
        let before = b.scene.clean;
        b.step(1.0 / 60.0, true);
        assert_eq!(b.scene.clean, before, "sem fôlego não limpa");
    }

    #[test]
    fn no_blowing_before_the_cartridge_arrives() {
        let mut b = Blow::new("Jogo");
        b.scene.lift = 0.5;
        b.step(0.5, true);
        assert_eq!(b.scene.clean, 0.0);
        assert!(!b.scene.blowing);
    }
}
