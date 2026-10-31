//! Soprar o cartucho: a cena desenhada por cima da TV fora do ar depois de
//! um mau contato — o cartucho sai do slot, voa para a frente do tubo e
//! vira com os contatos para quem joga; segurar o clique (ou Y/B) sopra a
//! poeira para fora e os pinos passam de foscos a dourados. O estado da
//! cena (fôlego, limpeza, poeira) vive no app; aqui só se desenha.

use super::*;

/// Um grão de poeira saindo dos contatos. `x`/`y` em larguras de cartucho a
/// partir do centro da boca dos contatos; `life` 1.0 → 0.0 some.
#[derive(Debug, Clone, Copy)]
pub struct Dust {
    pub x: f32,
    pub y: f32,
    pub life: f32,
    /// 0..1 — o tamanho relativo do grão.
    pub size: f32,
}

/// O quadro da cena do sopro — montado pelo app a cada frame.
#[derive(Debug, Clone, Default)]
pub struct BlowScene {
    /// 0.0 = o cartucho acima do slot do painel, 1.0 = na frente da TV, com
    /// os contatos virados para quem joga.
    pub lift: f32,
    /// 0..1 — quanto dos contatos já está limpo (fosco → dourado).
    pub clean: f32,
    /// 0..1 — a barra de fôlego.
    pub breath: f32,
    /// Soprando agora (desenha o ar subindo e treme o cartucho).
    pub blowing: bool,
    /// O fôlego acabou: o sopro não limpa até soltar e respirar.
    pub out_of_breath: bool,
    /// 0..1 — os pinos embaçados de tanto soprar (a umidade do mito).
    pub fog: f32,
    pub dust: Vec<Dust>,
    /// Segundos desde o começo da cena — anima o ar e o brilho dos pinos.
    pub time: f32,
    /// O nome do jogo, para o cartucho genérico (sem arte de cartucho).
    pub title: String,
    /// O microfone está ouvindo: o nível de 0..1 (a barrinha "MIC") e a
    /// dica pede para soprar nele. `None` = sem microfone.
    pub mic: Option<f32>,
}

/// Os botões da cena do sopro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlowButton {
    /// Inserir o cartucho de novo no slot.
    Insert,
    /// Desistir e guardar o cartucho na estante (o ejetar de sempre).
    Shelf,
}

const PLASTIC: (u8, u8, u8) = (150, 148, 140);
const PLASTIC_DARK: (u8, u8, u8) = (104, 102, 96);
const PIN_DIRTY: (f32, f32, f32) = (112.0, 86.0, 46.0);
const PIN_CLEAN: (f32, f32, f32) = (236.0, 190.0, 82.0);
const PINS: i32 = 31;

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

fn fcolor(c: (u8, u8, u8), a: f32) -> FColor {
    FColor::RGBA(
        c.0 as f32 / 255.0,
        c.1 as f32 / 255.0,
        c.2 as f32 / 255.0,
        a,
    )
}

/// Um quadrilátero preenchido com uma cor por vértice (sentido horário a
/// partir do canto de cima à esquerda).
fn quad(canvas: &mut WindowCanvas, pts: [(f32, f32); 4], colors: [FColor; 4]) {
    let verts: Vec<Vertex> = pts
        .iter()
        .zip(colors.iter())
        .map(|(p, c)| Vertex {
            position: sdl3::render::FPoint::new(p.0, p.1),
            color: *c,
            tex_coord: sdl3::render::FPoint::new(0.0, 0.0),
        })
        .collect();
    let _ = canvas.render_geometry(&verts, None::<&Texture>, &[0, 1, 2, 0, 2, 3][..]);
}

/// Um quadrilátero com textura (`uv` normalizado), escurecido por `shade`.
fn tex_quad(
    canvas: &mut WindowCanvas,
    tex: &Texture,
    pts: [(f32, f32); 4],
    uv: [(f32, f32); 4],
    shade: f32,
) {
    let color = FColor::RGBA(shade, shade, shade, 1.0);
    let verts: Vec<Vertex> = pts
        .iter()
        .zip(uv.iter())
        .map(|(p, t)| Vertex {
            position: sdl3::render::FPoint::new(p.0, p.1),
            color,
            tex_coord: sdl3::render::FPoint::new(t.0, t.1),
        })
        .collect();
    let _ = canvas.render_geometry(&verts, Some(tex), &[0, 1, 2, 0, 2, 3][..]);
}

