//! Background jobs: exchange rate updates, session cleanup.

use std::time::Duration;

use crate::state::{AppState, SharedState};

pub fn spawn(state: SharedState) {
    tokio::spawn(async move {
        // small delay so the server is up before the first network request
        tokio::time::sleep(Duration::from_secs(3)).await;
        loop {
            if state.config.ecb_update
                && let Err(e) = update_exchange_rates(&state).await
            {
                tracing::warn!("ECB exchange rate update failed: {e:#}");
            }
            if state.config.quote_update
                && let Err(e) = crate::quotes::update_all(&state).await
            {
                tracing::warn!("quote update failed: {e:#}");
            }
            if let Ok(n) = state.db.purge_sessions().await
                && n > 0
            {
                tracing::debug!("purged {n} expired sessions");
            }
            tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
        }
    });
}

/// Fetches ECB rates: the full history on first run, afterwards 90 days.
pub async fn update_exchange_rates(state: &AppState) -> anyhow::Result<usize> {
    let _guard = state.fx_update.lock().await;
    let http = pp_quotes::http_client();
    let (count, _) = state.db.exchange_rate_status().await?;
    let series = if count == 0 {
        pp_quotes::fetch_ecb_history(&http).await?
    } else {
        pp_quotes::fetch_ecb_recent(&http).await?
    };
    let n = state.db.upsert_exchange_rates(&series, "ECB").await?;
    state
        .db
        .set_setting("ecb_last_update", &chrono::Utc::now().to_rfc3339())
        .await?;
    state.reload_exchange_rates().await?;
    tracing::info!("ECB exchange rates updated ({n} rates)");
    Ok(n)
}
