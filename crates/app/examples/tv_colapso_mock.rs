//! MOCK — a segunda rodada da proposta TV (complementa o `tv_botoes_mock`):
//! 1) o colapso do CRT ao desligar (a imagem dobra numa linha horizontal
//!    brilhante e apaga — frames do vivo ao ponto de afterglow), cada frame
//!    num arquivo; 2) a grade de ventilação no queixo do gabinete e o bloom
//!    do bezel; 3) o feedback de pressed na tecla chiclete (o conteúdo
//!    afunda 1px no clique). 100% sintético; nada do app muda.
//!
//! Run: cargo run -p sixteen-app --example tv_colapso_mock — escreve
//! /tmp/tv-mock/{colapso-1..5.bmp, chin.bmp} (converter com sips para png).

use std::path::PathBuf;

use sixteen_platform::{Platform, Screen};

const BG: (u8, u8, u8) = (16, 14, 13);
const TEXTO: (u8, u8, u8) = (225, 220, 210);
const DIM: (u8, u8, u8) = (140, 134, 124);

/// Paleta da tecla chiclete (mesma do `tv_botoes_mock`).
const CHICLETE_ASSENTO: (u8, u8, u8) = (18, 17, 16);
const CHICLETE_CORPO: (u8, u8, u8) = (150, 144, 138);
const CHICLETE_LABEL: (u8, u8, u8) = (28, 26, 24);

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from("/tmp/tv-mock");
    std::fs::create_dir_all(&out)?;

    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("SixteeN mock", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("SixteeN v1.2.0\nsnes9x 1.63");

    // --- 1. O colapso do CRT: 5 frames -----------------------------------
    // Vivo -> esmagando (conteúdo já clareando/borrando) -> só a linha com
    // halo -> o ponto central com afterglow -> quase apagado.
    for (n, frac) in [1.0f32, 0.45, 0.10, 0.0, 0.0].into_iter().enumerate() {
        let path = out.join(format!("colapso-{}.bmp", n + 1));
        let dot = n >= 3; // frames 4 e 5: o ponto, com afterglow decaindo
        let glow = [1.0f32, 0.9, 0.55, 1.0, 0.35][n];
        cab.capture_shelf(BG, |d| frame_estante(d, frac, dot, glow), &path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    }

    // --- 2. O queixo: grade de ventilação + bloom do bezel ---------------
    let path = out.join("chin.bmp");
    cab.capture_shelf(BG, tela_chin, &path)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    println!("{}", out.display());
    Ok(())
}

/// A "estante" de brinquedo comprimida em `frac` da altura. Quanto menor o
/// frac, mais o conteúdo clareia em direção ao branco (a energia do fósforo
/// se concentra) e menos legível fica — nunca uma miniatura nítida. Com
/// `dot`, só o ponto central de afterglow, intensidade `glow`.
fn frame_estante(d: &mut Screen, frac: f32, dot: bool, glow: f32) {
    let (sw, sh) = d.size();
    let (sw, sh) = (sw as i32, sh as i32);
    let m = 34i32;

    if dot {
        // O tubo quase preto (nunca preto puro — o vidro reflete um pouco)
        for y in 0..sh {
            let t = (y as f32 / sh as f32).min(1.0);
            let v = 5.0 + 3.0 * t;
            d.fill(0, y, sw as u32, 1, (v as u8, v as u8, (v + 1.0) as u8, 255));
        }
        // O ponto: núcleo saturado + halo gaussiano barato em anéis
        let (cx, cy) = (sw / 2, sh / 2);
        for r in (1..=14).rev() {
            let a = (glow * (14 - r) as f32 / 14.0 * 90.0) as u8;
            let rr = (r as f32 * 1.6) as i32;
            disco(d, cx, cy, rr, (255, 240, 200, a));
        }
        disco(d, cx, cy, 3, (255, 255, 245, (255.0 * glow) as u8));
        return;
    }

    let cores: [(u8, u8, u8); 8] = [
        (200, 40, 40),
        (230, 140, 30),
        (60, 160, 70),
        (235, 200, 50),
        (90, 170, 60),
        (150, 60, 160),
        (120, 190, 80),
        (220, 70, 90),
    ];
    let frac = frac.clamp(0.0, 1.0);
    // Quanto mais colapsado, mais a tela vira branco (saturação do fósforo).
    let branco = 1.0 - frac;
    let mist = |c: (u8, u8, u8)| -> (u8, u8, u8) {
        let l = |x: u8| (x as f32 + (255.0 - x as f32) * branco * 0.85) as u8;
        (l(c.0), l(c.1), l(c.2))
    };
    let virt_h = ((sh as f32 * frac).round() as i32).max(2);
    let top = sh / 2 - virt_h / 2;

    if frac > 0.8 {
        d.text(m, top + 4, 1, DIM, "todos os jogos");
    }
    let tile = 96i32;
    let gap = 14i32;
    let cols = ((sw - 2 * m) / (tile + gap)).max(1);
    for r in 0..3usize {
        for c in 0..cols as usize {
            let cor = mist(cores[(r * cols as usize + c) % cores.len()]);
            let x = m + c as i32 * (tile + gap);
            let th = ((tile as f32 * frac) as i32).max(1);
            let y = top + 30 + r as i32 * (((tile + gap) as f32 * frac) as i32);
            if y + th > top + virt_h {
                break;
            }
            d.fill(x, y, tile as u32, th as u32, (cor.0, cor.1, cor.2, 255));
            let esc = mist((cor.0 / 3, cor.1 / 3, cor.2 / 3));
            d.fill(
                x + tile - 6,
                y + 2,
                5,
                (th - 4).max(0) as u32,
                (esc.0, esc.1, esc.2, 255),
            );
        }
    }

    // A linha do colapso: núcleo branco-estourado + halo largo decaindo +
    // franjas R/B de 1px (aberração do fósforo).
    if frac < 1.0 {
        let cy = sh / 2;
        let int = ((1.0 - frac) * glow).clamp(0.0, 1.0);
        // halo em anéis
        for (r, a) in [(6, 26), (4, 50), (2, 90)] {
            let a = (a as f32 * int) as u8;
            d.fill(0, cy - r, sw as u32, 1, (255, 240, 210, a));
            d.fill(0, cy + r - 1, sw as u32, 1, (255, 240, 210, a));
        }
        // franjas cromáticas
        d.fill(
            0,
            cy - 2,
            sw as u32,
            1,
            (255, 120, 110, (160.0 * int) as u8),
        );
        d.fill(
            0,
            cy + 1,
            sw as u32,
            1,
            (120, 150, 255, (160.0 * int) as u8),
        );
        // núcleo
        d.fill(
            0,
            cy - 1,
            sw as u32,
            2,
            (255, 255, 250, (235.0 * int + 20.0).min(255.0) as u8),
        );
        // o tubo apagando em volta
        d.fill(0, 0, sw as u32, top.max(0) as u32, (5, 5, 6, 255));
        d.fill(
            0,
            top + virt_h,
            sw as u32,
            (sh - top - virt_h).max(0) as u32,
            (5, 5, 6, 255),
        );
    }
}

fn disco(d: &mut Screen, cx: i32, cy: i32, r: i32, c: (u8, u8, u8, u8)) {
    for dy in -r..=r {
        let dx = ((r * r - dy * dy) as f32).sqrt().round() as i32;
        d.fill(cx - dx, cy + dy, (dx * 2) as u32, 1, c);
    }
}

/// A tela do "chin": a grade de ventilação e o bloom desenhados como se
/// estivessem no gabinete (aqui, dentro do tubo, para o mock).
fn tela_chin(d: &mut Screen) {
    let (sw, sh) = d.size();
    let (sw, sh) = (sw as i32, sh as i32);
    let m = 34i32;

    // Um "sinal" qualquer para o bloom ter contra o que brilhar.
    for y in 0..sh {
        for chunk in 0..(sw / 64 + 1) {
            let v = ((y * 7 + chunk * 31) % 90 + 30) as u8;
            d.fill(chunk * 64, y, 64, 1, (v, v, v, 255));
        }
    }
    d.text(m, m, 2, TEXTO, "chin: grade + bloom");

    // --- Bloom do bezel: luz do tubo vazando para dentro do recess -------
    for i in 0..6 {
        let a = (34 - i * 5) as u8;
        d.fill(i, i, (sw - 2 * i) as u32, 1, (255, 255, 255, a));
        d.fill(i, sh - 1 - i, (sw - 2 * i) as u32, 1, (255, 255, 255, a));
        d.fill(i, i, 1, (sh - 2 * i) as u32, (255, 255, 255, a));
        d.fill(sw - 1 - i, i, 1, (sh - 2 * i) as u32, (255, 255, 255, a));
    }

    // --- A grade de ventilação: frestas escuras, 3 fileiras -------------
    let gx = sw - m - 150;
    let gy = sh - 90;
    for r in 0..3 {
        let y = gy + r * 10;
        for c in 0..14 {
            let x = gx + c * 10;
            d.fill(x, y, 6, 3, (10, 9, 8, 255));
        }
    }
    d.text(gx, gy - 24, 1, DIM, "grade de ventilacao");

    // --- Pressed na tecla chiclete: o label afunda 1px ------------------
    d.text(m, gy - 60, 1, DIM, "pressed: label afunda 1px");
    chiclete(d, m, gy - 40, 140, 34, "voltar", false);
    chiclete(d, m + 160, gy - 40, 140, 34, "voltar", true);
    d.text(m, gy + 12, 1, DIM, "repouso          pressionado");
}

/// A tecla chiclete do mock anterior; `pressed` desloca o label 1px para
/// baixo e escurece o corpo um tom (o "afundar" da tecla física).
fn chiclete(d: &mut Screen, x: i32, y: i32, w: u32, h: u32, label: &str, pressed: bool) {
    let (w, h) = (w as i32, h as i32);
    let raio = (h / 5).max(2);
    let corpo = if pressed {
        (
            CHICLETE_CORPO.0 - 18,
            CHICLETE_CORPO.1 - 18,
            CHICLETE_CORPO.2 - 16,
        )
    } else {
        CHICLETE_CORPO
    };
    for r in 0..h {
        let inset = corner_inset(r, h, raio);
        d.fill(
            x + inset,
            y + r,
            (w - inset * 2).max(0) as u32,
            1,
            (
                CHICLETE_ASSENTO.0,
                CHICLETE_ASSENTO.1,
                CHICLETE_ASSENTO.2,
                255,
            ),
        );
    }
    for r in 0..h - 2 {
        let inset = corner_inset(r, h - 2, (raio - 1).max(1)).min((w - 2) / 2);
        d.fill(
            x + 1 + inset,
            y + 1 + r,
            (w - 2 - inset * 2).max(0) as u32,
            1,
            (corpo.0, corpo.1, corpo.2, 255),
        );
    }
    let tw = label.chars().count() as i32 * 9;
    let dy = if pressed { 1 } else { 0 };
    d.text(
        x + (w - tw) / 2,
        y + (h - 20) / 2 + dy,
        1,
        CHICLETE_LABEL,
        label,
    );
}

fn corner_inset(row: i32, h: i32, radius: i32) -> i32 {
    let r = radius.min(h / 2).max(0);
    let from_end = row.min(h - 1 - row);
    if from_end >= r {
        return 0;
    }
    let dx = r - from_end;
    r - ((r * r - dx * dx) as f32).sqrt().floor() as i32
}
