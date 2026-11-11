//! Controles desenhados: vidro da TV, botões do queixo, teclas, LEDs e
//! chaves rocker.

use super::*;

/// Um disco com alpha em coordenadas absolutas — o halo do afterglow do
/// tubo (usado pelo vidro da fase, estático e gameplay).
pub(super) fn disco_alpha(
    canvas: &mut WindowCanvas,
    cx: i32,
    cy: i32,
    r: i32,
    c: (u8, u8, u8, u8),
) {
    canvas.set_draw_color(Color::RGBA(c.0, c.1, c.2, c.3));
    for dy in -r..=r {
        let dx = ((r * r - dy * dy) as f32).sqrt().round() as i32;
        let _ = canvas.fill_rect(Rect::new(cx - dx, cy + dy, (dx * 2) as u32, 1));
    }
}

/// O reflexo diagonal do vidro (a faixa clara dos mocks) — alpha baixo,
/// desenhado sobre o vidro morto para ele não ser preto chapado.
pub(super) fn glass_reflection(
    canvas: &mut WindowCanvas,
    sx: i32,
    sy: i32,
    sw: i32,
    sh: i32,
    base: u8,
) {
    let len = (sw / 3).min(sh / 2);
    let esp = 26i32;
    canvas.set_blend_mode(BlendMode::Blend);
    for i in 0..len {
        for e in 0..esp {
            let x = sx + i;
            let y = sy + sh / 4 + esp - i - e;
            if x >= sx && x < sx + sw && y >= sy && y < sy + sh {
                let a = reflection_alpha(e, base);
                if a > 0 {
                    canvas.set_draw_color(Color::RGBA(255, 255, 255, a));
                    let _ = canvas.fill_rect(Rect::new(x, y, 1, 1));
                }
            }
        }
    }
}

/// O alpha do reflexo diagonal na faixa `e` (0..26) — a mesma escada de
/// [`glass_reflection`].
fn reflection_alpha(e: i32, base: u8) -> u8 {
    match e {
        0..=3 => base,
        4..=18 => base / 2,
        _ => base / 4,
    }
}

