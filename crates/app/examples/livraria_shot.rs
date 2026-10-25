//! Shots da Livraria Retro (plan revision: "adicionar um novo botão… mostra
//! um livreiro na tv") — semeia revistas PDF pequenininhas numa pasta
//! `magazine` ISOLADA (HOME falso → nada toca a pasta do usuário de
//! verdade), captura a estante com as capas, o leitor na página 2 e o
//! campo de URL do download.
//!
//! Rodar: `cargo run -p sixteen-app --example livraria_shot` — escreve em
//! /tmp/badge/livraria-{estante,leitor,url}.bmp.

use std::path::PathBuf;

use sixteen_platform::Platform;

const W: u32 = 1280;
const H: u32 = 800;

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from("/tmp/badge");
    std::fs::create_dir_all(&out)?;

    // HOME falso: o app_root da livraria nasce LÁ (pastas do exemplo), não
    // no Application Support de verdade.
    let home = std::env::temp_dir().join("sixteen-livraria-shot-home");
    std::fs::create_dir_all(&home)?;
    std::env::set_var("HOME", &home);

    // --- semeia revistas: capas de cores diferentes, 3 páginas cada ---
    let mag = sixteen_app::dirs::magazine_dir();
    make_magazine_pdf(&mag.join("Chip_Brasil_ED_04-1987.pdf"), coverpalette(0))?;
    make_magazine_pdf(&mag.join("Super_Jogos_Brazil-1988.pdf"), coverpalette(1))?;
    make_magazine_pdf(&mag.join("Zeta_Neo_Ordenadores-1989.pdf"), coverpalette(2))?;

    // --- estado do documento da TV ---
    let mut plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("SixteeN", W, H, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("SixteeN v1.3.2\nsnes9x 1.63");
    cab.set_powered(true);

    // (1) A ESTANTE (o livreiro): duas prateleiras, capas = página 1.
    sixteen_app::livraria::capture_preview(&mut cab, &out.join("livraria-estante.bmp"))?;
    println!("wrote {}", out.join("livraria-estante.bmp").display());

    // (2) O LEITOR: página 2 da primeira da fila, com os botões ‹/› vivos.
    sixteen_app::livraria::capture_reader_preview(&mut cab, 2, &out.join("livraria-leitor.bmp"))?;
    println!("wrote {}", out.join("livraria-leitor.bmp").display());

    // (3) O CAMPO do download (BAIXAR URL): draft colado + caret.
    sixteen_app::livraria::capture_url_preview(&mut cab, &out.join("livraria-url.bmp"))?;
    println!("wrote {}", out.join("livraria-url.bmp").display());
    Ok(())
}

/// Três paletas de capa (uma por revista) e uma página interna distinta por
/// página — o leitor mostra a página 2, então ela MESMO precisa de arte.
fn coverpalette(i: usize) -> Vec<([u8; 3], [u8; 3], [u8; 3])> {
    match i {
        0 => vec![
            ([30, 120, 60], [220, 210, 90], [16, 16, 16]), // verde/yellow/preto
            ([200, 200, 205], [70, 70, 75], [220, 60, 50]),
            ([60, 90, 200], [220, 220, 230], [200, 170, 40]),
        ],
        1 => vec![
            ([200, 60, 50], [250, 240, 200], [30, 30, 36]), // vermelho/creme/preto
            ([240, 230, 210], [180, 60, 50], [40, 40, 50]),
            ([160, 40, 40], [230, 210, 180], [60, 60, 70]),
        ],
        _ => vec![
            ([40, 80, 180], [230, 230, 235], [240, 200, 40]), // azul/branco/amarelo
            ([220, 220, 230], [40, 80, 180], [80, 90, 100]),
            ([30, 60, 150], [200, 210, 220], [230, 200, 60]),
        ],
    }
}

/// Um JPEG desenhistico: faixa de fundo, um bloco diagonal e uma tabela de
/// "tira" falsa — suficiente para ver a capa/página no tubo, sem texto.
fn paint_jpeg(w: u32, h: u32, bg: [u8; 3], band: [u8; 3], accent: [u8; 3]) -> Vec<u8> {
    let mut img = image::RgbImage::new(w, h);
    for (x, y, px) in img.enumerate_pixels_mut() {
        // Fundo com degradê vertical sutil.
        let t = y as f32 / h as f32;
        let mut c = [
            (bg[0] as f32 * (1.0 - 0.25 * t)) as u8,
            (bg[1] as f32 * (1.0 - 0.25 * t)) as u8,
            (bg[2] as f32 * (1.0 - 0.25 * t)) as u8,
        ];
        let diagonal = (x as i64) + (y as i64) - (w as i64 / 3);
        if y > h / 3 && y < h * 3 / 5 {
            c = band;
        } else if diagonal.abs() < (w as i64 / 14) {
            c = accent;
        }
        if x > w * 9 / 10 || y < h / 22 {
            c = [c[0] / 2, c[1] / 2, c[2] / 2];
        }
        *px = image::Rgb(c);
    }
    let mut bytes = Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 82)
        .encode_image(&img)
        .expect("encode do semente");
    bytes.into_inner()
}

use std::io::Cursor;

/// Um PDF de 3 páginas (uma imagem por página, exatamente como os scans de
/// revista que a livraria lê) — o mesmo caminho que `manual.rs` percorre.
fn make_magazine_pdf(path: &PathBuf, palette: Vec<([u8; 3], [u8; 3], [u8; 3])>) -> anyhow::Result<()> {
    use lopdf::{dictionary, Document, Object, Stream};

    let mut pages = Vec::new();
    for p in 0..3 {
        let (bg, band, accent) = palette[p % palette.len()];
        // Uma capa 3/4 (w240×h320) e páginas internas em A4 na horizontal
        // (842×595) — capas no vão do tile, páginas largas no leitor.
        let (w, h) = if p == 0 {
            (240u32, 320u32)
        } else {
            (640u32, 480u32)
        };
        let jpeg = paint_jpeg(w, h, bg, band, accent);
        pages.push((w, h, jpeg));
    }

    let mut doc = Document::with_version("1.5");
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {"Type" => "Pages"}));
    let mut page_refs = Vec::new();
    for (idx, (w, h, jpeg)) in pages.into_iter().enumerate() {
        let img_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Image",
                "Width" => Object::Integer(w as i64),
                "Height" => Object::Integer(h as i64),
                "ColorSpace" => "DeviceRGB",
                "BitsPerComponent" => Object::Integer(8),
                "Filter" => "DCTDecode",
            },
            jpeg,
        )));
        let content_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            format!("q /Im{idx} Do Q").into_bytes(),
        )));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(w as i64),
                Object::Integer(h as i64),
            ]),
            "Resources" => Object::Dictionary(dictionary! {
                "XObject" => Object::Dictionary(dictionary! { format!("Im{idx}") => img_id }),
            }),
            "Contents" => content_id,
        }));
        page_refs.push(Object::Reference(page_id));
    }
    doc.set_object(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => page_refs,
            "Count" => Object::Integer(3),
        }),
    );
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    }));
    doc.trailer
        .set("Root", Object::Reference(catalog_id));
    doc.save(path)?;
    Ok(())
}
