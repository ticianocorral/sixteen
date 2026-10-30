//! MOCK — os controles da TV no queixo (plan revision): o botão de POWER
//! (push-knob redondo, o clássico dos CRT de tubo) e o par VOL- / VOL+ de
//! volume, e o OSD retro no CRT — as barrinhas que sobem/descem quando o
//! volume muda, some sozinho depois de uns segundos. Frames: TV desligada
//! (só o queixo vivo), ligada com OSD em 3 níveis (baixo/médio/máximo) e
//! o OSD expirando. 100% sintético; nada do app muda.
//!
//! Run: cargo run -p sixteen-app --example tv_power_vol_mock — escreve
//! /tmp/tv-mock/tvvol-1..6.bmp (converter com sips para png).

use std::path::PathBuf;

use sixteen_platform::{Platform, Screen};

const BG: (u8, u8, u8) = (16, 14, 13);
const DIM: (u8, u8, u8) = (140, 134, 124);
const TEXTO: (u8, u8, u8) = (225, 220, 210);

/// O violeta da chave Power/Reset do console — o `FOSFORO` do app.
const FOSFORO: (u8, u8, u8) = (186, 138, 244);
const CHAVE: (u8, u8, u8) = (107, 70, 168);
const LED_ON: (u8, u8, u8) = (214, 44, 40);
const LED_ON_HI: (u8, u8, u8) = (255, 150, 140);
const LED_OFF: (u8, u8, u8) = (56, 26, 26);
/// Plástico do queixo — o `CABINET` do app.
const PLASTICO: (u8, u8, u8) = (40, 37, 33);
const PLASTICO_ESC: (u8, u8, u8) = (28, 26, 24);

fn main() -> anyhow::Result<()> {
    let out = PathBuf::from("/tmp/tv-mock");
    std::fs::create_dir_all(&out)?;

    let plat = Platform::new().map_err(|e| anyhow::anyhow!(e.to_string()))?;
    let mut cab = plat
        .create_cabinet("SixteeN mock", 1280, 800, false)
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    cab.set_nameplate("SixteeN v1.2.0\nsnes9x 1.63");

    // (tv ligada?, volume 0..16, osd ainda visível?)
    let fases: [(bool, u8, bool); 6] = [
        (false, 8, false), // 1: desligada — só o queixo com os controles
        (true, 8, true),   // 2: ligada, OSD no meio (8/16)
        (true, 14, true),  // 3: subiu — OSD quase cheio
        (true, 16, true),  // 4: máximo — OSD cheio + "MAX"
        (true, 5, true),   // 5: desceu — OSD baixo
        (true, 5, false),  // 6: OSD expirou — só a neve
    ];
    for (n, (on, vol, osd)) in fases.into_iter().enumerate() {
        let path = out.join(format!("tvvol-{}.bmp", n + 1));
        cab.capture_shelf(BG, |d| frame(d, on, vol, osd), &path)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
    }

    println!("{}", out.display());
    Ok(())
}

