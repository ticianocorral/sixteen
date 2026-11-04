//! Teclado na tela ("OSK"): um campo de texto que um controle sozinho
//! consegue preencher — grade de caracteres, d-pad move, A digita, X apaga,
//! Y alterna maiúscula/símbolos, LB/RB movem o cursor de texto, Start
//! confirma, B cancela. Funciona em qualquer lugar que já use
//! `Platform::poll_text_entry` (filtro da estante, campos das conquistas,
//! legenda de print, caderno): o teclado de verdade e o teclado do Steam
//! (Steam+X no Deck) continuam digitando como antes — `update` trata os dois
//! lados no mesmo passo.
//!
//! API (resumo):
//! ```ignore
//! let mut field = TextField::new(&inicial, 40, /*mascarado*/ false);
//! let mut osk = Osk::new();
//! osk.begin(plat.last_input_was_pad());   // ao abrir o campo
//! plat.start_text_input(cab);
//! // a cada quadro, enquanto edita:
//! let te = plat.poll_text_entry();
//! match osk.update(&te, &mut field, plat) {
//!     OskOutcome::Editing => {}
//!     OskOutcome::Commit => { /* usa field.text() */ }
//!     OskOutcome::Cancel => {}
//!     OskOutcome::Quit => { /* janela fechada */ }
//! }
//! // no desenho (dentro do `Screen` do tubo):
//! osk.draw(d, &field, "título do campo");
//! ```
//! Toda a lógica (cursor da grade, camadas, edição) é pura e testada; só
//! `draw` toca no `Screen`.

use super::Screen;
use crate::tr;
use crate::{MenuNav, Platform, TextEntryInput};

/// Colunas da grade de caracteres.
const COLS: usize = 10;
/// Linhas de caracteres (a linha de teclas especiais vem depois).
const CHAR_ROWS: usize = 4;
/// Total de linhas: caracteres + a de teclas especiais.
const ROWS: usize = CHAR_ROWS + 1;
/// Larguras (em colunas) das teclas da última linha: maiúscula, símbolos,
/// espaço, apagar, OK.
const SPECIAL_W: [usize; 5] = [2, 2, 3, 2, 1];

/// A camada de caracteres à mostra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Lower,
    /// Maiúscula de um toque só: volta para `Lower` depois de uma letra.
    Upper,
    Symbols,
}

/// O que uma tecla da grade faz.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Shift,
    Symbols,
    Space,
    Backspace,
    Ok,
}

/// As linhas de caracteres de uma camada.
fn char_rows(layer: Layer) -> [&'static str; CHAR_ROWS] {
    match layer {
        Layer::Lower => ["1234567890", "qwertyuiop", "asdfghjkl@", "zxcvbnm._-"],
        Layer::Upper => ["1234567890", "QWERTYUIOP", "ASDFGHJKL@", "ZXCVBNM._-"],
        Layer::Symbols => ["!?#$%&*()=", ":;/\\+<>[]|", "áéíóúãõâêô", "çàüñ'\",{}~"],
    }
}

/// Quantas teclas a linha `row` tem.
fn row_len(row: usize) -> usize {
    if row < CHAR_ROWS {
        COLS
    } else {
        SPECIAL_W.len()
    }
}

/// Início e largura (em colunas) da tecla `col` da linha `row`.
fn span(row: usize, col: usize) -> (usize, usize) {
    if row < CHAR_ROWS {
        (col, 1)
    } else {
        let start: usize = SPECIAL_W[..col].iter().sum();
        (start, SPECIAL_W[col])
    }
}

/// O centro horizontal da tecla, em meias colunas — a régua que mantém o
/// cursor "no mesmo lugar" ao subir/descer entre linhas de larguras
/// diferentes.
fn center2(row: usize, col: usize) -> usize {
    let (start, w) = span(row, col);
    start * 2 + w
}

/// A tecla da linha `row` mais perto de `c2` (centro em meias colunas).
fn nearest_col(row: usize, c2: usize) -> usize {
    (0..row_len(row))
        .min_by_key(|&c| center2(row, c).abs_diff(c2))
        .unwrap_or(0)
}

