//! Performance figures for a reporting period: categories (initial value,
//! gains, earnings, fees, taxes, transfers, final value) and IRR
//! (PP: ClientPerformanceSnapshot, ClientIRRYield).

use chrono::NaiveDate;
use serde::Serialize;

use crate::fx::CurrencyConverter;
use crate::math::irr;
use crate::model::{
    AccountTransactionType as AT, Client, PortfolioTransactionType as PT, UnitType,
};
use crate::money::AMOUNT_FACTOR;
use crate::security::SecurityPerformance;
use crate::snapshot::ClientSnapshot;

#[derive(Debug, Clone, Serialize)]
pub struct ClientPerformance {
    pub currency: String,
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub initial_value: i64,
    /// all price/currency gains (realized + unrealized)
    pub capital_gains: i64,
    pub realized_capital_gains: i64,
    pub unrealized_capital_gains: i64,
    pub earnings: i64,
    pub dividends: i64,
    pub interest: i64,
    pub fees: i64,
    pub taxes: i64,
    /// deposits - removals (incl. inbound/outbound deliveries)
    pub transfers: i64,
    pub deposits: i64,
    pub removals: i64,
    pub final_value: i64,
    pub absolute_delta: i64,
    pub irr: f64,
}

/// `interval` semantics as in PP: transactions in `(start, end]`.
pub fn client_performance(
    client: &Client,
    converter: &CurrencyConverter,
    start: NaiveDate,
    end: NaiveDate,
) -> ClientPerformance {
    let snap_start = ClientSnapshot::create(client, converter, start);
    let snap_end = ClientSnapshot::create(client, converter, end);
    let in_iv = |d: NaiveDate| d > start && d <= end;

    let (
        mut earnings,
        mut dividends,
        mut interest,
        mut fees,
        mut taxes,
        mut deposits,
        mut removals,
    ) = (0i64, 0i64, 0i64, 0i64, 0i64, 0i64, 0i64);

    let mut cash_flows: Vec<(NaiveDate, f64)> = Vec::new();

    for a in &client.accounts {
        for t in a.transactions.iter().filter(|t| in_iv(t.date())) {
            let d = t.date();
            let value = converter.convert_amount(d, &t.currency, t.amount);
            let conv = |v: i64| converter.convert_amount(d, &t.currency, v);
            match t.kind {
                AT::Dividends | AT::Interest => {
                    let tax = conv(t.unit_sum(UnitType::Tax));
                    let fee = conv(t.unit_sum(UnitType::Fee));
                    let gross = value + tax + fee;
                    earnings += gross;
                    if t.kind == AT::Dividends {
                        dividends += gross;
                    } else {
                        interest += gross;
                    }
                    taxes += tax;
                    fees += fee;
                }
                AT::InterestCharge => {
                    earnings -= value;
                    interest -= value;
                }
                AT::Deposit => deposits += value,
                AT::Removal => removals += value,
                AT::Fees => fees += value,
                AT::FeesRefund => fees -= value,
                AT::Taxes => taxes += value,
                AT::TaxRefund => taxes -= value,
                _ => {}
            }
            match t.kind {
                AT::Deposit | AT::TransferIn => cash_flows.push((d, -(value as f64))),
                AT::Removal | AT::TransferOut => cash_flows.push((d, value as f64)),
                _ => {}
            }
        }
    }
    for p in &client.portfolios {
        for t in p.transactions.iter().filter(|t| in_iv(t.date())) {
            let d = t.date();
            fees += converter.convert_amount(d, &t.currency, t.unit_sum(UnitType::Fee));
            taxes += converter.convert_amount(d, &t.currency, t.unit_sum(UnitType::Tax));
            let value = converter.convert_amount(d, &t.currency, t.amount);
            match t.kind {
                PT::DeliveryInbound => deposits += value,
                PT::DeliveryOutbound => removals += value,
                _ => {}
            }
            match t.kind {
                PT::DeliveryInbound | PT::TransferIn => cash_flows.push((d, -(value as f64))),
                PT::DeliveryOutbound | PT::TransferOut => cash_flows.push((d, value as f64)),
                _ => {}
            }
        }
    }

    let initial_value = snap_start.total();
    let final_value = snap_end.total();
    let transfers = deposits - removals;
    let capital_gains = final_value - initial_value - transfers - earnings + fees + taxes;

    let sec = SecurityPerformance::calculate(client, converter, start, end);
    let realized: i64 = sec.iter().map(|r| r.realized_gains).sum();

    // IRR
    cash_flows.sort_by_key(|c| c.0);
    let mut dates = Vec::new();
    let mut values = Vec::new();
    if initial_value != 0 {
        dates.push(start);
        values.push(-(initial_value as f64) / AMOUNT_FACTOR as f64);
    }
    for (d, v) in cash_flows {
        dates.push(d);
        values.push(v / AMOUNT_FACTOR as f64);
    }
    if final_value != 0 {
        dates.push(end);
        values.push(final_value as f64 / AMOUNT_FACTOR as f64);
    }
    let irr_value = if values.is_empty() {
        0.0
    } else {
        irr(&dates, &values)
    };

    ClientPerformance {
        currency: converter.term().to_string(),
        start,
        end,
        initial_value,
        capital_gains,
        realized_capital_gains: realized,
        unrealized_capital_gains: capital_gains - realized,
        earnings,
        dividends,
        interest,
        fees,
        taxes,
        transfers,
        deposits,
        removals,
        final_value,
        absolute_delta: final_value - initial_value - transfers,
        irr: irr_value,
    }
}
