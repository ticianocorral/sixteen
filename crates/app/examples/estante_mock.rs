//! MOCK — a estante de games do SixteeN desenhada como uma estante de
//! verdade (irmão do `estante_mock` do PolystnX, mesmo conceito nos dois
//! apps): prateleiras de madeira, jogos em pé, sombras — e o Donkey Kong
//! Country como cover 3D de verdade: frente + lateral + topo em
//! perspectiva numa textura RGBA com o fundo transparente, que compõe na
//! prateleira (sombra de contato assada no próprio PNG-mental), parecendo
//! uma caixa pousada de verdade. 100% sintético.
//!
//! Run: cargo run -p sixteen-app --example estante_mock — escreve
//! /tmp/estante-mock/estante-snes.bmp (converter com sips para png).

#![allow(clippy::too_many_arguments)]

use std::path::PathBuf;

use sixteen_platform::{Platform, ShelfPanelInfo};

const LARGURA_CAIXA: i32 = 112;
const ALTURA_CAIXA: i32 = 148;
const PRATELEIRA_H: i32 = 16;
const MARGEM: i32 = 34;

// A caixa 3D do DKC: frente 102×130 com profundidade (12, -9) — textura
// 116×142, transparente fora da silhueta.
const DKC_W: i32 = 116;
const DKC_H: i32 = 142;
const DKC_FRENTE_W: i32 = 102;
const DKC_FRENTE_H: i32 = 130;
const DKC_FRENTE_X: i32 = 0;
const DKC_FRENTE_Y: i32 = 10;
const PROF_X: i32 = 12;
const PROF_Y: i32 = -9;
const DKC_ID: u64 = 0xD00C1;

