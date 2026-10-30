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
                let a = match e {
                    0..=3 => base,
                    4..=18 => base / 2,
                    _ => base / 4,
                };
                if a > 0 {
                    canvas.set_draw_color(Color::RGBA(255, 255, 255, a));
                    let _ = canvas.fill_rect(Rect::new(x, y, 1, 1));
                }
            }
        }
    }
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
pub(super) fn draw_tv_chin_controls(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    screen: Rect,
    out_h: u32,
    on: bool,
) -> TvChinControls {
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
    // quase-branco no ligado, o médio no desligado. Os -/+ das teclas são
    // pretos (tinta de tecla de verdade sobre o alumínio).
    let ink = if on { PRATA_HI } else { PRATA_INK_DIM };
    let key_ink = PRATA_INK;

    // POWER: retângulo vertical MENOR com o símbolo ⏻ caprichado (aro
    // grosso de 3 lados + traço com ponta redonda), LED verde do ladinho.
    let ph = key_h + 12;
    let pw = (ph as f32 * 0.58).round() as i32;
    let pcx = screen.right() - pw / 2 - 26;
    let pcy = cy;
    let prect = Rect::new(pcx - pw / 2, pcy - ph / 2, pw as u32, ph as u32);
    draw_silver_key(canvas, prect, on);
    // o símbolo: aro grosso (2px) com um vão de ~80° no topo, e o traço
    // descendo até o centro do aro — pontinhas arredondadas (um pixel a
    // mais de cada lado na base do traço).
    let r_out = pw as f32 * 0.30;
    let r_in = r_out - 2.5;
    let pc_y = pcy + 2;
    let sym = if on { (24, 26, 32) } else { (56, 58, 66) };
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

    // O botão único do canal: rótulo "CH" + a tecla "3-4" — um clique
    // alterna entre os dois canais que a TV tem.
    let ch_label = "CH";
    let ch_lw = GLYPH_W as i32 * ch_label.chars().count() as i32;
    let ch_key_w = GLYPH_W as i32 * 3 + 12;
    let ch_left = pcx - pw / 2 - group_gap - ch_lw - 8 - ch_key_w;
    draw_text_absolute(
        canvas,
        font,
        ch_left,
        cy - GLYPH_H as i32 / 2,
        TextStyle::new(1, ink),
        ch_label,
        usize::MAX,
    );
    let ch = Rect::new(
        ch_left + ch_lw + 8,
        cy - key_h / 2,
        ch_key_w as u32,
        key_h as u32,
    );
    draw_silver_key(canvas, ch, on);
    let tw = GLYPH_W as i32 * 3;
    draw_text_absolute(
        canvas,
        font,
        ch.x() + (ch.width() as i32 - tw) / 2,
        ch.y() + (ch.height() as i32 - GLYPH_H as i32) / 2,
        TextStyle::new(1, key_ink),
        "3-4",
        usize::MAX,
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
                draw_silver_key(canvas, r, on);
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
    let vol_w = GLYPH_W as i32 * 3 + 8 + pair_gap + key_w * 2;
    let (vol_down, vol_up) = draw_pair(canvas, font, ch_left - group_gap - vol_w, "VOL");

    TvChinControls {
        vol_down,
        vol_up,
        ch,
        power: prect,
    }
}

/// A tecla redonda pequena (TV/VIDEO da foto): o mesmo alumínio das teclas
/// O dial de POWER do queixo (plan revision: "botão de power ... mais
/// retro") — o knob de tubo: assento afundado no plástico, carcaça
/// serrilhada (a pegada de dedos), torus gravado com dômo (a luz quebra
/// no topo, a sombra assenta embaixo) e o ponteiro gravado — em pé quando
/// Uma tecla de alumínio moldada do queixo (plan revision: os botões de
/// volume "mais retro") — o bisel é o que faz a tecla: 1px de assento
/// escuro em volta, a luz quebrando nos dois primeiros degraus da face e
/// a sombra mordendo os dois de baixo. O rótulo gravado é do caller.
pub(super) fn draw_silver_key(canvas: &mut WindowCanvas, rect: Rect, on: bool) {
    let w = rect.width() as i32;
    let h = rect.height() as i32;
    let (x, y) = (rect.x(), rect.y());
    // o assento: 1px escuro em volta da face inteira
    canvas.set_draw_color(Color::RGB(22, 22, 26));
    let _ = canvas.fill_rect(rect);
    // a face em scanlines, com o bisel nos lábios de cima e de baixo
    let body = if on { PRATA } else { PRATA_DIM };
    let lip_dark = if on { (86, 88, 96) } else { (56, 58, 66) };
    for row in 1..h - 1 {
        let inset = 1 + corner_inset(row - 1, h - 2, 2);
        let t = row - 1; // 0..h-2 dentro da face
        let c = if t < 2 {
            PRATA_HI
        } else if t >= h - 4 {
            lip_dark
        } else {
            body
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
/// harmless while every label here was short, a real problem once the
/// Cheats modal started passing database-length descriptions through).
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
    font: &mut Texture,
    rect: Rect,
    text: &str,
    lit: bool,
) -> Rect {
    let pal = if lit { &EJECT_LIT } else { &EJECT_DIM };
    draw_flat_key(canvas, rect, pal);

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
        TextStyle::new(1, pal.text),
        &shown,
        usize::MAX,
    );
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
    let (r, g, b) = if lit { on } else { LED_OFF };
    canvas.set_draw_color(Color::RGB(r, g, b));
    // Four rows, widening then narrowing (roughly 60/90/90/60% of `size`) —
    // a softer step than a plain narrow/wide/narrow, so it reads as a round
    // dot instead of a plus sign at this small a scale.
    let step = (size / 4).max(1);
    let widths = [size * 3 / 5, size * 9 / 10, size * 9 / 10, size * 3 / 5];
    for (i, w) in widths.into_iter().enumerate() {
        let w = w.max(2);
        let h = if i == widths.len() - 1 {
            (size - step * i as i32).max(1)
        } else {
            step
        };
        let _ = canvas.fill_rect(Rect::new(
            center_x - w / 2,
            top + step * i as i32,
            w as u32,
            h as u32,
        ));
    }
    if lit {
        canvas.set_draw_color(Color::RGB(on_hi.0, on_hi.1, on_hi.2));
        let hi = (size / 4).max(1) as u32;
        let hi_x = center_x - (size * 3 / 10);
        let _ = canvas.fill_rect(Rect::new(hi_x, top + step, hi, hi));
    }
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
    font: &mut Texture,
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

    // The label is printed on the key itself — inside the debossed oval,
    // which shares the thumb's centre.
    let text_w = (GLYPH_W as i32) * label.chars().count() as i32;
    let tx = thumb.x() + (thumb.width() as i32 - text_w).max(0) / 2;
    let ty = thumb.y() + (thumb.height() as i32 - GLYPH_H as i32) / 2;
    draw_text_absolute(
        canvas,
        font,
        tx,
        ty,
        TextStyle::new(
            1,
            if lit {
                ROCKER_LIT.text
            } else {
                ROCKER_DIM.text
            },
        ),
        label,
        usize::MAX,
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
    let oval_h = (body_h * 55 / 100).max(3);
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
