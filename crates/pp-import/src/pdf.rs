//! PDF import of bank statements with declarative rules (YAML).
//!
//! A rule file describes how to recognise a bank and its document types and
//! extracts fields with regular expressions and named capture groups:
//!
//! * `kind` – mapped via `kind.map` (e.g. `Kauf → BUY`) or fixed
//! * `date`, `time`
//! * `name`, `isin`, `wkn` – the security
//! * `shares`, `amount` (total booked on the account), `currency`
//! * `gross` – value before fees and taxes (optional)
//! * `fee…`, `tax…` – summed over all matches; `refund…` reduces taxes;
//!   `<name>_cur` gives the currency of the value
//! * `fx` – rate foreign→account currency, `fxi` – inverse notation
//!   (e.g. `EUR/USD 1,10`); `sec_currency` – currency of the security
//!
//! Additional rules can be placed as `*.yaml` into a directory and loaded at
//! runtime (`load_rules_dir`).

use std::collections::HashMap;

use chrono::{NaiveDate, NaiveDateTime, NaiveTime};
use regex::{Captures, Regex};
use rust_decimal::prelude::*;
use serde::{Deserialize, Serialize};

pub const BUILTIN_RULES: &[(&str, &str)] = &[
    ("traderepublic", include_str!("../rules/traderepublic.yaml")),
    (
        "scalablecapital",
        include_str!("../rules/scalablecapital.yaml"),
    ),
    ("ingdiba", include_str!("../rules/ingdiba.yaml")),
    ("dkb", include_str!("../rules/dkb.yaml")),
];

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum PatternDef {
    Simple(String),
    Full {
        pattern: String,
        #[serde(default)]
        optional: bool,
        #[serde(default)]
        last: bool,
    },
}

#[derive(Debug, Deserialize)]
struct KindDef {
    #[serde(default)]
    fixed: Option<String>,
    #[serde(default)]
    capture: Option<String>,
    #[serde(default)]
    map: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct DocumentDef {
    name: String,
    #[serde(rename = "match")]
    match_: String,
    kind: KindDef,
    fields: Vec<PatternDef>,
    /// fees = total - gross if no fee lines are present
    #[serde(default)]
    derive_fees: bool,
}

#[derive(Debug, Deserialize)]
struct RuleFile {
    bank: String,
    detect: Vec<String>,
    documents: Vec<DocumentDef>,
}

struct Pattern {
    re: Regex,
    optional: bool,
    last: bool,
}

struct Document {
    name: String,
    matcher: Regex,
    kind: KindDef,
    patterns: Vec<Pattern>,
    derive_fees: bool,
}

pub struct BankRules {
    pub id: String,
    pub bank: String,
    detect: Vec<Regex>,
    documents: Vec<Document>,
}

fn compile(p: &str) -> Result<Regex, String> {
    Regex::new(&format!("(?m){p}")).map_err(|e| format!("Regex ungültig: {e}"))
}

impl BankRules {
    pub fn parse(id: &str, yaml: &str) -> Result<Self, String> {
        let f: RuleFile = serde_yaml::from_str(yaml).map_err(|e| format!("{id}: {e}"))?;
        let detect = f
            .detect
            .iter()
            .map(|d| compile(d))
            .collect::<Result<_, _>>()?;
        let mut documents = Vec::new();
        for d in f.documents {
            let patterns = d
                .fields
                .iter()
                .map(|p| match p {
                    PatternDef::Simple(s) => Ok(Pattern {
                        re: compile(s)?,
                        optional: false,
                        last: false,
                    }),
                    PatternDef::Full {
                        pattern,
                        optional,
                        last,
                    } => Ok(Pattern {
                        re: compile(pattern)?,
                        optional: *optional,
                        last: *last,
                    }),
                })
                .collect::<Result<Vec<_>, String>>()?;
            documents.push(Document {
                name: d.name,
                matcher: compile(&d.match_)?,
                kind: d.kind,
                patterns,
                derive_fees: d.derive_fees,
            });
        }
        Ok(Self {
            id: id.into(),
            bank: f.bank,
            detect,
            documents,
        })
    }