/// O vidro morto (`TvPhase::Dead`) já composto em RGBA32 opaco, `w`x`h`:
/// as linhas do gradiente quase preto e o reflexo diagonal misturado por
/// cima (`src*a + dst*(1-a)`, arredondado como a GPU faz). Pixel a pixel
/// o que [`draw_tv_glass_phase`] desenha com ~600 `fill_rect` + milhares
/// de retângulos 1x1 por quadro — o frame vivo sobe isto UMA vez numa
/// textura ([`DeadGlass`]) e só copia.
pub(super) fn dead_glass_rgba(w: u32, h: u32) -> Vec<u8> {
    let (sw, sh) = (w as i32, h as i32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    for row in 0..sh {
        let t = row as f32 / sh as f32;
        let v = 5.0 + 3.0 * t;
        let c = [v as u8, v as u8, (v + 1.0) as u8, 255];
        let base = (row * sw * 4) as usize;
        for p in px[base..base + (sw * 4) as usize].as_chunks_mut::<4>().0 {
            p.copy_from_slice(&c);
        }
    }
    // O reflexo: cada (i, e) cai num pixel distinto (x fixa i, y fixa e),
    // então nenhum pixel recebe duas misturas — como no desenho direto.
    let len = (sw / 3).min(sh / 2);
    let esp = 26i32;
    for i in 0..len {
        for e in 0..esp {
            let (x, y) = (i, sh / 4 + esp - i - e);
            if x < 0 || x >= sw || y < 0 || y >= sh {
                continue;
            }
            let a = reflection_alpha(e, 5) as u32;
            if a == 0 {
                continue;
            }
            let o = ((y * sw + x) * 4) as usize;
            for ch in &mut px[o..o + 3] {
                let d = *ch as u32;
                *ch = ((255 * a + d * (255 - a) + 127) / 255) as u8;
            }
        }
    }
    px
}

/// O vidro morto pronto numa textura, refeita só quando o tamanho do tubo
/// muda — é tudo de que ele depende (cores e reflexo são constantes; a
/// posição entra só no `copy`). Textura estática, não render target:
/// sobrevive a um reset dos alvos do renderer.
pub(super) struct DeadGlass {
    pub size: (u32, u32),
    pub tex: Texture,
}

/// O que a fase do tubo manda desenhar NO VIDRO (`tv_phase`), como função
/// pura — o frame vivo (`draw_tv_phase_glass`) e os captures (que não
/// podem pegar `&mut self`) desenham o MESMO vidro, sobre a NEVE no
/// estático ou sobre a IMAGEM DO JOGO no gameplay. A área é sempre
/// `screen` (o glass; o bezel desenha por cima depois).
pub(super) fn draw_tv_glass_phase(canvas: &mut WindowCanvas, screen: Rect, phase: &TvPhase) {
    let (sw, sh) = (screen.width() as i32, screen.height() as i32);
    let sx = screen.x();
    let sy = screen.y();
    match phase {
        TvPhase::On => {}
        TvPhase::Dead => {
            // TV desligada: o vidro morto por cima de tudo — quase preto
            // com um resíduo mínimo e o reflexo diagonal fraco.
            for row in 0..sh {
                let t = row as f32 / sh as f32;
                let v = 5.0 + 3.0 * t;
                canvas.set_draw_color(Color::RGB(v as u8, v as u8, (v + 1.0) as u8));
                let _ = canvas.fill_rect(Rect::new(sx, sy + row, sw as u32, 1));
            }
            glass_reflection(canvas, sx, sy, sw, sh, 5);
        }
        TvPhase::Raster { open, hot, ponto } => {
            if *ponto {
                // afterglow: o ponto quente no centro, o resto do tubo morto
                let (cx, cy) = (sx + sw / 2, sy + sh / 2);
                let glow = (hot * 255.0) as u8;
                for r in (2..=16).rev() {
                    let a = (hot * (16 - r) as f32 / 16.0 * 80.0) as u8;
                    disco_alpha(canvas, cx, cy, (r as f32 * 1.8) as i32, (255, 240, 200, a));
                }
                disco_alpha(canvas, cx, cy, 4, (255, 255, 245, glow));
                let dead = (5.0 + 3.0) as u8;
                canvas.set_draw_color(Color::RGBA(dead, dead, dead + 1, 255));
            } else if *open < 1.0 {
                let virt_h = ((sh as f32 * open.clamp(0.02, 1.0)).round() as i32).max(2);
                let top = sy + sh / 2 - virt_h / 2;
                let bottom = top + virt_h;
                // o tubo fora do raster: morto
                canvas.set_draw_color(Color::RGB(5, 5, 6));
                if top > sy {
                    let _ = canvas.fill_rect(Rect::new(sx, sy, sw as u32, (top - sy) as u32));
                }
                if bottom < sy + sh {
                    let _ = canvas.fill_rect(Rect::new(
                        sx,
                        bottom,
                        sw as u32,
                        (sy + sh - bottom) as u32,
                    ));
                }
                // a borda do raster: gradiente âmbar quente
                for i in 0..8 {
                    let k = 1.0 - i as f32 / 8.0;
                    let a = (200.0 * hot * k) as u8;
                    canvas.set_draw_color(Color::RGBA(255, 214, 150, a));
                    let _ =
                        canvas.fill_rect(Rect::new(sx, (top + i).min(bottom - 1), sw as u32, 1));
                    let _ =
                        canvas.fill_rect(Rect::new(sx, (bottom - 1 - i).max(top), sw as u32, 1));
                }
                // a lavagem do brilho dentro do raster (o fósforo estourado)
                if *hot > 0.0 {
                    canvas.set_draw_color(Color::RGBA(255, 250, 235, (hot * 60.0) as u8));
                    let _ = canvas.fill_rect(Rect::new(sx, top, sw as u32, virt_h as u32));
                }
            } else if *hot > 0.0 {
                // raster aberto ainda assentando: um véu quente que decai
                canvas.set_draw_color(Color::RGBA(255, 250, 235, (hot * 26.0) as u8));
                let _ = canvas.fill_rect(Rect::new(sx, sy, sw as u32, sh as u32));
            }
        }
    }
}

/// Um disco preenchido em coordenadas absolutas do canvas — o assento do
/// Os controles físicos do queixo da TV (plan revision: knob de POWER +
/// par VOL-/VOL+ no `tom prateado como tv antiga`): numa função livre a
/// função da captura estática (`capture_static_bmp`, cuja passada não pode
/// usar `&mut self`) e o frame vivo (`draw_tv_chin`) desenham EXATAMENTE
/// o mesmo conjunto. Devolve os rects para hit-test —
/// `(power, vol_down, vol_up)`, noops quando o queixo é curto demais.
/// NADA de pintura própria da faixa: o bezel mesh já desenha o plástico do
/// queixo (mesma interpolação linear dos outros lados); daqui saem só os
/// controles — e o nameplate (`draw_brand`) segue por cima no caller.
///
/// Revision (a foto do painel real da TV): a fileira completa — TV/VIDEO
/// (tecla redonda pequena), par VOL com rótulo gravado embaixo, par CH
/// com rótulo embaixo, o dial POWER e o LED verde à direita dele.
/// A geometria da fileira de controles do queixo, da esquerda do POWER
/// até a borda esquerda do VOL — compartilhada pelo desenho dos controles
/// e por `brand_logo_rect`, que precisa saber onde a fileira começa.
struct ChinRow {
    key_h: i32,
    pw: i32,
    /// O raio do botão giratório do canal.
    knob_r: i32,
    /// A largura do grupo do canal: o botão e os "3"/"4" que saem dele.
    knob_w: i32,
    /// O rótulo "CH" à esquerda do seletor, como o "VOL" do volume.
    ch_lw: i32,
    menu_w: i32,
    vol_w: i32,
    /// Da borda esquerda do POWER até a borda esquerda do VOL.
    width: i32,
}

impl ChinRow {
    fn new(chin_h: i32) -> Self {
        let key_h = (chin_h as f32 * 0.34).round() as i32;
        let pw = ((key_h + 12) as f32 * 0.58).round() as i32;
        let key_w = (GLYPH_W as i32 * 3).max(38);
        let knob_r = ((chin_h as f32 * 0.29).round() as i32).max(10);
        let knob_w = 2 * knob_r + GLYPH_W as i32 + 4;
        let menu_w = GLYPH_W as i32 * 4 + 14;
        let vol_w = GLYPH_W as i32 * 3 + 8 + 6 + key_w * 2;
        let ch_lw = GLYPH_W as i32 * 2;
        let width = 14 + menu_w + 18 + ch_lw + 6 + knob_w + 18 + vol_w;
        ChinRow {
            key_h,
            pw,
            knob_r,
            knob_w,
            ch_lw,
            menu_w,
            vol_w,
            width,
        }
    }
}

/// Onde a marca prateada mora no queixo: centralizada sob o vidro, ou
/// empurrada para a esquerda quando a fileira de controles chega até ela
/// (janela estreita) — e fora (`None`) se nem assim couber sem pisar na
/// versão impressa no canto esquerdo.
pub(super) fn brand_logo_rect(screen: Rect, out_h: u32, logo_w_per_h: f32) -> Option<Rect> {
    let chin_top = screen.bottom();
    let chin_h = out_h as i32 - chin_top;
    if chin_h < 40 {
        return None;
    }
    let bh = (chin_h as f32 * 0.25).round() as i32;
    let bw = (bh as f32 * logo_w_per_h).round() as i32;
    let mut x = screen.left() + (screen.width() as i32 - bw) / 2;
    let row = ChinRow::new(chin_h);
    if row.key_h >= 14 {
        let pcx = screen.right() - row.pw / 2 - 26;
        let controls_left = pcx - row.pw / 2 - row.width;
        x = x.min(controls_left - 16 - bw);
    }
    // A versão do app e do núcleo, no canto esquerdo do queixo.
    let version_right = screen.left() + GLYPH_W as i32 * 14;
    (x >= version_right).then(|| Rect::new(x, chin_top + (chin_h - bh) / 2, bw as u32, bh as u32))
}

pub(super) fn draw_tv_chin_controls(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    screen: Rect,
    out_h: u32,
    look: ChinLook,
) -> TvChinControls {
    let on = look.on;
    // O queixo é a faixa entre o fim do vidro e o fim do canvas.
    let chin_top = screen.bottom();
    let chin_h = out_h as i32 - chin_top;
    if chin_h < 40 {
        return TvChinControls::offscreen(); // queixo curto demais: sem controles
    }

    // ---- Controles, da direita: POWER (retângulo vertical) + LED, CH (3-4), VOL+/VOL-
    let key_h = (chin_h as f32 * 0.34).round() as i32;
    if key_h < 14 {
        return TvChinControls::offscreen();
    }
    let key_w = (GLYPH_W as i32 * 3).max(38);
    let pair_gap = 6; // dentro do par
    let group_gap = 18; // entre grupos
    let cy = chin_top + chin_h / 2;
    // Revision: as copys precisam continuar legíveis COM a TV ligada —
    // quase-branco no ligado, o médio no desligado. As teclas de VOL e
    // MENU são do plástico escuro do seletor de canal, com a gravação
    // prateada; o alumínio fica só no POWER e na barra do seletor.
    let ink = if on { PRATA_HI } else { PRATA_INK_DIM };
    let key_ink = ink;

    // POWER: retângulo vertical MENOR com o símbolo ⏻ caprichado (aro
    // grosso de 3 lados + traço com ponta redonda), LED verde do ladinho.
    let ph = key_h + 12;
    let pw = (ph as f32 * 0.58).round() as i32;
    let pcx = screen.right() - pw / 2 - 26;
    let pcy = cy;
    let prect = Rect::new(pcx - pw / 2, pcy - ph / 2, pw as u32, ph as u32);
    // o POWER também é do plástico escuro, com o símbolo em prata
    draw_dark_key(canvas, prect, on);
    // o símbolo: aro grosso (2px) com um vão de ~80° no topo, e o traço
    // descendo até o centro do aro — pontinhas arredondadas (um pixel a
    // mais de cada lado na base do traço).
    let r_out = pw as f32 * 0.30;
    let r_in = r_out - 2.5;
    let pc_y = pcy + 2;
    let sym = if on { PRATA_HI } else { PRATA_INK_DIM };
    canvas.set_draw_color(Color::RGB(sym.0, sym.1, sym.2));
    let gap = std::f32::consts::PI / 5.0; // meio-vão em radianos
    let steps = 40;
    for i in 0..=steps {
        let a = gap + (std::f32::consts::TAU - 2.0 * gap) * (i as f32 / steps as f32);
        for rr in [r_in, (r_in + r_out) / 2.0, r_out] {
            let x = pcx as f32 + a.sin() * rr;
            let y = pc_y as f32 - a.cos() * rr;
            let _ = canvas.fill_rect(Rect::new(x.round() as i32 - 1, y.round() as i32, 2, 2));
        }
    }
    let stem_top = (pc_y as f32 - r_out - 3.0) as i32;
    let stem_h = (r_out + 4.0) as i32;
    let _ = canvas.fill_rect(Rect::new(pcx - 1, stem_top, 2, stem_h as u32));
    // a base arredondada do traço
    let _ = canvas.fill_rect(Rect::new(pcx - 2, stem_top + stem_h - 1, 3, 1));

    // LED verde do ladinho, aceso só com a TV ligada.
    let led_cx = screen.right() - 12;
    if on {
        draw_led_color(canvas, led_cx, cy - 6, 12, true, LED_TV_ON, LED_TV_ON_HI);
    }

    // MENU, colado à esquerda do POWER — sempre: é a única porta para o
    // menu de imagem. Numa janela estreita quem cede é a marca do centro
    // do queixo (`brand_logo_rect`), não um botão.
    let g = ChinRow::new(chin_h);
    let menu = Rect::new(
        pcx - pw / 2 - 14 - g.menu_w,
        cy - key_h / 2,
        g.menu_w as u32,
        key_h as u32,
    );
    draw_dark_key(canvas, menu, on);
    draw_text_absolute(
        canvas,
        font,
        menu.x() + 7,
        menu.y() + (menu.height() as i32 - GLYPH_H as i32) / 2,
        TextStyle::new(1, key_ink),
        "MENU",
        usize::MAX,
    );
    let ch_right = menu.x() - group_gap;

    // O seletor de canal giratório: o "3" e o "4" impressos fixos no
    // plástico, um de cada lado do topo, e o botão com a barra prateada
    // apontando o canal — um clique em qualquer ponto alterna.
    let ch_left = ch_right - g.knob_w - 6 - g.ch_lw;
    draw_text_absolute(
        canvas,
        font,
        ch_left,
        cy - GLYPH_H as i32 / 2,
        TextStyle::new(1, ink),
        "CH",
        usize::MAX,
    );
    let kcx = ch_right - g.knob_w / 2;
    // um pouco abaixo do meio: os números moram acima do botão
    let kcy = cy + (chin_h as f32 * 0.07).round() as i32;
    let ch = draw_channel_knob(canvas, font, kcx, kcy, g.knob_r, look.knob, on);
    // a área de clique não sobe para o vidro
    let ch_top = ch.y().max(chin_top);
    let ch = Rect::new(
        ch.x(),
        ch_top,
        ch.width(),
        (ch.bottom() - ch_top).max(1) as u32,
    );

    // O par do volume, com o rótulo inline — - e + em preto.
    let draw_pair =
        |canvas: &mut WindowCanvas, font: &mut Texture, left_x: i32, label: &str| -> (Rect, Rect) {
            let lw = GLYPH_W as i32 * label.chars().count() as i32;
            draw_text_absolute(
                canvas,
                font,
                left_x,
                cy - GLYPH_H as i32 / 2,
                TextStyle::new(1, ink),
                label,
                usize::MAX,
            );
            let down = Rect::new(left_x + lw + 8, cy - key_h / 2, key_w as u32, key_h as u32);
            let up = Rect::new(
                down.x() + pair_gap + key_w,
                down.y(),
                key_w as u32,
                key_h as u32,
            );
            for (r, glyph) in [(down, "-"), (up, "+")] {
                draw_dark_key(canvas, r, on);
                let tw = GLYPH_W as i32 * glyph.chars().count() as i32;
                draw_text_absolute(
                    canvas,
                    font,
                    r.x() + (r.width() as i32 - tw) / 2,
                    r.y() + (r.height() as i32 - GLYPH_H as i32) / 2,
                    TextStyle::new(1, key_ink),
                    glyph,
                    usize::MAX,
                );
            }
            (down, up)
        };
    let (vol_down, vol_up) = draw_pair(canvas, font, ch_left - group_gap - g.vol_w, "VOL");

    TvChinControls {
        vol_down,
        vol_up,
        ch,
        power: prect,
        menu,
    }
}

/// O que o queixo precisa saber do estado da TV para se desenhar.
#[derive(Clone, Copy)]
pub(super) struct ChinLook {
    pub on: bool,
    /// Onde o botão do canal está: 0 = canal 3, 1 = canal 4 (no meio do
    /// giro, um pouco além — o tranco do detent).
    pub knob: f32,
}

/// Quanto o ponteiro do seletor sai do topo para cada lado (40°).
const KNOB_SWING: f32 = 40.0 * std::f32::consts::PI / 180.0;

/// O seletor de canal do queixo (plan revision: "seletor de canal
/// giratório"): assento afundado, saia de plástico escuro serrilhada, a
/// barra prateada de pegada com o ponteiro gravado na ponta, e o "3"/"4"
/// fixos acima — o canal da vez aceso. Tudo em `fill_rect` por linha, sem
/// textura: o queixo é redesenhado em toda tela. Devolve a área de clique
/// (o botão e os números).
fn draw_channel_knob(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    cx: i32,
    cy: i32,
    r: i32,
    knob: f32,
    on: bool,
) -> Rect {
    let (cxf, cyf, rf) = (cx as f32, cy as f32, r as f32);
    let rgb = |c: (u8, u8, u8)| Color::RGB(c.0, c.1, c.2);

    // "3" e "4" fixos, com um tracinho de marca entre o número e o botão.
    let lit = if on { PRATA_HI } else { PRATA_INK_DIM };
    let unlit = if on { (112, 114, 122) } else { (78, 80, 88) };
    let current = if knob < 0.5 { 3 } else { 4 };
    for (n, a) in [(3, -KNOB_SWING), (4, KNOB_SWING)] {
        let ink = if n == current { lit } else { unlit };
        let (sx, sy) = (a.sin(), -a.cos());
        canvas.set_draw_color(rgb(ink));
        for d in [1.5f32, 3.0] {
            let (x, y) = (cxf + sx * (rf + d), cyf + sy * (rf + d));
            let _ = canvas.fill_rect(Rect::new(x.round() as i32, y.round() as i32, 2, 2));
        }
        let lr = rf + 5.0 + GLYPH_W as f32 / 2.0;
        let (lx, ly) = (cxf + sx * lr, cyf + sy * lr);
        draw_text_absolute(
            canvas,
            font,
            (lx - GLYPH_W as f32 / 2.0).round() as i32,
            (ly - GLYPH_H as f32 / 2.0).round() as i32,
            TextStyle::new(1, ink),
            if n == 3 { "3" } else { "4" },
            usize::MAX,
        );
    }

    // O assento afundado: um disco escuro um pouco maior e mais baixo.
    fill_disc(canvas, cx, cy + 1, r + 2, |_| (10, 10, 12));
    // A saia: plástico escuro com a luz vindo de cima.
    let (top, bot) = if on {
        ((96, 97, 104), (24, 24, 28))
    } else {
        ((62, 63, 70), (18, 18, 21))
    };
    fill_disc(canvas, cx, cy, r, |t| lerp_rgb(top, bot, t));

    let angle = (knob - 0.5) * 2.0 * KNOB_SWING;
    // O serrilhado da borda gira com o botão.
    canvas.set_draw_color(rgb(if on { (120, 122, 130) } else { (76, 78, 86) }));
    for i in 0..36 {
        let a = angle + i as f32 * std::f32::consts::TAU / 36.0;
        let (x, y) = (cxf + a.sin() * (rf - 2.0), cyf - a.cos() * (rf - 2.0));
        let _ = canvas.fill_rect(Rect::new(x.round() as i32, y.round() as i32, 1, 1));
    }

    // A barra de pegada: eixo `d` (para o ponteiro), largura em `p`.
    let d = (angle.sin(), -angle.cos());
    let p = (angle.cos(), angle.sin());
    let half_l = rf - 4.0;
    let half_w = (rf * 0.2).max(3.5);
    let at = |u: f32, v: f32, off: (f32, f32)| {
        (
            cxf + off.0 + u * p.0 + v * d.0,
            cyf + off.1 + u * p.1 + v * d.1,
        )
    };
    let bar = |u0: f32, u1: f32, v0: f32, v1: f32, c: f32, off: (f32, f32)| {
        [
            at(u0 + c, v0, off),
            at(u1 - c, v0, off),
            at(u1, v0 + c, off),
            at(u1, v1 - c, off),
            at(u1 - c, v1, off),
            at(u0 + c, v1, off),
            at(u0, v1 - c, off),
            at(u0, v0 + c, off),
        ]
    };
    let (hw, hl) = (half_w, half_l);
    // a sombra, o corpo, o lábio iluminado e o lado na sombra
    fill_convex(
        canvas,
        &bar(-hw, hw, -hl, hl, 2.0, (1.0, 2.0)),
        (10, 10, 12),
    );
    let body = if on { PRATA } else { PRATA_DIM };
    let hi = if on { PRATA_HI } else { (132, 134, 142) };
    let lo = if on { (100, 102, 110) } else { (70, 72, 80) };
    fill_convex(canvas, &bar(-hw, hw, -hl, hl, 2.0, (0.0, 0.0)), body);
    fill_convex(
        canvas,
        &bar(-hw + 1.0, -hw + 3.0, -hl + 2.0, hl - 2.0, 0.0, (0.0, 0.0)),
        hi,
    );
    fill_convex(
        canvas,
        &bar(hw - 2.5, hw - 0.5, -hl + 2.0, hl - 2.0, 0.0, (0.0, 0.0)),
        lo,
    );
    // o ponteiro gravado na ponta de cima
    fill_convex(
        canvas,
        &bar(-1.0, 1.0, hl - 8.0, hl - 2.0, 0.0, (0.0, 0.0)),
        PRATA_INK,
    );

    let lab = r + 5 + GLYPH_W as i32;
    Rect::new(
        cx - r - 6,
        cy - lab - GLYPH_H as i32 / 2,
        (2 * r + 12) as u32,
        (lab + GLYPH_H as i32 / 2 + r + 2) as u32,
    )
}

fn lerp_rgb(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (l(a.0, b.0), l(a.1, b.1), l(a.2, b.2))
}

/// Um disco cheio, uma linha por vez; `shade(t)` dá a cor da linha
/// (t = 0 no topo, 1 na base).
fn fill_disc(
    canvas: &mut WindowCanvas,
    cx: i32,
    cy: i32,
    r: i32,
    shade: impl Fn(f32) -> (u8, u8, u8),
) {
    for dy in -r..=r {
        let hw = ((r * r - dy * dy) as f32).sqrt().round() as i32;
        let c = shade((dy + r) as f32 / (2 * r) as f32);
        canvas.set_draw_color(Color::RGB(c.0, c.1, c.2));
        let _ = canvas.fill_rect(Rect::new(cx - hw, cy + dy, (2 * hw + 1) as u32, 1));
    }
}

/// Preenche um polígono convexo (pontos em ordem), uma linha por vez.
fn fill_convex(canvas: &mut WindowCanvas, pts: &[(f32, f32)], c: (u8, u8, u8)) {
    let (y0, y1) = pts
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
    canvas.set_draw_color(Color::RGB(c.0, c.1, c.2));
    for y in y0.floor() as i32..y1.ceil() as i32 {
        let yc = y as f32 + 0.5;
        let (mut xl, mut xr) = (f32::MAX, f32::MIN);
        for i in 0..pts.len() {
            let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
            if (a.1 <= yc && yc < b.1) || (b.1 <= yc && yc < a.1) {
                let x = a.0 + (yc - a.1) / (b.1 - a.1) * (b.0 - a.0);
                xl = xl.min(x);
                xr = xr.max(x);
            }
        }
        if xl <= xr {
            let (l, rr) = (xl.round() as i32, xr.round() as i32);
            let _ = canvas.fill_rect(Rect::new(l, y, (rr - l).max(1) as u32, 1));
        }
    }
}

/// Uma tecla de plástico escuro do queixo — o mesmo plástico da saia do
/// seletor de canal: 1px de assento quase preto em volta, a face caindo
/// do cinza de cima para o quase preto de baixo, um fio de luz no lábio
/// de cima e o de baixo afundado. O rótulo (prateado) é do caller.
pub(super) fn draw_dark_key(canvas: &mut WindowCanvas, rect: Rect, on: bool) {
    let w = rect.width() as i32;
    let h = rect.height() as i32;
    let (x, y) = (rect.x(), rect.y());
    canvas.set_draw_color(Color::RGB(10, 10, 12));
    let _ = canvas.fill_rect(rect);
    let (top, bot, lip) = if on {
        ((88, 89, 96), (30, 30, 34), (128, 130, 138))
    } else {
        ((60, 61, 68), (20, 20, 23), (84, 86, 94))
    };
    for row in 1..h - 1 {
        let inset = 1 + corner_inset(row - 1, h - 2, 2);
        let t = row - 1; // 0..h-2 dentro da face
        let c = if t == 0 {
            lip
        } else if t >= h - 3 {
            (16, 16, 19)
        } else {
            lerp_rgb(top, bot, t as f32 / (h - 2) as f32)
        };
        canvas.set_draw_color(Color::RGB(c.0, c.1, c.2));
        let _ = canvas.fill_rect(Rect::new(x + inset, y + row, (w - 2 * inset) as u32, 1));
    }
}

/// Horizontal inset of scanline `row` in a bar `h` rows tall with corner
/// radius `radius` — quarter-circle steps, the same fake-a-circle trick
/// `draw_led` uses, stacked into whole rows here. Clamped so a bar can never
/// be narrower than its own radius.
pub(super) fn corner_inset(row: i32, h: i32, radius: i32) -> i32 {
    let r = radius.min(h / 2).max(0);
    let from_end = row.min(h - 1 - row);
    if from_end >= r {
        return 0;
    }
    let dx = r - from_end;
    r - ((r * r - dx * dx) as f32).sqrt().floor() as i32
}

/// One flat matte key filling `rect` — the console's own plastic (plan
/// revision: rectangular, no gloss anywhere), built entirely from flat
/// scanline fills like the rest of this UI: a dark seating ring, softly
/// rounded corners and a dead-flat body colour. `rect` itself is untouched
/// beyond the fill — callers keep using it for hit-testing, so a restyle can
/// never move a click target.
pub(super) fn draw_flat_key(canvas: &mut WindowCanvas, rect: Rect, p: &FlatKeyPalette) {
    let (w, h) = (rect.width() as i32, rect.height() as i32);
    if w <= 0 || h <= 0 {
        return;
    }
    // Rectangular, only softly rounded — the photo's keys are rectangles,
    // not pills.
    let radius = (h / 5).max(2);
    // The seating ring: one rounded pass in the outline colour.
    canvas.set_draw_color(Color::RGB(p.outline.0, p.outline.1, p.outline.2));
    for row in 0..h {
        let inset = corner_inset(row, h, radius);
        let _ = canvas.fill_rect(Rect::new(
            rect.x() + inset,
            rect.y() + row,
            (w - inset * 2).max(0) as u32,
            1,
        ));
    }
    if w < 3 || h < 3 {
        return;
    }
    // The flat body one pixel inside the ring.
    let (body_w, body_h) = (w - 2, h - 2);
    let body_radius = (radius - 1).max(1);
    canvas.set_draw_color(Color::RGB(p.body.0, p.body.1, p.body.2));
    for row in 0..body_h {
        let inset = corner_inset(row, body_h, body_radius).min(body_w / 2);
        let _ = canvas.fill_rect(Rect::new(
            rect.x() + 1 + inset,
            rect.y() + 1 + row,
            (body_w - inset * 2).max(0) as u32,
            1,
        ));
    }
}

/// Draw one clickable panel button: a filled box (brighter/bordered when
/// `lit`, flush with the panel background otherwise) with `text` centered
/// inside — clipped to fit the box, with a trailing `...` if it doesn't
/// (plan revision: `text` used to just draw past the box's edge unclipped
/// when too long for it, visually merging into whatever sat next to it —
/// harmless while every label here was short, a real problem once longer
/// labels started coming through).
/// Returns the box's rect for hit-testing.
pub(super) fn draw_button(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    rect: Rect,
    text: &str,
    lit: bool,
) -> Rect {
    let (bg, border, fg) = if lit {
        (PANEL_BTN_BG, PANEL_TEXT, PANEL_TEXT)
    } else {
        (PANEL_BG, PANEL_BTN_BG, PANEL_DIM)
    };
    canvas.set_draw_color(Color::RGB(border.0, border.1, border.2));
    let _ = canvas.fill_rect(rect);
    let inset = Rect::new(
        rect.x() + 1,
        rect.y() + 1,
        rect.width() - 2,
        rect.height() - 2,
    );
    canvas.set_draw_color(Color::RGB(bg.0, bg.1, bg.2));
    let _ = canvas.fill_rect(inset);

    let side_pad = 6i32;
    let max_chars = (((rect.width() as i32 - side_pad * 2) / GLYPH_W as i32).max(1)) as usize;
    let shown = clip_label(text, max_chars);
    let text_w = (GLYPH_W as i32) * shown.chars().count() as i32;
    let tx = rect.x() + (rect.width() as i32 - text_w).max(4) / 2;
    let ty = rect.y() + (rect.height() as i32 - GLYPH_H as i32) / 2;
    draw_text_absolute(
        canvas,
        font,
        tx,
        ty,
        TextStyle::new(1, fg),
        &shown,
        usize::MAX,
    );
    rect
}

/// Eject alone (this function) wears the plain grey key — the real button's
/// own look (the reference photo: mid grey face, light moulded label),
/// rectangular and matte like the Power/Reset keys next to it (see
/// `draw_rocker`), just without their debossed oval. A step darker when
/// inert. Everything else on the panel stays a plain `draw_button`. Label
/// layout identical to `draw_button`; returns `rect` for hit-testing.
pub(super) fn draw_eject_button(
    canvas: &mut WindowCanvas,
    _font: &mut Texture,
    rect: Rect,
    text: &str,
    lit: bool,
) -> Rect {
    let pal = if lit { &EJECT_LIT } else { &EJECT_DIM };
    draw_flat_key(canvas, rect, pal);

    // Printed in the same vector SNES-style face as the Power/Reset keys
    // (see `snes_font`), shrunk to fit the key's width.
    let mut lh = (rect.height() * 50 / 100).max(6);
    while lh > 6 && snes_font::label_width(text, lh) as i32 > rect.width() as i32 - 12 {
        lh -= 1;
    }
    let lw = snes_font::label_width(text, lh) as i32;
    let tx = rect.x() + (rect.width() as i32 - lw) / 2;
    let ty = rect.y() + (rect.height() as i32 - lh as i32) / 2;
    snes_font::draw_label(canvas, tx, ty, lh, pal.text, text);
    rect
}

/// Truncate `text` to at most `max_chars`, replacing the last one with an
/// ellipsis when it doesn't fit — plain `...` rather than `…` since the
/// font atlas only covers Basic Latin through Latin-1 Supplement (see
/// `GLYPH_FIRST`/`GLYPH_LAST`), which that single-character ellipsis falls
/// outside of. A no-op (returns `text` unchanged, borrowed) when it
/// already fits.
pub(super) fn clip_label(text: &str, max_chars: usize) -> Cow<'_, str> {
    if text.chars().count() <= max_chars {
        return Cow::Borrowed(text);
    }
    if max_chars <= 3 {
        // No room for both content and "...": just hard-crop.
        return Cow::Owned(text.chars().take(max_chars).collect());
    }
    let keep: String = text.chars().take(max_chars - 3).collect();
    Cow::Owned(format!("{keep}..."))
}

