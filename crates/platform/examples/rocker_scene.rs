//! Headless eyeball of the Power/Reset rocker restyle (dev aid — no
//! window): renders the in-game panel powered and unpowered, plus Reset
//! mid-press, to BMPs via SDL's dummy driver so the switch look can be
//! inspected without launching the app.
//!
//! An optional second argument loads real cartridge art instead of the
//! synthetic grey block — a raw RGBA frame (u32 LE width, u32 LE height,
//! then rows):
//!
//! ```sh
//! ffmpeg -i "art.png" -f rawvideo -pix_fmt rgba art.raw
//! python3 -c "import struct; d=open('art.raw','rb').read(); \
//!   open('art.bin','wb').write(struct.pack('<II',700,500)+d)"
//! cargo run -p sixteen-platform --example rocker_scene -- out art.bin
//! ```

use std::path::PathBuf;

use sixteen_platform::{Cabinet, PanelButton, Platform};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    std::env::set_var("SDL_VIDEODRIVER", "dummy");
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("rocker_scene"));
    std::fs::create_dir_all(&out)?;

    let plat = Platform::new()?;
    let mut cab: Cabinet = plat.create_cabinet("rocker scene", 1280, 800, false)?;

    // Real art via the raw file, else a stand-in grey cartridge block.
    let load_raw = |p: &str| {
        let bytes = std::fs::read(p).ok()?;
        if bytes.len() < 8 {
            return None;
        }
        let w = u32::from_le_bytes(bytes[0..4].try_into().ok()?);
        let h = u32::from_le_bytes(bytes[4..8].try_into().ok()?);
        if bytes.len() < 8 + (w as usize) * (h as usize) * 4 {
            return None;
        }
        Some((w, h, bytes[8..].to_vec()))
    };
    let art = std::env::args().nth(2).and_then(|p| load_raw(&p));
    let (w, h, rgba) = art.unwrap_or_else(|| {
        let (w, h) = (700u32, 500u32);
        let mut rgba = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 0..w {
                let i = ((y * w + x) * 4) as usize;
                rgba[i] = 138;
                rgba[i + 1] = 132;
                rgba[i + 2] = 138;
                rgba[i + 3] = 255;
            }
        }
        (w, h, rgba)
    });
    cab.set_panel(
        None,
        Some((w, h, &rgba)),
        "Exemplo",
        &[
            (PanelButton::Power, "Ligar".into()),
            (PanelButton::Reset, "Zerar".into()),
        ],
    );

    cab.set_powered(false);
    cab.capture_static_bmp(0.0, &out.join("panel_off.bmp"))?;

    cab.set_powered(true);
    cab.capture_static_bmp(0.0, &out.join("panel_on.bmp"))?;

    cab.set_reset_pressed(true);
    cab.capture_static_bmp(0.0, &out.join("panel_reset.bmp"))?;
    println!("{}", out.display());
    Ok(())
}
