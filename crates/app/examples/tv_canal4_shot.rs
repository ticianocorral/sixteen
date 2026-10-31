//! Prints da interferência do canal 4: a TV ligada no canal vizinho do
//! modulador, com o mesmo quadro de jogo em vários instantes (o sinal rola,
//! rasga e oscila), mais o canal 3 limpo para comparar.
//!
//! Rodar: `cargo run -p sixteen-app --example tv_canal4_shot` — escreve em
//! /tmp/canal4/{ch3,ch4-N}.bmp.

use std::path::PathBuf;
use std::time::Duration;

use sixteen_platform::{FrameRef, PixelFormat, Platform};

/// Frames até o tubo terminar de aquecer (ver `tv_chin_shot`).
const WARM_FRAMES: u32 = 121;

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from("/tmp/canal4");
    std::fs::create_dir_all(&out)?;

    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("SixteeN", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_powered(true);

    // Um quadro de 256x224 como o do core: a capa de exemplo, colorida e
    // com texto — boa para ver cor, fantasma e borrão.
    let art = image::load_from_memory(include_bytes!("../assets/example-cover.png"))?
        .resize_exact(256, 224, image::imageops::FilterType::Triangle)
        .to_rgba8();
    let mut px = Vec::with_capacity(256 * 224 * 4);
    for p in art.pixels() {
        px.extend_from_slice(&[p[2], p[1], p[0], 255]);
    }
    let frame = FrameRef {
        width: 256,
        height: 224,
        pitch: 256 * 4,
        format: PixelFormat::Xrgb8888,
        pixels: &px,
    };

    // A TV pode já abrir ligada (o estado do último uso); só liga se não.
    if !cab.tv_on() {
        cab.toggle_tv_power();
    }
    for _ in 0..WARM_FRAMES {
        cab.present_frame(&frame, 4.0 / 3.0);
    }
    cab.capture_bmp(&frame, 4.0 / 3.0, &out.join("ch3.bmp"))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.toggle_channel();
    for n in 0..6 {
        for _ in 0..40 {
            cab.present_frame(&frame, 4.0 / 3.0);
            std::thread::sleep(Duration::from_millis(5));
        }
        let path = out.join(format!("ch4-{n}.bmp"));
        cab.capture_bmp(&frame, 4.0 / 3.0, &path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}
