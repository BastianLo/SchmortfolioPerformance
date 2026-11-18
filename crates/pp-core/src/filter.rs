//! Restricting a client to selected portfolios and accounts (PP: client
//! filters). Transactions crossing the boundary are converted so that the
//! filtered client is consistent: purchases paid from an excluded account
//! become deliveries, transfers to excluded owners become removals, etc.

use std::collections::HashSet;

use crate::model::*;

pub fn filter_client(client: &Client, portfolios: &[String], accounts: &[String]) -> Client {
    let pf: HashSet<&str> = portfolios.iter().map(String::as_str).collect();
    let acc: HashSet<&str> = accounts.iter().map(String::as_str).collect();
    let mut out = Client {
        version: client.version,
        base_currency: client.base_currency.clone(),
        securities: client.securities.clone(),
        accounts: vec![],
        portfolios: vec![],
        taxonomies: client.taxonomies.clone(),
        watchlists: client.watchlists.clone(),
        plans: vec![],
        extra: serde_json::Value::Null,
    };

    for p in client
        .portfolios
        .iter()
        .filter(|p| pf.contains(p.id.as_str()))
    {
        let mut np = Portfolio {
            transactions: vec![],
            ..p.clone()
        };
        for t in &p.transactions {
            let mut t = t.clone();
            let other_in = t.cross_entry.as_ref().is_some_and(|c| {
                pf.contains(c.other_owner.as_str()) || acc.contains(c.other_owner.as_str())
            });
            if !other_in {
                use PortfolioTransactionType as P;
                t.kind = match t.kind {
                    P::Buy if t.cross_entry.is_some() => P::DeliveryInbound,
                    P::Sell if t.cross_entry.is_some() => P::DeliveryOutbound,
                    P::TransferIn => P::DeliveryInbound,
                    P::TransferOut => P::DeliveryOutbound,
                    k => k,
                };
                t.common.cross_entry = None;
            }
            np.transactions.push(t);
        }
        out.portfolios.push(np);
    }

    for a in client
        .accounts
        .iter()
        .filter(|a| acc.contains(a.id.as_str()))
    {
        let mut na = Account {
            transactions: vec![],
            ..a.clone()
        };
        for t in &a.transactions {
            let mut t = t.clone();
            let other_in = t.cross_entry.as_ref().is_some_and(|c| {
                pf.contains(c.other_owner.as_str()) || acc.contains(c.other_owner.as_str())
            });
            if t.cross_entry.is_some() && !other_in {
                use AccountTransactionType as A;
                t.kind = match t.kind {
                    A::Buy | A::TransferOut => A::Removal,
                    A::Sell | A::TransferIn => A::Deposit,
                    k => k,
                };
                t.common.cross_entry = None;
                t.common.units.clear();
            }
            na.transactions.push(t);
        }
        out.accounts.push(na);
    }

    // dividends of held securities paid to excluded accounts: booked on a
    // pseudo account and removed immediately (as PP does)
    let held: HashSet<&str> = out
        .portfolios
        .iter()
        .flat_map(|p| p.transactions.iter().filter_map(|t| t.security.as_deref()))
        .collect();
    let mut pseudo = Account {
        id: "_filter".into(),
        name: "Erträge (Filter)".into(),
        currency: client.base_currency.clone(),
        ..Default::default()
    };
    for a in client
        .accounts
        .iter()
        .filter(|a| !acc.contains(a.id.as_str()))
    {
        for t in a
            .transactions
            .iter()
            .filter(|t| t.kind == AccountTransactionType::Dividends)
        {
            if !t.security.as_deref().is_some_and(|s| held.contains(s)) {
                continue;
            }
            pseudo.currency = a.currency.clone();
            pseudo.transactions.push(t.clone());
            let mut r = t.clone();
            r.kind = AccountTransactionType::Removal;
            r.common.id = format!("{}-removal", t.id);
            r.common.units.clear();
            r.common.security = None;
            r.common.shares = 0;
            pseudo.transactions.push(r);
        }
    }
    if !pseudo.transactions.is_empty() {
        out.accounts.push(pseudo);
    }
    out.normalize();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fx::{CurrencyConverter, ExchangeRateProvider};
    use crate::snapshot::ClientSnapshot;
    use chrono::NaiveDate;
    use std::sync::Arc;

    #[test]
    fn portfolio_only_filter_turns_buys_into_deliveries() {
        // build: deposit 1000, buy 500, dividend 10
        let mut c = Client {
            base_currency: "EUR".into(),
            ..Default::default()
        };
        c.securities.push(Security {
            id: "s".into(),
            name: "S".into(),
            currency: Some("EUR".into()),
            prices: vec![SecurityPrice {
                date: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                value: 60 * 100_000_000,
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
            id: "p".into(),
            name: "P".into(),
            reference_account: Some("a".into()),
            ..Default::default()
        });
        let date = NaiveDate::from_ymd_opt(2024, 1, 2)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let mk = |kind, amount, security: Option<&str>| crate::booking::Booking {
            kind,
            date_time: date,
            account_id: Some("a".into()),
            portfolio_id: Some("p".into()),
            target_account_id: None,
            target_portfolio_id: None,
            security_id: security.map(String::from),
            shares: if security.is_some() {
                10 * 100_000_000
            } else {
                0
            },
            amount,
            fees: 0,
            taxes: 0,
            exchange_rate: None,
            target_amount: None,
            note: None,
            source: None,
        };
        use crate::booking::{BookingKind as K, StoredTx};
        for b in [
            mk(K::Deposit, 1000_00, None),
            mk(K::Buy, 500_00, Some("s")),
            mk(K::Dividends, 10_00, Some("s")),
        ] {
            for t in b.build(&c, None, None).unwrap() {
                match t {
                    StoredTx::Account { tx, .. } => c.accounts[0].transactions.push(tx),
                    StoredTx::Portfolio { tx, .. } => c.portfolios[0].transactions.push(tx),
                }
            }
        }
        let f = filter_client(&c, &["p".into()], &[]);
        assert_eq!(
            f.portfolios[0].transactions[0].kind,
            PortfolioTransactionType::DeliveryInbound
        );
        // pseudo account: dividend + removal -> balance 0
        assert_eq!(f.accounts.len(), 1);
        let conv = CurrencyConverter::new(Arc::new(ExchangeRateProvider::default()), "EUR");
        let d = NaiveDate::from_ymd_opt(2024, 1, 3).unwrap();
        let s = ClientSnapshot::create(&f, &conv, d);
        assert_eq!(s.total(), 600_00);
        // full client is unchanged by an all-inclusive filter
        let all = filter_client(&c, &["p".into()], &["a".into()]);
        assert_eq!(
            ClientSnapshot::create(&all, &conv, d).total(),
            ClientSnapshot::create(&c, &conv, d).total()
        );
        assert_eq!(
            all.portfolios[0].transactions[0].kind,
            PortfolioTransactionType::Buy
        );
    }
}
