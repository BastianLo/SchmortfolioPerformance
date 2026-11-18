//! Quote and exchange rate feeds.
//!
//! Currently: ECB euro foreign exchange reference rates.

pub mod feeds;

use std::collections::BTreeMap;
use std::io::Read;
use std::str::FromStr;

use chrono::NaiveDate;
use pp_core::ExchangeRateSeries;
use rust_decimal::Decimal;

#[derive(Debug, thiserror::Error)]
pub enum FeedError {
    #[error("HTTP: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Format: {0}")]
    Format(String),
}

pub const ECB_HIST_ZIP: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-hist.zip";
pub const ECB_90D_XML: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-hist-90d.xml";

pub fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("SchmortfolioPerformance/0.1")
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("http client")
}

fn into_series(map: BTreeMap<String, Vec<(NaiveDate, Decimal)>>) -> Vec<ExchangeRateSeries> {
    map.into_iter()
        .map(|(term, mut rates)| {
            rates.sort_by_key(|r| r.0);
            ExchangeRateSeries {
                base: "EUR".into(),
                term,
                rates,
            }
        })
        .collect()
}

/// Parses the CSV contained in `eurofxref-hist.zip`.
pub fn parse_ecb_csv(text: &str) -> Result<Vec<ExchangeRateSeries>, FeedError> {
    let mut lines = text.lines();
    let header: Vec<&str> = lines
        .next()
        .ok_or_else(|| FeedError::Format("leere CSV".into()))?
        .split(',')
        .map(str::trim)
        .collect();
    let mut map: BTreeMap<String, Vec<(NaiveDate, Decimal)>> = BTreeMap::new();
    for line in lines {
        let cols: Vec<&str> = line.split(',').map(str::trim).collect();
        let Some(date) = cols
            .first()
            .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        else {
            continue;
        };
        for (i, v) in cols.iter().enumerate().skip(1) {
            let Some(cur) = header.get(i).filter(|c| c.len() == 3) else {
                continue;
            };
            if let Ok(rate) = Decimal::from_str(v) {
                map.entry(cur.to_string()).or_default().push((date, rate));
            }
        }
    }
    Ok(into_series(map))
}

/// Parses the ECB `gesmes` XML (daily / 90 days / history).
pub fn parse_ecb_xml(text: &str) -> Result<Vec<ExchangeRateSeries>, FeedError> {
    let doc = roxmltree::Document::parse(text).map_err(|e| FeedError::Format(e.to_string()))?;
    let mut map: BTreeMap<String, Vec<(NaiveDate, Decimal)>> = BTreeMap::new();
    for day in doc.descendants().filter(|n| n.has_attribute("time")) {
        let Some(date) = day
            .attribute("time")
            .and_then(|t| NaiveDate::parse_from_str(t, "%Y-%m-%d").ok())
        else {
            continue;
        };
        for c in day.children().filter(|c| c.has_attribute("currency")) {
            let (Some(cur), Some(rate)) = (c.attribute("currency"), c.attribute("rate")) else {
                continue;
            };
            if let Ok(rate) = Decimal::from_str(rate) {
                map.entry(cur.to_string()).or_default().push((date, rate));
            }
        }
    }
    Ok(into_series(map))
}

/// Downloads the full ECB history.
pub async fn fetch_ecb_history(
    client: &reqwest::Client,
) -> Result<Vec<ExchangeRateSeries>, FeedError> {
    let bytes = client
        .get(ECB_HIST_ZIP)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| FeedError::Format(e.to_string()))?;
    let mut file = archive
        .by_index(0)
        .map_err(|e| FeedError::Format(e.to_string()))?;
    let mut text = String::new();
    file.read_to_string(&mut text)
        .map_err(|e| FeedError::Format(e.to_string()))?;
    parse_ecb_csv(&text)
}

/// Downloads the last 90 days.
pub async fn fetch_ecb_recent(
    client: &reqwest::Client,
) -> Result<Vec<ExchangeRateSeries>, FeedError> {
    let text = client
        .get(ECB_90D_XML)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    parse_ecb_xml(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv() {
        let s = parse_ecb_csv(
            "Date,USD,JPY,CYP,\n2024-01-03,1.0919,155.12,N/A,\n2024-01-02,1.0956,155.46,N/A,\n",
        )
        .unwrap();
        assert_eq!(s.len(), 2);
        let usd = s.iter().find(|s| s.term == "USD").unwrap();
        assert_eq!(usd.rates[0].0.to_string(), "2024-01-02");
    }

    #[test]
    fn xml() {
        let x = r#"<?xml version="1.0" encoding="UTF-8"?>
<gesmes:Envelope xmlns:gesmes="http://www.gesmes.org/xml/2002-08-01" xmlns="http://www.ecb.int/vocabulary/2002-08-01/eurofxref">
<Cube><Cube time="2024-01-03"><Cube currency="USD" rate="1.0919"/><Cube currency="CHF" rate="0.9326"/></Cube></Cube></gesmes:Envelope>"#;
        let s = parse_ecb_xml(x).unwrap();
        assert_eq!(s.len(), 2);
    }
}
