# Modo Locadora — Plano de Implementação

> **For agentic workers:** implement this plan task-by-task, in order, checking
> off each step as it lands. Steps use checkbox (`- [ ]`) syntax for tracking.
> Execute inline, um checkpoint de revisão por task (as tasks compartilham
> estado sequencial — não paralelizar).

**Goal:** implementar o Modo Locadora (design em `docs/design-modo-locadora.md`):
locadora de SNES fictícia, opt-in, com carteira em centavos, cartucho "em
serviço" que paga por hora, hora avulsa com 2 trocas, ticket de 48h corridas
e loja de compras permanentes — cartucho gasto/novo com cerimônia de compra.

**Architecture:** nova crate pura `crates/locadora` (dinheiro, regras de
locação, troféus, persistência JSON atômica — zero SDL), integrada no
`crates/app` pelos padrões existentes: uma tela = `pub fn run(...) ->
Result<Exit>` com loop próprio (como `idle.rs`/`shelf.rs`), desenho por
closures em `Cabinet::*_2d`/`Screen`, avisos no tubo por `push_osd`, tempo
de sessão por `powered_since`/`powered_elapsed` do `runner.rs`, identidade
de jogo por `sha1` (a chave do `library.json`).

**Tech Stack:** Rust 2021 (workspace), serde/serde_json, thiserror, SDL3
só via `crates/platform` (UI do app; core untouched).

**Referências (ler antes de cada task de app):**
- Tela inicial/routing: `crates/app/src/idle.rs:45-57` (`IdleExit`),
  `idle.rs:563-610` (hit do painel), `crates/app/src/bin/sixteen.rs:288-423`
  ('app:'/'shelf:' loops e rotas).
- Padrão de tela-de-acervo: `crates/app/src/shelf.rs:1033` (`run`),
  `shelf.rs:118-145` (`Pick`), `shelf.rs:477-499` (`find_local_art`),
  `shelf.rs:33-36`. Constantes TILE/GAP.
- Tempo: `crates/app/src/runner.rs:1436-1444`, `runner.rs:472`,
  `runner.rs:208-215` (`add_playtime`), `GameExit` `runner.rs:48-56`,
  power on/off `runner.rs:1701-1734`.
- Catálogo: `crates/domain/src/catalog.rs:102-131` (`RomRow`, sha1),
  `Catalog::list` (catalog.rs:286).
- Cfg: `crates/app/src/config.rs:19-57` (`Config`), `Raw` (60-79),
  `to_toml` (193-230).
- Caminhos: `crates/app/src/dirs.rs` (`app_root`, `config_dir`,
  `library_path`).
- OSD/SFX/imagem: `Cabinet::push_osd` (cabinet.rs:1131),
  `Cabinet::set_image(id, w, h, rgba)` (cabinet.rs:2040),
  `crates/app/src/sfx.rs` (`Sfx::Achievement`).
- Escrita atômica de referência: `crates/app/src/dat_update.rs:40` (`.part`
  + rename).

---

## Fase A — crate de domínio, TDD estrito

### Task 1: Scaffold, `Money` (centavos) e `Prices`

**Files:**
- Modify: `Cargo.toml` (root, workspace members)
- Create: `crates/locadora/Cargo.toml`
- Create: `crates/locadora/src/lib.rs`
- Create: `crates/locadora/src/money.rs`
- Create: `crates/locadora/src/prices.rs`

- [ ] **Step 1: Registrar a crate no workspace e dependências**

Root `Cargo.toml`, em ` members`:

```toml
members = [
    "crates/emulation",
    "crates/platform",
    "crates/domain",
    "crates/ra",
    "crates/ntsc",
    "crates/app",
    "crates/locadora",
]
```

`crates/locadora/Cargo.toml`:

```toml
[package]
name = "sixteen-locadora"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
```

`crates/locadora/src/lib.rs`:

```rust
mod money;
mod prices;
```

- [ ] **Step 2: Escrever os testes falhando**

`crates/locadora/src/money.rs` — primeiro só os testes:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct Money(pub i64); // centavos

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_grows_with_add_assign() {
        let mut w = Money::default();
        w += Money(500);
        assert_eq!(w, Money(500));
        w += Money(500);
        assert_eq!(w, Money(1000));
    }

    #[test]
    fn checked_sub_fails_without_funds() {
        let w = Money(800);
        assert_eq!(w.checked_sub(Money(1200)), None);
        assert_eq!(w.checked_sub(Money(800)), Some(Money(0)));
    }

    #[test]
    fn display_uses_symbol_grouping_and_comma() {
        assert_eq!(Money(120_034).fmt("R$"), "R$ 1.200,34");
        assert_eq!(Money(5).fmt("US$"), "US$ 0,05");
        assert_eq!(Money(0).fmt("R$"), "R$ 0,00");
        assert_eq!(Money(-1).fmt("R$"), "-R$ 0,01");
    }

    #[test]
    fn serde_roundtrip_is_plain_cents() {
        let m = Money(999);
        let s = serde_json::to_string(&m).unwrap();
        assert_eq!(s, "999");
        assert_eq!(serde_json::from_str::<Money>(&s).unwrap(), m);
    }
}
```

`crates/locadora/src/prices.rs` — primeiro só os testes:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Money;

    #[test]
    fn defaults_match_the_design() {
        let p = Prices::default();
        assert_eq!(p.salary_per_hour, 500);  // R$ 5,00/h no cartucho em serviço
        assert_eq!(p.hourly_per_hour, 800);  // R$ 8,00/h na hora avulsa
        assert_eq!(p.ticket_48h, 1200);      // R$ 12,00 o ticket de 48h
        assert_eq!(p.purchase, 6000);        // R$ 60,00 a compra
    }

    #[test]
    fn salary_cents_per_ms_is_exact_floor() {
        let p = Prices::default();
        assert_eq!(p.salary_cents_for_ms(7_200_000), 1000);  // 2h = R$ 10
        assert_eq!(p.salary_cents_for_ms(60_000), 8);        // 1min = 8.33 chão 8
        assert_eq!(p.salary_cents_for_ms(0), 0);
    }

    #[test]
    fn hourly_cents_per_ms_is_exact_floor() {
        let p = Prices::default();
        assert_eq!(p.hourly_cents_for_ms(3_600_000), 800);   // 1h = R$ 8
        assert_eq!(p.hourly_cents_for_ms(1_800_000), 400);   // 30min
        assert_eq!(p.hourly_cents_for_ms(900_000), 200);     // 15min
    }

    #[test]
    fn huge_ms_does_not_overflow() { // i64 em centavos: saturar é honesto
        let p = Prices::default();
        assert_eq!(p.salary_cents_for_ms(i64::MAX), i64::MAX);
    }
}
```

- [ ] **Step 3: Rodar para ver falhar**

```bash
cargo test -p sixteen-locadora
```

Esperado: FAIL — `Prices` e `salary_cents_for_ms`/`hourly_cents_for_ms` não
existem (Money compila pois a struct já está no arquivo).

- [ ] **Step 4: Implementar o mínimo**

`crates/locadora/src/money.rs` (acima dos testes, completa):

