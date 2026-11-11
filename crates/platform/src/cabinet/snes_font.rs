//! Rótulos "POWER" / "RESET ▲" das chaves do console, com glifos vetoriais
//! no estilo da serigrafia do SNES: sans condensada, negrito e inclinada
//! (itálico). Cada letra é uma poli-linha (traço) numa grade de altura 100;
//! a máscara é rasterizada com antialiasing (4x4 por pixel) uma única vez por
//! tamanho e guardada em cache — depois, desenhar é só uma faixa de retângulos
//! por linha (nada de textura nem raster por frame, para poupar CPU/bateria).

use std::cell::RefCell;
use std::collections::HashMap;

use sdl3::pixels::Color;
use sdl3::rect::Rect;
use sdl3::render::{BlendMode, WindowCanvas};

/// Espessura do traço, na grade de altura 100.
const STROKE: f32 = 17.0;
/// Inclinação do itálico: deslocamento em x por unidade de altura.
const SLANT: f32 = 0.22;
/// Espaço entre letras (grade).
const TRACKING: f32 = 7.0;
/// Subamostras por eixo do antialiasing.
const SS: i32 = 8;

type Pt = (f32, f32);

/// Um glifo: largura de avanço, traços (poli-linhas) e, se houver, um
/// polígono preenchido (a seta ▲).
struct Glyph {
    w: f32,
    strokes: &'static [&'static [Pt]],
    fill: &'static [Pt],
}

const P: Glyph = Glyph {
    w: 52.0,
    strokes: &[&[
        (9.0, 92.0),
        (9.0, 9.0),
        (34.0, 9.0),
        (43.0, 18.0),
        (43.0, 38.0),
        (34.0, 47.0),
        (9.0, 47.0),
    ]],
    fill: &[],
};
const O: Glyph = Glyph {
    w: 54.0,
    strokes: &[&[
        (16.0, 9.0),
        (38.0, 9.0),
        (46.0, 20.0),
        (46.0, 80.0),
        (38.0, 91.0),
        (16.0, 91.0),
        (8.0, 80.0),
        (8.0, 20.0),
        (16.0, 9.0),
    ]],
    fill: &[],
};
const W: Glyph = Glyph {
    w: 80.0,
    strokes: &[&[
        (6.0, 9.0),
        (21.0, 91.0),
        (40.0, 14.0),
        (59.0, 91.0),
        (74.0, 9.0),
    ]],
    fill: &[],
};
const E: Glyph = Glyph {
    w: 46.0,
    strokes: &[
        &[(9.0, 9.0), (9.0, 91.0)],
        &[(9.0, 9.0), (41.0, 9.0)],
        &[(9.0, 50.0), (36.0, 50.0)],
        &[(9.0, 91.0), (41.0, 91.0)],
    ],
    fill: &[],
};
const R: Glyph = Glyph {
    w: 54.0,
    strokes: &[
        &[
            (9.0, 92.0),
            (9.0, 9.0),
            (35.0, 9.0),
            (44.0, 18.0),
            (44.0, 38.0),
            (35.0, 47.0),
            (9.0, 47.0),
        ],
        &[(29.0, 47.0), (46.0, 92.0)],
    ],
    fill: &[],
};
const S: Glyph = Glyph {
    w: 50.0,
    strokes: &[&[
        (42.0, 27.0),
        (37.0, 13.0),
        (26.0, 9.0),
        (15.0, 13.0),
        (9.0, 26.0),
        (13.0, 38.0),
        (25.0, 47.0),
        (36.0, 54.0),
        (42.0, 66.0),
        (38.0, 84.0),
        (27.0, 91.0),
        (15.0, 88.0),
        (8.0, 72.0),
    ]],
    fill: &[],
};
const T: Glyph = Glyph {
    w: 48.0,
    strokes: &[&[(3.0, 9.0), (45.0, 9.0)], &[(24.0, 9.0), (24.0, 92.0)]],
    fill: &[],
};
/// A setinha do RESET: triângulo estreito, ponta para cima.
const ARROW: Glyph = Glyph {
    w: 56.0,
    strokes: &[],
    fill: &[(12.0, 98.0), (56.0, 98.0), (34.0, 20.0)],
};

const J: Glyph = Glyph {
    w: 40.0,
    strokes: &[&[
        (32.0, 9.0),
        (32.0, 72.0),
        (27.0, 86.0),
        (18.0, 91.0),
        (10.0, 90.0),
        (5.0, 82.0),
    ]],
    fill: &[],
};
const C: Glyph = Glyph {
    w: 52.0,
    strokes: &[&[
        (44.0, 28.0),
        (40.0, 15.0),
        (30.0, 9.0),
        (22.0, 9.0),
        (12.0, 14.0),
        (8.0, 27.0),
        (8.0, 73.0),
        (12.0, 86.0),
        (22.0, 91.0),
        (30.0, 91.0),
        (40.0, 85.0),
        (44.0, 72.0),
    ]],
    fill: &[],
};
const A: Glyph = Glyph {
    w: 56.0,
    strokes: &[
        &[(5.0, 92.0), (21.0, 9.0), (35.0, 9.0), (51.0, 92.0)],
        &[(14.0, 66.0), (42.0, 66.0)],
    ],
    fill: &[],
};

fn glyph(ch: char) -> Option<&'static Glyph> {
    Some(match ch.to_ascii_uppercase() {
        'P' => &P,
        'O' => &O,
        'W' => &W,
        'E' => &E,
        'R' => &R,
        'S' => &S,
        'T' => &T,
        'J' => &J,
        'C' => &C,
        'A' => &A,
        '▲' => &ARROW,
        _ => return None,
    })
}

