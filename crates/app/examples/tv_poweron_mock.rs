//! MOCK — a TV ligando, o caminho de volta do `tv_colapso_mock` (e o estado
//! que o app abriria: TV desligada). Um CRT de verdade não acende de
//! repente: o filamento aquece, um brilho fraco nasce no centro, vira uma
//! linha, a linha abre revelando a neve ainda estourada de brilho, e só
//! depois de uns dois segundos o brilho assenta e o sinal aparece.
//! Keyframes: apagada -> ponto fraco -> ponto forte -> linha -> raster
//! abrindo (neve estourada, bordas quentes, spill no vidro) -> raster
//! aberto assentando -> estante. 100% sintético; nada do app muda.
//!
//! Run: cargo run -p sixteen-app --example tv_poweron_mock — escreve
//! /tmp/tv-mock/poweron-1..7.bmp (converter com sips para png).

use std::path::PathBuf;

use sixteen_platform::{Platform, Screen};

const BG: (u8, u8, u8) = (16, 14, 13);
const DIM: (u8, u8, u8) = (140, 134, 124);

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from("/tmp/tv-mock");
    std::fs::create_dir_all(&out)?;

    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("SixteeN mock", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("SixteeN v1.2.0\nsnes9x 1.63");

    // (abertura do raster 0..1, brilho extra do fósforo 0..1, neve?)
    let fases: [(f32, f32, bool); 7] = [
        (0.0, 0.0, false),  // 1: apagada
        (0.0, 0.35, false), // 2: ponto fraco (filamento acordando)
        (0.0, 1.0, false),  // 3: ponto forte
        (0.08, 1.0, false), // 4: virou linha
        (0.45, 0.85, true), // 5: raster abrindo, neve estourada
        (1.0, 0.35, true),  // 6: raster aberto, brilho assentando
        (1.0, 0.0, true),   // 7: brilho normal — a estante aparece
    ];
    for (n, (frac, hot, neve)) in fases.into_iter().enumerate() {
        let path = out.join(format!("poweron-{}.bmp", n + 1));
        cab.capture_shelf(BG, |d| frame(d, frac, hot, neve), &path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    }

    println!("{}", out.display());
    Ok(())
}

/// Um frame da TV acordando. `frac` é quanto do raster vertical já abriu
/// (0 = nada, só o ponto/linha; 1 = tela cheia), `hot` o brilho extra do
/// fósforo (0 = normal), `neve` se já há sinal (a neve) dentro do raster.
fn frame(d: &mut Screen, frac: f32, hot: f32, neve: bool) {
    let (sw, sh) = d.size();
    let (sw, sh) = (sw as i32, sh as i32);

    // O vidro desligado: quase preto, com um resíduo mínimo e um reflexo
    // diagonal fraquíssimo — nunca preto puro.
    for y in 0..sh {
        let t = y as f32 / sh as f32;
        let v = 5.0 + 3.0 * t;
        d.fill(0, y, sw as u32, 1, (v as u8, v as u8, (v + 1.0) as u8, 255));
    }
    reflexo_vidro(d, sw, sh, 5);

    if frac <= 0.0 {
        // Só o ponto: núcleo + halo largo, intensidade `hot`.
        if hot > 0.0 {
            let (cx, cy) = (sw / 2, sh / 2);
            for r in (1..=18).rev() {
                let a = (hot * (18 - r) as f32 / 18.0 * 70.0) as u8;
                disco(d, cx, cy, (r as f32 * 1.8) as i32, (255, 240, 200, a));
            }
            disco(d, cx, cy, 3, (255, 255, 245, (255.0 * hot) as u8));
        }
        return;
    }

    // O raster abrindo: uma faixa central de altura sh*frac.
    let frac = frac.clamp(0.02, 1.0);
    let virt_h = ((sh as f32 * frac).round() as i32).max(2);
    let top = sh / 2 - virt_h / 2;

    // Spill ANTES do conteúdo: a faixa acesa ilumina o vidro em volta —
    // gradientes quentes de ~24px sangrando para fora do raster.
    if frac < 1.0 {
        for i in 0..24 {
            let k = 1.0 - i as f32 / 24.0;
            let a = (70.0 * hot * k * k) as u8;
            if a == 0 {
                break;
            }
            d.fill(0, top - i, sw as u32, 1, (255, 238, 205, a));
            d.fill(0, top + virt_h + i, sw as u32, 1, (255, 238, 205, a));
        }
    }

    if neve {
        // Neve estourada: a maioria dos pixels clipa perto do branco — só
        // alguns pontos escuros variam. O `hot` faz o clipar.
        let mut rng = 0x9E37_79B9u32;
        let clip = (0.45 + 0.5 * hot).clamp(0.0, 0.95); // fração que estoura
        for i in 0..virt_h {
            // a deflexão ainda rampa: o ruído "esticado" — uma linha de
            // neve cobre 2px de tela nos primeiros frames
            let y = top + i;
            for x in (0..sw).step_by(2) {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                let r = rng & 0xFF;
                let v = if (r as f32 / 255.0) < clip {
                    210 + (rng % 46) as u8 // estourado
                } else {
                    50 + (rng % 120) as u8 // ponto escuro variando
                };
                let hh = if frac < 0.8 { 2 } else { 1 };
                d.fill(x, y, 2, hh, (v, v, v, 255));
            }
        }
    } else {
        // Antes do sinal: o raster vazio, um cinza quente.
        let v = (30.0 + 60.0 * hot) as u8;
        d.fill(0, top, sw as u32, virt_h as u32, (v, v, (v + 4), 255));
    }

    // As bordas do raster: gradiente âmbar de ~10px fundindo com o conteúdo
    // (o feixe acelera por ali — fósforo hit harder), não uma linha seca.
    if frac < 1.0 {
        for i in 0..10 {
            let k = 1.0 - i as f32 / 10.0;
            let a = (235.0 * hot * (0.35 + 0.65 * k)) as u8;
            let c = if i < 4 {
                (255, 252, 235)
            } else {
                (255, 214, 150)
            };
            d.fill(0, top + i, sw as u32, 1, (c.0, c.1, c.2, a));
            d.fill(0, top + virt_h - 1 - i, sw as u32, 1, (c.0, c.1, c.2, a));
        }
    } else if hot > 0.0 {
        // Raster aberto mas ainda quente: bloom geral nas bordas do vidro.
        for i in 0..4 {
            let a = ((26.0 * hot) as u8).saturating_mul((4 - i) as u8) / 4;
            d.fill(i, i, (sw - 2 * i) as u32, 1, (255, 255, 255, a.max(1)));
            d.fill(
                i,
                sh - 1 - i,
                (sw - 2 * i) as u32,
                1,
                (255, 255, 255, a.max(1)),
            );
        }
    }

    // A estante só existe com raster aberto e brilho assentado.
    if frac >= 1.0 && hot <= 0.0 {
        estante(d, sw, sh);
    }
}

/// A "estante" de brinquedo do mock do colapso — mesmo desenho, para os
/// dois mocks conversarem.
fn estante(d: &mut Screen, sw: i32, _sh: i32) {
    let m = 34i32;
    d.text(m, m, 1, DIM, "todos os jogos");
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
    let tile = 96i32;
    let gap = 14i32;
    let cols = ((sw - 2 * m) / (tile + gap)).max(1);
    for r in 0..3usize {
        for c in 0..cols as usize {
            let cor = cores[(r * cols as usize + c) % cores.len()];
            let x = m + c as i32 * (tile + gap);
            let y = m + 34 + r as i32 * (tile + gap);
            d.fill(x, y, tile as u32, tile as u32, (cor.0, cor.1, cor.2, 255));
            d.fill(
                x + tile - 6,
                y + 2,
                5,
                tile as u32 - 4,
                (cor.0 / 3, cor.1 / 3, cor.2 / 3, 255),
            );
        }
    }
}

fn disco(d: &mut Screen, cx: i32, cy: i32, r: i32, c: (u8, u8, u8, u8)) {
    for dy in -r..=r {
        let dx = ((r * r - dy * dy) as f32).sqrt().round() as i32;
        d.fill(cx - dx, cy + dy, (dx * 2) as u32, 1, c);
    }
}

/// O reflexo do vidro desligado — a mesma faixa diagonal do
/// `tv_botoes_mock`, aqui com alpha mínimo (só para o vidro não ser morto).
fn reflexo_vidro(d: &mut Screen, sw: i32, sh: i32, base: u8) {
    let len = (sw / 3).min(sh / 2);
    let esp = 26i32;
    for i in 0..len {
        for e in 0..esp {
            let x = i;
            let y = sh / 4 + esp - i - e;
            if x >= 0 && x < sw && y >= 0 && y < sh {
                let a = match e {
                    0..=3 => base,
                    4..=18 => base / 2,
                    _ => base / 4,
                };
                if a > 0 {
                    d.fill(x, y, 1, 1, (255, 255, 255, a));
                }
            }
        }
    }
}
