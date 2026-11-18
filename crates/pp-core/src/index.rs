//! Daily performance index of a client (PP: ClientIndex / PerformanceIndex).

use chrono::{Duration, NaiveDate};
use serde::Serialize;

use crate::fx::CurrencyConverter;
use crate::math::{Drawdown, Volatility, is_holiday};
use crate::model::{
    AccountTransactionType as AT, Client, PortfolioTransactionType as PT, UnitType,
};
use crate::snapshot::ValuationIterator;

#[derive(Debug, Clone, Serialize)]
pub struct PerformanceIndex {
    pub currency: String,
    pub dates: Vec<NaiveDate>,
    /// total valuation per day
    pub totals: Vec<i64>,
    /// daily return
    pub delta: Vec<f64>,
    /// cumulated TTWROR
    pub accumulated: Vec<f64>,
    pub inbound_transferals: Vec<i64>,
    pub outbound_transferals: Vec<i64>,
    pub taxes: Vec<i64>,
    pub dividends: Vec<i64>,
    pub interest: Vec<i64>,
    pub interest_charge: Vec<i64>,
    pub fees: Vec<i64>,
    pub buys: Vec<i64>,
    pub sells: Vec<i64>,
    pub warnings: Vec<String>,
}

impl PerformanceIndex {
    /// Calculates the index for `[start, end]`; `end` is capped at `today`.
    pub fn for_client(
        client: &Client,
        converter: &CurrencyConverter,
        start: NaiveDate,
        end: NaiveDate,
        today: NaiveDate,
    ) -> Self {
        let mut start = start;
        let mut end = end;
        if end > today {
            end = today;
            if start > end {
                start = end;
            }
        }
        let size = ((end - start).num_days() + 1).max(1) as usize;

        let mut idx = Self {
            currency: converter.term().to_string(),
            dates: Vec::with_capacity(size),
            totals: vec![0; size],
            delta: vec![0.0; size],
            accumulated: vec![0.0; size],
            inbound_transferals: vec![0; size],
            outbound_transferals: vec![0; size],
            taxes: vec![0; size],
            dividends: vec![0; size],
            interest: vec![0; size],
            interest_charge: vec![0; size],
            fees: vec![0; size],
            buys: vec![0; size],
            sells: vec![0; size],
            warnings: Vec::new(),
        };

        idx.collect_transferals(client, converter, start, end);

        let mut it = ValuationIterator::new(client, converter);
        idx.dates.push(start);
        let mut valuation = it.next_valuation(start);
        idx.totals[0] = valuation;

        let mut date = start + Duration::days(1);
        let mut i = 1;
        while date <= end && i < size {
            idx.dates.push(date);
            let this = it.next_valuation(date);
            idx.totals[i] = this;
            let inbound = idx.inbound_transferals[i];
            let outbound = idx.outbound_transferals[i];
            if valuation + inbound == 0 {
                idx.delta[i] = 0.0;
                let d = this - inbound + outbound - valuation;
                if d != 0 {
                    idx.warnings.push(format!(
                        "Wertänderung ohne Bestand am {date}: {:.2}",
                        d as f64 / 100.0
                    ));
                }
            } else {
                idx.delta[i] = (this + outbound) as f64 / (valuation + inbound) as f64 - 1.0;
            }
            idx.accumulated[i] = (idx.accumulated[i - 1] + 1.0) * (idx.delta[i] + 1.0) - 1.0;
            valuation = this;
            date += Duration::days(1);
            i += 1;
        }
        idx
    }

