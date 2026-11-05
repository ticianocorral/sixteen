//! A TV e o quarto em volta dela: o menu OSD de imagem,
//! o burn-in do tubo, os sons do próprio tubo (o chiado do flyback e os
//! estalos do vidro esquentando), o alto-falante da TV contra o 3 em 1 e
//! a luz do quarto pela hora do dia.

use super::*;

/// O menu OSD fecha sozinho depois disso sem mexer.
const MENU_IDLE: Duration = Duration::from_secs(6);
/// Passo de cada toque de VOL-/VOL+ no menu (escala 0..100).
const MENU_STEP: u8 = 5;
/// A taxa do som do tubo: alta o bastante para o flyback de 15.734 Hz.
const AMB_RATE: u32 = 48_000;
/// O flyback do NTSC: a frequência horizontal.
const FLYBACK_HZ: f32 = 15_734.0;
/// A grade do burn-in, na proporção da imagem do SNES.
const BURN_W: usize = 128;
const BURN_H: usize = 112;

/// Os ajustes de imagem do menu OSD — cada um de 0 a 100, 50 = neutro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Picture {
    pub brilho: u8,
    pub contraste: u8,
    pub cor: u8,
    pub nitidez: u8,
}

impl Default for Picture {
    fn default() -> Self {
        Self {
            brilho: 50,
            contraste: 50,
            cor: 50,
            nitidez: 50,
        }
    }
}

impl Picture {
    const ITEMS: usize = 4;

    fn get(&self, item: usize) -> u8 {
        [self.brilho, self.contraste, self.cor, self.nitidez][item]
    }

    fn set(&mut self, item: usize, v: u8) {
        let v = v.min(100);
        match item {
            0 => self.brilho = v,
            1 => self.contraste = v,
            2 => self.cor = v,
            _ => self.nitidez = v,
        }
    }

    fn label(item: usize) -> &'static str {
        match item {
            0 => tr!("BRIGHTNESS", "BRILHO"),
            1 => tr!("CONTRAST", "CONTRASTE"),
            2 => tr!("COLOR", "COR"),
            _ => tr!("SHARPNESS", "NITIDEZ"),
        }
    }
}

/// Por onde sai o som do jogo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TvSound {
    /// O 3 em 1 ligado na TV: estéreo cheio, como sai do core.
    #[default]
    Stereo,
    /// O alto-falante da própria TV: pequeno, mono e abafado.
    Speaker,
}

/// O alto-falante da TV: soma os canais, corta graves e agudos (um cone
/// de 3 polegadas numa caixa de plástico) e satura de leve quando o volume
/// sobe.
#[derive(Debug, Default)]
pub struct TvSpeaker {
    hp_in: f32,
    hp_out: f32,
    lp1: f32,
    lp2: f32,
}

impl TvSpeaker {
    /// Processa `buf` (estéreo intercalado, `rate` Hz) no lugar.
    pub fn process(&mut self, buf: &mut [i16], rate: u32) {
        let rate = rate.max(8000) as f32;
        let hp = 1.0 / (1.0 + std::f32::consts::TAU * 220.0 / rate);
        let lp = (std::f32::consts::TAU * 3200.0 / rate).min(1.0);
        for p in buf.as_chunks_mut::<2>().0 {
            let mono = (p[0] as f32 + p[1] as f32) * 0.5;
            // Passa-altas de 1 polo: o cone não faz grave.
            self.hp_out = hp * (self.hp_out + mono - self.hp_in);
            self.hp_in = mono;
            // Dois passa-baixas: o agudo morre na caixa.
            self.lp1 += lp * (self.hp_out - self.lp1);
            self.lp2 += lp * (self.lp1 - self.lp2);
            // Ganho para compensar o grave perdido, com a saturação suave
            // do amplificador pequeno.
            let x = self.lp2 * 1.6 / 32768.0;
            let y = (x / (1.0 + x.abs() * 0.6)) * 32000.0;
            let s = y.clamp(-32000.0, 32000.0) as i16;
            p[0] = s;
            p[1] = s;
        }
    }
}

