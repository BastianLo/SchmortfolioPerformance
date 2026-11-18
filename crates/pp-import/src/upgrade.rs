//! Data migrations for old PP file versions (subset of PP's
//! `ClientFactory#upgradeModel` that affects values).

use pp_core::model::Client;

fn scale_shares(client: &mut Client, factor: i64) {
    for p in &mut client.portfolios {
        for t in &mut p.transactions {
            t.common.shares *= factor;
        }
    }
    for a in &mut client.accounts {
        for t in &mut a.transactions {
            t.common.shares *= factor;
        }
    }
}

fn scale_quotes(client: &mut Client, factor: i64) {
    for s in &mut client.securities {
        for p in &mut s.prices {
            p.value *= factor;
        }
        if let Some(l) = &mut s.latest {
            l.value *= factor;
            if l.high != -1 {
                l.high *= factor;
            }
            if l.low != -1 {
                l.low *= factor;
            }
        }
    }
}

pub(crate) fn upgrade(client: &mut Client, warnings: &mut Vec<String>) {
    let v = client.version;
    if v <= 0 {
        warnings.push("Dateiversion unbekannt, keine Migration durchgeführt".into());
        return;
    }
    if v < 3 {
        // version 2: decimal places for shares
        for p in &mut client.portfolios {
            for t in &mut p.transactions {
                t.common.shares *= 100_000;
            }
        }
    }
    if v <= 25 {
        scale_shares(client, 10);
    }
    if v <= 28 {
        // currency support introduced: everything was EUR (fees/taxes were
        // already converted into units while reading)
        client.base_currency = "EUR".into();
    }
    if v <= 29 {
        scale_quotes(client, 100);
    }
    if v <= 48 {
        scale_shares(client, 100);
        scale_quotes(client, 10_000);
    }
    if v < 60 {
        // plan types introduced: derived from the portfolio / sign of the amount
        use pp_core::plan::PlanType;
        for p in &mut client.plans {
            if p.portfolio.is_some() {
                p.kind = PlanType::PurchaseOrDelivery;
            } else {
                p.kind = if p.amount >= 0 {
                    PlanType::Deposit
                } else {
                    PlanType::Removal
                };
                p.amount = p.amount.abs();
            }
        }
    }
    if v < crate::MAX_SUPPORTED_VERSION {
        client.version = crate::MAX_SUPPORTED_VERSION;
    }
}
