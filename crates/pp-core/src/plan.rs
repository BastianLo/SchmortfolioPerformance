//! Investment plans (Sparpläne), with the same scheduling rules as PP.

use chrono::{Datelike, Duration, Months, NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};

use crate::booking::{Booking, BookingKind};
use crate::fx::CurrencyConverter;
use crate::math::is_holiday;
use crate::model::{Client, Id};
use crate::money::{AMOUNT_FACTOR, QUOTE_FACTOR, SHARES_FACTOR, convert_amount};

/// Intervals above this value are weekly (`interval - 100` weeks).
pub const WEEKS_THRESHOLD: i32 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PlanType {
    PurchaseOrDelivery,
    Deposit,
    Removal,
    Interest,
}

impl PlanType {
    pub fn as_str(self) -> &'static str {
        match self {
            PlanType::PurchaseOrDelivery => "PURCHASE_OR_DELIVERY",
            PlanType::Deposit => "DEPOSIT",
            PlanType::Removal => "REMOVAL",
            PlanType::Interest => "INTEREST",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "PURCHASE_OR_DELIVERY" => PlanType::PurchaseOrDelivery,
            "DEPOSIT" => PlanType::Deposit,
            "REMOVAL" => PlanType::Removal,
            "INTEREST" => PlanType::Interest,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct InvestmentPlan {
    pub id: Id,
    pub name: String,
    pub note: Option<String>,
    pub kind: PlanType,
    pub security: Option<Id>,
    pub portfolio: Option<Id>,
    /// for purchases: without account the plan creates deliveries
    pub account: Option<Id>,
    pub auto_generate: bool,
    pub start: NaiveDateTime,
    /// months (1..99) or weeks + 100 (101..)
    pub interval: i32,
    pub amount: i64,
    pub fees: i64,
    pub taxes: i64,
    /// ids of the generated transactions
    pub transactions: Vec<Id>,
}

fn with_day(d: NaiveDate, day: u32) -> NaiveDate {
    let last = (d.with_day(1).unwrap() + Months::new(1) - Duration::days(1)).day();
    d.with_day(day.min(last)).unwrap()
}

impl InvestmentPlan {
    fn last_date(&self, client: &Client) -> Option<NaiveDate> {
        let mut last = None;
        for id in &self.transactions {
            if let Some((_, _, t, _)) = crate::booking::find(client, id) {
                let d = t.date();
                if last.is_none_or(|l| d > l) {
                    last = Some(d);
                }
            }
        }
        last
    }

    /// Next execution after `date` (PP: `InvestmentPlan#next`).
    pub fn next(&self, date: NaiveDate) -> NaiveDate {
        let start = self.start.date();
        let mut prev = date;
        let next = if self.interval < WEEKS_THRESHOLD {
            if prev.day() != start.day() {
                // the transaction was moved (e.g. weekend); find the scheduled date
                let mut best = prev;
                let mut best_diff = i64::MAX;
                let mut test = with_day(prev - Months::new(1), start.day());
                for _ in 0..3 {
                    let diff = (test - prev).num_days().abs();
                    if diff < best_diff {
                        best_diff = diff;
                        best = test;
                    }
                    test = with_day(test + Months::new(1), start.day());
                }
                prev = best;
            }
            with_day(prev + Months::new(self.interval.max(1) as u32), start.day())
        } else {
            prev + Duration::weeks((self.interval - WEEKS_THRESHOLD).max(1) as i64)
        };
        let mut next = if next < start { start } else { next };
        while is_holiday(next) {
            next += Duration::days(1);
        }
        next
    }

    pub fn next_date(&self, client: &Client) -> NaiveDate {
        match self.last_date(client) {
            Some(d) => self.next(d),
            None => {
                let mut d = self.start.date();
                while is_holiday(d) {
                    d += Duration::days(1);
                }
                d
            }
        }
    }

    /// Bookings due until `today` (does not modify the plan).
    pub fn due_bookings(
        &self,
        client: &Client,
        conv: &CurrencyConverter,
        today: NaiveDate,
    ) -> Result<Vec<Booking>, String> {
        let mut out = Vec::new();
        let mut date = self.next_date(client);
        let mut guard = 0;
        while date <= today && guard < 1000 {
            out.push(self.booking(client, conv, date)?);
            date = self.next(date);
            guard += 1;
        }
        Ok(out)
    }

    fn booking(
        &self,
        client: &Client,
        conv: &CurrencyConverter,
        date: NaiveDate,
    ) -> Result<Booking, String> {
        let note = Some(format!("Sparplan {}", self.name));
        let base = Booking {
            kind: BookingKind::Deposit,
            date_time: date.and_hms_opt(0, 0, 0).unwrap(),
            account_id: self.account.clone(),
            portfolio_id: self.portfolio.clone(),
            target_account_id: None,
            target_portfolio_id: None,
            security_id: None,
            shares: 0,
            amount: self.amount,
            fees: self.fees,
            taxes: self.taxes,
            exchange_rate: None,
            target_amount: None,
            note,
            source: None,
        };
        match self.kind {
            PlanType::Deposit => Ok(Booking {
                fees: 0,
                taxes: 0,
                ..base
            }),
            PlanType::Removal => Ok(Booking {
                kind: BookingKind::Removal,
                fees: 0,
                taxes: 0,
                ..base
            }),
            PlanType::Interest => Ok(Booking {
                kind: BookingKind::Interest,
                ..base
            }),
            PlanType::PurchaseOrDelivery => {
                let sid = self.security.as_deref().ok_or("Sparplan ohne Wertpapier")?;
                let sec = client.security(sid).ok_or("Wertpapier unbekannt")?;
                let price = sec.price_at(date).value;
                if price == 0 {
                    return Err(format!("Kein Kurs für {} vorhanden", sec.name));
                }
                let currency = match &self.account {
                    Some(a) => client.account(a).ok_or("Konto unbekannt")?.currency.clone(),
                    None => self
                        .portfolio
                        .as_deref()
                        .and_then(|p| client.portfolio(p))
                        .and_then(|p| p.reference_account.as_deref())
                        .and_then(|a| client.account(a))
                        .map(|a| a.currency.clone())
                        .unwrap_or_else(|| sec.currency_or(&client.base_currency).to_string()),
                };
                let sec_cur = sec.currency_or(&currency).to_string();
                let mut available = self.amount - self.fees - self.taxes;
                let mut rate = None;
                if sec_cur != currency {
                    let r = conv.with(&currency).rate(date, &sec_cur);
                    rate = Some(r);
                    available = convert_amount(available, crate::money::inverse_rate(r));
                }
                let shares = (available as f64
                    * SHARES_FACTOR as f64
                    * (QUOTE_FACTOR / AMOUNT_FACTOR) as f64
                    / price as f64)
                    .round() as i64;
                Ok(Booking {
                    kind: if self.account.is_some() {
                        BookingKind::Buy
                    } else {
                        BookingKind::DeliveryInbound
                    },
                    security_id: Some(sid.to_string()),
                    shares,
                    exchange_rate: rate,
                    ..base
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::*;
    use std::sync::Arc;

    fn d(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn plan(start: &str, interval: i32) -> InvestmentPlan {
        InvestmentPlan {
            id: "p".into(),
            name: "ETF".into(),
            note: None,
            kind: PlanType::PurchaseOrDelivery,
            security: Some("s".into()),
            portfolio: Some("pf".into()),
            account: Some("a".into()),
            auto_generate: true,
            start: d(start).and_hms_opt(0, 0, 0).unwrap(),
            interval,
            amount: 100_00,
            fees: 1_00,
            taxes: 0,
            transactions: vec![],
        }
    }

    #[test]
    fn monthly_schedule_keeps_day_and_skips_holidays() {
        let p = plan("2024-01-31", 1);
        assert_eq!(p.next(d("2024-01-31")), d("2024-02-29"));
        // 2024-03-31 is a Sunday -> Easter Monday 2024-04-01 is a holiday too -> 2024-04-02
        assert_eq!(p.next(d("2024-02-29")), d("2024-04-02"));
        // moved date is mapped back to its scheduled day
        assert_eq!(p.next(d("2024-04-02")), d("2024-04-30"));
        let w = plan("2024-01-03", 102);
        assert_eq!(w.next(d("2024-01-03")), d("2024-01-17"));
    }

    #[test]
    fn generates_due_purchases() {
        let mut c = Client {
            base_currency: "EUR".into(),
            ..Default::default()
        };
        c.securities.push(Security {
            id: "s".into(),
            name: "ETF".into(),
            currency: Some("EUR".into()),
            prices: vec![SecurityPrice {
                date: d("2024-01-01"),
                value: 50 * QUOTE_FACTOR,
            }],
            ..Default::default()
        });
        c.accounts.push(Account {
            id: "a".into(),
            name: "A".into(),
            currency: "EUR".into(),
            ..Default::default()
        });
        c.portfolios.push(Portfolio {
            id: "pf".into(),
            name: "P".into(),
            reference_account: Some("a".into()),
            ..Default::default()
        });
        let conv =
            CurrencyConverter::new(Arc::new(crate::fx::ExchangeRateProvider::default()), "EUR");
        let p = plan("2024-01-15", 1);
        let b = p.due_bookings(&c, &conv, d("2024-03-20")).unwrap();
        assert_eq!(b.len(), 3);
        assert_eq!(b[0].kind, BookingKind::Buy);
        assert_eq!(b[0].shares, 198_000_000); // 99 EUR / 50 EUR
        assert_eq!(b[1].date_time.date(), d("2024-02-15"));
        // after the first execution only the remaining ones are due
        let txs = b[0].build(&c, None, None).unwrap();
        let mut p2 = p.clone();
        p2.transactions.push(txs[0].id().to_string());
        for t in txs {
            match t {
                crate::booking::StoredTx::Portfolio { tx, .. } => {
                    c.portfolios[0].transactions.push(tx)
                }
                crate::booking::StoredTx::Account { tx, .. } => c.accounts[0].transactions.push(tx),
            }
        }
        assert_eq!(
            p2.due_bookings(&c, &conv, d("2024-03-20")).unwrap().len(),
            2
        );
    }
}