struct Caixa {
    id: u64,
    titulo: &'static str,
    cor: (u8, u8, u8),
    /// A caixa 3D com transparência (o DKC) — o pintor não desenha sombra
    /// nela: a sombra vai assada na textura.
    terceira_dim: bool,
}

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from("/tmp/estante-mock");
    std::fs::create_dir_all(&out)?;

    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("SixteeN", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("SixteeN v1.2.0\nsnes9x 1.63");

    // O DKC em 3D + as caixas homebrew da horta (o alfabeto do mini-font
    // dos mocks é A B C D E F H I K L M N O P Q R S T U V W X Y 0-3 8).
    let caixas = [
        Caixa {
            id: DKC_ID,
            titulo: "DONKEY KONG",
            cor: (0, 0, 0),
            terceira_dim: true,
        },
        Caixa {
            id: 0xC0FF01,
            titulo: "MUNDO DO TOMATE",
            cor: (200, 40, 40),
            terceira_dim: false,
        },
        Caixa {
            id: 0xC0FF02,
            titulo: "CENOURA CRAVE",
            cor: (230, 140, 30),
            terceira_dim: false,
        },
        Caixa {
            id: 0xC0FF03,
            titulo: "SUPER ALFACE",
            cor: (60, 160, 70),
            terceira_dim: false,
        },
        Caixa {
            id: 0xC0FF04,
            titulo: "MILHO MALUCO",
            cor: (235, 200, 50),
            terceira_dim: false,
        },
        Caixa {
            id: 0xC0FF05,
            titulo: "TOMATE VERDE",
            cor: (90, 170, 60),
            terceira_dim: false,
        },
        Caixa {
            id: 0xC0FF06,
            titulo: "SALADA MORTA",
            cor: (150, 60, 160),
            terceira_dim: false,
        },
        Caixa {
            id: 0xC0FF07,
            titulo: "ABOBRINHA",
            cor: (120, 190, 80),
            terceira_dim: false,
        },
        Caixa {
            id: 0xC0FF08,
            titulo: "PIMENTA MAGNA",
            cor: (220, 70, 90),
            terceira_dim: false,
        },
    ];
    cab.set_image(DKC_ID, DKC_W as u32, DKC_H as u32, &dkc_cover());
    for c in &caixas[1..] {
        cab.set_image(
            c.id,
            LARGURA_CAIXA as u32,
            ALTURA_CAIXA as u32,
            &caixa_image(c),
        );
    }

    // O painel lateral com as infos do jogo puxado (o DKC) — os tempos do
    // HowLongToBeat são os reais da tabela embutida (CRC32 762AF827).
    let hltb_icon: u64 = 0x484C_5442;
    if let Ok(icon) = image::load_from_memory(sixteen_app::HLTB_ICON_PNG) {
        let icon = icon.to_rgba8();
        cab.set_image(hltb_icon, icon.width(), icon.height(), icon.as_raw());
    }
    let hltb = Some(sixteen_platform::HltbBars {
        icon: hltb_icon,
        rows: [
            ("história principal", 232u64, (52u8, 124, 246)),
            ("principal + extras", 300, (86, 88, 204)),
            ("completista", 363, (198, 60, 72)),
            ("todos os estilos", 257, (44, 162, 156)),
        ]
        .into_iter()
        .map(|(label, minutos, color)| sixteen_platform::HltbBar {
            label: label.to_string(),
            text: if minutos >= 60 {
                format!("{}h {}min", minutos / 60, minutos % 60)
            } else {
                format!("{minutos}min")
            },
            frac: minutos as f32 / 363.0,
            color,
        })
        .collect(),
    });
    cab.set_shelf_panel(ShelfPanelInfo {
        title: "Donkey Kong Country".to_string(),
        logo_img: Some(DKC_ID),
        cartridge_img: None,
        backcover_img: None,
        release: Some("1994".to_string()),
        info: vec![
            ("editora".to_string(), "Nintendo".to_string()),
            ("conquistas".to_string(), "28 de 40 (70%)".to_string()),
        ],
        section: sixteen_platform::PanelSection::Informacoes,
        scroll: 0,
        favorite: Some(true),
        achievements: true,
        achievements_reason: None,
        manual_first_page: None,
        award_img: None,

        igdb: None,
        hltb,
    });

    cab.capture_shelf(
        (16, 14, 13),
        |d| {
            let (sw, sh) = d.size();
            let (sw, sh) = (sw as i32, sh as i32);

            // Parede: gradiente vertical quente, escuro em cima.
            for y in 0..sh {
                let t = y as f32 / sh as f32;
                d.fill(
                    0,
                    y,
                    sw as u32,
                    1,
                    (
                        (24.0 + 6.0 * t) as u8,
                        (22.0 + 5.0 * t) as u8,
                        (20.0 + 5.0 * t) as u8,
                        255,
                    ),
                );
            }

            // Cabeçalho: contador e busca.
            let dim = (150, 150, 158);
            d.text(MARGEM, 16, 2, (200, 196, 190), "58 games");
            let (bx, bw, bh) = (sw - MARGEM - 280, 280, 32);
            d.outline(bx, 16, bw, bh, 1, (96, 92, 86, 255));
            d.fill(bx + 1, 17, bw - 2, bh - 2, (30, 27, 24, 255));
            d.text(bx + 10, 22, 1, dim, "buscar...");

            // Três prateleiras: recentes, todos, favoritos.
            let rotulos = ["jogados recentemente", "todos os jogos", "favoritos"];
            let fileiras: [&[usize]; 3] = [&[0, 1, 2, 3], &[4, 5, 6, 7, 8], &[0, 6]];
            let topo = 64;
            let passo = (sh - topo - 8) / 3;
            let foco = (0usize, 0usize); // fileira 0, posição 0 — o DKC puxado

            for k in 0..3usize {
                let prateleira_y = topo + passo * (k as i32 + 1) - PRATELEIRA_H;
                let caixa_y = prateleira_y - ALTURA_CAIXA;

                // Rótulo da fileira, na parede, acima das caixas.
                d.text(MARGEM, caixa_y - 26, 1, dim, rotulos[k]);

                // Brilho quente da lâmpada escorrendo na parede, logo
                // acima do tampo.
                for (i, alpha) in [0.10, 0.05].into_iter().enumerate() {
                    let c = mistura((26, 24, 22), (255, 180, 90), alpha);
                    d.fill(
                        MARGEM,
                        prateleira_y - 2 - i as i32,
                        (sw - 2 * MARGEM) as u32,
                        1,
                        (c.0, c.1, c.2, 255),
                    );
                }

                // A madeira: tampo com luz no topo, veio no rosto, boca
                // escura embaixo e suportes nas pontas.
                d.fill(
                    MARGEM,
                    prateleira_y,
                    (sw - 2 * MARGEM) as u32,
                    PRATELEIRA_H as u32,
                    (106, 71, 44, 255),
                );
                d.fill(
                    MARGEM,
                    prateleira_y,
                    (sw - 2 * MARGEM) as u32,
                    2,
                    (150, 110, 68, 255),
                );
                d.fill(
                    MARGEM,
                    prateleira_y + PRATELEIRA_H - 4,
                    (sw - 2 * MARGEM) as u32,
                    4,
                    (62, 42, 26, 255),
                );
                for (dy, c) in [(5, (92u8, 61, 38)), (9, (96, 64, 40)), (12, (90, 60, 37))] {
                    d.fill(
                        MARGEM + 6,
                        prateleira_y + dy,
                        (sw - 2 * MARGEM - 12) as u32,
                        1,
                        (c.0, c.1, c.2, 255),
                    );
                }
                // Juntas das tábuas.
                for jx in [sw / 3, 2 * sw / 3] {
                    d.fill(
                        jx,
                        prateleira_y + 2,
                        1,
                        (PRATELEIRA_H - 4) as u32,
                        (66, 43, 27, 255),
                    );
                }
                // Sombra da prateleira na parede, embaixo.
                for (i, (_, c)) in [(0, (13u8, 11, 10)), (1, (16, 14, 12)), (2, (19, 17, 15))]
                    .into_iter()
                    .enumerate()
                {
                    d.fill(
                        MARGEM + 4,
                        prateleira_y + PRATELEIRA_H + i as i32,
                        (sw - 2 * MARGEM - 8) as u32,
                        1,
                        (c.0, c.1, c.2, 255),
                    );
                }
                // Suportes.
                for sx in [MARGEM + 6, sw - MARGEM - 22] {
                    d.fill(sx, prateleira_y + PRATELEIRA_H, 16, 8, (48, 32, 20, 255));
                }

                // As caixas em pé; a 3D (DKC) pousa com a sombra assada na
                // própria textura — a frente termina 2px acima do fim da
                // textura, então o "pouso" é 2px mais fundo.
                let fileira = fileiras[k];
                let passo_x = LARGURA_CAIXA + 26;
                for (pos, &idx) in fileira.iter().enumerate() {
                    let c = &caixas[idx];
                    let puxado = (k, pos) == foco;
                    let (dx, dy) = if puxado { (-10, 18) } else { (0, 0) };
                    let x = MARGEM + 10 + pos as i32 * passo_x + dx;

                    if c.terceira_dim {
                        let y = prateleira_y - (DKC_FRENTE_Y + DKC_FRENTE_H) - 2 + dy;
                        d.image_fit(c.id, x, y, DKC_W as u32, DKC_H as u32);
                        if puxado {
                            d.outline(
                                x - 3,
                                y - 3,
                                (DKC_W + 6) as u32,
                                (DKC_H + 6) as u32,
                                2,
                                (245, 245, 250, 255),
                            );
                        }
                        continue;
                    }

                    let y = caixa_y + dy;
                    // Sombra na parede (lado direito) e na madeira (base).
                    let sombra = if puxado { (10, 8, 8) } else { (14, 12, 11) };
                    d.fill(
                        x + LARGURA_CAIXA,
                        y + 6,
                        6,
                        (ALTURA_CAIXA - 6) as u32,
                        (sombra.0, sombra.1, sombra.2, 255),
                    );
                    let base = if puxado {
                        y + ALTURA_CAIXA
                    } else {
                        prateleira_y
                    };
                    d.fill(
                        x + 4,
                        base,
                        (LARGURA_CAIXA + 12) as u32,
                        if puxado { 7 } else { 5 },
                        (26, 17, 11, 255),
                    );

                    d.image_fit(c.id, x, y, LARGURA_CAIXA as u32, ALTURA_CAIXA as u32);

                    // A face lateral direita da caixa (3D barato): faixa
                    // escura da própria cor.
                    let esc = escurece(c.cor, 0.45);
                    d.fill(
                        x + LARGURA_CAIXA - 7,
                        y + 2,
                        5,
                        (ALTURA_CAIXA - 4) as u32,
                        (esc.0, esc.1, esc.2, 255),
                    );
                    // Fio de luz no canto esquerdo.
                    let luz = mistura(c.cor, (255, 255, 255), 0.35);
                    d.fill(
                        x,
                        y + 2,
                        1,
                        (ALTURA_CAIXA - 4) as u32,
                        (luz.0, luz.1, luz.2, 255),
                    );

                    if puxado {
                        d.outline(
                            x - 3,
                            y - 3,
                            (LARGURA_CAIXA + 6) as u32,
                            (ALTURA_CAIXA + 6) as u32,
                            2,
                            (245, 245, 250, 255),
                        );
                    }
                }

                // Favoritos: estrela na primeira caixa da fileira.
                if k == 2 {
                    let x = MARGEM + 10 + LARGURA_CAIXA;
                    d.star(x - 8, caixa_y + 10, 7, (240, 200, 60));
                }
            }

            // Uma pilha de caixas deitadas no fim da última fileira.
            let pilha_x = MARGEM + 10 + fileiras[2].len() as i32 * (LARGURA_CAIXA + 26) + 40;
            let pilha_y = topo + passo * 3 - PRATELEIRA_H;
            for (i, cor) in [(150u8, 60, 160), (70, 110, 200)].into_iter().enumerate() {
                let y = pilha_y - 22 - i as i32 * 24;
                let viva = (cor.0, cor.1, cor.2, 255);
                d.fill(
                    pilha_x,
                    y,
                    (LARGURA_CAIXA + 30) as u32,
                    22,
                    (cor.0 / 5, cor.1 / 5, cor.2 / 5, 255),
                );
                d.outline(pilha_x, y, (LARGURA_CAIXA + 30) as u32, 22, 1, viva);
                let borda = escurece(cor, 0.4);
                d.fill(
                    pilha_x + 3,
                    y + 19,
                    (LARGURA_CAIXA + 24) as u32,
                    2,
                    (borda.0, borda.1, borda.2, 255),
                );
            }
        },
        &out.join("estante-snes.bmp"),
    )
    .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{}", out.join("estante.bmp").display());
    Ok(())
}

