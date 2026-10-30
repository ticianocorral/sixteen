//! O queixo da TV: badge do RetroAchievements, OSD de notificações e a
//! placa de versão com as setas de update.

use super::*;

/// The badge's two rows and their colors — pure so the tests can pin the
/// contract: the "RA enabled" row always in the OSD green, the mode in amber
/// when hardcore and in the nameplate grey when softcore.
pub(super) fn ra_badge_rows(hardcore: bool) -> [(&'static str, (u8, u8, u8)); 2] {
    [
        (tr!("RA ENABLED", "RA ATIVADO"), OSD_GREEN),
        (
            if hardcore { "HARDCORE" } else { "SOFTCORE" },
            if hardcore {
                (240, 180, 60)
            } else {
                (235, 235, 225)
            },
        ),
    ]
}

/// The persistent RetroAchievements badge — the same chin corner as the
/// unlock block, shown whenever that block isn't on screen: the RA logo
/// (the registered favicon; falls back to the pixel gamepad tile) left of
/// two rows, "RA ATIVADO" in the OSD green and the mode below (amber for
/// hardcore, the nameplate grey for softcore). Same 2px dark shadow as
/// everything else on the glass.
pub(super) fn draw_chin_ra(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    screen: Rect,
    _canvas_w: i32,
    nameplate: &str,
    hardcore: bool,
) {
    const MARGIN: i32 = 24;
    const TILE: i32 = 32;
    const TILE_BG: (u8, u8, u8) = (18, 22, 52);
    const TILE_EDGE: (u8, u8, u8) = (240, 180, 60);
    const GOLD: (u8, u8, u8) = (240, 190, 70);
    // A moldura superior, canto DIREITO — a mesma posição do par
    // fechar/minimizar, espelhada (plan revision: "os dados de RA no mesmo
    // posicionamento do botão de fechar, só que do lado direito").
    let strip_h = screen.y();
    if strip_h < 32 {
        return;
    }
    const TILE_Y: i32 = 14;
    let cell = GLYPH_W as i32;
    /// A copy no respiro do emblema: 10px entre a cauda da tinta e o tile
    /// (plan revision: "afastar um pouco a logo das copys") — a sombra 2px
    /// da copy conta 8px de ar visível.
    const COPY_GAP: i32 = 10;
    let rows = ra_badge_rows(hardcore);
    let nameplate_w = nameplate
        .lines()
        .map(|l| l.chars().count() as i32 * cell)
        .max()
        .unwrap_or(0);
    // A simetria REAL considerando o painel lateral (plan revision): o X
    // mora a 14px do canto esquerdo do CANVAS — o começo de tudo. O
    // espelho do lado direito é contra o fim do território da TV: a borda
    // da moldura lateral direita, que fica a `screen.left()` (o BEZEL_SIDE,
    // simétrico) do fim do vidro. O bloco do RA encosta a 14px dali —
    // mesma margem, outro canto, e nunca no painel.
    let right = screen.right() + screen.left() - 14;
    let tile_x = right - TILE;
    let tile_y = TILE_Y;
    let left_limit = screen.left() + nameplate_w + MARGIN * 2 + TILE + COPY_GAP;
    let max_cols = ((right - left_limit).max(cell) / cell) as usize;
    let shown: Vec<(String, (u8, u8, u8))> = rows
        .iter()
        .map(|(l, c)| (truncate_to_cols(l, max_cols), *c))
        .collect();
    // A copy como colar do emblema: as duas linhas com passo 14 — a tinta
    // das maiúsculas (cap +4..+14 na célula de 20px, que estas linhas não
    // têm descendente) fica 4px de vão de tinta entre as linhas e o bloco
    // inteiro centrado NO EIXO do emblema (datum 30 do par
    // fechar/minimizar). Antes: passo 17, vão de tinta 7px, tinta colada.
    // Tinta: "RA ATIVADO" 18..28, modo 32..42 — copy inset 4px dos tetos
    // do tile (14..46), a logo mais alta que a copy, sem colisão.
    const LINE_PITCH: i32 = 14;
    let mut y = tile_y;

    // The mark: the official RA favicon (registered by the app) when it's
    // there; the pixel gamepad tile is the fallback so the badge never
    // depends on the image having been registered.
    if images.contains_key(&RA_LOGO_IMG) {
        draw_image_absolute(
            canvas,
            images,
            RA_LOGO_IMG,
            tile_x,
            tile_y,
            TILE as u32,
            TILE as u32,
        );
    } else {
        canvas.set_draw_color(Color::RGB(TILE_EDGE.0, TILE_EDGE.1, TILE_EDGE.2));
        let _ = canvas.fill_rect(Rect::new(tile_x, tile_y, TILE as u32, TILE as u32));
        canvas.set_draw_color(Color::RGB(TILE_BG.0, TILE_BG.1, TILE_BG.2));
        let _ = canvas.fill_rect(Rect::new(
            tile_x + 2,
            tile_y + 2,
            (TILE - 4) as u32,
            (TILE - 4) as u32,
        ));
        const PAD: [&str; 7] = [
            "..............",
            "##############",
            "#..#......o.o#",
            "#.###........#",
            "#..#.........#",
            "##############",
            "..............",
        ];
        let s = 2;
        let px = tile_x + (TILE - 14 * s) / 2;
        let py = tile_y + (TILE - 7 * s) / 2;
        canvas.set_draw_color(Color::RGB(GOLD.0, GOLD.1, GOLD.2));
        for (gy, line) in PAD.iter().enumerate() {
            for (gx, ch) in line.chars().enumerate() {
                if ch == '.' {
                    continue;
                }
                let _ = canvas.fill_rect(Rect::new(
                    px + gx as i32 * s,
                    py + gy as i32 * s,
                    s as u32,
                    s as u32,
                ));
            }
        }
    }
    for (line, color) in &shown {
        let x = tile_x - COPY_GAP - line.chars().count() as i32 * cell;
        draw_text_absolute(
            canvas,
            font,
            x + 2,
            y + 2,
            TextStyle::new(1, (12, 14, 12)),
            line,
            usize::MAX,
        );
        draw_text_absolute(
            canvas,
            font,
            x,
            y,
            TextStyle::new(1, *color),
            line,
            usize::MAX,
        );
        y += LINE_PITCH;
    }
}

/// The achievements notification (plan fase 4): a right-aligned block in
/// the top strip. Same grammar as the persistent RA badge: the header in
/// the OSD green ("CONQUISTA DESBLOQUEADA"), then the remaining lines
/// merged into ONE tight row — the name (first) in the near-white LIGHT,
/// the trailing metas (pontos, modo) in the nameplate's own grey, joined
/// by dim "·" separators — so everything the block carries fits the strip.
/// Both rows use the badge's 14px pitch, ink centred on the strip datum
/// (button pair axis, y=30), next to the 40px disc with the badge's own
/// 10px copy gap. All flat on
/// the glass with the same 2px dark shadow as the CH 3 banner. Lines
/// longer than the space between the nameplate and the disc truncate with
/// "..." in the name so the two blocks never touch. `badge` is the image
/// id registered via `set_image`, drawn as the corner disc.
#[allow(clippy::too_many_arguments)] // canvas/font/images + 4 layout facts
pub(super) fn draw_chin_osd(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    screen: Rect,
    _out_h: u32,
    nameplate: &str,
    lines: &[String],
    badge: Option<u64>,
) {
    const DISC: u32 = 40;
    const MARGIN: i32 = 24;
    /// Mesmo ritmo do badge do RA: primeira linha desenhada no teto do
    /// emblema (tinta 18..28) e passo 14 — vão de tinta 4px entre as
    /// linhas, bloco centrado no eixo 30 junto ao disco.
    const FIRST_Y: i32 = 14;
    const PITCH: i32 = 14;
    /// A copy no respiro do emblema (plan revision: mêmes 10px de folga do
    /// badge fixo entre a cauda da tinta e o disco).
    const COPY_GAP: i32 = 10;
    const LIGHT: (u8, u8, u8) = (235, 235, 225);
    const SHADOW: (u8, u8, u8) = (12, 14, 12);
    // A moldura superior, como o badge do RA — o queixo é dos controles.
    let chin_h = screen.y();
    if chin_h < 24 || lines.is_empty() {
        return;
    }
    let cell = GLYPH_W as i32;
    let has_badge = badge.is_some_and(|id| images.contains_key(&id));
    // The nameplate's widest line sets this block's left limit, so a long
    // achievement name truncates instead of colliding with it.
    let nameplate_w = nameplate
        .lines()
        .map(|l| l.chars().count() as i32 * cell)
        .max()
        .unwrap_or(0);
    // A mesma âncora de canto do badge do RA (a moldura lateral direita,
    // 14px no espelho): o disco ocupa a coluna do emblema e a copy troca de
    // estado sem saltar de lugar na tela.
    let right = screen.right() + screen.left() - 14;
    let disc_x = right - DISC as i32;
    let disc_y = 30 - DISC as i32 / 2;
    // Mesma folga copy<->emblema do badge fixo: 10px (plan revision:
    // "afastar um pouco a logo das copys"). Sem disco, a copy encosta no
    // canto com a margem 14 do par fechar/minimizar.
    let row_right = right
        - if has_badge {
            DISC as i32 + COPY_GAP
        } else {
            14
        };
    let left_limit = screen.left() + nameplate_w + MARGIN * 2 + DISC as i32 + COPY_GAP;
    let max_cols = ((row_right - left_limit).max(cell) / cell) as usize;
    let header = truncate_to_cols(&lines[0], max_cols);
    // O corpo numa linha só: o primeiro segmento (o nome) em LIGHT, os
    // seguintes (pontos, modo) no cinza do nameplate, unidos por " · " na
    // mesma cor da cauda.
    let mut body: Vec<(String, (u8, u8, u8))> = Vec::new();
    for (i, line) in lines[1..].iter().enumerate() {
        if i > 0 {
            body.push((" · ".to_string(), BRAND_TEXT));
        }
        if i == 0 {
            // A cauda (separadores + pontos + modo) é fixa; a truncagem
            // "..." mora só no nome, que é o que dá leitura ao bloco.
            let tail_cols: usize =
                (lines.len() - 2) * 3 + lines[2..].iter().map(|l| l.chars().count()).sum::<usize>();
            let room = max_cols.saturating_sub(tail_cols);
            body.push((truncate_to_cols(line, room), LIGHT));
        } else {
            body.push((line.clone(), BRAND_TEXT));
        }
    }
    if has_badge {
        // Centrado no eixo 30 — o mesmo datum do badge fixo e do par
        // fechar/minimizar; a troca badge<->notificação não salta.
        draw_image_absolute(
            canvas,
            images,
            badge.unwrap_or(0),
            disc_x,
            disc_y,
            DISC,
            DISC,
        );
    }
    let draw_row = |canvas: &mut WindowCanvas,
                    font: &mut Texture,
                    y: i32,
                    row_right: i32,
                    segments: &[(String, (u8, u8, u8))]| {
        let total: i32 = segments
            .iter()
            .map(|(s, _)| s.chars().count() as i32 * cell)
            .sum();
        let mut pen = row_right - total;
        for (seg, color) in segments {
            draw_text_absolute(
                canvas,
                font,
                pen + 2,
                y + 2,
                TextStyle::new(1, SHADOW),
                seg,
                usize::MAX,
            );
            draw_text_absolute(
                canvas,
                font,
                pen,
                y,
                TextStyle::new(1, *color),
                seg,
                usize::MAX,
            );
            pen += seg.chars().count() as i32 * cell;
        }
    };
    draw_row(canvas, font, FIRST_Y, row_right, &[(header, OSD_GREEN)]);
    draw_row(canvas, font, FIRST_Y + PITCH, row_right, &body);
}

pub(super) fn truncate_to_cols(s: &str, max_cols: usize) -> String {
    if s.chars().count() <= max_cols {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_cols.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

/// `(rect da seta do app, rect da seta do core)` — `None` quando a linha
/// não tem seta neste frame. Saída de [`draw_brand`] para o hit-test do
/// idle (plan revision: "ao clicar no icone verde de atualizar").
pub(super) type UpdateArrowRects = (Option<Rect>, Option<Rect>);

pub(super) fn draw_brand(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    screen: Rect,
    out_h: u32,
    label: &str,
    updates: (bool, bool),
) -> UpdateArrowRects {
    let mut arrows: UpdateArrowRects = (None, None);
    let chin_top = screen.bottom();
    let chin_h = out_h as i32 - chin_top;
    if chin_h < 24 {
        return arrows;
    }
    // A '\n' in the label stacks lines (plan revision: "no nameplate colocar
    // a versão do snes9x abaixo do sixteen") — the block centred in
    // the chin. When the chin can't fit them all, keep only the first (the
    // app's own name/version) rather than spilling over the bezel.
    let lines: Vec<&str> = label.split('\n').collect();
    let row = GLYPH_H as i32;
    let gap = 4i32;
    let fits = chin_h >= (lines.len() as i32 * row + (lines.len() as i32 - 1) * gap).max(row);
    let shown = if fits { &lines[..] } else { &lines[..1] };
    // Revision: a MARCA (a arte prateada embutida) mora centralizada no
    // queixo — as linhas pequenas da esquerda viram só a informação de
    // versão (o nome sai delas). O desenho em si é feito pelo caller,
    // que tem o mapa de imagens (ver draw_brand_logo).
    let block_h = shown.len() as i32 * row + (shown.len() as i32 - 1) * gap;
    let mut y = chin_top + (chin_h - block_h) / 2;
    for (i, line) in shown.iter().enumerate() {
        let cleaned: String = line.replace("SixteeN", "").trim_start().to_string();
        draw_text_absolute(
            canvas,
            font,
            screen.left(),
            y,
            TextStyle::new(1, BRAND_TEXT),
            &cleaned,
            usize::MAX,
        );
        // The "tem update" arrow (plan revision: "um icone verde no
        // nameplate ... uma seta verde pra cima com update") — right after
        // the line's own text, vertically centred on it. Line 0 is the
        // app's version, line 1 the core's.
        let wants_arrow = i == 0 && updates.0 || i == 1 && updates.1;
        if wants_arrow {
            let ax = screen.left() + cleaned.chars().count() as i32 * GLYPH_W as i32 + 10;
            draw_update_arrow(canvas, ax, y + row / 2);
            // Área de clique generosa (o ícone tem 11×12 px) — uma faixa
            // da altura da linha, um pouco mais larga que a seta.
            let hit = Rect::new(ax - 4, y, 19u32, row as u32);
            if i == 0 {
                arrows.0 = Some(hit);
            } else {
                arrows.1 = Some(hit);
            }
        }
        y += row + gap;
    }
    arrows
}

/// The nameplate's "tem update" marker (plan revision: "uma seta verde pra
/// cima com update") — a chunky pixel arrow pointing up, in the same green
/// as the CH 3 banner / OSD header, with the furniture's usual 1px dark
/// drop shadow. 11×12 px: a stepped triangle head (1-3-5-7-9 px rows,
/// matching the bitmap font's chunkiness) over a 3px stem.
pub(super) fn draw_update_arrow(canvas: &mut WindowCanvas, x: i32, y_center: i32) {
    const HEAD_ROWS: [i32; 5] = [1, 3, 5, 7, 9];
    const STEM_W: i32 = 3;
    const STEM_H: i32 = 3;
    let total_h = HEAD_ROWS.len() as i32 + STEM_H;
    let top = y_center - total_h / 2;
    let cx = x + 5; // centre line of the widest head row
    let mut draw = |color: Color, ox: i32, oy: i32| {
        canvas.set_draw_color(color);
        for (r, &w) in HEAD_ROWS.iter().enumerate() {
            let _ = canvas.fill_rect(Rect::new(cx - w / 2 + ox, top + r as i32 + oy, w as u32, 1));
        }
        let _ = canvas.fill_rect(Rect::new(
            cx - STEM_W / 2 + ox,
            top + HEAD_ROWS.len() as i32 + oy,
            STEM_W as u32,
            STEM_H as u32,
        ));
    };
    draw(Color::RGB(12, 14, 12), 1, 1); // shadow
    draw(Color::RGB(OSD_GREEN.0, OSD_GREEN.1, OSD_GREEN.2), 0, 0);
}
