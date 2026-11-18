//! Deterministic demo data: five years of a realistic savings plan portfolio
//! with fictitious securities.

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use pp_core::Money;
use pp_core::model::*;
use pp_core::money::{QUOTE_FACTOR, market_value};
use rust_decimal::Decimal;

struct Rng(u64);

impl Rng {
    fn next_f64(&mut self) -> f64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64 / (1u64 << 53) as f64
    }

    fn normal(&mut self) -> f64 {
        let u1 = self.next_f64().max(1e-12);
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

struct Spec {
    name: &'static str,
    isin: &'static str,
    ticker: &'static str,
    currency: &'static str,
    start: f64,
    drift: f64,
    vol: f64,
    /// dividend yield p.a. and payments per year
    dividend: (f64, u32),
    asset_class: &'static str,
    region: &'static str,
}

const SPECS: &[Spec] = &[
    Spec {
        name: "Welt Aktien ETF (thesaurierend)",
        isin: "ZZDEMO000011",
        ticker: "WLD.DEMO",
        currency: "EUR",
        start: 62.0,
        drift: 0.075,
        vol: 0.15,
        dividend: (0.0, 0),
        asset_class: "equity",
        region: "world",
    },
    Spec {
        name: "Europa Dividenden ETF",
        isin: "ZZDEMO000029",
        ticker: "EUDIV.DEMO",
        currency: "EUR",
        start: 24.5,
        drift: 0.045,
        vol: 0.13,
        dividend: (0.038, 4),
        asset_class: "equity",
        region: "europe",
    },
    Spec {
        name: "Schwellenländer ETF",
        isin: "ZZDEMO000037",
        ticker: "EM.DEMO",
        currency: "EUR",
        start: 31.0,
        drift: 0.04,
        vol: 0.19,
        dividend: (0.0, 0),
        asset_class: "equity",
        region: "em",
    },
    Spec {
        name: "Nordwind Technologie AG",
        isin: "ZZDEMO000045",
        ticker: "NWT.DEMO",
        currency: "EUR",
        start: 84.0,
        drift: 0.13,
        vol: 0.32,
        dividend: (0.012, 1),
        asset_class: "equity",
        region: "europe",
    },
    Spec {
        name: "Atlantic Consumer Inc.",
        isin: "ZZDEMO000052",
        ticker: "ATLC",
        currency: "USD",
        start: 138.0,
        drift: 0.08,
        vol: 0.21,
        dividend: (0.024, 4),
        asset_class: "equity",
        region: "na",
    },
    Spec {
        name: "Euro Staatsanleihen ETF",
        isin: "ZZDEMO000060",
        ticker: "EUGOV.DEMO",
        currency: "EUR",
        start: 118.0,
        drift: 0.01,
        vol: 0.05,
        dividend: (0.021, 1),
        asset_class: "bonds",
        region: "europe",
    },
];

const TAX_RATE: f64 = 0.26375;

fn business_day(d: NaiveDate) -> NaiveDate {
    let mut d = d;
    while matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
        d += Duration::days(1);
    }
    d
}

fn add_months(d: NaiveDate, m: u32) -> NaiveDate {
    let total = d.month0() + m;
    NaiveDate::from_ymd_opt(
        d.year() + (total / 12) as i32,
        total % 12 + 1,
        d.day().min(28),
    )
    .unwrap()
}

struct Builder {
    client: Client,
    n: u64,
}

impl Builder {
    fn id(&mut self, prefix: &str) -> String {
        self.n += 1;
        format!("demo-{prefix}-{:06}", self.n)
    }

    fn common(&mut self, date: NaiveDate, currency: &str, amount: i64) -> TransactionCommon {
        TransactionCommon {
            id: self.id("tx"),
            date_time: date.and_hms_opt(0, 0, 0).unwrap(),
            currency: currency.into(),
            amount,
            shares: 0,
            security: None,
            note: None,
            source: None,
            units: vec![],
            cross_entry: None,
            updated_at: None,
        }
    }

    fn account_tx(
        &mut self,
        account: usize,
        kind: AccountTransactionType,
        date: NaiveDate,
        amount: i64,
        security: Option<&str>,
        units: Vec<TransactionUnit>,
        note: Option<&str>,
    ) {
        let mut c = self.common(date, "EUR", amount);
        c.security = security.map(String::from);
        c.units = units;
        c.note = note.map(String::from);
        self.client.accounts[account]
            .transactions
            .push(AccountTransaction {
                kind,
                common: c,
                ex_date: None,
            });
    }

