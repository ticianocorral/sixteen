//! O "visual da tela": cada look é um preset de sinal (no app, o filtro
//! NTSC), uma camada de efeitos por cima da imagem — scanlines, grade de
//! apertura, grade de pixels de LCD, brilho suave — e uma curvatura do
//! tubo. Os efeitos são multiplicados (`Mul`) sobre a MESMA malha curva
//! da imagem, com texturas pequenas geradas uma vez (só refeitas ao
//! mudar o tamanho da janela ou o número de linhas do jogo), então o
//! custo por quadro é poucos desenhos da malha de 1.089 vértices.

use super::*;

/// Os visuais selecionáveis. `Rf` é o padrão: a TV de antena de sempre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScreenLook {
    /// A TV de antena: composto mole com ruído de RF e tubo curvo.
    #[default]
    Rf,
    /// Vídeo composto: cores vazando, sem o ruído da antena.
    Composite,
    /// S-Video: mais limpo, com scanlines leves e um brilho suave.
    SVideo,
    /// Monitor profissional RGB (PVM): imagem limpa, scanlines e máscara.
    Pvm,
    /// Trinitron: grade de apertura e cores vivas.
    Trinitron,
    /// LCD nítido: pixels quadrados com grade, sem curvatura.
    Lcd,
    /// TV preto e branco.
    Mono,
}

/// As linhas e a máscara por cima da imagem, escolhidas à parte do sinal:
/// dá para ter a TV de antena (RF) com as scanlines de um monitor RGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScreenLines {
    /// As do próprio visual (o padrão).
    #[default]
    Auto,
    /// Nenhuma.
    Off,
    /// Scanlines leves.
    Light,
    /// As do monitor RGB (PVM): scanlines fortes e máscara RGB.
    Pvm,
    /// Grade de apertura (Trinitron).
    Grille,
    /// Grade de pixels de LCD.
    Grid,
}

impl ScreenLines {
    pub const ALL: [ScreenLines; 6] = [
        ScreenLines::Auto,
        ScreenLines::Off,
        ScreenLines::Light,
        ScreenLines::Pvm,
        ScreenLines::Grille,
        ScreenLines::Grid,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ScreenLines::Auto => "auto",
            ScreenLines::Off => "off",
            ScreenLines::Light => "light",
            ScreenLines::Pvm => "pvm",
            ScreenLines::Grille => "grille",
            ScreenLines::Grid => "grid",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|l| l.as_str() == s)
    }

    pub fn label(self) -> &'static str {
        match self {
            ScreenLines::Auto => tr!("from the look", "do visual"),
            ScreenLines::Off => tr!("none", "nenhuma"),
            ScreenLines::Light => tr!("light scanlines", "scanlines leves"),
            ScreenLines::Pvm => tr!("RGB / PVM scanlines", "scanlines RGB / PVM"),
            ScreenLines::Grille => tr!("aperture grille", "grade de apertura"),
            ScreenLines::Grid => tr!("LCD pixel grid", "grade de LCD"),
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|&l| l == self).unwrap_or(0)
    }

    pub fn cycle(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn cycle_back(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    /// Os efeitos de verdade: `own` são os do visual, usados no `Auto`.
    /// O brilho suave (bloom) fica sempre com o visual.
    pub fn apply(self, own: LookFx) -> LookFx {
        let z = LookFx {
            bloom: own.bloom,
            ..LookFx::default()
        };
        match self {
            ScreenLines::Auto => own,
            ScreenLines::Off => z,
            ScreenLines::Light => LookFx {
                scanlines: 0.20,
                ..z
            },
            ScreenLines::Pvm => LookFx {
                scanlines: 0.38,
                mask: 0.18,
                ..z
            },
            ScreenLines::Grille => LookFx {
                scanlines: 0.10,
                mask: 0.30,
                ..z
            },
            ScreenLines::Grid => LookFx { grid: 0.40, ..z },
        }
    }
}

/// A força de cada efeito (0 = desligado, 1 = máximo razoável).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct LookFx {
    pub scanlines: f32,
    pub mask: f32,
    pub grid: f32,
    pub bloom: f32,
}

impl ScreenLook {
    pub const ALL: [ScreenLook; 7] = [
        ScreenLook::Rf,
        ScreenLook::Composite,
        ScreenLook::SVideo,
        ScreenLook::Pvm,
        ScreenLook::Trinitron,
        ScreenLook::Lcd,
        ScreenLook::Mono,
    ];

