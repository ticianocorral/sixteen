//! Regras da locadora: coleção, cartucho em serviço, inseribilidade,
//! economia (salário, hora avulsa, blackout, ticket, multa tardia),
//! compra, sorteio de boas-vindas, cartaz da semana e reconciliação.

use std::collections::HashSet;

use crate::money::Money;
use crate::prices::Prices;
use crate::store::{HourlySession, Persisted, Ticket};

/// Modo de uma sessão de jogo (o runner consulta).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub enum RentMode {
    #[default]
    Service,
    Hourly,
    Ticket,
}

/// Por que um cartucho não entra no slot (design §3/§7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertCheck {
    Ok,
    NotRented,
    TicketExpired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectError {
    NotOwned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HourlyError {
    EmptyWallet,
    GameOwned,
    NoSwapsLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    InsufficientFunds,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TicketEvent {
    Nothing,
    AutoReturned { fined: i64 },
}

pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

impl Persisted {
    pub fn owns(&self, game: &str) -> bool {
        self.owned.iter().any(|g| g == game)
    }

    /// O cartucho entra na máquina? Coleção sempre; ticket dentro do prazo;
    /// cartucho da sessão de hora avulsa.
    pub fn insert_check(&self, game: &str) -> InsertCheck {
        if self.owns(game) {
            return InsertCheck::Ok;
        }
        if let Some(t) = &self.active_ticket {
            if t.game == game {
                return if self.now() >= t.deadline_unix {
                    InsertCheck::TicketExpired
                } else {
                    InsertCheck::Ok
                };
            }
        }
        if let Some(h) = &self.hourly {
            if h.game == game {
                return InsertCheck::Ok;
            }
        }
        InsertCheck::NotRented
    }

    /// Elege o cartucho em serviço — só da coleção (design §5).
    pub fn elect_service(&mut self, game: &str) -> Result<(), ElectError> {
        if !self.owns(game) {
            return Err(ElectError::NotOwned);
        }
        self.service_game = Some(game.to_string());
        Ok(())
    }

    /// Credita salário (rate neutro, sem fim de semana). Sem drift: pendência
    /// em ms, centavos chão sobre o total pendente.
    pub fn credit_service_ms(&mut self, ms: i64, p: &Prices) -> i64 {
        self.credit_with_rate(ms, p.salary_per_hour)
    }

    /// Crédito com dia explícito — o runner passa o unix do tick, e o
    /// fim de semana paga o dobro (§17).
    pub fn credit_service_ms_at(&mut self, ms: i64, p: &Prices, unix: i64) -> i64 {
        let rate = p.salary_rate_at(unix);
        self.credit_with_rate(ms, rate)
    }

    fn credit_with_rate(&mut self, ms: i64, rate: i64) -> i64 {
        self.stats.ms_served += ms;
        self.service_pending_ms += ms;
        let cents = Money::cents_from_ms(self.service_pending_ms, rate);
        if cents == 0 {
            return 0;
        }
        self.service_pending_ms -= cents * 3_600_000 / rate;
        self.wallet_cents += cents;
        self.lifetime_earned_cents += cents;
        if let Some(t) = &mut self.active_ticket {
            t.earned_cents += cents; // base da multa tardia (§6)
        }
        cents
    }

    /// Hora avulsa: só jogo do acervo (não comprado), com caixa no mínimo
    /// 1 minuto de freguesia — ou cupom de locação grátis (§17).
    pub fn start_hourly(&mut self, game: &str, p: &Prices) -> Result<(), HourlyError> {
        if self.owns(game) {
            return Err(HourlyError::GameOwned);
        }
        let first_min = p.hourly_cents_for_ms(60_000).max(1);
        if self.wallet_cents < first_min {
            if self.coupons > 0 {
                self.coupons -= 1;
                self.free_session = true;
            } else {
                return Err(HourlyError::EmptyWallet);
            }
        }
        self.hourly = Some(HourlySession { game: game.into(), swaps_used: 0 });
        self.stats.rentals += 1;
        *self.rentals_per_game.entry(game.into()).or_insert(0) += 1;
        self.bump_rental_day();
        Ok(())
    }

    /// Fecha a sessão debitando exatamente o custo dos ms jogados (chão).
    /// Sessão paga por cupom não debita nada.
    pub fn end_hourly(&mut self, ms_jogados: i64, p: &Prices) -> i64 {
        if self.hourly.take().is_none() {
            return 0;
        }
        if self.free_session {
            self.free_session = false;
            return 0;
        }
        let cost = p.hourly_cents_for_ms(ms_jogados).min(self.wallet_cents);
        self.wallet_cents -= cost;
        self.lifetime_spent_cents += cost;
        cost
    }

    /// Corta? custo projetado da sessão + 1s > carteira. O runner consulta
    /// a cada segundo; a baixa real acontece no `end_hourly`.
    pub fn hourly_blackout_at(&self, ms_jogados: i64, p: &Prices) -> bool {
        if self.free_session {
            return false;
        }
        let projetado = p.hourly_cents_for_ms(ms_jogados) + p.hourly_cents_for_ms(1000);
        projetado > self.wallet_cents
    }

    /// Troca o cartucho DENTRO da hora avulsa, limite 2 (design §6).
    pub fn swap_hourly(&mut self, nxt: &str) -> Result<(), HourlyError> {
        let Some(h) = &mut self.hourly else {
            return Err(HourlyError::EmptyWallet);
        };
        if h.swaps_used >= 2 {
            return Err(HourlyError::NoSwapsLeft);
        }
        h.game = nxt.to_string();
        h.swaps_used += 1;
        self.stats.swaps += 1;
        Ok(())
    }

    /// Ticket 48h: paga adiantado (ou usa cupom), prazo real em unix.
    /// Devolve o unix do início do ticket.
    pub fn start_ticket(&mut self, game: &str, p: &Prices) -> Result<i64, StoreError> {
        let now = self.now();
        let mut free = false;
        if self.coupons > 0 && self.wallet_cents < p.ticket_48h {
            self.coupons -= 1;
            self.free_session = true;
            free = true;
        }
        if !free {
            let back = Money(self.wallet_cents)
                .checked_sub(Money(p.ticket_48h))
                .ok_or(StoreError::InsufficientFunds)?;
            self.wallet_cents = back.0;
            self.lifetime_spent_cents += p.ticket_48h;
        }
        self.active_ticket = Some(Ticket {
            game: game.into(),
            deadline_unix: now + 48 * 3600,
            earned_cents: 0,
        });
        self.stats.rentals += 1;
        *self.rentals_per_game.entry(game.into()).or_insert(0) += 1;
        self.bump_rental_day();
        Ok(now)
    }

    /// Devolve o ticket AGORA. True = atrasada (multa aplicada, §6).
    pub fn return_ticket(&mut self) -> bool {
        let now = self.now();
        match self.active_ticket.take() {
            None => false,
            Some(t) => {
                if now > t.deadline_unix {
                    let _ = self.apply_late_fine(&t);
                    self.stats.returns_late += 1;
                    self.stats.on_time_streak = 0;
                    true
                } else {
                    self.stats.returns_in_time += 1;
                    self.stats.on_time_streak += 1;
                    if self.stats.on_time_streak >= 5 {
                        self.coupons += 1; // cartão de fidelidade (§17)
                        self.stats.on_time_streak = 0;
                    }
                    false
                }
            }
        }
    }

    /// Devolução automática: 24h depois do prazo a locadora leva o cartucho
    /// de volta (§6). Chamar no boot, na entrada do balcão e a cada segundo
    /// com o tubo ligado.
    pub fn enforce_ticket(&mut self) -> TicketEvent {
        let Some(t) = &self.active_ticket else {
            return TicketEvent::Nothing;
        };
        let deadline = t.deadline_unix;
        if self.now() >= deadline + 24 * 3600 {
            let ticket = self.active_ticket.take().expect("checado acima");
            let fined = self.apply_late_fine(&ticket);
            self.stats.returns_late += 1;
            self.stats.on_time_streak = 0;
            TicketEvent::AutoReturned { fined }
        } else {
            TicketEvent::Nothing
        }
    }

    /// Multa: 30% do ganho do período, limitado à carteira (nunca negativa).
    fn apply_late_fine(&mut self, t: &Ticket) -> i64 {
        let fine = (t.earned_cents * 30 / 100).min(self.wallet_cents);
        self.wallet_cents -= fine;
        self.lifetime_spent_cents += fine;
        fine
    }

    /// Compra na loja: permanente, caixa nova (design §7).
    pub fn buy(&mut self, game: &str, p: &Prices) -> Result<(), StoreError> {
        let back = Money(self.wallet_cents)
            .checked_sub(Money(p.purchase))
            .ok_or(StoreError::InsufficientFunds)?;
        self.wallet_cents = back.0;
        self.lifetime_spent_cents += p.purchase;
        self.owned.push(game.into());
        self.stats.purchases += 1;
        if self.active_ticket.as_ref().map(|t| t.game == game).unwrap_or(false) {
            self.active_ticket = None; // comprou o que estava locado
        }
        Ok(())
    }

    pub fn can_afford_purchase(&self, p: &Prices) -> bool {
        self.wallet_cents >= p.purchase
    }

    /// Sincroniza com o acervo real (design §16). Devolve os sha1s que
    /// saíram. Sem reembolso: carteira, troféus e stats globais persistem.
    pub fn reconcile_acervo(&mut self, present: &[String]) -> Vec<String> {
        let mural: HashSet<&str> = present.iter().map(|s| s.as_str()).collect();
        let mut removidos = Vec::new();
        self.owned.retain(|g| {
            if mural.contains(g.as_str()) {
                true
            } else {
                removidos.push(g.clone());
                false
            }
        });
        if self.service_game.as_deref().is_some_and(|g| !mural.contains(g)) {
            self.service_game = None; // cargo vago
        }
        if self
            .active_ticket
            .as_ref()
            .is_some_and(|t| !mural.contains(t.game.as_str()))
        {
            self.active_ticket = None; // sumiço: sem crédito de prazo
        }
        if self
            .hourly
            .as_ref()
            .is_some_and(|h| !mural.contains(h.game.as_str()))
        {
            self.hourly = None; // sessão encerrada
        }
        removidos
    }

    /// Aplica a escolha do sorteio único: nasce comprado e em serviço (§8).
    pub fn apply_intro(&mut self, trio: &[String], chosen: &str) -> Result<(), &'static str> {
        if self.intro_done {
            return Err("sorteio único da vida");
        }
        if !trio.iter().any(|g| g == chosen) {
            return Err("fora do trio");
        }
        self.intro_done = true;
        self.owned.push(chosen.to_string());
        self.service_game = Some(chosen.to_string());
        Ok(())
    }

    fn bump_rental_day(&mut self) {
        let day = self.now() / 86_400;
        if day != self.stats.last_rental_day {
            self.stats.last_rental_day = day;
            self.stats.rental_days += 1;
        }
    }

    /// Relógio injetável para testes (nunca persistido, ver store.rs).
    pub fn set_clock_for_test(&mut self, unix: i64) {
        self.clock = Some(unix);
    }

    fn now(&self) -> i64 {
        self.clock.unwrap_or_else(now_unix)
    }
}

/// Sorteio de boas-vindas: até 3 sha1s distintas, determinístico pela semente.
pub fn draw_trio(acervo: &[String], seed: u64) -> Vec<String> {
    let mut rng = if seed == 0 { 0xA7C2_5D1E_398B_9F41 } else { seed };
    let mut out = Vec::new();
    let mut pool: Vec<String> = acervo.to_vec();
    if pool.is_empty() {
        return out;
    }
    for _ in 0..3.min(pool.len()) {
        rng ^= rng << 13;
        rng ^= rng >> 7;
        rng ^= rng << 17; // xorshift64
        let i = (rng as usize) % pool.len();
        out.push(pool.remove(i));
    }
    out
}

/// Cartaz da semana (§17): seed pelo nº da semana, determinístico.
pub fn poster_of_week(acervo: &[String], unix: i64) -> String {
    let week = unix.div_euclid(604_800) as u64;
    draw_trio(acervo, week ^ 0x5EED_BEEF).first().cloned().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(owned: &[&str], service: Option<&str>) -> Persisted {
        Persisted {
            owned: owned.iter().map(|s| s.to_string()).collect(),
            service_game: service.map(|s| s.to_string()),
            ..Persisted::default()
        }
    }

    // --- coleção / inseribilidade (Task 3) ---

    #[test]
    fn collection_goes_in_free() {
        let s = st(&["aaa", "bbb"], Some("aaa"));
        assert_eq!(s.insert_check("aaa"), InsertCheck::Ok);
        assert_eq!(s.insert_check("bbb"), InsertCheck::Ok);
        assert_eq!(s.insert_check("stranger"), InsertCheck::NotRented);
    }

    #[test]
    fn ticket_inside_deadline_inserts() {
        let mut s = st(&["aaa"], Some("aaa"));
        s.active_ticket = Some(Ticket { game: "tkt".into(), deadline_unix: 999_999_999_999, earned_cents: 0 });
        assert_eq!(s.insert_check("tkt"), InsertCheck::Ok);
        s.active_ticket = Some(Ticket { game: "tkt".into(), deadline_unix: 0, earned_cents: 0 });
        assert_eq!(s.insert_check("tkt"), InsertCheck::TicketExpired);
    }

    #[test]
    fn hourly_session_only_covers_its_own_cart() {
        let mut s = st(&["aaa"], Some("aaa"));
        s.hourly = Some(HourlySession { game: "cur".into(), swaps_used: 0 });
        assert_eq!(s.insert_check("cur"), InsertCheck::Ok);
        assert_eq!(s.insert_check("other"), InsertCheck::NotRented);
    }

    #[test]
    fn service_election_is_collection_only() {
        let mut s = st(&["aaa"], None);
        assert!(s.elect_service("aaa").is_ok());
        assert_eq!(s.service_game.as_deref(), Some("aaa"));
        assert_eq!(s.elect_service("nope"), Err(ElectError::NotOwned));
        assert_eq!(s.service_game.as_deref(), Some("aaa"));
    }

    #[test]
    fn return_ticket_clears_state() {
        let mut s = st(&["aaa"], Some("aaa"));
        s.active_ticket = Some(Ticket { game: "tkt".into(), deadline_unix: 1, earned_cents: 0 });
        assert!(s.return_ticket()); // prazo 1 < now: atrasada
        assert_eq!(s.active_ticket, None);
        assert_eq!(s.insert_check("tkt"), InsertCheck::NotRented);
    }

    // --- economia (Task 4) ---

    #[test]
    fn salary_credits_per_ms_without_drift() {
        let mut s = Persisted::default();
        let p = Prices::default();
        assert_eq!(s.credit_service_ms(60 * 60_000, &p), 500);
        assert_eq!(s.credit_service_ms(30_000, &p), 4);
        assert_eq!(s.credit_service_ms(30_000, &p), 4); // pendência de 1.2s + 30s
        assert_eq!(s.wallet_cents, 508);
        assert_eq!(s.stats.ms_served, 61 * 60_000);
    }

    #[test]
    fn hourly_end_debits_exact_session_cost() {
        let mut s = Persisted::default();
        s.wallet_cents = 800;
        let p = Prices::default();
        s.start_hourly("xxx", &p).unwrap();
        s.end_hourly(1_800_000, &p);
        assert_eq!(s.wallet_cents, 400);
        assert_eq!(s.stats.rentals, 1);
        assert!(s.hourly.is_none());
    }

    #[test]
    fn hourly_needs_wallet_and_nonowned_game() {
        let mut s = Persisted::default();
        let p = Prices::default();
        assert_eq!(s.start_hourly("xxx", &p), Err(HourlyError::EmptyWallet));
        s.wallet_cents = 13; // 1 minuto de saldo (chão)
        s.owned.push("aaa".into());
        assert_eq!(s.start_hourly("aaa", &p), Err(HourlyError::GameOwned));
        assert_eq!(s.start_hourly("xxx", &p), Ok(()));
        assert!(s.hourly.is_some());
    }

    #[test]
    fn blackout_when_projected_cost_passes_wallet() {
        let s = Persisted { wallet_cents: 400, ..Persisted::default() };
        let p = Prices::default();
        assert!(s.hourly_blackout_at(31 * 60_000, &p)); // 413 > 400
        assert!(!s.hourly_blackout_at(29 * 60_000, &p)); // 387 <= 400
    }

    #[test]
    fn swaps_are_two_per_hourly_session() {
        let mut s = Persisted::default();
        s.wallet_cents = 3000;
        let p = Prices::default();
        s.start_hourly("a", &p).unwrap();
        assert_eq!(s.swap_hourly("b"), Ok(()));
        assert_eq!(s.swap_hourly("c"), Ok(()));
        assert_eq!(s.swap_hourly("d"), Err(HourlyError::NoSwapsLeft));
        assert_eq!(s.stats.swaps, 2);
        s.end_hourly(600_000, &p);
    }

    #[test]
    fn ticket_costs_upfront_with_real_deadline() {
        let mut s = Persisted::default();
        let p = Prices::default();
        s.wallet_cents = 1200;
        let start = s.start_ticket("tkt", &p).unwrap();
        assert_eq!(s.wallet_cents, 0);
        assert_eq!(
            s.active_ticket.as_ref().unwrap().deadline_unix,
            start + 48 * 3600
        );
        assert_eq!(s.start_ticket("tkt", &p), Err(StoreError::InsufficientFunds));
    }

    #[test]
    fn buy_requires_funds_and_unlocks() {
        let mut s = Persisted::default();
        let p = Prices::default();
        s.wallet_cents = 5999;
        assert_eq!(s.buy("gg", &p), Err(StoreError::InsufficientFunds));
        s.wallet_cents = 6000;
        s.buy("gg", &p).unwrap();
        assert!(s.owns("gg"));
        assert_eq!(s.wallet_cents, 0);
        assert_eq!(s.stats.purchases, 1);
    }

    // --- devolução tardia (Task 4b) ---

    #[test]
    fn ticket_accumulates_earnings_of_the_period() {
        let mut s = Persisted::default();
        let p = Prices::default();
        s.wallet_cents = 5000;
        s.start_ticket("tkt", &p).unwrap();
        s.credit_service_ms(60 * 60_000, &p);
        assert_eq!(s.active_ticket.as_ref().unwrap().earned_cents, 500);
    }

    #[test]
    fn late_manual_return_fines_30_percent() {
        let mut s = Persisted::default();
        let p = Prices::default();
        s.wallet_cents = 5000;
        let start = s.start_ticket("tkt", &p).unwrap();
        s.credit_service_ms(2 * 60 * 60_000, &p); // ganhou 1000 no período
        s.set_clock_for_test(start + 48 * 3600 + 25 * 3600);
        assert!(s.return_ticket());
        assert_eq!(s.stats.returns_late, 1);
        assert_eq!(s.stats.on_time_streak, 0);
        // 5000 - 1200 (ticket) + 1000 (ganhos) - 300 (multa 30%)
        assert_eq!(s.wallet_cents, 4500);
    }

    #[test]
    fn auto_return_kicks_in_at_deadline_plus_24h() {
        let mut s = Persisted::default();
        let p = Prices::default();
        s.wallet_cents = 5000;
        let start = s.start_ticket("tkt", &p).unwrap();
        s.credit_service_ms(60 * 60_000, &p); // 500 no período
        s.set_clock_for_test(start + 48 * 3600 + 23 * 3600);
        assert_eq!(s.enforce_ticket(), TicketEvent::Nothing);
        s.set_clock_for_test(start + 48 * 3600 + 24 * 3600);
        assert_eq!(s.enforce_ticket(), TicketEvent::AutoReturned { fined: 150 });
        assert!(s.active_ticket.is_none());
        assert_eq!(s.stats.returns_late, 1);
        assert_eq!(s.wallet_cents, 5000 - 1200 + 500 - 150); // 4150
    }

    #[test]
    fn fine_never_pushes_wallet_negative() {
        let mut s = Persisted::default();
        let p = Prices::default();
        s.wallet_cents = 6000;
        let start = s.start_ticket("tkt", &p).unwrap();
        s.credit_service_ms(10 * 60 * 60_000, &p); // ganhou 5000 no período
        s.set_clock_for_test(start + 100 * 3600);
        s.enforce_ticket();
        assert!(s.wallet_cents >= 0);
        assert_eq!(s.wallet_cents, 6000 - 1200 + 5000 - 1500); // 8300
    }

    #[test]
    fn on_time_streak_grants_coupon_at_five() {
        let mut s = Persisted::default();
        let p = Prices::default();
        for i in 0..5 {
            s.wallet_cents = 1200;
            let start = s.start_ticket("tkt", &p).unwrap();
            s.set_clock_for_test(start + 10); // dentro do prazo
            assert!(!s.return_ticket());
            s.set_clock_for_test(start + 90_000 + i * 100);
        }
        assert_eq!(s.coupons, 1);
        assert_eq!(s.stats.on_time_streak, 0);
    }

    // --- sorteio, cartaz, reconciliação (Tasks 5/3b/5b) ---

    #[test]
    fn draws_three_distinct_from_sha1s() {
        let acervo: Vec<String> = (0..10).map(|i| format!("sha-{i}")).collect();
        let trio = draw_trio(&acervo, 42);
        assert_eq!(trio.len(), 3);
        for g in &trio {
            assert!(acervo.contains(g));
        }
        assert_ne!(trio[0], trio[1]);
        assert_ne!(trio[1], trio[2]);
        assert_eq!(draw_trio(&acervo, 42), draw_trio(&acervo, 42));
    }

    #[test]
    fn intro_picks_from_trio_once_in_life() {
        let mut s = Persisted::default();
        let trio = vec!["a".into(), "b".into(), "c".into()];
        assert!(s.apply_intro(&trio, "c").is_ok());
        assert!(s.intro_done && s.owns("c"));
        assert_eq!(s.service_game.as_deref(), Some("c"));
        assert_eq!(s.apply_intro(&trio, "zzz"), Err("sorteio único da vida"));
        let mut s2 = Persisted::default();
        assert_eq!(s2.apply_intro(&trio, "zzz"), Err("fora do trio"));
    }

    #[test]
    fn reconcile_removes_missing_from_collection_and_sessions() {
        let mut s = Persisted::default();
        s.intro_done = true;
        s.owned = vec!["fica".into(), "sai".into()];
        s.service_game = Some("sai".into());
        s.active_ticket = Some(Ticket { game: "sai".into(), deadline_unix: 9_999, earned_cents: 0 });
        s.wallet_cents = 777;
        let removidos = s.reconcile_acervo(&["fica".into(), "outro".into()]);
        assert_eq!(s.owned, vec!["fica".to_string()]);
        assert_eq!(s.service_game, None);
        assert_eq!(s.active_ticket, None);
        assert_eq!(s.stats.returns_in_time, 0); // sumiço não é devolução
        assert_eq!(s.wallet_cents, 777); // carteira do freguês fica
        assert_eq!(removidos, vec!["sai".to_string()]);
        assert!(s.reconcile_acervo(&["fica".into()]).is_empty()); // idempotente
    }

    #[test]
    fn reconcile_kills_hourly_session_of_removed_cart() {
        let mut s = Persisted::default();
        s.hourly = Some(HourlySession { game: "gone".into(), swaps_used: 1 });
        s.stats.swaps = 1;
        s.reconcile_acervo(&["fica".into()]);
        assert_eq!(s.hourly, None);
        assert_eq!(s.stats.swaps, 1); // histórico global persiste
    }

    #[test]
    fn rentals_per_game_counts_each_open() {
        let mut s = Persisted::default();
        s.wallet_cents = 30_000;
        let p = Prices::default();
        s.start_hourly("a", &p).unwrap();
        s.end_hourly(60_000, &p);
        s.start_hourly("a", &p).unwrap();
        s.end_hourly(60_000, &p);
        s.start_ticket("b", &p).unwrap();
        assert_eq!(s.rentals_per_game.get("a"), Some(&2));
        assert_eq!(s.rentals_per_game.get("b"), Some(&1));
    }

    #[test]
    fn weekend_pays_double_salary() {
        let mut s = Persisted::default();
        let p = Prices::default();
        // 2026-10-24 é sábado: 12:00 UTC → unix 1792843200
        assert_eq!(p.salary_rate_at(1_792_843_200), 1000);
        // 2026-10-19 é segunda: 12:00 UTC → unix 1792411200
        assert_eq!(p.salary_rate_at(1_792_411_200), 500);
        s.credit_service_ms_at(60 * 60_000, &p, 1_792_843_200);
        assert_eq!(s.wallet_cents, 1000); // 1h de sábado
    }

    #[test]
    fn coupon_pays_the_rental_when_wallet_is_empty() {
        let mut s = Persisted::default();
        s.coupons = 1;
        let p = Prices::default();
        assert_eq!(s.start_hourly("x", &p), Ok(()));
        assert_eq!(s.coupons, 0);
        s.end_hourly(60_000, &p); // sessão grátis
        assert_eq!(s.wallet_cents, 0);
    }

    #[test]
    fn weekly_poster_is_deterministic_by_week() {
        let acervo: Vec<String> = (0..8).map(|i| format!("s{i}")).collect();
        let a = poster_of_week(&acervo, 1_791_816_000);
        let b = poster_of_week(&acervo, 1_791_816_000 + 3600);
        assert_eq!(a, b); // mesma semana → mesmo cartaz
        assert!(acervo.contains(&a));
    }
}
