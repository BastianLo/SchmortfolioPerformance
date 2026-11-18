mod api;
mod auth;
mod config;
mod demo;
mod error;
mod jobs;
mod state;
mod web;

use std::sync::Arc;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("openapi") {
        println!("{}", api::openapi_json());
        return Ok(());
    }

    let json_logs = std::env::var("PP_LOG_FORMAT")
        .map(|v| v == "json")
        .unwrap_or(false);
    let filter =
        EnvFilter::try_from_env("PP_LOG").unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"));
    if json_logs {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .json()
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    let config = config::Config::from_env();
    if config.database_url.starts_with("sqlite://") {
        std::fs::create_dir_all(&config.data_dir)?;
    }
    let db = pp_db::Db::connect(&config.database_url).await?;

    let oidc = match &config.oidc {
        Some(c) => match auth::oidc::Oidc::discover(c.clone()).await {
            Ok(o) => {
                tracing::info!("OIDC enabled (issuer {})", c.issuer);
                Some(o)
            }
            Err(e) => {
                tracing::error!(
                    "OIDC discovery for {} failed: {e:#}. SSO is disabled.",
                    c.issuer
                );
                None
            }
        },
        None => None,
    };

    let state = Arc::new(state::AppState::new(config.clone(), db, oidc).await?);
    auth::bootstrap_admin(&state).await?;
    jobs::spawn(state.clone());

    let app = api::app(state.clone());
    let listener = tokio::net::TcpListener::bind(&config.bind).await?;
    tracing::info!(
        "SchmortfolioPerformance listening on http://{} (public url {})",
        config.bind,
        config.public_url
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
}
