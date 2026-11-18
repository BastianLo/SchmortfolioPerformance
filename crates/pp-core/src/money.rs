//! Fixed-point value types, mirroring the scaling used by Portfolio Performance.
//!
//! * money amounts: `i64` in minor units (factor 100)
//! * shares: `i64` with factor 10^8
//! * quotes (prices per share): `i64` with factor 10^8

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

pub const AMOUNT_FACTOR: i64 = 100;
pub const SHARES_FACTOR: i64 = 100_000_000;
pub const QUOTE_FACTOR: i64 = 100_000_000;

/// Number of decimal digits a quote has more than a money amount (8 - 2).
const QUOTE_TO_MONEY_DIGITS: u32 = 6;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Money {
    pub currency: String,
    pub amount: i64,
}

impl Money {
    pub fn new(currency: impl Into<String>, amount: i64) -> Self {
        Self {
            currency: currency.into(),
            amount,
        }
    }

    pub fn zero(currency: impl Into<String>) -> Self {
        Self::new(currency, 0)
    }

    pub fn is_zero(&self) -> bool {
        self.amount == 0
    }
}

/// Rounds an integer to the given number of significant digits (HALF_UP),
/// returning the rounded value (still in the same scale).
fn round_significant(value: i128, digits: u32) -> i128 {
    let abs = value.unsigned_abs();
    let len = if abs == 0 { 1 } else { abs.ilog10() + 1 };
    if len <= digits {
        return value;
    }
    let drop = len - digits;
    let div = 10u128.pow(drop);
    let q = abs / div;
    let r = abs % div;
    let q = if r * 2 >= div { q + 1 } else { q };
    let rounded = (q * div) as i128;
    if value < 0 { -rounded } else { rounded }
}

/// Divides with HALF_UP rounding (away from zero on ties).
fn div_half_up(value: i128, div: i128) -> i128 {
    let q = value / div;
    let r = value % div;
    if r.abs() * 2 >= div.abs() {
        q + value.signum() * div.signum()
    } else {
        q
    }
}

/// Market value (in money minor units) of `shares` at `quote`.
///
/// Emulates PP: `shares/1e8 * quote` with `MathContext(10, HALF_UP)`, then
/// scaled to minor units with HALF_UP.
pub fn market_value(shares: i64, quote: i64) -> i64 {
    // exact product with 16 implicit decimals (8 shares + 8 quote)
    let product = shares as i128 * quote as i128;
    // MathContext(10) applies to the decimal value; the significant digits are
    // the same in the scaled integer representation.
    let rounded = round_significant(product, 10);
    // product has scale 16; money has scale 2 => divide by 10^14
    div_half_up(rounded, 10i128.pow(8 + QUOTE_TO_MONEY_DIGITS)) as i64
}

/// Converts an amount with an exchange rate, rounding HALF_DOWN like PP.
pub fn convert_amount(amount: i64, rate: Decimal) -> i64 {
    if amount == 0 {
        return 0;
    }
    let v = (rate * Decimal::from(amount))
        .round_dp_with_strategy(0, RoundingStrategy::MidpointTowardZero);
    i64::try_from(v.mantissa() / 10i128.pow(v.scale())).unwrap_or(0)
}

/// `1 / rate` with 10 decimals, HALF_DOWN (as PP's `ExchangeRate.inverse`).
pub fn inverse_rate(rate: Decimal) -> Decimal {
    if rate.is_zero() {
        return Decimal::ZERO;
    }
    (Decimal::ONE / rate).round_dp_with_strategy(10, RoundingStrategy::MidpointTowardZero)
}

pub fn amount_to_f64(amount: i64) -> f64 {
    amount as f64 / AMOUNT_FACTOR as f64
}

pub fn shares_to_f64(shares: i64) -> f64 {
    shares as f64 / SHARES_FACTOR as f64
}

pub fn quote_to_f64(quote: i64) -> f64 {
    quote as f64 / QUOTE_FACTOR as f64
}

/// Price per share as quote (factor 1e8) from a money amount and shares.
pub fn price_per_share(amount: i64, shares: i64) -> i64 {
    if shares == 0 {
        return 0;
    }
    // amount (scale 2) / shares (scale 8) -> quote (scale 8)
    // quote = amount * 1e6 * 1e8 / shares
    let num = amount as i128 * 10i128.pow(QUOTE_TO_MONEY_DIGITS) * SHARES_FACTOR as i128;
    div_half_up(num, shares as i128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn market_value_simple() {
        // 10 shares at 100.00 => 1000.00
        assert_eq!(
            market_value(10 * SHARES_FACTOR, 100 * QUOTE_FACTOR),
            100_000
        );
        // 1.5 shares at 33.333 => 49.9995 -> 50.00
        assert_eq!(market_value(150_000_000, 3_333_300_000), 5000);
    }

    #[test]
    fn convert_half_down() {
        let r = Decimal::from_str("0.5").unwrap();
        assert_eq!(convert_amount(5, r), 2); // 2.5 -> 2 (half down)
        assert_eq!(convert_amount(7, r), 3); // 3.5 -> 3
        assert_eq!(convert_amount(-5, r), -2);
    }

    #[test]
    fn inverse() {
        let r = Decimal::from_str("1.1588").unwrap();
        assert_eq!(inverse_rate(r).to_string(), "0.8629616845");
    }

    #[test]
    fn significant() {
        assert_eq!(round_significant(123_456_789_012, 10), 123_456_789_000);
        assert_eq!(round_significant(123_456_789_050, 10), 123_456_789_100);
        assert_eq!(round_significant(-15, 1), -20);
    }
}
