//! Estado inteiro do modo, persistido em config/locadora.json (design §10).
//! Chave de jogo = sha1 da ROM (a mesma do library.json).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::errors::Error;

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Persisted {
    pub version: u32,
    pub wallet_cents: i64,
    pub lifetime_earned_cents: i64,
    pub lifetime_spent_cents: i64,
    pub intro_done: bool,
    /// ms jogados no cartucho em serviço ainda não convertidos em centavos.
    pub service_pending_ms: i64,
    pub owned: Vec<String>,
    pub service_game: Option<String>,
    pub active_ticket: Option<Ticket>,
    pub hourly: Option<HourlySession>,
    pub trophies: Vec<TrophyRecord>,
    pub stats: Stats,
    /// §17 cicatriz da estante: locações por exemplar.
    pub rentals_per_game: HashMap<String, u32>,
    /// §17 cupons de locação grátis (cartão de fidelidade).
    pub coupons: u32,
    /// Sessão atual paga por cupom — end_hourly/start_ticket não debitam.
    #[serde(skip)]
    pub free_session: bool,
    /// Relógio injetável para testes. Nunca persistido.
    #[serde(skip)]
    pub clock: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ticket {
    pub game: String,
    pub deadline_unix: i64,
    /// Salário ganho pela carteira durante o período do ticket (base da
    /// multa de devolução tardia, §6).
    pub earned_cents: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HourlySession {
    pub game: String,
    pub swaps_used: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrophyRecord {
    pub id: String,
    pub unlocked_unix: i64,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Stats {
    pub ms_served: i64,   // total de ms no cartucho em serviço
    pub rentals: u32,     // locações abertas (avulsa + ticket)
    pub swaps: u32,
    pub purchases: u32,
    pub returns_in_time: u32,
    pub returns_late: u32,
    pub on_time_streak: u32,
    pub rental_days: u32, // dias distintos com locação
    pub last_rental_day: i64,
    pub blackout_recoveries: u32,
}

impl Default for Persisted {
    fn default() -> Self {
        Self {
            version: 1,
            wallet_cents: 0,
            lifetime_earned_cents: 0,
            lifetime_spent_cents: 0,
            intro_done: false,
            service_pending_ms: 0,
            owned: Vec::new(),
            service_game: None,
            active_ticket: None,
            hourly: None,
            trophies: Vec::new(),
            stats: Stats::default(),
            rentals_per_game: HashMap::new(),
            coupons: 0,
            free_session: false,
            clock: None,
        }
    }
}

impl Persisted {
    /// Escrita atômica: .part no mesmo diretório + rename (padrão
    /// dat_update.rs:40).
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(self)?;
        let part = path.with_extension("json.part");
        std::fs::write(&part, text.as_bytes())?;
        std::fs::rename(&part, path)?;
        Ok(())
    }

    /// Leitura tolerante: corrupto/faltando → default (padrão load_store,
    /// catalog.rs:336-345).
    pub fn load(path: &std::path::Path) -> Persisted {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Persisted::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_then_load_roundtrips() {
        let mut st = Persisted::default();
        st.intro_done = true;
        st.owned.push("sha-a".into());
        st.service_game = Some("sha-a".into());
        st.wallet_cents = 1234;
        st.rentals_per_game.insert("sha-a".into(), 3);
        st.coupons = 1;
        let dir = std::env::temp_dir().join("locadora-test-roundtrip");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("locadora.json");
        st.save(&path).unwrap();
        let back = Persisted::load(&path);
        assert_eq!(back, st);
    }

    #[test]
    fn corrupt_file_defaults_without_panicking() {
        let dir = std::env::temp_dir().join("locadora-test-corrupt");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("locadora.json");
        std::fs::write(&path, b"{ nis").unwrap();
        assert_eq!(Persisted::load(&path), Persisted::default());
    }

    #[test]
    fn missing_file_defaults() {
        let dir = std::env::temp_dir().join("locadora-test-missing");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nope").join("locadora.json");
        assert_eq!(Persisted::load(&path), Persisted::default());
    }

    #[test]
    fn save_is_atomic_no_part_file_left_behind() {
        let dir = std::env::temp_dir().join("locadora-test-atomic");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("locadora.json");
        Persisted::default().save(&path).unwrap();
        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(std::result::Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!names.iter().any(|n| n.contains(".part")), "{names:?}");
    }

    #[test]
    fn clock_is_never_persisted() {
        let mut st = Persisted::default();
        st.clock = Some(123);
        let dir = std::env::temp_dir().join("locadora-test-clock");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("locadora.json");
        st.save(&path).unwrap();
        let back = Persisted::load(&path);
        assert_eq!(back.clock, None);
    }
}
