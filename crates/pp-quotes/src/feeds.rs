//! Security quote feeds. Feed ids follow Portfolio Performance so that
//! imported securities keep their configuration.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use pp_core::model::{LatestSecurityPrice, SecurityPrice};
use pp_core::money::QUOTE_FACTOR;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::FeedError;

pub const YAHOO: &str = "YAHOO";
pub const GENERIC_JSON: &str = "GENERIC-JSON";
pub const GENERIC_HTML_TABLE: &str = "GENERIC_HTML_TABLE";
pub const COINGECKO: &str = "COINGECKO";
pub const ALPHAVANTAGE: &str = "ALPHAVANTAGE";
pub const MANUAL: &str = "MANUAL";

pub const SUPPORTED: &[&str] = &[
    YAHOO,
    GENERIC_JSON,
    GENERIC_HTML_TABLE,
    COINGECKO,
    ALPHAVANTAGE,
];

/// Additional feed settings (stored with the security).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct FeedConfig {
    /// JSON path to the dates, e.g. `$.data[*].date`
    pub date_path: Option<String>,
    /// JSON path to the close values
    pub close_path: Option<String>,
    /// divide values by this factor (e.g. 100 for pence)
    pub factor: Option<f64>,
}

/// Endpoints; overridable for tests.
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub yahoo: String,
    pub coingecko: String,
    pub alphavantage: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            yahoo: "https://query1.finance.yahoo.com".into(),
            coingecko: "https://api.coingecko.com".into(),
            alphavantage: "https://www.alphavantage.co".into(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FeedRequest {
    pub feed: String,
    pub url: Option<String>,
    pub ticker: Option<String>,
    pub isin: Option<String>,
    pub wkn: Option<String>,
    pub currency: Option<String>,
    pub config: FeedConfig,
    /// first date to fetch
    pub from: NaiveDate,
    pub alphavantage_key: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct FeedResult {
    pub prices: Vec<SecurityPrice>,
    pub latest: Option<LatestSecurityPrice>,
}

pub fn to_quote(v: f64) -> i64 {
    (v * QUOTE_FACTOR as f64).round() as i64
}

fn need<'a>(v: &'a Option<String>, what: &str) -> Result<&'a str, FeedError> {
    v.as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| FeedError::Format(format!("{what} fehlt")))
}

fn placeholders(url: &str, r: &FeedRequest) -> String {
    let today = Utc::now().date_naive();
    url.replace("{TICKER}", r.ticker.as_deref().unwrap_or_default())
        .replace("{ISIN}", r.isin.as_deref().unwrap_or_default())
        .replace("{WKN}", r.wkn.as_deref().unwrap_or_default())
        .replace("{CURRENCY}", r.currency.as_deref().unwrap_or_default())
        .replace("{FROM}", &r.from.to_string())
        .replace("{TO}", &today.to_string())
        .replace("{TODAY}", &today.to_string())
}