/// A caixa 3D do Donkey Kong Country — textura RGBA com o fundo
/// transparente: frente com a arte (selva, silhueta do macaco, título),
/// lateral e topo em perspectiva, sombra de contato assada embaixo.
fn dkc_cover() -> Vec<u8> {
    let mut px = vec![0u8; (DKC_W * DKC_H * 4) as usize];
    let w = DKC_W as u32;
    let h = DKC_H as u32;
    let fx = DKC_FRENTE_X;
    let fy = DKC_FRENTE_Y;
    let fw = DKC_FRENTE_W;
    let fh = DKC_FRENTE_H;

    // --- Topo (paralelogramo, a madeira da prateleira aparece nele) ---
    for y in (fy + PROF_Y)..=fy {
        let t = (y - (fy + PROF_Y)) as f32 / (-PROF_Y) as f32;
        let x0 = PROF_X as f32 * (1.0 - t);
        let x1 = (fx + fw) as f32 + PROF_X as f32 * (1.0 - t);
        fill_rgba(
            &mut px,
            w,
            h,
            x0 as i32,
            y,
            (x1 - x0) as i32,
            1,
            (54, 74, 26, 255),
        );
    }
    // --- Lateral direita (paralelogramo escuro, lombada) ---
    for c in fx..(fx + fw + PROF_X) {
        let t = ((c - (fx + fw)).max(0)) as f32 / PROF_X as f32;
        let y_top = fy as f32 + PROF_Y as f32 * t;
        let y_bot = (fy + fh) as f32 + PROF_Y as f32 * t;
        let lomb = if c >= fx + fw {
            (12, 18, 10, 255)
        } else {
            continue;
        };
        fill_rgba(
            &mut px,
            w,
            h,
            c,
            y_top as i32,
            1,
            (y_bot - y_top) as i32,
            lomb,
        );
        // filete dourado da lombada, no meio da lateral
        if c == fx + fw + 4 {
            fill_rgba(
                &mut px,
                w,
                h,
                c,
                y_top as i32 + 4,
                1,
                (y_bot - y_top) as i32 - 8,
                (238, 196, 42, 255),
            );
        }
    }

    // --- A frente: selva em gradiente ---
    for y in fy..(fy + fh) {
        let t = (y - fy) as f32 / fh as f32;
        let c = mistura((14, 52, 22), (26, 96, 40), t);
        fill_rgba(&mut px, w, h, fx, y, fw, 1, (c.0, c.1, c.2, 255));
    }
    // Copa escura no canto superior.
    disco(&mut px, w, h, fx + 22, fy + 10, 16, (10, 38, 16, 255));
    disco(&mut px, w, h, fx + 84, fy + 6, 12, (10, 38, 16, 255));

    // O macaco: silhueta marrom com focinho claro e gravata vermelha.
    disco(&mut px, w, h, fx + 51, fy + 72, 26, (92, 58, 34, 255));
    disco(&mut px, w, h, fx + 25, fy + 58, 7, (92, 58, 34, 255));
    disco(&mut px, w, h, fx + 77, fy + 58, 7, (92, 58, 34, 255));
    disco(&mut px, w, h, fx + 51, fy + 86, 13, (206, 160, 110, 255));
    disco(&mut px, w, h, fx + 43, fy + 66, 4, (240, 236, 220, 255));
    disco(&mut px, w, h, fx + 59, fy + 66, 4, (240, 236, 220, 255));
    disco(&mut px, w, h, fx + 43, fy + 67, 2, (30, 20, 14, 255));
    disco(&mut px, w, h, fx + 59, fy + 67, 2, (30, 20, 14, 255));
    disco(&mut px, w, h, fx + 46, fy + 84, 2, (58, 36, 22, 255));
    disco(&mut px, w, h, fx + 56, fy + 84, 2, (58, 36, 22, 255));
    fill_rgba(&mut px, w, h, fx + 44, fy + 96, 14, 20, (196, 44, 44, 255));
    disco(&mut px, w, h, fx + 51, fy + 118, 9, (196, 44, 44, 255));

    // --- Moldura dourada da caixa (por cima da arte) ---
    let ouro = (238, 196, 42, 255);
    for t in 0..3 {
        fill_rgba(&mut px, w, h, fx + t, fy + t, fw - t * 2, 1, ouro);
        fill_rgba(&mut px, w, h, fx + t, fy + fh - 1 - t, fw - t * 2, 1, ouro);
        fill_rgba(&mut px, w, h, fx + t, fy + t, 1, fh - t * 2, ouro);
        fill_rgba(&mut px, w, h, fx + fw - 1 - t, fy + t, 1, fh - t * 2, ouro);
    }

    // --- Título ---
    let amarelo = (252, 212, 40, 255);
    let x = (fw - "DONKEY".len() as i32 * 12) / 2 + fx;
    draw_text_rgba(&mut px, w, h, x, fy + 14, 2, "DONKEY", amarelo);
    let x = (fw - "KONG".len() as i32 * 12) / 2 + fx;
    draw_text_rgba(&mut px, w, h, x, fy + 30, 2, "KONG", amarelo);
    let x = (fw - "COUNTRY".len() as i32 * 6) / 2 + fx;
    draw_text_rgba(&mut px, w, h, x, fy + 50, 1, "COUNTRY", (230, 180, 60, 255));

    // Rodapé do selo.
    let x = (fw - "NINTENDO".len() as i32 * 6) / 2 + fx;
    draw_text_rgba(
        &mut px,
        w,
        h,
        x,
        fy + fh - 16,
        1,
        "NINTENDO",
        (230, 230, 230, 255),
    );

    // --- Sombra de contato assada: a caixa pousa na madeira ---
    for (i, (x0, alfa)) in [(4, 70u8), (1, 40)].into_iter().enumerate() {
        fill_rgba(
            &mut px,
            w,
            h,
            x0,
            DKC_H - 4 + i as i32,
            DKC_W - x0 - 2,
            1,
            (8, 6, 5, alfa),
        );
    }
    px
}