/// A small round-ish power LED (plan revision: "luz vermelha led indicando
/// o power, em cima do botao ejetar, igual o console original") —
/// approximated with three stacked rects (narrow/wide/narrow) rather than a
/// true circle, since every other shape in this UI is a flat rect and a real
/// circle would need its own mesh just for a decoration this small. Lit red
/// while the console is powered, a dark unlit red otherwise (the same
/// resting look almost every console's power LED has when off), with a tiny
/// glossy highlight dot when lit.
pub(super) fn draw_led(canvas: &mut WindowCanvas, center_x: i32, top: i32, size: i32, lit: bool) {
    draw_led_color(canvas, center_x, top, size, lit, LED_ON, LED_ON_HI);
}

/// O mesmo LED com cor própria — o queixo da TV usa o verde da foto do
/// painel real, o console continua no vermelho.
pub(super) fn draw_led_color(
    canvas: &mut WindowCanvas,
    center_x: i32,
    top: i32,
    size: i32,
    lit: bool,
    on: (u8, u8, u8),
    on_hi: (u8, u8, u8),
) {
    // A real anti-aliased disc: per-pixel coverage at the rim, a soft
    // radial falloff toward the edge and — lit only — a small smooth
    // highlight, instead of the old stacked-rect approximation.
    let base = if lit { on } else { LED_OFF };
    let r = size as f32 * 0.47;
    let (cx, cy) = (center_x as f32, top as f32 + size as f32 / 2.0);
    let n = size / 2 + 2;
    let prev = canvas.blend_mode();
    canvas.set_blend_mode(BlendMode::Blend);
    for iy in -n..=n {
        for ix in -n..=n {
            let (px, py) = (center_x + ix, top + size / 2 + iy);
            // Pixel centre relative to the disc's centre.
            let dx = px as f32 + 0.5 - cx;
            let dy = py as f32 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let cover = (r + 0.5 - dist).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let d = (dist / r).min(1.0);
            let shade = 1.0 - 0.38 * d * d;
            let mut col = [
                base.0 as f32 * shade,
                base.1 as f32 * shade,
                base.2 as f32 * shade,
            ];
            if lit {
                let hx = dx + r * 0.32;
                let hy = dy + r * 0.38;
                let hd = (hx * hx + hy * hy).sqrt() / (r * 0.42);
                let w = (1.0 - hd).clamp(0.0, 1.0);
                let w = w * w * (3.0 - 2.0 * w);
                col[0] += (on_hi.0 as f32 - col[0]) * w;
                col[1] += (on_hi.1 as f32 - col[1]) * w;
                col[2] += (on_hi.2 as f32 - col[2]) * w;
            }
            canvas.set_draw_color(Color::RGBA(
                col[0] as u8,
                col[1] as u8,
                col[2] as u8,
                (cover * 255.0) as u8,
            ));
            let _ = canvas.fill_rect(Rect::new(px, py, 1, 1));
        }
    }
    canvas.set_blend_mode(prev);
}