pub async fn fetch(
    http: &reqwest::Client,
    ep: &Endpoints,
    r: &FeedRequest,
) -> Result<FeedResult, FeedError> {
    let get = |url: String| async move {
        let res = http
            .get(&url)
            .header("accept", "application/json, text/html;q=0.9, */*;q=0.5")
            .send()
            .await?;
        let status = res.status();
        let text = res.text().await?;
        if !status.is_success() {
            return Err(FeedError::Format(format!(
                "HTTP {status}: {}",
                text.chars().take(200).collect::<String>()
            )));
        }
        Ok::<String, FeedError>(text)
    };
    let mut result = match r.feed.as_str() {
        YAHOO => {
            let ticker = need(&r.ticker, "Ticker-Symbol")?;
            let p1 = r.from.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
            let p2 = Utc::now().timestamp() + 86400;
            let url = format!(
                "{}/v8/finance/chart/{ticker}?period1={p1}&period2={p2}&interval=1d&events=history",
                ep.yahoo
            );
            parse_yahoo(&get(url).await?)?
        }
        GENERIC_JSON => {
            let url = placeholders(need(&r.url, "URL")?, r);
            let v: Value = serde_json::from_str(&get(url).await?)
                .map_err(|e| FeedError::Format(e.to_string()))?;
            FeedResult {
                prices: parse_json(&v, &r.config)?,
                latest: None,
            }
        }
        GENERIC_HTML_TABLE => {
            let url = placeholders(need(&r.url, "URL")?, r);
            FeedResult {
                prices: parse_html_table(&get(url).await?)?,
                latest: None,
            }
        }
        COINGECKO => {
            let id = need(&r.ticker, "CoinGecko-ID (Ticker)")?;
            let cur = r.currency.as_deref().unwrap_or("EUR").to_lowercase();
            let days = (Utc::now().date_naive() - r.from).num_days().clamp(1, 365);
            let url = format!(
                "{}/api/v3/coins/{id}/market_chart?vs_currency={cur}&days={days}&interval=daily",
                ep.coingecko
            );
            FeedResult {
                prices: parse_coingecko(&get(url).await?)?,
                latest: None,
            }
        }
        ALPHAVANTAGE => {
            let symbol = need(&r.ticker, "Ticker-Symbol")?;
            let key = need(&r.alphavantage_key, "Alpha-Vantage-API-Key")?;
            let size = if (Utc::now().date_naive() - r.from).num_days() > 100 {
                "full"
            } else {
                "compact"
            };
            let url = format!(
                "{}/query?function=TIME_SERIES_DAILY&symbol={symbol}&outputsize={size}&apikey={key}",
                ep.alphavantage
            );
            FeedResult {
                prices: parse_alphavantage(&get(url).await?)?,
                latest: None,
            }
        }
        other => {
            return Err(FeedError::Format(format!(
                "Kursquelle {other} wird nicht unterstützt"
            )));
        }
    };
    if let Some(f) = r.config.factor.filter(|f| *f != 0.0 && *f != 1.0) {
        for p in &mut result.prices {
            p.value = (p.value as f64 / f).round() as i64;
        }
        if let Some(l) = &mut result.latest {
            l.value = (l.value as f64 / f).round() as i64;
        }
    }
    result.prices.retain(|p| p.date >= r.from && p.value > 0);
    result.prices.sort_by_key(|p| p.date);
    result.prices.dedup_by_key(|p| p.date);
    Ok(result)
}

// ------------------------------------------------------------------ parsers

fn epoch_date(ts: i64) -> Option<NaiveDate> {
    DateTime::<Utc>::from_timestamp(ts, 0).map(|d| d.date_naive())
}