/// A arte da caixa homebrew (RGBA no tamanho exato da caixa): fundo escuro
/// da cor, moldura viva, lombada à esquerda, céu com sol e chão, o nome em
/// duas linhas e a tag da editora embaixo.
fn caixa_image(c: &Caixa) -> Vec<u8> {
    let w = LARGURA_CAIXA;
    let h = ALTURA_CAIXA;
    let mut px = vec![0u8; (w * h * 4) as usize];
    let (r, g, b) = c.cor;
    let fundo = (r / 5, g / 5, b / 5, 255);
    let viva = (r, g, b, 255);
    fill_rgba(&mut px, w as u32, h as u32, 0, 0, w, h, fundo);

    // Céu mais claro no terço de cima, sol, e chão faixado embaixo.
    let ceu = mistura((r / 5, g / 5, b / 5), c.cor, 0.18);
    fill_rgba(
        &mut px,
        w as u32,
        h as u32,
        3,
        3,
        w - 6,
        h / 3,
        (ceu.0, ceu.1, ceu.2, 255),
    );
    disco(&mut px, w as u32, h as u32, w - 34, 24, 10, viva);
    disco(
        &mut px,
        w as u32,
        h as u32,
        w - 34,
        24,
        6,
        (255, 244, 200, 255),
    );
    let chao = escurece(c.cor, 0.3);
    fill_rgba(
        &mut px,
        w as u32,
        h as u32,
        3,
        h - 46,
        w - 6,
        16,
        (chao.0, chao.1, chao.2, 255),
    );

    // Moldura.
    for t in 0..3 {
        fill_rgba(&mut px, w as u32, h as u32, t, t, w - t * 2, 1, viva);
        fill_rgba(
            &mut px,
            w as u32,
            h as u32,
            t,
            h - 1 - t,
            w - t * 2,
            1,
            viva,
        );
        fill_rgba(&mut px, w as u32, h as u32, t, t, 1, h - t * 2, viva);
        fill_rgba(
            &mut px,
            w as u32,
            h as u32,
            w - 1 - t,
            t,
            1,
            h - t * 2,
            viva,
        );
    }
    // Lombada: faixa escura à esquerda com duas marcas da cor.
    let lomb = escurece(c.cor, 0.35);
    fill_rgba(
        &mut px,
        w as u32,
        h as u32,
        3,
        3,
        10,
        h - 6,
        (lomb.0, lomb.1, lomb.2, 255),
    );
    fill_rgba(&mut px, w as u32, h as u32, 6, 14, 4, 18, viva);
    fill_rgba(&mut px, w as u32, h as u32, 6, 40, 4, 8, viva);

    // O nome, em duas linhas centradas (a primeira palavra grande; a sobra
    // desce para a escala pequena quando não cabe).
    let (a, bb) = c.titulo.split_once(' ').unwrap_or((c.titulo, ""));
    let cabe = |texto: &str, s: i32| texto.len() as i32 * 6 * s <= w - 24;
    let s1 = [2, 1].into_iter().find(|&s| cabe(a, s)).unwrap_or(1);
    let x1 = (w - a.len() as i32 * 6 * s1) / 2 + 3;
    draw_text_rgba(&mut px, w as u32, h as u32, x1, h / 2 - 16, s1, a, viva);
    let s2 = [2, 1].into_iter().find(|&s| cabe(bb, s)).unwrap_or(1);
    let x2 = (w - bb.len() as i32 * 6 * s2) / 2 + 3;
    draw_text_rgba(
        &mut px,
        w as u32,
        h as u32,
        x2,
        h / 2 + 6,
        s2,
        bb,
        (235, 235, 235, 255),
    );
    // Tag da editora.
    draw_text_rgba(
        &mut px,
        w as u32,
        h as u32,
        w / 2 - 30,
        h - 26,
        1,
        "HORTA GAMES",
        (200, 200, 200, 255),
    );
    px
}

