//! O canal 4 (plan revision: "melhorar efeito de interferencia no canal 4 -
//! deixe o mais real possivel"): o modulador RF do console sai no canal 3,
//! e a TV sintonizada no canal vizinho só pega a borda do sinal. O que se
//! via de verdade, e o que este módulo reproduz no próprio sinal (antes do
//! tubo, não uma camada por cima):
//!
//! - **sinal fraco**: imagem apagada e lavada, o preto subindo (o AGC da TV
//!   puxando ganho), e a força oscilando com quedas curtas (fading);
//! - **neve**: ruído de luminância esticado na horizontal (a banda do vídeo
//!   é estreita), com riscos brancos curtos e um pouco de neve colorida;
//! - **vertical no limite**: travado quase sempre, mas de tempos em tempos
//!   o sincronismo escapa e a imagem dá uma volta, com a faixa preta do
//!   retraço vertical passando;
//! - **horizontal instável**: linhas tremendo, o topo do quadro entortando
//!   logo depois do sincronismo (flagging) e rasgos em faixa de vez em quando;
//! - **fantasma**: uma cópia fraca deslocada para a direita (multipercurso);
//! - **cor sumindo**: o color killer liga e desliga, e o que sobra de croma
//!   gira de matiz;
//! - **batimento**: listras diagonais finas andando (a portadora do canal
//!   vizinho) e a barra de hum rolando.
//!
//! Tudo na dose de quem ainda consegue jogar (plan revision: "reduza um
//! pouco a degradação ... tem que ser possivel jogar ainda"): o sinal é
//! fraco, não perdido.
//!
//! O áudio acompanha ([`RfAudio`]): o som do jogo abafado e mais baixo,
//! chiado e o zumbido do vertical, seguindo a mesma força de sinal.

use std::time::Instant;

use super::PixelFormat;

/// Quantas linhas o retraço vertical ocupa, como fração da altura — a
/// faixa preta que passa quando a imagem rola.
const VBI_FRAC: f32 = 0.085;

/// xorshift32 — ruído barato, por pixel.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> u32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0
    }

    /// Uniforme em 0..1.
    fn unit(&mut self) -> f32 {
        (self.next() >> 8) as f32 / (1u32 << 24) as f32
    }
}

/// O estado do sinal degradado entre quadros (rolagem, rasgos, quedas) e os
/// buffers reaproveitados.
pub(super) struct RfDetune {
    rng: Rng,
    t0: Instant,
    /// Deslocamento vertical acumulado, em linhas do quadro.
    roll: f32,
    roll_speed: f32,
    /// Quadros até o vertical decidir de novo (rolar, travar, escorregar).
    roll_timer: u32,
    /// Rasgo horizontal em curso: quadros restantes, linha central, força.
    tear: u32,
    tear_y: f32,
    tear_amp: f32,
    /// Queda de sinal em curso: quadros restantes e profundidade.
    fade: u32,
    fade_depth: f32,
    /// A força do sinal do último quadro (0..1) — o áudio segue esta.
    signal: f32,
    y: Vec<f32>,
    i: Vec<f32>,
    q: Vec<f32>,
    /// A luminância de uma linha já borrada e com o fantasma somado.
    line: Vec<f32>,
    /// sin/cos do batimento por coluna (a fase da linha entra por soma).
    beat_s: Vec<f32>,
    beat_c: Vec<f32>,
    pub(super) out: Vec<u8>,
}

impl RfDetune {
    pub(super) fn new() -> Self {
        Self {
            rng: Rng(0x1234_5679),
            t0: Instant::now(),
            roll: 0.0,
            roll_speed: 0.0,
            roll_timer: 120,
            tear: 0,
            tear_y: 0.0,
            tear_amp: 0.0,
            fade: 0,
            fade_depth: 0.0,
            signal: 0.58,
            y: Vec::new(),
            i: Vec::new(),
            q: Vec::new(),
            line: Vec::new(),
            beat_s: Vec::new(),
            beat_c: Vec::new(),
            out: Vec::new(),
        }
    }

    /// A força do sinal do último quadro degradado.
    pub(super) fn signal(&self) -> f32 {
        self.signal
    }

    fn next(&mut self) -> u32 {
        self.rng.next()
    }

    fn unit(&mut self) -> f32 {
        self.rng.unit()
    }