/// O burn-in: a média lenta do que fica parado na tela e o desgaste que
/// ela deixa no fósforo, numa grade baixa.
struct Burn {
    avg: Vec<f32>,
    wear: Vec<f32>,
    last: Instant,
    /// Os pixels RGBA do fantasma, reaproveitados entre as atualizações.
    rgba: Vec<u8>,
    tex: Option<Texture>,
    /// Há desgaste visível? (sem nada, nem desenha.)
    visible: bool,
}

impl Burn {
    fn new() -> Self {
        Self {
            avg: vec![0.0; BURN_W * BURN_H],
            wear: vec![0.0; BURN_W * BURN_H],
            last: Instant::now(),
            rgba: vec![0; BURN_W * BURN_H * 4],
            tex: None,
            visible: false,
        }
    }

    /// Um passo de `hours` horas com a imagem `luma` (a grade, 0..1):
    /// o que fica claro e parado desgasta, e o desgaste some devagar.
    fn step(&mut self, luma: &[f32], hours: f32) {
        // Em passos de no máximo ~1 minuto: com o tempo acelerado (teste)
        // o passo grande desestabilizaria a conta.
        let n = (hours * 60.0).ceil().max(1.0) as usize;
        let h = hours / n as f32;
        // A média segue a imagem em ~1 minuto: só o que fica parado marca.
        let follow = (h * 60.0).min(1.0);
        // Cerca de 2 horas para marcar forte; some com meia-vida de ~1 hora.
        let (grow, fade) = (h / 1.2, h / 1.5);
        for _ in 0..n.min(600) {
            for ((a, w), &l) in self.avg.iter_mut().zip(&mut self.wear).zip(luma) {
                *a += (l - *a) * follow;
                *w = (*w + grow * (*a - 0.3).max(0.0) - fade * *w).clamp(0.0, 1.0);
            }
        }
    }

    /// O fantasma em RGBA: só o desgaste ACIMA da média aparece (o tubo
    /// gasto por igual não marca nada), num cinza quente bem fraco.
    fn paint(&mut self) {
        let mean = self.wear.iter().sum::<f32>() / self.wear.len() as f32;
        let mut any = false;
        for (px, &w) in self.rgba.as_chunks_mut::<4>().0.iter_mut().zip(&self.wear) {
            let a = ((w - mean).max(0.0) * 0.25).min(0.13);
            any |= a > 0.01;
            *px = [196, 186, 164, (a * 255.0) as u8];
        }
        self.visible = any;
    }
}

/// O estado da TV e do quarto, dentro do [`Cabinet`].
pub(super) struct TvExtras {
    pub picture: Picture,
    /// O menu OSD aberto: o item em ajuste e o último toque.
    menu: Option<(usize, Instant)>,
    pub sound: TvSound,
    burn_on: bool,
    burn: Option<Burn>,
    /// Acelera o burn-in para testar (`SIXTEEN_BURN_RAPIDO=N`).
    burn_speed: f32,
    luma: Vec<f32>,
    flyback: bool,
    amb: Option<AudioOut>,
    amb_buf: Vec<i16>,
    amb_rng: u32,
    amb_phase: f32,
    /// Os estalos do vidro ainda por vir, depois de ligar.
    ticks: Vec<Instant>,
    /// O estalo tocando: (envelope, fase, passo).
    tick: Option<(f32, f32, f32)>,
    /// O brilho médio da imagem, suavizado (0..1), e o alvo dele.
    pub lum: f32,
    lum_target: f32,
    lum_sampled: Instant,
}

impl TvExtras {
    pub fn new() -> Self {
        let env_f32 = |k: &str| std::env::var(k).ok().and_then(|v| v.parse::<f32>().ok());
        let mut me = Self {
            picture: Picture::default(),
            menu: None,
            sound: TvSound::Stereo,
            burn_on: false,
            burn: None,
            burn_speed: env_f32("SIXTEEN_BURN_RAPIDO").unwrap_or(1.0).max(1.0),
            luma: vec![0.0; BURN_W * BURN_H],
            flyback: false,
            amb: None,
            amb_buf: Vec::new(),
            amb_rng: 0x1F2E_3D4C,
            amb_phase: 0.0,
            ticks: Vec::new(),
            tick: None,
            lum: 0.2,
            lum_target: 0.2,
            lum_sampled: Instant::now(),
        };
        // O app abre com a TV ligando: o vidro também estala.
        me.schedule_ticks();
        me
    }