    fn transfer(&mut self, from: usize, to: usize, date: NaiveDate, amount: i64) {
        let out_id = self.id("tx");
        let in_id = self.id("tx");
        let from_id = self.client.accounts[from].id.clone();
        let to_id = self.client.accounts[to].id.clone();
        let mut o = self.common(date, "EUR", amount);
        o.id = out_id.clone();
        o.cross_entry = Some(CrossEntry {
            kind: CrossEntryKind::AccountTransfer,
            other_owner: to_id,
            other_transaction: in_id.clone(),
        });
        let mut i = self.common(date, "EUR", amount);
        i.id = in_id;
        i.cross_entry = Some(CrossEntry {
            kind: CrossEntryKind::AccountTransfer,
            other_owner: from_id,
            other_transaction: out_id,
        });
        self.client.accounts[from]
            .transactions
            .push(AccountTransaction {
                kind: AccountTransactionType::TransferOut,
                common: o,
                ex_date: None,
            });
        self.client.accounts[to]
            .transactions
            .push(AccountTransaction {
                kind: AccountTransactionType::TransferIn,
                common: i,
                ex_date: None,
            });
    }

    /// Buys or sells for a gross value in EUR; returns the shares.
    #[allow(clippy::too_many_arguments)]
    fn trade(
        &mut self,
        buy: bool,
        sec: usize,
        date: NaiveDate,
        gross_eur: i64,
        quote: i64,
        eurusd: f64,
        fee: i64,
        tax: i64,
        shares: Option<i64>,
    ) -> i64 {
        let s = &self.client.securities[sec];
        let usd = s.currency.as_deref() == Some("USD");
        let sec_id = s.id.clone();
        // quote is in security currency; convert gross to security currency
        let gross_sec = if usd {
            (gross_eur as f64 * eurusd).round() as i64
        } else {
            gross_eur
        };
        let shares = shares.unwrap_or_else(|| {
            let raw = gross_sec as i128 * 100_000_000_000_000 / quote as i128;
            (raw / 100 * 100) as i64 // six decimals
        });
        let gross_sec = market_value(shares, quote);
        let gross_eur = if usd {
            (gross_sec as f64 / eurusd).round() as i64
        } else {
            gross_sec
        };
        let amount = if buy {
            gross_eur + fee + tax
        } else {
            gross_eur - fee - tax
        };

        let mut units = vec![];
        if fee != 0 {
            units.push(TransactionUnit {
                kind: UnitType::Fee,
                amount: Money::new("EUR", fee),
                forex: None,
                exchange_rate: None,
            });
        }
        if tax != 0 {
            units.push(TransactionUnit {
                kind: UnitType::Tax,
                amount: Money::new("EUR", tax),
                forex: None,
                exchange_rate: None,
            });
        }
        if usd {
            let rate = Decimal::from_f64_retain(1.0 / eurusd)
                .unwrap_or(Decimal::ONE)
                .round_dp(6);
            units.push(TransactionUnit {
                kind: UnitType::GrossValue,
                amount: Money::new("EUR", gross_eur),
                forex: Some(Money::new("USD", gross_sec)),
                exchange_rate: Some(rate),
            });
        }
        let ptx_id = self.id("tx");
        let atx_id = self.id("tx");
        let portfolio_id = self.client.portfolios[0].id.clone();
        let account_id = self.client.accounts[0].id.clone();
        let mut pc = self.common(date, "EUR", amount);
        pc.id = ptx_id.clone();
        pc.shares = shares;
        pc.security = Some(sec_id.clone());
        pc.units = units;
        pc.cross_entry = Some(CrossEntry {
            kind: CrossEntryKind::BuySell,
            other_owner: account_id,
            other_transaction: atx_id.clone(),
        });
        let mut ac = self.common(date, "EUR", amount);
        ac.id = atx_id;
        ac.security = Some(sec_id);
        ac.cross_entry = Some(CrossEntry {
            kind: CrossEntryKind::BuySell,
            other_owner: portfolio_id,
            other_transaction: ptx_id,
        });
        self.client.portfolios[0]
            .transactions
            .push(PortfolioTransaction {
                kind: if buy {
                    PortfolioTransactionType::Buy
                } else {
                    PortfolioTransactionType::Sell
                },
                common: pc,
            });
        self.client.accounts[0]
            .transactions
            .push(AccountTransaction {
                kind: if buy {
                    AccountTransactionType::Buy
                } else {
                    AccountTransactionType::Sell
                },
                common: ac,
                ex_date: None,
            });
        shares
    }
}

