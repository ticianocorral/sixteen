//! Headless shot of the brand art (dev/testing only):
//! `cargo run --example brand_shot -- [dir]` writes `brand-idle.bmp` —
//! the idle screen with the baked-in console logo on the panel and the
//! slot tag on the cartridge base, the same path `console_art` uses (decode
//! → `set_console_logo`/`set_slot_tag`), so a rebrand can be eyeballed
//! without opening a window.

use std::path::{Path, PathBuf};

use sixteen_app::idle;
use sixteen_platform::Platform;

fn main() -> anyhow::Result<()> {
    let dir = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "/tmp".into()));
    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("SixteeN", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("SixteeN v1.1.5\nsnes9x 1.63 TaC2rea");

    let logo = image::load_from_memory(include_bytes!("../assets/console_logo.png"))?
        .thumbnail(640, 640)
        .to_rgba8();
    cab.set_console_logo(Some((logo.width(), logo.height(), logo.as_raw().as_slice())));

    let tag = image::load_from_memory(include_bytes!("../assets/console_tag.png"))?
        .thumbnail(1024, 1024)
        .to_rgba8();
    cab.set_slot_tag(Some((tag.width(), tag.height(), tag.as_raw().as_slice())));

    let shot = dir.join("brand-idle.bmp");
    idle::capture_preview(&mut cab, idle::RESTING_STATIC, true, true, Path::new(&shot))?;
    println!("wrote {}", shot.display());
    Ok(())
}
