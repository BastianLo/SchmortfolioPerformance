//! Creating transactions from user input ("bookings").
//!
//! A booking describes a business event (buy, transfer, dividend, ...). It is
//! turned into one or two linked transactions with the correct units, the
//! same way Portfolio Performance stores them. `reconstruct` is the inverse
//! and is used for editing.

use chrono::NaiveDateTime;
use rust_decimal::prelude::*;
use serde::{Deserialize, Serialize};

use crate::model::*;
use crate::money::Money;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BookingKind {
    Buy,
    Sell,
    DeliveryInbound,
    DeliveryOutbound,
    SecurityTransfer,
    CashTransfer,
    Deposit,
    Removal,
    Dividends,
    Interest,
    InterestCharge,
    Fees,
    FeesRefund,
    Taxes,
    TaxRefund,
}

impl BookingKind {
    fn account_type(self) -> Option<AccountTransactionType> {
        use AccountTransactionType as A;
        Some(match self {
            BookingKind::Deposit => A::Deposit,
            BookingKind::Removal => A::Removal,
            BookingKind::Dividends => A::Dividends,
            BookingKind::Interest => A::Interest,
            BookingKind::InterestCharge => A::InterestCharge,
            BookingKind::Fees => A::Fees,
            BookingKind::FeesRefund => A::FeesRefund,
            BookingKind::Taxes => A::Taxes,
            BookingKind::TaxRefund => A::TaxRefund,
            _ => return None,
        })
    }
}

/// User input for a transaction. Amounts use the usual factors (money 100,
/// shares 1e8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct Booking {
    pub kind: BookingKind,
    pub date_time: NaiveDateTime,
    /// account (cash side, source for cash transfers)
    pub account_id: Option<String>,
    /// portfolio (securities side, source for security transfers)
    pub portfolio_id: Option<String>,
    /// target of a cash transfer
    pub target_account_id: Option<String>,
    /// target of a security transfer
    pub target_portfolio_id: Option<String>,
    pub security_id: Option<String>,
    #[serde(default)]
    pub shares: i64,
    /// total amount in the currency of the transaction: for buys including
    /// fees and taxes, for sells / dividends after fees and taxes
    pub amount: i64,
    #[serde(default)]
    pub fees: i64,
    #[serde(default)]
    pub taxes: i64,
    /// exchange rate from the security (or target) currency into the
    /// transaction currency (e.g. USD→EUR 0.92); required if currencies differ
    #[cfg_attr(feature = "openapi", schema(value_type = Option<String>))]
    pub exchange_rate: Option<Decimal>,
    /// amount credited on the target account of a cash transfer in a
    /// different currency
    pub target_amount: Option<i64>,
    pub note: Option<String>,
    pub source: Option<String>,
}

/// A transaction ready to be stored, with its owner.
#[derive(Debug, Clone)]
pub enum StoredTx {
    Account { owner: Id, tx: AccountTransaction },
    Portfolio { owner: Id, tx: PortfolioTransaction },
}

impl StoredTx {
    pub fn id(&self) -> &str {
        match self {
            StoredTx::Account { tx, .. } => &tx.id,
            StoredTx::Portfolio { tx, .. } => &tx.id,
        }
    }
}