/// O campo de texto editado — com cursor, limite e máscara. Todo texto
/// passa por aqui (digitado, colado, ou vindo da grade).
#[derive(Debug, Clone, Default)]
pub struct TextField {
    text: String,
    /// Posição do cursor, em caracteres (0..=len).
    cursor: usize,
    limit: usize,
    masked: bool,
}

impl TextField {
    /// `initial` já preenche o campo (cursor no fim); `limit` em caracteres;
    /// `masked` mostra `*` no lugar do texto (senhas).
    pub fn new(initial: &str, limit: usize, masked: bool) -> Self {
        let text: String = initial
            .chars()
            .filter(|c| !c.is_control())
            .take(limit)
            .collect();
        let cursor = text.chars().count();
        Self {
            text,
            cursor,
            limit,
            masked,
        }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_masked(&self) -> bool {
        self.masked
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn len(&self) -> usize {
        self.text.chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// O texto como aparece na tela (`*` por caractere se mascarado).
    pub fn display(&self) -> String {
        if self.masked {
            "*".repeat(self.len())
        } else {
            self.text.clone()
        }
    }

    fn byte_at(&self, ch: usize) -> usize {
        self.text
            .char_indices()
            .nth(ch)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len())
    }

    /// Insere `s` no cursor, respeitando o limite e descartando controles.
    pub fn insert_str(&mut self, s: &str) {
        for c in s.chars().filter(|c| !c.is_control()) {
            if self.len() >= self.limit {
                break;
            }
            let at = self.byte_at(self.cursor);
            self.text.insert(at, c);
            self.cursor += 1;
        }
    }

    /// Apaga o caractere antes do cursor.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let at = self.byte_at(self.cursor - 1);
        self.text.remove(at);
        self.cursor -= 1;
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.len());
    }
}

/// O que um quadro de edição decidiu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OskOutcome {
    Editing,
    /// Enter, Start ou a tecla OK: o chamador usa `field.text()`.
    Commit,
    /// Esc ou B: descarta o rascunho.
    Cancel,
    /// A janela foi fechada.
    Quit,
}

/// O efeito de apertar um botão no teclado na tela (lógica pura).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OskAct {
    None,
    Insert(char),
    Backspace,
    CursorLeft,
    CursorRight,
    Commit,
}

/// O teclado na tela: a posição na grade, a camada e se está visível.
#[derive(Debug, Clone)]
pub struct Osk {
    shown: bool,
    row: usize,
    col: usize,
    layer: Layer,
}

impl Default for Osk {
    fn default() -> Self {
        Self::new()
    }
}

impl Osk {
    pub fn new() -> Self {
        Self {
            shown: false,
            row: 1,
            col: 0,
            layer: Layer::Lower,
        }
    }

    /// Começa uma edição: `show` costuma ser `plat.last_input_was_pad()`.
    /// Sem teclado visível, qualquer botão do controle durante a edição o
    /// revela (o primeiro aperto só mostra a grade).
    pub fn begin(&mut self, show: bool) {
        *self = Self::new();
        self.shown = show;
    }

    /// Visível agora? Quem desenha o campo só chama `draw` quando sim.
    pub fn shown(&self) -> bool {
        self.shown
    }

    pub fn layer(&self) -> Layer {
        self.layer
    }

    /// A posição do cursor da grade `(linha, coluna)`.
    pub fn pos(&self) -> (usize, usize) {
        (self.row, self.col)
    }

    /// A tecla sob o cursor da grade.
    pub fn key(&self) -> Key {
        self.key_at(self.row, self.col)
    }

    /// A tecla da posição `(row, col)` na camada atual.
    pub fn key_at(&self, row: usize, col: usize) -> Key {
        if row < CHAR_ROWS {
            let c = char_rows(self.layer)[row].chars().nth(col).unwrap_or(' ');
            Key::Char(c)
        } else {
            match col {
                0 => Key::Shift,
                1 => Key::Symbols,
                2 => Key::Space,
                3 => Key::Backspace,
                _ => Key::Ok,
            }
        }
    }