    fn rand(&mut self) -> f32 {
        self.amb_rng ^= self.amb_rng << 13;
        self.amb_rng ^= self.amb_rng >> 17;
        self.amb_rng ^= self.amb_rng << 5;
        (self.amb_rng >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Os estalos do vidro esquentando: uns dez nos primeiros minutos,
    /// cada vez mais espaçados.
    pub fn schedule_ticks(&mut self) {
        let now = Instant::now();
        let mut t = 2.0;
        self.ticks.clear();
        for k in 0..10 {
            t += 1.5 + (k * k) as f32 * 0.7 + self.rand() * 4.0;
            self.ticks.push(now + Duration::from_secs_f32(t));
        }
    }

    // --- Menu OSD ------------------------------------------------------

    /// MENU: abre no primeiro item, passa para o próximo, e depois do
    /// último fecha.
    pub fn menu_press(&mut self) {
        let next = match self.menu {
            None => Some(0),
            Some((i, _)) if i + 1 < Picture::ITEMS => Some(i + 1),
            Some(_) => None,
        };
        self.menu = next.map(|i| (i, Instant::now()));
    }

    /// VOL-/VOL+ com o menu aberto ajustam o item; `false` = menu fechado
    /// (o toque é volume mesmo).
    pub fn menu_adjust(&mut self, delta: i32) -> bool {
        let Some((item, _)) = self.menu_open() else {
            return false;
        };
        let v = self.picture.get(item) as i32 + delta * MENU_STEP as i32;
        self.picture.set(item, v.clamp(0, 100) as u8);
        self.menu = Some((item, Instant::now()));
        true
    }

    pub fn menu_open(&mut self) -> Option<(usize, Instant)> {
        if self.menu.is_some_and(|(_, t)| t.elapsed() >= MENU_IDLE) {
            self.menu = None;
        }
        self.menu
    }

    pub fn close_menu(&mut self) {
        self.menu = None;
    }

    // --- Burn-in -------------------------------------------------------

    pub fn set_burn_in(&mut self, on: bool) {
        self.burn_on = on;
        if !on {
            self.burn = None;
        }
    }

    /// Amostra o quadro do jogo: o brilho médio (para a luz azul do tubo
    /// no quarto) a cada 100 ms e, com o burn-in ligado, o desgaste a
    /// cada segundo.
    pub fn sample_frame(&mut self, frame: &FrameRef) {
        if self.lum_sampled.elapsed() < Duration::from_millis(100) {
            return;
        }
        self.lum_sampled = Instant::now();
        let (w, h) = (frame.width as usize, frame.height as usize);
        if w == 0 || h == 0 {
            return;
        }
        let bpp = frame.format.bytes_per_pixel();
        let pitch = if frame.pitch == 0 {
            w * bpp
        } else {
            frame.pitch
        };
        let luma_at = |x: usize, y: usize| -> f32 {
            let i = y * pitch + x * bpp;
            let Some(px) = frame.pixels.get(i..i + bpp) else {
                return 0.0;
            };
            let (r, g, b) = match frame.format {
                PixelFormat::Rgb565 => {
                    let v = u16::from_le_bytes([px[0], px[1]]);
                    (
                        ((v >> 11) & 31) as f32 / 31.0,
                        ((v >> 5) & 63) as f32 / 63.0,
                        (v & 31) as f32 / 31.0,
                    )
                }
                PixelFormat::Rgb1555 => {
                    let v = u16::from_le_bytes([px[0], px[1]]);
                    (
                        ((v >> 10) & 31) as f32 / 31.0,
                        ((v >> 5) & 31) as f32 / 31.0,
                        (v & 31) as f32 / 31.0,
                    )
                }
                PixelFormat::Xrgb8888 => (
                    px[2] as f32 / 255.0,
                    px[1] as f32 / 255.0,
                    px[0] as f32 / 255.0,
                ),
            };
            0.3 * r + 0.59 * g + 0.11 * b
        };

        let burn_due = self.burn_on
            && self
                .burn
                .as_ref()
                .is_none_or(|b| b.last.elapsed() >= Duration::from_secs(1));
        if burn_due {
            for gy in 0..BURN_H {
                for gx in 0..BURN_W {
                    self.luma[gy * BURN_W + gx] = luma_at(
                        gx * w / BURN_W + w / BURN_W / 2,
                        gy * h / BURN_H + h / BURN_H / 2,
                    );
                }
            }
            self.lum_target = self.luma.iter().sum::<f32>() / self.luma.len() as f32;
            let speed = self.burn_speed;
            let burn = self.burn.get_or_insert_with(Burn::new);
            let hours = burn.last.elapsed().as_secs_f32().min(5.0) / 3600.0 * speed;
            burn.last = Instant::now();
            burn.step(&self.luma, hours);
            burn.paint();
            if let (Some(tex), true) = (&mut burn.tex, burn.visible) {
                let _ = tex.update(None, &burn.rgba, BURN_W * 4);
            }
        } else {
            let mut sum = 0.0;
            for gy in 0..12 {
                for gx in 0..16 {
                    sum += luma_at(gx * w / 16 + w / 32, gy * h / 12 + h / 24);
                }
            }
            self.lum_target = sum / 192.0;
        }
    }

    /// O alvo do brilho médio fora do jogo (a neve, a estante).
    pub fn set_lum_target(&mut self, v: f32) {
        self.lum_target = v;
    }

    /// Um quadro: o brilho segue o alvo com um atraso de olho (~0,3 s).
    pub fn tick_lum(&mut self, tv_on: bool) {
        let target = if tv_on { self.lum_target } else { 0.0 };
        self.lum += (target - self.lum) * 0.08;
    }

    // --- Som do tubo ---------------------------------------------------

    pub fn set_flyback(&mut self, on: bool) {
        self.flyback = on;
    }

    /// Cala o som do tubo (a janela sumiu): fecha o stream — a fila vai
    /// junto e o dispositivo de áudio pode dormir. Reabre sozinho no
    /// próximo estalo/flyback (`feed_sound`).
    pub(super) fn silence_sound(&mut self) {
        self.amb = None;
    }

    /// O menu OSD de imagem está na tela (ele some sozinho por tempo —
    /// `MENU_IDLE` —, então um quadro com ele não é estático).
    pub(super) fn menu_visible(&self) -> bool {
        self.menu.is_some()
    }

    /// Alimenta o stream do tubo: o flyback e os estalos —
    /// fora do volume da TV (é o aparelho fazendo barulho, não o som do
    /// canal). Sem nada a tocar, não abre nem enfileira nada.
    pub fn feed_sound(&mut self, audio: &AudioSubsystem, tv_on: bool, hidden: bool) {
        // Janela minimizada: os menus caem para 10 Hz e a fila de ~94 ms
        // esvaziaria entre um quadro e outro (estalos periódicos). Silêncio
        // — e o que já estava na fila sai junto. Os estalos que vencem
        // escondidos são descartados (senão tocariam em rajada, um por
        // quadro, quando a janela voltasse).
        if hidden {
            self.silence_sound();
            let now = Instant::now();
            self.ticks.retain(|t| *t > now);
            return;
        }
        let now = Instant::now();
        let tick_due = tv_on && self.ticks.first().is_some_and(|t| now >= *t);
        if tick_due {
            self.ticks.remove(0);
            let freq = 1800.0 + self.rand() * 2200.0;
            let amp = 0.5 + self.rand() * 0.5;
            self.tick = Some((amp, 0.0, std::f32::consts::TAU * freq / AMB_RATE as f32));
        }
        let whine = self.flyback && tv_on;
        if !whine && self.tick.is_none() {
            // Nada a tocar: com a fila já tocada até o fim, fecha o stream
            // (aberto, ele segura o dispositivo do SDL e o sink do PipeWire
            // acordados — codec e amplificador ligados no Deck mesmo nos
            // menus parados). O próximo estalo/flyback reabre.
            if amb_can_close(self.amb.as_ref().map(|a| a.queued_frames())) {
                self.amb = None;
            }
            return;
        }
        if self.amb.is_none() {
            match AudioOut::new(audio, AMB_RATE) {
                Ok(a) => self.amb = Some(a),
                Err(e) => {
                    log::warn!("som do tubo: {e}");
                    self.flyback = false;
                    self.tick = None;
                    return;
                }
            }
        }
        let Some(amb) = &self.amb else { return };
        // ~60 ms na fila: o bastante para não engasgar entre quadros.
        let want = (AMB_RATE / 16) as usize;
        let queued = amb.queued_frames();
        if queued >= want {
            return;
        }
        let n = want * 3 / 2 - queued;
        let dt = 1.0 / AMB_RATE as f32;
        let step = std::f32::consts::TAU * FLYBACK_HZ * dt;
        self.amb_buf.clear();
        for _ in 0..n {
            let mut v = 0.0f32;
            if whine {
                self.amb_phase = (self.amb_phase + step) % std::f32::consts::TAU;
                v += self.amb_phase.sin() * 140.0;
            }
            if let Some((env, ph, st)) = &mut self.tick {
                *ph += *st;
                v += ph.sin() * *env * 5200.0;
                *env *= 0.9955;
                if *env < 0.003 {
                    self.tick = None;
                }
            }
            let s = v.clamp(-30000.0, 30000.0) as i16;
            self.amb_buf.push(s);
            self.amb_buf.push(s);
        }
        amb.queue(&self.amb_buf);
    }
}

impl Cabinet {
    /// Os ajustes de imagem do menu OSD (o app passa para o filtro NTSC e
    /// salva na configuração).
    pub fn picture(&self) -> Picture {
        self.tvx.picture
    }

    pub fn set_picture(&mut self, p: Picture) {
        self.tvx.picture = p;
    }

    pub fn tv_sound(&self) -> TvSound {
        self.tvx.sound
    }

    pub fn set_tv_sound(&mut self, s: TvSound) {
        self.tvx.sound = s;
    }

    pub fn set_burn_in(&mut self, on: bool) {
        self.tvx.set_burn_in(on);
    }

    pub fn set_flyback(&mut self, on: bool) {
        self.tvx.set_flyback(on);
    }

    /// Os sons do tubo, uma vez por quadro apresentado.
    pub(super) fn feed_tv_sound(&mut self) {
        let on = self.tv_on();
        self.tvx.tick_lum(on);
        self.tvx.feed_sound(&self.audio, on, self.window_hidden);
    }

    /// Deixa pronto o que o vidro desenha neste quadro: a textura do
    /// burn-in criada, a hora relida e o menu fechado se a TV apagou ou
    /// ele expirou.
    pub(super) fn prepare_glass(&mut self) {
        if !self.tv_on() {
            self.tvx.close_menu();
        }
        self.tvx.menu_open();
        let Some(burn) = &mut self.tvx.burn else {
            return;
        };
        if burn.visible && burn.tex.is_none() {
            if let Ok(mut tex) = self.canvas.create_texture_streaming(
                SdlFormat::RGBA32,
                BURN_W as u32,
                BURN_H as u32,
            ) {
                tex.set_blend_mode(BlendMode::Blend);
                tex.set_scale_mode(SdlScaleMode::Linear);
                let _ = tex.update(None, &burn.rgba, BURN_W * 4);
                burn.tex = Some(tex);
            }
        }
    }

    /// O vidro por cima da imagem no frame vivo (as capturas chamam
    /// [`draw_glass`] direto com o próprio canvas).
    pub(super) fn draw_glass_extras(&mut self, mesh: &CrtMesh, screen: Rect) {
        self.prepare_glass();
        let view = self.tvx.glass_view(self.tv_on());
        draw_glass(&mut self.canvas, &mut self.font, mesh, screen, &view);
    }
}

/// O que o vidro mostra num quadro, emprestado do [`TvExtras`].
pub(super) struct GlassView<'a> {
    burn: Option<&'a Texture>,
    menu: Option<(usize, Picture)>,
}

impl TvExtras {
    pub fn glass_view(&self, tv_on: bool) -> GlassView<'_> {
        GlassView {
            burn: self
                .burn
                .as_ref()
                .filter(|b| b.visible && tv_on)
                .and_then(|b| b.tex.as_ref()),
            menu: self.menu.filter(|_| tv_on).map(|(i, _)| (i, self.picture)),
        }
    }
}