```rust
use core::ops::{AddAssign, Sub};
use serde::{Deserialize, Serialize};

/// Dinheiro fictício da locadora, em centavos. Nunca é real.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Money(pub i64);

impl AddAssign for Money {
    fn add_assign(&mut self, rhs: Self) { self.0 += rhs.0; }
}

impl Sub for Money {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self { Money(self.0 - rhs.0) }
}

impl Money {
    /// Subtrai com saldo checado: None quando não cobre.
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        (self.0 >= rhs.0).then(|| Money(self.0 - rhs.0))
    }

    /// Centavos por milissegundos com teto exato e chão inteiro.
    /// (ms * cents/h / 3.600_000); satura em i64::MAX.
    pub fn cents_from_ms(ms: i64, cents_per_hour: i64) -> i64 {
        let per_ms = cents_per_hour / 3_600_000; // 0 na prática
        let _ = per_ms;
        let raw = match ms.checked_mul(cents_per_hour) {
            Some(v) => v,
            None => return i64::MAX,
        };
        raw / 3_600_000
    }

    /// Formato com ponto de milhar e vírgula decimal, estilo ficha de balcão.
    pub fn fmt(&self, symbol: &str) -> String {
        let (sign, c) = if self.0 < 0 { ("-", -self.0) } else { ("", self.0) };
        let whole = c / 100;
        let frac = c % 100;
        let digits = whole.to_string();
        let mut grouped = String::new();
        for (i, ch) in digits.chars().enumerate() {
            let rest = digits.len() - i;
            if i > 0 && rest % 3 == 0 { grouped.push('.'); }
            grouped.push(ch);
        }
        format!("{sign}{symbol} {grouped},{frac:02}")
    }
}
```

`crates/locadora/src/prices.rs` (acima dos testes):

```rust
use serde::{Deserialize, Serialize};
use crate::money::Money;

/// Valores do design §4 (centavos). Todos editáveis no cfg do app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prices {
    pub salary_per_hour: i64,
    pub hourly_per_hour: i64,
    pub ticket_48h: i64,
    pub purchase: i64,
}

impl Default for Prices {
    fn default() -> Self {
        Self { salary_per_hour: 500, hourly_per_hour: 800,
               ticket_48h: 1200, purchase: 6000 }
    }
}

impl Prices {
    pub fn salary_cents_for_ms(&self, ms: i64) -> i64 {
        Money::cents_from_ms(ms, self.salary_per_hour)
    }
    pub fn hourly_cents_for_ms(&self, ms: i64) -> i64 {
        Money::cents_from_ms(ms, self.hourly_per_hour)
    }
}
```

- [ ] **Step 5: Rodar até verde**

```bash
cargo test -p sixteen-locadora
```

Esperado: PASS nos dois módulos; `cargo test --workspace` segue verde.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/locadora
git commit -m "feat(locadora): crate de domínio — Money em centavos e Prices do design"
```

---

### Task 2: Persistência — `locadora.json` atômico e tolerante

**Files:**
- Create: `crates/locadora/src/errors.rs`
- Create: `crates/locadora/src/store.rs`
- Modify: `crates/locadora/src/lib.rs`

- [ ] **Step 1: Escrever os testes falhando**

`crates/locadora/src/store.rs` começando só com a semântica dos dados
(mais os testes — o `save`/`load` ainda não existem):

```rust
use serde::{Deserialize, Serialize};

/// Estado inteiro do modo, persistido em config/locadora.json (design §10).
/// Chave de jogo = sha1 da ROM (o mesmo do library.json).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Persisted {
    pub version: u32,
    pub wallet_cents: i64,
    pub lifetime_earned_cents: i64,
    pub lifetime_spent_cents: i64,
    pub intro_done: bool,
    pub service_pending_ms: i64,   // ms jogados ainda não convertidos em centavos
    pub owned: Vec<String>,        // sha1s da coleção (sorteado + comprados)
    pub service_game: Option<String>,
    pub active_ticket: Option<Ticket>,
    pub hourly: Option<HourlySession>,
    pub trophies: Vec<TrophyRecord>,
    pub stats: Stats,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ticket { pub game: String, pub deadline_unix: i64 }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HourlySession { pub game: String, pub swaps_used: u32 }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrophyRecord { pub id: String, pub unlocked_unix: i64 }

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Stats {
    pub ms_served: i64,          // total de ms no cartucho em serviço
    pub rentals: u32,            // locações abertas (avulsa + ticket)
    pub swaps: u32,
    pub purchases: u32,
    pub returns_in_time: u32,
    pub rental_days: u32,        // dias distintos com locação (bs bucket por dia)
    pub last_rental_day: i64,
    pub blackout_recoveries: u32,
}

impl Default for Persisted {
    fn default() -> Self {
        Self { version: 1, wallet_cents: 0, lifetime_earned_cents: 0,
               lifetime_spent_cents: 0, intro_done: false, service_pending_ms: 0,
               owned: Vec::new(), service_game: None, active_ticket: None,
               hourly: None, trophies: Vec::new(), stats: Stats::default() }
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
        let dir = std::env::temp_dir().join("locadora-test-roundtrip");
        let path = dir.join("locadora.json");
        let _ = std::fs::remove_dir_all(&dir);
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
        let back = Persisted::load(&path);
        assert_eq!(back, Persisted::default());
    }

    #[test]
    fn missing_file_defaults() {
        let dir = std::env::temp_dir().join("locadora-test-missing");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nope").join("locadora.json");
        let back = Persisted::load(&path); // caminho inexistente: default
        assert_eq!(back, Persisted::default());
    }

