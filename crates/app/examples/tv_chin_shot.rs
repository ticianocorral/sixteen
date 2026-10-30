//! Shot dos controles do queixo da TV — static (chin-off/chin-on) e a
//! gameplay: o frame sintético com o chin por perto de cima do jogo, e o
//! mesmo jogo com a TV DESLIGADA (vidro morto onde estaria a imagem —
//! "desligar a tv com o jogo rodando, o jogo segue funcionando").
//!
//! Rodar: `cargo run -p sixteen-app --example tv_chin_shot` — escreve em
//! /tmp/badge/chin-{off,on,game,game-off}.bmp.

use std::path::PathBuf;
use std::time::Duration;

use sixteen_platform::{FrameRef, PixelFormat, Platform};

const W: u32 = 1280;
const H: u32 = 800;

/// Frames de warm-up do tubo (60fps) — quantos presents para chegar do
/// Warming ao estado LIGADA (`None`): as fases avançam APENAS nos
/// presents/frames vivos; um a mais, por causa do +1 do contador.
const WARM_FRAMES: u32 = 121;

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from("/tmp/badge");
    std::fs::create_dir_all(&out)?;

    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("SixteeN", W, H, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("SixteeN v1.2.0\nsnes9x 1.63");
    cab.set_powered(true);
    cab.set_session_time(Duration::from_secs(1_493));

    // Frame 4:3 sintético — um "jogo" chapado (céu + chão de faixas) para
    // o vidro ter imagem na TV (bytes XRGB: blue, green, red).
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

    // (1) TV DESLIGADA com o "jogo rodando": a captura é uma foto do
    // estado — vidro morto onde estaria a imagem, o jogo impossível de
    // ver, o chin aço no escuro (o que a janela real mostraria).
    cab.capture_bmp(&frame, 4.0 / 3.0, &out.join("chin-game-off.bmp"))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("wrote {}", out.join("chin-game-off.bmp").display());

    // (2) Static: chin aço (estado Off) e chin prata acesa. O presente
    // adianta o Warming para o estado ligada (None) — jeden das fazes
    // avançadas APENAS pelos present/frames vivos.
    cab.capture_static_bmp(0.08, &out.join("chin-off.bmp"))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("wrote {}", out.join("chin-off.bmp").display());
    cab.toggle_tv_power();
    for _ in 0..WARM_FRAMES {
        cab.present_static(0.08);
    }
    cab.capture_static_bmp(0.08, &out.join("chin-on.bmp"))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("wrote {}", out.join("chin-on.bmp").display());

    // (3) GAMEPLAY com a TV ligada: a imagem do jogo + o chin prata —
    // "tem que estar sempre na tela". E o CH 3 do religar (plan
    // revision: "mostrar o CH 3 novamente por alguns segundos, até a
    // tv entender que esta dando sinal") — os presents levaram menos
    // de 3s de parede, o banner do toggle ainda está no prazo, sobre a
    // imagem recém-aberta.
    cab.capture_bmp(&frame, 4.0 / 3.0, &out.join("chin-game.bmp"))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("wrote {}", out.join("chin-game.bmp").display());

    // (4) Passado o prazo (3s): a TV entendeu que dá sinal — a imagem
    // fica limpa, sem banner.
    std::thread::sleep(Duration::from_millis(3_200));
    cab.capture_bmp(&frame, 4.0 / 3.0, &out.join("chin-game-settled.bmp"))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("wrote {}", out.join("chin-game-settled.bmp").display());

    // (5) Canal 4 (a tecla CH do queixo): a interferência por cima da
    // imagem — estático picado + a barra de hum, o banner "CH  4" aceso.
    cab.toggle_channel();
    cab.capture_bmp(&frame, 4.0 / 3.0, &out.join("chin-game-ch4.bmp"))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("wrote {}", out.join("chin-game-ch4.bmp").display());
    Ok(())
}