    /// Y: minúscula → maiúscula → símbolos → minúscula.
    pub fn cycle_layer(&mut self) {
        self.layer = match self.layer {
            Layer::Lower => Layer::Upper,
            Layer::Upper => Layer::Symbols,
            Layer::Symbols => Layer::Lower,
        };
    }

    /// Move o cursor da grade (com volta nas bordas). Subir/descer mantém a
    /// posição horizontal mesmo entre linhas de larguras diferentes.
    pub fn move_cursor(&mut self, nav: MenuNav) {
        match nav {
            MenuNav::Left => {
                let n = row_len(self.row);
                self.col = (self.col + n - 1) % n;
            }
            MenuNav::Right => {
                self.col = (self.col + 1) % row_len(self.row);
            }
            MenuNav::Up | MenuNav::Down => {
                let c2 = center2(self.row, self.col);
                self.row = if nav == MenuNav::Up {
                    (self.row + ROWS - 1) % ROWS
                } else {
                    (self.row + 1) % ROWS
                };
                self.col = nearest_col(self.row, c2);
            }
            _ => {}
        }
    }

    /// Um botão do controle com o teclado visível: devolve o efeito.
    pub fn press(&mut self, nav: MenuNav) -> OskAct {
        match nav {
            MenuNav::Up | MenuNav::Down | MenuNav::Left | MenuNav::Right => {
                self.move_cursor(nav);
                OskAct::None
            }
            MenuNav::Confirm => self.activate(),
            MenuNav::Alt => OskAct::Backspace,
            MenuNav::Extra => {
                self.cycle_layer();
                OskAct::None
            }
            MenuNav::PageUp => OskAct::CursorLeft,
            MenuNav::PageDown => OskAct::CursorRight,
            MenuNav::Start => OskAct::Commit,
            _ => OskAct::None,
        }
    }

    /// A na tecla sob o cursor.
    fn activate(&mut self) -> OskAct {
        match self.key() {
            Key::Char(c) => {
                // Maiúscula vale uma letra só.
                if self.layer == Layer::Upper {
                    self.layer = Layer::Lower;
                }
                OskAct::Insert(c)
            }
            Key::Space => OskAct::Insert(' '),
            Key::Backspace => OskAct::Backspace,
            Key::Ok => OskAct::Commit,
            Key::Shift => {
                self.layer = if self.layer == Layer::Upper {
                    Layer::Lower
                } else {
                    Layer::Upper
                };
                OskAct::None
            }
            Key::Symbols => {
                self.layer = if self.layer == Layer::Symbols {
                    Layer::Lower
                } else {
                    Layer::Symbols
                };
                OskAct::None
            }
        }
    }

    /// Um quadro de edição: o que veio do teclado de verdade (texto,
    /// Backspace, colar, copiar, Enter, Esc) e do controle (`te.pad`) cai no
    /// `field`. Digitar no teclado de verdade esconde a grade; qualquer
    /// botão do controle a mostra de volta (o primeiro aperto só revela).
    /// B sempre cancela — com a grade ou sem ela.
    pub fn update(
        &mut self,
        te: &TextEntryInput,
        field: &mut TextField,
        plat: &Platform,
    ) -> OskOutcome {
        if te.quit {
            return OskOutcome::Quit;
        }
        if !te.typed.is_empty() || te.backspace {
            self.shown = false;
        }
        if te.backspace {
            field.backspace();
        }
        field.insert_str(&te.typed);
        if let Some(paste) = &te.paste {
            field.insert_str(paste);
        }
        if te.copy {
            plat.set_clipboard_text(field.text());
        }
        let mut commit = te.commit;
        for nav in &te.pad {
            // B e Start já viraram `cancel`/`commit` na plataforma.
            if matches!(nav, MenuNav::Back | MenuNav::Start) {
                continue;
            }
            if !self.shown {
                self.shown = true;
                continue;
            }
            match self.press(*nav) {
                OskAct::Insert(c) => field.insert_str(&c.to_string()),
                OskAct::Backspace => field.backspace(),
                OskAct::CursorLeft => field.left(),
                OskAct::CursorRight => field.right(),
                OskAct::Commit => commit = true,
                OskAct::None => {}
            }
        }
        if te.cancel {
            OskOutcome::Cancel
        } else if commit {
            OskOutcome::Commit
        } else {
            OskOutcome::Editing
        }
    }