pub fn parse_yahoo(text: &str) -> Result<FeedResult, FeedError> {
    let v: Value = serde_json::from_str(text).map_err(|e| FeedError::Format(e.to_string()))?;
    let res = &v["chart"]["result"][0];
    if res.is_null() {
        let msg = v["chart"]["error"]["description"]
            .as_str()
            .unwrap_or("keine Daten");
        return Err(FeedError::Format(format!("Yahoo: {msg}")));
    }
    let ts = res["timestamp"].as_array().cloned().unwrap_or_default();
    let close = res["indicators"]["quote"][0]["close"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let offset = res["meta"]["gmtoffset"].as_i64().unwrap_or(0);
    let prices = ts
        .iter()
        .zip(close.iter())
        .filter_map(|(t, c)| {
            Some(SecurityPrice {
                date: epoch_date(t.as_i64()? + offset)?,
                value: to_quote(c.as_f64()?),
            })
        })
        .collect();
    let meta = &res["meta"];
    let latest = (|| {
        let price = meta["regularMarketPrice"].as_f64()?;
        let date = epoch_date(meta["regularMarketTime"].as_i64()? + offset)?;
        Some(LatestSecurityPrice {
            date,
            value: to_quote(price),
            high: meta["regularMarketDayHigh"]
                .as_f64()
                .map(to_quote)
                .unwrap_or(-1),
            low: meta["regularMarketDayLow"]
                .as_f64()
                .map(to_quote)
                .unwrap_or(-1),
            volume: meta["regularMarketVolume"].as_i64().unwrap_or(-1),
        })
    })();
    Ok(FeedResult { prices, latest })
}

/// Minimal JSONPath: `$`, `.key`, `['key']`, `[n]`, `[*]`.
pub fn json_path<'a>(v: &'a Value, path: &str) -> Vec<&'a Value> {
    let mut cur: Vec<&Value> = vec![v];
    let p = path.trim().trim_start_matches('$');
    let mut tokens: Vec<String> = Vec::new();
    let mut buf = String::new();
    let mut chars = p.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '.' => {
                if !buf.is_empty() {
                    tokens.push(std::mem::take(&mut buf));
                }
            }
            '[' => {
                if !buf.is_empty() {
                    tokens.push(std::mem::take(&mut buf));
                }
                let mut inner = String::new();
                for c2 in chars.by_ref() {
                    if c2 == ']' {
                        break;
                    }
                    inner.push(c2);
                }
                tokens.push(format!(
                    "[{}]",
                    inner.trim_matches(|c| c == '\'' || c == '"')
                ));
            }
            c => buf.push(c),
        }
    }
    if !buf.is_empty() {
        tokens.push(buf);
    }
    for t in tokens {
        let mut next = Vec::new();
        for v in cur {
            if t == "[*]" {
                match v {
                    Value::Array(a) => next.extend(a.iter()),
                    Value::Object(o) => next.extend(o.values()),
                    _ => {}
                }
            } else if let Some(idx) = t.strip_prefix('[').and_then(|x| x.strip_suffix(']')) {
                if let Ok(i) = idx.parse::<usize>() {
                    if let Some(x) = v.get(i) {
                        next.push(x);
                    }
                } else if let Some(x) = v.get(idx) {
                    next.push(x);
                }
            } else if let Some(x) = v.get(t.as_str()) {
                next.push(x);
            }
        }
        cur = next;
    }
    cur
}

pub fn parse_date_value(v: &Value) -> Option<NaiveDate> {
    match v {
        Value::Number(n) => {
            let ts = n.as_i64()?;
            // milliseconds vs seconds
            epoch_date(if ts > 100_000_000_000 { ts / 1000 } else { ts })
        }
        Value::String(s) => parse_date_str(s),
        _ => None,
    }
}

pub fn parse_date_str(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    for fmt in [
        "%Y-%m-%d", "%d.%m.%Y", "%d.%m.%y", "%m/%d/%Y", "%Y/%m/%d", "%d/%m/%Y", "%Y%m%d",
    ] {
        if let Ok(d) = NaiveDate::parse_from_str(s.get(..s.len().min(10)).unwrap_or(s), fmt) {
            return Some(d);
        }
        if let Ok(d) = NaiveDate::parse_from_str(s, fmt) {
            return Some(d);
        }
    }
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.date_naive())
}

/// Parses numbers in German or English notation.
pub fn parse_number(s: &str) -> Option<f64> {
    let t: String = s
        .trim()
        .chars()
        .filter(|c| c.is_ascii_digit() || matches!(c, ',' | '.' | '-'))
        .collect();
    if t.is_empty() {
        return None;
    }
    let (last_dot, last_comma) = (t.rfind('.'), t.rfind(','));
    let norm = match (last_dot, last_comma) {
        (Some(d), Some(c)) if c > d => t.replace('.', "").replace(',', "."),
        (Some(_), Some(_)) => t.replace(',', ""),
        (None, Some(_)) => t.replace(',', "."),
        _ => t,
    };
    norm.parse().ok()
}

fn value_number(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => parse_number(s),
        _ => None,
    }
}

pub fn parse_json(v: &Value, cfg: &FeedConfig) -> Result<Vec<SecurityPrice>, FeedError> {
    let dp = cfg
        .date_path
        .as_deref()
        .ok_or_else(|| FeedError::Format("JSON-Pfad für Datum fehlt".into()))?;
    let cp = cfg
        .close_path
        .as_deref()
        .ok_or_else(|| FeedError::Format("JSON-Pfad für Kurs fehlt".into()))?;
    let dates = json_path(v, dp);
    let closes = json_path(v, cp);
    if dates.is_empty() || dates.len() != closes.len() {
        return Err(FeedError::Format(format!(
            "JSON-Pfade liefern {} Daten und {} Kurse",
            dates.len(),
            closes.len()
        )));
    }
    Ok(dates
        .iter()
        .zip(closes.iter())
        .filter_map(|(d, c)| {
            Some(SecurityPrice {
                date: parse_date_value(d)?,
                value: to_quote(value_number(c)?),
            })
        })
        .collect())
}

