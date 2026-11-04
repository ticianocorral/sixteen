//! O livro de pausa e os modais (salvar, carregar, print, cheats).

use super::*;

/// Where the pause book's two pages sit: a symmetric spread with a spine gap
/// between them, margins all round. Not tied to the tube's geometry — this
/// screen replaces the whole window (plan §3.2/§3.4).
pub(super) fn pause_pages(out_w: u32, out_h: u32) -> (Rect, Rect) {
    let margin = (out_w as f32 * PAUSE_MARGIN) as i32;
    let gap = (out_w as f32 * PAUSE_GAP) as i32;
    let top = (out_h as f32 * PAUSE_TOP) as i32;
    let page_w = ((out_w as i32 - 2 * margin - gap) / 2).max(32) as u32;
    let page_h = (out_h as i32 - 2 * top).max(32) as u32;
    let left = Rect::new(margin, top, page_w, page_h);
    let right = Rect::new(margin + page_w as i32 + gap, top, page_w, page_h);
    (left, right)
}

/// Draw the pause book: left page is the notebook's status (how many pages
/// captured so far, or a hint if there are none), right page is the most
/// recent capture, full-size, plus two clickable buttons below the pages —
/// "Continuar" (resume) and "Avancar quadro" (step one frame, plan revision:
/// mouse/gamepad only, no keyboard). `None` (shouldn't happen — always set
/// before `present_pause`/`capture_pause_bmp` are called) draws nothing and
/// returns no buttons.
pub(super) fn draw_pause_book(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    pause: Option<&PauseNote>,
    out_w: u32,
    out_h: u32,
) -> Vec<(PanelButton, Rect)> {
    canvas.set_draw_color(Color::RGB(CABINET.0, CABINET.1, CABINET.2));
    let _ = canvas.fill_rect(Rect::new(0, 0, out_w, out_h));
    let Some(pause) = pause else {
        return Vec::new();
    };

    let (left, right) = pause_pages(out_w, out_h);
    for page in [left, right] {
        canvas.set_draw_color(Color::RGB(PANEL_BG.0, PANEL_BG.1, PANEL_BG.2));
        let _ = canvas.fill_rect(page);
    }

    let pad = 24i32;
    let lx = left.x() + pad;
    let ly = left.y() + pad;
    let lw = left.width().saturating_sub(pad as u32 * 2);
    let btn_h = (GLYPH_H + 10) as i32;
    let mut buttons = Vec::new();

    let mut cy = draw_text_wrapped_absolute(
        canvas,
        font,
        lx,
        ly,
        lw,
        TextStyle::new(2, PANEL_TEXT),
        &pause.title,
    );
    cy += 16;

    if let Some(draft) = &pause.draft {
        // Editing mode (plan revision: either the free-text note or a
        // slot's caption, `draft_heading` says which): the rest of the left
        // page becomes a live text editor — Continuar/Avancar quadro (below
        // both pages) and the cheats list are hidden until the draft is
        // saved/cancelled, so there's nothing to accidentally lose by
        // clicking.
        draw_text_absolute(
            canvas,
            font,
            lx,
            cy,
            TextStyle::new(1, PANEL_DIM),
            &pause.draft_heading,
            usize::MAX,
        );
        cy += GLYPH_H as i32 + 8;
        cy = draw_text_wrapped_absolute(
            canvas,
            font,
            lx,
            cy,
            lw,
            TextStyle::new(1, PANEL_TEXT),
            &format!("{draft}_"),
        );
        cy += 8;
        draw_text_absolute(
            canvas,
            font,
            lx,
            cy,
            TextStyle::new(1, PANEL_DIM),
            &format!("{}/{}", draft.chars().count(), pause.draft_limit),
            usize::MAX,
        );

        let cancel_y = left.bottom() - pad - btn_h;
        let save_y = cancel_y - btn_h - 6;
        let save_btn = Rect::new(lx, save_y, lw, btn_h as u32);
        let cancel_btn = Rect::new(lx, cancel_y, lw, btn_h as u32);
        buttons.push((
            PanelButton::PauseDraftSave,
            draw_button(canvas, font, save_btn, tr!("Save", "Salvar"), true),
        ));
        buttons.push((
            PanelButton::PauseDraftCancel,
            draw_button(canvas, font, cancel_btn, tr!("Cancel", "Cancelar"), true),
        ));
    } else {
        // Left page: a text-note slot viewer (plan revision — mirrors the
        // right page's photo album layout almost exactly, bottom-up: pin/
        // delete row, write row, nav row, then the info line, then
        // whatever's left for the content itself).
        draw_text_absolute(
            canvas,
            font,
            lx,
            cy,
            TextStyle::new(1, PANEL_DIM),
            tr!("notes", "anotações"),
            usize::MAX,
        );
        cy += GLYPH_H as i32 + 8;

        let pin_del_y = left.bottom() - pad - btn_h;
        let write_y = pin_del_y - 6 - btn_h;
        let nav_y = write_y - 6 - btn_h;
        let info_y = nav_y - 6 - (GLYPH_H as i32 + 6);

        match &pause.text_content {
            Some(text) => {
                draw_text_wrapped_absolute(
                    canvas,
                    font,
                    lx,
                    cy,
                    lw,
                    TextStyle::new(1, PANEL_TEXT),
                    text,
                );
            }
            None => {
                draw_text_absolute(
                    canvas,
                    font,
                    lx,
                    cy,
                    TextStyle::new(1, PANEL_DIM),
                    tr!("empty slot", "slot vazio"),
                    usize::MAX,
                );
            }
        }

        let info = format!(
            "{}/{}{}",
            pause.text_page + 1,
            pause.captures,
            if pause.text_pinned {
                tr!(" (pinned)", " (fixado)")
            } else {
                ""
            }
        );
        draw_text_absolute(
            canvas,
            font,
            lx,
            info_y,
            TextStyle::new(1, PANEL_DIM),
            &info,
            usize::MAX,
        );

        let half = (lw / 2).saturating_sub(4);
        let prev_btn = Rect::new(lx, nav_y, half, btn_h as u32);
        let next_btn = Rect::new(lx + half as i32 + 8, nav_y, half, btn_h as u32);
        buttons.push((
            PanelButton::PauseTextPrev,
            draw_button(
                canvas,
                font,
                prev_btn,
                tr!("< previous", "< anterior"),
                pause.text_page > 0,
            ),
        ));
        buttons.push((
            PanelButton::PauseTextNext,
            draw_button(
                canvas,
                font,
                next_btn,
                tr!("next >", "próxima >"),
                pause.text_page + 1 < pause.captures,
            ),
        ));

        let write_btn = Rect::new(lx, write_y, lw, btn_h as u32);
        buttons.push((
            PanelButton::PauseWrite,
            draw_button(
                canvas,
                font,
                write_btn,
                if pause.text_content.is_some() {
                    tr!("Edit", "Editar")
                } else {
                    tr!("Write", "Escrever")
                },
                !pause.text_pinned,
            ),
        ));

        let pin_btn = Rect::new(lx, pin_del_y, half, btn_h as u32);
        let del_btn = Rect::new(lx + half as i32 + 8, pin_del_y, half, btn_h as u32);
        // Fixar só em telas maiores que 800p (o painel lateral não comporta
        // a nota fixada em 720p/800p); desafixar sempre é permitido.
        let can_pin = out_h > 800 || pause.text_pinned;
        let text_pin = draw_button(
            canvas,
            font,
            pin_btn,
            if pause.text_pinned {
                tr!("Pinned", "Fixado")
            } else {
                tr!("Pin", "Fixar")
            },
            can_pin,
        );
        if can_pin {
            buttons.push((PanelButton::PauseTextPin, text_pin));
        }
        buttons.push((
            PanelButton::PauseTextDelete,
            draw_button(
                canvas,
                font,
                del_btn,
                tr!("Delete", "Apagar"),
                pause.text_content.is_some() && !pause.text_pinned,
            ),
        ));
    }

    // Right page: the selected slot's image, a pin/caption row, and
    // pagination through all 15 (plan revision — used to always show just
    // the latest, no pinning or naming).
    let rx = right.x() + pad;
    let ry = right.y() + pad;
    let rw = right.width().saturating_sub(pad as u32 * 2);
    // Bottom-up: pin/rename row, then Prev/Next, then the info line(s) —
    // always reserved (whether or not there's a caption) so the image's
    // height doesn't jump around as captions come and go.
    let pin_y = right.bottom() - pad - btn_h;
    let nav_y = pin_y - 6 - btn_h;
    let label_y = nav_y - 6 - (GLYPH_H as i32 + 4);
    let info_y = label_y - (GLYPH_H as i32 + 6);
    let img_h = (info_y - ry).max(0) as u32;

    if pause.has_thumb {
        draw_image_absolute(canvas, images, PAUSE_THUMB_IMG, rx, ry, rw, img_h);
    } else {
        draw_text_absolute(
            canvas,
            font,
            rx,
            ry,
            TextStyle::new(1, PANEL_DIM),
            tr!("empty slot", "slot vazio"),
            usize::MAX,
        );
    }

    let info = format!(
        "{}/{}{}",
        pause.page + 1,
        pause.captures,
        if pause.pinned {
            tr!(" (pinned)", " (fixado)")
        } else {
            ""
        }
    );
    draw_text_absolute(
        canvas,
        font,
        rx,
        info_y,
        TextStyle::new(1, PANEL_DIM),
        &info,
        usize::MAX,
    );
    if !pause.slot_label.is_empty() {
        draw_text_absolute(
            canvas,
            font,
            rx,
            label_y,
            TextStyle::new(1, PANEL_TEXT),
            &pause.slot_label,
            usize::MAX,
        );
    }

    let half = (rw / 2).saturating_sub(4);
    let prev_btn = Rect::new(rx, nav_y, half, btn_h as u32);
    let next_btn = Rect::new(rx + half as i32 + 8, nav_y, half, btn_h as u32);
    buttons.push((
        PanelButton::PauseNotePrev,
        draw_button(
            canvas,
            font,
            prev_btn,
            tr!("< previous", "< anterior"),
            pause.page > 0,
        ),
    ));
    buttons.push((
        PanelButton::PauseNoteNext,
        draw_button(
            canvas,
            font,
            next_btn,
            tr!("next >", "próxima >"),
            pause.page + 1 < pause.captures,
        ),
    ));

    let pin_btn = Rect::new(rx, pin_y, half, btn_h as u32);
    let name_btn = Rect::new(rx + half as i32 + 8, pin_y, half, btn_h as u32);
    let can_pin = out_h > 800 || pause.pinned;
    let note_pin = draw_button(
        canvas,
        font,
        pin_btn,
        if pause.pinned {
            tr!("Pinned", "Fixado")
        } else {
            tr!("Pin", "Fixar")
        },
        can_pin,
    );
    if can_pin {
        buttons.push((PanelButton::PauseNotePin, note_pin));
    }
    buttons.push((
        PanelButton::PauseNoteName,
        draw_button(
            canvas,
            font,
            name_btn,
            if pause.slot_label.is_empty() {
                tr!("Name screenshot", "Nomear print")
            } else {
                tr!("Rename", "Renomear")
            },
            true,
        ),
    ));

    // Button below both pages, in the same margin band above them
    // (`pause_pages` leaves a symmetric top/bottom gap of `PAUSE_TOP`) —
    // hidden while writing (commit/cancel the draft first). Spans both pages
    // (plan revision: used to split this band with "Avancar quadro", a
    // dev-only frame-step nobody reached through the actual UI — removed,
    // so the one real action left gets the whole width).
    if pause.draft.is_none() {
        let band = (out_h as f32 * PAUSE_TOP) as i32;
        let bottom_btn_h = (GLYPH_H + 12) as i32;
        let btn_y = out_h as i32 - band + (band - bottom_btn_h).max(0) / 2;
        let continue_btn = Rect::new(
            left.x(),
            btn_y,
            (right.right() - left.x()).max(0) as u32,
            bottom_btn_h as u32,
        );
        buttons.push((
            PanelButton::PauseContinue,
            draw_button(
                canvas,
                font,
                continue_btn,
                tr!("Back to the game (B)", "Voltar ao jogo (B)"),
                true,
            ),
        ));
        // Dica do gamepad logo abaixo do botão (só cabe com a faixa baixa
        // folgada; some em janelas muito pequenas).
        let hint_y = btn_y + bottom_btn_h + 4;
        if hint_y + GLYPH_H as i32 <= out_h as i32 {
            let hint = tr!(
                "< > screenshot   LB/RB note   X pin   Y name   Start write   Select pin note",
                "< > print   LB/RB nota   X fixar   Y nomear   Start escrever   Select fixar nota"
            );
            let w = GLYPH_W as i32 * hint.chars().count() as i32;
            draw_text_absolute(
                canvas,
                font,
                (out_w as i32 - w) / 2,
                hint_y,
                TextStyle::new(1, PANEL_DIM),
                hint,
                usize::MAX,
            );
        }
    }

    buttons
}