/// One Power/Reset rocker switch, styled after the reference photo of the
/// real console's controls: a dark rectangular pocket with a grey rim, and a
/// wide violet key riding in it — rectangular with softly rounded corners,
/// its face broken only by a shallow oval pressed into the middle, the label
/// printed inside that oval like the real switches. All flat matte colour —
/// the old convex dome's gloss is gone (plan revision). The key sits at the
/// top when `up` (Power: on; Reset: mid-press) or the bottom otherwise.
/// `lit` dims the whole thing the same way `draw_button` does for a control
/// that wouldn't do anything right now (Reset while the console is off).
/// Returns `track` for hit-testing — the whole switch body is clickable,
/// not just the thumb.
pub(super) fn draw_rocker(
    canvas: &mut WindowCanvas,
    _font: &mut Texture,
    track: Rect,
    label: &str,
    up: bool,
    lit: bool,
) -> Rect {
    canvas.set_draw_color(Color::RGB(
        SWITCH_TRACK_BORDER.0,
        SWITCH_TRACK_BORDER.1,
        SWITCH_TRACK_BORDER.2,
    ));
    let _ = canvas.fill_rect(track);
    let inset = Rect::new(
        track.x() + 2,
        track.y() + 2,
        track.width().saturating_sub(4),
        track.height().saturating_sub(4),
    );
    canvas.set_draw_color(Color::RGB(
        SWITCH_TRACK_BG.0,
        SWITCH_TRACK_BG.1,
        SWITCH_TRACK_BG.2,
    ));
    let _ = canvas.fill_rect(inset);
    // The recess reads because light from above leaves its top edge dark —
    // a couple of scanlines hugging the lip, darker than the pocket floor.
    canvas.set_draw_color(Color::RGB(
        SWITCH_TRACK_SHADOW.0,
        SWITCH_TRACK_SHADOW.1,
        SWITCH_TRACK_SHADOW.2,
    ));
    let _ = canvas.fill_rect(Rect::new(inset.x(), inset.y(), inset.width(), 2));

    // The thumb, riding high or low with the switch's state — wide margins
    // all round the pocket, like the photo.
    let thumb_h = (inset.height() * 55 / 100).max(1);
    let thumb_y = if up {
        inset.y() + 3
    } else {
        inset.bottom() - thumb_h as i32 - 3
    };
    let thumb = Rect::new(
        inset.x() + 4,
        thumb_y,
        inset.width().saturating_sub(8),
        thumb_h,
    );
    draw_rocker_thumb(canvas, thumb, lit);

    // The label is printed on the key itself — inside the debossed oval, in
    // vector glyphs shaped like the SNES silkscreen (bold condensed italic;
    // see `snes_font`), shrunk if it would overflow the oval.
    let oval_w = (thumb.width() as i32 - 2) * 8 / 10;
    let oval_h = (thumb.height() as i32 - 2) * 66 / 100;
    let mut lh = ((oval_h * 80 / 100).max(6)) as u32;
    while lh > 6 && snes_font::label_width(label, lh) as i32 > oval_w * 88 / 100 {
        lh -= 1;
    }
    let lw = snes_font::label_width(label, lh) as i32;
    let tx = thumb.x() + (thumb.width() as i32 - lw) / 2;
    let ty = thumb.y() + (thumb.height() as i32 - lh as i32) / 2;
    snes_font::draw_label(
        canvas,
        tx,
        ty,
        lh,
        if lit {
            ROCKER_LIT.text
        } else {
            ROCKER_DIM.text
        },
        label,
    );
    track
}