/// O stream do tubo pode fechar? Só aberto (`Some`) e com a fila vazia
/// — o que ainda está na fila toca até o fim antes.
fn amb_can_close(queued: Option<usize>) -> bool {
    queued == Some(0)
}

/// O que vai no vidro por cima da imagem: o brilho, o fantasma do
/// burn-in e o menu OSD.
pub(super) fn draw_glass(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    mesh: &CrtMesh,
    screen: Rect,
    view: &GlassView,
) {
    draw_glass_sheen(canvas, screen);
    if let Some(tex) = view.burn {
        let _ = canvas.render_geometry(&mesh.verts, Some(tex), &mesh.indices[..]);
    }
    if let Some((item, picture)) = view.menu {
        draw_picture_menu(canvas, font, screen, picture, item);
    }
}

/// A malha do brilho do vidro, pronta: depende só do retângulo do tubo e
/// da curvatura (`crt_warp()`) — as cores são constantes, nada por quadro.
struct SheenMesh {
    /// (x, y, w, h do tubo, bits do warp): mudou qualquer um, refaz.
    key: (i32, i32, u32, u32, u32),
    verts: Vec<Vertex>,
    idx: Vec<i32>,
}

thread_local! {
    /// O cache da malha do brilho. `draw_glass` é função livre (as capturas
    /// não têm `&mut self`), então o cache mora aqui; tudo roda na thread
    /// de UI (o canvas do SDL não sai dela).
    static SHEEN: std::cell::RefCell<Option<SheenMesh>> =
        const { std::cell::RefCell::new(None) };
}