/// Moldura do destaque do gamepad numa linha do modal: borda dupla clara e
/// uma setinha à esquerda, legíveis mesmo sobre uma linha apagada.
fn draw_cursor_frame(canvas: &mut WindowCanvas, rect: Rect) {
    canvas.set_draw_color(Color::RGB(PANEL_WARN.0, PANEL_WARN.1, PANEL_WARN.2));
    for k in 0..2i32 {
        let r = Rect::new(
            rect.x() - k - 1,
            rect.y() - k - 1,
            rect.width() + 2 * (k as u32 + 1),
            rect.height() + 2 * (k as u32 + 1),
        );
        let _ = canvas.draw_rect(r);
    }
    // Tinta translúcida por dentro, para o destaque ler de longe.
    canvas.set_blend_mode(sdl3::render::BlendMode::Blend);
    canvas.set_draw_color(Color::RGBA(PANEL_WARN.0, PANEL_WARN.1, PANEL_WARN.2, 46));
    let _ = canvas.fill_rect(rect);
    canvas.set_blend_mode(sdl3::render::BlendMode::None);
}

/// Draw the modal dialog: a single centered card on a plain background,
/// replacing the whole window like the pause book does (plan revision) but
/// much lighter — a title, then either a grid of clickable slot rows (two
/// columns once there are more than a handful, so the 15 print slots don't
/// make the card comically tall) plus a Cancel button, or — once a print's
/// slot is picked — a text field (name it) with Confirm/Cancel. `None`
/// (shouldn't happen — always set before `present_modal`/`capture_modal_bmp`
/// are called) draws nothing and returns no buttons.
pub(super) fn draw_modal(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    modal: Option<&ModalInfo>,
    out_w: u32,
    out_h: u32,
) -> Vec<(PanelButton, Rect)> {
    canvas.set_draw_color(Color::RGB(CABINET.0, CABINET.1, CABINET.2));
    let _ = canvas.fill_rect(Rect::new(0, 0, out_w, out_h));
    let Some(modal) = modal else {
        return Vec::new();
    };

    let pad = 24i32;
    let btn_h = (GLYPH_H + 10) as i32;
    let row_h = (GLYPH_H + 6) as i32;
    let whole = Rect::new(0, 0, out_w, out_h);
    let mut buttons = Vec::new();

    // Naming step: the row grid is replaced by a text field, same card width
    // either way so the dialog doesn't visibly jump size between steps.
    let card_w = 460u32.min(out_w.saturating_sub(80));
    if let Some(draft) = &modal.draft {
        let card_h = pad as u32 * 2 + GLYPH_H + 8 + GLYPH_H * 2 + 12 + btn_h as u32;
        let card = centered_in(whole, card_w, card_h);
        canvas.set_draw_color(Color::RGB(PANEL_BG.0, PANEL_BG.1, PANEL_BG.2));
        let _ = canvas.fill_rect(card);

        let x = card.x() + pad;
        let inner_w = card.width().saturating_sub(pad as u32 * 2);
        let mut cy = card.y() + pad;
        cy = draw_text_wrapped_absolute(
            canvas,
            font,
            x,
            cy,
            inner_w,
            TextStyle::new(1, PANEL_DIM),
            &modal.draft_heading,
        );
        cy += 8;
        draw_text_absolute(
            canvas,
            font,
            x,
            cy,
            TextStyle::new(2, PANEL_TEXT),
            draft,
            usize::MAX,
        );
        let counter = format!("{}/{}", draft.chars().count(), modal.draft_limit);
        let counter_w = (GLYPH_W as i32) * counter.chars().count() as i32;
        draw_text_absolute(
            canvas,
            font,
            card.right() - pad - counter_w,
            cy,
            TextStyle::new(1, PANEL_DIM),
            &counter,
            usize::MAX,
        );
        cy += GLYPH_H as i32 * 2 + 12;

        let gap = 12i32;
        let half_w = ((inner_w as i32 - gap) / 2).max(1) as u32;
        let confirm = Rect::new(x, cy, half_w, btn_h as u32);
        let cancel = Rect::new(x + half_w as i32 + gap, cy, half_w, btn_h as u32);
        buttons.push((
            PanelButton::ModalConfirm,
            draw_button(canvas, font, confirm, tr!("Save", "Salvar"), true),
        ));
        buttons.push((
            PanelButton::ModalCancel,
            draw_button(canvas, font, cancel, tr!("Cancel", "Cancelar"), true),
        ));
        return buttons;
    }

    // Row-picking step.
    let n_rows = modal.rows.len();
    let cols = if n_rows > 8 { 2usize } else { 1usize };
    let rows_per_col = n_rows.div_ceil(cols).max(1);
    let col_gap = 16i32;
    // Wide enough for the longest label, but never so wide it'd spill past
    // the window — whatever
    // doesn't fit even then still clips with an ellipsis (`draw_button`),
    // this is just about not truncating the common case. Floors at 200 to
    // keep Save/Load/Print (short "Slot N" labels) exactly as wide as
    // before this revision.
    let longest_label = modal
        .rows
        .iter()
        .map(|r| r.label.chars().count())
        .max()
        .unwrap_or(0) as u32;
    let desired_col_w = (longest_label + 2) * GLYPH_W + 12;
    let max_col_w = ((out_w.saturating_sub(160))
        .saturating_sub(col_gap as u32 * (cols as u32 - 1))
        / cols as u32)
        .max(120);
    let col_w = desired_col_w.max(200).min(max_col_w);
    let card_w =
        card_w.max(pad as u32 * 2 + col_w * cols as u32 + col_gap as u32 * (cols as u32 - 1));
    let card_w = card_w.min(out_w.saturating_sub(40));

    // How many rows fit without any scroll UI eating into the budget —
    // save/load's 10 and print's 15 normally just show everything; only a
    // list that overflows the card (a small window) scrolls, and only then
    // do the two extra rows it costs enter the picture at all.
    let max_card_h = out_h.saturating_sub(40);
    let fixed_overhead = pad as u32 * 2 + GLYPH_H + 16 + 12 + btn_h as u32;
    let budget_no_scroll = max_card_h.saturating_sub(fixed_overhead) as i32;
    let rows_that_fit = (budget_no_scroll / row_h).max(1) as usize;
    let scrollable = n_rows > 0 && rows_per_col > rows_that_fit;
    let visible_rows = if scrollable {
        let scroll_ui_h = (btn_h + 8) * 2;
        let budget = budget_no_scroll - scroll_ui_h;
        (budget / row_h).max(1) as usize
    } else {
        rows_per_col
    };
    let scroll = if scrollable {
        let mut sc = modal.scroll.min(rows_per_col - visible_rows);
        if modal.cursor_on && !modal.rows.is_empty() {
            // O destaque do pad manda: rola a grade até ele aparecer.
            let line = modal.cursor % rows_per_col;
            if line < sc {
                sc = line;
            } else if line >= sc + visible_rows {
                sc = line + 1 - visible_rows;
            }
        }
        sc
    } else {
        0
    };

    let extra_h = if scrollable {
        ((btn_h + 8) * 2) as u32
    } else {
        0
    };
    let card_h = fixed_overhead + extra_h + visible_rows as u32 * row_h as u32;
    let card = centered_in(whole, card_w, card_h.min(max_card_h));
    canvas.set_draw_color(Color::RGB(PANEL_BG.0, PANEL_BG.1, PANEL_BG.2));
    let _ = canvas.fill_rect(card);

    let x = card.x() + pad;
    let inner_w = card.width().saturating_sub(pad as u32 * 2);
    let mut cy = card.y() + pad;
    draw_text_absolute(
        canvas,
        font,
        x,
        cy,
        TextStyle::new(2, PANEL_TEXT),
        &modal.title,
        usize::MAX,
    );
    if scrollable {
        // Same "counter next to a scale-2 heading" placement the naming
        // step's char counter already uses, just against the title instead
        // of a draft's text — no extra row spent on it.
        let counter = format!(
            "{}-{}/{}",
            scroll + 1,
            (scroll + visible_rows).min(rows_per_col),
            rows_per_col
        );
        let counter_w = (GLYPH_W as i32) * counter.chars().count() as i32;
        draw_text_absolute(
            canvas,
            font,
            card.right() - pad - counter_w,
            cy,
            TextStyle::new(1, PANEL_DIM),
            &counter,
            usize::MAX,
        );
    }
    cy += GLYPH_H as i32 + 16;

    if scrollable {
        let up = Rect::new(x, cy, inner_w, btn_h as u32);
        buttons.push((
            PanelButton::ModalScrollUp,
            draw_button(canvas, font, up, tr!("^ Up", "^ Cima"), scroll > 0),
        ));
        cy += btn_h + 8;
    }

    let rows_top = cy;
    for (i, row) in modal.rows.iter().enumerate() {
        let col = i / rows_per_col;
        let row_in_col = i % rows_per_col;
        if row_in_col < scroll || row_in_col >= scroll + visible_rows {
            continue; // scrolled out of view this frame
        }
        let rx = x + col as i32 * (col_w as i32 + col_gap);
        let ry = rows_top + (row_in_col - scroll) as i32 * row_h;
        let rect = Rect::new(rx, ry, col_w, (row_h - 4) as u32);
        buttons.push((
            PanelButton::ModalSlot(i as u16),
            draw_button(canvas, font, rect, &row.label, row.enabled),
        ));
        if modal.cursor_on && i == modal.cursor {
            draw_cursor_frame(canvas, rect);
        }
    }
    cy = rows_top + visible_rows as i32 * row_h;

    if scrollable {
        let down = Rect::new(x, cy, inner_w, btn_h as u32);
        buttons.push((
            PanelButton::ModalScrollDown,
            draw_button(
                canvas,
                font,
                down,
                tr!("v Down", "v Baixo"),
                scroll + visible_rows < rows_per_col,
            ),
        ));
        cy += btn_h + 8;
    }
    cy += 12;

    let cancel = Rect::new(x, cy, inner_w, btn_h as u32);
    buttons.push((
        PanelButton::ModalCancel,
        draw_button(canvas, font, cancel, tr!("Cancel", "Cancelar"), true),
    ));

    buttons
}
