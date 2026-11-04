//! "Sugestão da noite": a escolha de um jogo para hoje, de preferência um
//! que o jogador nunca abriu ou que está parado há muito tempo. Lógica pura
//! (sem disco, sem relógio, sem sorteio global) — a estante monta os
//! `Candidate` a partir do catálogo (`last_played_at`, `play_count`) e do
//! tempo de jogo já gravado, e passa um `Rng` com semente.

use sixteen_platform::tr;

/// O que se sabe de um jogo para decidir se vale sugeri-lo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Candidate {
    /// Tempo total ligado, em segundos (0 = nunca).
    pub playtime_secs: u64,
    /// Quando foi aberto pela última vez (segundos Unix), se já foi.
    pub last_played_at: Option<i64>,
    /// Quantas vezes foi aberto pela estante.
    pub play_count: u32,
}

impl Candidate {
    /// Nunca aberto: sem registro de abertura nem de tempo ligado.
    pub fn never_played(&self) -> bool {
        self.last_played_at.is_none() && self.play_count == 0 && self.playtime_secs == 0
    }
}

/// Por que este jogo foi sugerido — vira a frase do cartão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Never,
    /// Faz N dias desde a última vez (só quando N >= 30).
    LongAgo {
        days: u32,
    },
    /// Poucos minutos de jogo no total.
    LittlePlayed {
        minutes: u32,
    },
    /// Nada de especial: só uma boa pedida.
    Plain,
}

/// Abaixo disto o jogo conta como "pouco jogado".
const LITTLE_SECS: u64 = 30 * 60;
const DAY: i64 = 86_400;

/// Gerador pseudoaleatório pequeno (splitmix64): determinístico dada a
/// semente, o que deixa os testes estáveis.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

fn days_since(c: &Candidate, now: i64) -> Option<u32> {
    c.last_played_at
        .map(|t| ((now - t).max(0) / DAY).min(u32::MAX as i64) as u32)
}

/// O peso do jogo no sorteio: nunca jogado pesa mais que tudo; depois
/// vem o tempo parado (1 ponto por dia, até um ano) e um bônus para o que
/// teve pouco tempo de jogo.
pub fn weight(c: &Candidate, now: i64) -> u64 {
    if c.never_played() {
        return 1000;
    }
    let mut w = 100u64;
    if let Some(d) = days_since(c, now) {
        w += d.min(365) as u64;
    }
    w += match c.playtime_secs {
        s if s < LITTLE_SECS => 300,
        s if s < 2 * 3600 => 120,
        s if s < 5 * 3600 => 40,
        _ => 0,
    };
    w
}

/// Sorteia um índice de `cands`, ponderado por [`weight`]. `avoid` (a
/// sugestão anterior) fica de fora, a menos que seja o único jogo.
/// `None` só com a lista vazia.
pub fn pick(cands: &[Candidate], now: i64, avoid: Option<usize>, rng: &mut Rng) -> Option<usize> {
    if cands.is_empty() {
        return None;
    }
    let skip = avoid.filter(|_| cands.len() > 1);
    let total: u64 = cands
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != skip)
        .map(|(_, c)| weight(c, now))
        .sum();
    let mut r = rng.next_u64() % total.max(1);
    for (i, c) in cands.iter().enumerate() {
        if Some(i) == skip {
            continue;
        }
        let w = weight(c, now);
        if r < w {
            return Some(i);
        }
        r -= w;
    }
    None
}

/// A razão a mostrar no cartão.
pub fn reason(c: &Candidate, now: i64) -> Reason {
    if c.never_played() {
        return Reason::Never;
    }
    if let Some(days) = days_since(c, now) {
        if days >= 30 {
            return Reason::LongAgo { days };
        }
    }
    if c.playtime_secs < LITTLE_SECS {
        return Reason::LittlePlayed {
            minutes: (c.playtime_secs / 60) as u32,
        };
    }
    Reason::Plain
}

