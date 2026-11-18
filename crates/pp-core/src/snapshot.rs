//! Valuation of a client at a given date (PP: ClientSnapshot /
//! ClientSnapshotIterator).

use std::collections::HashMap;

use chrono::NaiveDate;
use serde::Serialize;

use crate::fx::CurrencyConverter;
use crate::model::{Client, PortfolioTransaction, Security};
use crate::money::{market_value, price_per_share};

/// Price used for the valuation; falls back to the gross price per share of
/// the last transaction if no quote exists (PP behaviour).
pub fn valuation_price(
    security: &Security,
    date: NaiveDate,
    last_tx: Option<&PortfolioTransaction>,
    converter: &CurrencyConverter,
) -> i64 {
    let p = security.price_at(date).value;
    if p != 0 {
        return p;
    }
    let Some(tx) = last_tx else { return 0 };
    let sec_currency = security.currency_or(&tx.currency);
    let gross = converter.with(sec_currency).convert_amount(tx.date(), &tx.currency, tx.gross_value_amount());
    price_per_share(gross, tx.shares)
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountSnapshot {
    pub account_id: String,
    pub currency: String,
    pub funds: i64,
    pub funds_converted: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PositionSnapshot {
    pub portfolio_id: String,
    pub security_id: String,
    pub currency: String,
    pub shares: i64,
    pub price: i64,
    pub price_date: NaiveDate,
    pub value: i64,
    pub value_converted: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ClientSnapshot {
    pub date: NaiveDate,
    pub currency: String,
    pub accounts: Vec<AccountSnapshot>,
    pub positions: Vec<PositionSnapshot>,
    /// positions merged across portfolios
    pub joint: Vec<PositionSnapshot>,
}

impl ClientSnapshot {
    pub fn create(client: &Client, converter: &CurrencyConverter, date: NaiveDate) -> Self {
        let accounts = client
            .accounts
            .iter()
            .map(|a| {
                let funds: i64 = a
                    .transactions
                    .iter()
                    .filter(|t| t.date() <= date)
                    .map(|t| if t.kind.is_credit() { t.amount } else { -t.amount })
                    .sum();
                AccountSnapshot {
                    account_id: a.id.clone(),
                    currency: a.currency.clone(),
                    funds,
                    funds_converted: converter.convert_amount(date, &a.currency, funds),
                }
            })
            .collect();

        let mut positions = Vec::new();
        for p in &client.portfolios {
            let mut by_sec: HashMap<&str, (i64, &PortfolioTransaction)> = HashMap::new();
            let mut order: Vec<&str> = Vec::new();
            for t in p.transactions.iter().filter(|t| t.date() <= date) {
                let Some(sid) = t.security.as_deref() else { continue };
                let e = by_sec.entry(sid).or_insert_with(|| {
                    order.push(sid);
                    (0, t)
                });
                e.0 += if t.kind.is_purchase() { t.shares } else { -t.shares };
                e.1 = t;
            }
            for sid in order {
                let (shares, last) = by_sec[sid];
                if shares == 0 {
                    continue;
                }
                let Some(sec) = client.security(sid) else { continue };
                let currency = sec.currency_or(&client.base_currency).to_string();
                let price = valuation_price(sec, date, Some(last), converter);
                let value = market_value(shares, price);
                positions.push(PositionSnapshot {
                    portfolio_id: p.id.clone(),
                    security_id: sid.to_string(),
                    value_converted: converter.convert_amount(date, &currency, value),
                    currency,
                    shares,
                    price,
                    price_date: sec.price_at(date).date,
                    value,
                });
            }
        }

        let joint = merge_positions(&positions, converter, date);
        Self { date, currency: converter.term().to_string(), accounts, positions, joint }
    }

    pub fn total_cash(&self) -> i64 {
        self.accounts.iter().map(|a| a.funds_converted).sum()
    }

    /// Value of all securities; positions are merged across portfolios first
    /// (PP: joint portfolio).
    pub fn total_securities(&self) -> i64 {
        self.joint.iter().map(|p| p.value_converted).sum()
    }

    /// Total assets (PP: `getMonetaryAssets`).
    pub fn total(&self) -> i64 {
        self.total_cash() + self.total_securities()
    }

    /// Positions merged over all portfolios (PP: joint portfolio).
    pub fn joint_positions(&self) -> &[PositionSnapshot] {
        &self.joint
    }
}

fn merge_positions(positions: &[PositionSnapshot], converter: &CurrencyConverter, date: NaiveDate) -> Vec<PositionSnapshot> {
    let mut map: HashMap<&str, PositionSnapshot> = HashMap::new();
    let mut order = Vec::new();
    for p in positions {
        match map.get_mut(p.security_id.as_str()) {
            Some(e) => e.shares += p.shares,
            None => {
                order.push(p.security_id.as_str());
                let mut c = p.clone();
                c.portfolio_id = String::new();
                map.insert(p.security_id.as_str(), c);
            }
        }
    }
    order
        .into_iter()
        .filter_map(|k| map.remove(k))
        .filter(|p| p.shares != 0)
        .map(|mut p| {
            p.value = market_value(p.shares, p.price);
            p.value_converted = converter.convert_amount(date, &p.currency, p.value);
            p
        })
        .collect()
}

/// Incremental valuation for strictly increasing dates (PP:
/// ClientSnapshotIterator). Positions are aggregated per security across all
/// portfolios.
pub struct ValuationIterator<'a> {
    client: &'a Client,
    converter: &'a CurrencyConverter,
    account_tx: Vec<(usize, Vec<(NaiveDate, i64)>)>,
    account_index: Vec<usize>,
    funds: Vec<i64>,
    portfolio_tx: Vec<&'a PortfolioTransaction>,
    portfolio_index: usize,
    securities: Vec<(&'a Security, i64, Option<&'a PortfolioTransaction>)>,
    security_slot: HashMap<&'a str, usize>,
}

impl<'a> ValuationIterator<'a> {
    pub fn new(client: &'a Client, converter: &'a CurrencyConverter) -> Self {
        let account_tx: Vec<_> = client
            .accounts
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let mut v: Vec<_> = a
                    .transactions
                    .iter()
                    .map(|t| (t.date(), if t.kind.is_credit() { t.amount } else { -t.amount }))
                    .collect();
                v.sort_by_key(|x| x.0);
                (i, v)
            })
            .collect();
        let mut portfolio_tx: Vec<_> = client.portfolios.iter().flat_map(|p| p.transactions.iter()).collect();
        portfolio_tx.sort_by_key(|t| t.date_time);
        let n = account_tx.len();
        Self {
            client,
            converter,
            account_tx,
            account_index: vec![0; n],
            funds: vec![0; n],
            portfolio_tx,
            portfolio_index: 0,
            securities: Vec::new(),
            security_slot: HashMap::new(),
        }
    }

    pub fn next_valuation(&mut self, date: NaiveDate) -> i64 {
        for (slot, (_, txs)) in self.account_tx.iter().enumerate() {
            let idx = &mut self.account_index[slot];
            while *idx < txs.len() && txs[*idx].0 <= date {
                self.funds[slot] += txs[*idx].1;
                *idx += 1;
            }
        }
        while self.portfolio_index < self.portfolio_tx.len() {
            let t = self.portfolio_tx[self.portfolio_index];
            if t.date() > date {
                break;
            }
            self.portfolio_index += 1;
            let Some(sid) = t.security.as_deref() else { continue };
            let slot = match self.security_slot.get(sid) {
                Some(s) => *s,
                None => {
                    let Some(sec) = self.client.security(sid) else { continue };
                    self.securities.push((sec, 0, None));
                    self.security_slot.insert(&sec.id, self.securities.len() - 1);
                    self.securities.len() - 1
                }
            };
            let e = &mut self.securities[slot];
            e.1 += if t.kind.is_purchase() { t.shares } else { -t.shares };
            e.2 = Some(t);
        }

        let mut valuation = 0i64;
        for (slot, (ai, _)) in self.account_tx.iter().enumerate() {
            let funds = self.funds[slot];
            if funds == 0 {
                continue;
            }
            let a = &self.client.accounts[*ai];
            valuation += self.converter.convert_amount(date, &a.currency, funds);
        }
        for (sec, shares, last) in &self.securities {
            if *shares == 0 {
                continue;
            }
            let price = valuation_price(sec, date, *last, self.converter);
            let mv = market_value(*shares, price);
            let cur = sec.currency_or(&self.client.base_currency);
            valuation += self.converter.convert_amount(date, cur, mv);
        }
        valuation
    }
}
