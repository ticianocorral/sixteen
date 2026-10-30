//! O painel lateral: jogo, configurações e estante.

use super::*;

/// Where the side panel sits: a column on the right, `PANEL_FRAC` of the
/// window, clamped so it neither collapses nor swallows a small window.
pub(super) fn panel_rect(out_w: u32, out_h: u32) -> Rect {
    let w = ((out_w as f32) * PANEL_FRAC)
        .round()
        .clamp(PANEL_MIN as f32, PANEL_MAX as f32) as u32;
    let w = w.min(out_w.saturating_sub(64));
    Rect::new((out_w - w) as i32, 0, w, out_h)
}

/// Draw the side panel: background, logo (or title) at top, session timer at
/// the bottom. Plain widgets, not warped by the tube (plan §2, §3.2). `None`
/// (no game loaded — the idle/root screen) draws just the "Inserir cartucho"
/// button in the logo's spot. Returns the clickable buttons drawn this frame,
/// in `rect`'s (output/canvas) coordinate space — the caller stores them for
/// `Cabinet::hit_panel_button`.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_panel(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    panel: Option<&PanelInfo>,
    rect: Rect,
    session: Duration,
    idle_core_prompt: Option<&str>,
    dev_mode: bool,
    core_status: Option<&str>,
    tv_on: bool,
) -> Vec<(PanelButton, Rect)> {
    if rect.width() == 0 {
        return Vec::new();
    }
    canvas.set_draw_color(Color::RGB(PANEL_BG.0, PANEL_BG.1, PANEL_BG.2));
    let _ = canvas.fill_rect(rect);

    let pad = 20i32;
    let inner_w = rect.width().saturating_sub(pad as u32 * 2);
    let x = rect.x() + pad;
    let y = rect.y() + pad;

    let Some(panel) = panel else {
        // Idle/root screen == the "cartridge ejected" screen (plan revision:
        // one screen, not two — startup, backing out of the shelf, and
        // ejecting a game all land here). The console's own brand logo where
        // a game's logo would sit and, abaixo dele, o botão da estante — e
        // nada de console: a face do console vive só na tela de jogo (plan
        // revision: "remover o console da tela inicial"). No command legend
        // — there's nothing loaded to command — and Configuracoes moves to
        // the footer, the same spot the session clock uses during play.
        let mut cy = if images.contains_key(&PANEL_CONSOLE_LOGO_IMG) {
            draw_image_absolute(canvas, images, PANEL_CONSOLE_LOGO_IMG, x, y, inner_w, 110);
            y + 110
        } else {
            draw_text_wrapped_absolute(
                canvas,
                font,
                x,
                y,
                inner_w,
                TextStyle::new(2, PANEL_TEXT),
                BRAND,
            )
        };
        cy += 26;
        // O botão da estante flutuante no topo — o slot, as chaves e o
        // Eject saíram junto (plan revision: "remover o console da tela
        // inicial"). O resto da coluna fica limpo até o rodapé.
        let btn_w = (inner_w as f32 * 0.75).round() as u32;
        let btn_h = GLYPH_H * 2 + 24;
        let btn = Rect::new(
            x + (inner_w as i32 - btn_w as i32) / 2,
            cy + 6,
            btn_w,
            btn_h,
        );
        let insert_drawn = draw_button(
            canvas,
            font,
            btn,
            tr!("GAME SHELF", "ESTANTE DE GAMES"),
            true,
        );
        // A estante mora no tubo: com a TV desligada, o aviso fica logo
        // abaixo do botão (as Configurações não dependem da TV — só a
        // estante de games).
        let mut hint_bottom = insert_drawn.bottom();
        if !tv_on {
            let hint = tr!(
                "turn the TV on to open the shelf",
                "ligue a TV para abrir a estante"
            );
            let hint_w = GLYPH_W as i32 * hint.chars().count() as i32;
            draw_text_absolute(
                canvas,
                font,
                btn.x() + (btn_w as i32 - hint_w) / 2,
                insert_drawn.bottom() + 4,
                TextStyle::new(1, (150, 150, 158)),
                hint,
                usize::MAX,
            );
            hint_bottom = insert_drawn.bottom() + 4 + GLYPH_H as i32;
        }
        cy = hint_bottom;

        let btn_h = (GLYPH_H + 12) as i32;
        let settings = Rect::new(x, rect.bottom() - pad - btn_h, inner_w, btn_h as u32);
        let mut buttons = vec![
            (PanelButton::Insert, insert_drawn),
            (
                PanelButton::Settings,
                draw_button(
                    canvas,
                    font,
                    settings,
                    tr!("Settings", "Configurações"),
                    true,
                ),
            ),
        ];
        // O botão do modo dev (plan revision: "habilitar botão em cima da
        // configuração") — só existe com o devmode ligado, e o devmode só
        // entra pelo código Konami nesta tela. Entra na pilha entre o
        // "Configurações" e o aviso de core, que sobe junto.
        let mut btn_top = settings.y();
        if dev_mode {
            let dev = Rect::new(x, btn_top - 8 - btn_h, inner_w, btn_h as u32);
            btn_top = dev.y();
            buttons.push((
                PanelButton::Dev,
                draw_button(canvas, font, dev, "Dev", true),
            ));
        }
        // The core prompt (plan revision: "avisar que para jogar é
        // necessário o download do core" / "coloque o aviso em cima do
        // botão de download"): warning line directly above the button, both
        // just above the Configurações footer (ou do Dev, quando ele existe).
        if let Some(label) = idle_core_prompt {
            let warn = tr!(
                "To play you need to download the snes9x core.",
                "Para jogar é necessário baixar o núcleo snes9x."
            );
            let btn = Rect::new(x, btn_top - 8 - btn_h, inner_w, btn_h as u32);
            let warn_h = wrapped_height(inner_w, 1, warn);
            let warn_y = btn.y() - 6 - warn_h;
            if warn_y > cy {
                draw_text_wrapped_absolute(
                    canvas,
                    font,
                    x,
                    warn_y,
                    inner_w,
                    TextStyle::new(1, PANEL_WARN),
                    warn,
                );
                buttons.push((
                    PanelButton::CoreDownload,
                    draw_button(canvas, font, btn, label, true),
                ));
            }
        } else if let Some(status) = core_status {
            // Andamento do download do núcleo na seta verde (plan revision:
            // "cliquei no icone verde de update do nucleo mas nao abriu
            // nada") — linha dim acima do rodapé; sem botão, o clique é na
            // seta do nameplate.
            let warn_h = wrapped_height(inner_w, 1, status);
            let warn_y = btn_top - 6 - warn_h;
            if warn_y > cy {
                draw_text_wrapped_absolute(
                    canvas,
                    font,
                    x,
                    warn_y,
                    inner_w,
                    TextStyle::new(1, PANEL_WARN),
                    status,
                );
            }
        }
        return buttons;
    };

    // 1. Logo, or the title if there isn't one (plan §3.2, item 1).
    let mut cy = if panel.has_logo {
        draw_image_absolute(canvas, images, PANEL_LOGO_IMG, x, y, inner_w, 110);
        y + 110
    } else {
        draw_text_wrapped_absolute(
            canvas,
            font,
            x,
            y,
            inner_w,
            TextStyle::new(2, PANEL_TEXT),
            &panel.title,
        )
    };

    // 1b. Cartridge art (plan revision) — independent of the logo, drawn
    // right below whichever of the two just ran. The block is the console's
    // loading slot itself (`draw_panel_slot`): the cartridge sits seated in
    // it at rest, and the insert/eject animation plays right here — where
    // the cartridge lives during gameplay, not as a screen transition.
    if panel.has_cartridge {
        cy += 8;
        const CARTRIDGE_H: u32 = 220;
        let (t, ejecting) = panel.cartridge_motion.unwrap_or((1.0, false));
        draw_panel_slot(
            canvas,
            images,
            Rect::new(x, cy, inner_w, CARTRIDGE_H),
            t,
            ejecting,
        );
        cy += CARTRIDGE_H as i32;
    }

    // Below this y, stop — leave the session clock's own band (item 6)
    // clear. There are a lot more command rows than there used to be (plan
    // revision added six), so a long title plus a game with several curated
    // cheats can genuinely run out of room; a clean cutoff beats spilling
    // into the clock. No scrolling yet (a real gap, not silently accepted —
    // see `docs/fase-4.md`'s revision note).
    let limit = rect.bottom() - pad - (GLYPH_H as i32 + 12);

    // 2. Power / Eject / Reset (plan revision): styled after the real
    // console's own controls instead of three more text rows — Power and
    // Reset are rocker switches (see `draw_rocker`), Eject sits between
    // them the way the cartridge-slot label does on the actual hardware.
    // Pulled out of the generic command loop below (which skips these three
    // by kind) so they get this dedicated look instead of a plain button.
    const SWITCH_TRACK_H: i32 = 64;
    const SWITCH_GROUP_H: i32 = SWITCH_TRACK_H + 4;
    let mut buttons = Vec::new();
    if cy + SWITCH_GROUP_H <= limit {
        cy += 14;
        let gap = 10i32;
        // Eject gets first claim on width (its "EJETAR"/"EJECT" label doesn't
        // shrink), the two switches split whatever's left evenly — on a
        // narrow panel that leaves them tighter, not the label overflowing
        // its box.
        let eject_label = tr!("EJECT", "EJETAR");
        let eject_w = (GLYPH_W as i32) * eject_label.len() as i32 + 16;
        let switch_w = ((inner_w as i32 - gap * 2 - eject_w) / 2).max(1);
        let eject_w = (inner_w as i32 - gap * 2 - switch_w * 2).max(eject_w);
        let power_track = Rect::new(x, cy, switch_w as u32, SWITCH_TRACK_H as u32);
        let eject_rect = Rect::new(
            x + switch_w + gap,
            cy + SWITCH_TRACK_H - (GLYPH_H as i32 + 6),
            eject_w as u32,
            GLYPH_H + 6,
        );
        let reset_track = Rect::new(
            x + switch_w + gap + eject_w + gap,
            cy,
            switch_w as u32,
            SWITCH_TRACK_H as u32,
        );
        buttons.push((
            PanelButton::Power,
            draw_rocker(canvas, font, power_track, "POWER", panel.powered, true),
        ));
        buttons.push((
            PanelButton::Eject,
            draw_eject_button(canvas, font, eject_rect, eject_label, !panel.powered),
        ));
        buttons.push((
            PanelButton::Reset,
            draw_rocker(
                canvas,
                font,
                reset_track,
                "RESET",
                panel.reset_pressed,
                panel.powered,
            ),
        ));
        // Power LED, in the gap above Eject (plan revision) — the rockers
        // fill the switch group's full height, but Eject's own box only
        // spans the bottom of it, same as it does on the real hardware.
        let led_size = 16;
        let led_top = cy + (eject_rect.y() - cy - led_size) / 2;
        draw_led(
            canvas,
            eject_rect.x() + eject_rect.width() as i32 / 2,
            led_top,
            led_size,
            panel.powered,
        );
        cy += SWITCH_GROUP_H;
    }

    // 3. Commands — the console's own buttons, not the emulator's extras
    // (plan §3.2, item 3), every one of them clickable (plan revision:
    // mouse/gamepad only, no keyboard legend any more). Everything here
    // only acts while powered — Power/Eject/Reset are drawn separately
    // above (item 2) with their own dim rules, so they're filtered out of
    // this list rather than drawn twice.
    let other_commands: Vec<&(PanelButton, String)> = panel
        .commands
        .iter()
        .filter(|(k, _)| {
            !matches!(
                k,
                PanelButton::Power | PanelButton::Eject | PanelButton::Reset
            )
        })
        .collect();
    if !other_commands.is_empty() && cy < limit {
        cy += 16;
        draw_text_absolute(
            canvas,
            font,
            x,
            cy,
            TextStyle::new(1, PANEL_DIM),
            tr!("commands", "comandos"),
            usize::MAX,
        );
        cy += GLYPH_H as i32 + 6;
    }
    for (kind, label) in other_commands {
        // Check the row's projected bottom, not just its top — otherwise
        // the last row drawn can still poke into the session clock's band.
        if cy + 4 + (GLYPH_H + 6) as i32 > limit {
            break;
        }
        cy += 4;
        // Tighter than a lone idle-screen button (plan revision: nine of
        // these stack now, not three — every extra pixel per row adds up).
        let btn = Rect::new(x, cy, inner_w, GLYPH_H + 6);
        let drawn = draw_button(canvas, font, btn, label, panel.powered);
        cy = drawn.bottom();
        buttons.push((*kind, drawn));
    }

    // 4. Cheats — just a count here (plan revision: used to list every
    // active description, one per line, which could run the panel out of
    // room on its own for a game with several curated cheats on at once;
    // the actual list lives in the Cheats modal, reachable from a
    // `commands` row above, so nothing is lost by not repeating it here).
    // Absent entirely with none on, not a "0 cheats" filler.
    let cheats_on_count = panel.cheats.iter().filter(|(_, on)| *on).count();
    // Room for the header plus one line — the count line never wraps to
    // more than that (it's always short: "N cheats ativados").
    let cheats_fit = cy + 16 + (GLYPH_H as i32 + 2) <= limit;
    if cheats_on_count > 0 && cheats_fit {
        cy += 16;
        let label = if cheats_on_count == 1 {
            tr!("1 cheat enabled", "1 cheat ativado").to_string()
        } else {
            tr!(
                format!("{cheats_on_count} cheats enabled"),
                format!("{cheats_on_count} cheats ativados")
            )
        };
        cy = draw_text_wrapped_absolute(
            canvas,
            font,
            x,
            cy,
            inner_w,
            TextStyle::new(1, PANEL_TEXT),
            &label,
        );
    }

    // 5. Notes — a pinned print's thumbnail + counter, and/or a pinned
    // text-note slot's content (plan §3.2, item 5; §3.4; plan revision:
    // the two are independent — either, both, or neither can be showing at
    // once, "mostrar apenas uma nota e/ou uma imagem"). Used to show
    // whatever was captured/viewed most recently regardless of pin, which
    // was surprising; now it's opt-in on both sides. Absent entirely with
    // nothing pinned either way, not a "no notes" filler. Same room-check
    // shape as cheats, above — the thumbnail and the text block each add
    // their own extra room when present.
    let text_h = panel
        .text_note
        .as_ref()
        .map_or(0, |t| wrapped_height(inner_w, 1, t) + 6);
    let notes_h = 16
        + (GLYPH_H as i32 + 6)
        + if panel.has_note_thumb { 70 + 6 } else { 0 }
        + if panel.note_count > 0 {
            GLYPH_H as i32 + 6
        } else {
            0
        }
        + text_h;
    if (panel.note_count > 0 || panel.text_note.is_some()) && cy + notes_h <= limit {
        cy += 16;
        draw_text_absolute(
            canvas,
            font,
            x,
            cy,
            TextStyle::new(1, PANEL_DIM),
            tr!("notes", "notas"),
            usize::MAX,
        );
        cy += GLYPH_H as i32 + 6;
        if panel.has_note_thumb {
            draw_image_absolute(canvas, images, PANEL_NOTE_IMG, x, cy, inner_w, 70);
            cy += 70 + 6;
        }
        if panel.note_count > 0 {
            let label = if panel.note_count == 1 {
                tr!("1 slot pinned", "1 slot fixado").to_string()
            } else {
                tr!(
                    format!("{} slots pinned", panel.note_count),
                    format!("{} slots fixados", panel.note_count)
                )
            };
            draw_text_absolute(
                canvas,
                font,
                x,
                cy,
                TextStyle::new(1, PANEL_TEXT),
                &label,
                usize::MAX,
            );
            cy += GLYPH_H as i32 + 6;
        }
        if let Some(text) = &panel.text_note {
            draw_text_wrapped_absolute(
                canvas,
                font,
                x,
                cy,
                inner_w,
                TextStyle::new(1, PANEL_TEXT),
                text,
            );
        }
    }

    // 6. Session clock, pinned to the bottom (plan §3.2, item 6).
    let secs = session.as_secs();
    let stamp = if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, (secs / 60) % 60, secs % 60)
    } else {
        format!("{:02}:{:02}", secs / 60, secs % 60)
    };
    let ty = rect.bottom() - pad - GLYPH_H as i32;
    let session_label = tr!("session", "sessão");
    draw_text_absolute(
        canvas,
        font,
        x,
        ty,
        TextStyle::new(1, PANEL_DIM),
        session_label,
        usize::MAX,
    );
    // label + um espaço de respiro antes do relógio (a fonte é monoespaçada,
    // então a conta é direta).
    let label_w = (GLYPH_W as i32) * (session_label.chars().count() as i32 + 1);
    draw_text_absolute(
        canvas,
        font,
        x + label_w,
        ty,
        TextStyle::new(1, PANEL_TEXT),
        &stamp,
        usize::MAX,
    );

    buttons
}