/// One Power/Reset thumb face (plan revision: "deve ser retangular, com um
/// leve oval em baixo relevo no meio... mantenha a cor mas nao deve ter
/// brilho") — the flat matte key of `draw_flat_key` in the switch violet
/// (`ROCKER_DIM`'s grey when inert), plus a shallow oval pressed into the
/// middle: its floor a darker violet, its top wall darker still (shadow) and
/// its bottom wall a touch lighter than the floor — light from above reading
/// the recess. The oval is an ellipse chord per scanline, the same
/// fake-a-circle trick `corner_inset` uses.
pub(super) fn draw_rocker_thumb(canvas: &mut WindowCanvas, rect: Rect, lit: bool) {
    let (w, h) = (rect.width() as i32, rect.height() as i32);
    if w <= 0 || h <= 0 {
        return;
    }
    let p = if lit { &ROCKER_LIT } else { &ROCKER_DIM };
    draw_flat_key(canvas, rect, p);
    if w < 5 || h < 5 {
        return;
    }
    let (body_w, body_h) = (w - 2, h - 2);

    // The shallow oval pressed into the face — most of the key's width, a
    // bit over half its height, dead centre. Its top ~third sits in shadow
    // and its bottom edge catches the light, the floor between them.
    let oval_h = (body_h * 66 / 100).max(3);
    let oval_w = (body_w * 8 / 10).max(3);
    let oval_left = rect.x() + 1 + body_w / 2 - oval_w / 2;
    let oval_top = rect.y() + 1 + body_h / 2 - oval_h / 2;
    let half_w = oval_w as f32 / 2.0;
    let half_h = oval_h as f32 / 2.0;
    let dark_rows = (oval_h * 3 / 10).max(1);
    let lit_rows = (oval_h * 2 / 10).max(1);
    for row in 0..oval_h {
        let dy = (row as f32 + 0.5 - half_h) / half_h;
        let half = ((1.0 - dy * dy).max(0.0).sqrt() * half_w) as i32;
        if half <= 0 {
            continue;
        }
        let (r, g, b) = if row < dark_rows {
            p.wall_dark
        } else if row >= oval_h - lit_rows {
            p.wall_lit
        } else {
            p.floor
        };
        canvas.set_draw_color(Color::RGB(r, g, b));
        let _ = canvas.fill_rect(Rect::new(
            oval_left + oval_w / 2 - half,
            oval_top + row,
            (half * 2) as u32,
            1,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_glass_matches_the_drawn_rows_and_reflection() {
        let (w, h) = (640u32, 480u32);
        let px = dead_glass_rgba(w, h);
        assert_eq!(px.len(), (w * h * 4) as usize);
        assert!(px.as_chunks::<4>().0.iter().all(|p| p[3] == 255), "opaco");
        let at = |x: u32, y: u32| {
            let o = ((y * w + x) * 4) as usize;
            [px[o], px[o + 1], px[o + 2]]
        };
        // As linhas: 5 no topo, quase 8 embaixo, o azul +1.
        assert_eq!(at(w - 1, 0), [5, 5, 6]);
        assert_eq!(at(w - 1, h - 1), [7, 7, 8]);
        // O reflexo: começa em (0, h/4 + 26) na faixa clara (alpha 5).
        let y0 = h / 4 + 26;
        let row = at(w - 1, y0);
        let lit = at(0, y0);
        assert!(lit[0] > row[0], "reflexo mais claro: {lit:?} vs {row:?}");
        // Mistura arredondada: dst 6, a 5 → (1275 + 6*250 + 127)/255 = 11.
        let d = row[0] as u32;
        assert_eq!(lit[0] as u32, (255 * 5 + d * 250 + 127) / 255);
        // Quantos pixels o desenho direto toca (dentro do tubo, alpha > 0).
        let len = (w as i32 / 3).min(h as i32 / 2);
        let mut touched = 0;
        let mut brighter = 0;
        for i in 0..len {
            for e in 0..26 {
                let y = h as i32 / 4 + 26 - i - e;
                if y >= 0 && y < h as i32 && reflection_alpha(e, 5) > 0 {
                    touched += 1;
                    let base = at(w - 1, y as u32);
                    if at(i as u32, y as u32) != base {
                        brighter += 1;
                    }
                }
            }
        }
        assert!(touched > 0);
        assert_eq!(brighter, touched, "cada pixel do reflexo misturado");
    }

    #[test]
    fn dead_glass_tiny_and_empty_sizes() {
        assert!(dead_glass_rgba(0, 0).is_empty());
        let px = dead_glass_rgba(3, 2);
        assert_eq!(px.len(), 24);
    }
}