// ---------------------------------------------------------------- helpers

fn mistura(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (l(a.0, b.0), l(a.1, b.1), l(a.2, b.2))
}

fn escurece(c: (u8, u8, u8), f: f32) -> (u8, u8, u8) {
    (
        (c.0 as f32 * f) as u8,
        (c.1 as f32 * f) as u8,
        (c.2 as f32 * f) as u8,
    )
}

fn fill_rgba(px: &mut [u8], w: u32, h: u32, x: i32, y: i32, ww: i32, hh: i32, c: (u8, u8, u8, u8)) {
    for yy in y..y + hh {
        for xx in x..x + ww {
            put_rgba(px, w, h, xx, yy, c);
        }
    }
}

fn disco(px: &mut [u8], w: u32, h: u32, cx: i32, cy: i32, r: i32, c: (u8, u8, u8, u8)) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r {
                put_rgba(px, w, h, cx + dx, cy + dy, c);
            }
        }
    }
}

fn put_rgba(px: &mut [u8], w: u32, h: u32, x: i32, y: i32, c: (u8, u8, u8, u8)) {
    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
        return;
    }
    let i = ((y as u32 * w + x as u32) * 4) as usize;
    if i + 3 < px.len() {
        px[i] = c.0;
        px[i + 1] = c.1;
        px[i + 2] = c.2;
        px[i + 3] = c.3;
    }
}

