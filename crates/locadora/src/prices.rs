//! Valores do design §4 e §17 (centavos). Todos editáveis no cfg do app;
//! os defaults nunca divergem do design.

use serde::{Deserialize, Serialize};

use crate::money::Money;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prices {
    pub salary_per_hour: i64, // centavos/h no cartucho em serviço
    pub hourly_per_hour: i64, // centavos/h na hora avulsa
    pub ticket_48h: i64,      // centavos o ticket de 48h corridas
    pub purchase: i64,        // centavos a compra permanente
    pub weekend_double: bool, // §17: sáb/dom paga o dobro
}

impl Default for Prices {
    fn default() -> Self {
        Self {
            salary_per_hour: 500,
            hourly_per_hour: 800,
            ticket_48h: 1200,
            purchase: 6000,
            weekend_double: true,
        }
    }
}

impl Prices {
    pub fn salary_cents_for_ms(&self, ms: i64) -> i64 {
        Money::cents_from_ms(ms, self.salary_per_hour)
    }

    /// Salário com regra de fim de semana (§17): dia do unix, 5=sáb, 6=dom.
    pub fn salary_rate_at(&self, unix: i64) -> i64 {
        let day = unix.div_euclid(86_400).rem_euclid(7); // 0 = quinta (1970-01-01)
        if self.weekend_double && (day == 5 || day == 6) {
            self.salary_per_hour * 2
        } else {
            self.salary_per_hour
        }
    }

    pub fn hourly_cents_for_ms(&self, ms: i64) -> i64 {
        Money::cents_from_ms(ms, self.hourly_per_hour)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_design() {
        let p = Prices::default();
        assert_eq!(p.salary_per_hour, 500);
        assert_eq!(p.hourly_per_hour, 800);
        assert_eq!(p.ticket_48h, 1200);
        assert_eq!(p.purchase, 6000);
        assert!(p.weekend_double);
    }

    #[test]
    fn salary_cents_per_ms_is_exact_floor() {
        let p = Prices::default();
        assert_eq!(p.salary_cents_for_ms(7_200_000), 1000);
        assert_eq!(p.salary_cents_for_ms(60_000), 8);
        assert_eq!(p.salary_cents_for_ms(0), 0);
    }

    #[test]
    fn hourly_cents_per_ms_is_exact_floor() {
        let p = Prices::default();
        assert_eq!(p.hourly_cents_for_ms(3_600_000), 800);
        assert_eq!(p.hourly_cents_for_ms(1_800_000), 400);
        assert_eq!(p.hourly_cents_for_ms(900_000), 200);
    }

    #[test]
    fn weekend_pays_double_on_saturday_and_sunday() {
        let p = Prices::default();
        // 1970-01-01 foi quinta (day 0). Sábado = day 5 → unix 5*86400 - 1.
        let saturday_noon = 5 * 86_400; // 1970-01-03 00:00 UTC
        let sunday_noon = 6 * 86_400;
        let monday_noon = 7 * 86_400;
        assert_eq!(p.salary_rate_at(saturday_noon), 1000);
        assert_eq!(p.salary_rate_at(sunday_noon), 1000);
        assert_eq!(p.salary_rate_at(monday_noon), 500);
        // desligável no cfg:
        let off = Prices { weekend_double: false, ..Prices::default() };
        assert_eq!(off.salary_rate_at(saturday_noon), 500);
    }
}
