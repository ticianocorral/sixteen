//! Cheat Genius: a ROM original (ver `cheat_genius/cheat_genius.s`) que roda
//! no core antes do jogo, como o acessório de trapaça dos anos 90 rodava
//! antes do cartucho. Ela lista os códigos, deixa ligar/desligar e digitar
//! um novo; o app só empresta a memória (a "caixa de correio" na WRAM) e,
//! quando a ROM termina, descarrega-a, carrega o jogo e aplica a escolha.
//!
//! Este módulo é o lado Rust do protocolo descrito no cabeçalho do `.s`:
//! [`encode`] monta a caixa de correio, [`decode`] lê o resultado. As duas
//! são funções puras sobre bytes (testáveis sem core); [`start`] e [`poll`]
//! só ligam essas funções à WRAM do core.

use sixteen_emulation::{Core, MEMORY_SYSTEM_RAM};

/// A ROM, montada por `scripts/build_cheat_genius.sh` e versionada.
pub const ROM: &[u8] = include_bytes!("../cheat_genius/cheat_genius.sfc");

const MB_READY: usize = 0x1000;
const MB_STATUS: usize = 0x1001;
const MB_COUNT: usize = 0x1002;
const MB_ORIG: usize = 0x1004;
const MB_NCUST: usize = 0x1006;
const MB_TITLE: usize = 0x1010;
const MB_CUST: usize = 0x1100;
const ENTRIES: usize = 0x2000;
const ENTRY_SIZE: usize = 32;
const CUST_SIZE: usize = 128;

/// Entradas que cabem (3840) e quantos códigos novos a ROM aceita por sessão.
pub const MAX_ENTRIES: usize = 3840;
/// A WRAM inteira que `encode`/`decode` endereçam (`ENTRIES + MAX_ENTRIES *
/// ENTRY_SIZE` fecha exatamente em 128 KB).
const WRAM_LEN: usize = 0x20000;
pub const MAX_NEW: usize = 8;
/// Caracteres de descrição que a ROM desenha por linha.
const DESC_COLS: usize = 26;
const TITLE_COLS: usize = 30;

const FLAG_ON: u8 = 1;
const FLAG_CUSTOM: u8 = 2;
const FLAG_DELETE: u8 = 4;

/// Uma linha da lista: o que a ROM mostra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub desc: String,
    pub on: bool,
    /// Código do jogador (pode ser apagado com X na ROM).
    pub custom: bool,
}

/// O que o jogador decidiu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// `true`: jogar com os cheats marcados; `false`: B, jogar sem nenhum.
    pub play: bool,
    /// Estado final de cada entrada enviada (mesma ordem), `None` para as
    /// personalizadas marcadas para apagar.
    pub on: Vec<bool>,
    /// Índices (da lista enviada) das personalizadas marcadas para apagar.
    pub deleted: Vec<usize>,
    /// Códigos novos digitados na ROM, com o estado final de cada um.
    pub new_codes: Vec<(String, bool)>,
}

/// Texto que a fonte da ROM desenha: ASCII imprimível, no máximo `max`
/// colunas (com ".." se cortou).
fn ascii_clip(s: &str, max: usize) -> String {
    let clean: String = s
        .chars()
        .map(|c| if (' '..='~').contains(&c) { c } else { '?' })
        .collect();
    if clean.chars().count() <= max {
        clean
    } else {
        let head: String = clean.chars().take(max - 2).collect();
        format!("{head}..")
    }
}

fn put_str(ram: &mut [u8], at: usize, s: &str) {
    ram[at..at + s.len()].copy_from_slice(s.as_bytes());
    ram[at + s.len()] = 0;
}

