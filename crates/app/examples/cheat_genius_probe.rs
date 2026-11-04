//! Dev/testing: roda a ROM do Cheat Genius no core de verdade, sem janela.
//!
//!   cargo run --release -p sixteen-app --example cheat_genius_probe -- \
//!       --core <snes9x_libretro.dylib> [--entries N] [--title T] SCRIPT...
//!
//! SCRIPT é uma sequência de passos: `run:N` (N quadros), `press:BOTAO`
//! (aperta 2 quadros e solta 2), `hold:BOTAO:N`, `shot:ARQUIVO.png`, `status`.

use std::path::{Path, PathBuf};

use sixteen_app::genius::{self, Entry};
use sixteen_emulation::{Button, Core, Frame, PixelFormat};

fn rgb(frame: &Frame) -> image::RgbImage {
    let mut img = image::RgbImage::new(frame.width, frame.height);
    for y in 0..frame.height as usize {
        let row = &frame.pixels[y * frame.pitch..];
        for x in 0..frame.width as usize {
            let px = match frame.format {
                PixelFormat::Rgb565 => {
                    let v = u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]);
                    let (r, g, b) = ((v >> 11) & 31, (v >> 5) & 63, v & 31);
                    [
                        (r << 3 | r >> 2) as u8,
                        (g << 2 | g >> 4) as u8,
                        (b << 3 | b >> 2) as u8,
                    ]
                }
                PixelFormat::Rgb1555 => {
                    let v = u16::from_le_bytes([row[x * 2], row[x * 2 + 1]]);
                    let (r, g, b) = ((v >> 10) & 31, (v >> 5) & 31, v & 31);
                    [
                        (r << 3 | r >> 2) as u8,
                        (g << 3 | g >> 2) as u8,
                        (b << 3 | b >> 2) as u8,
                    ]
                }
                PixelFormat::Xrgb8888 => [row[x * 4 + 2], row[x * 4 + 1], row[x * 4]],
            };
            img.put_pixel(x as u32, y as u32, image::Rgb(px));
        }
    }
    img
}

fn button(name: &str) -> Button {
    match name.to_ascii_lowercase().as_str() {
        "a" => Button::A,
        "b" => Button::B,
        "x" => Button::X,
        "y" => Button::Y,
        "l" => Button::L,
        "r" => Button::R,
        "start" => Button::Start,
        "select" => Button::Select,
        "up" => Button::Up,
        "down" => Button::Down,
        "left" => Button::Left,
        "right" => Button::Right,
        other => panic!("botão desconhecido {other}"),
    }
}

fn main() {
    let mut core_path = std::env::var_os("SIXTEEN_CORE").map(PathBuf::from);
    let mut n = 40usize;
    let mut title = "Aladdin (USA)".to_string();
    let mut script = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--core" => core_path = it.next().map(PathBuf::from),
            "--entries" => n = it.next().unwrap().parse().unwrap(),
            "--title" => title = it.next().unwrap(),
            _ => script.push(a),
        }
    }
    let mut core = Core::load(core_path.expect("--core")).expect("core");
    let tmp = std::env::temp_dir().join("cheat_genius_probe");
    std::fs::create_dir_all(&tmp).unwrap();
    core.set_directories(&tmp, &tmp);
    core.init();
    core.load_game(Path::new("cheat_genius.sfc"), genius::ROM)
        .expect("load rom");

    let entries: Vec<Entry> = (0..n)
        .map(|i| Entry {
            desc: format!("Cheat number {i:04} with a long description"),
            on: i % 7 == 3,
            custom: i < 2,
        })
        .collect();
    assert!(genius::start(&mut core, &title, &entries));

    let mut last: Option<Frame> = None;
    let step = |core: &mut Core, last: &mut Option<Frame>| {
        core.run();
        if let Some(f) = core.take_frame() {
            *last = Some(f);
        }
    };
    for s in script {
        let mut p = s.splitn(3, ':');
        match (p.next().unwrap(), p.next(), p.next()) {
            ("run", Some(k), _) => {
                (0..k.parse::<usize>().unwrap()).for_each(|_| step(&mut core, &mut last))
            }
            ("press", Some(b), _) => {
                core.set_button(0, button(b), true);
                step(&mut core, &mut last);
                step(&mut core, &mut last);
                core.set_button(0, button(b), false);
                step(&mut core, &mut last);
                step(&mut core, &mut last);
            }
            ("hold", Some(b), Some(k)) => {
                core.set_button(0, button(b), true);
                (0..k.parse::<usize>().unwrap()).for_each(|_| step(&mut core, &mut last));
                core.set_button(0, button(b), false);
                step(&mut core, &mut last);
            }
            ("shot", Some(f), _) => {
                rgb(last.as_ref().expect("sem frame")).save(f).unwrap();
                println!("wrote {f}");
            }
            ("game", Some(path), _) => {
                // a troca que o runner faz: descarrega a ROM do Genius, carrega o jogo
                let path = format!(
                    "{}{}",
                    path,
                    p.next().map(|r| format!(":{r}")).unwrap_or_default()
                );
                let bytes = sixteen_domain::library::load_rom(Path::new(&path)).expect("rom");
                core.unload_game();
                core.load_game(Path::new(&path), &bytes).expect("load game");
                println!("game loaded: {} bytes", bytes.len());
            }
            ("cheat", Some(code), _) => {
                core.cheat_reset();
                core.cheat_set(0, true, code);
            }
            ("ram", Some(addr), _) => {
                let a = usize::from_str_radix(addr, 16).unwrap();
                core.with_memory(sixteen_emulation::MEMORY_SYSTEM_RAM, |r| {
                    println!("ram[{a:06X}] = {:02X}", r[a])
                });
            }
            ("status", _, _) => println!("{:?}", genius::poll(&mut core, n)),
            (other, _, _) => panic!("passo desconhecido {other}"),
        }
    }
}
