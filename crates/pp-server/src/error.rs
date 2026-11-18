//! API errors as RFC 9457 problem details.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Nicht angemeldet")]
    Unauthorized,
    #[error("Keine Berechtigung")]
    Forbidden,
    #[error("Nicht gefunden")]
    NotFound,
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Conflict(String),
    #[error("Interner Fehler: {0}")]
    Internal(String),
}

#[derive(Serialize, ToSchema)]
pub struct Problem {
    #[serde(rename = "type")]
    pub kind: String,
    pub title: String,
    pub status: u16,
    pub detail: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self {
            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,
            ApiError::Forbidden => StatusCode::FORBIDDEN,
            ApiError::NotFound => StatusCode::NOT_FOUND,
            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ApiError::Conflict(_) => StatusCode::CONFLICT,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        if let ApiError::Internal(e) = &self {
            tracing::error!("internal error: {e}");
        }
        let body = Problem {
            kind: "about:blank".into(),
            title: status.canonical_reason().unwrap_or("Error").into(),
            status: status.as_u16(),
            detail: self.to_string(),
        };
        let mut res = (status, axum::Json(body)).into_response();
        res.headers_mut()
            .insert("content-type", "application/problem+json".parse().unwrap());
        res
    }
}

impl From<pp_db::DbError> for ApiError {
    fn from(e: pp_db::DbError) -> Self {
        match e {
            pp_db::DbError::NotFound => ApiError::NotFound,
            pp_db::DbError::Invalid(m) => ApiError::Conflict(m),
            other => ApiError::Internal(other.to_string()),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        ApiError::Internal(e.to_string())
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