const DATE_HEADERS: &[&str] = &["datum", "date", "handelstag", "tag", "day", "zeit"];
const CLOSE_HEADERS: &[&str] = &[
    "schluss",
    "schlusskurs",
    "close",
    "kurs",
    "price",
    "rücknahmepreis",
    "ruecknahmepreis",
    "nav",
    "letzter",
    "last",
    "rate",
    "wert",
];

pub fn parse_html_table(html: &str) -> Result<Vec<SecurityPrice>, FeedError> {
    let doc = scraper::Html::parse_document(html);
    let table_sel = scraper::Selector::parse("table").unwrap();
    let row_sel = scraper::Selector::parse("tr").unwrap();
    let cell_sel = scraper::Selector::parse("th, td").unwrap();
    let mut best: Vec<SecurityPrice> = Vec::new();
    for table in doc.select(&table_sel) {
        let rows: Vec<Vec<String>> = table
            .select(&row_sel)
            .map(|r| {
                r.select(&cell_sel)
                    .map(|c| c.text().collect::<String>().trim().to_string())
                    .collect()
            })
            .collect();
        let Some(header_idx) = rows.iter().position(|r| {
            r.iter()
                .any(|c| DATE_HEADERS.iter().any(|h| c.to_lowercase().starts_with(h)))
        }) else {
            continue;
        };
        let header: Vec<String> = rows[header_idx].iter().map(|c| c.to_lowercase()).collect();
        let Some(di) = header
            .iter()
            .position(|c| DATE_HEADERS.iter().any(|h| c.starts_with(h)))
        else {
            continue;
        };
        let ci = CLOSE_HEADERS
            .iter()
            .find_map(|h| {
                header
                    .iter()
                    .position(|c| c.starts_with(h) && !c.contains("vortag"))
            })
            .or_else(|| (di + 1 < header.len()).then_some(di + 1));
        let Some(ci) = ci else { continue };
        let prices: Vec<SecurityPrice> = rows[header_idx + 1..]
            .iter()
            .filter_map(|r| {
                Some(SecurityPrice {
                    date: parse_date_str(r.get(di)?)?,
                    value: to_quote(parse_number(r.get(ci)?)?),
                })
            })
            .collect();
        if prices.len() > best.len() {
            best = prices;
        }
    }
    if best.is_empty() {
        return Err(FeedError::Format("keine Kurstabelle gefunden".into()));
    }
    Ok(best)
}

pub fn parse_coingecko(text: &str) -> Result<Vec<SecurityPrice>, FeedError> {
    let v: Value = serde_json::from_str(text).map_err(|e| FeedError::Format(e.to_string()))?;
    let arr = v["prices"]
        .as_array()
        .ok_or_else(|| FeedError::Format("CoinGecko: keine Kurse".into()))?;
    let mut map = BTreeMap::new();
    for p in arr {
        if let (Some(ts), Some(val)) = (p[0].as_i64(), p[1].as_f64())
            && let Some(d) = epoch_date(ts / 1000)
        {
            map.insert(d, to_quote(val));
        }
    }
    Ok(map
        .into_iter()
        .map(|(date, value)| SecurityPrice { date, value })
        .collect())
}