fn scale_color(c: (u8, u8, u8), k: f32) -> (u8, u8, u8) {
    (
        (c.0 as f32 * k) as u8,
        (c.1 as f32 * k) as u8,
        (c.2 as f32 * k) as u8,
    )
}

fn fill_rgba(canvas: &mut WindowCanvas, x: i32, y: i32, w: u32, h: u32, c: (u8, u8, u8, u8)) {
    canvas.set_draw_color(Color::RGBA(c.0, c.1, c.2, c.3));
    let _ = canvas.fill_rect(Rect::new(x, y, w.max(1), h.max(1)));
}

/// Texto com a sombra de 2px do OSD da TV, centrado em `cx`.
fn osd_text_centered(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    cx: i32,
    y: i32,
    scale: u32,
    color: (u8, u8, u8),
    s: &str,
) {
    let w = (GLYPH_W * scale) as i32 * s.chars().count() as i32;
    let x = cx - w / 2;
    draw_text_absolute(
        canvas,
        font,
        x + 2,
        y + 2,
        TextStyle::new(scale, (12, 14, 12)),
        s,
        usize::MAX,
    );
    draw_text_absolute(
        canvas,
        font,
        x,
        y,
        TextStyle::new(scale, color),
        s,
        usize::MAX,
    );
}