/// Preenche a caixa de correio em `ram` (WRAM inteira, 128 KB).
pub fn encode(ram: &mut [u8], title: &str, entries: &[Entry]) {
    if ram.len() < WRAM_LEN {
        log::warn!("cheat genius: WRAM de {} bytes, esperava 128 KB", ram.len());
        return;
    }
    let n = entries.len().min(MAX_ENTRIES);
    if entries.len() > MAX_ENTRIES {
        log::warn!(
            "cheat genius: {} entradas, só {MAX_ENTRIES} cabem",
            entries.len()
        );
    }
    ram[MB_STATUS] = 0;
    ram[MB_COUNT..MB_COUNT + 2].copy_from_slice(&(n as u16).to_le_bytes());
    ram[MB_ORIG..MB_ORIG + 2].copy_from_slice(&(n as u16).to_le_bytes());
    ram[MB_NCUST..MB_NCUST + 2].copy_from_slice(&0u16.to_le_bytes());
    put_str(ram, MB_TITLE, &ascii_clip(title, TITLE_COLS));
    for (i, e) in entries.iter().take(n).enumerate() {
        let at = ENTRIES + i * ENTRY_SIZE;
        ram[at] = (e.on as u8 * FLAG_ON) | (e.custom as u8 * FLAG_CUSTOM);
        put_str(ram, at + 1, &ascii_clip(&e.desc, DESC_COLS));
    }
    // por último: a ROM só segue da tela de carga quando vê isto
    ram[MB_READY] = 0xA5;
}

/// Lê o resultado; `None` enquanto a ROM ainda não terminou. `sent` é o
/// número de entradas que [`encode`] mandou.
pub fn decode(ram: &[u8], sent: usize) -> Option<Verdict> {
    if ram.len() < WRAM_LEN {
        return None;
    }
    let status = ram[MB_STATUS];
    if status == 0 {
        return None;
    }
    let sent = sent.min(MAX_ENTRIES);
    let count = u16::from_le_bytes([ram[MB_COUNT], ram[MB_COUNT + 1]]) as usize;
    let ncust = (u16::from_le_bytes([ram[MB_NCUST], ram[MB_NCUST + 1]]) as usize).min(MAX_NEW);
    let mut on = Vec::with_capacity(sent);
    let mut deleted = Vec::new();
    for i in 0..sent {
        let f = ram[ENTRIES + i * ENTRY_SIZE];
        if f & FLAG_DELETE != 0 && f & FLAG_CUSTOM != 0 {
            deleted.push(i);
            on.push(false);
        } else {
            on.push(f & FLAG_ON != 0);
        }
    }
    let mut new_codes = Vec::new();
    for k in 0..ncust {
        let idx = sent + k;
        // `count` vem da RAM: sem o teto, `sent` perto do máximo indexava
        // além dos 128 KB da WRAM (panic).
        if idx >= count || idx >= MAX_ENTRIES {
            break;
        }
        let f = ram[ENTRIES + idx * ENTRY_SIZE];
        if f & FLAG_DELETE != 0 {
            continue;
        }
        let at = MB_CUST + k * CUST_SIZE;
        let raw = &ram[at..at + CUST_SIZE];
        let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
        let code = String::from_utf8_lossy(&raw[..end]).into_owned();
        new_codes.push((code, f & FLAG_ON != 0));
    }
    Some(Verdict {
        play: status == 1,
        on,
        deleted,
        new_codes,
    })
}

/// Um código que o jogador digitou na ROM (o banco embutido não o tem).
/// Vive em `custom_cheats.txt`, uma linha `1<TAB>CÓDIGO` (ligado) ou
/// `0<TAB>CÓDIGO` por código.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Custom {
    pub code: String,
    pub on: bool,
}

/// Quantos códigos próprios um jogo guarda (as linhas da lista não acabam).
pub const MAX_CUSTOM: usize = 64;

pub fn parse_customs(text: &str) -> Vec<Custom> {
    text.lines()
        .filter_map(|line| {
            let (flag, code) = line.split_once('\t')?;
            let code = code.trim();
            sixteen_domain::is_valid_cheat_code(code).then(|| Custom {
                code: code.to_string(),
                on: flag.trim() == "1",
            })
        })
        .take(MAX_CUSTOM)
        .collect()
}

