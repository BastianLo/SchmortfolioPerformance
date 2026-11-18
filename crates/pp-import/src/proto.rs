//! Reader for the protobuf based binary format of Portfolio Performance.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use pp_core::Money;
use pp_core::model::*;
use prost::Message;
use rust_decimal::Decimal;

use crate::ImportError;

#[allow(clippy::all, dead_code)]
pub mod pb {
    include!(concat!(env!("OUT_DIR"), "/name.abuchen.portfolio.rs"));
}

use pb::p_transaction::Type as TT;

fn epoch_day(d: i64) -> NaiveDate {
    NaiveDate::from_ymd_opt(1970, 1, 1).unwrap() + chrono::Duration::days(d)
}

fn ts(t: &Option<prost_types::Timestamp>) -> Option<NaiveDateTime> {
    t.as_ref()
        .and_then(|t| DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32))
        .map(|d| d.naive_utc())
}

fn ts_string(t: &Option<prost_types::Timestamp>) -> Option<String> {
    t.as_ref()
        .and_then(|t| DateTime::from_timestamp(t.seconds, t.nanos.max(0) as u32))
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true))
}

fn decimal(v: &pb::PDecimalValue) -> Option<Decimal> {
    if v.value.is_empty() {
        return Some(Decimal::ZERO);
    }
    // big-endian two's complement
    let mut n: i128 = if v.value[0] & 0x80 != 0 { -1 } else { 0 };
    for b in &v.value {
        n = n.checked_shl(8)? | (*b as i128);
    }
    Decimal::try_from_i128_with_scale(n, v.scale)
        .ok()
        .map(|d| d.normalize())
}

fn any_to_json(v: &pb::PAnyValue) -> serde_json::Value {
    use pb::p_any_value::Kind;
    match &v.kind {
        Some(Kind::String(s)) => s.clone().into(),
        Some(Kind::Int32(i)) => (*i).into(),
        Some(Kind::Int64(i)) => (*i).into(),
        Some(Kind::Double(d)) => serde_json::Number::from_f64(*d)
            .map(serde_json::Value::Number)
            .unwrap_or_default(),
        Some(Kind::Bool(b)) => (*b).into(),
        Some(Kind::Map(m)) => kv_to_json(&m.entries),
        Some(Kind::Null(_)) | None => serde_json::Value::Null,
    }
}

fn kv_to_json(kv: &[pb::PKeyValue]) -> serde_json::Value {
    serde_json::Value::Object(
        kv.iter()
            .map(|e| {
                (
                    e.key.clone(),
                    e.value.as_ref().map(any_to_json).unwrap_or_default(),
                )
            })
            .collect(),
    )
}