/// Draw the shelf's own flat side panel (plan revision — moved out of the
/// warped screen buffer so its text and art render undistorted, the same
/// treatment the in-game panel already gets): logo/title, cartridge art,
/// play count, then whatever extra info the No-Intro DAT carried beyond the
/// bare title, and finally the shelf's own Voltar/Configuracoes buttons.
/// `None` (nothing selected yet — an empty catalogue) still draws the two
/// buttons. Returns the clickable buttons drawn this frame, in `rect`'s
/// (output/canvas) coordinate space — the caller stores them for
/// `Cabinet::hit_shelf_button`.
/// The settings screen's flat panel (plan revision: "coloque o painel ... se
/// for interessante faça secoes na configuração"): title, one button per
/// section (the current one lit), and "Voltar" pinned at the bottom — the
/// same bottom-button stack the shelf panel uses.
pub(super) fn draw_settings_panel(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    panel: Option<&SettingsPanelInfo>,
    rect: Rect,
) -> Vec<(SettingsButton, Rect)> {
    if rect.width() == 0 {
        return Vec::new();
    }
    canvas.set_draw_color(Color::RGB(PANEL_BG.0, PANEL_BG.1, PANEL_BG.2));
    let _ = canvas.fill_rect(rect);

    let pad = 20i32;
    let inner_w = rect.width().saturating_sub(pad as u32 * 2);
    let x = rect.x() + pad;
    let mut y = rect.y() + pad;

    let Some(panel) = panel else {
        return Vec::new();
    };
    draw_text_wrapped_absolute(
        canvas,
        font,
        x,
        y,
        inner_w,
        TextStyle::new(2, PANEL_TEXT),
        &panel.title,
    );
    y += 2 * GLYPH_H as i32 + 16;

    let btn_h = (GLYPH_H + 12) as i32;
    let back_rect = Rect::new(x, rect.bottom() - pad - btn_h, inner_w, btn_h as u32);
    let mut buttons = vec![(
        SettingsButton::Back,
        draw_button(canvas, font, back_rect, tr!("Back", "Voltar"), true),
    )];

    // Section buttons, top to bottom under the title — the currently shown
    // section lit, the others dim (same lit/dim language as every other
    // panel button).
    let limit = back_rect.y() - 12;
    for (i, name) in panel.sections.iter().enumerate() {
        if y + btn_h > limit {
            break;
        }
        let r = Rect::new(x, y, inner_w, btn_h as u32);
        buttons.push((
            SettingsButton::Section(i),
            draw_button(canvas, font, r, name, i == panel.selected),
        ));
        y += btn_h + 8;
    }

    buttons
}

