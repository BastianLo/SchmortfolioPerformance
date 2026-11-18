//! Serves the embedded frontend (single page app).

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist"]
#[allow_missing = true]
struct Assets;

pub async fn static_handler(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    if let Some(r) = serve(path) {
        return r;
    }
    // SPA fallback
    serve("index.html").unwrap_or_else(|| {
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
            FALLBACK_HTML,
        )
            .into_response()
    })
}

fn serve(path: &str) -> Option<Response> {
    if path.is_empty() {
        return None;
    }
    let file = Assets::get(path)?;
    let mime = file.metadata.mimetype().to_string();
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    Some(
        (
            [
                (header::CONTENT_TYPE, mime),
                (header::CACHE_CONTROL, cache.to_string()),
            ],
            file.data,
        )
            .into_response(),
    )
}

const FALLBACK_HTML: &str = r#"<!doctype html><html lang="de"><head><meta charset="utf-8"><title>SchmortfolioPerformance</title></head>
<body style="font-family:system-ui;padding:2rem"><h1>SchmortfolioPerformance</h1>
<p>Das Frontend wurde nicht eingebettet. Bitte <code>pnpm --dir web build</code> ausführen und den Server neu bauen.</p>
<p>API-Dokumentation: <a href="/api/docs">/api/docs</a></p></body></html>"#;
