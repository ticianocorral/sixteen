//! Load a libretro core and print what it reports — no ROM needed.
//!
//!   cargo run -p sixteen-emulation --example probe -- <path/to/core>

use sixteen_emulation::Core;

fn main() -> anyhow::Result<()> {
    let path = std::env::args()
        .nth(1)
        .or_else(|| std::env::var("SIXTEEN_CORE").ok())
        .ok_or_else(|| anyhow::anyhow!("usage: probe <core> (or set $SIXTEEN_CORE)"))?;

    let mut core = Core::load(&path)?;
    println!(
        "library      : {} {}",
        core.system_name(),
        core.system_version()
    );
    println!("extensions   : {}", core.valid_extensions().join(", "));

    core.init();
    println!("retro_init   : ok");
    println!("\ncore loads and initialises cleanly.");
    Ok(())
}
