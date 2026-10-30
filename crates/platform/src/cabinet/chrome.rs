//! Mobília da TV: fechar/minimizar, logo da marca e as camadas de canal.

use super::*;

/// The top-left "fechar app" button (plan revision: "criar botao de fechar
/// app no canto superior esquerdo") — a small square with an "X", same
/// visual language as `draw_button`. Fixed distance from the cabinet's own
/// top-left corner (canvas-local coordinates, the same space
/// `window_to_output` maps clicks into), so it sits in the same spot
/// regardless of window size or ultrawide letterboxing. Callers draw the
/// pair last, on top of whatever else the screen drew, on EVERY screen now
/// (plan revision: "mostrar o fechar e minimizar em todas as telas").
/// Returns the rect drawn, for `Cabinet::close_button`/`hit_close_button`.
pub(super) fn draw_close_button(canvas: &mut WindowCanvas, font: &mut Texture) -> Rect {
    const MARGIN: i32 = 14;
    const SIZE: u32 = 32;
    draw_button(
        canvas,
        font,
        Rect::new(MARGIN, MARGIN, SIZE, SIZE),
        "X",
        true,
    )
}

/// The minimize button, sitting right next to `draw_close_button` (plan
/// revision: "colocar botão de minimizar ao lado de fechar") — same square
/// look, a dash for the iconify glyph. Returns the rect drawn, for
/// `Cabinet::minimize_button`/`hit_minimize_button`.
pub(super) fn draw_minimize_button(canvas: &mut WindowCanvas, font: &mut Texture) -> Rect {
    const MARGIN: i32 = 14;
    const SIZE: u32 = 32;
    const GAP: i32 = 8;
    draw_button(
        canvas,
        font,
        Rect::new(MARGIN + SIZE as i32 + GAP, MARGIN, SIZE, SIZE),
        "-",
        true,
    )
}

/// The set's nameplate, printed into the chin left of the tube — part of the
/// cabinet itself, so unlike the panel it's drawn in every context (shelf,
/// game, idle-off) and never disappears. A no-op if the chin is too short to
/// hold it. `label` is `Cabinet::nameplate` (plan revision — `BRAND` plus the
/// app/core version once the app calls `set_nameplate`).
/// The channel banner's green — vivid OSD green, like the phosphor filter a
/// TV applies to its own on-screen display (plan revision, "CH 3").
pub(super) const OSD_GREEN: (u8, u8, u8) = (60, 230, 70);

/// The "CH 3" channel banner (plan revision: "quando a tv tiver fora do ar
/// ... mostrar CH 3 na tv como era na tv antiga quando nao tinha sinal") —
/// green, top-right of the tube. Drawn flat on the glass, deliberately NOT
/// warped with the picture: an OSD is the TV's own overlay, not part of the
/// A marca "SixteeN" centralizada no queixo — a arte prateada embutida
/// (`BRAND_LOGO_*`), desenhada do mapa de imagens; sem ela registrada,
/// nada a fazer.
pub(super) fn draw_brand_logo(
    canvas: &mut WindowCanvas,
    images: &HashMap<u64, ImgTex>,
    screen: Rect,
    out_h: u32,
) {
    if !images.contains_key(&BRAND_LOGO_ID) {
        return;
    }
    let chin_top = screen.bottom();
    let chin_h = out_h as i32 - chin_top;
    if chin_h < 40 {
        return;
    }
    let bh = (chin_h as f32 * 0.25).round() as u32;
    let bw = (bh as f32 * (BRAND_LOGO_W as f32 / BRAND_LOGO_H as f32)).round() as u32;
    let x = screen.left() + (screen.width() as i32 - bw as i32) / 2;
    let y = chin_top + (chin_h - bh as i32) / 2;
    draw_image_absolute(canvas, images, BRAND_LOGO_ID, x, y, bw, bh);
}

/// signal (same reasoning as `draw_brand`'s chin text).
pub(super) fn draw_ch_osd(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    screen: Rect,
    label: &str,
) {
    const SCALE: u32 = 3;
    let label = if label.ends_with(' ') {
        label.to_string()
    } else {
        format!(" {} ", label)
    };
    let w = GLYPH_W as i32 * SCALE as i32 * label.chars().count() as i32;
    let margin = 24;
    let x = screen.right() - margin - w;
    let y = screen.top() + margin;
    // A 2px dark offset keeps the green readable over bright snow without a
    // full outline pass.
    draw_text_absolute(
        canvas,
        font,
        x + 2,
        y + 2,
        TextStyle::new(SCALE, (12, 14, 12)),
        &label,
        usize::MAX,
    );
    draw_text_absolute(
        canvas,
        font,
        x,
        y,
        TextStyle::new(SCALE, OSD_GREEN),
        &label,
        usize::MAX,
    );
}

/// A interferência do canal 4 desenhada sobre o sinal: o estático
/// picado (`specks`, pré-computado pelo caller), as linhas de
/// dessincronia e a barra de hum rolando devagar — o canal errado de
/// uma TV de tubo com o dial fora do ponto.
pub(super) fn draw_ch4_layer(canvas: &mut WindowCanvas, screen: Rect, specks: &[(i32, i32, bool)]) {
    if screen.width() == 0 || screen.height() == 0 {
        return;
    }
    for (x, y, bright) in specks {
        if *bright {
            canvas.set_draw_color(Color::RGBA(255, 255, 255, 130));
        } else {
            canvas.set_draw_color(Color::RGBA(0, 0, 0, 130));
        }
        let _ = canvas.fill_rect(Rect::new(*x, *y, 2, 1));
    }
    // Linhas de dessincronia: faixas horizontais finas e claras.
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut seed = millis | 1;
    for _ in 0..3 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let y = screen.y() + (seed % (screen.height() as u64)) as i32;
        canvas.set_draw_color(Color::RGBA(255, 255, 255, 36));
        let _ = canvas.fill_rect(Rect::new(
            screen.x(),
            y,
            screen.width(),
            1 + (seed % 2) as u32,
        ));
    }
    // A barra de hum: uma faixa clara + uma escura rolando de cima a
    // baixo, ~6s por volta.
    let period = screen.height() as u64 + 240;
    let by = screen.y() + ((millis * 60 / 1000) % period) as i32 - 120;
    canvas.set_draw_color(Color::RGBA(255, 255, 255, 22));
    let _ = canvas.fill_rect(Rect::new(screen.x(), by, screen.width(), 70));
    canvas.set_draw_color(Color::RGBA(0, 0, 0, 40));
    let _ = canvas.fill_rect(Rect::new(screen.x(), by + 70, screen.width(), 90));
}