pub(super) fn draw_shelf_panel(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    panel: Option<&ShelfPanelInfo>,
    rect: Rect,
) -> Vec<(ShelfButton, Rect)> {
    if rect.width() == 0 {
        return Vec::new();
    }
    canvas.set_draw_color(Color::RGB(PANEL_BG.0, PANEL_BG.1, PANEL_BG.2));
    let _ = canvas.fill_rect(rect);

    let pad = 20i32;
    let inner_w = rect.width().saturating_sub(pad as u32 * 2);
    let x = rect.x() + pad;
    let y = rect.y() + pad;
    let panel_favorite = panel.and_then(|p| p.favorite);
    let panel_achievements = panel.is_some_and(|p| p.achievements);
    let panel_ach_reason = panel.and_then(|p| p.achievements_reason.as_deref());

    // Stacked buttons at the bottom, "Favoritar" / "Conquistas" /
    // "Configurações" / "Voltar" top to bottom — "Voltar" always the last
    // one (plan revision: "botao voltar sempre o ultimo botao do painel,
    // acima dele coloque configurações e depois acima o de favoritos");
    // "Conquistas" sits between favoritar e configurações (plan revision:
    // "quando ligar o RA colocar botão de lista de conquistas entre
    // favoritar e configurações"); "Atualizar" moved out of the panel into
    // the header row beside "histórico".
    let btn_h = (GLYPH_H + 12) as i32;
    let back_rect = Rect::new(x, rect.bottom() - pad - btn_h, inner_w, btn_h as u32);
    let settings_rect = Rect::new(x, back_rect.y() - 8 - btn_h, inner_w, btn_h as u32);
    // A pilha cresce para cima: Conquistas (quando o RA identificou) e
    // Favoritar — cada um só quando existe, e o corte do corpo acompanha
    // o botão de cima de verdade. O Manual vive nas abas do topo.
    let mut top = settings_rect.y();
    let ach_rect = panel_achievements.then(|| {
        let r = Rect::new(x, top - 8 - btn_h, inner_w, btn_h as u32);
        top = r.y();
        r
    });
    let fav_rect = panel_favorite.map(|_| {
        let r = Rect::new(x, top - 8 - btn_h, inner_w, btn_h as u32);
        top = r.y();
        r
    });
    let mut buttons = vec![
        (
            ShelfButton::Back,
            draw_button(canvas, font, back_rect, tr!("Back", "Voltar"), true),
        ),
        (
            ShelfButton::Settings,
            draw_button(
                canvas,
                font,
                settings_rect,
                tr!("Settings", "Configurações"),
                true,
            ),
        ),
    ];
    if let Some(ach_r) = ach_rect {
        // Bloqueado (RA marcou o hash como versão não suportada): botão
        // apagado com o motivo no rótulo e SEM hit — clicar não faz nada
        // (plan revision: "mostrar mas deixar desativado").
        match panel_ach_reason {
            Some(reason) => {
                let _ = draw_button(
                    canvas,
                    font,
                    ach_r,
                    &format!("{} ({reason})", tr!("Achievements", "Conquistas")),
                    false,
                );
            }
            None => buttons.push((
                ShelfButton::ShelfAchievements,
                draw_button(canvas, font, ach_r, tr!("Achievements", "Conquistas"), true),
            )),
        }
    }
    // The favorite button sits above the trio, drawn before the early
    // `None` return so it never depends on the panel having content.
    if let (Some(fav), Some(fav_r)) = (panel_favorite, fav_rect) {
        buttons.push((
            ShelfButton::ToggleFavorite,
            draw_button(
                canvas,
                font,
                fav_r,
                if fav {
                    tr!("Remove favorite", "Remover favorito")
                } else {
                    tr!("Add favorite", "Favoritar")
                },
                true,
            ),
        ));
    }

    let Some(panel) = panel else {
        return buttons;
    };

    // Everything above the buttons — same cutoff rule `draw_panel` uses
    // for its command rows: a clean stop beats spilling into the buttons.
    // The cutoff is the TOP OF THE FIRST BUTTON ACTUALLY DRAWN (the stack
    // grows upward from "Voltar") — an earlier revision stopped one button
    // short, which was harmless while that slot sat empty and became a real
    // overflow the moment the RA's "Conquistas" button filled it (the info
    // text ran straight over the button).
    let limit = top - 12;

    let cy = if let Some(id) = panel.logo_img {
        draw_image_absolute(canvas, images, id, x, y, inner_w, 110);
        y + 110
    } else {
        draw_text_wrapped_absolute(
            canvas,
            font,
            x,
            y,
            inner_w,
            TextStyle::new(2, PANEL_TEXT),
            &panel.title,
        )
    };

    // Abas (plan revision: "a ideia é mostrar um info de cada vez mesmo em
    // resolução grande, mudar ao clicar") — o corpo mostra SÓ os blocos da
    // seção ativa: capa traseira (imagem), cartucho (imagem) ou
    // informações (lançamento + campos do DAT/RA). Os três botões do topo
    // trocam a aba; a seção sem conteúdo mostra um aviso dim.
    let capa_blocks: Vec<PanelBlock> = panel
        .backcover_img
        .map(|id| vec![PanelBlock::Image(id, 280)])
        .unwrap_or_default();
    let cart_blocks: Vec<PanelBlock> = panel
        .cartridge_img
        .map(|id| vec![PanelBlock::Image(id, 210)])
        .unwrap_or_default();
    let mut info_blocks: Vec<PanelBlock> = Vec::new();
    if panel.release.is_some() || !panel.info.is_empty() {
        info_blocks.push(PanelBlock::Title(tr!(
            "General Information",
            "Informações Gerais"
        )));
    }
    if let Some(release) = &panel.release {
        info_blocks.push(PanelBlock::Field(tr!("released", "lançamento"), release));
    }
    for (label, value) in &panel.info {
        // A linha de conquistas ganha a medalha de prêmio do RA quando o
        // servidor concedeu um. A comparação cobre os dois idiomas: quem
        // produz o rótulo (`ShelfPanelInfo.info`) já fala `tr!` — o lado EN
        // chega como "achievements", o PT como "conquistas".
        match panel
            .award_img
            .filter(|_| label == "conquistas" || label == "achievements")
        {
            Some(medal) => info_blocks.push(PanelBlock::RaField(label, value, medal)),
            None => info_blocks.push(PanelBlock::Field(label, value)),
        }
    }
    // O bloco HowLongToBeat fecha a aba: o cabeçalho com o ícone e uma
    // barra por estilo — em blocos separados para o scroll encaixar o que
    // couber (o bloco inteiro não caberia e o `break` o pularia inteiro).
    // A seção de nota da IGDB, entre as infos gerais e o HowLongToBeat.
    if let Some(igdb) = &panel.igdb {
        info_blocks.push(PanelBlock::SectionHead(igdb.icon, "IGDB"));
        info_blocks.push(PanelBlock::Field(tr!("rating", "nota"), &igdb.nota));
    }
    if let Some(hltb) = &panel.hltb {
        if !hltb.rows.is_empty() {
            info_blocks.push(PanelBlock::SectionHead(hltb.icon, "HowLongToBeat"));
            for bar in &hltb.rows {
                info_blocks.push(PanelBlock::Field(&bar.label, &bar.text));
            }
        }
    }
    let manual_blocks: Vec<PanelBlock> = panel
        .manual_first_page
        .map(|id| vec![PanelBlock::Image(id, 280)])
        .unwrap_or_default();
    let (blocks, empty_label): (Vec<PanelBlock>, &str) = match panel.section {
        PanelSection::CapaTraseira => (capa_blocks, tr!("no back cover", "sem capa traseira")),
        PanelSection::Cartucho => (cart_blocks, tr!("no cartridge", "sem cartucho")),
        PanelSection::Informacoes => (info_blocks, tr!("no information", "sem informações")),
        PanelSection::Manual => (manual_blocks, tr!("no manual", "sem manual")),
    };

    let has = |s: PanelSection| match s {
        PanelSection::CapaTraseira => panel.backcover_img.is_some(),
        PanelSection::Cartucho => panel.cartridge_img.is_some(),
        PanelSection::Informacoes => {
            panel.release.is_some() || !panel.info.is_empty() || panel.hltb.is_some()
        }
        PanelSection::Manual => panel.manual_first_page.is_some(),
    };

    // A aba ativa transborda? (só "informações", com muitos campos,
    // realisticamente) — se sim, o `scroll` do caller pula blocos dentro
    // dela; as outras seções têm um bloco só.
    let total_h: i32 = blocks.iter().map(|b| panel_block_height(inner_w, b)).sum();
    let scrollable = cy + total_h > limit;

    let scroll_btn_h = btn_h;
    let start = if scrollable {
        panel.scroll.min(blocks.len().saturating_sub(1))
    } else {
        0
    };

    // Plan revision ("eu queria aqueles 3 botões de organização das infos
    // do game na tela grande também"): os saltos ficam SEMPRE visíveis com
    // jogo focused — em telas grandes o conteúdo costuma caber sem rolagem
    // e antes os botões nem apareciam. Sem transbordo o salto é no-op
    // (tudo já está na tela). Como a fileira de baixo das antigas setas
    // sumiu, o corpo ganha o espaço dela de volta.
    let gap = 6i32;
    let half = (inner_w as i32 - gap) / 2;
    let active = panel.section;
    // Quatro abas em duas fileiras: informações | capa traseira, depois
    // manual | cartucho (plan revision: "mudar ordem dos botoes: informações,
    // capa traseira, manual e cartucho").
    let tabs = [
        (
            ShelfButton::PanelJump(PanelSection::Informacoes),
            tr!("information", "informações"),
            active == PanelSection::Informacoes || has(PanelSection::Informacoes),
        ),
        (
            ShelfButton::PanelJump(PanelSection::CapaTraseira),
            tr!("back cover", "capa traseira"),
            active == PanelSection::CapaTraseira || has(PanelSection::CapaTraseira),
        ),
        (
            ShelfButton::PanelJump(PanelSection::Manual),
            tr!("manual", "manual"),
            active == PanelSection::Manual || has(PanelSection::Manual),
        ),
        (
            ShelfButton::PanelJump(PanelSection::Cartucho),
            tr!("cartridge", "cartucho"),
            active == PanelSection::Cartucho || has(PanelSection::Cartucho),
        ),
    ];
    // Respiro entre a logo/título e a fileira de botões: sem ele os botões
    // nascem colados na borda de baixo da logo (plan revision: "baixar um
    // pouco para os botões não ficarem sobrepostos com a logo").
    let mut body_top = cy + 10;
    for (n, (btn, label, lit)) in tabs.iter().enumerate() {
        let row = (n / 2) as i32;
        let col = (n % 2) as i32;
        let r = Rect::new(
            x + col * (half + gap),
            body_top + row * (scroll_btn_h + 8),
            half as u32,
            scroll_btn_h as u32,
        );
        buttons.push((*btn, draw_button(canvas, font, r, label, *lit)));
    }
    body_top += (scroll_btn_h + 8) * 2;

    let mut cy = body_top;
    if blocks.is_empty() {
        // Aba escolhida sem conteúdo (não tem a arte local, o DAT não
        // trouxe campos) — aviso dim em vez de corpo vazio.
        let _ = draw_text_wrapped_absolute(
            canvas,
            font,
            x,
            cy,
            inner_w,
            TextStyle::new(1, PANEL_DIM),
            empty_label,
        );
    }
    for block in &blocks[start..] {
        let h = panel_block_height(inner_w, block);
        if cy + h > limit {
            break;
        }
        let block_top = cy;
        cy = draw_panel_block(canvas, font, images, x, cy, inner_w, block);
        // The back cover is clickable (plan revision: "ao clicar no back
        // cover possibilitar mostrar em tamanho maior, com botão de
        // fechar") — outline it as the affordance and hand the hit rect up.
        if let (PanelBlock::Image(id, _), Some(bc)) = (block, panel.backcover_img) {
            if *id == bc {
                let hit = Rect::new(x, block_top, inner_w, h.max(1) as u32);
                // Borda preta (plan revision: "a borda do back cover coloque
                // em preto") — discreta sobre o painel claro.
                canvas.set_draw_color(Color::RGBA(0, 0, 0, 200));
                let _ = canvas.draw_rect(hit);
                buttons.push((ShelfButton::Backcover, hit));
            }
        }
        // O cartucho ganha o mesmo tratamento do back cover (plan revision:
        // "colocar zoom igual ao backcover no cartucho").
        if let (PanelBlock::Image(id, _), Some(cc)) = (block, panel.cartridge_img) {
            if *id == cc {
                let hit = Rect::new(x, block_top, inner_w, h.max(1) as u32);
                canvas.set_draw_color(Color::RGBA(0, 0, 0, 200));
                let _ = canvas.draw_rect(hit);
                buttons.push((ShelfButton::Cartridge, hit));
            }
        }
        // E a primeira página do manual (plan revision: "ao clicar abrir
        // nele visualização") — abre o leitor de páginas.
        if let (PanelBlock::Image(id, _), Some(mp)) = (block, panel.manual_first_page) {
            if *id == mp {
                let hit = Rect::new(x, block_top, inner_w, h.max(1) as u32);
                canvas.set_draw_color(Color::RGBA(0, 0, 0, 200));
                let _ = canvas.draw_rect(hit);
                buttons.push((ShelfButton::Manual, hit));
            }
        }
    }

    // (sem fileira de setas embaixo — os saltos nomeados acima cobrem a
    // navegação do corpo)

    buttons
}

