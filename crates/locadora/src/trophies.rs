//! Troféus da locadora (design §9 e §17). v1 local; espelho RA fica pra v2.

use crate::store::Persisted;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TrophyId {
    BemVindoAFicha,         // sorteou e escolheu o primeiro jogo
    PrimeiroTurno,          // 1h acumulada no cartucho em serviço
    VizinhoDoAcervo,        // primeira locação
    TrocaTroca,             // primeira troca de cartucho
    PrazoCumprido,          // devolveu um ticket dentro do prazo
    Colegao,                // 5 jogos na coleção
    Lojista,                // 10 jogos comprados
    FreguesFiel,            // locou em 7 dias distintos
    SobreviventeDaEstatica, // voltou ao trampo depois do tubo apagar
    DevolucaoAtrasada,      // devolveu (ou foi devolvido) depois do prazo
}

impl TrophyId {
    pub const ALL: [Self; 10] = [
        Self::BemVindoAFicha,
        Self::PrimeiroTurno,
        Self::VizinhoDoAcervo,
        Self::TrocaTroca,
        Self::PrazoCumprido,
        Self::Colegao,
        Self::Lojista,
        Self::FreguesFiel,
        Self::SobreviventeDaEstatica,
        Self::DevolucaoAtrasada,
    ];

    pub fn title(&self) -> &'static str {
        match self {
            Self::BemVindoAFicha => "Bem-vindo à ficha",
            Self::PrimeiroTurno => "Primeiro turno",
            Self::VizinhoDoAcervo => "Vizinho do acervo",
            Self::TrocaTroca => "Troca-troca",
            Self::PrazoCumprido => "Prazo cumprido",
            Self::Colegao => "Colecão",
            Self::Lojista => "Lojista",
            Self::FreguesFiel => "Freguês fiel",
            Self::SobreviventeDaEstatica => "Sobrevivente da estática",
            Self::DevolucaoAtrasada => "Devolução Atrasada",
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            Self::BemVindoAFicha => "escolha seu primeiro jogo no balcão",
            Self::PrimeiroTurno => "jogue 1h no cartucho em serviço",
            Self::VizinhoDoAcervo => "loque seu primeiro jogo",
            Self::TrocaTroca => "troque de cartucho na hora avulsa",
            Self::PrazoCumprido => "devolva um ticket dentro do prazo",
            Self::Colegao => "tenha 5 jogos na coleção",
            Self::Lojista => "compre 10 jogos na loja",
            Self::FreguesFiel => "loque em 7 dias distintos",
            Self::SobreviventeDaEstatica => "volte ao trampo depois do tubo apagar",
            Self::DevolucaoAtrasada => "todo mundo devolve tarde uma vez",
        }
    }

    pub fn key(&self) -> String {
        format!("{self:?}")
    }
}

/// Condições novas do estado → troféus desbloqueados agora (cada um uma vez).
pub fn check_trophies(st: &mut Persisted, now: i64) -> Vec<TrophyId> {
    let mut out = Vec::new();
    for t in TrophyId::ALL {
        if st.trophies.iter().any(|r| r.id == t.key()) {
            continue;
        }
        let won = match t {
            TrophyId::BemVindoAFicha => st.intro_done,
            TrophyId::PrimeiroTurno => st.stats.ms_served >= 60 * 60_000,
            TrophyId::VizinhoDoAcervo => st.stats.rentals >= 1,
            TrophyId::TrocaTroca => st.stats.swaps >= 1,
            TrophyId::PrazoCumprido => st.stats.returns_in_time >= 1,
            TrophyId::Colegao => st.owned.len() >= 5,
            TrophyId::Lojista => st.stats.purchases >= 10,
            TrophyId::FreguesFiel => st.stats.rental_days >= 7,
            TrophyId::SobreviventeDaEstatica => st.stats.blackout_recoveries >= 1,
            TrophyId::DevolucaoAtrasada => st.stats.returns_late >= 1,
        };
        if won {
            st.trophies.push(crate::store::TrophyRecord {
                id: t.key(),
                unlocked_unix: now,
            });
            out.push(t);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prices::Prices;

    #[test]
    fn first_hour_unlocks_intro_and_turn() {
        let mut s = Persisted::default();
        s.intro_done = true;
        s.credit_service_ms(60 * 60_000, &Prices::default());
        let got = check_trophies(&mut s, 1_000);
        assert!(got.contains(&TrophyId::BemVindoAFicha));
        assert!(got.contains(&TrophyId::PrimeiroTurno));
        let again = check_trophies(&mut s, 2_000);
        assert!(again.is_empty());
        assert_eq!(s.trophies.len(), 2);
    }

    #[test]
    fn every_condition_has_trophy() {
        let mut s = Persisted::default();
        s.intro_done = true;
        s.stats.blackout_recoveries = 1;
        s.stats.swaps = 1;
        s.stats.rentals = 5; // VizinhoDoAcervo
        s.stats.returns_in_time = 1; // PrazoCumprido
        s.stats.returns_late = 1; // DevolucaoAtrasada
        s.stats.rental_days = 7; // FreguesFiel
        for i in 0..5 {
            s.owned.push(format!("s{i}"));
        } // Colegão
        let got = check_trophies(&mut s, 500);
        for t in [
            TrophyId::BemVindoAFicha,
            TrophyId::VizinhoDoAcervo,
            TrophyId::TrocaTroca,
            TrophyId::PrazoCumprido,
            TrophyId::Colegao,
            TrophyId::FreguesFiel,
            TrophyId::SobreviventeDaEstatica,
            TrophyId::DevolucaoAtrasada,
        ] {
            assert!(got.contains(&t), "faltou {t:?}");
        }
    }
}