/// A chave do cache do brilho: a posição entra porque os vértices são
/// absolutos no canvas; o warp entra em bits (o ajuste da curvatura muda
/// a geometria sem mexer no tamanho).
fn sheen_key(screen: Rect, warp: f32) -> (i32, i32, u32, u32, u32) {
    (
        screen.x(),
        screen.y(),
        screen.width(),
        screen.height(),
        warp.to_bits(),
    )
}

/// O brilho do vidro, sempre presente (a TV ligada ou não): uma mancha
/// de luz difusa no alto à esquerda, uma faixa diagonal mais fina e um
/// aro claro rente à borda — tudo sobre a mesma grade curva da imagem,
/// para o vidro parecer abaulado e não um adesivo plano. A malha (625
/// vértices com exp/powi) fica em cache e só é refeita quando o tubo ou o
/// warp mudam — antes eram ~35 KB alocados e recalculados todo quadro.
fn draw_glass_sheen(canvas: &mut WindowCanvas, screen: Rect) {
    let warp = crt_warp();
    let key = sheen_key(screen, warp);
    SHEEN.with_borrow_mut(|cache| {
        if !cache.as_ref().is_some_and(|m| m.key == key) {
            *cache = Some(build_glass_sheen(screen, warp, key));
        }
        let m = cache.as_ref().unwrap();
        canvas.set_blend_mode(BlendMode::Blend);
        let _ = canvas.render_geometry(&m.verts, None::<&Texture>, &m.idx[..]);
    });
}