/// Largura do texto na grade (sem o espaço depois da última letra) e o
/// deslocamento extra à direita que a inclinação causa.
fn layout_width(label: &str) -> f32 {
    let mut x = 0.0;
    for ch in label.chars() {
        if let Some(g) = glyph(ch) {
            x += g.w + TRACKING;
        }
    }
    (x - TRACKING).max(0.0) + SLANT * 100.0
}

/// Largura em pixels do rótulo desenhado com altura de caixa `h`.
pub(super) fn label_width(label: &str, h: u32) -> u32 {
    (layout_width(label) * h as f32 / 100.0).ceil() as u32
}

fn seg_dist2(p: Pt, a: Pt, b: Pt) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    let (cx, cy) = (a.0 + t * dx, a.1 + t * dy);
    (p.0 - cx).powi(2) + (p.1 - cy).powi(2)
}

fn in_polygon(p: Pt, poly: &[Pt]) -> bool {
    let mut inside = false;
    let mut j = poly.len().wrapping_sub(1);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[j]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < (b.0 - a.0) * (p.1 - a.1) / (b.1 - a.1) + a.0 {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Máscara de cobertura (0..=255) do rótulo, `w` x `h` pixels.
struct Mask {
    w: u32,
    h: u32,
    alpha: Vec<u8>,
}

fn rasterize(label: &str, h: u32) -> Mask {
    let w = label_width(label, h).max(1);
    let scale = h as f32 / 100.0;
    // Posição x (grade) de cada glifo.
    let mut placed: Vec<(f32, &'static Glyph)> = Vec::new();
    let mut x = 0.0;
    for ch in label.chars() {
        if let Some(g) = glyph(ch) {
            placed.push((x, g));
            x += g.w + TRACKING;
        }
    }
    let half2 = (STROKE / 2.0).powi(2);
    let mut alpha = vec![0u8; (w * h) as usize];
    for py in 0..h as i32 {
        for px in 0..w as i32 {
            let mut hits = 0;
            for sy in 0..SS {
                for sx in 0..SS {
                    let fx = px as f32 + (sx as f32 + 0.5) / SS as f32;
                    let fy = py as f32 + (sy as f32 + 0.5) / SS as f32;
                    // Para a grade do glifo, desfazendo o itálico.
                    let gy = fy / scale;
                    let gx = fx / scale - SLANT * (100.0 - gy);
                    if sample_hit((gx, gy), &placed, half2) {
                        hits += 1;
                    }
                }
            }
            alpha[(py as u32 * w + px as u32) as usize] = (hits * 255 / (SS * SS)) as u8;
        }
    }
    Mask { w, h, alpha }
}

fn sample_hit(p: Pt, placed: &[(f32, &Glyph)], half2: f32) -> bool {
    for (ox, g) in placed {
        let q = (p.0 - ox, p.1);
        if q.0 < -STROKE || q.0 > g.w + STROKE {
            continue;
        }
        if !g.fill.is_empty() && in_polygon(q, g.fill) {
            return true;
        }
        for s in g.strokes {
            for pair in s.windows(2) {
                if seg_dist2(q, pair[0], pair[1]) <= half2 {
                    return true;
                }
            }
        }
    }
    false
}

thread_local! {
    static CACHE: RefCell<HashMap<(String, u32), Mask>> = RefCell::new(HashMap::new());
}

/// Desenha `label` com o canto superior esquerdo em `(x, y)` e altura de
/// caixa `h` (as letras ocupam a altura toda), na cor `rgb`. Caracteres sem
/// glifo são ignorados.
pub(super) fn draw_label(
    canvas: &mut WindowCanvas,
    x: i32,
    y: i32,
    h: u32,
    rgb: (u8, u8, u8),
    label: &str,
) {
    if h == 0 {
        return;
    }
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        let mask = c
            .entry((label.to_string(), h))
            .or_insert_with(|| rasterize(label, h));
        let prev = canvas.blend_mode();
        canvas.set_blend_mode(BlendMode::Blend);
        for row in 0..mask.h {
            let line = &mask.alpha[(row * mask.w) as usize..((row + 1) * mask.w) as usize];
            let mut col = 0usize;
            while col < line.len() {
                let a = line[col];
                if a == 0 {
                    col += 1;
                    continue;
                }
                let start = col;
                while col < line.len() && line[col] == a {
                    col += 1;
                }
                canvas.set_draw_color(Color::RGBA(rgb.0, rgb.1, rgb.2, a));
                let _ = canvas.fill_rect(Rect::new(
                    x + start as i32,
                    y + row as i32,
                    (col - start) as u32,
                    1,
                ));
            }
        }
        canvas.set_blend_mode(prev);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_label_char_has_a_glyph() {
        for ch in "POWERSTJCA▲".chars() {
            assert!(glyph(ch).is_some(), "{ch}");
        }
        assert!(glyph(' ').is_none());
    }

    #[test]
    fn mask_has_ink_and_scales_with_height() {
        let small = rasterize("POWER", 10);
        let big = rasterize("POWER", 20);
        assert!(small.alpha.contains(&255));
        assert!(big.w > small.w);
        assert_eq!(label_width("POWER", 10), small.w);
    }

    #[test]
    fn arrow_is_filled_and_extends_reset() {
        let m = rasterize("▲", 20);
        // Base larga, ponta estreita: perto da base há mais tinta que perto da ponta.
        let ink = |r: u32| {
            (0..m.w)
                .filter(|&c| m.alpha[(r * m.w + c) as usize] > 127)
                .count()
        };
        assert!(ink(m.h - 3) > ink(6));
        assert!(label_width("RESET▲", 12) > label_width("RESET", 12));
    }
}