    /// Avança o relógio do sinal um quadro: força, vertical, rasgos.
    fn step(&mut self, h: f32) {
        let t = self.t0.elapsed().as_secs_f32();
        // Força: oscila devagar, com quedas curtas de vez em quando.
        if self.fade == 0 && self.unit() < 0.003 {
            self.fade = 6 + (self.next() % 24);
            self.fade_depth = 0.08 + self.unit() * 0.10;
        }
        let dip = if self.fade > 0 {
            self.fade -= 1;
            self.fade_depth
        } else {
            0.0
        };
        self.signal =
            (0.58 + 0.06 * (t * 0.63).sin() + 0.04 * (t * 2.1 + 1.3).sin() - dip).clamp(0.35, 0.72);

        // Vertical: travado quase sempre (dá para jogar); de tempos em
        // tempos o sincronismo escapa, a imagem dá uma volta rápida com a
        // faixa do retraço passando e trava de novo.
        let period = h * (1.0 + VBI_FRAC);
        if self.roll_speed > 0.0 {
            self.roll += self.roll_speed;
            if self.roll >= period {
                self.roll = 0.0;
                self.roll_speed = 0.0;
                self.roll_timer = 480 + self.next() % 900;
            }
        } else if self.roll_timer == 0 {
            self.roll_speed = 5.0 + self.unit() * 4.0;
        } else {
            self.roll_timer -= 1;
        }

        // Rasgo horizontal: uma faixa de linhas puxada para o lado.
        if self.tear > 0 {
            self.tear -= 1;
        } else if self.unit() < 0.004 {
            self.tear = 2 + self.next() % 5;
            self.tear_y = self.unit() * h;
            self.tear_amp = 0.02 + self.unit() * 0.06;
        }
    }