    /// Desenha o teclado (e o campo, com cursor) na base do `Screen`. Não
    /// faz nada enquanto escondido — o chamador desenha seu campo normal.
    pub fn draw(&self, d: &mut Screen, field: &TextField, title: &str) {
        if !self.shown {
            return;
        }
        let (sw, sh) = d.size();
        let l = layout(sw, sh);
        const BG: (u8, u8, u8, u8) = (22, 22, 28, 255);
        const EDGE: (u8, u8, u8, u8) = (150, 150, 158, 255);
        const KEY: (u8, u8, u8, u8) = (44, 44, 54, 255);
        const KEY_SPECIAL: (u8, u8, u8, u8) = (62, 58, 78, 255);
        const SEL: (u8, u8, u8, u8) = (186, 138, 244, 255);
        const TEXT: (u8, u8, u8) = (232, 232, 232);
        const DARK: (u8, u8, u8) = (20, 20, 26);
        const DIM: (u8, u8, u8) = (150, 150, 158);

        let (px, py, pw, ph) = l.panel;
        d.fill(px, py, pw, ph, BG);
        d.outline(px, py, pw, ph, 2, EDGE);
        d.text(px + 12, py + 8, 1, DIM, title);

        // O campo, com o cursor de texto (rola para mantê-lo à vista).
        let (ix, iy, iw, ih) = l.input;
        d.fill(ix, iy, iw, ih, (34, 34, 40, 255));
        d.outline(ix, iy, iw, ih, 1, EDGE);
        let shown = field.display();
        let chars: Vec<char> = shown.chars().collect();
        let max_chars = ((iw as i32 - 16) / 9).max(4) as usize;
        let cur = field.cursor().min(chars.len());
        let start = (cur + 1).saturating_sub(max_chars);
        let visible: String = chars.iter().skip(start).take(max_chars).collect();
        d.text(ix + 8, iy + (ih as i32 - 20) / 2, 1, TEXT, &visible);
        let caret_x = ix + 8 + ((cur - start) as i32) * 9;
        d.fill(caret_x, iy + 4, 2, ih.saturating_sub(8), SEL);

        // A grade.
        for row in 0..ROWS {
            for col in 0..row_len(row) {
                let (rx, ry, rw, rh) = l.cell(row, col);
                let sel = (row, col) == (self.row, self.col);
                let key = self.key_at(row, col);
                let special = !matches!(key, Key::Char(_));
                let bg = if sel {
                    SEL
                } else if special {
                    KEY_SPECIAL
                } else {
                    KEY
                };
                d.fill(rx, ry, rw, rh, bg);
                let label = match key {
                    Key::Char(c) => c.to_string(),
                    Key::Shift => "Aa".to_string(),
                    Key::Symbols => {
                        if self.layer == Layer::Symbols {
                            "abc".to_string()
                        } else {
                            "#@!".to_string()
                        }
                    }
                    Key::Space => tr!("space", "espaço").to_string(),
                    Key::Backspace => tr!("delete", "apagar").to_string(),
                    Key::Ok => "OK".to_string(),
                };
                let tw = label.chars().count() as i32 * 9;
                let color = if sel { DARK } else { TEXT };
                d.text(
                    rx + (rw as i32 - tw) / 2,
                    ry + (rh as i32 - 20) / 2,
                    1,
                    color,
                    &label,
                );
            }
        }

        let (hx, hy, hw, _) = l.hint;
        d.text_wrapped(
            hx,
            hy,
            hw,
            1,
            DIM,
            tr!(
                "A type  X delete  Y shift/symbols  LB/RB cursor  Start OK  B cancel",
                "A digita  X apaga  Y maiúsc./símbolos  LB/RB cursor  Start OK  B cancela"
            ),
        );
    }
}

/// A geometria do teclado dentro de um `Screen` de `sw`×`sh` — pura, para o
/// teste conferir que tudo cabe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OskLayout {
    pub panel: (i32, i32, u32, u32),
    pub input: (i32, i32, u32, u32),
    pub hint: (i32, i32, u32, u32),
    grid_x: i32,
    grid_y: i32,
    cell_w: u32,
    cell_h: u32,
}

