//! HTTP API (`/api/v1`) and OpenAPI document.

mod clients;
mod data;
mod dto;
mod fx;
mod reports;

use axum::extract::DefaultBodyLimit;
use axum::routing::get;
use axum::{Json, Router};
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;
use utoipa_scalar::{Scalar, Servable};

use crate::state::SharedState;

#[derive(OpenApi)]
#[openapi(
    info(title = "SchmortfolioPerformance API", version = "0.1.0",
         description = "Selbsthostbare Portfolio-Verwaltung, kompatibel zu Portfolio Performance. Geldbeträge sind ganze Zahlen in Hundertsteln (Cent), Stückzahlen und Kurse in 10^-8."),
    servers((url = "/api/v1")),
    tags(
        (name = "auth", description = "Anmeldung (lokal und OIDC)"),
        (name = "clients", description = "Portfolios (entspricht einer PP-Datei) und Import"),
        (name = "data", description = "Wertpapiere, Konten, Depots, Buchungen"),
        (name = "reports", description = "Auswertungen"),
        (name = "system", description = "Wechselkurse und Systemstatus"),
    )
)]
struct ApiDoc;

fn api_router() -> OpenApiRouter<SharedState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(crate::auth::router())
        .merge(clients::router())
        .merge(data::router())
        .merge(reports::router())
        .merge(fx::router())
}

pub fn openapi_json() -> String {
    let (_, api) = api_router().split_for_parts();
    api.to_pretty_json().unwrap()
}

async fn health() -> &'static str {
    "ok"
}

pub fn app(state: SharedState) -> Router {
    let (api, openapi) = api_router().split_for_parts();
    let openapi_for_json = openapi.clone();
    let max_upload = state.config.max_upload_mb * 1024 * 1024;

    let api = api
        .route(
            "/openapi.json",
            get(move || async move { Json(openapi_for_json) }),
        )
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            crate::auth::origin_check,
        ))
        .layer(DefaultBodyLimit::max(max_upload));

    Router::new()
        .nest("/api/v1", api)
        .merge(Scalar::with_url("/api/docs", openapi))
        .route("/healthz", get(health))
        .fallback(crate::web::static_handler)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[cfg(test)]
mod tests;
