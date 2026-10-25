//! Shot do painel do idle com os três botões (estante, livraria, museu).
use sixteen_platform::Platform;

fn main() -> anyhow::Result<()> {
    let out = std::path::PathBuf::from("/tmp/badge");
    std::fs::create_dir_all(&out)?;
    let mut plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("SixteeN", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_powered(true);
    cab.capture_static_bmp(0.08, &out.join("museu.bmp"))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("wrote {}", out.join("museu.bmp").display());
    Ok(())
}