pub fn serialize_customs(customs: &[Custom]) -> String {
    customs
        .iter()
        .map(|c| format!("{}\t{}\n", c.on as u8, c.code))
        .collect()
}

/// A lista que a ROM mostra: os códigos do jogador primeiro (são os que ele
/// quer achar), depois os do banco, cada um com o estado salvo.
pub fn build_entries(
    db: &[sixteen_domain::CheatDef],
    state: &[bool],
    customs: &[Custom],
) -> Vec<Entry> {
    let own = customs.iter().map(|c| Entry {
        desc: c.code.clone(),
        on: c.on,
        custom: true,
    });
    let shipped = db.iter().zip(state).map(|(d, &on)| Entry {
        desc: d.desc.to_string(),
        on,
        custom: false,
    });
    own.chain(shipped).collect()
}

/// Aplica o que o jogador decidiu ao estado do app: os liga/desliga do
/// banco, os códigos próprios (com os apagados removidos) e os recém
/// digitados (só os válidos e ainda não existentes).
pub fn apply_verdict(v: &Verdict, state: &mut [bool], customs: &mut Vec<Custom>) {
    let nc = customs.len();
    let mut kept = Vec::with_capacity(nc);
    for (j, c) in customs.iter().enumerate() {
        if !v.deleted.contains(&j) {
            kept.push(Custom {
                code: c.code.clone(),
                on: v.on.get(j).copied().unwrap_or(c.on),
            });
        }
    }
    for (i, slot) in state.iter_mut().enumerate() {
        if let Some(&on) = v.on.get(nc + i) {
            *slot = on;
        }
    }
    for (code, on) in &v.new_codes {
        let code = code.trim().to_ascii_uppercase();
        if kept.len() < MAX_CUSTOM
            && sixteen_domain::is_valid_cheat_code(&code)
            && !kept.iter().any(|k| k.code == code)
        {
            kept.push(Custom { code, on: *on });
        }
    }
    *customs = kept;
}

/// Entrega a lista à ROM que acabou de ser carregada, antes do 1º frame.
pub fn start(core: &mut Core, title: &str, entries: &[Entry]) -> bool {
    core.with_memory_mut(MEMORY_SYSTEM_RAM, |ram| encode(ram, title, entries))
}