fn err<T>(m: impl Into<String>) -> Result<T, String> {
    Err(m.into())
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

fn convert(amount: i64, rate: Decimal) -> i64 {
    crate::money::convert_amount(amount, rate)
}

impl Booking {
    fn unit(&self, kind: UnitType, currency: &str, v: i64) -> Option<TransactionUnit> {
        (v != 0).then(|| TransactionUnit {
            kind,
            amount: Money::new(currency, v),
            forex: None,
            exchange_rate: None,
        })
    }

    fn fee_tax_units(&self, currency: &str) -> Vec<TransactionUnit> {
        [
            self.unit(UnitType::Fee, currency, self.fees),
            self.unit(UnitType::Tax, currency, self.taxes),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// GROSS_VALUE unit if the security currency differs from `currency`.
    fn forex_unit(
        &self,
        gross: i64,
        currency: &str,
        security_currency: &str,
    ) -> Result<Option<TransactionUnit>, String> {
        if security_currency == currency {
            return Ok(None);
        }
        let Some(rate) = self.exchange_rate.filter(|r| !r.is_zero()) else {
            return err(format!(
                "Wechselkurs {security_currency}/{currency} erforderlich"
            ));
        };
        let forex = convert(gross, crate::money::inverse_rate(rate));
        Ok(Some(TransactionUnit {
            kind: UnitType::GrossValue,
            amount: Money::new(currency, gross),
            forex: Some(Money::new(security_currency, forex)),
            exchange_rate: Some(rate),
        }))
    }

    fn common(
        &self,
        id: String,
        currency: &str,
        amount: i64,
        units: Vec<TransactionUnit>,
        now: &Option<String>,
    ) -> TransactionCommon {
        TransactionCommon {
            id,
            date_time: self.date_time,
            currency: currency.into(),
            amount,
            shares: self.shares,
            security: self.security_id.clone(),
            note: self.note.clone().filter(|n| !n.trim().is_empty()),
            source: self.source.clone(),
            units,
            cross_entry: None,
            updated_at: now.clone(),
        }
    }

    /// Builds the transaction(s). `primary_id` keeps the id when editing.
    pub fn build(
        &self,
        client: &Client,
        primary_id: Option<String>,
        now: Option<String>,
    ) -> Result<Vec<StoredTx>, String> {
        if self.amount < 0 || self.fees < 0 || self.taxes < 0 || self.shares < 0 {
            return err("Beträge dürfen nicht negativ sein");
        }
        let id = primary_id.unwrap_or_else(new_id);
        let account = |aid: &Option<String>| -> Result<&Account, String> {
            let aid = aid.as_deref().ok_or("Konto fehlt")?;
            client
                .account(aid)
                .ok_or_else(|| "Konto unbekannt".to_string())
        };
        let portfolio = |pid: &Option<String>| -> Result<&Portfolio, String> {
            let pid = pid.as_deref().ok_or("Depot fehlt")?;
            client
                .portfolio(pid)
                .ok_or_else(|| "Depot unbekannt".to_string())
        };
        let security = || -> Result<&Security, String> {
            let sid = self.security_id.as_deref().ok_or("Wertpapier fehlt")?;
            client
                .security(sid)
                .ok_or_else(|| "Wertpapier unbekannt".to_string())
        };
        use BookingKind as K;
        match self.kind {
            K::Buy | K::Sell => {
                let a = account(&self.account_id)?;
                let p = portfolio(&self.portfolio_id)?;
                let s = security()?;
                if self.shares <= 0 {
                    return err("Stückzahl erforderlich");
                }
                let buy = self.kind == K::Buy;
                let gross = if buy {
                    self.amount - self.fees - self.taxes
                } else {
                    self.amount + self.fees + self.taxes
                };
                if gross <= 0 {
                    return err("Gebühren und Steuern übersteigen den Betrag");
                }
                let mut units = self.fee_tax_units(&a.currency);
                if let Some(u) = self.forex_unit(gross, &a.currency, s.currency_or(&a.currency))? {
                    units.push(u);
                }
                let other = new_id_for(&id);
                let mut pc = self.common(id.clone(), &a.currency, self.amount, units, &now);
                pc.cross_entry = Some(CrossEntry {
                    kind: CrossEntryKind::BuySell,
                    other_owner: a.id.clone(),
                    other_transaction: other.clone(),
                });
                let mut ac = self.common(other, &a.currency, self.amount, vec![], &now);
                ac.shares = 0;
                ac.cross_entry = Some(CrossEntry {
                    kind: CrossEntryKind::BuySell,
                    other_owner: p.id.clone(),
                    other_transaction: id,
                });
                Ok(vec![
                    StoredTx::Portfolio {
                        owner: p.id.clone(),
                        tx: PortfolioTransaction {
                            kind: if buy {
                                PortfolioTransactionType::Buy
                            } else {
                                PortfolioTransactionType::Sell
                            },
                            common: pc,
                        },
                    },
                    StoredTx::Account {
                        owner: a.id.clone(),
                        tx: AccountTransaction {
                            kind: if buy {
                                AccountTransactionType::Buy
                            } else {
                                AccountTransactionType::Sell
                            },
                            common: ac,
                            ex_date: None,
                        },
                    },
                ])
            }
            K::DeliveryInbound | K::DeliveryOutbound => {
                let p = portfolio(&self.portfolio_id)?;
                let s = security()?;
                if self.shares <= 0 {
                    return err("Stückzahl erforderlich");
                }
                let currency = p
                    .reference_account
                    .as_deref()
                    .and_then(|r| client.account(r))
                    .map(|a| a.currency.clone())
                    .unwrap_or_else(|| s.currency_or(&client.base_currency).to_string());
                let inbound = self.kind == K::DeliveryInbound;
                let gross = if inbound {
                    self.amount - self.fees - self.taxes
                } else {
                    self.amount + self.fees + self.taxes
                };
                let mut units = self.fee_tax_units(&currency);
                if let Some(u) = self.forex_unit(gross, &currency, s.currency_or(&currency))? {
                    units.push(u);
                }
                Ok(vec![StoredTx::Portfolio {
                    owner: p.id.clone(),
                    tx: PortfolioTransaction {
                        kind: if inbound {
                            PortfolioTransactionType::DeliveryInbound
                        } else {
                            PortfolioTransactionType::DeliveryOutbound
                        },
                        common: self.common(id, &currency, self.amount, units, &now),
                    },
                }])
            }
            K::SecurityTransfer => {
                let src = portfolio(&self.portfolio_id)?;
                let dst = portfolio(&self.target_portfolio_id)?;
                let s = security()?;
                if src.id == dst.id {
                    return err("Quell- und Zieldepot sind identisch");
                }
                if self.shares <= 0 {
                    return err("Stückzahl erforderlich");
                }
                let currency = s.currency_or(&client.base_currency).to_string();
                let other = new_id_for(&id);
                let mut o = self.common(id.clone(), &currency, self.amount, vec![], &now);
                o.cross_entry = Some(CrossEntry {
                    kind: CrossEntryKind::PortfolioTransfer,
                    other_owner: dst.id.clone(),
                    other_transaction: other.clone(),
                });
                let mut i = self.common(other, &currency, self.amount, vec![], &now);
                i.cross_entry = Some(CrossEntry {
                    kind: CrossEntryKind::PortfolioTransfer,
                    other_owner: src.id.clone(),
                    other_transaction: id,
                });
                Ok(vec![
                    StoredTx::Portfolio {
                        owner: src.id.clone(),
                        tx: PortfolioTransaction {
                            kind: PortfolioTransactionType::TransferOut,
                            common: o,
                        },
                    },
                    StoredTx::Portfolio {
                        owner: dst.id.clone(),
                        tx: PortfolioTransaction {
                            kind: PortfolioTransactionType::TransferIn,
                            common: i,
                        },
                    },
                ])
            }
            K::CashTransfer => {
                let src = account(&self.account_id)?;
                let dst = account(&self.target_account_id)?;
                if src.id == dst.id {
                    return err("Quell- und Zielkonto sind identisch");
                }
                if self.amount <= 0 {
                    return err("Betrag muss positiv sein");
                }
                let mut units = vec![];
                let target_amount = if src.currency != dst.currency {
                    let t = match (self.target_amount, self.exchange_rate) {
                        (Some(t), _) => t,
                        (None, Some(r)) if !r.is_zero() => {
                            convert(self.amount, crate::money::inverse_rate(r))
                        }
                        _ => {
                            return err(format!(
                                "Zielbetrag in {} oder Wechselkurs erforderlich",
                                dst.currency
                            ));
                        }
                    };
                    let rate = self.exchange_rate.unwrap_or_else(|| {
                        (Decimal::from(self.amount) / Decimal::from(t.max(1))).round_dp(10)
                    });
                    units.push(TransactionUnit {
                        kind: UnitType::GrossValue,
                        amount: Money::new(&src.currency, self.amount),
                        forex: Some(Money::new(&dst.currency, t)),
                        exchange_rate: Some(rate),
                    });
                    t
                } else {
                    self.amount
                };
                let other = new_id_for(&id);
                let mut o = self.common(id.clone(), &src.currency, self.amount, units, &now);
                o.shares = 0;
                o.security = None;
                o.cross_entry = Some(CrossEntry {
                    kind: CrossEntryKind::AccountTransfer,
                    other_owner: dst.id.clone(),
                    other_transaction: other.clone(),
                });
                let mut i = self.common(other, &dst.currency, target_amount, vec![], &now);
                i.shares = 0;
                i.security = None;
                i.cross_entry = Some(CrossEntry {
                    kind: CrossEntryKind::AccountTransfer,
                    other_owner: src.id.clone(),
                    other_transaction: id,
                });
                Ok(vec![
                    StoredTx::Account {
                        owner: src.id.clone(),
                        tx: AccountTransaction {
                            kind: AccountTransactionType::TransferOut,
                            common: o,
                            ex_date: None,
                        },
                    },
                    StoredTx::Account {
                        owner: dst.id.clone(),
                        tx: AccountTransaction {
                            kind: AccountTransactionType::TransferIn,
                            common: i,
                            ex_date: None,
                        },
                    },
                ])
            }
            k => {
                let kind = k.account_type().unwrap();
                let a = account(&self.account_id)?;
                if self.amount <= 0 && self.taxes == 0 && self.fees == 0 {
                    return err("Betrag muss positiv sein");
                }
                let earning = matches!(
                    kind,
                    AccountTransactionType::Dividends | AccountTransactionType::Interest
                );
                if kind == AccountTransactionType::Dividends && self.security_id.is_none() {
                    return err("Dividende benötigt ein Wertpapier");
                }
                if let Some(sid) = &self.security_id
                    && client.security(sid).is_none()
                {
                    return err("Wertpapier unbekannt");
                }
                let mut units = if earning {
                    self.fee_tax_units(&a.currency)
                } else {
                    vec![]
                };
                if kind == AccountTransactionType::Dividends {
                    let s = security()?;
                    let gross = self.amount + self.fees + self.taxes;
                    if let Some(u) = self
                        .forex_unit(gross, &a.currency, s.currency_or(&a.currency))
                        .ok()
                        .flatten()
                    {
                        units.push(u);
                    }
                }
                let mut c = self.common(id, &a.currency, self.amount, units, &now);
                if !matches!(kind, AccountTransactionType::Dividends) {
                    c.shares = 0;
                }
                if !matches!(
                    kind,
                    AccountTransactionType::Dividends
                        | AccountTransactionType::Fees
                        | AccountTransactionType::FeesRefund
                        | AccountTransactionType::Taxes
                        | AccountTransactionType::TaxRefund
                ) {
                    c.security = None;
                }
                Ok(vec![StoredTx::Account {
                    owner: a.id.clone(),
                    tx: AccountTransaction {
                        kind,
                        common: c,
                        ex_date: None,
                    },
                }])
            }
        }
    }
}

/// Deterministic id for the counterpart, so editing keeps both ids stable.
fn new_id_for(primary: &str) -> String {
    let ns = uuid::Uuid::from_bytes([
        0x5c, 0x68, 0x6d, 0x6f, 0x72, 0x74, 0x66, 0x6f, 0x6c, 0x69, 0x6f, 0x2d, 0x70, 0x61, 0x69,
        0x72,
    ]);
    uuid::Uuid::new_v5(&ns, primary.as_bytes()).to_string()
}

/// Finds a transaction and its owner by id.
pub fn find<'a>(
    client: &'a Client,
    id: &str,
) -> Option<(&'a str, bool, &'a TransactionCommon, String)> {
    for a in &client.accounts {
        if let Some(t) = a.transactions.iter().find(|t| t.id == id) {
            return Some((a.id.as_str(), true, &t.common, t.kind.as_str().to_string()));
        }
    }
    for p in &client.portfolios {
        if let Some(t) = p.transactions.iter().find(|t| t.id == id) {
            return Some((p.id.as_str(), false, &t.common, t.kind.as_str().to_string()));
        }
    }
    None
}

/// Rebuilds the booking of an existing transaction (for editing). Returns
/// the booking and the id of the primary transaction of the pair.
pub fn reconstruct(client: &Client, id: &str) -> Option<(Booking, String)> {
    let (owner, is_account, t, kind) = find(client, id)?;
    // for pairs, use the portfolio side (buy/sell) or the outgoing side
    // (transfers) as primary
    if let Some(ce) = &t.cross_entry {
        let primary_is_other = match (ce.kind, kind.as_str()) {
            (CrossEntryKind::BuySell, _) => is_account,
            (_, "TRANSFER_IN") => true,
            _ => false,
        };
        if primary_is_other {
            return reconstruct(client, &ce.other_transaction.clone());
        }
    }
    let fees = t.unit_sum(UnitType::Fee);
    let taxes = t.unit_sum(UnitType::Tax);
    let rate = t.unit(UnitType::GrossValue).and_then(|u| u.exchange_rate);
    let mut b = Booking {
        kind: BookingKind::Deposit,
        date_time: t.date_time,
        account_id: None,
        portfolio_id: None,
        target_account_id: None,
        target_portfolio_id: None,
        security_id: t.security.clone(),
        shares: t.shares,
        amount: t.amount,
        fees,
        taxes,
        exchange_rate: rate,
        target_amount: None,
        note: t.note.clone(),
        source: t.source.clone(),
    };
    let other_owner = t.cross_entry.as_ref().map(|c| c.other_owner.clone());
    b.kind = match kind.as_str() {
        "BUY" | "SELL" => {
            b.portfolio_id = Some(owner.into());
            b.account_id = other_owner;
            if kind == "BUY" {
                BookingKind::Buy
            } else {
                BookingKind::Sell
            }
        }
        "DELIVERY_INBOUND" => {
            b.portfolio_id = Some(owner.into());
            BookingKind::DeliveryInbound
        }
        "DELIVERY_OUTBOUND" => {
            b.portfolio_id = Some(owner.into());
            BookingKind::DeliveryOutbound
        }
        "TRANSFER_OUT" if !is_account => {
            b.portfolio_id = Some(owner.into());
            b.target_portfolio_id = other_owner;
            BookingKind::SecurityTransfer
        }
        "TRANSFER_OUT" => {
            b.account_id = Some(owner.into());
            b.target_account_id = other_owner;
            b.target_amount = t
                .unit(UnitType::GrossValue)
                .and_then(|u| u.forex.as_ref())
                .map(|f| f.amount);
            b.fees = 0;
            b.taxes = 0;
            BookingKind::CashTransfer
        }
        k => {
            b.account_id = Some(owner.into());
            match AccountTransactionType::parse(k)? {
                AccountTransactionType::Deposit => BookingKind::Deposit,
                AccountTransactionType::Removal => BookingKind::Removal,
                AccountTransactionType::Dividends => BookingKind::Dividends,
                AccountTransactionType::Interest => BookingKind::Interest,
                AccountTransactionType::InterestCharge => BookingKind::InterestCharge,
                AccountTransactionType::Fees => BookingKind::Fees,
                AccountTransactionType::FeesRefund => BookingKind::FeesRefund,
                AccountTransactionType::Taxes => BookingKind::Taxes,
                AccountTransactionType::TaxRefund => BookingKind::TaxRefund,
                _ => return None,
            }
        }
    };
    Some((b, t.id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::str::FromStr;

    fn client() -> Client {
        let mut c = Client {
            base_currency: "EUR".into(),
            ..Default::default()
        };
        c.accounts.push(Account {
            id: "eur".into(),
            name: "EUR".into(),
            currency: "EUR".into(),
            ..Default::default()
        });
        c.accounts.push(Account {
            id: "usd".into(),
            name: "USD".into(),
            currency: "USD".into(),
            ..Default::default()
        });
        c.portfolios.push(Portfolio {
            id: "p1".into(),
            name: "D1".into(),
            reference_account: Some("eur".into()),
            ..Default::default()
        });
        c.portfolios.push(Portfolio {
            id: "p2".into(),
            name: "D2".into(),
            reference_account: Some("eur".into()),
            ..Default::default()
        });
        c.securities.push(Security {
            id: "apple".into(),
            name: "Apple".into(),
            currency: Some("USD".into()),
            ..Default::default()
        });
        c
    }

    fn base(kind: BookingKind) -> Booking {
        Booking {
            kind,
            date_time: NaiveDate::from_ymd_opt(2024, 3, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap(),
            account_id: Some("eur".into()),
            portfolio_id: Some("p1".into()),
            target_account_id: None,
            target_portfolio_id: None,
            security_id: Some("apple".into()),
            shares: 10 * 100_000_000,
            amount: 1000_00,
            fees: 5_00,
            taxes: 0,
            exchange_rate: Some(Decimal::from_str("0.9").unwrap()),
            target_amount: None,
            note: None,
            source: None,
        }
    }

    fn apply(c: &mut Client, txs: Vec<StoredTx>) {
        for t in txs {
            match t {
                StoredTx::Account { owner, tx } => c
                    .accounts
                    .iter_mut()
                    .find(|a| a.id == owner)
                    .unwrap()
                    .transactions
                    .push(tx),
                StoredTx::Portfolio { owner, tx } => c
                    .portfolios
                    .iter_mut()
                    .find(|a| a.id == owner)
                    .unwrap()
                    .transactions
                    .push(tx),
            }
        }
    }

    #[test]
    fn forex_buy_has_gross_value_unit() {
        let c = client();
        let txs = base(BookingKind::Buy).build(&c, None, None).unwrap();
        let StoredTx::Portfolio { tx, .. } = &txs[0] else {
            panic!()
        };
        let u = tx.unit(UnitType::GrossValue).unwrap();
        assert_eq!(u.amount.amount, 995_00);
        assert_eq!(u.forex.as_ref().unwrap().amount, 1105_56); // 995 / 0.9
        assert_eq!(tx.cross_entry.as_ref().unwrap().other_owner, "eur");
        // missing exchange rate is rejected
        let mut b = base(BookingKind::Buy);
        b.exchange_rate = None;
        assert!(b.build(&c, None, None).is_err());
    }

    #[test]
    fn roundtrip_reconstruct() {
        for kind in [
            BookingKind::Buy,
            BookingKind::Sell,
            BookingKind::SecurityTransfer,
            BookingKind::CashTransfer,
            BookingKind::Dividends,
            BookingKind::Deposit,
            BookingKind::DeliveryInbound,
        ] {
            let mut c = client();
            let mut b = base(kind);
            match kind {
                BookingKind::SecurityTransfer => {
                    b.target_portfolio_id = Some("p2".into());
                    b.fees = 0;
                    b.exchange_rate = None;
                }
                BookingKind::CashTransfer => {
                    b.target_account_id = Some("usd".into());
                    b.target_amount = Some(1100_00);
                    b.fees = 0;
                    b.security_id = None;
                    b.shares = 0;
                }
                BookingKind::Deposit => {
                    b.fees = 0;
                    b.security_id = None;
                    b.shares = 0;
                    b.exchange_rate = None;
                }
                _ => {}
            }
            let txs = b.build(&c, None, None).unwrap();
            let ids: Vec<String> = txs.iter().map(|t| t.id().to_string()).collect();
            apply(&mut c, txs);
            for id in &ids {
                let (r, primary) = reconstruct(&c, id).unwrap();
                assert_eq!(primary, ids[0], "{kind:?}");
                assert_eq!(r.kind, b.kind);
                assert_eq!(r.amount, b.amount, "{kind:?}");
                assert_eq!(
                    r.account_id.is_some(),
                    b.account_id.is_some()
                        && kind != BookingKind::SecurityTransfer
                        && kind != BookingKind::DeliveryInbound
                        || r.account_id.is_some()
                );
                // rebuilding gives identical ids
                let again = r.build(&c, Some(primary), None).unwrap();
                assert_eq!(
                    again.iter().map(|t| t.id().to_string()).collect::<Vec<_>>(),
                    ids
                );
            }
        }
    }
}
