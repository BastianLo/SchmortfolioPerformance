//! PDF rules against the sample texts in testdata/pdf (from PP's test suite).
//! Expected values were checked manually against the documents.

use std::path::PathBuf;

use pp_import::pdf::{builtin_rules, extract};

fn run(rel: &str) -> pp_import::pdf::ExtractedTx {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/pdf")
        .join(rel);
    let text = String::from_utf8_lossy(&std::fs::read(p).unwrap()).to_string();
    let r = extract(&text, &builtin_rules());
    assert!(r.errors.is_empty(), "{rel}: {:?}", r.errors);
    assert_eq!(r.transactions.len(), 1, "{rel}");
    r.transactions.into_iter().next().unwrap()
}

/// (file, kind, date, isin, shares * 1e8, amount, fees, taxes, security currency)
#[allow(clippy::type_complexity)]
const CASES: &[(&str, &str, &str, &str, i64, i64, i64, i64, Option<&str>)] = &[
    (
        "traderepublic/Kauf01.txt",
        "BUY",
        "2019-05-13",
        "IE00BKM4GZ66",
        100_000_000,
        25_05,
        1_00,
        0,
        None,
    ),
    (
        "traderepublic/Kauf02.txt",
        "BUY",
        "2019-05-13",
        "US08862E1091",
        100_000_000,
        59_19,
        1_00,
        0,
        None,
    ),
    (
        "traderepublic/Verkauf01.txt",
        "SELL",
        "2019-06-18",
        "AU000000CUV3",
        8_000_000_000,
        1792_29,
        1_00,
        32_31,
        None,
    ),
    (
        "traderepublic/Verkauf02.txt",
        "SELL",
        "2020-06-10",
        "DE000TR95XU9",
        50_000_000_000,
        3615_63,
        1_00,
        -21_63,
        None,
    ),
    (
        "traderepublic/Sparplan01.txt",
        "BUY",
        "2019-11-18",
        "IE00B4L5Y983",
        45_340_000,
        25_00,
        0,
        0,
        None,
    ),
    (
        "traderepublic/Sparplan02.txt",
        "BUY",
        "2020-05-04",
        "DE000A0H0744",
        492_610_000,
        100_00,
        0,
        0,
        None,
    ),
    (
        "traderepublic/Dividende01.txt",
        "DIVIDENDS",
        "2019-09-25",
        "IE00B652H904",
        1_000_000_000,
        4_18,
        0,
        93,
        Some("USD"),
    ),
    (
        "traderepublic/Dividende02.txt",
        "DIVIDENDS",
        "2019-09-12",
        "US5949181045",
        1_000_000_000,
        3_10,
        0,
        1_06,
        Some("USD"),
    ),
    (
        "scalablecapital/Kauf01.txt",
        "BUY",
        "2024-12-12",
        "IE0008T6IUX0",
        300_000_000,
        19_49,
        99,
        0,
        None,
    ),
    (
        "scalablecapital/Kauf02.txt",
        "BUY",
        "2025-09-05",
        "CA25039N4084",
        2_300_000_000,
        2_11,
        99,
        0,
        None,
    ),
    (
        "scalablecapital/Kauf03.txt",
        "BUY",
        "2025-09-16",
        "US4228062083",
        400_000_000,
        864_00,
        0,
        0,
        None,
    ),
    (
        "scalablecapital/Verkauf01.txt",
        "SELL",
        "2024-12-30",
        "LU2903252349",
        100_000_000,
        8_60,
        99,
        0,
        None,
    ),
    (
        "scalablecapital/Verkauf02.txt",
        "SELL",
        "2025-05-12",
        "DE000HD5ZFL9",
        20_000_000_000,
        472_00,
        0,
        0,
        None,
    ),
    (
        "scalablecapital/Dividende01.txt",
        "DIVIDENDS",
        "2025-01-15",
        "US00123Q1040",
        66_312_900,
        7,
        0,
        1,
        Some("USD"),
    ),
    (
        "scalablecapital/Dividende02.txt",
        "DIVIDENDS",
        "2025-06-16",
        "US5949181045",
        28_428_500,
        15,
        0,
        5,
        Some("USD"),
    ),
    (
        "ingdiba/Kauf01.txt",
        "BUY",
        "2015-11-17",
        "DE0002635307",
        1_400_000_000,
        533_39,
        0,
        0,
        None,
    ),
    (
        "ingdiba/Kauf02.txt",
        "BUY",
        "2015-06-11",
        "DE0002635307",
        1_800_000_000,
        726_28,
        0,
        0,
        None,
    ),
    (
        "ingdiba/Verkauf01.txt",
        "SELL",
        "2015-05-07",
        "DE0006632003",
        3_100_000_000,
        1887_64,
        9_90,
        0,
        None,
    ),
    (
        "ingdiba/Verkauf02.txt",
        "SELL",
        "2016-12-22",
        "DE0002635281",
        3_000_000_000,
        568_41,
        9_90,
        19_32,
        None,
    ),
    (
        "ingdiba/Dividende01.txt",
        "DIVIDENDS",
        "2016-12-15",
        "US5801351017",
        6_600_000_000,
        44_01,
        0,
        15_10,
        Some("USD"),
    ),
    (
        "ingdiba/Dividende02.txt",
        "DIVIDENDS",
        "2016-12-15",
        "DE000A0H0744",
        69_400_000_000,
        234_92,
        0,
        68_68,
        None,
    ),
    (
        "dkb/Kauf02.txt",
        "BUY",
        "2016-01-25",
        "BMG7945E1057",
        100_000_000_000,
        1760_91,
        10_91,
        0,
        None,
    ),
    (
        "dkb/Kauf03.txt",
        "BUY",
        "2017-03-06",
        "LU0392494562",
        2_928_930_000,
        1410_00,
        10_00,
        0,
        None,
    ),
    (
        "dkb/Verkauf02.txt",
        "SELL",
        "2016-02-10",
        "DE0005140008",
        20_000_000_000,
        3000_00,
        10_00,
        0,
        None,
    ),
    (
        "dkb/Dividende02.txt",
        "DIVIDENDS",
        "2016-04-07",
        "DE0007100000",
        3_000_000_000,
        97_50,
        0,
        0,
        None,
    ),
    (
        "dkb/Dividende03.txt",
        "DIVIDENDS",
        "2017-12-07",
        "IE00B3XNN521",
        21_600_000_000,
        32_93,
        0,
        12_68,
        None,
    ),
];