/// Pergunta à ROM se o jogador já decidiu.
pub fn poll(core: &mut Core, sent: usize) -> Option<Verdict> {
    let mut out = None;
    core.with_memory(MEMORY_SYSTEM_RAM, |ram| out = decode(ram, sent));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ram() -> Vec<u8> {
        vec![0; 0x20000]
    }

    fn e(desc: &str, on: bool) -> Entry {
        Entry {
            desc: desc.into(),
            on,
            custom: false,
        }
    }

    #[test]
    fn encode_writes_the_layout_the_rom_expects() {
        let mut r = ram();
        encode(
            &mut r,
            "Aladdin (USA)",
            &[e("Infinite Apples", true), e("Max Gems", false)],
        );
        assert_eq!(r[MB_READY], 0xA5);
        assert_eq!(u16::from_le_bytes([r[MB_COUNT], r[MB_COUNT + 1]]), 2);
        assert_eq!(&r[MB_TITLE..MB_TITLE + 14], b"Aladdin (USA)\0");
        assert_eq!(r[ENTRIES], FLAG_ON);
        assert_eq!(&r[ENTRIES + 1..ENTRIES + 17], b"Infinite Apples\0");
        assert_eq!(r[ENTRIES + ENTRY_SIZE], 0);
    }

    #[test]
    fn long_descriptions_are_clipped_to_what_the_rom_draws() {
        let mut r = ram();
        let long = "A very long cheat description that goes on";
        encode(&mut r, "x", &[e(long, false)]);
        let at = ENTRIES + 1;
        let end = r[at..].iter().position(|&b| b == 0).unwrap();
        assert_eq!(end, DESC_COLS);
        assert_eq!(&r[at + end - 2..at + end], b"..");
    }

    #[test]
    fn non_ascii_becomes_question_marks() {
        assert_eq!(ascii_clip("Vidas Infinitas ã", 30), "Vidas Infinitas ?");
    }

    #[test]
    fn decode_is_none_until_the_rom_reports() {
        let mut r = ram();
        encode(&mut r, "x", &[e("a", false)]);
        assert_eq!(decode(&r, 1), None);
    }

    #[test]
    fn decode_reads_toggles_deletes_and_new_codes() {
        let mut r = ram();
        let entries = [
            e("a", false),
            Entry {
                desc: "mine".into(),
                on: true,
                custom: true,
            },
            Entry {
                desc: "old".into(),
                on: true,
                custom: true,
            },
        ];
        encode(&mut r, "x", &entries);
        // a ligada, "old" marcada para apagar
        r[ENTRIES] |= FLAG_ON;
        r[ENTRIES + 2 * ENTRY_SIZE] |= FLAG_DELETE;
        // um código novo na ROM
        r[ENTRIES + 3 * ENTRY_SIZE] = FLAG_ON | FLAG_CUSTOM;
        r[MB_CUST..MB_CUST + 10].copy_from_slice(b"C24F-74D4\0");
        r[MB_COUNT] = 4;
        r[MB_NCUST] = 1;
        r[MB_STATUS] = 1;
        let v = decode(&r, 3).unwrap();
        assert!(v.play);
        assert_eq!(v.on, vec![true, true, false]);
        assert_eq!(v.deleted, vec![2]);
        assert_eq!(v.new_codes, vec![("C24F-74D4".to_string(), true)]);
    }

    #[test]
    fn status_two_means_play_without_cheats() {
        let mut r = ram();
        encode(&mut r, "x", &[e("a", true)]);
        r[MB_STATUS] = 2;
        assert!(!decode(&r, 1).unwrap().play);
    }

    #[test]
    fn customs_round_trip_and_drop_junk() {
        let customs = vec![
            Custom {
                code: "C24F-74D4".into(),
                on: true,
            },
            Custom {
                code: "7E036999".into(),
                on: false,
            },
        ];
        let text = serialize_customs(&customs);
        assert_eq!(text, "1\tC24F-74D4\n0\t7E036999\n");
        assert_eq!(parse_customs(&text), customs);
        assert_eq!(parse_customs("1\tnot a code\n0\n\n"), vec![]);
    }

    #[test]
    fn entries_put_the_players_own_codes_first() {
        let db = sixteen_domain::cheats_for_title("Aladdin (USA)");
        let state = vec![false; db.len()];
        let customs = vec![Custom {
            code: "C24F-74D4".into(),
            on: true,
        }];
        let list = build_entries(&db, &state, &customs);
        assert_eq!(list.len(), db.len() + 1);
        assert!(list[0].custom && list[0].on && list[0].desc == "C24F-74D4");
        assert!(!list[1].custom);
    }

    #[test]
    fn verdict_updates_state_customs_and_new_codes() {
        let mut state = vec![false, false];
        let mut customs = vec![
            Custom {
                code: "C24F-74D4".into(),
                on: true,
            },
            Custom {
                code: "7E036999".into(),
                on: true,
            },
        ];
        // custom 1 apagado, custom 0 desligado, db[1] ligado, mais um novo
        // (um duplicado e um inválido são ignorados)
        let v = Verdict {
            play: true,
            on: vec![false, false, false, true],
            deleted: vec![1],
            new_codes: vec![
                ("d3e6-e4a4".into(), true),
                ("C24F-74D4".into(), true),
                ("nope".into(), true),
            ],
        };
        apply_verdict(&v, &mut state, &mut customs);
        assert_eq!(state, vec![false, true]);
        assert_eq!(
            customs,
            vec![
                Custom {
                    code: "C24F-74D4".into(),
                    on: false
                },
                Custom {
                    code: "D3E6-E4A4".into(),
                    on: true
                },
            ]
        );
    }
}