fn build_glass_sheen(screen: Rect, warp: f32, key: (i32, i32, u32, u32, u32)) -> SheenMesh {
    const N: usize = 24;
    let (sx, sy) = (screen.x() as f32, screen.y() as f32);
    let (sw, sh) = (screen.width() as f32, screen.height() as f32);
    let z = sdl3::render::FPoint::new(0.0, 0.0);
    let mut verts = Vec::with_capacity((N + 1) * (N + 1));
    for j in 0..=N {
        for i in 0..=N {
            let (u, v) = (i as f32 / N as f32, j as f32 / N as f32);
            let (cx, cy) = (u * 2.0 - 1.0, v * 2.0 - 1.0);
            let dx = cx * (1.0 - warp * cy * cy);
            let dy = cy * (1.0 - warp * cx * cx);
            // Mancha: gaussiana larga centrada em (0,22; 0,20).
            let (mx, my) = ((u - 0.22) / 0.30, (v - 0.20) / 0.26);
            let blob = (-(mx * mx + my * my)).exp() * 0.10;
            // Faixa diagonal: some ao se afastar da reta u + v = 0,62.
            let d = (u + v - 0.62) / 0.07;
            let streak = (-(d * d)).exp() * 0.02 * (1.0 - v).clamp(0.0, 1.0);
            // Aro: luz que pega na borda do vidro curvo.
            let edge = cx.abs().max(cy.abs());
            let rim = ((edge - 0.90) / 0.10).clamp(0.0, 1.0).powi(2) * 0.05;
            let a = (blob + streak + rim).min(0.20);
            verts.push(Vertex {
                position: sdl3::render::FPoint::new(
                    sx + (dx * 0.5 + 0.5) * sw,
                    sy + (dy * 0.5 + 0.5) * sh,
                ),
                color: FColor::RGBA(0.96, 0.98, 1.0, a),
                tex_coord: z,
            });
        }
    }
    let stride = (N + 1) as i32;
    let mut idx = Vec::with_capacity(N * N * 6);
    for j in 0..N as i32 {
        for i in 0..N as i32 {
            let a = j * stride + i;
            idx.extend_from_slice(&[a, a + stride, a + 1, a + 1, a + stride, a + stride + 1]);
        }
    }
    SheenMesh { key, verts, idx }
}

