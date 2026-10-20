//! Modo Locadora — domínio puro da locadora de SNES fictícia (design em
//! docs/design-modo-locadora.md). Zero SDL: regras, dinheiro em centavos,
//! troféus e persistência. A UI mora no `app`.

mod errors;
mod money;
mod prices;
pub mod rules;
pub mod store;
pub mod trophies;

pub use errors::Error;
pub use money::Money;
pub use prices::Prices;
pub use rules::{
    draw_trio, poster_of_week, ElectError, HourlyError, InsertCheck, RentMode,
    StoreError, TicketEvent,
};
pub use store::{HourlySession, Persisted, Stats, Ticket, TrophyRecord};
pub use trophies::{check_trophies, TrophyId};
