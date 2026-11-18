//! Per-security performance records: FIFO / moving average cost, realized and
//! unrealized gains, dividends, IRR (PP: SecurityPerformanceSnapshot).

use std::collections::HashMap;

use chrono::{NaiveDate, NaiveDateTime};
use serde::Serialize;

use crate::fx::CurrencyConverter;
use crate::math::irr;
use crate::model::{
    AccountTransactionType as AT, Client, PortfolioTransaction, PortfolioTransactionType as PT,
    Security, TransactionCommon, UnitType,
};
use crate::money::{AMOUNT_FACTOR, convert_amount, inverse_rate, market_value};
use crate::snapshot::valuation_price;

/// Sum of units of a kind in the term currency, preferring the forex amount
/// (PP: `getUnitSum(type, converter)`).
pub fn unit_sum_in(t: &TransactionCommon, kind: UnitType, conv: &CurrencyConverter) -> i64 {
    t.units
        .iter()
        .filter(|u| u.kind == kind)
        .map(|u| {
            if u.amount.currency == conv.term() {
                u.amount.amount
            } else if let Some(fx) = u.forex.as_ref().filter(|fx| fx.currency == conv.term()) {
                fx.amount
            } else {
                conv.convert_amount(t.date(), &u.amount.currency, u.amount.amount)
            }
        })
        .sum()
}

/// PP: `PortfolioTransaction#getMonetaryAmount(converter)`.
pub fn monetary_amount_in(
    t: &PortfolioTransaction,
    security: &Security,
    conv: &CurrencyConverter,
) -> i64 {
    if t.currency == conv.term() {
        return t.amount;
    }
    if security.currency.as_deref() == Some(conv.term()) {
        let rate = t
            .unit(UnitType::GrossValue)
            .and_then(|u| u.exchange_rate)
            .unwrap_or_else(|| conv.rate(t.date(), &t.currency));
        return convert_amount(t.amount, inverse_rate(rate));
    }
    conv.convert_amount(t.date(), &t.currency, t.amount)
}

/// PP: `PortfolioTransaction#getGrossValue(converter)`.
pub fn gross_value_in(
    t: &PortfolioTransaction,
    security: &Security,
    conv: &CurrencyConverter,
) -> i64 {
    if t.currency == conv.term() {
        return t.gross_value_amount();
    }
    if security.currency.as_deref() == Some(conv.term())
        && let Some(fx) = t.unit(UnitType::GrossValue).and_then(|u| u.forex.as_ref())
    {
        return fx.amount;
    }
    conv.convert_amount(t.date(), &t.currency, t.gross_value_amount())
}

#[derive(Debug, Clone, Serialize)]
pub struct SecurityRecord {
    pub security_id: String,
    pub currency: String,
    pub shares: i64,
    /// last quote in security currency
    pub quote: i64,
    pub quote_date: Option<NaiveDate>,
    pub market_value: i64,
    pub fifo_cost: i64,
    pub fifo_cost_net: i64,
    pub moving_average_cost: i64,
    pub moving_average_cost_net: i64,
    pub fees: i64,
    pub taxes: i64,
    pub dividends: i64,
    pub realized_gains: i64,
    pub unrealized_gains: i64,
    pub unrealized_gains_percent: f64,
    pub delta: i64,
    pub irr: f64,
}

struct Lot {
    owner: String,
    shares: i64,
    gross: i64,
    net: i64,
}

enum Item<'a> {
    Start {
        owner: String,
        shares: i64,
        value: i64,
    },
    Portfolio {
        owner: &'a str,
        tx: &'a PortfolioTransaction,
        other_owner: Option<&'a str>,
    },
    Account {
        tx: &'a crate::model::AccountTransaction,
    },
}

pub struct SecurityPerformance;

