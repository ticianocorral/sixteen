//! MOCK — proposta de revisão visual da TV e dos botões, desenhada dentro
//! do tubo de verdade (warp + vignette, via `capture_shelf`): três
//! alternativas de botão lado a lado (o `draw_button` atual, a tecla
//! chiclete cinza do `draw_flat_key`, e o menu de fósforo estilo OSD) e,
//! por cima de tudo, as melhorias de TV — scanlines, cantos arredondados e
//! o reflexo do vidro. 100% sintético; nada do app muda.
//!
//! Run: cargo run -p sixteen-app --example tv_botoes_mock — escreve
//! /tmp/tv-mock/tv-botoes.bmp (converter com sips para png).

use std::path::PathBuf;

use sixteen_platform::{Platform, Screen};

const BG: (u8, u8, u8) = (16, 14, 13);
const PAINEL_BG: (u8, u8, u8) = (34, 32, 29);
const TEXTO: (u8, u8, u8) = (225, 220, 210);
const DIM: (u8, u8, u8) = (140, 134, 124);
/// O violeta da chave Power/Reset do console — o candidato a linguagem do menu de fósforo.
const FOSFORO: (u8, u8, u8) = (186, 138, 244);

/// Paleta da tecla chiclete cinza (a alternativa A, irmã do EJECT_LIT do
/// console) — assento escuro, corpo cinza-médio, label escuro.
const CHICLETE_ASSENTO: (u8, u8, u8) = (18, 17, 16);
const CHICLETE_CORPO: (u8, u8, u8) = (120, 115, 110);
const CHICLETE_CORPO_LIT: (u8, u8, u8) = (150, 144, 138);
const CHICLETE_LABEL: (u8, u8, u8) = (28, 26, 24);

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from("/tmp/tv-mock");
    std::fs::create_dir_all(&out)?;

    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("SixteeN mock", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("SixteeN v1.2.0\nsnes9x 1.63");

    cab.capture_shelf(BG, |d| comparativo(d), &out.join("tv-botoes.bmp"))
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    println!("{}", out.join("tv-botoes.bmp").display());
    Ok(())
}

/// A tela de comparativo: três linhas (uma por alternativa de botão, com o
/// mesmo rótulo "histórico" nas três) + a fileira de abas em fósforo, e as
/// melhorias de tubo por cima de tudo no fim.
fn comparativo(d: &mut Screen) {
    let (sw, sh) = d.size();
    let (sw, sh) = (sw as i32, sh as i32);
    let m = 40i32;

    d.text(m, m, 2, TEXTO, "proposta: botoes + tv");
    d.text(m, m + 52, 1, DIM, "mesma acao, tres linguagens");

    // --- Linha 1: o estilo atual (draw_button) -------------------------
    let y1 = m + 96;
    d.text(m, y1, 1, DIM, "A. atual");
    botao_atual(d, m + 130, y1 - 4, 150, 32, "historico");
    botao_atual(d, m + 292, y1 - 4, 150, 32, "atualizar");
    botao_atual(d, m + 454, y1 - 4, 150, 32, "filtrar");

    // --- Linha 2: a tecla chiclete cinza -------------------------------
    let y2 = y1 + 72;
    d.text(m, y2, 1, DIM, "B. chiclete");
    botao_chiclete(d, m + 130, y2 - 8, 150, 36, "historico", true);
    botao_chiclete(d, m + 292, y2 - 8, 150, 36, "atualizar", true);
    // Um inert, para ver o dim da tecla.
    botao_chiclete(d, m + 454, y2 - 8, 150, 36, "filtrar", false);

    // --- Linha 3: o menu de fósforo (sem caixa) ------------------------
    let y3 = y2 + 72;
    d.text(m, y3, 1, DIM, "C. fosforo");
    fosforo_item(d, m + 130, y3 - 8, "historico", true);
    fosforo_item(d, m + 262, y3 - 8, "atualizar", false);
    fosforo_item(d, m + 394, y3 - 8, "filtrar", false);

    // --- As abas do painel da estante em fósforo -----------------------
    let y4 = y3 + 78;
    d.text(m, y4, 1, DIM, "abas (informacoes/capa/manual/cartucho)");
    let abas = ["informacoes", "capa traseira", "manual", "cartucho"];
    let mut x = m;
    for (i, a) in abas.iter().enumerate() {
        let ativa = i == 0;
        let cor = if ativa { FOSFORO } else { DIM };
        d.text(x, y4 + 26, 1, cor, a);
        if ativa {
            // sublinhado no item ativo, 2px, a largura do texto
            let w = a.chars().count() as i32 * 9;
            d.fill(
                x,
                y4 + 26 + 21,
                w as u32,
                2,
                (FOSFORO.0, FOSFORO.1, FOSFORO.2, 255),
            );
        }
        x += a.chars().count() as i32 * 9 + 36;
    }

    // --- Melhorias da TV, por cima de tudo ------------------------------
    // 1. Scanlines sutis: uma linha escura a cada 3px, alpha ~28 (11%).
    let mut y = 0;
    while y < sh {
        d.fill(0, y + 2, sw as u32, 1, (0, 0, 0, 28));
        y += 3;
    }
    // 2. Cantos arredondados: o canto do tubo "come" a imagem com o BG.
    cantos(d, sw, sh, 26);
    // 3. Reflexo do vidro: faixa diagonal clara, canto superior esquerdo.
    reflexo(d, sw, sh);
    // 4. Filete de bloom na borda interna do vidro (luz vazando).
    d.outline(
        1,
        1,
        (sw - 2) as u32,
        (sh - 2) as u32,
        1,
        (255, 255, 255, 10),
    );
}

