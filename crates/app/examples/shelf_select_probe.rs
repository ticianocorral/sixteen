//! Throwaway: vê a estante real com a seleção MOVIDA do primeiro tile —
//! clica no tile 2 da grade (deseleciona o DKC) e salva o frame final
//! (opts.shot captura na saída).
use std::path::PathBuf;
use std::time::Duration;

use sixteen_app::dirs;
use sixteen_app::shelf::{self, ShelfOpts};
use sixteen_domain::{Catalog, NoIntroDat, Order};
use sixteen_platform::Platform;

fn main() -> anyhow::Result<()> {
    let mut plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("SixteeN", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let dat = NoIntroDat::load(&dirs::nointro_dat_path()).ok();
    let catalog = Catalog::open(&dirs::roms_dir(), &dirs::library_path(), dat.as_ref())
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    // O cursor sobre o segundo tile — o botão de play desenha nele.
    plat.debug_set_pointer(Some((493, 339)));

    // Visita 1: shot cedo, DKC focused. Visita 2 (o mesmo Cabinet — as
    // texturas persistem, como ao voltar de um jogo): clique no tile 2 aos
    // 2s deseleciona o DKC; se a transparência sobreviver à reentrada, a
    // flag mora na textura e não num registro paralelo.
    let opts1 = ShelfOpts {
        order: Order::Shelf,
        max_frames: Some(240),
        shot: Some(PathBuf::from("/tmp/shelf_visit1.bmp")),
        fade_in: None,
        preset_filter: None,
        ra: None,
    };
    let _ = shelf::run(&mut plat, &mut cab, &catalog, &opts1)?;
    plat.push_synthetic_click_later(493, 339, Duration::from_secs(2));
    let opts2 = ShelfOpts {
        order: Order::Shelf,
        max_frames: Some(700),
        shot: Some(PathBuf::from("/tmp/shelf_visit2.bmp")),
        fade_in: None,
        preset_filter: None,
        ra: None,
    };
    let _ = shelf::run(&mut plat, &mut cab, &catalog, &opts2)?;
    println!("saiu; frames em /tmp/shelf_visit1.bmp e /tmp/shelf_visit2.bmp");
    Ok(())
}