/// O menu de imagem dos anos 90: letras verdes grandes com sombra, o item
/// em ajuste aceso e os outros apagados, cada um com a barrinha de
/// segmentos e o número.
fn draw_picture_menu(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    screen: Rect,
    picture: Picture,
    selected: usize,
) {
    let scale = if screen.height() >= 520 { 2 } else { 1 };
    let cell_w = (GLYPH_W * scale) as i32;
    let row = (GLYPH_H * scale) as i32 + 10;
    let x = screen.x() + 40;
    let mut y = screen.y() + 40;
    let dim = (36, 150, 48);
    let shadow = (12, 14, 12);
    let text = |canvas: &mut WindowCanvas, font: &mut Texture, x: i32, y: i32, c, s: &str| {
        draw_text_absolute(
            canvas,
            font,
            x + 2,
            y + 2,
            TextStyle::new(scale, shadow),
            s,
            usize::MAX,
        );
        draw_text_absolute(canvas, font, x, y, TextStyle::new(scale, c), s, usize::MAX);
    };
    text(canvas, font, x, y, OSD_GREEN, tr!("PICTURE", "IMAGEM"));
    y += row + 6;
    let label_cols = 11;
    let segs = 20;
    let seg_w = (6 * scale) as i32;
    let seg_h = (GLYPH_H * scale) as i32 / 2;
    for item in 0..Picture::ITEMS {
        let on = item == selected;
        let c = if on { OSD_GREEN } else { dim };
        let mark = if on { ">" } else { " " };
        text(
            canvas,
            font,
            x,
            y,
            c,
            &format!("{mark}{}", Picture::label(item)),
        );
        let v = picture.get(item);
        let bx = x + cell_w * (label_cols + 1);
        let by = y + ((GLYPH_H * scale) as i32 - seg_h) / 2;
        let lit = (v as i32 * segs + 50) / 100;
        for s in 0..segs {
            let sx = bx + s * (seg_w + 2);
            canvas.set_draw_color(Color::RGB(shadow.0, shadow.1, shadow.2));
            let _ = canvas.fill_rect(Rect::new(sx + 2, by + 2, seg_w as u32, seg_h as u32));
            let col = if s < lit { c } else { (20, 34, 22) };
            canvas.set_draw_color(Color::RGB(col.0, col.1, col.2));
            let _ = canvas.fill_rect(Rect::new(sx, by, seg_w as u32, seg_h as u32));
        }
        text(
            canvas,
            font,
            bx + segs * (seg_w + 2) + 10,
            y,
            c,
            &format!("{v:3}"),
        );
        y += row;
    }
    y += 6;
    let hint = tr!(
        "MENU: next   VOL -/+: adjust",
        "MENU: próximo   VOL -/+: ajusta"
    );
    draw_text_absolute(
        canvas,
        font,
        x + 1,
        y + 1,
        TextStyle::new(1, shadow),
        hint,
        usize::MAX,
    );
    draw_text_absolute(canvas, font, x, y, TextStyle::new(1, dim), hint, usize::MAX);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amb_closes_only_open_and_drained() {
        assert!(!amb_can_close(None), "fechado já está");
        assert!(!amb_can_close(Some(120)), "a fila ainda toca");
        assert!(amb_can_close(Some(0)));
    }

    #[test]
    fn silence_drops_the_stream_and_menu_counts_as_timed() {
        let mut tv = TvExtras::new();
        tv.silence_sound();
        assert!(tv.amb.is_none());
        assert!(!tv.menu_visible());
        tv.menu_press();
        assert!(tv.menu_visible(), "o menu some por tempo: não é estático");
    }

    #[test]
    fn menu_cycles_and_adjusts() {
        let mut tv = TvExtras::new();
        assert!(!tv.menu_adjust(1), "fechado, VOL é volume");
        tv.menu_press();
        assert!(tv.menu_adjust(1));
        assert_eq!(tv.picture.brilho, 55);
        tv.menu_press();
        for _ in 0..30 {
            tv.menu_adjust(-1);
        }
        assert_eq!(tv.picture.contraste, 0);
        for _ in 0..3 {
            tv.menu_press();
        }
        assert!(tv.menu_open().is_none(), "depois do último item fecha");
    }

    #[test]
    fn burn_marks_what_stays_still_and_fades() {
        let mut b = Burn::new();
        // Metade da tela clara e parada (o HUD), metade escura.
        let luma: Vec<f32> = (0..BURN_W * BURN_H)
            .map(|i| if i < BURN_W * 10 { 0.95 } else { 0.1 })
            .collect();
        for _ in 0..180 {
            b.step(&luma, 1.0 / 60.0); // 3 horas, de minuto em minuto
        }
        b.paint();
        assert!(b.visible);
        assert!(b.rgba[3] > 20, "o HUD marcou");
        assert_eq!(b.rgba[BURN_W * 50 * 4 + 3], 0, "o resto não");
        let dark = vec![0.1; BURN_W * BURN_H];
        for _ in 0..360 {
            b.step(&dark, 1.0 / 60.0); // 6 horas em outro jogo
        }
        b.paint();
        assert!(b.rgba[3] < 8, "sumiu com o tempo");
    }

    #[test]
    fn speaker_is_mono_and_cuts_the_bass() {
        let mut sp = TvSpeaker::default();
        let rate = 32_000;
        // 40 Hz só na esquerda: no alto-falante sai mono e quase nada.
        let mut buf: Vec<i16> = (0..rate)
            .flat_map(|i| {
                let s = ((i as f32 * 40.0 / rate as f32 * std::f32::consts::TAU).sin() * 20000.0)
                    as i16;
                [s, 0]
            })
            .collect();
        sp.process(&mut buf, rate as u32);
        let tail = &buf[buf.len() / 2..];
        assert!(tail.chunks(2).all(|p| p[0] == p[1]));
        let peak = tail.iter().map(|s| s.unsigned_abs()).max().unwrap();
        assert!(peak < 4000, "grave passou: {peak}");
    }
}