/// O estilo atual (`draw_button` do cabinet): caixa com borda clara quando
/// lit, texto centrado.
fn botao_atual(d: &mut Screen, x: i32, y: i32, w: u32, h: u32, label: &str) {
    d.outline(x, y, w, h, 1, (TEXTO.0, TEXTO.1, TEXTO.2, 255));
    d.fill(
        x + 1,
        y + 1,
        w - 2,
        h - 2,
        (PAINEL_BG.0, PAINEL_BG.1, PAINEL_BG.2, 255),
    );
    let tw = label.chars().count() as i32 * 9;
    d.text(
        x + (w as i32 - tw) / 2,
        y + (h as i32 - 20) / 2,
        1,
        TEXTO,
        label,
    );
}

/// A alternativa B: a tecla chiclete do `draw_flat_key` em cinza — assento
/// escuro, corpo com cantos suaves (scanline por scanline, como o resto da
/// UI), label escuro. Inert: corpo uma passo mais escuro, label dim.
fn botao_chiclete(d: &mut Screen, x: i32, y: i32, w: u32, h: u32, label: &str, lit: bool) {
    let (w, h) = (w as i32, h as i32);
    let raio = (h / 5).max(2);
    let corpo = if lit {
        CHICLETE_CORPO_LIT
    } else {
        CHICLETE_CORPO
    };
    let linha = |d: &mut Screen, yy: i32, inset: i32, ww: i32, c: (u8, u8, u8)| {
        d.fill(
            x + inset,
            yy,
            (ww - inset * 2).max(0) as u32,
            1,
            (c.0, c.1, c.2, 255),
        );
    };
    // o assento (anel externo)
    for r in 0..h {
        let inset = corner_inset(r, h, raio);
        linha(d, y + r, inset, w, CHICLETE_ASSENTO);
    }
    // o corpo, 1px pra dentro
    for r in 0..h - 2 {
        let inset = corner_inset(r, h - 2, (raio - 1).max(1)).min((w - 2) / 2);
        linha(d, y + 1 + r, 1 + inset, w - 2, corpo);
    }
    let cor_label = if lit { CHICLETE_LABEL } else { DIM };
    let tw = label.chars().count() as i32 * 9;
    d.text(x + (w - tw) / 2, y + (h - 20) / 2, 1, cor_label, label);
}

/// A alternativa C: item de menu de fósforo — sem caixa, texto verde no
/// ativo (cursor ">" na frente + underline embaixo) e dim nos demais.
fn fosforo_item(d: &mut Screen, x: i32, y: i32, label: &str, ativo: bool) {
    let cor = if ativo { FOSFORO } else { DIM };
    if ativo {
        d.text(x, y, 1, FOSFORO, ">");
    }
    d.text(x + 18, y, 1, cor, label);
    if ativo {
        let w = label.chars().count() as i32 * 9;
        d.fill(
            x + 18,
            y + 21,
            w as u32,
            2,
            (FOSFORO.0, FOSFORO.1, FOSFORO.2, 255),
        );
    }
}

/// O inset horizontal da scanline `row` numa barra `h` alta com raio `r` —
/// o mesmo truque de quarto-de-círculo do `corner_inset` do cabinet.
fn corner_inset(row: i32, h: i32, radius: i32) -> i32 {
    let r = radius.min(h / 2).max(0);
    let from_end = row.min(h - 1 - row);
    if from_end >= r {
        return 0;
    }
    let dx = r - from_end;
    r - ((r * r - dx * dx) as f32).sqrt().floor() as i32
}

/// Cantos arredondados do tubo: preenche cada canto com o BG da janela em
/// passos de quarto-de-círculo, "comendo" o retângulo.
fn cantos(d: &mut Screen, sw: i32, sh: i32, r: i32) {
    let canto = |d: &mut Screen, cx: i32, cy: i32, sx: i32, sy: i32| {
        for row in 0..r {
            // quantos px este row precisa cobrir a partir da borda
            let dx = r - row - 1;
            let w = r - ((r * r - dx * dx) as f32).sqrt().floor() as i32;
            for col in 0..w.max(0) {
                let px = cx + sx * col;
                let py = cy + sy * row;
                d.fill(px, py, 1, 1, (BG.0, BG.1, BG.2, 255));
            }
        }
    };
    canto(d, 0, 0, 1, 1); // sup. esq. cresce pra direita/baixo
    canto(d, sw - 1, 0, -1, 1);
    canto(d, 0, sh - 1, 1, -1);
    canto(d, sw - 1, sh - 1, -1, -1);
}

/// O reflexo do vidro: uma faixa diagonal clara atravessando o quadrante
/// superior esquerdo, alpha baixinho — luz de quarto batendo no CRT.
fn reflexo(d: &mut Screen, sw: i32, sh: i32) {
    let len = (sw / 3).min(sh / 2);
    let esp = 26i32;
    for i in 0..len {
        // faixa entre duas diagonais paralelas
        for e in 0..esp {
            let x = i;
            let y = sh / 4 + esp - i - e;
            if x >= 0 && x < sw && y >= 0 && y < sh {
                let a = match e {
                    0..=3 => 26,
                    4..=18 => 14,
                    _ => 8,
                };
                d.fill(x, y, 1, 1, (255, 255, 255, a));
            }
        }
    }
}