    /// Degrada `pixels` (um quadro do core) e deixa o resultado em
    /// [`RfDetune::out`], XRGB8888 com pitch `w * 4`.
    pub(super) fn process(
        &mut self,
        pixels: &[u8],
        w: u32,
        h: u32,
        pitch: usize,
        format: PixelFormat,
    ) {
        let (w, h) = (w as usize, h as usize);
        if w == 0 || h == 0 {
            return;
        }
        self.decode(pixels, w, h, pitch, format);
        self.step(h as f32);
        let t = self.t0.elapsed().as_secs_f32();
        let s = self.signal;
        // Medidas em "pixels de SNES": o quadro pode vir do filtro NTSC,
        // mais largo, e o efeito tem que ter o mesmo tamanho na tela.
        let sx = w as f32 / 256.0;
        let vbi = (h as f32 * VBI_FRAC) as usize;
        let period = h + vbi;

        let gain = 1.0 + (0.6 - s) * 0.6;
        let lift = 0.04 + (0.6 - s) * 0.08;
        let snow = 0.06 + (0.75 - s) * 0.30;
        // Color killer: com o sinal fraco a cor some; no meio, pisca.
        // A fase da subportadora escorrega um pouco: a matiz desvia e
        // treme em volta da certa, sem dar a volta no círculo.
        let chroma = ((s - 0.30) * 1.4).clamp(0.0, 0.38) * (0.7 + 0.3 * (t * 11.0).sin().abs());
        let hue = 0.35 * (t * 0.4).sin() + 0.15 * (t * 3.1).sin();
        let (hs, hc) = hue.sin_cos();
        let ghost_dx = (9.0 * sx) as usize;
        let ghost = 0.15;
        let corr = if sx > 1.5 { 0.72 } else { 0.5 };
        let beat_fx = std::f32::consts::TAU / (5.3 * sx);
        let beat_fy = std::f32::consts::TAU / 7.0;
        let beat_amp = 0.02 + (0.6 - s).max(0.0) * 0.05;

        let roll = self.roll as usize;
        let (tear, tear_y, tear_amp) = (self.tear, self.tear_y, self.tear_amp);
        let Self {
            rng,
            y: luma_buf,
            i: i_buf,
            q: q_buf,
            line,
            beat_s,
            beat_c,
            out,
            ..
        } = self;
        if beat_s.len() != w {
            *beat_s = (0..w).map(|x| (x as f32 * beat_fx).sin()).collect();
            *beat_c = (0..w).map(|x| (x as f32 * beat_fx).cos()).collect();
        }
        line.resize(w, 0.0);
        out.resize(w * h * 4, 0);
        let blur = (sx.round() as usize).max(1);
        // Os riscos brancos chegam a intervalos aleatórios (geométricos):
        // um sorteio por risco, não um por pixel.
        let p_streak = 0.0004 + (0.6 - s).max(0.0) * 0.002;
        let gap = |rng: &mut Rng| (-(rng.unit().max(1e-6)).ln() / p_streak) as u32;
        let mut until_streak = gap(rng);
        let mut line_jit = 0.0f32;
        for oy in 0..h {
            let sy = (oy + roll) % period;
            let row = &mut out[oy * w * 4..(oy + 1) * w * 4];
            // Retraço vertical: a faixa escura que passa, com a linha mais
            // clara do pulso de sincronismo e a neve por cima.
            if sy >= h {
                let k = sy - h;
                let base = if k == vbi / 2 || k == vbi / 2 + 1 {
                    0.14
                } else {
                    0.0
                };
                let mut n = 0.0f32;
                for px in row.as_chunks_mut::<4>().0 {
                    n = n * corr + (rng.unit() - 0.5) * (1.0 - corr) * 2.0;
                    let v = (base + n * snow * 0.8).clamp(0.0, 1.0);
                    let b = (v * 255.0) as u8;
                    px.copy_from_slice(&[b, b, b, 0]);
                }
                continue;
            }
            // Deslocamento horizontal da linha: tremor, a curva do topo
            // logo depois do sincronismo e o rasgo em faixa.
            line_jit = line_jit * 0.65 + (rng.unit() - 0.5) * 0.9 * sx;
            let flag = 8.0 * sx * (-(sy as f32) / (h as f32 * 0.04)).exp();
            let wobble = 0.7 * sx * (t * 8.0 + sy as f32 * 0.06).sin();
            let mut shift = line_jit + flag + wobble;
            if tear > 0 {
                let d = (sy as f32 - tear_y) / (h as f32 * 0.05);
                if d.abs() < 1.0 {
                    shift += tear_amp * w as f32 * (1.0 - d.abs()) * (1.0 + d);
                }
            }
            let src = sy * w;
            let ly = &luma_buf[src..src + w];
            let li = &i_buf[src..src + w];
            let lq = &q_buf[src..src + w];
            // Banda estreita: a luminância chega borrada na horizontal, e o
            // fantasma (multipercurso) soma uma cópia atrasada.
            for x in 0..w {
                let a = ly[x.saturating_sub(blur)];
                let c = ly[(x + blur).min(w - 1)];
                let g = if x >= ghost_dx { ly[x - ghost_dx] } else { 0.0 };
                line[x] = 0.25 * a + 0.5 * ly[x] + 0.25 * c + ghost * g;
            }
            let sample = |l: &[f32], x: f32| -> f32 {
                if x < 0.0 || x >= (w - 1) as f32 {
                    return 0.0;
                }
                let x0 = x as usize;
                let f = x - x0 as f32;
                l[x0] * (1.0 - f) + l[x0 + 1] * f
            };
            let (row_s, row_c) = (sy as f32 * beat_fy - t * 3.1).sin_cos();
            let hum = 1.0
                - 0.11
                    * (0.5
                        + 0.5 * ((sy as f32 / h as f32 - t * 0.31) * std::f32::consts::TAU).sin());
            let mut n = 0.0f32;
            let mut streak = 0u32;
            let mut ci = 0.0f32;
            let mut cq = 0.0f32;
            for (ox, px) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let x = ox as f32 - shift;
                // Um sorteio por pixel, repartido entre a neve de
                // luminância e as duas de croma.
                let r = rng.next();
                let r0 = (r & 0x3FF) as f32 / 1024.0 - 0.5;
                let r1 = ((r >> 10) & 0x3FF) as f32 / 1024.0 - 0.5;
                let r2 = ((r >> 20) & 0x3FF) as f32 / 1024.0 - 0.5;
                n = n * corr + r0 * (1.0 - corr) * 3.4;
                if until_streak == 0 {
                    streak = 2 + rng.next() % (8 * sx as u32 + 1);
                    until_streak = gap(rng);
                } else {
                    until_streak -= 1;
                }
                let spark = if streak > 0 {
                    streak -= 1;
                    0.55
                } else {
                    0.0
                };
                let beat = beat_amp * (beat_s[ox] * row_c + beat_c[ox] * row_s);
                let luma =
                    (sample(line, x) * s * gain * 1.6 + lift) * hum + beat + n * snow + spark;
                // Croma: o que sobrou, desviado de matiz, mais a neve colorida.
                ci = ci * 0.8 + r1 * 0.06;
                cq = cq * 0.8 + r2 * 0.06;
                let (mut iv, mut qv) = (ci, cq);
                if chroma > 0.0 {
                    let i0 = sample(li, x) * chroma;
                    let q0 = sample(lq, x) * chroma;
                    iv += i0 * hc - q0 * hs;
                    qv += i0 * hs + q0 * hc;
                }
                let r = luma + 0.956 * iv + 0.621 * qv;
                let g = luma - 0.272 * iv - 0.647 * qv;
                let b = luma - 1.106 * iv + 1.703 * qv;
                px.copy_from_slice(&[to_u8(b), to_u8(g), to_u8(r), 0]);
            }
        }
    }

    /// O quadro do core em YIQ (0..1), qualquer que seja o formato.
    fn decode(&mut self, pixels: &[u8], w: usize, h: usize, pitch: usize, format: PixelFormat) {
        let n = w * h;
        self.y.resize(n, 0.0);
        self.i.resize(n, 0.0);
        self.q.resize(n, 0.0);
        let bpp = format.bytes_per_pixel();
        let pitch = if pitch == 0 { w * bpp } else { pitch };
        for yy in 0..h {
            let Some(row) = pixels.get(yy * pitch..yy * pitch + w * bpp) else {
                break;
            };
            for xx in 0..w {
                let p = &row[xx * bpp..xx * bpp + bpp];
                let (r, g, b) = match format {
                    PixelFormat::Xrgb8888 => (
                        p[2] as f32 / 255.0,
                        p[1] as f32 / 255.0,
                        p[0] as f32 / 255.0,
                    ),
                    PixelFormat::Rgb565 => {
                        let v = u16::from_le_bytes([p[0], p[1]]);
                        (
                            ((v >> 11) & 0x1F) as f32 / 31.0,
                            ((v >> 5) & 0x3F) as f32 / 63.0,
                            (v & 0x1F) as f32 / 31.0,
                        )
                    }
                    PixelFormat::Rgb1555 => {
                        let v = u16::from_le_bytes([p[0], p[1]]);
                        (
                            ((v >> 10) & 0x1F) as f32 / 31.0,
                            ((v >> 5) & 0x1F) as f32 / 31.0,
                            (v & 0x1F) as f32 / 31.0,
                        )
                    }
                };
                let k = yy * w + xx;
                self.y[k] = 0.299 * r + 0.587 * g + 0.114 * b;
                self.i[k] = 0.596 * r - 0.274 * g - 0.322 * b;
                self.q[k] = 0.211 * r - 0.523 * g + 0.312 * b;
            }
        }
    }
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0) as u8
}