pub fn parse_alphavantage(text: &str) -> Result<Vec<SecurityPrice>, FeedError> {
    let v: Value = serde_json::from_str(text).map_err(|e| FeedError::Format(e.to_string()))?;
    let Some(series) = v["Time Series (Daily)"].as_object() else {
        let msg = v["Note"]
            .as_str()
            .or(v["Error Message"].as_str())
            .or(v["Information"].as_str())
            .unwrap_or("keine Daten");
        return Err(FeedError::Format(format!("Alpha Vantage: {msg}")));
    };
    Ok(series
        .iter()
        .filter_map(|(d, o)| {
            Some(SecurityPrice {
                date: parse_date_str(d)?,
                value: to_quote(value_number(&o["4. close"])?),
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yahoo() {
        let t = r#"{"chart":{"result":[{"meta":{"currency":"EUR","regularMarketPrice":101.5,"regularMarketTime":1704283200,"gmtoffset":3600,"regularMarketDayHigh":102,"regularMarketDayLow":100.5,"regularMarketVolume":1234},
            "timestamp":[1704182400,1704268800,1704355200],"indicators":{"quote":[{"close":[100.25,null,101.0]}]}}],"error":null}}"#;
        let r = parse_yahoo(t).unwrap();
        assert_eq!(r.prices.len(), 2);
        assert_eq!(r.prices[0].date.to_string(), "2024-01-02");
        assert_eq!(r.prices[0].value, 10_025_000_000);
        assert_eq!(r.latest.unwrap().value, 10_150_000_000);
        assert!(
            parse_yahoo(r#"{"chart":{"result":null,"error":{"description":"No data found"}}}"#)
                .is_err()
        );
    }

    #[test]
    fn json_paths() {
        let v: Value = serde_json::from_str(
            r#"{"data":[{"d":"2024-01-02","c":"12,5"},{"d":1704240000000,"c":13}]}"#,
        )
        .unwrap();
        let cfg = FeedConfig {
            date_path: Some("$.data[*].d".into()),
            close_path: Some("$.data[*]['c']".into()),
            factor: None,
        };
        let p = parse_json(&v, &cfg).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].value, 1_250_000_000);
        assert_eq!(p[1].date.to_string(), "2024-01-03");
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_number("1.234,56 €"), Some(1234.56));
        assert_eq!(parse_number("1,234.56"), Some(1234.56));
        assert_eq!(parse_number("12,5"), Some(12.5));
        assert_eq!(parse_number("-3"), Some(-3.0));
    }

    #[test]
    fn html_table() {
        let html = r#"<html><body><table><tr><td>Navigation</td></tr></table>
            <table><thead><tr><th>Datum</th><th>Eröffnung</th><th>Schlusskurs</th><th>Volumen</th></tr></thead>
            <tbody><tr><td>03.01.2024</td><td>10,00</td><td>10,50</td><td>1.000</td></tr>
            <tr><td>02.01.2024</td><td>9,80</td><td>9,95</td><td>900</td></tr></tbody></table></body></html>"#;
        let p = parse_html_table(html).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].value, 1_050_000_000);
        assert_eq!(p[1].date.to_string(), "2024-01-02");
    }

    #[test]
    fn coingecko_and_alphavantage() {
        let p = parse_coingecko(r#"{"prices":[[1704153600000,40000.5],[1704240000000,41000.0]]}"#)
            .unwrap();
        assert_eq!(p.len(), 2);
        let a = parse_alphavantage(r#"{"Time Series (Daily)":{"2024-01-03":{"4. close":"185.6400"},"2024-01-02":{"4. close":"185.0"}}}"#).unwrap();
        assert_eq!(a.len(), 2);
        assert!(
            parse_alphavantage(r#"{"Note":"rate limit"}"#)
                .unwrap_err()
                .to_string()
                .contains("rate limit")
        );
    }

    #[tokio::test]
    async fn fetch_against_mock_server() {
        use axum::routing::get;
        let app = axum::Router::new().route(
            "/v8/finance/chart/{t}",
            get(|| async {
                r#"{"chart":{"result":[{"meta":{"regularMarketPrice":5.0,"regularMarketTime":1704283200},"timestamp":[1704182400],"indicators":{"quote":[{"close":[4.5]}]}}]}}"#
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let ep = Endpoints {
            yahoo: base,
            ..Default::default()
        };
        let r = FeedRequest {
            feed: YAHOO.into(),
            ticker: Some("TEST".into()),
            from: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            ..Default::default()
        };
        let res = fetch(&crate::http_client(), &ep, &r).await.unwrap();
        assert_eq!(res.prices.len(), 1);
        assert_eq!(res.latest.unwrap().value, 500_000_000);
    }
}
