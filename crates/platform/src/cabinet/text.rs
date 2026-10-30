//! Texto: atlas da fonte, estilos e os helpers de desenho em coordenadas
//! absolutas (inclui o painel de desempenho do modo dev).

use super::*;

/// Map a char to its column in the font atlas; anything outside the covered
/// range (or with no glyph in the font) falls back to `?`.
pub(super) fn glyph_index(ch: char) -> u32 {
    let c = atlas_char(ch) as u32;
    if (GLYPH_FIRST..=GLYPH_LAST).contains(&c) {
        c - GLYPH_FIRST
    } else {
        '?' as u32 - GLYPH_FIRST
    }
}

/// Copies fixas e strings dinâmicas (changelog do GitHub, títulos de jogo,
/// status) trazem pontuação tipográfica e setas fora do atlas — cada uma vira
/// o glifo ASCII mais próximo em vez do `?` (plan revision: "corrigir copy
/// com ? que estao aparecendo no app"). Só mapeia o que está FORA de
/// 0x20..=0xFF: o que o Latin-1 já cobre usa o glifo de verdade.
pub(super) fn atlas_char(ch: char) -> char {
    match ch {
        '–' | '—' | '•' => '-',
        '…' => '.',
        '‹' | '←' => '<',
        '›' | '→' => '>',
        '↑' => '^',
        '↓' => 'v',
        '‘' | '’' => '\'',
        '“' | '”' => '"',
        '\u{3000}' => ' ',
        // full-width (Ａ→A, １→1): espelho fixo do ASCII, 0xFEE0 abaixo.
        c if (0xFF01..=0xFF5E).contains(&(c as u32)) => {
            char::from_u32(c as u32 - 0xFEE0).unwrap_or('?')
        }
        c => c,
    }
}

/// A linha do painel de desempenho — pura para o teste fixar o formato:
/// ASCII de atlas (sem °/·), dois espaços entre segmentos e os segmentos
/// cuja leitura falhou simplesmente omitidos.
pub(super) fn perf_line(fps: u32, cpu: Option<u32>, ram_mb: Option<u32>) -> String {
    let mut parts = vec![format!("{fps} FPS")];
    if let Some(c) = cpu {
        parts.push(format!("CPU {c}C"));
    }
    if let Some(m) = ram_mb {
        parts.push(format!("RAM {m} MB"));
    }
    parts.join("  ")
}

/// O painel de desempenho do modo dev — na faixa da moldura acima da tela:
/// centralizado sobre a TV, fundo escuro translúcido para ler sobre a
/// carcaça. `center` é o rect da tela (para o x); `band` é (topo, altura).
pub(super) fn draw_perf_overlay(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    center: Rect,
    band: (i32, i32),
    line: &str,
) {
    const PAD_X: i32 = 10;
    const PAD_Y: i32 = 6;
    if line.is_empty() {
        return;
    }
    let text_w = line.chars().count() as i32 * GLYPH_W as i32;
    let row = GLYPH_H as i32;
    let x = center.left() + (center.width() as i32 - text_w) / 2;
    let y = band.0 + ((band.1 - row) / 2).max(2);
    canvas.set_blend_mode(BlendMode::Blend);
    canvas.set_draw_color(Color::RGBA(0, 0, 0, 150));
    let _ = canvas.fill_rect(Rect::new(
        x - PAD_X,
        y - PAD_Y,
        (text_w + PAD_X * 2) as u32,
        (row + PAD_Y * 2) as u32,
    ));
    draw_text_absolute(
        canvas,
        font,
        x,
        y,
        TextStyle::new(1, PANEL_TEXT),
        line,
        usize::MAX,
    );
}

/// Scale + colour for one of the absolute-coordinate text helpers below —
/// bundled so those functions stay under clippy's argument-count limit.
#[derive(Clone, Copy)]
pub(super) struct TextStyle {
    pub(super) scale: u32,
    pub(super) color: (u8, u8, u8),
    /// Negrito da fonte mono: cada glifo desenhado duas vezes, 1px ao
    /// lado — duplo impacto, o jeito clássico de engrossar bitmap.
    pub(super) bold: bool,
}

impl TextStyle {
    pub(super) fn new(scale: u32, color: (u8, u8, u8)) -> Self {
        Self {
            scale,
            color,
            bold: false,
        }
    }
    pub(super) fn bold(scale: u32, color: (u8, u8, u8)) -> Self {
        Self {
            scale,
            color,
            bold: true,
        }
    }
}

