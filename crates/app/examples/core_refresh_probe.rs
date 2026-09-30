//! Probe (dev/testing only) for the "atualizo o núcleo mas a versão/commit
//! não muda" report: mirrors what the app does across a core update in one
//! session — `dlopen` the installed dylib (what the startup nameplate does),
//! overwrite that same file with a different build (what the download does),
//! then `dlopen` it again and read the version. If the second read equals
//! the on-disk build, the app is not to blame: the displayed version is
//! `library_version` ("1.63 <commit>"), which only changes when the snes9x
//! upstream (`libretro/snes9x`) actually lands a new commit — the buildbot's
//! nightly rebuild of the same revision keeps the same string.
//!
//! `cargo run --example core_refresh_probe -- <dylib-antiga> <dylib-nova>`

use std::path::PathBuf;

use xperience_emulation::Core;

fn version_at(path: &std::path::Path) -> String {
    Core::load(path)
        .map(|c| c.system_version().to_string())
        .unwrap_or_else(|e| format!("(erro: {e})"))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("uso: core_refresh_probe <dylib-antiga> <dylib-nova>");
        std::process::exit(2);
    }
    let (older, newer) = (&args[1], &args[2]);
    let dir = std::env::temp_dir().join("xperience-core-probe");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let probe = dir.join("core.dylib");

    std::fs::copy(older, &probe).unwrap();
    let before = version_at(&probe);
    println!("1. dlopen do core instalado:        {before}");

    std::fs::copy(newer, &probe).unwrap();
    let after_disk = version_at(&PathBuf::from(newer));
    let after = version_at(&probe);
    println!("2. arquivo sobrescrito pela build nova ({after_disk})");
    println!("3. dlopen de novo, mesmo path:      {after}");
    if after == after_disk {
        println!("   → o dlopen re-leu o disco: o app mostra o que está instalado.");
        println!("   → se \"não muda\", é porque a build nova tem o mesmo commit upstream.");
    } else {
        println!("   → BUG: o dyld serviu a imagem já mapeada, ignorando o disco.");
    }
}