    /// O nome na configuração (`screen_look = "..."`).
    pub fn as_str(self) -> &'static str {
        match self {
            ScreenLook::Rf => "rf",
            ScreenLook::Composite => "composite",
            ScreenLook::SVideo => "svideo",
            ScreenLook::Pvm => "pvm",
            ScreenLook::Trinitron => "trinitron",
            ScreenLook::Lcd => "lcd",
            ScreenLook::Mono => "mono",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|l| l.as_str() == s)
    }

    /// O nome na tela de configurações.
    pub fn label(self) -> &'static str {
        match self {
            ScreenLook::Rf => tr!("Antenna TV (RF)", "TV de antena (RF)"),
            ScreenLook::Composite => "Composite",
            ScreenLook::SVideo => "S-Video",
            ScreenLook::Pvm => "RGB / PVM",
            ScreenLook::Trinitron => "Trinitron",
            ScreenLook::Lcd => tr!("Sharp LCD", "LCD nítido"),
            ScreenLook::Mono => tr!("Black & white", "Preto e branco"),
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|&l| l == self).unwrap_or(0)
    }

    pub fn cycle(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn cycle_back(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }

    /// Os efeitos por cima da imagem.
    pub fn fx(self) -> LookFx {
        let z = LookFx::default();
        match self {
            ScreenLook::Rf => z,
            ScreenLook::Composite => LookFx {
                scanlines: 0.12,
                ..z
            },
            ScreenLook::SVideo => LookFx {
                scanlines: 0.22,
                bloom: 0.10,
                ..z
            },
            ScreenLook::Pvm => LookFx {
                scanlines: 0.38,
                mask: 0.18,
                bloom: 0.10,
                ..z
            },
            ScreenLook::Trinitron => LookFx {
                scanlines: 0.10,
                mask: 0.30,
                bloom: 0.16,
                ..z
            },
            ScreenLook::Lcd => LookFx { grid: 0.40, ..z },
            ScreenLook::Mono => LookFx {
                scanlines: 0.20,
                bloom: 0.08,
                ..z
            },
        }
    }

    /// O quanto as bordas do tubo curvam (0 = vidro plano).
    pub fn warp(self) -> f32 {
        match self {
            ScreenLook::Lcd => 0.0,
            ScreenLook::Pvm => 0.03,
            ScreenLook::Trinitron => 0.02,
            _ => CRT_WARP,
        }
    }
}

/// Texturas e vértices dos efeitos, refeitos só quando a chave muda.
#[derive(Default)]
pub(super) struct LookState {
    pub look: ScreenLook,
    pub lines: ScreenLines,
    /// (linhas do jogo, força em %).
    scan: Option<((u32, u32), Texture)>,
    /// (largura em pixels da tela, força em %).
    mask: Option<((u32, u32), Texture)>,
    /// (linhas do jogo, força em %).
    grid: Option<((u32, u32), Texture)>,
    glow: Option<((u32, u32), Texture)>,
    /// A malha da imagem com a cor branca (os `Mul` não podem escurecer
    /// pela vinheta de novo) e com a vinheta (os `Add`).
    white: Vec<Vertex>,
    tint: Vec<Vertex>,
    mesh_key: (u32, u32, u32),
    /// As forças já com o tamanho da janela considerado, e o ganho que
    /// devolve o brilho que as máscaras tiraram.
    scan_s: f32,
    mask_s: f32,
    grid_s: f32,
    gain: f32,
    bloom: f32,
}

fn make_tex(
    canvas: &WindowCanvas,
    w: u32,
    h: u32,
    blend: BlendMode,
    fill: impl Fn(u32, u32) -> [u8; 3],
) -> Option<Texture> {
    let mut data = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let [r, g, b] = fill(x, y);
            data.extend_from_slice(&[r, g, b, 255]);
        }
    }
    let mut tex = canvas
        .create_texture_streaming(SdlFormat::RGBA32, w, h)
        .ok()?;
    tex.set_blend_mode(blend);
    tex.set_scale_mode(SdlScaleMode::Linear);
    tex.update(None, &data, (w * 4) as usize).ok()?;
    Some(tex)
}

fn gray(v: f32) -> [u8; 3] {
    let g = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    [g, g, g]
}

