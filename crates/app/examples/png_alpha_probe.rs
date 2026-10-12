//! Throwaway: contagem de alpha da capa — fundo realmente vazado (a=0) ou
//! backdrop opaco pintado (tudo a=255)?
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let mut it = std::env::args().skip(1);
    let path = it.next().map(PathBuf::from).expect("png path");
    let img = image::open(&path)?.to_rgba8();
    let (w, h) = img.dimensions();
    let (mut zero, mut opaque, mut semi, mut semi_cinza) = (0u64, 0u64, 0u64, 0u64);
    for p in img.pixels() {
        let a = p.0[3];
        if a == 0 {
            zero += 1;
        } else if a >= 250 {
            opaque += 1;
        } else {
            semi += 1;
            let (r, g, b) = (p.0[0], p.0[1], p.0[2]);
            if r.abs_diff(g) < 24 && g.abs_diff(b) < 24 {
                semi_cinza += 1;
            }
        }
    }
    let total = (w * h) as u64;
    println!(
        "{path:?}\n  {zero} a=0 ({}%), {opaque} a>=250 ({}%), {semi} semi ({semi_cinza} cinza)",
        100 * zero / total,
        100 * opaque / total
    );
    Ok(())
}