    #[test]
    fn save_is_atomic_no_part_file_left_behind() {
        let dir = std::env::temp_dir().join("locadora-test-atomic");
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("locadora.json");
        Persisted::default().save(&path).unwrap();
        let dir_entry = std::fs::read_dir(&dir).unwrap()
            .filter_map(std::result::Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>().join(",");
        assert!(!dir_entry.contains("part"), "restos de .part? {dir_entry}");
    }
}
```

- [ ] **Step 2: Ver a falha**

```bash
cargo test -p sixteen-locadora
```

Esperado: FAIL — `save`/`load` inexistentes.

- [ ] **Step 3: Implementar save/load e o erro**

`crates/locadora/src/errors.rs`:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("io falhou: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialização: {0}")]
    Serde(#[from] serde_json::Error),
}
```

Em `crates/locadora/src/store.rs`, adicionar acima dos testes:

```rust
use crate::errors::Error;

type Result<T> = std::result::Result<T, Error>;

impl Persisted {
    /// Escrita atômica: .part no mesmo diretório + rename (padra dat_update.rs:40).
    pub fn save(&self, path: &std::path::Path) -> Result<()> {
        if let Some(dir) = path.parent() { std::fs::create_dir_all(dir)?; }
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
```

Em `lib.rs` (`crates/locadora/src/lib.rs`):

```rust
mod money;
mod prices;
pub mod errors;
pub mod store;
```

- [ ] **Step 4: Rodar verde + workspace**

```bash
cargo test -p sixteen-locadora && cargo test --workspace
```

- [ ] **Step 5: Commit**

```bash
git add crates/locadora
git commit -m "feat(locadora): locadora.json atômico e tolerante a corruptos"
```

---

### Task 3: Regras — coleção, em serviço, inseribilidade e devolução

**Files:**
- Modify: `crates/locadora/src/rules.rs` (novo)
- Modify: `crates/locadora/src/lib.rs` (declara `pub mod rules;`)

- [ ] **Step 1: Escrever os testes falhando**

`crates/locadora/src/rules.rs` começando só com os testes:

```rust
#[cfg(test)]
mod tests {
    use crate::store::{HourlySession, Persisted, Ticket};

    fn st(owned: &[&str], service: Option<&str>) -> Persisted {
        Persisted {
            owned: owned.iter().map(|s| s.to_string()).collect(),
            service_game: service.map(|s| s.to_string()),
            ..Persisted::default()
        }
    }

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
        s.active_ticket = Some(Ticket { game: "tkt".into(), deadline_unix: 10_000 });
        assert_eq!(s.insert_check("tkt"), InsertCheck::Ok);
        s = st(&["aaa"], Some("aaa"));
        s.active_ticket = Some(Ticket { game: "tkt".into(), deadline_unix: 0 });
        assert_eq!(s.insert_check("tkt"), InsertCheck::TicketExpired);
    }

    #[test]
    fn hourly_session_only_cover_its_own_cart() {
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
        assert_eq!(s.elect_service("nopek"), Err(Collections::NotOwned));
        assert_eq!(s.service_game.as_deref(), Some("aaa"));
    }

    #[test]
    fn return_ticket_clears_state() {
        let mut s = st(&["aaa"], Some("aaa"));
        s.active_ticket = Some(Ticket { game: "tkt".into(), deadline_unix: 1 });
        s.return_ticket();
        assert_eq!(s.active_ticket, None);
        assert_eq!(s.insert_check("tkt"), InsertCheck::NotRented);
    }
}
```

- [ ] **Step 2: Ver a falha**

```bash
cargo test -p sixteen-locadora
```

Esperado: FAIL (tipos/métodos não existem).

- [ ] **Step 3: Implementar**

Em `rules.rs`, acima dos testes:

```rust
use crate::store::{HourlySession, Persisted, Ticket};

/// Por que um cartucho não entra no slot (design §3/§7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InsertCheck { Ok, NotRented, TicketExpired }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Collections { NotOwned }

impl Persisted {
    pub fn owns(&self, game: &str) -> bool {
        self.owned.iter().any(|g| g == game)
    }

    /// O cartucho entra na máquina? Coleção sempre; ticket dentro do prazo;
    /// cartucho da sessão de hora avulsa.
    pub fn insert_check(&self, game: &str) -> InsertCheck {
        if self.owns(game) { return InsertCheck::Ok; }
        if let Some(t) = &self.active_ticket {
            if t.game == game { return InsertCheck::TicketExpired; }
        }
        if let Some(h) = &self.hourly {
            if h.game == game { return InsertCheck::Ok; }
        }
        InsertCheck::NotRented
    }

    /// Elege o cartucho em serviço — só da coleção (design §5).
    pub fn elect_service(&mut self, game: &str) -> Result<(), Collections> {
        if !self.owns(game) { return Err(Collections::NotOwned); }
        self.service_game = Some(game.to_string());
        Ok(())
    }
}
```

Correção aplicada durante a implementação: o teste `insert_check` de ticket
usado em `10_000` depende de prazo comparado com `now` real — a versão
final do método leva prazo em conta via `now_unix()`:

```rust
    pub fn insert_check(&self, game: &str) -> InsertCheck {
        if self.owns(game) { return InsertCheck::Ok; }
        if let Some(t) = &self.active_ticket {
            if t.game == game {
                return if super::now_unix() >= t.deadline_unix
                    { InsertCheck::TicketExpired } else { InsertCheck::Ok };
            }
        }
        if let Some(h) = &self.hourly {
            if h.game == game { return InsertCheck::Ok; }
        }
        InsertCheck::NotRented
    }
```

E os testes da etapa 1 correspondem: `deadline_unix: 999_999_999_999`
(futuro) para o sub-caso positivo, e `deadline_unix: 0` para o vencido.
`return_ticket()` fica em `Persisted` como implementação trivial
(`self.active_ticket = None;`).

Em `lib.rs`, junte:

```rust
pub mod rules;
/// now() compartilhado das regras (tempo real, Cooperated unix, s pro teste).
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
```

- [ ] **Step 4: verde + commit**

```bash
cargo test -p sixteen-locadora
git add crates/locadora
git commit -m "feat(locadora): coleção, eleição de em serviço, inseribilidade e devolução"
```

---

### Task 4: Regras — salário, hora avulsa com trocas, blackout e ticket

**Files:**
- Modify: `crates/locadora/src/rules.rs`
- Modify: `crates/locadora/src/prices.rs` (helper de custo de sessão)

- [ ] **Step 1: Testes falhando (append no `mod tests` de rules.rs)**

```rust
    use crate::money::Money;
    use crate::prices::Prices;
    use crate::rules::{HourlyError, StoreError};

    #[test]
    fn salary_credits_per_ms_without_drift() {
        let mut s = Persisted::default();
        let p = Prices::default();
        assert_eq!(s.credit_service_ms(60 * 60_000, &p), 500);      // 1h
        assert_eq!(s.credit_service_ms(30_000, &p), 4);             // +30s → 4.16 chão 4
        assert_eq!(s.credit_service_ms(30_000, &p), 0);             // +30s de novo: nada (pendência de 2s já paga '')
        assert_eq!(s.wallet_cents, 504);
        assert_eq!(s.stats.ms_served, 60 * 60_000 + 60_000);
    }

    #[test]
    fn hourly_end_debits_exact_session_cost() {
        let mut s = Persisted::default();
        s.wallet_cents = 800;
        let p = Prices::default();
        s.start_hourly("xxx").unwrap();
        s.end_hourly(1_800_000, &p);   // 30min
        assert_eq!(s.wallet_cents, 400);
        assert_eq!(s.stats.rentals, 1);
        assert!(s.hourly.is_none());
    }

    #[test]
    fn hourly_needs_wallet_and_nonowned_game() {
        let mut s = Persisted::default();
        let p = Prices::default();
        assert_eq!(s.start_hourly("xxx"), Err(HourlyError::EmptyWallet));
        s.wallet_cents = 13;   // 1 minuto de saldo (800/60 chão 13)
        s.owned.push("aaa".into());
        assert_eq!(s.start_hourly("aaa"), Err(HourlyError::GameOwned));
        assert_eq!(s.start_hourly("xxx"), Ok(()));
        assert!(s.hourly.is_some());
    }

    #[test]
    fn blackout_is_wallet_smaller_than_week_of_cost() {
        let mut s = Persisted::default();
        s.wallet_cents = 400;
        let p = Prices::default();
        // custo de 31min = 413 > 400 → corta
        assert!(s.hourly_blackout_at(31 * 60_000, &p));
        // custo de 29min = 387 ≤ 400 → segue
        assert!(!s.hourly_blackout_at(29 * 60_000, &p));
    }

    #[test]
    fn swaps_are_two_per_hourly_session() {
        let mut s = Persisted::default();
        s.wallet_cents = 3000;
        let p = Prices::default();
        s.start_hourly("a").unwrap();
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
        let now = s.start_ticket("tkt", &p).unwrap();
        assert_eq!(s.wallet_cents, 0);
        assert_eq!(s.active_ticket.as_ref().unwrap().deadline_unix, now + 48 * 3600);
        assert_eq!(s.start_ticket("tkt", &p), Err(StoreError::InsufficientFunds));
        // sem carteira: nem começa a repetir
        s.wallet_cents = 0;
        s.start_hourly("yyy").unwrap_or(()); // com 0, nada
        s.return_ticket();
        s.wallet_cents = 10_000;
        assert!(s.start_ticket("tkt", &p).is_ok());
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
```

- [ ] **Step 2: Implementar (acima dos testes de rules.rs)**

```rust
use crate::prices::Prices;
use crate::money::Money;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HourlyError { EmptyWallet, GameOwned, NoSwapsLeft }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError { InsufficientFunds }

impl Persisted {
    /// Credita salário do cartucho em serviço. Guarda pendência em
    /// ms (service_pending_ms) — nada de drift, centavos chão sobre o
    /// TOTAL pendente.
    pub fn credit_service_ms(&mut self, ms: i64, p: &Prices) -> i64 {
        self.stats.ms_served += ms;
        self.service_pending_ms += ms;
        let cents = self.service_pending_ms
            .checked_mul(p.salary_per_hour).unwrap_or(i64::MAX)
            / 3_600_000;
        if cents == 0 { return 0; }
        self.service_pending_ms -= cents * 3_600_000 / p.salary_per_hour;
        self.wallet_cents += cents;
        self.lifetime_earned_cents += cents;
        cents
    }

    /// Hora avulsa: só jogo do acervo (não-comprado) e com caixa no mínimo
    /// 1 minuto de freguesia.
    pub fn start_hourly(&mut self, game: &str) -> Result<(), HourlyError> {
        let p = crate::top_prices::LAST.with(|c| *c.borrow());
        let _ = p; // ver impl abaixo com prices passado por parâmetro
        self._start_hourly(game, &Prices::default())
    }

    fn _start_hourly(&mut self, game: &str, p: &Prices) -> Result<(), HourlyError> {
        if self.owns(game) { return Err(HourlyError::GameOwned); }
        let first_min = p.hourly_cents_for_ms(60_000).max(1);
        if self.wallet_cents < first_min { return Err(HourlyError::EmptyWallet); }
        self.hourly = Some(HourlySession { game: game.into(), swaps_used: 0 });
        self.stats.rentals += 1;
        self.bump_rental_day();
        Ok(())
    }

    /// Fecha a sessão debitando exatamente o custo dos ms jogados (chão).
    pub fn end_hourly(&mut self, ms_jugado: i64, p: &Prices) -> i64 {
        if self.hourly.take().is_none() { return 0; }
        let cost = p.hourly_cents_for_ms(ms_jugado);
        let cost = cost.min(self.wallet_cents); // carteira nunca negativa
        self.wallet_cents -= cost;
        self.lifetime_spent_cents += cost;
        cost
    }

    /// Co'll be corta? custo projetado da sessão+1s > carteira. (O runner
    /// consulta a cada segundo; a baixa é aplicada só no end_hourly real.)
    pub fn hourly_blackout_at(&self, ms_jugado: i64, p: &Prices) -> bool {
        let projetado = p.hourly_cents_for_ms(ms_jugado) + p.hourly_cents_for_ms(1000);
        projetado > self.wallet_cents
    }

    /// Troca o cartucho DENTRO da hora avulsa, límite 2 (design §6).
    pub fn swap_hourly(&mut self, nxt: &str) -> Result<(), HourlyError> {
        let Some(h) = &mut self.hourly else { return Err(HourlyError::EmptyWallet); };
        if h.swaps_used >= 2 { return Err(HourlyError::NoSwapsLeft); }
        h.game = nxt.to_string();
        h.swaps_used += 1;
        self.stats.swaps += 1;
        Ok(())
    }

    /// Ticket 48h: paga unext frente, prazo real (unix). Devolve (now).
    pub fn start_ticket(&mut self, game: &str, p: &Prices) -> Result<i64, StoreError> {
        let cost = Money(p.ticket_48h);
        let back = Money(self.wallet_cents).checked_sub(cost)
            .ok_or(StoreError::InsufficientFunds)?;
        self.wallet_cents = back.0;
        self.lifetime_spent_cents += cost.0;
        let now = crate::now_unix();
        self.active_ticket = Some(Ticket { game: game.into(), deadline_unix: now + 48 * 3600 });
        self.stats.rentals += 1;
        self.bump_rental_day();
        Ok(now)
    }

    pub fn return_ticket(&mut self) { self.active_ticket = None; }

    pub fn buy(&mut self, game: &str, p: &Prices) -> Result<(), StoreError> {
        let back = Money(self.wallet_cents).checked_sub(Money(p.purchase))
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

    fn bump_rental_day(&mut self) {
        let day = crate::now_unix() / 86_400;
        if day != self.stats.last_rental_day {
            self.stats.last_rental_day = day;
            self.stats.rental_days += 1;
        }
    }
}
```

**Nota de coerência (o executor ajust):** `start_hourly` não pode
instânciar `Prices::default()` — a assinatura correta leva os preços:
`pub fn start_hourly(&mut self, game: &str, p: &Prices) -> Result<(),
 HourlyError>`. Os testes serão o percurso: passe `&Prices::default()`
no call-site dos testes (substituir `s.start_hourly("xxx")` por
`s.start_hourly("xxx", &p)` nos pontos assinalados acima).

- [ ] **Step 3: verde + workspace**

```bash
cargo test -p sixteen-locadora && cargo test --workspace
```

- [ ] **Step 4: commit**

```bash
git add crates/locadora
git commit -m "feat(locadora): economia — salário sem drift, hora avulsa, trocas, blackout, ticket e compra"
```

---

### Task 5: Sorteio de boas-vindas + troféus

**Files:**
- Modify: `crates/locadora/src/rules.rs`
- Create: `crates/locadora/src/trophies.rs`
- Modify: `crates/locadora/src/lib.rs` (`pub mod trophies;`)

- [ ] **Step 1: Testes falhando**

`crates/locadora/src/rules.rs` append aos testes:

```rust
    #[test]
    fn draws_three_distinct_from_sha1s() {
        let acervo: Vec<String> =
            (0..10).map(|i| format!("sha-{i}")).collect();
        let trio = crate::rules::draw_trio(&acervo, 42);
        assert_eq!(trio.len(), 3);
        for g in &trio { assert!(acervo.contains(g)); }
        assert_ne!(trio[0], trio[1]);
        assert_ne!(trio[1], trio[2]);
        // mesma semente → mesmo trio (determinístico, testável)
        assert_eq!(crate::rules::draw_trio(&acervo, 42),
                   crate::rules::draw_trio(&acervo, 42));
    }

    #[test]
    fn intro_picks_from_trio_once_in_life() {
        let mut s = Persisted::default();
        let trio = vec!["a".into(), "b".into(), "c".into()];
        assert!(s.apply_intro(&trio, "c").is_ok());
        assert!(s.intro_done && s.owns("c"));
        assert_eq!(s.service_game.as_deref(), Some("c"));
        assert_eq!(s.apply_intro(&trio, "a"), Err("sorteio único da vida"));
        assert_eq!(s.apply_intro(&trio, "zzz"), Err("fora do trio"));
    }
```

`crates/locadora/src/trophies.rs` (novo, começa nos testes):

```rust
#[cfg(test)]
mod tests {
    use crate::prices::Prices;
    use crate::store::Persisted;
    use crate::trophies::{check_trophies, TrophyId};

    #[test]
    fn first_hour_unlocks_intro_and_turn() {
        let mut s = Persisted::default();
        s.intro_done = true;
        s.credit_service_ms(60 * 60_000, &Prices::default());
        let got = check_trophies(&mut s, 1_000);
        assert!(got.contains(&TrophyId::BemVindoAFicha));
        assert!(got.contains(&TrophyId::PrimeiroTurno));
        // de novo: nada novo, lista estável
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
        s.stats.rentals = 5;         // VizinhoDoAcervo
        s.stats.returns_in_time = 1; // PrazoCumprido
        s.stats.rental_days = 7;     // FreguesFiel
        for i in 0..5 { s.owned.push(format!("s{i}")); } // Colegão
        let got = check_trophies(&mut s, 500);
        for t in [TrophyId::BemVindoAFicha, TrophyId::VizinhoDoAcervo,
                  TrophyId::TrocaTroca, TrophyId::PrazoCumprido,
                  TrophyId::Colegao, TrophyId::FreguesFiel,
                  TrophyId::SobreviventeDaEstatica] {
            assert!(got.contains(&t), "faltou {t:?}");
        }
    }
}
```

- [ ] **Step 2: Implementar**

Em `rules.rs` (fora do `mod tests`):

```rust
/// Sorteio de boas-vindas: 3 sha1s distintas, determinístico pela semente.
pub fn draw_trio(acervo: &[String], seed: u64) -> Vec<String> {
    let mut rng = if seed == 0 { 0xA7C2_5D1E_398B_9F41 } else { seed };
    let mut out = Vec::new();
    let mut pool: Vec<String> = acervo.to_vec();
    if pool.is_empty() { return out; }
    for _ in 0..3.min(pool.len()) {
        rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17; // xorshift64
        let i = (rng as usize) % pool.len();
        out.push(pool.remove(i));
    }
    out
}

impl Persisted {
    /// Aplica a escolha do sorteio único: nasce comprado e em serviço.
    pub fn apply_intro(&mut self, trio: &[String], chosen: &str)
        -> Result<(), &'static str>
    {
        if self.intro_done { return Err("sorteio único da vida"); }
        if !trio.iter().any(|g| g == chosen) { return Err("fora do trio"); }
        self.intro_done = true;
        self.owned.push(chosen.to_string());
        self.service_game = Some(chosen.to_string());
        Ok(())
    }
}
```

`crates/locadora/src/trophies.rs`:

```rust
use crate::store::Persisted;

/// Troféus da locadora (design §9). v1 local; espelho RA fica pra v2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TrophyId {
    BemVindoAFicha,
    PrimeiroTurno,
    VizinhoDoAcervo,
    TrocaTroca,
    PrazoCumprido,
    Colegao,
    Lojista,
    FreguesFiel,
    SobreviventeDaEstatica,
}

impl TrophyId {
    pub const ALL: [Self; 9] = [
        Self::BemVindoAFicha, Self::PrimeiroTurno, Self::VizinhoDoAcervo,
        Self::TrocaTroca, Self::PrazoCumprido, Self::Colegao,
        Self::Lojista, Self::FreguesFiel, Self::SobreviventeDaEstatica,
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
        }
    }

    pub fn key(&self) -> String { format!("{self:?}") }
}

/// Condições novas do estado → troféus desbloqueados agora (chão: id, once).
pub fn check_trophies(st: &mut Persisted, now: i64) -> Vec<TrophyId> {
    let mut out = Vec::new();
    for t in TrophyId::ALL {
        if st.trophies.iter().any(|r| r.id == t.key()) { continue; }
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
        };
        if won {
            st.trophies.push(crate::store::TrophyRecord {
                id: t.key(), unlocked_unix: now,
            });
            out.push(t);
        }
    }
    out
}
```

Ajustar `TrophyRecord` (Task 2) para `id: String` (já está) e marcar no
`bump`/`return_ticket` a condição "devolveu dentro do prazo": em
`return_ticket`, comparar `now < deadline` e incrementar `returns_in_time`;

```rust
    /// Devolve o ticket; retorna true se foi no prazo (para troféu/adega).
    pub fn return_ticket(&mut self) -> bool {
        match self.active_ticket.take() {
            None => false,
            Some(t) => {
                let ok = crate::now_unix() < t.deadline_unix;
                if ok { self.stats.returns_in_time += 1; }
                ok
            }
        }
    }
```

(Atualizar o teste da Task 3 `return_ticket_clears_state` para
`assert!(!s.return_ticket())` quando o prazo era 1 — prazo no passado.)

- [ ] **Step 3: verde + workspace + commit**

```bash
cargo test -p sixteen-locadora && cargo test --workspace
git add crates/locadora
git commit -m "feat(locadora): sorteio de boas-vindas único e troféus determinísticos"
```

---

## Fase B — app

### Task 6: Caminho, Config (preços/moeda/mínimo) e leitura no boot

**Files:**
- Modify: `crates/app/src/dirs.rs`
- Modify: `crates/app/src/config.rs`

- [ ] **Step 1: Testes falhando**

Em `dirs.rs` (no `mod tests` existente):

```rust
    #[test]
    fn locadora_json_lives_in_config() {
        assert_eq!(locadora_path(), config_dir().join("locadora.json"));
    }
```

Em `config.rs` (no `mod tests`):

```rust
    #[test]
    fn locadora_fields_have_sane_defaults() {
        let cfg = Config::default();
        assert_eq!(cfg.locadora_needed, 3);
        assert_eq!(cfg.locadora_currency, "R$");
        assert_eq!(cfg.locadora_salary_cents, 500);
        assert_eq!(cfg.locadora_hourly_cents, 800);
        assert_eq!(cfg.locadora_ticket_cents, 1200);
        assert_eq!(cfg.locadora_purchase_cents, 6000);
    }

    #[test]
    fn locadora_fields_roundtrip_through_toml() {
        let mut cfg = Config::default();
        cfg.locadora_needed = 5;
        cfg.locadora_currency = "US$".into();
        cfg.locadora_purchase_cents = 9000;
        // to_toml deve conter campos [locadora] e load de volta deve preservar
        let text = cfg.to_toml();
        assert!(text.contains("[locadora]"));
        assert!(text.contains("needed = 5"));
        assert!(text.contains("currency = \"US$\""));
        assert!(text.contains("purchase_cents = 9000"));
    }
```

- [ ] **Step 2: Implementar**

`dirs.rs` — junto dos outros arquivos:

```rust
pub fn locadora_path() -> PathBuf { config_dir().join("locadora.json") }
```

`config.rs`:
- `Config`: `pub locadora_needed: u32, pub locadora_currency: String,
  pub locadora_salary_cents: i64, pub locadora_hourly_cents: i64,
  pub locadora_ticket_cents: i64, pub locadora_purchase_cents: i64` —
  defaults `3, "R$", 500, 800, 1200, 6000` (mesma tripa dos defaults de
  locadora::Prices para nunca divergirem).
- `Raw`: campos `Option<u32>/Option<String>/Option<i64>` e parse
  seguindo exatamente o padrão dos campos existentes (ver quaisquer
  `Option<String>` de `ra_user`; strings de default via
  `.filter(|s| !s.trim().is_empty()).unwrap_or("R$")`).
- `to_toml()`: emitir seção:

```toml
[locadora]
needed = 3
currency = "R$"
salary_cents = 500
hourly_cents = 800
ticket_cents = 1200
purchase_cents = 6000
```

(Seguir o layout de chave do arquivo de hoje; a função é manual.)

- [ ] **Step 3: verde + commit**

```bash
cargo test --workspace
git add crates/app/src/dirs.rs crates/app/src/config.rs
git commit -m "feat(app): caminho config/locadora.json e campos [locadora] no cfg"
```

---

### Task 7: Tela inicial nova + botão, e rota vertical (o modo existe)

**Files:**
- Modify: `crates/platform/src/cabinet.rs` (`PanelButton`, hit/desenho do painel idle)
- Modify: `crates/app/src/idle.rs` (`IdleExit`, gate de acervo)
- Create: `crates/app/src/locadora_screen.rs` (v1: balcão vazio funcional)
- Modify: `crates/app/src/lib.rs` (`pub mod locadora_screen;`)
- Modify: `crates/app/src/bin/sixteen.rs` (rota `'app:`)

- [ ] **Step 1: Cabinet — botão novo**

1. Em `enum PanelButton` (cabinet.rs:499-511): `Locadora`.
2. No desenho do painel do idle, empilhar **dois botões** (design §2):
   linha 1 (a de hoje, o prompt de `set_idle_core_prompt` passa a dizer
   "ESTANTE DE GAMES · FREE PLAY"), linha 2 nova "MODO LOCADORA" — rect
   novo em campo `locadora_btn: Rect` perto do que guarda o rect do
   prompt de hoje (procurar onde o hit lê o rect do botão de inserir).
3. `hit_panel_button` (que hoje sorteia `PanelButton::Insert` etc.) teste
   o rect novo antes do de inserir e devolve `Some(PanelButton::Locadora)`.

- [ ] **Step 2: idle.rs — exit novo + OSD de acervo insuficiente**

`IdleExit` ganha `OpenLocadora`. O idle deve saber quantos jogos há —
`idle::run` recebe `catalog_counts: u32` (novo parâmetro, chamado com
`catalog.counts()` no sixteen.rs; conferir como `dat_installed` já
entra por parâmetro no mesmo ponto — `sixteen.rs:295-302`).

No hit (idle.rs:563-599):

```rust
Some(PanelButton::Locadora) => {
    let need = cfg_snapshot.locadora_needed as usize; // ver leitura abaixo
    if catalog_counts >= need {
        return Ok(IdleExit::OpenLocadora);
    }
    cab.push_osd(
        &[format!("{catalog_counts}/{need} jogos — acervo insuficiente para a locadora")],
        None, 4,
    );
}
```

(`cfg_snapshot`: `idle::run` também passa a receber `locadora_needed` como
parâmetro — o idle hoje não lê o Config; dois parâmetros simples batem com
o estilo já usado por `core_path.is_some(), dat_installed`.)

- [ ] **Step 3: locadora_screen.rs — vertical slice (balcão vazio)**

```rust
//! Balcão da locadora. v1: carteira visível + voltar. O acervo completo
//! chega na Task 8; estrutura de loop e saída já é definitiva.

use crate::dirs;
use locadora::Persisted;
use locadora::Prices;

pub enum Pick { Back, Quit }

pub fn run(
    plat: &mut Platform,
    cab: &mut Cabinet,
    loc: &mut Persisted,
    prices: &Prices,
    currency: &str,
) -> Result<Pick, Box<dyn std::error::Error>> {
    let交易中心 = loc.wallet_cents.fmt(currency);
    loop {
        plat.poll_events()?;
        let m = plat.mouse();
        if plat.quit_requested() { return Ok(Pick::Quit); }
        if m.clicked || plat.menu_nav().is_confirm() {
            // "voltar ao corredor" (ESC/B/Backspace) → tela inicial
            if m.clicked || plat.menu_nav().is_back() {
                loc.save(&dirs::locadora_path())?;
                return Ok(Pick::Back);
            }
        }
        cab.frame_idle_2d(|d| {
            d.fill(12, 10, 14, 255, 0, 0, 10_000, 10_000);
            d.text("LOCADORA SIXTEEN", 40, 40, 1);
            d.outline(200, 190, 170, 255, 40, 80, 240, 60);
            d.text(&format!("CAIXA: {wallet_display}"), 52, 108, 1);
            d.text("voltar (ESC)", 52, 140, 1);
        })?;
    }
}
```

(**O executor alinea com a API real de `idle.rs`/`shelf.rs`: saída de
loop, `plat.poll_*, m.clicked`, `frame_idle_2d` closure signature e
`Screen::{fill,outline,text}` — copiar o padrão dos primeiros frames
de `idle::run` (`idle.rs:172-250`) em vez de inventarairro. Estrutura
de `Pick` + `run` acima é a definitiva; só o desenho muda nas tasks
seguintes. Sem dep para `Box<dyn Error>`: usar `anyhow::Result` como os
resto das telas.**)

- [ ] **Step 4: rota no sixteen.rs**

```rust
IdleExit::OpenLocadora => {
    let prices = locadora::Prices {
        salary_per_hour: cfg.locadora_salary_cents,
        hourly_per_hour: cfg.locadora_hourly_cents,
        ticket_48h: cfg.locadora_ticket_cents,
        purchase: cfg.locadora_purchase_cents,
    };
    let mut loc = locadora::Persisted::load(&dirs::locadora_path());
    match crate::locadora_screen::run(plat, cab, &mut loc, &prices, &cfg.locadora_currency) {
        Ok(LocadoraScreenPick::Quit) => return Ok(()),
        _ => {} // Back cai pro idle de novo
    }
}
```

(`LocadoraScreenPick` renomeia pra `Pick` local; seguir imports padrões.)

- [ ] **Step 5: verificar headless + commit**

```bash
cargo test --workspace && cargo build
# manual: <3 jogos → OSD; ≥3 → entra no balcão e volta pelo ESC
git add crates/app crates/platform
git commit -m "feat(app): botão MODO LOCADORA na tela inicial e balcão vertical"
```

---

### Task 8: Prateleira da locadora — estados dos cartuchos e ações

**Files:**
- Modify: `crates/app/src/locadora_screen.rs`
- Modify: `crates/app/src/shelf.rs` (extrair `find_local_art` para módulo próprio)
- Create: `crates/app/src/art.rs` (`pub fn find_local_art(...)`, corpo copiado de shelf.rs:477-499; shelf.rs passa a `use crate::art::find_local_art;`)

- [ ] **Step 1: Decisão de renderização por estado (design §7)**

Por `CatalogEntry` (ordem `Order::Shelf`), computar:

```rust
use locadora::rules::InsertCheck;

struct TileState { insert: InsertCheck, owned: bool, is_service: bool,
                   ticket_deadline: Option<i64>, hourly_cart: bool }
```

Desenho por tile (com a arte carregada como no shelf: `cab.set_image(id,
w,h,rgba)` com id derivado do sha1 — pattern de `cover_id` shelf.rs:184-201,
e `d.image_fit(id, …)` do `Screen`, cabinet.rs:3191):

```rust
/// Cartucho de locadora: amarelecimento, rasgos, fita e faixa clássica (§7).
fn draw_worn(d: &mut Screen, id: u64, x: i32, y: i32, w: i32, h: i32) {
    d.image_fit(id, x, y, w, h);
    d.fill(60, 52, 38, 70, x, y, w, h);           // idade
    d.fill(234, 226, 210, 255, x, y, 10, 6);       // rasgo sup.
    d.fill(234, 226, 210, 255, x, y + h - 6, 14, 6);// rasgo inf.
    d.fill(214, 196, 120, 120, x + w / 2 - 22, y + h / 2 - 24, 44, 6); // fita
    d.fill(232, 222, 198, 214, x + 4, y + h / 2 - 9, w - 8, 18);       // faixa
    d.text("PROPRIEDADE DA LOCADORA", x + 10, y + h / 2 - 5, 1);
}

/// Adesivo de prazo do ticket: branco datilografado por cima.
fn draw_ticket_sticker(d: &mut Screen, x: i32, y: i32, w: i32, deadline: i64) {
    d.fill(244, 242, 230, 235, x, y, w, 22);
    d.outline(90, 80, 60, 255, x, y, w, 22);
    d.text(&format!("DEVOLVER ATÉ {}", fmt_deadline(deadline)), x + 6, y + 5, 1);
}

/// unix → "DD/MM hh:mm", calculadora civil própria (std only).
fn fmt_deadline(unix: i64) -> String { /* dias desde epoch → d/m com tabelas de mês bissesto */ }
```

Cartucho **comprado** (na prateleira, §7): `d.image_fit(id, …)` sem
overlays + contorno duplo fino na cor da capa + selo "COLEÇÃO" canto.

- [ ] **Step 2: Prompt por estado + ação**

```rust
let prompt = match ts.insert {
    InsertCheck::Ok if ts.is_service => "JOGAR — cartucho em serviço (+R$/h)",
    InsertCheck::Ok if ts.ticket_deadline.is_some() => "JOGAR — em ticket de 48h",
    InsertCheck::Ok if ts.hourly_cart => "JOGAR — hora avulsa (≤2 trocas)",
    InsertCheck::Ok => "INSERIR — jogo da coleção",
    InsertCheck::TicketExpired => "PRAZO VENCIDO — clique para devolver",
    InsertCheck::NotRented => "LOCAR — avulsa R$ 8/h (2 trocas) · ticket R$ 12",
};
```

Ações de clique (sel opção como o shelf faz com `Enter` e cliques):

```rust
match ts.insert {
    InsertCheck::TicketExpired => { loc.return_ticket(); cab.push_osd(&["devolvido à locadora"], None, 4); }
    InsertCheck::NotRented => {
        // submenu do tile: avulsa | ticket 48h (2 botões inline no painel)
        match sub_choice {
            Sub::Hourly if loc.start_hourly(&sha, prices).is_ok() =>
                return Ok(Pick::Play { rom: rom_path.into_path(), hourly: true }),
            Sub::Ticket if loc.start_ticket(&sha, prices).is_ok() =>
                return Ok(Pick::Play { rom: rom_path.into_path(), hourly: false }),
            Sub::Hourly => cab.push_osd(&["grana insuficiente — jogue o cartucho em serviço".into()], None, 4),
            Sub::Ticket => cab.push_osd(&["grana insuficiente para o ticket".into()], None, 4),
        }
    }
    _ => return Ok(Pick::Play { rom: rom_path.into_path(), hourly: ts.hourly_cart }),
}
```

`Pick` do balcão cresce:

```rust
pub enum Pick {
    /// insert validado pela tela; `hourly` diz ao runner o modo de sessão
    Play { rom: std::path::PathBuf, hourly: bool },
    Back,
    Quit,
}
```

(**Em `Ok`/locação, salvar `loc.save(&dirs::locadora_path())?` ANTES de
devolver Play: o runner necesita as regras atualizadas.**)

- [ ] **Step 3: Em serviço (eleição) — selo no painel**

Cartucho da coleção não em serviço exibe ação "ELEGER EM SERVIÇO"
(botão): `loc.elect_service(&sha)?` + OSD. O tile em serviço ganha
cartão "EM SERVIÇO".

- [ ] **Step 4:(caixa nº a linha de registro do balcão**)

```rust
let wallet = Money(loc.wallet_cents).fmt(currency);
let service_title = loc.service_game.as_ref()
    .and_then(|sha| catalog_entries.iter().find(|e| e.rom.sha1 == *sha))
    .map(|e| e.title().into_owned())
    .unwrap_or("—".into());
d.text(&format!("CAIXA: {wallet}   EM SERVIÇO: {service_title}"), x, y, 1);
```

- [ ] **Step 5: headless + commit**

```bash
cargo test --workspace && cargo build
# manual: prateleira lista jogos com gasto visual; ações de locar/ticket/devolver/eleger
git add crates/app
git commit -m "feat(app): prateleira da locadora com física dos cartuchos e ações do balcão"
```

---

### Task 9: Sorteio de boas-vindas na primeira entrada

**Files:**
- Modify: `crates/app/src/locadora_screen.rs`

- [ ] **Step 1: Gate e cerimônia**

No início de `run`, antes da prateleira:

```rust
if !loc.intro_done {
    let acervo: Vec<String> = catalog.list(Order::Shelf)?
        .iter().map(|e| e.rom.sha1.clone()).collect();
    let trio = locadora::rules::draw_trio(&acervo, crate::now_unix() as u64);
    if let Some(chosen) = draw_intro_spotlight(cab, plat, &trio)? {
        loc.apply_intro(&trio, &chosen)?;
        loc.save(&dirs::locadora_path())?;
        // cerimônia v1 (peel simplificado): OSD + brilho no cartucho vencedor
        cab.push_osd(&["este cartucho é SEU — foi isso que comprar te dá".into()], None, 5);
        crate::sfx::play(cab, Sfx::Insert);
    }
}
```

`draw_intro_spotlight`: tubo cheio com os **3** cartuchos lado a lado —
os não escolhidos em `draw_worn`, hover escolhe (like shelf hover), texto
"ESCOLHA SEU JOGO" e letras da locadora. O escolhido redesenha limpo
(`image_fit` puro + outline duplo) durante 0.8s com `Sfx::Insert`,
"SEU — EM SERVIÇO" estampado, antes de cair na prateleira normalmente.

- [ ] **Step 2: headless + commit**

```bash
cargo test --workspace
git add crates/app
git commit -m "feat(app): sorteio de boas-vindas com cerimônia de label nova"
```

---

### Task 10: Loja (vitrine) + destrava de contracapa e manual

**Files:**
- Modify: `crates/app/src/locadora_screen.rs` (sub-tela `screen = 1`)
- Modify: `crates/app/src/bin/sixteen.rs` (passar playtime da locadora), se aplicável

- [ ] **Step 1: vitrine e por jogo**

Sub-somewhat `screen = 1` (botão "LOJA" no balcão): lista do acervo com
estado — comprado caixa nova com "COLEÇÃO" + botão contracapa/manual;
não comprado com caixa gasta, preço `purchase.fmt(currency)`, sem
contracapa (no lugar: ficha de catálogo datilografada), botão COMPRAR.

- [ ] **Step 2: comprar**

```rust
if loc.can_afford_purchase(prices) {
    loc.buy(&sha, prices)?;
    loc.save(&dirs::locadora_path())?;
    peel_ceremony(cab, &sha)?; // 3 fases §7: gasto sobe → faixa descasca → label nova
    crate::sfx::play(cab, Sfx::Insert);
    let new_trophies = locadora::trophies::check_trophies(loc, crate::now_unix());
    if !new_trophies.is_empty() {
        // persiste e anuncia (Task 12 centraliza, mas aqui já chora)
        loc.save(&dirs::locadora_path())?;
    }
} else {
    cab.push_osd(&["grana insuficiente — jogue mais no cartucho em serviço".into()], None, 4);
}
```

- [ ] **Step 3: destrava por compra**

No painel do jogo (`ShelfPanelInfo`-style da tela da locadora), campos
`backcover_img` e manual só para `loc.owns(&sha)`. Manual reusa o reader
do shelf (shelf.rs:248-305, `ManualState` shelf.rs:1064) — extrair para
`app::manual_view` se preciso. Não comprado: ficha de catálogo no lugar
da contracapa (título, Ano de Fábrica e gênero — lift dos mesmos dados
que `shelf.rs:2219-2384` alimenta em `ShelfPanelInfo`).

- [ ] **Step 4: headless + commit**

```bash
cargo test --workspace
git add crates/app
git commit -m "feat(app): loja com cerimônia de compra e destrava de manual/contracapa"
```

---

### Task 11: Runner — salário por segundo, blackout, prazo e estatísticas

**Files:**
- Modify: `crates/app/src/runner.rs` (`GameSpec`, loop)
- Modify: `crates/app/src/bin/sixteen.rs` ('shelf:'→'locadora:' loop)

- [ ] **Step 1: Estado da sessão entra no runner**

`runner::GameSpec` (runner.rs:59-98) ganha:

```rust
pub struct Rentals {
    pub mode: locadora::RentMode,     // Service | Hourly | Ticket
    pub is_service_cart: bool,        // o sha deste jogo == service_game
}
// campo: pub rentals: Option<Rentals>
```

E `runner::run_game(...)` recebe `locadora: Option<&mut locadora::Persisted>`
(e `prices: &locadora::Prices`) — só existe quando o modo locadora
chamou; em None tudo fica grátis como hoje (**free play intocado**).

- [ ] **Step 2: bilhete por segundo (no loop, junto do `set_session_time`)**

```rust
// a cada frame, powered: ms contínuo de console ligado
let real_ms = live_session_time(powered_elapsed, powered_since).as_millis() as i64;
if real_ms - billed_ms >= 1000 {
    if let (Some(loc), Some(ren)) = (loc.as_deref_mut(), spec.rentals.as_ref()) {
        match ren.mode {
            locadora::RentMode::Service => {
                if ren.is_service_cart {
                    loc.credit_service_ms(real_ms - billed_ms, prices);
                }
            }
            locadora::RentMode::Hourly => {
                let since = real_ms - billed_ms;
                if loc.hourly_blackout_at(session_hourly_ms + since, prices) {
                    loc.end_hourly(session_hourly_ms + since, prices);
                    save_loc(loc)?;
                    let burst = power_off_burst(plat, cab); // tubo apaga §4
                    loc.stats.blackout_recoveries += 1;
                    save_loc(loc)?;
                    return Ok(GameExit::Ejected { static_level: burst }); // volta ao balcão
                }
                session_hourly_ms += since;
            }
            locadora::RentMode::Ticket => {
                if let Some(t) = &loc.active_ticket {
                    if crate::now_unix() >= t.deadline_unix { /* venceu com o tubo ligado */ }
                }
            }
        }
    }
    billed_ms = real_ms;
}
```

Taxa de implementação: `billed_ms` e `session_hourly_ms` são `i64`
locais de `run_game`, zerados no boot da sessão; `power_off_burst`
`runner.rs:384` já devolve `static_level`.

- [ ] **Step 3: bordas de sessão**

- **Início da sessão** (perto de `cartridge_insert_animation`,
  runner.rs:1378-1385): OSD do balcão por modo — Service:
  "cartucho em serviço: a gaveta tiquetaca +R$ 5/h"; Hourly:
  "freguesia: R$ 8/h — grana acaba, tubo apaga"; Ticket: "devolver até
  {fmt_deadline}".
- **Vencimento com tubo ligado** (Ticket): a cada segundo, prazo
  ultrapassado → OSD "prazo vencido — devolva no balcão" uma vez
  (flag local).
- **Eject game off** (`GameExit::Ejected`, runner.rs:1735-1746):
  antes de sair, fechar a freguesia — `loc.end_hourly(session_hourly_ms,
  prices)` quando Hourly; `save_loc`.
  **Credito de salário NÃO é no fim**: já persiste a cada segundo
  (`save_loc` a cada billing — save de <1 KB, tranqüilo). Regras de
  troca ficam na tela da locadora (Task 8), o runner só debite/credita
  e registra.
- **Power off** (`UiEvent::Quit` power branch, runner.rs:1701-1711):
  mesma cobrança do eject (end_hourly parcial + save).

`GameExit` mantém o braço `Ejected { static_level }` as-is (o corta a luz
entra como Ejected; sixteen.rs distingue por OSD já empurrado).

- [ ] **Step 4: rota no sixteen.rs (loop da locadora)**

```rust
// 'locadora: espelho do 'shelf: loop (sixteen.rs:333-422)
locadora::rules::check_trophies(&mut loc, crate::now_unix()); // pois sessão muda stats
loc.save(&dirs::locadora_path())?;
match crate::locadora_screen::run(plat, cab, &mut loc, &prices, &cur) { ... }
```

- [ ] **Step 5: headless + commit**

```bash
cargo test --workspace && cargo build
# manual: 10 min no em serviço eleva a carteira; locação por hora corta quando zera
git add crates/app
git commit -m "feat(app): runner credita debeatda freguesia por segundo e aplica corte de luz"
```

---

### Task 12: Parede de troféus + docs

**Files:**
- Modify: `crates/app/src/locadora_screen.rs` (sub-tela `screen = 2`)
- Modify: `README.md` (seção breve), `CHANGELOG.md` (1.4.0)

- [ ] **Step 1: parede**

Grade 3×3 dos 9 troféus (`TrophyId::ALL`), pedestal+figura desenhados
proceduralmente sobre `d.fill` (pixel-art leve, estilo medal_rgba do
ra.rs:780). Desbloqueado: figura pintada + título `t.title()` + data
(fmt_deadline/unlocked). Trancado: pedestal vago, título "???".

- [ ] **Step 2: OSD de desbloqueio**

Centralizar: onde `check_trophies` rodar (Task 10/11), para cada novo:

```rust
for t in unlocked {
    cab.push_osd(&[format!("🏆 TROFÉU: {}", t.title())], None, 6);
    crate::sfx::play(cab, Sfx::Achievement);
}
```

- [ ] **Step 3: docs e release**

README: parágrafo "Modo Locadora" (o que é, como entra, como desligar).
CHANGELOG: entrada da versão com bullets (economia, trocas, ticket, loja,
troféus, corte de luz).

- [ ] **Step 4: verificação final + commit**

```bash
cargo test --workspace && cargo clippy --workspace
# roteiro manual completo §8: sorteio → 1h → avulsa/2 trocas → ticket 48h →
# loja/compra → parede de troféus → sair sem perder nada
git add -A
git commit -m "feat(locadora): parede de troféus, OSD de desbloqueio e docs"
```

---

## Checklist de verificação (feature completa)

- [ ] `cargo test --workspace` e `cargo clippy --workspace` limpos.
- [ ] Playtest do roteiro do design §8 completo no app real.
- [ ] `config/locadora.json` sobrevive a boot, crash e arquivo corrupto (zera sem pânico).
- [ ] Free play intocado: estante de games, runner, anotações e RA funcionam como hoje.
- [ ] Menos de 3 jogos: MODO LOCADORA responde com OSD e não abre.