    fn collect_transferals(
        &mut self,
        client: &Client,
        conv: &CurrencyConverter,
        start: NaiveDate,
        end: NaiveDate,
    ) {
        let add = |arr: &mut Vec<i64>, d: NaiveDate, currency: &str, v: i64| {
            if v == 0 {
                return;
            }
            let i = (d - start).num_days() as usize;
            arr[i] += conv.convert_amount(d, currency, v);
        };
        for a in &client.accounts {
            for t in a
                .transactions
                .iter()
                .filter(|t| t.date() >= start && t.date() <= end)
            {
                let d = t.date();
                let c = t.currency.as_str();
                match t.kind {
                    AT::Deposit => add(&mut self.inbound_transferals, d, c, t.amount),
                    AT::Removal => add(&mut self.outbound_transferals, d, c, t.amount),
                    AT::Taxes => add(&mut self.taxes, d, c, t.amount),
                    AT::TaxRefund => add(&mut self.taxes, d, c, -t.amount),
                    AT::Dividends => {
                        add(&mut self.taxes, d, c, t.unit_sum(UnitType::Tax));
                        add(&mut self.dividends, d, c, t.amount);
                        add(&mut self.fees, d, c, t.unit_sum(UnitType::Fee));
                    }
                    AT::Interest => {
                        add(&mut self.taxes, d, c, t.unit_sum(UnitType::Tax));
                        add(&mut self.interest, d, c, t.amount);
                    }
                    AT::InterestCharge => {
                        add(&mut self.interest, d, c, -t.amount);
                        add(&mut self.interest_charge, d, c, t.amount);
                    }
                    AT::Fees => add(&mut self.fees, d, c, t.amount),
                    AT::FeesRefund => add(&mut self.fees, d, c, -t.amount),
                    _ => {}
                }
            }
        }
        for p in &client.portfolios {
            for t in p
                .transactions
                .iter()
                .filter(|t| t.date() >= start && t.date() <= end)
            {
                let d = t.date();
                let c = t.currency.as_str();
                add(&mut self.taxes, d, c, t.unit_sum(UnitType::Tax));
                add(&mut self.fees, d, c, t.unit_sum(UnitType::Fee));
                match t.kind {
                    PT::DeliveryInbound => add(&mut self.inbound_transferals, d, c, t.amount),
                    PT::DeliveryOutbound => add(&mut self.outbound_transferals, d, c, t.amount),
                    PT::Buy => add(&mut self.buys, d, c, t.amount),
                    PT::Sell => add(&mut self.sells, d, c, t.amount),
                    _ => {}
                }
            }
        }
    }

    pub fn final_accumulated(&self) -> f64 {
        *self.accumulated.last().unwrap_or(&0.0)
    }

    pub fn days(&self) -> i64 {
        match (self.dates.first(), self.dates.last()) {
            (Some(a), Some(b)) => (*b - *a).num_days(),
            _ => 0,
        }
    }

    pub fn final_accumulated_annualized(&self) -> f64 {
        let days = self.days();
        if days <= 0 {
            return 0.0;
        }
        (1.0 + self.final_accumulated()).powf(365.0 / days as f64) - 1.0
    }

    pub fn drawdown(&self) -> Drawdown {
        let start_at = self
            .totals
            .iter()
            .position(|t| *t != 0)
            .unwrap_or(self.totals.len().saturating_sub(1));
        Drawdown::new(&self.accumulated, &self.dates, start_at)
    }

    pub fn volatility(&self) -> Volatility {
        Volatility::new(&self.delta, |i| {
            i > 0 && self.totals[i] != 0 && self.totals[i - 1] != 0 && !is_holiday(self.dates[i])
        })
    }

    /// Aggregates the series to the last value per week / month / year.
    pub fn sample_indices(&self, interval: Interval) -> Vec<usize> {
        use chrono::Datelike;
        let n = self.dates.len();
        if n == 0 {
            return vec![];
        }
        let key = |d: &NaiveDate| -> (i32, u32) {
            match interval {
                Interval::Day => (d.year(), d.ordinal()),
                Interval::Week => (d.iso_week().year(), d.iso_week().week()),
                Interval::Month => (d.year(), d.month()),
                Interval::Year => (d.year(), 0),
            }
        };
        let mut out = Vec::new();
        for i in 0..n {
            if i + 1 == n || key(&self.dates[i]) != key(&self.dates[i + 1]) {
                out.push(i);
            }
        }
        if out.first() != Some(&0) {
            out.insert(0, 0);
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Interval {
    Day,
    Week,
    Month,
    Year,
}