    fn detects(&self, text: &str) -> bool {
        self.detect.iter().any(|r| r.is_match(text))
    }
}

pub fn builtin_rules() -> Vec<BankRules> {
    BUILTIN_RULES
        .iter()
        .map(|(id, y)| BankRules::parse(id, y).expect("builtin rule"))
        .collect()
}

/// Loads additional rules from `*.yaml` files in a directory.
pub fn load_rules_dir(dir: &std::path::Path) -> (Vec<BankRules>, Vec<String>) {
    let mut rules = Vec::new();
    let mut errors = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return (rules, errors);
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("yaml") {
            continue;
        }
        let id = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("custom")
            .to_string();
        match std::fs::read_to_string(&p)
            .map_err(|e| e.to_string())
            .and_then(|y| BankRules::parse(&id, &y))
        {
            Ok(r) => rules.push(r),
            Err(e) => errors.push(format!("{}: {e}", p.display())),
        }
    }
    (rules, errors)
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ExtractedTx {
    /// booking kind (BUY, SELL, DIVIDENDS, …)
    pub kind: String,
    pub date_time: NaiveDateTime,
    pub name: Option<String>,
    pub isin: Option<String>,
    pub wkn: Option<String>,
    /// shares (factor 1e8)
    pub shares: i64,
    /// amount booked on the account (factor 100)
    pub amount: i64,
    pub currency: String,
    pub fees: i64,
    pub taxes: i64,
    /// currency of the security, if it differs from `currency`
    pub security_currency: Option<String>,
    /// rate security currency → account currency
    pub exchange_rate: Option<Decimal>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ExtractResult {
    pub bank: Option<String>,
    pub document: Option<String>,
    pub transactions: Vec<ExtractedTx>,
    pub errors: Vec<String>,
}

/// Normalises extracted text: non-breaking spaces, repeated blanks.
pub fn normalize(text: &str) -> String {
    text.replace(['\u{a0}', '\u{202f}'], " ")
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extracts the text of a PDF (all pages, in content order).
pub fn pdf_text(bytes: &[u8]) -> Result<String, String> {
    let doc = lopdf::Document::load_mem(bytes)
        .map_err(|e| format!("PDF konnte nicht gelesen werden: {e}"))?;
    if doc.is_encrypted() {
        return Err("Das PDF ist verschlüsselt".into());
    }
    let pages: Vec<u32> = doc.get_pages().keys().cloned().collect();
    let text = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| doc.extract_text(&pages)))
        .map_err(|_| "PDF konnte nicht gelesen werden".to_string())?
        .map_err(|e| format!("Text konnte nicht extrahiert werden: {e}"))?;
    if text.trim().is_empty() {
        return Err("Das PDF enthält keinen Text (gescannt?)".into());
    }
    Ok(normalize(&text))
}

/// German number notation ("1.234,56"); also accepts "1234.56".
pub fn parse_de(s: &str) -> Option<Decimal> {
    let t: String = s
        .trim()
        .trim_end_matches(['-', '+'])
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let neg = t.starts_with('-');
    let t = t.trim_start_matches(['-', '+']);
    let norm = if t.contains(',') {
        t.replace('.', "").replace(',', ".")
    } else if t.matches('.').count() == 1 && t.split('.').nth(1).is_some_and(|d| d.len() != 3) {
        t.to_string()
    } else {
        t.replace('.', "")
    };
    let d = Decimal::from_str(&norm).ok()?;
    Some(if neg { -d } else { d })
}

fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    for f in ["%d.%m.%Y", "%Y-%m-%d", "%d.%m.%y", "%d/%m/%Y"] {
        if let Ok(d) = NaiveDate::parse_from_str(s, f) {
            return Some(d);
        }
    }
    None
}

fn to_i64(d: Decimal, factor: i64) -> i64 {
    (d.abs() * Decimal::from(factor))
        .round()
        .to_i64()
        .unwrap_or(0)
}

#[derive(Default)]
struct Values {
    single: HashMap<String, String>,
    /// (value, currency) of summed groups
    fees: Vec<(Decimal, Option<String>)>,
    taxes: Vec<(Decimal, Option<String>)>,
}

impl Values {
    fn absorb(&mut self, re: &Regex, caps: &Captures, summed: bool) {
        for name in re.capture_names().flatten() {
            let Some(m) = caps.name(name) else { continue };
            if name.ends_with("_cur") {
                continue;
            }
            let cur = caps
                .name(&format!("{name}_cur"))
                .map(|c| c.as_str().to_string());
            let is_fee = name.starts_with("fee");
            let is_tax = name.starts_with("tax");
            let is_refund = name.starts_with("refund");
            if is_fee || is_tax || is_refund {
                if !summed {
                    continue;
                }
                let Some(v) = parse_de(m.as_str()) else {
                    continue;
                };
                if is_fee {
                    self.fees.push((v.abs(), cur));
                } else if is_tax {
                    self.taxes.push((v.abs(), cur));
                } else {
                    self.taxes.push((-v.abs(), cur));
                }
            } else {
                self.single
                    .insert(name.to_string(), m.as_str().trim().to_string());
                if let Some(c) = cur {
                    self.single.insert(format!("{name}_cur"), c);
                }
            }
        }
    }
}