impl OskLayout {
    /// O retângulo da tecla `(row, col)`.
    pub fn cell(&self, row: usize, col: usize) -> (i32, i32, u32, u32) {
        let (start, w) = span(row, col);
        (
            self.grid_x + (start as u32 * self.cell_w) as i32 + 2,
            self.grid_y + (row as u32 * self.cell_h) as i32 + 2,
            w as u32 * self.cell_w - 4,
            self.cell_h - 4,
        )
    }
}

/// Calcula a geometria: painel centrado na base, grade de 10 colunas.
pub fn layout(sw: u32, sh: u32) -> OskLayout {
    let side = 16u32;
    let cell_w = ((sw.saturating_sub(side * 2 + 24)).min(560) / COLS as u32).max(28);
    // A altura é a parte apertada: a grade cede antes de estourar a tela.
    let fixed = 8 + 20 + 6 + 34 + 8 + 8 + 46;
    let cell_h = (sh.saturating_sub(fixed + 60) / ROWS as u32).clamp(24, 36);
    let grid_w = cell_w * COLS as u32;
    let pw = grid_w + 24;
    let input_h = 34u32;
    let hint_h = 46u32;
    let ph = 8 + 20 + 6 + input_h + 8 + cell_h * ROWS as u32 + 8 + hint_h;
    let px = (sw as i32 - pw as i32) / 2;
    let py = (sh as i32 - ph as i32 - 12).max(0);
    let input = (px + 12, py + 8 + 20 + 6, grid_w, input_h);
    let grid_y = input.1 + input_h as i32 + 8;
    OskLayout {
        panel: (px, py, pw, ph),
        input,
        hint: (
            px + 12,
            grid_y + (cell_h * ROWS as u32) as i32 + 6,
            grid_w,
            hint_h,
        ),
        grid_x: px + 12,
        grid_y,
        cell_w,
        cell_h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_inserts_at_the_cursor_and_respects_the_limit() {
        let mut f = TextField::new("ab", 4, false);
        f.left();
        f.insert_str("X");
        assert_eq!(f.text(), "aXb");
        assert_eq!(f.cursor(), 2);
        f.insert_str("123456");
        assert_eq!(f.text(), "aX1b");
        assert_eq!(f.len(), 4);
    }

    #[test]
    fn field_backspace_and_cursor_edges() {
        let mut f = TextField::new("áé", 10, false);
        f.backspace();
        assert_eq!(f.text(), "á");
        f.left();
        f.left();
        assert_eq!(f.cursor(), 0);
        f.backspace();
        assert_eq!(f.text(), "á");
        f.right();
        f.right();
        assert_eq!(f.cursor(), 1);
    }

    #[test]
    fn field_drops_control_characters_and_masks() {
        let mut f = TextField::new("", 10, true);
        f.insert_str("ab\ncd\t");
        assert_eq!(f.text(), "abcd");
        assert_eq!(f.display(), "****");
        assert!(f.is_masked());
    }

    #[test]
    fn grid_wraps_horizontally_and_vertically() {
        let mut o = Osk::new();
        o.begin(true);
        // começa na linha das letras (q).
        assert_eq!(o.key(), Key::Char('q'));
        o.move_cursor(MenuNav::Left);
        assert_eq!(o.key(), Key::Char('p'));
        o.move_cursor(MenuNav::Right);
        assert_eq!(o.key(), Key::Char('q'));
        o.move_cursor(MenuNav::Up);
        assert_eq!(o.key(), Key::Char('1'));
        o.move_cursor(MenuNav::Up); // dá a volta para a linha especial
        assert_eq!(o.pos().0, CHAR_ROWS);
    }

    #[test]
    fn vertical_moves_keep_the_horizontal_position_across_row_widths() {
        let mut o = Osk::new();
        o.begin(true);
        // 'p' (coluna 9, borda direita) desce para zxc.. e depois para OK.
        o.move_cursor(MenuNav::Left); // p
        o.move_cursor(MenuNav::Down); // l/@
        o.move_cursor(MenuNav::Down); // '-'
        assert_eq!(o.key(), Key::Char('-'));
        o.move_cursor(MenuNav::Down);
        assert_eq!(o.key(), Key::Ok);
        o.move_cursor(MenuNav::Up);
        assert_eq!(o.key(), Key::Char('-'));
        // 'q' (esquerda) desce até Aa e volta para a mesma coluna.
        let mut o = Osk::new();
        o.begin(true);
        for _ in 0..2 {
            o.move_cursor(MenuNav::Down);
        }
        assert_eq!(o.key(), Key::Char('z'));
        o.move_cursor(MenuNav::Down);
        assert_eq!(o.key(), Key::Shift);
        o.move_cursor(MenuNav::Up);
        assert_eq!(o.key(), Key::Char('z'));
    }

    #[test]
    fn every_required_symbol_is_reachable() {
        for want in "@._-:/+".chars() {
            let found = [Layer::Lower, Layer::Symbols]
                .iter()
                .any(|l| char_rows(*l).iter().any(|r| r.contains(want)));
            assert!(found, "falta {want:?} na grade");
        }
        // Linhas todas com 10 colunas.
        for l in [Layer::Lower, Layer::Upper, Layer::Symbols] {
            for r in char_rows(l) {
                assert_eq!(r.chars().count(), COLS, "{r}");
            }
        }
    }

    #[test]
    fn a_types_and_one_shot_shift_returns_to_lowercase() {
        let mut o = Osk::new();
        o.begin(true);
        assert_eq!(o.press(MenuNav::Extra), OskAct::None);
        assert_eq!(o.layer(), Layer::Upper);
        assert_eq!(o.press(MenuNav::Confirm), OskAct::Insert('Q'));
        assert_eq!(o.layer(), Layer::Lower);
        assert_eq!(o.press(MenuNav::Confirm), OskAct::Insert('q'));
    }

    #[test]
    fn y_cycles_layers_and_x_backspaces() {
        let mut o = Osk::new();
        o.begin(true);
        o.press(MenuNav::Extra);
        o.press(MenuNav::Extra);
        assert_eq!(o.layer(), Layer::Symbols);
        o.press(MenuNav::Extra);
        assert_eq!(o.layer(), Layer::Lower);
        assert_eq!(o.press(MenuNav::Alt), OskAct::Backspace);
        assert_eq!(o.press(MenuNav::PageUp), OskAct::CursorLeft);
        assert_eq!(o.press(MenuNav::PageDown), OskAct::CursorRight);
        assert_eq!(o.press(MenuNav::Start), OskAct::Commit);
    }

    #[test]
    fn special_row_keys_act() {
        let mut o = Osk::new();
        o.begin(true);
        for _ in 0..3 {
            o.move_cursor(MenuNav::Down);
        }
        assert_eq!(o.key(), Key::Shift);
        o.move_cursor(MenuNav::Right); // símbolos
        assert_eq!(o.press(MenuNav::Confirm), OskAct::None);
        assert_eq!(o.layer(), Layer::Symbols);
        assert_eq!(o.press(MenuNav::Confirm), OskAct::None);
        assert_eq!(o.layer(), Layer::Lower);
        o.move_cursor(MenuNav::Right); // espaço
        assert_eq!(o.press(MenuNav::Confirm), OskAct::Insert(' '));
        o.move_cursor(MenuNav::Right); // apagar
        assert_eq!(o.press(MenuNav::Confirm), OskAct::Backspace);
        o.move_cursor(MenuNav::Right); // OK
        assert_eq!(o.press(MenuNav::Confirm), OskAct::Commit);
    }

    #[test]
    fn layout_fits_common_screens() {
        for (w, h) in [(640u32, 360u32), (800, 480), (1000, 640), (480, 320)] {
            let l = layout(w, h);
            let (px, py, pw, ph) = l.panel;
            assert!(px >= 0 && py >= 0, "{w}x{h}: {:?}", l.panel);
            assert!(px as u32 + pw <= w, "{w}x{h}: larga demais {:?}", l.panel);
            assert!(py as u32 + ph <= h, "{w}x{h}: alta demais {:?}", l.panel);
        }
    }
}
