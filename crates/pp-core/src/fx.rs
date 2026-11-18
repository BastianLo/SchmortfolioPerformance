//! Exchange rates and currency conversion (PP semantics).

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, RwLock};

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::money::{Money, convert_amount, inverse_rate};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeRateSeries {
    pub base: String,
    pub term: String,
    /// sorted by date
    pub rates: Vec<(NaiveDate, Decimal)>,
}

impl ExchangeRateSeries {
    pub fn new(base: impl Into<String>, term: impl Into<String>) -> Self {
        Self {
            base: base.into(),
            term: term.into(),
            rates: Vec::new(),
        }
    }

    pub fn add(&mut self, date: NaiveDate, rate: Decimal) {
        match self.rates.binary_search_by_key(&date, |r| r.0) {
            Ok(i) => self.rates[i].1 = rate,
            Err(i) => self.rates.insert(i, (date, rate)),
        }
    }

    /// Last rate on or before `date`; first rate if `date` is before all rates.
    pub fn lookup(&self, date: NaiveDate) -> Option<Decimal> {
        if self.rates.is_empty() {
            return None;
        }
        Some(match self.rates.binary_search_by_key(&date, |r| r.0) {
            Ok(i) => self.rates[i].1,
            Err(0) => self.rates[0].1,
            Err(i) => self.rates[i - 1].1,
        })
    }
}

type PathCache = RwLock<HashMap<(String, String), Option<Vec<Step>>>>;

#[derive(Debug, Clone, Copy)]
enum Step {
    Direct(usize),
    Inverse(usize),
}

/// Holds all available exchange rate series and resolves currency pairs
/// directly, inverted or chained (shortest path).
#[derive(Debug, Default)]
pub struct ExchangeRateProvider {
    series: Vec<ExchangeRateSeries>,
    paths: PathCache,
}

impl ExchangeRateProvider {
    pub fn new(series: Vec<ExchangeRateSeries>) -> Self {
        Self {
            series,
            paths: RwLock::new(HashMap::new()),
        }
    }

    pub fn series(&self) -> &[ExchangeRateSeries] {
        &self.series
    }

    fn path(&self, base: &str, term: &str) -> Option<Vec<Step>> {
        let key = (base.to_string(), term.to_string());
        if let Some(p) = self.paths.read().unwrap().get(&key) {
            return p.clone();
        }
        let p = self.compute_path(base, term);
        self.paths.write().unwrap().insert(key, p.clone());
        p
    }

    fn compute_path(&self, base: &str, term: &str) -> Option<Vec<Step>> {
        // BFS over currencies
        let mut queue = VecDeque::new();
        let mut visited = HashSet::new();
        queue.push_back((base.to_string(), Vec::<Step>::new()));
        visited.insert(base.to_string());
        while let Some((cur, path)) = queue.pop_front() {
            if cur == term {
                return Some(path);
            }
            for (i, s) in self.series.iter().enumerate() {
                let (next, step) = if s.base == cur {
                    (s.term.clone(), Step::Direct(i))
                } else if s.term == cur {
                    (s.base.clone(), Step::Inverse(i))
                } else {
                    continue;
                };
                if visited.insert(next.clone()) {
                    let mut p = path.clone();
                    p.push(step);
                    queue.push_back((next, p));
                }
            }
        }
        None
    }

    /// Rate to convert one unit of `base` into `term` at `date`.
    pub fn rate(&self, date: NaiveDate, base: &str, term: &str) -> Option<Decimal> {
        if base == term {
            return Some(Decimal::ONE);
        }
        let path = self.path(base, term)?;
        let mut value = Decimal::ONE;
        for step in path {
            let r = match step {
                Step::Direct(i) => self.series[i].lookup(date)?,
                Step::Inverse(i) => inverse_rate(self.series[i].lookup(date)?),
            };
            value *= r;
        }
        Some(value)
    }
}

/// Converts money into a term currency.
#[derive(Debug, Clone)]
pub struct CurrencyConverter {
    provider: Arc<ExchangeRateProvider>,
    term: String,
    missing: Arc<RwLock<HashSet<String>>>,
}

impl CurrencyConverter {
    pub fn new(provider: Arc<ExchangeRateProvider>, term: impl Into<String>) -> Self {
        Self {
            provider,
            term: term.into(),
            missing: Default::default(),
        }
    }

    pub fn term(&self) -> &str {
        &self.term
    }

    pub fn with(&self, term: &str) -> Self {
        Self {
            provider: self.provider.clone(),
            term: term.to_string(),
            missing: self.missing.clone(),
        }
    }

    /// Rate from `currency` to the term currency; falls back to 1 (as PP does)
    /// and records the missing pair.
    pub fn rate(&self, date: NaiveDate, currency: &str) -> Decimal {
        if currency == self.term {
            return Decimal::ONE;
        }
        match self.provider.rate(date, currency, &self.term) {
            Some(r) => r,
            None => {
                self.missing
                    .write()
                    .unwrap()
                    .insert(format!("{currency}/{}", self.term));
                Decimal::ONE
            }
        }
    }

    pub fn convert_amount(&self, date: NaiveDate, currency: &str, amount: i64) -> i64 {
        if currency == self.term || amount == 0 {
            return amount;
        }
        convert_amount(amount, self.rate(date, currency))
    }

    pub fn convert(&self, date: NaiveDate, money: &Money) -> Money {
        Money::new(
            self.term.clone(),
            self.convert_amount(date, &money.currency, money.amount),
        )
    }

    /// Currency pairs for which no exchange rate was found so far.
    pub fn missing_pairs(&self) -> Vec<String> {
        let mut v: Vec<_> = self.missing.read().unwrap().iter().cloned().collect();
        v.sort();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::from_str(s).unwrap()
    }

    #[test]
    fn inverse_and_chain() {
        let mut usd = ExchangeRateSeries::new("EUR", "USD");
        usd.add(d("2015-01-16"), Decimal::from_str("1.1588").unwrap());
        let mut chf = ExchangeRateSeries::new("EUR", "CHF");
        chf.add(d("2015-01-16"), Decimal::from_str("1.0128").unwrap());
        let p = Arc::new(ExchangeRateProvider::new(vec![usd, chf]));
        let c = CurrencyConverter::new(p.clone(), "EUR");
        assert_eq!(
            c.convert_amount(d("2015-01-16"), "USD", 100_000),
            (100_000f64 / 1.1588).round() as i64
        );
        // chained USD -> EUR -> CHF
        let r = p.rate(d("2015-01-20"), "USD", "CHF").unwrap();
        assert!((r.to_string().parse::<f64>().unwrap() - 1.0128 / 1.1588).abs() < 1e-8);
    }
}
