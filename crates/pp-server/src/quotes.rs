//! Updating security prices from quote feeds.

use std::sync::Arc;

use chrono::Duration;
use pp_core::model::Security;
use pp_quotes::feeds::{self, Endpoints, FeedConfig, FeedRequest};
use serde::Serialize;
use tokio::sync::Semaphore;
use utoipa::ToSchema;

use crate::state::AppState;

pub const FEED_CONFIG_KEY: &str = "_feed";

pub fn feed_config(s: &Security) -> FeedConfig {
    s.attributes
        .get(FEED_CONFIG_KEY)
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

pub fn endpoints() -> Endpoints {
    let mut e = Endpoints::default();
    if let Ok(v) = std::env::var("PP_YAHOO_URL") {
        e.yahoo = v;
    }
    if let Ok(v) = std::env::var("PP_COINGECKO_URL") {
        e.coingecko = v;
    }
    if let Ok(v) = std::env::var("PP_ALPHAVANTAGE_URL") {
        e.alphavantage = v;
    }
    e
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct QuoteUpdateResult {
    pub security_id: String,
    pub name: String,
    pub feed: String,
    pub ok: bool,
    pub new_prices: usize,
    pub message: Option<String>,
}

/// Fetches new prices for the securities of a client (all active securities
/// with a supported feed, or only `only`).
pub async fn update_client(
    state: &Arc<AppState>,
    client_id: &str,
    only: Option<&str>,
) -> anyhow::Result<Vec<QuoteUpdateResult>> {
    let client = state.db.load_client(client_id).await?;
    let api_key = state.db.setting("alphavantage_api_key").await?;
    let http = pp_quotes::http_client();
    let ep = endpoints();
    let sem = Arc::new(Semaphore::new(4));
    let today = crate::api::today();

    let mut tasks = Vec::new();
    for s in client.securities.iter() {
        if only.is_some_and(|o| o != s.id) {
            continue;
        }
        if only.is_none() && s.retired {
            continue;
        }
        let Some(feed) = s
            .feed
            .clone()
            .filter(|f| feeds::SUPPORTED.contains(&f.as_str()))
        else {
            if only.is_some() {
                tasks.push(tokio::spawn({
                    let r = QuoteUpdateResult {
                        security_id: s.id.clone(),
                        name: s.name.clone(),
                        feed: s.feed.clone().unwrap_or_default(),
                        ok: false,
                        new_prices: 0,
                        message: Some("Keine automatische Kursquelle konfiguriert".into()),
                    };
                    async move { (r, None) }
                }));
            }
            continue;
        };
        // refetch a few days to pick up corrections
        let from = s
            .prices
            .last()
            .map(|p| p.date - Duration::days(5))
            .unwrap_or(today - Duration::days(365 * 10));
        let req = FeedRequest {
            feed: feed.clone(),
            url: s.feed_url.clone(),
            ticker: s.ticker.clone(),
            isin: s.isin.clone(),
            wkn: s.wkn.clone(),
            currency: s.currency.clone(),
            config: feed_config(s),
            from,
            alphavantage_key: api_key.clone(),
        };
        let existing: std::collections::HashMap<_, _> =
            s.prices.iter().map(|p| (p.date, p.value)).collect();
        let (http, ep, sem) = (http.clone(), ep.clone(), sem.clone());
        let (sid, name) = (s.id.clone(), s.name.clone());
        tasks.push(tokio::spawn(async move {
            let _permit = sem.acquire().await;
            let res = feeds::fetch(&http, &ep, &req).await;
            match res {
                Ok(r) => {
                    let new: Vec<_> = r
                        .prices
                        .into_iter()
                        .filter(|p| existing.get(&p.date) != Some(&p.value))
                        .collect();
                    (
                        QuoteUpdateResult {
                            security_id: sid,
                            name,
                            feed,
                            ok: true,
                            new_prices: new.len(),
                            message: None,
                        },
                        Some((new, r.latest)),
                    )
                }
                Err(e) => (
                    QuoteUpdateResult {
                        security_id: sid,
                        name,
                        feed,
                        ok: false,
                        new_prices: 0,
                        message: Some(e.to_string()),
                    },
                    None,
                ),
            }
        }));
    }

    let mut results = Vec::new();
    let mut changed = false;
    for t in tasks {
        let (r, data) = t.await?;
        if let Some((prices, latest)) = data {
            if !prices.is_empty() {
                state
                    .db
                    .upsert_prices(client_id, &r.security_id, &prices)
                    .await?;
                changed = true;
            }
            if let Some(l) = latest {
                state.db.set_latest(client_id, &r.security_id, &l).await?;
                changed = true;
            }
        }
        if !r.feed.is_empty() && feeds::SUPPORTED.contains(&r.feed.as_str()) {
            state
                .db
                .set_feed_status(
                    client_id,
                    &r.security_id,
                    r.ok,
                    r.message.as_deref(),
                    r.new_prices as i64,
                )
                .await?;
        }
        results.push(r);
    }
    if changed {
        state.db.bump_version(client_id).await?;
    }
    Ok(results)
}

/// Updates all clients (background job).
pub async fn update_all(state: &Arc<AppState>) -> anyhow::Result<()> {
    for id in state.db.all_client_ids().await? {
        match update_client(state, &id, None).await {
            Ok(r) => {
                let failed = r.iter().filter(|x| !x.ok).count();
                let new: usize = r.iter().map(|x| x.new_prices).sum();
                tracing::info!("quotes updated for client {id}: {new} new prices, {failed} errors");
            }
            Err(e) => tracing::warn!("quote update for client {id} failed: {e:#}"),
        }
    }
    Ok(())
}