fn apply_doc(doc: &Document, text: &str) -> Result<ExtractedTx, String> {
    let mut v = Values::default();
    for p in &doc.patterns {
        let has_sum =
            p.re.capture_names()
                .flatten()
                .any(|n| n.starts_with("fee") || n.starts_with("tax") || n.starts_with("refund"));
        let mut any = false;
        let all: Vec<Captures> = p.re.captures_iter(text).collect();
        if has_sum {
            for c in &all {
                v.absorb(&p.re, c, true);
                any = true;
            }
        } else if let Some(c) = if p.last { all.last() } else { all.first() } {
            v.absorb(&p.re, c, false);
            any = true;
        }
        if !any && !p.optional {
            return Err(format!(
                "Muster nicht gefunden: {}",
                p.re.as_str().trim_start_matches("(?m)")
            ));
        }
    }
    let get = |k: &str| v.single.get(k).map(String::as_str);

    let kind = match (&doc.kind.fixed, &doc.kind.capture) {
        (Some(f), _) => f.clone(),
        (None, Some(c)) => {
            let raw = get(c).ok_or("Buchungsart nicht erkannt")?;
            doc.kind
                .map
                .get(raw)
                .cloned()
                .unwrap_or_else(|| raw.to_uppercase())
        }
        _ => return Err("Buchungsart nicht definiert".into()),
    };
    let date = get("date")
        .and_then(parse_date)
        .ok_or("Datum nicht erkannt")?;
    let time = get("time")
        .and_then(|t| {
            NaiveTime::parse_from_str(t, "%H:%M:%S")
                .or_else(|_| NaiveTime::parse_from_str(t, "%H:%M"))
                .ok()
        })
        .unwrap_or_default();
    let amount = get("amount")
        .and_then(parse_de)
        .ok_or("Betrag nicht erkannt")?;
    let currency = get("amount_cur")
        .or(get("currency"))
        .unwrap_or("EUR")
        .to_string();

    // exchange rate security currency -> account currency
    let mut rate = get("fx").and_then(parse_de).filter(|r| !r.is_zero());
    if rate.is_none() {
        rate = get("fxi")
            .and_then(parse_de)
            .filter(|r| !r.is_zero())
            .map(|r| (Decimal::ONE / r).round_dp(10));
    }
    let sec_currency = get("sec_currency")
        .map(String::from)
        .filter(|c| *c != currency);
    let convert = |val: Decimal, cur: &Option<String>| -> Decimal {
        match (cur, rate) {
            (Some(c), Some(r)) if *c != currency => (val * r).round_dp(2),
            _ => val,
        }
    };
    let mut fees: Decimal = v.fees.iter().map(|(x, c)| convert(*x, c)).sum();
    let taxes: Decimal = v.taxes.iter().map(|(x, c)| convert(*x, c)).sum();

    let shares = get("shares").and_then(parse_de).unwrap_or_default();
    if doc.derive_fees
        && fees.is_zero()
        && let Some(gross) = get("gross").and_then(parse_de)
    {
        let gross = convert(gross.abs(), &get("gross_cur").map(String::from));
        let diff = match kind.as_str() {
            "BUY" | "DELIVERY_INBOUND" => amount.abs() - gross - taxes,
            _ => gross - amount.abs() - taxes,
        };
        if diff > Decimal::ZERO {
            fees = diff;
        }
    }
    let trade = matches!(
        kind.as_str(),
        "BUY" | "SELL" | "DELIVERY_INBOUND" | "DELIVERY_OUTBOUND"
    );
    let isin = get("isin").map(|s| s.replace(' ', ""));
    if (trade || kind == "DIVIDENDS")
        && isin.is_none()
        && get("wkn").is_none()
        && get("name").is_none()
    {
        return Err("Wertpapier nicht erkannt".into());
    }
    if trade && shares.is_zero() {
        return Err("Stückzahl nicht erkannt".into());
    }
    Ok(ExtractedTx {
        kind,
        date_time: date.and_time(time),
        name: get("name").map(|n| n.trim().to_string()),
        isin,
        wkn: get("wkn").map(String::from),
        shares: to_i64(shares, 100_000_000),
        amount: to_i64(amount, 100),
        currency,
        fees: to_i64(fees, 100),
        taxes: (taxes * Decimal::from(100)).round().to_i64().unwrap_or(0),
        exchange_rate: sec_currency.as_ref().and(rate),
        security_currency: sec_currency,
    })
}

/// Recognises the bank and document type and extracts the transaction.
pub fn extract(text: &str, rules: &[BankRules]) -> ExtractResult {
    let text = normalize(text);
    let Some(bank) = rules.iter().find(|r| r.detects(&text)) else {
        return ExtractResult {
            bank: None,
            document: None,
            transactions: vec![],
            errors: vec!["Bank nicht erkannt".into()],
        };
    };
    let mut errors = Vec::new();
    for doc in bank.documents.iter().filter(|d| d.matcher.is_match(&text)) {
        match apply_doc(doc, &text) {
            Ok(tx) => {
                return ExtractResult {
                    bank: Some(bank.bank.clone()),
                    document: Some(doc.name.clone()),
                    transactions: vec![tx],
                    errors: vec![],
                };
            }
            Err(e) => errors.push(format!("{}: {e}", doc.name)),
        }
    }
    if errors.is_empty() {
        errors.push("Dokumenttyp nicht unterstützt".into());
    }
    ExtractResult {
        bank: Some(bank.bank.clone()),
        document: None,
        transactions: vec![],
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(parse_de("1.234,56").unwrap().to_string(), "1234.56");
        assert_eq!(parse_de("-0,690").unwrap().to_string(), "-0.690");
        assert_eq!(parse_de("1.000").unwrap().to_string(), "1000");
        assert_eq!(parse_de("2.030,66-").unwrap().to_string(), "2030.66");
        assert_eq!(parse_de("38.10").unwrap().to_string(), "38.10");
    }

    #[test]
    fn builtin_rules_compile() {
        assert_eq!(builtin_rules().len(), BUILTIN_RULES.len());
    }
}