/// A single line of text at absolute window coordinates, truncated to
/// `max_chars` (`usize::MAX` for no truncation) — for cabinet furniture that
/// isn't inside a `Screen`.
pub(super) fn draw_text_absolute(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    x: i32,
    y: i32,
    style: TextStyle,
    s: &str,
    max_chars: usize,
) {
    let (r, g, b) = style.color;
    font.set_color_mod(r, g, b);
    let cell = (GLYPH_W * style.scale) as i32;
    let mut pen = x;
    for ch in s.chars().take(max_chars) {
        let idx = glyph_index(ch);
        if ch != ' ' {
            let src = Rect::new(idx as i32 * GLYPH_W as i32, 0, GLYPH_W, GLYPH_H);
            let strikes = if style.bold { 2 } else { 1 };
            for strike in 0..strikes {
                let dst = Rect::new(
                    pen + strike * style.scale as i32,
                    y,
                    GLYPH_W * style.scale,
                    GLYPH_H * style.scale,
                );
                let _ = canvas.copy(font, src, dst);
            }
        }
        pen += cell;
    }
}

/// Word-wrapped text at absolute window coordinates, mirroring
/// `Screen::text_wrapped`'s layout. Returns the y past the last line.
pub(super) fn draw_text_wrapped_absolute(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    x: i32,
    y: i32,
    max_w: u32,
    style: TextStyle,
    s: &str,
) -> i32 {
    let cols = (max_w / (GLYPH_W * style.scale)).max(1) as usize;
    let row = (GLYPH_H * style.scale) as i32;
    let mut line = String::new();
    let mut line_len = 0usize;
    let mut cy = y;
    for word in s.split_whitespace() {
        let word_len = word.chars().count();
        if !line.is_empty() && line_len + 1 + word_len > cols {
            draw_text_absolute(canvas, font, x, cy, style, &line, usize::MAX);
            cy += row + 2;
            line.clear();
            line_len = 0;
        }
        if !line.is_empty() {
            line.push(' ');
            line_len += 1;
        }
        line.push_str(word);
        line_len += word_len;
        // `.chars()` (not `split_at`, byte-indexed): a wrap point that fell
        // mid-character would panic on any accented/multi-byte word.
        while line_len > cols {
            let head: String = line.chars().take(cols).collect();
            let tail: String = line.chars().skip(cols).collect();
            draw_text_absolute(canvas, font, x, cy, style, &head, usize::MAX);
            cy += row + 2;
            line = tail;
            line_len = line.chars().count();
        }
    }
    if !line.is_empty() {
        draw_text_absolute(canvas, font, x, cy, style, &line, usize::MAX);
        cy += row + 2;
    }
    cy
}

/// Line-count math shared by [`Cabinet::text_wrapped`]'s layout and
/// [`Cabinet::wrapped_height`]. Mirrors the wrap loop: greedy word packing into
/// `cols` chars, hard-splitting any word longer than a line.
pub(super) fn wrapped_height(max_w: u32, scale: u32, s: &str) -> i32 {
    let cols = (max_w / (GLYPH_W * scale)).max(1) as usize;
    let row = (GLYPH_H * scale) as i32;
    let mut lines = 0i32;
    let mut len = 0usize;
    let mut open = false;
    for word in s.split_whitespace() {
        let word_len = word.chars().count();
        if open && len + 1 + word_len > cols {
            lines += 1;
            len = 0;
            open = false;
        }
        if open {
            len += 1;
        }
        len += word_len;
        open = true;
        while len > cols {
            lines += 1;
            len -= cols;
        }
    }
    if open {
        lines += 1;
    }
    lines * (row + 2)
}

/// Rasterize the covered Unicode range once into one wide texture strip —
/// each cell holds one anti-aliased glyph (alpha = the crate's per-pixel
/// intensity, RGB left white so every draw call tints it via
/// `set_color_mod`), left-aligned in its `GLYPH_W`-wide cell exactly like
/// the atlas the old bitmap font used, just with grayscale edges instead of
/// hard 1-bit ones.
pub(super) fn build_font_atlas(canvas: &mut WindowCanvas) -> Result<Texture, PlatformError> {
    let w = GLYPH_COLS * GLYPH_W;
    let h = GLYPH_H;
    let mut rgba = vec![0u8; (w * h * 4) as usize];
    for i in 0..GLYPH_COLS {
        let Some(ch) = char::from_u32(GLYPH_FIRST + i) else {
            continue;
        };
        let Some(glyph) = get_raster(ch, FONT_WEIGHT, FONT_HEIGHT) else {
            continue;
        };
        for (row, line) in glyph.raster().iter().enumerate().take(h as usize) {
            for (col, &v) in line.iter().enumerate().take(GLYPH_W as usize) {
                if v == 0 {
                    continue;
                }
                let px = i * GLYPH_W + col as u32;
                let o = ((row as u32 * w + px) * 4) as usize;
                rgba[o..o + 4].copy_from_slice(&[255, 255, 255, v]);
            }
        }
    }
    let mut tex = canvas
        .create_texture_static(SdlFormat::RGBA32, w, h)
        .map_err(|e| PlatformError::Sdl(e.to_string()))?;
    tex.update(None, &rgba, (w * 4) as usize)
        .map_err(|e| PlatformError::Sdl(e.to_string()))?;
    tex.set_blend_mode(BlendMode::Blend);
    tex.set_scale_mode(SdlScaleMode::Nearest);
    Ok(tex)
}
