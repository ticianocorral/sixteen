//! Shot do badge RA (lugar + copy) em offscreen: chama `capture_bmp` com um
//! frame sintético (sem ROM/núcleo) e grava o BMP para inspeção.
//!
//! Rodar: `cargo run -p sixteen-app --example ra_badge_shot` — escreve em
//! /tmp/badge/.

use std::path::PathBuf;
use std::time::Duration;

use sixteen_platform::{FrameRef, PixelFormat, Platform, RaStatus, DEMO_BADGE_IMG, RA_LOGO_IMG};

const W: u32 = 1280;
const H: u32 = 800;

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from("/tmp/badge");
    std::fs::create_dir_all(&out)?;

    let mut plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("SixteeN", W, H, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("SixteeN v1.2.0\nsnes9x 1.63");
    if let Ok(icon) = image::load_from_memory(sixteen_app::RA_ICON_PNG) {
        let icon = icon.to_rgba8();
        cab.set_image(RA_LOGO_IMG, icon.width(), icon.height(), icon.as_raw());
    }

    cab.set_powered(true);
    cab.set_session_time(std::time::Duration::from_secs(1_493));
    cab.set_ra_status(Some(RaStatus { hardcore: true }));

    // Frame 4:3 sintético, sem núcleo — um "jogo" chapado (céu + chão de
    // faixas) só para o vidro ter algo na TV embaixo do badge. Bytes XRGB:
    // na sequência blue, green, red (little-endian).
    let (w, h) = (640usize, 480usize);
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) * 4;
            let (r, g, b) = if y < 300 {
                let t = y as f32 / 300.0;
                (
                    (30.0 + 40.0 * t) as u8,
                    (70.0 + 50.0 * t) as u8,
                    (120.0 + 30.0 * t) as u8,
                )
            } else {
                let g2 = if (x / 24 + y / 12) % 2 == 0 { 90 } else { 70 };
                (40, g2, 40)
            };
            px[i] = b;
            px[i + 1] = g;
            px[i + 2] = r;
            px[i + 3] = 255;
        }
    }
    let frame = FrameRef {
        width: w as u32,
        height: h as u32,
        pitch: w * 4,
        format: PixelFormat::Xrgb8888,
        pixels: &px,
    };
    cab.capture_bmp(&frame, 4.0 / 3.0, &out.join("badge.bmp"))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("wrote {}", out.join("badge.bmp").display());

    // Notificação no lugar (fila de OSD): o desbloqueio REAL tem 4 linhas
    // (cabeçalho, nome, pontos, modo) — o caso que esbarrava na faixa.
    cab.set_image(DEMO_BADGE_IMG, 64, 64, &demo_badge());
    cab.push_osd(
        &[
            "CONQUISTA DESBLOQUEADA",
            "Bem-vindo ao Mundo",
            "+5 pontos",
            "modo hardcore",
        ],
        Some(DEMO_BADGE_IMG),
        std::time::Duration::from_secs(6),
    );
    cab.capture_bmp(&frame, 4.0 / 3.0, &out.join("notificacao.bmp"))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("wrote {}", out.join("notificacao.bmp").display());

    // Aviso de hábito sem badge (savestate bloqueado no hardcore) — em um
    // gabinete novo, para a fila do primeiro não aninhar o desenho.
    let mut cab2 = plat
        .create_cabinet("SixteeN", W, H, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab2.set_nameplate("SixteeN v1.2.0\nsnes9x 1.63");
    cab2.set_powered(true);
    cab2.set_session_time(std::time::Duration::from_secs(1_493));
    cab2.set_ra_status(Some(RaStatus { hardcore: true }));
    cab2.push_osd(
        &["MODO HARDCORE", "savestates bloqueados"],
        None,
        Duration::from_secs(3),
    );
    cab2.capture_bmp(&frame, 4.0 / 3.0, &out.join("aviso.bmp"))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("wrote {}", out.join("aviso.bmp").display());
    Ok(())
}

/// Badge 64×64 sintético (disco) — stand-in do badge da conta.
fn demo_badge() -> Vec<u8> {
    const S: u32 = 64;
    let mut px = vec![0u8; (S * S * 4) as usize];
    for y in 0..S as i32 {
        for x in 0..S as i32 {
            let (dx, dy) = (x - 32, y - 32);
            let d = (dx * dx + dy * dy) as f32;
            let i = ((y * S as i32 + x) * 4) as usize;
            if d <= 30.0 * 30.0 {
                let c = if d >= 26.0 * 26.0 {
                    (245, 238, 220)
                } else {
                    (200, 40, 40)
                };
                px[i] = c.0;
                px[i + 1] = c.1;
                px[i + 2] = c.2;
                px[i + 3] = 255;
            }
        }
    }
    px
}