pub fn generate(today: NaiveDate) -> Client {
    let mut rng = Rng(0x5EED_1234_ABCD_9876);
    let start = NaiveDate::from_ymd_opt(today.year() - 5, today.month(), 1).unwrap();

    // ---- price series (business days) and EUR/USD
    let mut days = Vec::new();
    let mut d = start - Duration::days(10);
    while d <= today {
        if !matches!(d.weekday(), Weekday::Sat | Weekday::Sun) {
            days.push(d);
        }
        d += Duration::days(1);
    }
    let dt: f64 = 1.0 / 252.0;
    let market: Vec<f64> = days.iter().map(|_| rng.normal()).collect();
    let mut eurusd = Vec::with_capacity(days.len());
    let mut fx = 1.12f64;
    for _ in &days {
        fx *= (0.07 * dt.sqrt() * rng.normal()).exp();
        fx = fx.clamp(0.95, 1.30);
        eurusd.push(fx);
    }

    let mut b = Builder {
        client: Client {
            version: pp_import::MAX_SUPPORTED_VERSION,
            base_currency: "EUR".into(),
            ..Default::default()
        },
        n: 0,
    };
    for (i, spec) in SPECS.iter().enumerate() {
        let mut price = spec.start;
        let beta = if spec.asset_class == "bonds" {
            0.1
        } else {
            0.75
        };
        let mut prices = Vec::with_capacity(days.len());
        for (k, day) in days.iter().enumerate() {
            let shock = beta * market[k] + (1.0 - beta * beta).sqrt() * rng.normal();
            price *= ((spec.drift - 0.5 * spec.vol * spec.vol) * dt + spec.vol * dt.sqrt() * shock)
                .exp();
            prices.push(SecurityPrice {
                date: *day,
                value: (price * 10_000.0).round() as i64 * (QUOTE_FACTOR / 10_000),
            });
        }
        b.client.securities.push(Security {
            id: format!("demo-sec-{i}"),
            name: spec.name.into(),
            currency: Some(spec.currency.into()),
            isin: Some(spec.isin.into()),
            ticker: Some(spec.ticker.into()),
            feed: Some("MANUAL".into()),
            note: Some("Fiktives Wertpapier (Demo-Daten)".into()),
            prices,
            ..Default::default()
        });
    }
    let price_at =
        |b: &Builder, sec: usize, d: NaiveDate| b.client.securities[sec].price_at(d).value;
    let fx_at = |d: NaiveDate| -> f64 {
        match days.binary_search(&d) {
            Ok(i) => eurusd[i],
            Err(0) => eurusd[0],
            Err(i) => eurusd[i - 1],
        }
    };

    b.client.accounts.push(Account {
        id: "demo-acc-main".into(),
        name: "Verrechnungskonto".into(),
        currency: "EUR".into(),
        ..Default::default()
    });
    b.client.accounts.push(Account {
        id: "demo-acc-savings".into(),
        name: "Tagesgeld".into(),
        currency: "EUR".into(),
        ..Default::default()
    });
    b.client.portfolios.push(Portfolio {
        id: "demo-pf".into(),
        name: "Depot".into(),
        reference_account: Some("demo-acc-main".into()),
        ..Default::default()
    });

    // ---- initial investment
    let d0 = business_day(start);
    b.account_tx(
        0,
        AccountTransactionType::Deposit,
        d0,
        25_000_00,
        None,
        vec![],
        Some("Startkapital"),
    );
    b.transfer(0, 1, d0, 5_000_00);
    for (sec, eur) in [
        (0usize, 6_000_00i64),
        (3, 3_000_00),
        (4, 3_500_00),
        (5, 3_000_00),
        (1, 2_000_00),
    ] {
        let q = price_at(&b, sec, d0);
        b.trade(true, sec, d0, eur, q, fx_at(d0), 9_90, 0, None);
    }

    // ---- monthly savings plan, deposits, interest
    let mut m = 1;
    loop {
        let month = add_months(start, m);
        if month > today {
            break;
        }
        let dep = business_day(month);
        if dep <= today {
            b.account_tx(
                0,
                AccountTransactionType::Deposit,
                dep,
                1_500_00,
                None,
                vec![],
                Some("Gehalt"),
            );
        }
        let plan = business_day(month + Duration::days(14));
        if plan <= today {
            for (sec, eur) in [(0usize, 500_00i64), (1, 200_00), (2, 150_00)] {
                let q = price_at(&b, sec, plan);
                b.trade(true, sec, plan, eur - 1_50, q, fx_at(plan), 1_50, 0, None);
            }
            if m % 3 == 0 {
                b.transfer(0, 1, plan, 300_00);
            }
        }
        // interest on savings account, end of month
        let eom = business_day(add_months(start, m + 1) - Duration::days(3));
        if eom <= today {
            let bal: i64 = b.client.accounts[1]
                .transactions
                .iter()
                .filter(|t| t.date() <= eom)
                .map(|t| {
                    if t.kind.is_credit() {
                        t.amount
                    } else {
                        -t.amount
                    }
                })
                .sum();
            let gross = (bal as f64 * 0.025 / 12.0).round() as i64;
            if gross > 0 {
                let tax = (gross as f64 * TAX_RATE).round() as i64;
                b.account_tx(
                    1,
                    AccountTransactionType::Interest,
                    eom,
                    gross - tax,
                    None,
                    vec![TransactionUnit {
                        kind: UnitType::Tax,
                        amount: Money::new("EUR", tax),
                        forex: None,
                        exchange_rate: None,
                    }],
                    Some("Zinsgutschrift"),
                );
            }
        }
        // opportunistic buys of single stocks twice a year
        if m % 6 == 4 {
            let d = business_day(month + Duration::days(20));
            if d <= today {
                let sec = if m % 12 == 4 { 3 } else { 4 };
                let q = price_at(&b, sec, d);
                b.trade(true, sec, d, 1_500_00, q, fx_at(d), 9_90, 0, None);
            }
        }
        m += 1;
    }

    // ---- partial sale of the tech stock after three years
    let sale = business_day(add_months(start, 37) + Duration::days(9));
    if sale <= today {
        let held: i64 = b.client.portfolios[0]
            .transactions
            .iter()
            .filter(|t| t.security.as_deref() == Some("demo-sec-3") && t.date() <= sale)
            .map(|t| {
                if t.kind.is_purchase() {
                    t.shares
                } else {
                    -t.shares
                }
            })
            .sum();
        let shares = held / 2 / 100 * 100;
        let q = price_at(&b, 3, sale);
        let gross = market_value(shares, q);
        // rough gain for the tax estimate
        let tax = ((gross as f64 * 0.35).max(0.0) * TAX_RATE).round() as i64;
        b.trade(false, 3, sale, gross, q, 1.0, 9_90, tax, Some(shares));
    }

    // ---- dividends
    for (sec, spec) in SPECS.iter().enumerate() {
        let (yield_pa, per_year) = spec.dividend;
        if per_year == 0 {
            continue;
        }
        let step = 12 / per_year;
        let mut k = 1;
        loop {
            let pay = business_day(add_months(start, k * step) + Duration::days(9));
            if pay > today {
                break;
            }
            k += 1;
            let shares: i64 = b.client.portfolios[0]
                .transactions
                .iter()
                .filter(|t| {
                    t.security.as_deref() == Some(b.client.securities[sec].id.as_str())
                        && t.date() < pay
                })
                .map(|t| {
                    if t.kind.is_purchase() {
                        t.shares
                    } else {
                        -t.shares
                    }
                })
                .sum();
            if shares <= 0 {
                continue;
            }
            let q = price_at(&b, sec, pay);
            let gross_sec = market_value(shares, q) as f64 * yield_pa / per_year as f64;
            let usd = spec.currency == "USD";
            let rate = fx_at(pay);
            let gross_eur = if usd { gross_sec / rate } else { gross_sec };
            let gross_eur = gross_eur.round() as i64;
            let tax = (gross_eur as f64 * TAX_RATE).round() as i64;
            let mut units = vec![TransactionUnit {
                kind: UnitType::Tax,
                amount: Money::new("EUR", tax),
                forex: None,
                exchange_rate: None,
            }];
            if usd {
                units.push(TransactionUnit {
                    kind: UnitType::GrossValue,
                    amount: Money::new("EUR", gross_eur),
                    forex: Some(Money::new("USD", gross_sec.round() as i64)),
                    exchange_rate: Some(
                        Decimal::from_f64_retain(1.0 / rate)
                            .unwrap_or(Decimal::ONE)
                            .round_dp(6),
                    ),
                });
            }
            let sid = b.client.securities[sec].id.clone();
            let idx = b.client.accounts[0].transactions.len();
            b.account_tx(
                0,
                AccountTransactionType::Dividends,
                pay,
                gross_eur - tax,
                Some(&sid),
                units,
                Some("Ausschüttung"),
            );
            b.client.accounts[0].transactions[idx].common.shares = shares;
        }
    }

    // ---- yearly custody fee
    for y in (start.year() + 1)..=today.year() {
        let d = business_day(NaiveDate::from_ymd_opt(y, 1, 15).unwrap());
        if d <= today && d > start {
            b.account_tx(
                0,
                AccountTransactionType::Fees,
                d,
                24_00,
                None,
                vec![],
                Some("Depotgebühr"),
            );
        }
    }

    // ---- taxonomies
    let a = |v: &str, w: i32| Assignment {
        vehicle: v.into(),
        weight: w,
        rank: 0,
    };
    let class =
        |id: &str, parent: Option<&str>, name: &str, color: &str, assignments: Vec<Assignment>| {
            Classification {
                id: id.into(),
                parent_id: parent.map(String::from),
                name: name.into(),
                note: None,
                color: Some(color.into()),
                weight: 10000,
                rank: 0,
                assignments,
                data: serde_json::Value::Null,
            }
        };
    let sec_ids: Vec<String> = b.client.securities.iter().map(|s| s.id.clone()).collect();
    let by = |pred: &dyn Fn(&Spec) -> bool| -> Vec<Assignment> {
        SPECS
            .iter()
            .enumerate()
            .filter(|(_, s)| pred(s))
            .map(|(i, _)| a(&sec_ids[i], 10000))
            .collect()
    };
    b.client.taxonomies.push(Taxonomy {
        id: "demo-assetclasses".into(),
        name: "Anlageklassen".into(),
        source: None,
        dimensions: vec![],
        classifications: vec![
            class("ac-root", None, "Anlageklassen", "#64748b", vec![]),
            class(
                "ac-equity",
                Some("ac-root"),
                "Aktien",
                "#3b82f6",
                by(&|s| s.asset_class == "equity"),
            ),
            class(
                "ac-bonds",
                Some("ac-root"),
                "Anleihen",
                "#f59e0b",
                by(&|s| s.asset_class == "bonds"),
            ),
            class(
                "ac-cash",
                Some("ac-root"),
                "Barvermögen",
                "#10b981",
                vec![a("demo-acc-main", 10000), a("demo-acc-savings", 10000)],
            ),
        ],
    });
    b.client.taxonomies.push(Taxonomy {
        id: "demo-regions".into(),
        name: "Regionen".into(),
        source: None,
        dimensions: vec![],
        classifications: vec![
            class("rg-root", None, "Regionen", "#64748b", vec![]),
            class(
                "rg-world",
                Some("rg-root"),
                "Welt",
                "#6366f1",
                by(&|s| s.region == "world"),
            ),
            class(
                "rg-europe",
                Some("rg-root"),
                "Europa",
                "#0ea5e9",
                by(&|s| s.region == "europe"),
            ),
            class(
                "rg-na",
                Some("rg-root"),
                "Nordamerika",
                "#ef4444",
                by(&|s| s.region == "na"),
            ),
            class(
                "rg-em",
                Some("rg-root"),
                "Schwellenländer",
                "#22c55e",
                by(&|s| s.region == "em"),
            ),
        ],
    });
    b.client.watchlists.push(Watchlist {
        name: "Einzelwerte".into(),
        securities: vec![sec_ids[3].clone(), sec_ids[4].clone()],
    });
    b.client.normalize();
    b.client
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_is_consistent() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        let c = generate(today);
        assert_eq!(c.securities.len(), SPECS.len());
        let tx: usize = c.portfolios[0].transactions.len();
        assert!(tx > 150, "{tx}");
        // cash account never negative
        let mut bal = 0i64;
        let mut all: Vec<_> = c.accounts[0].transactions.iter().collect();
        all.sort_by_key(|t| t.date_time);
        for t in all {
            bal += if t.kind.is_credit() {
                t.amount
            } else {
                -t.amount
            };
        }
        assert!(bal > 0, "balance {bal}");
        // deterministic
        assert_eq!(
            generate(today).securities[0].prices.last(),
            c.securities[0].prices.last()
        );
    }
}
