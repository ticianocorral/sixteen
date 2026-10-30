//! Arte do painel lateral: imagens absolutas e o slot do cartucho.

use super::*;

/// Like `Screen::image_fit`, but at absolute window coordinates instead of
/// offset into the 2D screen buffer — for cabinet furniture and panel art.
pub(super) fn draw_image_absolute(
    canvas: &mut WindowCanvas,
    images: &HashMap<u64, ImgTex>,
    id: u64,
    x: i32,
    y: i32,
    bw: u32,
    bh: u32,
) {
    let Some(img) = images.get(&id) else {
        return;
    };
    let (iwf, ihf) = (img.w as f32, img.h as f32);
    let scale = (bw as f32 / iwf).min(bh as f32 / ihf);
    let dw = (iwf * scale).round() as i32;
    let dh = (ihf * scale).round() as i32;
    let dx = x + (bw as i32 - dw) / 2;
    let dy = y + (bh as i32 - dh) / 2;
    let _ = canvas.copy(
        &img.tex,
        None,
        Rect::new(dx, dy, dw.max(1) as u32, dh.max(1) as u32),
    );
}

/// The panel's cartridge slot, in panel-local pixels: the in-game panel's
/// cartridge block drawn as the console seen from the front — the loading
/// base as a solid slab across the block's bottom (`panel_slot_base`) with
/// the dark slot mouth across its top edge (`panel_slot_mouth`) — so the
/// game's cartridge art stands visibly *plugged into* the console at rest,
/// and `runner`'s insert/eject animation plays right there (plan revision:
/// the reference photo — a cartridge standing upright, seated in the
/// console's base). Pure geometry, so the motion math in
/// `panel_cartridge_rects` stays unit-testable without a canvas.
///
/// The cartridge enters the frame from above and sinks into the mouth,
/// which occludes everything past its top edge — the clip trick that keeps
/// the seated cartridge's base hidden inside the console.
pub(super) fn panel_slot_base(block: Rect) -> Rect {
    let bh = (block.height() as f32 * 0.40).round() as u32;
    Rect::new(
        block.x(),
        block.bottom() - bh.max(1) as i32,
        block.width(),
        bh.max(1),
    )
}

/// The slot opening: a dark band across the base's top edge, near the base's
/// full width — where the cartridge enters and below which it is never
/// drawn. Its top edge is the clip line the cartridge art is drawn against.
pub(super) fn panel_slot_mouth(block: Rect, base: Rect) -> Rect {
    Rect::new(
        block.x() + 4,
        base.y() + 2,
        block.width().saturating_sub(8).max(1),
        8.min(base.height().saturating_sub(2)).max(1),
    )
}

/// Where the cartridge sprite is at progress `t` (0.0..=1.0) of the insert
/// — or, with `ejecting`, the mirrored eject — inside the panel's cartridge
/// block. `iw, ih` is the art texture's own size and `content` its opaque
/// bounding box (`ImgTex::content`): the fit and the seating key off the
/// *content*, not the canvas, so the cart's actual body spans the base's
/// full width and its visible base sits right at the slot mouth — no float
/// gap from transparent margins (plan revision: the user's annotated
/// screenshot). Returns the full-texture destination rect plus the clip
/// rect the caller must draw it under (the block's area above the mouth's
/// top edge, so whatever has passed the lip is hidden). At `t = 1.0`
/// (insert done, or eject at 0.0) the cart is seated: content base just
/// past the mouth's lip, label standing proud of the console.
pub(super) fn panel_cartridge_rects(
    t: f32,
    ejecting: bool,
    block: Rect,
    mouth: Rect,
    iw: u32,
    ih: u32,
    content: (u32, u32, u32, u32),
) -> (Rect, Rect) {
    let t = t.clamp(0.0, 1.0);
    // Insert eases in with a smoothstep (gentle start, slides home, settles);
    // eject mirrors it as an out-quad pop — quick off the seat, slowing as it
    // rises clear.
    let e = if ejecting {
        1.0 - (1.0 - t) * (1.0 - t)
    } else {
        t * t * (3.0 - 2.0 * t)
    };
    // 0.0 = entirely above the block, 1.0 = seated in the slot.
    let p = if ejecting { 1.0 - e } else { e };
    let (cx, cy, cw, ch) = content;
    let (cw, ch) = (cw.max(1), ch.max(1));
    // Fit the cart's body to the annotated width — a shade inside the base's
    // full span — bounded by the standing space: seated, a whole
    // `SEAT_HIDDEN_FRAC` of the body stays below the lip (hidden inside the
    // console), so the content's top must keep a 2px margin inside the
    // block: ch*scale*(1-frac) ≤ top_room-2.
    let top_room = (mouth.y() - block.y()).max(1) as f32;
    let scale = (block.width() as f32 * CART_WIDTH_FRAC / cw as f32)
        .min((top_room - 2.0) / (ch as f32 * (1.0 - SEAT_HIDDEN_FRAC)));
    let scale = scale.max(0.01);
    let w = ((iw as f32) * scale).round().max(1.0) as u32;
    let h = ((ih as f32) * scale).round().max(1.0) as u32;
    // Centre the *content* on the block — the transparent margins may be
    // asymmetric, so the canvas rect itself can sit off-centre.
    let x = block.x() + block.width() as i32 / 2
        - ((cx as f32 + cw as f32 / 2.0) * scale).round() as i32;
    let content_bottom_in_dst = (cy as f32 + ch as f32) * scale;
    // Seated: the cart's base reaches `hidden` pixels past the lip — the
    // clip trims everything below the mouth's edge, so that much of the
    // body is visibly inside the console. Enter: the content fully above
    // the block, the slot empty.
    let hidden = SEAT_HIDDEN_FRAC * ch as f32 * scale;
    let seated_top = mouth.y() as f32 + hidden - content_bottom_in_dst;
    let enter_top = block.y() as f32 - content_bottom_in_dst;
    let top = enter_top + (seated_top - enter_top) * p;
    let dst = Rect::new(x, top.round() as i32, w, h);
    let clip = Rect::new(
        block.x(),
        block.y(),
        block.width(),
        (mouth.y() - block.y()).max(1) as u32,
    );
    (dst, clip)
}