impl LookState {
    /// Deixa prontas as texturas do look para esta malha e este número de
    /// linhas, e o brilho suave com o quadro atual. Fora do alvo de captura.
    pub fn prepare(&mut self, canvas: &mut WindowCanvas, mesh: &CrtMesh, src: &SrcTexture) {
        let fx = self.lines.apply(self.look.fx());
        let lines = src.h.max(1);
        // Perto de 2 px por linha as scanlines viram moiré: somem aos poucos.
        let ppl = mesh.h as f32 / lines as f32;
        let cell = mesh.w as f32 / 256.0;
        self.scan_s = fx.scanlines * ((ppl - 2.0) / 2.0).clamp(0.0, 1.0);
        self.grid_s = fx.grid * ((cell - 1.5) / 1.5).clamp(0.0, 1.0);
        self.mask_s = if mesh.w >= 360 { fx.mask } else { 0.0 };
        self.bloom = fx.bloom;
        let mean =
            (1.0 - self.scan_s * 0.5) * (1.0 - self.mask_s * 2.0 / 3.0) * (1.0 - self.grid_s * 0.5);
        self.gain = (1.0 / mean - 1.0).min(0.30);

        let key = (mesh.w, mesh.h, crt_warp().to_bits());
        if self.mesh_key != key || self.white.len() != mesh.verts.len() {
            self.mesh_key = key;
            self.white = mesh.verts.clone();
            for v in &mut self.white {
                v.color = FColor::RGBA(1.0, 1.0, 1.0, 1.0);
            }
            self.tint = mesh.verts.clone();
        }

        let pct = |s: f32| (s * 100.0).round() as u32;
        if self.scan_s > 0.0 {
            let k = (lines, pct(self.scan_s));
            if !matches!(&self.scan, Some((o, _)) if *o == k) {
                const SUB: u32 = 8;
                let s = self.scan_s;
                // Claro no meio da linha, escuro na divisa: cosseno suave.
                self.scan = make_tex(canvas, 2, lines * SUB, BlendMode::Mul, |_, y| {
                    let t = ((y % SUB) as f32 + 0.5) / SUB as f32;
                    gray(1.0 - s * (0.5 - 0.5 * (std::f32::consts::TAU * t).cos()))
                })
                .map(|t| (k, t));
            }
        }
        if self.mask_s > 0.0 {
            let w = (mesh.w / 3 * 3).max(3);
            let k = (w, pct(self.mask_s));
            if !matches!(&self.mask, Some((o, _)) if *o == k) {
                let dim = ((1.0 - self.mask_s) * 255.0).round() as u8;
                // Faixas R, G, B de 1 pixel, cada uma deixa passar só a sua cor.
                self.mask = make_tex(canvas, w, 2, BlendMode::Mul, |x, _| match x % 3 {
                    0 => [255, dim, dim],
                    1 => [dim, 255, dim],
                    _ => [dim, dim, 255],
                })
                .map(|t| (k, t));
            }
        }
        if self.grid_s > 0.0 {
            let k = (lines, pct(self.grid_s));
            if !matches!(&self.grid, Some((o, _)) if *o == k) {
                let f = 1.0 - self.grid_s;
                // 256 colunas de pixels do SNES, 4x4 texels cada: a última
                // coluna e a última linha da célula são o vão.
                self.grid = make_tex(canvas, 256 * 4, lines * 4, BlendMode::Mul, |x, y| {
                    let mut v = 1.0;
                    if x % 4 == 3 {
                        v *= f;
                    }
                    if y % 4 == 3 {
                        v *= f;
                    }
                    gray(v)
                })
                .map(|t| (k, t));
            }
        }
        if self.bloom > 0.0 {
            let k = ((src.w / 4).max(8), (src.h / 4).max(8));
            if !matches!(&self.glow, Some((o, _)) if *o == k) {
                self.glow = canvas
                    .create_texture_target(SdlFormat::RGBA32, k.0, k.1)
                    .ok()
                    .map(|mut t| {
                        t.set_blend_mode(BlendMode::Add);
                        t.set_scale_mode(SdlScaleMode::Linear);
                        (k, t)
                    });
            }
            // O quadro encolhido (e por isso borrado) é o brilho.
            if let Some((_, glow)) = &mut self.glow {
                let _ = canvas.with_texture_canvas(glow, |c| {
                    let _ = c.copy(&src.tex, None, None);
                });
            }
        }
    }

    /// Os efeitos por cima da imagem já desenhada, na mesma malha.
    pub fn draw(&mut self, canvas: &mut WindowCanvas, mesh: &CrtMesh, src: &mut SrcTexture) {
        if self.white.len() != mesh.verts.len() {
            return;
        }
        // Devolve o brilho que as máscaras tiram: a própria imagem, somada.
        if self.gain > 0.01 {
            src.tex.set_blend_mode(BlendMode::Add);
            for v in &mut self.tint {
                v.color.a = self.gain;
            }
            let _ = canvas.render_geometry(&self.tint, Some(&src.tex), &mesh.indices[..]);
            src.tex.set_blend_mode(BlendMode::None);
        }
        if let Some((_, glow)) = self.glow.as_ref().filter(|_| self.bloom > 0.0) {
            for v in &mut self.tint {
                v.color.a = self.bloom;
            }
            let _ = canvas.render_geometry(&self.tint, Some(glow), &mesh.indices[..]);
        }
        for (on, tex) in [
            (self.scan_s > 0.0, self.scan.as_ref()),
            (self.mask_s > 0.0, self.mask.as_ref()),
            (self.grid_s > 0.0, self.grid.as_ref()),
        ] {
            if let (true, Some((_, tex))) = (on, tex) {
                let _ = canvas.render_geometry(&self.white, Some(tex), &mesh.indices[..]);
            }
        }
    }
}

impl Cabinet {
    pub fn screen_lines(&self) -> ScreenLines {
        self.looks.lines
    }

    pub fn set_screen_lines(&mut self, lines: ScreenLines) {
        self.looks.lines = lines;
    }

    pub fn screen_look(&self) -> ScreenLook {
        self.looks.look
    }

    /// Troca o visual da tela: os efeitos e a curvatura do tubo.
    pub fn set_screen_look(&mut self, look: ScreenLook) {
        if self.looks.look != look {
            self.looks.look = look;
            set_crt_warp(look.warp());
        }
    }
}