/// Desenha a cena do sopro (`screen`/`panel` em coordenadas do viewport do
/// gabinete) e devolve os botões clicáveis.
pub(super) fn draw_blow_scene(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    screen: Rect,
    panel: Rect,
    scene: &BlowScene,
) -> Vec<(BlowButton, Rect)> {
    canvas.set_blend_mode(sdl3::render::BlendMode::Blend);
    let (sx, sy) = (screen.x() as f32, screen.y() as f32);
    let (sw, sh) = (screen.width() as f32, screen.height() as f32);

    // A viagem: do alto do slot do painel até o centro do tubo, crescendo;
    // na segunda metade o cartucho tomba, mostrando a face de baixo.
    let e = smooth(scene.lift);
    let pose = smooth((scene.lift - 0.3) / 0.7);
    let start = (
        panel.x() as f32 + panel.width() as f32 / 2.0,
        panel.y() as f32 + 150.0,
        panel.width() as f32 * 0.62,
    );
    let end = (sx + sw / 2.0, sy + sh * 0.44, (sw * 0.48).min(sh * 0.74));
    let shake = if scene.blowing && !scene.out_of_breath {
        (
            (scene.time * 61.0).sin() * 2.0,
            (scene.time * 47.0).cos() * 1.5,
        )
    } else {
        (0.0, 0.0)
    };
    let cx = lerp(start.0, end.0, e) + shake.0;
    let cy = lerp(start.1, end.1, e) + shake.1;
    let w = lerp(start.2, end.2, e);

    // O cartucho em 3D de verdade, em milímetros: a frente W×H no plano
    // z=0 e o corpo indo para trás até z=-D (um cartucho de SNES tem uns
    // 12 × 2 cm de borda). Ele gira no eixo horizontal — a borda de baixo
    // vem para quem joga — numa perspectiva suave.
    let art = images.get(&PANEL_CARTRIDGE_IMG);
    let front_aspect = art
        .map(|a| {
            let (_, _, cw, ch) = a.content;
            ch.max(1) as f32 / cw.max(1) as f32
        })
        .unwrap_or(0.72);
    const CART_W: f32 = 120.0;
    const CAM: f32 = 520.0;
    let cart_h = CART_W * front_aspect;
    let cart_d = CART_W * 0.18;
    let (sin, cos) = (pose * 55f32.to_radians()).sin_cos();
    let mm = w / CART_W;
    let (hw, hh) = (CART_W / 2.0, cart_h / 2.0);
    let proj = |x: f32, y: f32, z: f32| -> (f32, f32) {
        // Gira em torno do meio do corpo.
        let z = z + cart_d / 2.0;
        let yr = y * cos - z * sin;
        let zr = y * sin + z * cos;
        let k = CAM / (CAM - zr);
        (cx + x * k * mm, cy + yr * k * mm)
    };
    // A frente escurece um pouco ao tombar (sai da luz).
    let shade = 1.0 - 0.28 * pose;

    match art {
        Some(a) => {
            // Em faixas: a textura afim por triângulo entorta num trapézio
            // grande; em fatias finas, a perspectiva fica limpa.
            let (ux, uy, uw, uh) = a.content;
            let (iw, ih) = (a.w.max(1) as f32, a.h.max(1) as f32);
            let (u0, u1) = (ux as f32 / iw, (ux + uw) as f32 / iw);
            const STRIPS: usize = 12;
            for i in 0..STRIPS {
                let (t0, t1) = (i as f32 / STRIPS as f32, (i + 1) as f32 / STRIPS as f32);
                let (y0, y1) = (-hh + cart_h * t0, -hh + cart_h * t1);
                let (v0, v1) = (
                    (uy as f32 + uh as f32 * t0) / ih,
                    (uy as f32 + uh as f32 * t1) / ih,
                );
                tex_quad(
                    canvas,
                    &a.tex,
                    [
                        proj(-hw, y0, 0.0),
                        proj(hw, y0, 0.0),
                        proj(hw, y1, 0.0),
                        proj(-hw, y1, 0.0),
                    ],
                    [(u0, v0), (u1, v0), (u1, v1), (u0, v1)],
                    shade,
                );
            }
        }
        None => {
            let plastic = scale_color(PLASTIC, shade);
            quad(
                canvas,
                [
                    proj(-hw, -hh, 0.0),
                    proj(hw, -hh, 0.0),
                    proj(hw, hh, 0.0),
                    proj(-hw, hh, 0.0),
                ],
                [fcolor(plastic, 1.0); 4],
            );
            let label = scale_color((30, 28, 40), shade);
            let (lx0, lx1, ly0, ly1) = (-hw * 0.72, hw * 0.72, -hh * 0.84, hh * 0.3);
            quad(
                canvas,
                [
                    proj(lx0, ly0, 0.0),
                    proj(lx1, ly0, 0.0),
                    proj(lx1, ly1, 0.0),
                    proj(lx0, ly1, 0.0),
                ],
                [fcolor(label, 1.0); 4],
            );
            let (tx, ty) = proj(0.0, (ly0 + ly1) / 2.0, 0.0);
            let scale = if w > 260.0 { 2 } else { 1 };
            let cols = ((w * 0.66) / (GLYPH_W * scale) as f32).max(1.0) as usize;
            let title: String = scene.title.chars().take(cols).collect();
            osd_text_centered(
                canvas,
                font,
                tx as i32,
                ty as i32 - (GLYPH_H * scale) as i32 / 2,
                scale,
                (232, 200, 120),
                &title,
            );
        }
    }

    if pose < 0.02 {
        return Vec::new();
    }

    // A borda de baixo: o plástico (mais claro perto da quina, que pega a
    // luz), a fenda escura e, lá dentro, a ponta da placa verde com os
    // contatos dourados finos.
    let bottom = |x0: f32, x1: f32, z0: f32, z1: f32| {
        [
            proj(x0, hh, z0),
            proj(x1, hh, z0),
            proj(x1, hh, z1),
            proj(x0, hh, z1),
        ]
    };
    quad(
        canvas,
        bottom(-hw, hw, 0.0, -cart_d),
        [
            fcolor((170, 168, 160), 1.0),
            fcolor((170, 168, 160), 1.0),
            fcolor(PLASTIC_DARK, 1.0),
            fcolor(PLASTIC_DARK, 1.0),
        ],
    );
    let (slot_x, slot_z0, slot_z1) = (hw * 0.86, -cart_d * 0.2, -cart_d * 0.8);
    quad(
        canvas,
        bottom(-slot_x, slot_x, slot_z0, slot_z1),
        [fcolor((12, 11, 14), 1.0); 4],
    );
    let (pcb_x, pcb_z0, pcb_z1) = (hw * 0.82, -cart_d * 0.32, -cart_d * 0.68);
    quad(
        canvas,
        bottom(-pcb_x, pcb_x, pcb_z0, pcb_z1),
        [fcolor((30, 70, 44), 1.0); 4],
    );

    let clean = scene.clean.clamp(0.0, 1.0);
    let pin = (
        lerp(PIN_DIRTY.0, PIN_CLEAN.0, clean),
        lerp(PIN_DIRTY.1, PIN_CLEAN.1, clean),
        lerp(PIN_DIRTY.2, PIN_CLEAN.2, clean),
    );
    let span = pcb_x * 2.0 * 0.96;
    let pitch = span / PINS as f32;
    // Limpos de vez: um brilho corre pelos contatos.
    let sparkle = (clean > 0.97).then(|| ((scene.time * 14.0) as i32).rem_euclid(PINS + 12));
    for i in 0..PINS {
        let x0 = -span / 2.0 + pitch * (i as f32 + 0.25);
        let x1 = x0 + pitch * 0.5;
        // A sujeira é oxidação: uns contatos bem mais escuros que outros,
        // clareando com a limpeza.
        let h = (i as u32).wrapping_mul(2_654_435_761) >> 16;
        let grime = (1.0 - clean) * if h.is_multiple_of(3) { 0.75 } else { 0.25 };
        let mut c = (
            lerp(pin.0, 64.0, grime) as u8,
            lerp(pin.1, 52.0, grime) as u8,
            lerp(pin.2, 36.0, grime) as u8,
        );
        if sparkle == Some(i) {
            c = (255, 246, 200);
        }
        quad(
            canvas,
            bottom(x0, x1, pcb_z0 - cart_d * 0.02, pcb_z1 + cart_d * 0.02),
            [fcolor(c, 1.0); 4],
        );
    }
    if scene.fog > 0.0 {
        quad(
            canvas,
            bottom(-slot_x, slot_x, slot_z0, slot_z1),
            [fcolor((214, 222, 230), scene.fog.clamp(0.0, 1.0) * 0.8); 4],
        );
    }

    let (sl, _) = proj(-slot_x, hh, -cart_d * 0.5);
    let (sr, slot_y) = proj(slot_x, hh, -cart_d * 0.5);
    let (ox, ow) = (sl, sr - sl);
    let bottom_h = proj(0.0, hh, -cart_d).1 - proj(0.0, hh, 0.0).1;
    let mouth = ((sl + sr) / 2.0, slot_y);

    // O ar do sopro: arcos claros subindo de baixo até a boca.
    if scene.blowing && !scene.out_of_breath {
        for i in 0..7 {
            let p = (scene.time * 2.6 + i as f32 / 7.0).fract();
            let x0 = ox + ow * (0.1 + 0.8 * i as f32 / 6.0);
            let y0 = mouth.1 + bottom_h * 0.9 + sh * 0.22 * (1.0 - p);
            let a = ((1.0 - p) * 200.0) as u8;
            canvas.set_draw_color(Color::RGBA(205, 230, 255, a));
            let mut prev = (x0, y0);
            for k in 1..=6 {
                let ky = y0 - k as f32 * 6.0;
                let kx = x0 + ((k as f32 * 0.9 + i as f32).sin()) * 5.0;
                let _ = canvas.draw_line(
                    sdl3::render::FPoint::new(prev.0, prev.1),
                    sdl3::render::FPoint::new(kx, ky),
                );
                prev = (kx, ky);
            }
        }
    }

    // A poeira saindo dos contatos.
    for d in &scene.dust {
        let a = (d.life.clamp(0.0, 1.0) * 220.0) as u8;
        let r = (2.0 + d.size * 4.0) * (w / 300.0).max(0.6);
        fill_rgba(
            canvas,
            (mouth.0 + d.x * w - r / 2.0) as i32,
            (mouth.1 + d.y * w - r / 2.0) as i32,
            r as u32,
            r as u32,
            (172, 162, 142, a),
        );
    }

    if scene.lift < 1.0 {
        return Vec::new();
    }

    // O OSD: a dica no alto, o fôlego e os contatos embaixo, à esquerda.
    let hint = if scene.out_of_breath {
        tr!(
            "OUT OF BREATH - LET GO AND BREATHE",
            "SEM FÔLEGO - SOLTE E RESPIRE"
        )
    } else if scene.fog > 0.05 {
        tr!("PINS FOGGED UP - WAIT", "PINOS EMBAÇADOS - ESPERE SECAR")
    } else if clean >= 0.999 {
        tr!("ALL CLEAN - INSERT IT AGAIN", "LIMPINHO - INSIRA DE NOVO")
    } else {
        match scene.mic {
            Some(_) => tr!(
                "BLOW INTO THE MICROPHONE (OR HOLD CLICK)",
                "SOPRE NO MICROFONE (OU SEGURE O CLIQUE)"
            ),
            None => tr!("HOLD CLICK OR Y TO BLOW", "SEGURE O CLIQUE OU Y PRA SOPRAR"),
        }
    };
    let hint_color = if scene.out_of_breath {
        (240, 80, 70)
    } else {
        OSD_GREEN
    };
    osd_text_centered(
        canvas,
        font,
        (sx + sw / 2.0) as i32,
        (sy + sh * 0.06) as i32,
        2,
        hint_color,
        hint,
    );

    let mx = (sx + sw * 0.06) as i32;
    let my = (sy + sh * 0.72) as i32;
    let bar_w = (sw * 0.26) as u32;
    draw_text_absolute(
        canvas,
        font,
        mx + 2,
        my + 2,
        TextStyle::new(2, (12, 14, 12)),
        tr!("BREATH", "FÔLEGO"),
        usize::MAX,
    );
    draw_text_absolute(
        canvas,
        font,
        mx,
        my,
        TextStyle::new(2, OSD_GREEN),
        tr!("BREATH", "FÔLEGO"),
        usize::MAX,
    );
    let bar_y = my + (GLYPH_H * 2) as i32 + 6;
    fill_rgba(canvas, mx, bar_y, bar_w, 12, (20, 34, 22, 220));
    let bar_c = if scene.out_of_breath || scene.breath > 0.85 {
        (240, 80, 70, 255)
    } else {
        (OSD_GREEN.0, OSD_GREEN.1, OSD_GREEN.2, 255)
    };
    fill_rgba(
        canvas,
        mx,
        bar_y,
        (bar_w as f32 * scene.breath.clamp(0.0, 1.0)) as u32,
        12,
        bar_c,
    );
    // O microfone: uma barrinha fina de nível logo abaixo do fôlego.
    if let Some(level) = scene.mic {
        let mic_y = bar_y + 22;
        draw_text_absolute(
            canvas,
            font,
            mx,
            mic_y,
            TextStyle::new(1, OSD_GREEN),
            "MIC",
            usize::MAX,
        );
        let lx = mx + (GLYPH_W * 4) as i32;
        let lw = bar_w.saturating_sub(GLYPH_W * 4);
        fill_rgba(canvas, lx, mic_y + 3, lw, 6, (20, 34, 22, 220));
        fill_rgba(
            canvas,
            lx,
            mic_y + 3,
            (lw as f32 * level.clamp(0.0, 1.0)) as u32,
            6,
            (205, 230, 255, 255),
        );
    }
    let contacts = format!(
        "{} {}%",
        tr!("CONTACTS", "CONTATOS"),
        (clean * 100.0).round() as i32
    );
    let cx2 = mx + bar_w as i32 + 28;
    draw_text_absolute(
        canvas,
        font,
        cx2 + 2,
        my + 2,
        TextStyle::new(2, (12, 14, 12)),
        &contacts,
        usize::MAX,
    );
    draw_text_absolute(
        canvas,
        font,
        cx2,
        my,
        TextStyle::new(2, (236, 190, 82)),
        &contacts,
        usize::MAX,
    );

    // Os dois botões, centrados embaixo: inserir de novo e guardar.
    let labels = [
        (
            BlowButton::Insert,
            tr!("Insert again (A)", "Inserir de novo (A)"),
        ),
        (
            BlowButton::Shelf,
            tr!("Back to shelf (Select)", "Guardar na estante (Select)"),
        ),
    ];
    let gap = 18i32;
    let width_at = |scale: u32| -> i32 {
        labels
            .iter()
            .map(|(_, l)| (GLYPH_W * scale) as i32 * l.chars().count() as i32 + 24)
            .sum::<i32>()
            + gap
    };
    let scale = if width_at(2) as f32 <= sw * 0.92 {
        2u32
    } else {
        1
    };
    let pad = 12i32;
    let bh = (GLYPH_H * scale) as i32 + pad * 2 - 6;
    let mut bx = (sx + sw / 2.0) as i32 - width_at(scale) / 2;
    let by = (sy + sh * 0.86) as i32;
    let mut out = Vec::new();
    for (btn, label) in labels.iter() {
        let tw = (GLYPH_W * scale) as i32 * label.chars().count() as i32;
        let bw = tw + 24;
        let rect = Rect::new(bx, by, bw as u32, bh as u32);
        let (bg, fg) = if *btn == BlowButton::Insert {
            ((60, 46, 110, 235), (236, 230, 255))
        } else {
            ((24, 24, 28, 220), (190, 190, 196))
        };
        fill_rgba(canvas, rect.x(), rect.y(), rect.width(), rect.height(), bg);
        canvas.set_draw_color(Color::RGBA(120, 100, 200, 255));
        let _ = canvas.draw_rect(sdl3::render::FRect::new(
            rect.x() as f32,
            rect.y() as f32,
            rect.width() as f32,
            rect.height() as f32,
        ));
        draw_text_absolute(
            canvas,
            font,
            rect.x() + 12,
            rect.y() + pad - 3,
            TextStyle::new(scale, fg),
            label,
            usize::MAX,
        );
        out.push((*btn, rect));
        bx += bw + gap;
    }
    out
}