#[test]
fn bank_documents() {
    for (file, kind, date, isin, shares, amount, fees, taxes, sec_cur) in CASES {
        let t = run(file);
        assert_eq!(t.kind, *kind, "{file}");
        assert_eq!(t.date_time.date().to_string(), *date, "{file}");
        assert_eq!(t.isin.as_deref(), Some(*isin), "{file}");
        assert_eq!(t.shares, *shares, "{file}");
        assert_eq!(t.amount, *amount, "{file}");
        assert_eq!(t.fees, *fees, "{file}");
        assert_eq!(t.taxes, *taxes, "{file}");
        assert_eq!(t.currency, "EUR", "{file}");
        assert_eq!(t.security_currency.as_deref(), *sec_cur, "{file}");
        assert_eq!(t.exchange_rate.is_some(), sec_cur.is_some(), "{file}");
    }
}

#[test]
fn unknown_bank_is_reported() {
    let r = extract("Irgendeine Bank\nKontoauszug", &builtin_rules());
    assert_eq!(r.bank, None);
    assert!(!r.errors.is_empty());
}

#[test]
fn real_pdf_files() {
    // PDFs generated from the sample texts (Chromium print), read with lopdf
    for (file, isin, amount) in [
        ("generated/traderepublic_Kauf01.pdf", "IE00BKM4GZ66", 25_05),
        ("generated/dkb_Dividende03.pdf", "IE00B3XNN521", 32_93),
    ] {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/pdf")
            .join(file);
        let text = pp_import::pdf::pdf_text(&std::fs::read(p).unwrap()).unwrap();
        let r = extract(&text, &builtin_rules());
        assert!(r.errors.is_empty(), "{file}: {:?}\n{text}", r.errors);
        assert_eq!(r.transactions[0].isin.as_deref(), Some(isin));
        assert_eq!(r.transactions[0].amount, amount);
    }
    assert!(pp_import::pdf::pdf_text(b"no pdf").is_err());
}
