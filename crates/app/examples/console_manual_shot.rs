//! Prints do manual do console: a tela inicial com o botão "Manual do
//! console" e o leitor aberto no tubo.
//!
//! Rodar: `SIXTEEN_LANG=pt cargo run -p sixteen-app --example
//! console_manual_shot` — escreve BMPs em /tmp/console-manual/.

use std::path::PathBuf;

use sixteen_app::idle;
use sixteen_platform::Platform;

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from("/tmp/console-manual");
    std::fs::create_dir_all(&out)?;
    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("SixteeN", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    idle::capture_preview(
        &mut cab,
        idle::RESTING_STATIC,
        true,
        true,
        &out.join("inicial.bmp"),
    )?;
    for page in [1, 3] {
        idle::capture_console_manual_preview(
            &mut cab,
            page,
            &out.join(format!("manual-{page}.bmp")),
        )?;
    }
    Ok(())
}