/// O som do canal 4: o áudio do jogo mais baixo e abafado (a portadora de
/// som do canal vizinho chega fraca), chiado e o zumbido de 60 Hz do
/// vertical, seguindo a força do sinal da imagem.
pub struct RfAudio {
    rng: u32,
    lp_l: f32,
    lp_r: f32,
    phase: f32,
}

impl Default for RfAudio {
    fn default() -> Self {
        Self {
            rng: 0x7F4A_7C15,
            lp_l: 0.0,
            lp_r: 0.0,
            phase: 0.0,
        }
    }
}

impl RfAudio {
    /// Degrada `samples` (estéreo intercalado, `rate` Hz) com o sinal
    /// `signal` (0..1) e o volume da TV (`vol`/16), escrevendo em `out`.
    pub fn process(
        &mut self,
        samples: &[i16],
        rate: u32,
        signal: f32,
        vol: u8,
        out: &mut Vec<i16>,
    ) {
        out.clear();
        let rate = rate.max(8000) as f32;
        let vol = vol as f32 / 16.0;
        let s = signal.clamp(0.0, 1.0);
        // O jogo: atrás do chiado, sem agudos.
        let game = (0.45 + s * 0.55) * vol;
        let a = (std::f32::consts::TAU * 3800.0 / rate).min(1.0);
        let hiss = (900.0 + (0.75 - s).max(0.0) * 3000.0) * vol;
        let buzz = (300.0 + (0.75 - s).max(0.0) * 600.0) * vol;
        let step = 59.94 / rate;
        for p in samples.as_chunks::<2>().0 {
            self.lp_l += a * (p[0] as f32 - self.lp_l);
            self.lp_r += a * (p[1] as f32 - self.lp_r);
            self.rng ^= self.rng << 13;
            self.rng ^= self.rng >> 17;
            self.rng ^= self.rng << 5;
            let n = ((self.rng >> 8) & 0xFFFF) as f32 / 32768.0 - 1.0;
            // O zumbido do vertical: um pulso estreito a 59,94 Hz (rico em
            // harmônicos, o "bzzz" do sincronismo vazando no som).
            self.phase = (self.phase + step).fract();
            let z = if self.phase < 0.08 { 1.0 } else { -0.087 };
            let noise = n * hiss + z * buzz;
            let l = self.lp_l * game + noise;
            let r = self.lp_r * game + noise;
            out.push(l.clamp(-32000.0, 32000.0) as i16);
            out.push(r.clamp(-32000.0, 32000.0) as i16);
        }
    }
}