fn frame(d: &mut Screen, on: bool, vol: u8, osd: bool) {
    let (sw, sh) = d.size();
    let (sw, sh) = (sw as i32, sh as i32);

    // ---- O TUBO ---------------------------------------------------------
    if on {
        // A neve do canal 3 (representação do mock — no app é a de verdade)
        let mut rng = 0x9E37_79B9u32;
        for y in 0..sh - 90 {
            for x in (0..sw).step_by(2) {
                rng ^= rng << 13;
                rng ^= rng >> 17;
                rng ^= rng << 5;
                let v = (20 + (rng % 110)) as u8;
                d.fill(x, y, 2, 1, (v, v, v, 255));
            }
        }
        // "CH 3" — o banner verde que já existe no app
        d.text(sw - 150, 24, 3, (60, 230, 70), "CH  3");

        // ---- O OSD DE VOLUME: as barrinhas retro ------------------------
        if osd {
            const SEG: i32 = 22; // largura de um segmento
            const TOTAL: u8 = 16;
            let blocos = 3 * (SEG + 6);
            let bx = (sw - blocos * (TOTAL as i32)) / 2 + 10;
            let by = sh - 90 - 120;

            // rótulo "VOLUME" à esquerda das barras, no fósforo
            d.text(bx - 90, by + 6, 1, FOSFORO, "VOLUME");
            let maximo = vol >= TOTAL;
            let _pct = format!("{}{}", if maximo { "MAX" } else { "" }, "");
            if maximo {
                d.text(bx + blocos * TOTAL as i32 + 16, by + 6, 1, FOSFORO, "MAX");
            }
            for i in 0..TOTAL {
                let aceso = i < vol;
                let x = bx + i as i32 * blocos;
                // o segmento: par de barras verticais (o jeito dos OSD de
                // TV antiga — não um retângulo cheio)
                let cor = if aceso { FOSFORO } else { PLASTICO_ESC };
                d.fill(x, by, (SEG / 2 - 2) as u32, 34, (cor.0, cor.1, cor.2, 255));
                d.fill(
                    x + SEG / 2 + 2,
                    by,
                    (SEG / 2 - 2) as u32,
                    26,
                    (cor.0, cor.1, cor.2, 255),
                );
            }
            // o filete de brilho sob as barras acesas — fósforo pingando
            if vol > 0 {
                let w = blocos * vol as i32 - 6;
                d.fill(
                    bx,
                    by + 40,
                    w.max(4) as u32,
                    1,
                    (FOSFORO.0, FOSFORO.1, FOSFORO.2, 160),
                );
            }
        }
    } else {
        // TV desligada: o vidro quase preto com reflexo fraco
        for y in 0..sh - 90 {
            let t = y as f32 / sh as f32;
            let v = 5.0 + 3.0 * t;
            d.fill(0, y, sw as u32, 1, (v as u8, v as u8, (v + 1.0) as u8, 255));
        }
    }

    // ---- O QUEIXO (faixa de plástico embaixo do vidro) ------------------
    let ch = 90;
    let cy = sh - ch;
    d.fill(
        0,
        cy,
        sw as u32,
        ch as u32,
        (PLASTICO.0, PLASTICO.1, PLASTICO.2, 255),
    );
    // a sombra do vidro sobre o plástico
    d.fill(
        0,
        cy,
        sw as u32,
        3,
        (PLASTICO_ESC.0, PLASTICO_ESC.1, PLASTICO_ESC.2, 255),
    );

    // nameplate à esquerda (como no app)
    d.text(34, cy + 18, 1, (92, 86, 78), "SixteeN v1.2.0");
    d.text(34, cy + 42, 1, (92, 86, 78), "snes9x 1.63");

    // ---- Os controles, à direita: VOL-  VOL+  [POWER] -------------------
    let mut x = sw - 34 - 46; // o power mora no canto

    // POWER: push-knob redondo — anel afundado + botão violeta + LED
    let pcx = x + 23;
    let pcy = cy + ch / 2 + 2;
    // o anel (assento afundado) — círculo escuro
    for dy in -19..=19 {
        let dx = ((19 * 19 - dy * dy) as f32).sqrt().round() as i32;
        d.fill(
            pcx - dx,
            pcy + dy,
            (dx * 2) as u32,
            1,
            (PLASTICO_ESC.0, PLASTICO_ESC.1, PLASTICO_ESC.2, 255),
        );
    }
    // sombra interna do anel (luz de cima)
    for dx in -14..=14 {
        let dy = ((14 * 14 - dx * dx) as f32).sqrt().round() as i32;
        d.fill(pcx + dx, pcy - dy, 1, 2, (18, 16, 15, 255));
    }
    // o botão em si: disco violeta com aro
    for dy in -14..=14 {
        let dx = ((14 * 14 - dy * dy) as f32).sqrt().round() as i32;
        d.fill(pcx - dx, pcy + dy, (dx * 2) as u32, 1, (70, 46, 110, 255));
    }
    for dy in -12..=12 {
        let dx = ((12 * 12 - dy * dy) as f32).sqrt().round() as i32;
        d.fill(
            pcx - dx,
            pcy + dy,
            (dx * 2) as u32,
            1,
            (CHAVE.0, CHAVE.1, CHAVE.2, 255),
        );
    }
    // highlight discreto no topo do botão (plástico fosco)
    for dx in -8..=8 {
        let dy = ((8 * 8 - dx * dx) as f32).sqrt().round() as i32;
        d.fill(pcx + dx, pcy - dy - 1, 1, 1, (140, 100, 200, 255));
    }
    // a linha indicadora do knob — apontando pra cima quando ligado
    if on {
        d.fill(pcx, pcy - 10, 2, 8, (TEXTO.0, TEXTO.1, TEXTO.2, 255));
    } else {
        d.fill(pcx - 1 + 8, pcy - 3, 2, 8, (60, 44, 90, 255));
    }
    d.text(pcx - 18, cy + ch - 16, 1, DIM, "POWER");

    // LED do power, à esquerda do knob
    let led_c = if on { LED_ON } else { LED_OFF };
    for dy in -5..=5 {
        let dx = ((5 * 5 - dy * dy) as f32).sqrt().round() as i32;
        d.fill(
            pcx - 40 - dx,
            pcy + dy,
            (dx * 2) as u32,
            1,
            (led_c.0, led_c.1, led_c.2, 255),
        );
    }
    if on {
        d.fill(
            pcx - 43,
            pcy - 3,
            2,
            2,
            (LED_ON_HI.0, LED_ON_HI.1, LED_ON_HI.2, 255),
        );
    }

    // VOL- e VOL+: botõezinhos retangulares (as teclas do console, em mini)
    x = pcx - 40 - 14;
    for (rotulo, dx_desl) in [("VOL+", 0i32), ("VOL-", 1)] {
        let bw = 64u32;
        let bx = x - (bw as i32 + 10) * dx_desl - bw as i32;
        let by = pcy - 16;
        // assento
        d.fill(
            bx,
            by,
            bw,
            32,
            (PLASTICO_ESC.0, PLASTICO_ESC.1, PLASTICO_ESC.2, 255),
        );
        // a tecla
        d.fill(bx + 1, by + 1, bw - 2, 30, (CHAVE.0, CHAVE.1, CHAVE.2, 255));
        // o rótulo dentro
        let tw = rotulo.chars().count() as i32 * 9;
        d.text(
            bx + (bw as i32 - tw) / 2,
            by + 8,
            1,
            (243, 240, 232),
            rotulo,
        );
    }
    // etiqueta do conjunto
    d.text(pcx - 40 - 14 - 74 - 74 - 26, cy + ch - 16, 1, DIM, "VOLUME");
}
