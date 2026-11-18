//! Golden tests: expected values are taken from Portfolio Performance's own
//! test suite (scenarios/*TestCase.java), using the same fixed exchange rates
//! as PP's `TestCurrencyConverter`.

use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;

use chrono::NaiveDate;
use pp_core::performance::client_performance;
use pp_core::security::SecurityPerformance;
use pp_core::snapshot::ClientSnapshot;
use pp_core::{CurrencyConverter, ExchangeRateProvider, ExchangeRateSeries};
use rust_decimal::Decimal;

fn d(s: &str) -> NaiveDate {
    NaiveDate::from_str(s).unwrap()
}

fn load(rel: &str) -> pp_core::Client {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/pp")
        .join(rel);
    pp_import::read_client(&std::fs::read(p).unwrap(), None)
        .unwrap()
        .client
}

fn test_converter() -> CurrencyConverter {
    let mut usd = ExchangeRateSeries::new("EUR", "USD");
    for (date, r) in [
        ("2014-12-31", "1.2141"),
        ("2015-01-02", "1.2043"),
        ("2015-01-05", "1.1915"),
        ("2015-01-06", "1.1914"),
        ("2015-01-07", "1.1831"),
        ("2015-01-08", "1.1768"),
        ("2015-01-09", "1.1813"),
        ("2015-01-12", "1.1804"),
        ("2015-01-13", "1.1782"),
        ("2015-01-14", "1.1775"),
        ("2015-01-15", "1.1708"),
        ("2015-01-16", "1.1588"),
    ] {
        usd.add(d(date), Decimal::from_str(r).unwrap());
    }
    let mut chf = ExchangeRateSeries::new("EUR", "CHF");
    for (date, r) in [
        ("2014-12-31", "1.2024"),
        ("2015-01-02", "1.2022"),
        ("2015-01-05", "1.2016"),
        ("2015-01-06", "1.2014"),
        ("2015-01-07", "1.2011"),
        ("2015-01-08", "1.2010"),
        ("2015-01-09", "1.2010"),
        ("2015-01-12", "1.2010"),
        ("2015-01-13", "1.2010"),
        ("2015-01-14", "1.2010"),
        ("2015-01-15", "1.0280"),
        ("2015-01-16", "1.0128"),
    ] {
        chf.add(d(date), Decimal::from_str(r).unwrap());
    }
    CurrencyConverter::new(Arc::new(ExchangeRateProvider::new(vec![usd, chf])), "EUR")
}

#[test]
fn client_performance_snapshot_test_case() {
    let client = load("scenarios/client_performance_snapshot.xml");
    let p = client_performance(&client, &test_converter(), d("2015-01-02"), d("2015-01-14"));

    assert_eq!(p.initial_value, 2128_58);
    assert_eq!(p.earnings, 28_54);
    assert_eq!(p.fees, 17_52);
    assert_eq!(p.taxes, 116_50);
    assert_eq!(p.transfers, 787_49);
    assert_eq!(p.final_value, 2854_32);
    // PP: capital gains 21.35 + realized 4.69 + currency gains 17.68
    assert!(
        (p.capital_gains - (21_35 + 4_69 + 17_68)).abs() <= 1,
        "capital gains {}",
        p.capital_gains
    );
    assert_eq!(p.realized_capital_gains, 4_69);
}

#[test]
fn currency_test_case_snapshot() {
    let client = load("scenarios/currency_sample.xml");
    let conv = test_converter();
    let snap = ClientSnapshot::create(&client, &conv, d("2015-01-16"));
    let usd = client
        .accounts
        .iter()
        .find(|a| a.name == "Account USD")
        .unwrap();
    let eur = client
        .accounts
        .iter()
        .find(|a| a.name == "Account EUR")
        .unwrap();
    let acc = |id: &str| snap.accounts.iter().find(|a| a.account_id == id).unwrap();
    assert_eq!(acc(&eur.id).funds_converted, 1000_00);
    assert_eq!(acc(&usd.id).funds, 1000_00);
    assert_eq!(
        acc(&usd.id).funds_converted,
        (1000_00f64 * (1.0 / 1.1588)).round() as i64
    );
}

#[test]
fn currency_test_case_performance() {
    let client = load("scenarios/currency_sample.xml");
    let p = client_performance(&client, &test_converter(), d("2015-01-02"), d("2015-01-14"));
    assert_eq!(p.initial_value, 4131_99);
    assert_eq!(p.final_value, 4187_94);
    assert_eq!(
        p.absolute_delta,
        p.final_value - p.transfers - p.initial_value
    );
    assert!((p.irr - 0.505460984).abs() < 0.00000001, "irr {}", p.irr);
}

#[test]
fn currency_test_case_fifo_with_forex() {
    let client = load("scenarios/currency_sample.xml");
    let conv = test_converter();
    let apple = client
        .securities
        .iter()
        .find(|s| s.name == "Apple")
        .unwrap();

    let records = SecurityPerformance::calculate(&client, &conv, NaiveDate::MIN, d("2015-08-09"));
    let r = records.iter().find(|r| r.security_id == apple.id).unwrap();
    assert_eq!(r.fifo_cost, 454_60 + 471_05 + 498_45);

    let records = SecurityPerformance::calculate(&client, &conv, d("2014-12-31"), d("2015-08-10"));
    let r = records.iter().find(|r| r.security_id == apple.id).unwrap();
    assert_eq!(r.shares, 15 * 100_000_000);
    assert_eq!(r.fifo_cost, 454_60 + 471_05 + 498_45);
    assert_eq!(r.market_value, 149534);
    assert_eq!(r.quote, 11552000000);
    assert_eq!(r.moving_average_cost, 142410);
    assert_eq!(r.fees, 824);
    assert_eq!(r.taxes, 824);
    assert_eq!(r.delta, 7124);
    assert_eq!(r.unrealized_gains, 7124);
    assert!((r.unrealized_gains_percent - 0.050024576925777664).abs() < 0.0001);
    assert!((r.irr - 0.1446282845545095).abs() < 0.0001, "irr {}", r.irr);
}

#[test]
fn currency_test_case_fifo_in_usd() {
    let client = load("scenarios/currency_sample.xml");
    let conv = test_converter().with("USD");
    let apple = client
        .securities
        .iter()
        .find(|s| s.name == "Apple")
        .unwrap();
    let records = SecurityPerformance::calculate(&client, &conv, NaiveDate::MIN, d("2015-01-16"));
    let r = records.iter().find(|r| r.security_id == apple.id).unwrap();
    let rate = 1.0 / 0.8237f64;
    let rate = (rate * 1e10).round() / 1e10;
    assert_eq!(r.fifo_cost, (454_60f64 * rate).round() as i64 + 571_90);
}

#[test]
fn volatility_test_case_client() {
    let client = load("scenarios/volatility.xml");
    let conv = test_converter();
    let today = d("2026-01-01");
    let idx = pp_core::index::PerformanceIndex::for_client(
        &client,
        &conv,
        d("2014-01-31"),
        d("2014-07-31"),
        today,
    );
    assert!(idx.warnings.is_empty(), "{:?}", idx.warnings);
    let v = idx.volatility().standard_deviation;
    assert!((v - 0.141568791460).abs() < 1e-10, "volatility {v}");

    let idx = pp_core::index::PerformanceIndex::for_client(
        &client,
        &conv,
        d("2014-01-31"),
        d("2015-01-31"),
        today,
    );
    let v = idx.volatility().standard_deviation;
    assert!((v - 0.200599730118).abs() < 1e-10, "volatility {v}");
}
