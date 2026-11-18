mod api;
mod auth;
mod config;
mod demo;
mod error;
mod jobs;
mod quotes;
mod state;
mod web;

use std::sync::Arc;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("healthcheck") {
        std::process::exit(healthcheck());
    }
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

/// Minimal HTTP probe for container health checks (no curl in the image).
fn healthcheck() -> i32 {
    use std::io::{Read, Write};
    let bind = std::env::var("PP_BIND").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let port = bind.rsplit(':').next().unwrap_or("8080");
    let Ok(mut s) = std::net::TcpStream::connect(format!("127.0.0.1:{port}")) else {
        return 1;
    };
    let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(3)));
    if s.write_all(b"GET /healthz HTTP/1.0\r\nHost: localhost\r\n\r\n")
        .is_err()
    {
        return 1;
    }
    let mut buf = String::new();
    let _ = s.read_to_string(&mut buf);
    if buf.starts_with("HTTP/1.1 200") || buf.starts_with("HTTP/1.0 200") {
        0
    } else {
        1
    }
}