impl SecurityPerformance {
    /// Records for all securities with transactions, for transactions in
    /// `(start, end]`.
    pub fn calculate(
        client: &Client,
        conv: &CurrencyConverter,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Vec<SecurityRecord> {
        let in_iv = |d: NaiveDate| d > start && d <= end;
        let start_dt = start.and_hms_opt(0, 0, 0).unwrap();

        // collect items per security
        let mut items: HashMap<&str, Vec<(NaiveDateTime, u8, Item)>> = HashMap::new();
        let mut last_tx: HashMap<&str, &PortfolioTransaction> = HashMap::new();
        let mut shares_at_start: HashMap<(&str, &str), (i64, Option<&PortfolioTransaction>)> =
            HashMap::new();
        let mut end_shares: HashMap<&str, i64> = HashMap::new();

        for p in &client.portfolios {
            for t in &p.transactions {
                let Some(sid) = t.security.as_deref() else {
                    continue;
                };
                let d = t.date();
                let signed = if t.kind.is_purchase() {
                    t.shares
                } else {
                    -t.shares
                };
                if d <= start {
                    let e = shares_at_start
                        .entry((p.id.as_str(), sid))
                        .or_insert((0, None));
                    e.0 += signed;
                    e.1 = Some(t);
                }
                if d <= end {
                    *end_shares.entry(sid).or_default() += signed;
                    let lt = last_tx.entry(sid).or_insert(t);
                    if t.date_time >= lt.date_time {
                        *lt = t;
                    }
                }
                if in_iv(d) {
                    let other = t.cross_entry.as_ref().map(|c| c.other_owner.as_str());
                    items.entry(sid).or_default().push((
                        t.date_time,
                        1,
                        Item::Portfolio {
                            owner: &p.id,
                            tx: t,
                            other_owner: other,
                        },
                    ));
                }
            }
        }
        for ((owner, sid), (shares, last)) in &shares_at_start {
            if *shares == 0 {
                continue;
            }
            let Some(sec) = client.security(sid) else {
                continue;
            };
            let price = valuation_price(sec, start, *last, conv);
            let value = market_value(*shares, price);
            items.entry(sid).or_default().push((
                start_dt,
                0,
                Item::Start {
                    owner: owner.to_string(),
                    shares: *shares,
                    value,
                },
            ));
        }
        for a in &client.accounts {
            for t in &a.transactions {
                let Some(sid) = t.security.as_deref() else {
                    continue;
                };
                if !in_iv(t.date()) {
                    continue;
                }
                if matches!(
                    t.kind,
                    AT::Dividends | AT::Fees | AT::FeesRefund | AT::Taxes | AT::TaxRefund
                ) {
                    items
                        .entry(sid)
                        .or_default()
                        .push((t.date_time, 2, Item::Account { tx: t }));
                }
            }
        }

        let mut out = Vec::new();
        for sec in &client.securities {
            let Some(mut list) = items.remove(sec.id.as_str()) else {
                continue;
            };
            list.sort_by_key(|(dt, prio, _)| (*dt, *prio));
            out.push(Self::record(
                client,
                conv,
                sec,
                start,
                end,
                list,
                end_shares.get(sec.id.as_str()).copied(),
                last_tx.get(sec.id.as_str()).copied(),
            ));
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn record(
        client: &Client,
        conv: &CurrencyConverter,
        sec: &Security,
        start: NaiveDate,
        end: NaiveDate,
        items: Vec<(NaiveDateTime, u8, Item)>,
        end_shares: Option<i64>,
        last_tx: Option<&PortfolioTransaction>,
    ) -> SecurityRecord {
        let sec_currency = sec.currency_or(&client.base_currency).to_string();
        let mut fifo: Vec<Lot> = Vec::new();
        let (mut mv_cost, mut mv_net, mut held) = (0i64, 0i64, 0i64);
        let (mut fees, mut taxes, mut dividends, mut realized) = (0i64, 0i64, 0i64, 0i64);
        let mut flows: Vec<(NaiveDate, f64)> = Vec::new();
        let mut inflow = 0i64; // start value + buys
        let mut outflow = 0i64; // sells + dividends

        for (_, _, item) in &items {
            match item {
                Item::Start {
                    owner,
                    shares,
                    value,
                } => {
                    let amount = conv.convert_amount(start, &sec_currency, *value);
                    fifo.push(Lot {
                        owner: owner.clone(),
                        shares: *shares,
                        gross: amount,
                        net: amount,
                    });
                    mv_cost += amount;
                    mv_net += amount;
                    held += shares;
                    flows.push((start, -(amount as f64)));
                    inflow += amount;
                }
                Item::Portfolio {
                    owner,
                    tx: t,
                    other_owner,
                } => {
                    let fee = unit_sum_in(t, UnitType::Fee, conv);
                    let tax = unit_sum_in(t, UnitType::Tax, conv);
                    fees += fee;
                    taxes += tax;
                    let amount = monetary_amount_in(t, sec, conv);
                    match t.kind {
                        PT::Buy | PT::DeliveryInbound | PT::TransferIn => {
                            flows.push((t.date(), (-amount + tax) as f64));
                        }
                        _ => flows.push((t.date(), (amount + tax) as f64)),
                    }
                    match t.kind {
                        PT::Buy | PT::DeliveryInbound => {
                            let net = gross_value_in(t, sec, conv);
                            fifo.push(Lot {
                                owner: owner.to_string(),
                                shares: t.shares,
                                gross: amount,
                                net,
                            });
                            mv_cost += amount;
                            mv_net += net;
                            held += t.shares;
                            inflow += amount;
                        }
                        PT::Sell | PT::DeliveryOutbound => {
                            outflow += amount;
                            let mut sold = t.shares;
                            let remaining = held - sold;
                            if remaining <= 0 {
                                mv_cost = 0;
                                mv_net = 0;
                                held = 0;
                            } else {
                                mv_cost = (mv_cost as f64 / held as f64 * remaining as f64).round()
                                    as i64;
                                mv_net =
                                    (mv_net as f64 / held as f64 * remaining as f64).round() as i64;
                                held = remaining;
                            }
                            let sale_gross = gross_value_in(t, sec, conv);
                            let total_shares = t.shares;
                            let mut cost_of_sold = 0i64;
                            for lot in fifo.iter_mut() {
                                if sold <= 0 {
                                    break;
                                }
                                if lot.owner != *owner || lot.shares == 0 {
                                    continue;
                                }
                                let n = sold.min(lot.shares);
                                let g = (n as f64 / lot.shares as f64 * lot.gross as f64).round()
                                    as i64;
                                let nn =
                                    (n as f64 / lot.shares as f64 * lot.net as f64).round() as i64;
                                lot.gross -= g;
                                lot.net -= nn;
                                lot.shares -= n;
                                cost_of_sold += nn;
                                sold -= n;
                            }
                            let matched = total_shares - sold.max(0);
                            if total_shares > 0 && matched > 0 {
                                let proceeds = (sale_gross as f64 * matched as f64
                                    / total_shares as f64)
                                    .round() as i64;
                                realized += proceeds - cost_of_sold;
                            }
                        }
                        PT::TransferIn => {
                            let source = other_owner.unwrap_or_default();
                            let mut moved = t.shares;
                            let mut i = 0;
                            while i < fifo.len() && moved > 0 {
                                if fifo[i].owner != source || fifo[i].shares == 0 {
                                    i += 1;
                                    continue;
                                }
                                let n = moved.min(fifo[i].shares);
                                if n == fifo[i].shares {
                                    fifo[i].owner = owner.to_string();
                                } else {
                                    let lot = &mut fifo[i];
                                    let g = (n as f64 / lot.shares as f64 * lot.gross as f64)
                                        .round() as i64;
                                    let nn = (n as f64 / lot.shares as f64 * lot.net as f64).round()
                                        as i64;
                                    lot.gross -= g;
                                    lot.net -= nn;
                                    lot.shares -= n;
                                    fifo.insert(
                                        i + 1,
                                        Lot {
                                            owner: owner.to_string(),
                                            shares: n,
                                            gross: g,
                                            net: nn,
                                        },
                                    );
                                    i += 1;
                                }
                                moved -= n;
                                i += 1;
                            }
                        }
                        PT::TransferOut => {}
                    }
                }
                Item::Account { tx: t } => {
                    let d = t.date();
                    let value = conv.convert_amount(d, &t.currency, t.amount);
                    match t.kind {
                        AT::Dividends => {
                            let fee = unit_sum_in(t, UnitType::Fee, conv);
                            let tax = unit_sum_in(t, UnitType::Tax, conv);
                            fees += fee;
                            taxes += tax;
                            dividends += value;
                            outflow += value;
                            flows.push((d, (value + tax) as f64));
                        }
                        AT::Taxes => taxes += value,
                        AT::TaxRefund => taxes -= value,
                        AT::Fees => {
                            fees += value;
                            flows.push((d, -(value as f64)));
                        }
                        AT::FeesRefund => {
                            fees -= value;
                            flows.push((d, value as f64));
                        }
                        _ => {}
                    }
                }
            }
        }

        let shares = end_shares.unwrap_or(0);
        let price = valuation_price(sec, end, last_tx, conv);
        let mv_sec = market_value(shares, price);
        let market_value_conv = conv.convert_amount(end, &sec_currency, mv_sec);
        if market_value_conv != 0 {
            flows.push((end, market_value_conv as f64));
        }
        flows.sort_by_key(|f| f.0);
        let irr_value = if flows.is_empty() {
            f64::NAN
        } else {
            let dates: Vec<_> = flows.iter().map(|f| f.0).collect();
            let values: Vec<_> = flows.iter().map(|f| f.1 / AMOUNT_FACTOR as f64).collect();
            irr(&dates, &values)
        };

        let fifo_cost: i64 = fifo.iter().map(|l| l.gross).sum();
        let fifo_cost_net: i64 = fifo.iter().map(|l| l.net).sum();
        let unrealized = market_value_conv - fifo_cost;
        let quote = sec.price_at(end);
        SecurityRecord {
            security_id: sec.id.clone(),
            currency: sec_currency,
            shares,
            quote: price,
            quote_date: (quote.value != 0).then_some(quote.date),
            market_value: market_value_conv,
            fifo_cost,
            fifo_cost_net,
            moving_average_cost: mv_cost,
            moving_average_cost_net: mv_net,
            fees,
            taxes,
            dividends,
            realized_gains: realized,
            unrealized_gains: unrealized,
            unrealized_gains_percent: if fifo_cost != 0 {
                unrealized as f64 / fifo_cost as f64
            } else {
                0.0
            },
            delta: market_value_conv + outflow - inflow,
            irr: irr_value,
        }
    }
}

/// Dividend / interest payments per month in the term currency.
#[derive(Debug, Clone, Serialize)]
pub struct EarningEntry {
    pub date: NaiveDate,
    pub account_id: String,
    pub security_id: Option<String>,
    pub kind: AT,
    /// net amount (after taxes and fees)
    pub amount: i64,
    pub gross: i64,
    pub taxes: i64,
    pub fees: i64,
}

pub fn earnings(
    client: &Client,
    conv: &CurrencyConverter,
    start: NaiveDate,
    end: NaiveDate,
) -> Vec<EarningEntry> {
    let mut out = Vec::new();
    for a in &client.accounts {
        for t in &a.transactions {
            let d = t.date();
            if d <= start || d > end || !matches!(t.kind, AT::Dividends | AT::Interest) {
                continue;
            }
            let amount = conv.convert_amount(d, &t.currency, t.amount);
            let taxes = unit_sum_in(t, UnitType::Tax, conv);
            let fees = unit_sum_in(t, UnitType::Fee, conv);
            out.push(EarningEntry {
                date: d,
                account_id: a.id.clone(),
                security_id: t.security.clone(),
                kind: t.kind,
                amount,
                gross: amount + taxes + fees,
                taxes,
                fees,
            });
        }
    }
    out.sort_by_key(|e| e.date);
    out
}
