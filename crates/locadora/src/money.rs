//! Dinheiro fictício da locadora, em centavos. Nunca é real.

use core::ops::{AddAssign, Sub};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Money(pub i64); // centavos

impl AddAssign for Money {
    fn add_assign(&mut self, rhs: Self) {
        self.0 += rhs.0;
    }
}

impl Sub for Money {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Money(self.0 - rhs.0)
    }
}

impl Money {
    /// Subtrai com saldo checado: None quando não cobre.
    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        (self.0 >= rhs.0).then(|| Money(self.0 - rhs.0))
    }

    /// Centavos por milissegundos: (ms * cents/h) / 3.600.000, chão inteiro,
    /// saturando em i64::MAX.
    pub fn cents_from_ms(ms: i64, cents_per_hour: i64) -> i64 {
        match ms.checked_mul(cents_per_hour) {
            Some(v) => v / 3_600_000,
            None => i64::MAX,
        }
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
            if i > 0 && rest % 3 == 0 {
                grouped.push('.');
            }
            grouped.push(ch);
        }
        format!("{sign}{symbol} {grouped},{frac:02}")
    }
}

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

    #[test]
    fn cents_from_ms_is_exact_floor() {
        assert_eq!(Money::cents_from_ms(3_600_000, 500), 500);
        assert_eq!(Money::cents_from_ms(60_000, 500), 8);
        assert_eq!(Money::cents_from_ms(0, 500), 0);
        assert_eq!(Money::cents_from_ms(i64::MAX, 500), i64::MAX);
    }
}