/// One scrollable piece of the shelf panel's body (plan revision: "criar
/// rolagem no painel quando necessario") — see `draw_shelf_panel`.
/// `Release` stays its own variant rather than folding into `Field`
/// because it draws label and value on the *same* line (a small
/// headline stat), unlike `Field`'s stacked label-then-value.
pub(super) enum PanelBlock<'a> {
    Image(u64, u32),
    /// Título de seção da aba ("Informações Gerais"), centralizado.
    Title(&'a str),
    /// label/valor com a medalha de prêmio à esquerda do valor — a linha
    /// "conquistas" quando o servidor concedeu um prêmio ao jogo.
    RaField(&'a str, &'a str, u64),
    Field(&'a str, &'a str),
    /// Cabeçalho de seção: ícone + título em negrito (IGDB, HowLongToBeat).
    SectionHead(u64, &'a str),
}

/// Ícone do HowLongToBeat no cabeçalho do bloco (quadrado, com o glifo da
/// legenda dentro) e altura da barra de tempo (o glifo + respiro em cima e
/// embaixo para o tempo caber escrito dentro).
pub(super) const HLTB_ICON: i32 = 40;

/// `block`'s height in pixels, gap included — must match `draw_panel_block`
/// exactly, or "does it fit" and "how tall did it draw" disagree (the bug
/// `draw_shelf_panel`'s own history note above the `info` loop describes).
pub(super) fn panel_block_height(inner_w: u32, block: &PanelBlock) -> i32 {
    match block {
        PanelBlock::Image(_, h) => 8 + *h as i32,
        PanelBlock::Title(_) => 10 + GLYPH_H as i32,
        PanelBlock::RaField(label, value, _) => {
            10 + wrapped_height(inner_w, 1, label) + wrapped_height(inner_w, 1, value)
        }
        PanelBlock::Field(label, value) => {
            10 + wrapped_height(inner_w, 1, label) + wrapped_height(inner_w, 1, value)
        }
        PanelBlock::SectionHead(_, _) => 8 + HLTB_ICON + 4,
    }
}

/// Texto de painel centralizado na largura útil — o que estourar cai no
/// wrap alinhado à esquerda de sempre. Devolve o y logo abaixo do texto.
pub(super) fn draw_text_centered_absolute(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    x: i32,
    y: i32,
    inner_w: u32,
    style: TextStyle,
    s: &str,
) -> i32 {
    let per = GLYPH_W as i32 + if style.bold { 1 } else { 0 };
    let tw = s.chars().count() as i32 * per - if style.bold { 1 } else { 0 };
    if tw <= inner_w as i32 {
        draw_text_absolute(
            canvas,
            font,
            x + (inner_w as i32 - tw) / 2,
            y,
            style,
            s,
            usize::MAX,
        );
        y + GLYPH_H as i32
    } else {
        draw_text_wrapped_absolute(canvas, font, x, y, inner_w, style, s)
    }
}

/// Draw `block` at `cy`, returning the y just past it.
pub(super) fn draw_panel_block(
    canvas: &mut WindowCanvas,
    font: &mut Texture,
    images: &HashMap<u64, ImgTex>,
    x: i32,
    cy: i32,
    inner_w: u32,
    block: &PanelBlock,
) -> i32 {
    match block {
        PanelBlock::Image(id, h) => {
            let cy = cy + 8;
            draw_image_absolute(canvas, images, *id, x, cy, inner_w, *h);
            cy + *h as i32
        }
        PanelBlock::Title(t) => {
            let cy = cy + 10;
            draw_text_centered_absolute(
                canvas,
                font,
                x,
                cy,
                inner_w,
                TextStyle::bold(1, PANEL_TEXT),
                t,
            )
        }
        PanelBlock::RaField(label, value, medal) => {
            let cy = cy + 10;
            let cy = draw_text_centered_absolute(
                canvas,
                font,
                x,
                cy,
                inner_w,
                TextStyle::new(1, PANEL_DIM),
                label,
            );
            // Número + medalha como uma unidade, centrados — a fonte é
            // monoespaçada (GLYPH_W por caractere no size 1), então o fim do
            // texto é conhecido sem medição.
            let value_w = value.chars().count() as i32 * GLYPH_W as i32;
            let total = value_w + AWARD_MEDAL_GAP + AWARD_MEDAL_W;
            let vx = if total <= inner_w as i32 {
                x + (inner_w as i32 - total) / 2
            } else {
                x
            };
            draw_text_absolute(
                canvas,
                font,
                vx,
                cy,
                TextStyle::new(1, PANEL_TEXT),
                value,
                usize::MAX,
            );
            draw_image_absolute(
                canvas,
                images,
                *medal,
                vx + value_w + AWARD_MEDAL_GAP,
                cy + (GLYPH_H as i32 - AWARD_MEDAL_H) / 2,
                AWARD_MEDAL_W as u32,
                AWARD_MEDAL_H as u32,
            );
            cy + GLYPH_H as i32
        }
        PanelBlock::Field(label, value) => {
            let cy = cy + 10;
            let cy = draw_text_centered_absolute(
                canvas,
                font,
                x,
                cy,
                inner_w,
                TextStyle::new(1, PANEL_DIM),
                label,
            );
            draw_text_centered_absolute(
                canvas,
                font,
                x,
                cy,
                inner_w,
                TextStyle::new(1, PANEL_TEXT),
                value,
            )
        }
        PanelBlock::SectionHead(icon, titulo) => {
            // Cabeçalho de seção centrado: o ícone + o título em negrito
            // como uma unidade.
            let cy = cy + 8;
            let legenda = *titulo;
            let total = HLTB_ICON + 6 + legenda.len() as i32 * (GLYPH_W as i32 + 1) - 1;
            let hx = x + (inner_w as i32 - total) / 2;
            draw_image_absolute(
                canvas,
                images,
                *icon,
                hx,
                cy,
                HLTB_ICON as u32,
                HLTB_ICON as u32,
            );
            draw_text_absolute(
                canvas,
                font,
                hx + HLTB_ICON + 8,
                cy + (HLTB_ICON - GLYPH_H as i32) / 2,
                TextStyle::bold(1, PANEL_TEXT),
                legenda,
                usize::MAX,
            );
            cy + HLTB_ICON + 4
        }
    }
}