fn draw_text_rgba(
    px: &mut [u8],
    w: u32,
    h: u32,
    x0: i32,
    y0: i32,
    s: i32,
    text: &str,
    c: (u8, u8, u8, u8),
) {
    let mut pen = x0;
    for ch in text.chars() {
        if let Some(g) = glyph(ch) {
            for (gy, row) in g.iter().enumerate() {
                for (gx, cell) in row.chars().enumerate() {
                    if cell != '#' {
                        continue;
                    }
                    for yy in 0..s {
                        for xx in 0..s {
                            put_rgba(
                                px,
                                w,
                                h,
                                pen + gx as i32 * s + xx,
                                y0 + gy as i32 * s + yy,
                                c,
                            );
                        }
                    }
                }
            }
        }
        pen += 6 * s;
    }
}

fn glyph(ch: char) -> Option<[&'static str; 5]> {
    Some(match ch {
        'A' => [".###.", "#...#", "#####", "#...#", "#...#"],
        'B' => ["####.", "#...#", "####.", "#...#", "####."],
        'C' => [".###.", "#....", "#....", "#....", ".###."],
        'D' => ["####.", "#...#", "#...#", "#...#", "####."],
        'E' => ["#####", "#....", "###..", "#....", "#####"],
        'F' => ["#####", "#....", "####.", "#....", "#...."],
        'G' => [".###.", "#....", "#.##.", "#...#", ".###."],
        'H' => ["#...#", "#...#", "#####", "#...#", "#...#"],
        'I' => ["#####", "..#..", "..#..", "..#..", "#####"],
        'K' => ["#...#", "#..#.", "###..", "#..#.", "#...#"],
        'L' => ["#....", "#....", "#....", "#....", "#####"],
        'M' => ["#...#", "##.##", "#.#.#", "#...#", "#...#"],
        'N' => ["#...#", "##..#", "#.#.#", "#..##", "#...#"],
        'O' => [".###.", "#...#", "#...#", "#...#", ".###."],
        'P' => ["####.", "#...#", "####.", "#....", "#...."],
        'Q' => [".###.", "#...#", "#.#.#", "#..#.", ".##.#"],
        'R' => ["####.", "#...#", "####.", "#..#.", "#...#"],
        'S' => [".####", "#....", ".###.", "....#", "####."],
        'T' => ["#####", "..#..", "..#..", "..#..", "..#.."],
        'U' => ["#...#", "#...#", "#...#", "#...#", ".###."],
        'V' => ["#...#", "#...#", "#...#", "#.#.#", ".#.#."],
        'W' => ["#...#", "#...#", "#.#.#", "##.##", "#...#"],
        'X' => ["#...#", ".#.#.", "..#..", ".#.#.", "#...#"],
        'Y' => ["#...#", ".#.#.", "..#..", "..#..", "..#.."],
        '0' => [".###.", "#..##", "#.#.#", "##..#", ".###."],
        '1' => ["..#..", ".##..", "..#..", "..#..", "#####"],
        '2' => [".###.", "#...#", "..##.", ".#...", "#####"],
        '3' => ["####.", "....#", ".###.", "....#", "####."],
        '-' => [".....", ".....", "#####", ".....", "....."],
        ' ' => [".....", ".....", ".....", ".....", "....."],
        _ => return None,
    })
}
