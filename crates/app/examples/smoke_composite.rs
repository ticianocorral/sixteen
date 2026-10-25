//! Smoke test do caminho 2D com janela: `frame_2d` e `frame_shelf` passam
//! pelo `composite_screen`, que desde o cache da CRT mesh alterna entre
//! "construir o mesh" (primeiro frame) e "reusar do cache" (segundo em
//! diante). Desenhar a MESMA tela duas vezes e capturar no fim confere os
//! dois caminhos sem pânico nem tela vazia — o BMP de saída é a prova
//! visual (o retângulo vermelho tem que aparecer warpado dentro do tubo).
use sixteen_platform::Platform;

fn main() -> anyhow::Result<()> {
    let mut plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("smoke", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let draw = |d: &mut sixteen_platform::Screen| {
        d.fill(0, 0, 640, 448, (200, 40, 40, 255));
        d.text(10, 90, 1, (220, 220, 220), "composite smoke");
    };
    for _ in 0..2 {
        cab.frame_2d((10, 10, 11), draw);
        cab.frame_shelf((10, 10, 11), draw);
    }
    cab.capture_2d((10, 10, 11), draw, std::path::Path::new("/tmp/badge/smoke-composite.bmp"))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("wrote /tmp/badge/smoke-composite.bmp");
    Ok(())
}
