//! Exchange rates and system status.

use axum::Json;
use axum::extract::State;
use serde::Serialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::state::SharedState;

#[derive(Serialize, ToSchema)]
pub struct ExchangeRateStatus {
    pub rates: i64,
    pub last_date: Option<String>,
    pub last_update: Option<String>,
    pub currencies: Vec<String>,
}

async fn status(state: &SharedState) -> ApiResult<ExchangeRateStatus> {
    let (rates, last_date) = state.db.exchange_rate_status().await?;
    let series = state.exchange_rate_series().await;
    Ok(ExchangeRateStatus {
        rates,
        last_date,
        last_update: state.db.setting("ecb_last_update").await?,
        currencies: series
            .iter()
            .map(|s| format!("{}/{}", s.base, s.term))
            .collect(),
    })
}

#[utoipa::path(get, path = "/exchange-rates", tag = "system", responses((status = 200, body = ExchangeRateStatus)))]
async fn get_status(
    State(state): State<SharedState>,
    _user: CurrentUser,
) -> ApiResult<Json<ExchangeRateStatus>> {
    Ok(Json(status(&state).await?))
}

/// Downloads the ECB reference rates now (admin only).
#[utoipa::path(post, path = "/exchange-rates/update", tag = "system", responses((status = 200, body = ExchangeRateStatus), (status = 502)))]
async fn update(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<ExchangeRateStatus>> {
    if !user.is_admin {
        return Err(ApiError::Forbidden);
    }
    crate::jobs::update_exchange_rates(&state)
        .await
        .map_err(|e| ApiError::BadRequest(format!("Aktualisierung fehlgeschlagen: {e:#}")))?;
    Ok(Json(status(&state).await?))
}

pub fn router() -> OpenApiRouter<SharedState> {
    OpenApiRouter::new()
        .routes(routes!(get_status))
        .routes(routes!(update))
}