/// Draw the slot's console furniture into `block`: the loading base as a
/// light-grey slab across the block's bottom (a darker grounding line along
/// its bottom edge), the bezel ring and near-black mouth across the base's
/// top edge, and — on the base face, left-aligned — the console tag
/// wordmark when one is loaded (`SLOT_TAG_IMG`, plan revision: "imagem
/// console-tag... na base do cartucho, alinhado a esquerda"), a bare
/// dust-shield groove otherwise. Used by the game panel's seated cartridge
/// (`draw_panel_slot`) — o painel idle não tem mais console (plan revision:
/// "remover o console da tela inicial"). Returns the mouth rect — the lip
/// line where a cartridge crosses into the console.
pub(super) fn draw_slot_furniture(
    canvas: &mut WindowCanvas,
    images: &HashMap<u64, ImgTex>,
    block: Rect,
) -> Rect {
    let fill = |canvas: &mut WindowCanvas, color: (u8, u8, u8), r: Rect| {
        canvas.set_draw_color(Color::RGB(color.0, color.1, color.2));
        let _ = canvas.fill_rect(r);
    };
    let base = panel_slot_base(block);
    let mouth = panel_slot_mouth(block, base);

    fill(canvas, SLOT_SHELL, base);
    fill(
        canvas,
        SLOT_SHELL_EDGE,
        Rect::new(
            base.x(),
            base.bottom() - 3,
            base.width(),
            3.min(base.height()).max(1),
        ),
    );

    let bezel = Rect::new(
        mouth.x() - 3,
        mouth.y() - 3,
        mouth.width() + 6,
        mouth.height() + 6,
    );
    fill(canvas, SLOT_BEZEL, bezel);
    fill(canvas, SLOT_MOUTH, mouth);

    // The free strip of base face between the opening and the grounding
    // edge — the tag's home when one is loaded, the bare groove otherwise.
    let zone_top = mouth.bottom() + 8;
    let zone_bottom = base.bottom() - 6;
    let zone_h = (zone_bottom - zone_top).max(1) as f32;
    if let Some(tag) = images.get(&SLOT_TAG_IMG) {
        // Fit the wordmark's opaque body into the strip, left-aligned on the
        // base with a 10px margin, capped so it stays a badge — not a
        // billboard — even on wide panels.
        let (tcx, tcy, tcw, tch) = tag.content;
        let (tcw, tch) = (tcw.max(1), tch.max(1));
        let max_h = zone_h.min((base.width() as f32 * 0.16).min(44.0));
        let scale = (max_h / tch as f32).min(((base.width() - 20).max(1)) as f32 / tcw as f32);
        let tw = (tcw as f32 * scale).round().max(1.0) as u32;
        let th = (tch as f32 * scale).round().max(1.0) as u32;
        let src = Rect::new(tcx as i32, tcy as i32, tcw.min(tag.w), tch.min(tag.h));
        let dst = Rect::new(
            base.x() + 10,
            zone_top + ((zone_h - th as f32) / 2.0).round() as i32,
            tw,
            th,
        );
        let _ = canvas.copy(&tag.tex, src, dst);
    } else {
        fill(
            canvas,
            SLOT_RIDGE,
            Rect::new(
                base.x() + 10,
                zone_top + 6,
                base.width().saturating_sub(20),
                3,
            ),
        );
    }
    mouth
}

/// Draw the panel's cartridge block as the console seen from the front,
/// with the game's cartridge art standing in its slot (plan revision —
/// the reference photo: cartridge upright, plugged into the loading base,
/// the base keeping the light-grey plastic colours this panel already
/// used). `t`/`ejecting` come from `PanelInfo::cartridge_motion` (`None`
/// outside the animation means seated, i.e. `t = 1.0`): the cartridge
/// drops into the mouth on insert and back up out of it on eject, right
/// where the cartridge always lives during gameplay. Requires
/// `PANEL_CARTRIDGE_IMG` to be loaded — `draw_panel` only calls this under
/// `has_cartridge`.
pub(super) fn draw_panel_slot(
    canvas: &mut WindowCanvas,
    images: &HashMap<u64, ImgTex>,
    block: Rect,
    t: f32,
    ejecting: bool,
) {
    let mouth = draw_slot_furniture(canvas, images, block);

    let Some(art) = images.get(&PANEL_CARTRIDGE_IMG) else {
        return;
    };
    let (dst, clip) = panel_cartridge_rects(t, ejecting, block, mouth, art.w, art.h, art.content);
    // Everything below the mouth's top edge is "inside the console" — the
    // clip seats the cartridge into the slot instead of drawing over it.
    // Reset right after, like every other shared-state user here.
    canvas.set_clip_rect(Some(clip));
    let _ = canvas.copy(&art.tex, None, dst);
    canvas.set_clip_rect(None);
}