/// A frase da razão, na língua da interface.
pub fn reason_text(r: Reason) -> String {
    match r {
        Reason::Never => tr!("never played", "nunca jogado").to_string(),
        Reason::LongAgo { days } => {
            let months = days / 30;
            if months >= 12 {
                let y = months / 12;
                if y == 1 {
                    tr!(
                        "it's been a year since you played",
                        "faz 1 ano que você não joga"
                    )
                    .to_string()
                } else {
                    tr!(
                        format!("it's been {y} years since you played"),
                        format!("faz {y} anos que você não joga")
                    )
                }
            } else if months == 1 {
                tr!(
                    "it's been a month since you played",
                    "faz 1 mês que você não joga"
                )
                .to_string()
            } else {
                tr!(
                    format!("it's been {months} months since you played"),
                    format!("faz {months} meses que você não joga")
                )
            }
        }
        Reason::LittlePlayed { minutes: 0 } => tr!("barely played", "quase não jogado").to_string(),
        Reason::LittlePlayed { minutes } => tr!(
            format!("only {minutes} min played"),
            format!("só {minutes} min jogados")
        ),
        Reason::Plain => tr!("a good pick for tonight", "uma boa pedida pra hoje").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_800_000_000;

    fn played(days_ago: i64, mins: u64) -> Candidate {
        Candidate {
            playtime_secs: mins * 60,
            last_played_at: Some(NOW - days_ago * DAY),
            play_count: 3,
        }
    }

    fn never() -> Candidate {
        Candidate {
            playtime_secs: 0,
            last_played_at: None,
            play_count: 0,
        }
    }

    #[test]
    fn weights_rank_never_then_old_then_recent() {
        let n = weight(&never(), NOW);
        let old = weight(&played(200, 600), NOW);
        let recent = weight(&played(2, 600), NOW);
        assert!(n > old && old > recent);
        // pouco tempo de jogo ganha bônus
        assert!(weight(&played(2, 10), NOW) > recent);
        // nunca ultrapassa o melhor caso de um jogo já jogado
        assert!(n > weight(&played(9999, 1), NOW));
    }

    #[test]
    fn empty_list_gives_none() {
        assert_eq!(pick(&[], NOW, None, &mut Rng::new(1)), None);
    }

    #[test]
    fn deterministic_for_a_seed() {
        let c = [never(), played(100, 50), played(1, 900), played(40, 5)];
        let a = pick(&c, NOW, None, &mut Rng::new(42));
        let b = pick(&c, NOW, None, &mut Rng::new(42));
        assert_eq!(a, b);
    }

    #[test]
    fn avoids_the_last_suggestion_but_keeps_a_lone_game() {
        let c = [never(), played(100, 50), played(1, 900)];
        for seed in 0..200 {
            assert_ne!(pick(&c, NOW, Some(0), &mut Rng::new(seed)), Some(0));
        }
        assert_eq!(pick(&[never()], NOW, Some(0), &mut Rng::new(7)), Some(0));
    }

    #[test]
    fn favours_unplayed_and_stale_games() {
        let c = [never(), played(1, 900), played(2, 900), played(3, 900)];
        let hits = (0..500)
            .filter(|s| pick(&c, NOW, None, &mut Rng::new(*s)) == Some(0))
            .count();
        // peso 1000 contra ~3x(100+1..3): bem mais da metade
        assert!(hits > 300, "never played got only {hits}/500");
    }

    #[test]
    fn reasons() {
        assert_eq!(reason(&never(), NOW), Reason::Never);
        assert_eq!(
            reason(&played(100, 600), NOW),
            Reason::LongAgo { days: 100 }
        );
        assert_eq!(
            reason(&played(3, 12), NOW),
            Reason::LittlePlayed { minutes: 12 }
        );
        assert_eq!(reason(&played(3, 600), NOW), Reason::Plain);
        // tempo ligado sem data de abertura não é "nunca jogado"
        let c = Candidate {
            playtime_secs: 3 * 3600,
            last_played_at: None,
            play_count: 0,
        };
        assert_eq!(reason(&c, NOW), Reason::Plain);
    }

    #[test]
    fn reason_text_months() {
        let t = reason_text(Reason::LongAgo { days: 95 });
        assert!(t.contains('3'), "{t}");
    }
}