pub(crate) fn read_protobuf(bytes: &[u8]) -> Result<(Client, Vec<String>), ImportError> {
    let pc = pb::PClient::decode(bytes)
        .map_err(|e| ImportError::Format(format!("Protobuf ungültig: {e}")))?;
    let mut warnings = Vec::new();

    let securities: Vec<Security> = pc
        .securities
        .iter()
        .map(|s| Security {
            id: s.uuid.clone(),
            name: s.name.clone(),
            currency: s.currency_code.clone(),
            target_currency: s.target_currency_code.clone(),
            isin: s.isin.clone(),
            wkn: s.wkn.clone(),
            ticker: s.ticker_symbol.clone(),
            online_id: s.online_id.clone(),
            calendar: s.calendar.clone(),
            feed: s.feed.clone(),
            feed_url: s.feed_url.clone(),
            latest_feed: s.latest_feed.clone(),
            latest_feed_url: s.latest_feed_url.clone(),
            note: s.note.clone(),
            retired: s.is_retired,
            prices: s
                .prices
                .iter()
                .map(|p| SecurityPrice {
                    date: epoch_day(p.date),
                    value: p.close,
                })
                .collect(),
            latest: s.latest.as_ref().map(|l| LatestSecurityPrice {
                date: epoch_day(l.date),
                value: l.close,
                high: l.high,
                low: l.low,
                volume: l.volume,
            }),
            events: s
                .events
                .iter()
                .map(|e| SecurityEvent {
                    kind: match e.r#type() {
                        pb::p_security_event::Type::StockSplit => SecurityEventKind::StockSplit,
                        pb::p_security_event::Type::Note => SecurityEventKind::Note,
                        pb::p_security_event::Type::DividendPayment => {
                            SecurityEventKind::DividendPayment
                        }
                    },
                    date: epoch_day(e.date),
                    details: Some(e.details.clone()).filter(|d| !d.is_empty()),
                    data: serde_json::Value::Array(e.data.iter().map(any_to_json).collect()),
                })
                .collect(),
            attributes: kv_to_json(&s.attributes),
            updated_at: ts_string(&s.updated_at),
        })
        .collect();

    let mut accounts: Vec<Account> = pc
        .accounts
        .iter()
        .map(|a| Account {
            id: a.uuid.clone(),
            name: a.name.clone(),
            currency: a.currency_code.clone(),
            note: a.note.clone(),
            retired: a.is_retired,
            transactions: vec![],
            attributes: kv_to_json(&a.attributes),
            updated_at: ts_string(&a.updated_at),
        })
        .collect();
    let mut portfolios: Vec<Portfolio> = pc
        .portfolios
        .iter()
        .map(|p| Portfolio {
            id: p.uuid.clone(),
            name: p.name.clone(),
            note: p.note.clone(),
            retired: p.is_retired,
            reference_account: p.reference_account.clone(),
            transactions: vec![],
            attributes: kv_to_json(&p.attributes),
            updated_at: ts_string(&p.updated_at),
        })
        .collect();
    let acc_idx: HashMap<String, usize> = accounts
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id.clone(), i))
        .collect();
    let pf_idx: HashMap<String, usize> = portfolios
        .iter()
        .enumerate()
        .map(|(i, p)| (p.id.clone(), i))
        .collect();

    for t in &pc.transactions {
        let Some(date_time) = ts(&t.date) else {
            warnings.push(format!("Buchung {} ohne Datum übersprungen", t.uuid));
            continue;
        };
        let units: Vec<TransactionUnit> = t
            .units
            .iter()
            .map(|u| TransactionUnit {
                kind: match u.r#type() {
                    pb::p_transaction_unit::Type::GrossValue => UnitType::GrossValue,
                    pb::p_transaction_unit::Type::Tax => UnitType::Tax,
                    pb::p_transaction_unit::Type::Fee => UnitType::Fee,
                },
                amount: Money::new(u.currency_code.clone(), u.amount),
                forex: u
                    .fx_amount
                    .map(|a| Money::new(u.fx_currency_code.clone().unwrap_or_default(), a)),
                exchange_rate: u.fx_rate_to_base.as_ref().and_then(decimal),
            })
            .collect();
        let common = |id: &str,
                      units: Vec<TransactionUnit>,
                      cross: Option<CrossEntry>,
                      updated: &Option<prost_types::Timestamp>| {
            TransactionCommon {
                id: id.to_string(),
                date_time,
                currency: t.currency_code.clone(),
                amount: t.amount,
                shares: t.shares.unwrap_or(0),
                security: t.security.clone(),
                note: t.note.clone(),
                source: t.source.clone(),
                units,
                cross_entry: cross,
                updated_at: ts_string(updated),
            }
        };
        let account = t.account.as_ref().and_then(|a| acc_idx.get(a).copied());
        let portfolio = t.portfolio.as_ref().and_then(|p| pf_idx.get(p).copied());
        let other_uuid = t
            .other_uuid
            .clone()
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        let simple_account = |kind: AccountTransactionType| -> Option<(usize, AccountTransaction)> {
            Some((
                account?,
                AccountTransaction {
                    kind,
                    common: common(&t.uuid, units.clone(), None, &t.updated_at),
                    ex_date: t.ex_date.as_ref().map(|d| {
                        epoch_day(d.epoch_day).and_hms_opt(0, 0, 0).unwrap()
                            + chrono::Duration::seconds(d.second_of_day as i64)
                    }),
                },
            ))
        };

        match t.r#type() {
            TT::Purchase | TT::Sale => {
                let (Some(ai), Some(pi)) = (account, portfolio) else {
                    warnings.push(format!("Kauf/Verkauf {} ohne Konto/Depot", t.uuid));
                    continue;
                };
                let buy = t.r#type() == TT::Purchase;
                let ptx = PortfolioTransaction {
                    kind: if buy {
                        PortfolioTransactionType::Buy
                    } else {
                        PortfolioTransactionType::Sell
                    },
                    common: common(
                        &t.uuid,
                        units.clone(),
                        Some(CrossEntry {
                            kind: CrossEntryKind::BuySell,
                            other_owner: accounts[ai].id.clone(),
                            other_transaction: other_uuid.clone(),
                        }),
                        &t.updated_at,
                    ),
                };
                let mut atx_common = common(
                    &other_uuid,
                    vec![],
                    Some(CrossEntry {
                        kind: CrossEntryKind::BuySell,
                        other_owner: portfolios[pi].id.clone(),
                        other_transaction: t.uuid.clone(),
                    }),
                    &t.other_updated_at,
                );
                atx_common.shares = 0;
                accounts[ai].transactions.push(AccountTransaction {
                    kind: if buy {
                        AccountTransactionType::Buy
                    } else {
                        AccountTransactionType::Sell
                    },
                    common: atx_common,
                    ex_date: None,
                });
                portfolios[pi].transactions.push(ptx);
            }
            TT::SecurityTransfer => {
                let target = t
                    .other_portfolio
                    .as_ref()
                    .and_then(|p| pf_idx.get(p).copied());
                let (Some(si), Some(ti)) = (portfolio, target) else {
                    warnings.push(format!("Depotwechsel {} unvollständig", t.uuid));
                    continue;
                };
                let src_id = portfolios[si].id.clone();
                let tgt_id = portfolios[ti].id.clone();
                portfolios[si].transactions.push(PortfolioTransaction {
                    kind: PortfolioTransactionType::TransferOut,
                    common: common(
                        &t.uuid,
                        units.clone(),
                        Some(CrossEntry {
                            kind: CrossEntryKind::PortfolioTransfer,
                            other_owner: tgt_id,
                            other_transaction: other_uuid.clone(),
                        }),
                        &t.updated_at,
                    ),
                });
                portfolios[ti].transactions.push(PortfolioTransaction {
                    kind: PortfolioTransactionType::TransferIn,
                    common: common(
                        &other_uuid,
                        vec![],
                        Some(CrossEntry {
                            kind: CrossEntryKind::PortfolioTransfer,
                            other_owner: src_id,
                            other_transaction: t.uuid.clone(),
                        }),
                        &t.other_updated_at,
                    ),
                });
            }
            TT::CashTransfer => {
                let target = t
                    .other_account
                    .as_ref()
                    .and_then(|a| acc_idx.get(a).copied());
                let (Some(si), Some(ti)) = (account, target) else {
                    warnings.push(format!("Umbuchung {} unvollständig", t.uuid));
                    continue;
                };
                let src_id = accounts[si].id.clone();
                let tgt_id = accounts[ti].id.clone();
                let mut target_common = common(
                    &other_uuid,
                    vec![],
                    Some(CrossEntry {
                        kind: CrossEntryKind::AccountTransfer,
                        other_owner: src_id,
                        other_transaction: t.uuid.clone(),
                    }),
                    &t.other_updated_at,
                );
                if let Some(fx) = units
                    .iter()
                    .find(|u| u.kind == UnitType::GrossValue)
                    .and_then(|u| u.forex.clone())
                {
                    target_common.amount = fx.amount;
                    target_common.currency = fx.currency;
                }
                accounts[si].transactions.push(AccountTransaction {
                    kind: AccountTransactionType::TransferOut,
                    common: common(
                        &t.uuid,
                        units.clone(),
                        Some(CrossEntry {
                            kind: CrossEntryKind::AccountTransfer,
                            other_owner: tgt_id,
                            other_transaction: other_uuid.clone(),
                        }),
                        &t.updated_at,
                    ),
                    ex_date: None,
                });
                accounts[ti].transactions.push(AccountTransaction {
                    kind: AccountTransactionType::TransferIn,
                    common: target_common,
                    ex_date: None,
                });
            }
            TT::InboundDelivery | TT::OutboundDelivery => {
                let Some(pi) = portfolio else {
                    warnings.push(format!("Einlieferung {} ohne Depot", t.uuid));
                    continue;
                };
                portfolios[pi].transactions.push(PortfolioTransaction {
                    kind: if t.r#type() == TT::InboundDelivery {
                        PortfolioTransactionType::DeliveryInbound
                    } else {
                        PortfolioTransactionType::DeliveryOutbound
                    },
                    common: common(&t.uuid, units.clone(), None, &t.updated_at),
                });
            }
            other => {
                use AccountTransactionType as A;
                let mut kind = match other {
                    TT::Deposit => A::Deposit,
                    TT::Removal => A::Removal,
                    TT::Dividend => A::Dividends,
                    TT::Interest => A::Interest,
                    TT::InterestCharge => A::InterestCharge,
                    TT::Tax => A::Taxes,
                    TT::TaxRefund => A::TaxRefund,
                    TT::Fee => A::Fees,
                    TT::FeeRefund => A::FeesRefund,
                    _ => unreachable!(),
                };
                if kind == A::Dividends && t.security.is_none() {
                    kind = A::Interest;
                }
                match simple_account(kind) {
                    Some((ai, tx)) => accounts[ai].transactions.push(tx),
                    None => warnings.push(format!("Kontobuchung {} ohne Konto", t.uuid)),
                }
            }
        }
    }

    let taxonomies = pc
        .taxonomies
        .iter()
        .map(|t| Taxonomy {
            id: t.id.clone(),
            name: t.name.clone(),
            source: t.source.clone(),
            dimensions: t.dimensions.clone(),
            classifications: t
                .classifications
                .iter()
                .map(|c| Classification {
                    id: c.id.clone(),
                    parent_id: c.parent_id.clone(),
                    name: c.name.clone(),
                    note: c.note.clone(),
                    color: Some(c.color.clone()).filter(|c| !c.is_empty()),
                    weight: c.weight,
                    rank: c.rank,
                    assignments: c
                        .assignments
                        .iter()
                        .map(|a| Assignment {
                            vehicle: a.investment_vehicle.clone(),
                            weight: a.weight,
                            rank: a.rank,
                        })
                        .collect(),
                    data: kv_to_json(&c.data),
                })
                .collect(),
        })
        .collect();

    let watchlists = pc
        .watchlists
        .iter()
        .map(|w| Watchlist {
            name: w.name.clone(),
            securities: w.securities.clone(),
        })
        .collect();

    let client = Client {
        version: pc.version,
        base_currency: if pc.base_currency.is_empty() {
            "EUR".into()
        } else {
            pc.base_currency.clone()
        },
        securities,
        accounts,
        portfolios,
        taxonomies,
        watchlists,
        plans: pc
            .plans
            .iter()
            .map(|p| pp_core::plan::InvestmentPlan {
                id: uuid::Uuid::new_v4().to_string(),
                name: p.name.clone(),
                note: p.note.clone(),
                kind: match p.r#type() {
                    pb::p_investment_plan::Type::PurchaseOrDelivery => {
                        pp_core::plan::PlanType::PurchaseOrDelivery
                    }
                    pb::p_investment_plan::Type::Deposit => pp_core::plan::PlanType::Deposit,
                    pb::p_investment_plan::Type::Removal => pp_core::plan::PlanType::Removal,
                    pb::p_investment_plan::Type::Interest => pp_core::plan::PlanType::Interest,
                },
                security: p.security.clone(),
                portfolio: p.portfolio.clone(),
                account: p.account.clone(),
                auto_generate: p.auto_generate,
                start: epoch_day(p.date).and_hms_opt(0, 0, 0).unwrap(),
                interval: p.interval,
                amount: p.amount,
                fees: p.fees,
                taxes: p.taxes,
                transactions: p.transactions.clone(),
            })
            .collect(),
        extra: serde_json::json!({
            "properties": pc.properties,
        }),
    };
    Ok((client, warnings))
}
