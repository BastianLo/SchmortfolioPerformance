//! Authentication: local accounts, sessions and OIDC.

pub mod oidc;

use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use axum::Json;
use axum::extract::{FromRequestParts, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use base64::Engine;
use pp_db::{NewUser, User};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::error::{ApiError, ApiResult};
use crate::state::SharedState;

pub const SESSION_COOKIE: &str = "pp_session";

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| {
            Argon2::default()
                .verify_password(password.as_bytes(), &h)
                .is_ok()
        })
        .unwrap_or(false)
}

pub fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn token_hash(token: &str) -> String {
    let d = Sha256::digest(token.as_bytes());
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// The authenticated user of a request.
pub struct CurrentUser(pub User);

impl FromRequestParts<SharedState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &SharedState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar
            .get(SESSION_COOKIE)
            .map(|c| c.value().to_string())
            .ok_or(ApiError::Unauthorized)?;
        let (user, _) = state
            .db
            .session_user(&token_hash(&token))
            .await?
            .ok_or(ApiError::Unauthorized)?;
        Ok(CurrentUser(user))
    }
}

pub fn session_cookie(state: &SharedState, token: String) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, token))
        .path("/")
        .http_only(true)
        .secure(state.config.cookie_secure)
        .same_site(SameSite::Lax)
        .max_age(time_days(state.config.session_days))
        .build()
}

fn time_days(days: i64) -> time::Duration {
    time::Duration::days(days)
}

pub async fn start_session(
    state: &SharedState,
    jar: CookieJar,
    user: &User,
    id_token: Option<&str>,
) -> ApiResult<CookieJar> {
    let token = random_token();
    let expires = chrono::Utc::now() + chrono::Duration::days(state.config.session_days);
    state
        .db
        .create_session(
            &token_hash(&token),
            &user.id,
            &expires.to_rfc3339(),
            id_token,
        )
        .await?;
    Ok(jar.add(session_cookie(state, token)))
}

/// Rejects state changing requests from foreign origins (CSRF protection in
/// addition to SameSite cookies).
pub async fn origin_check(
    State(state): State<SharedState>,
    headers: HeaderMap,
    req: axum::extract::Request,
    next: Next,
) -> Response {
    let safe = matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS);
    if !safe && let Some(origin) = headers.get("origin").and_then(|o| o.to_str().ok()) {
        let host = headers
            .get("host")
            .and_then(|h| h.to_str().ok())
            .unwrap_or_default();
        let origin_host = origin.split("://").nth(1).unwrap_or(origin);
        let public = state
            .config
            .public_url
            .split("://")
            .nth(1)
            .unwrap_or_default();
        if origin_host != host && origin_host != public {
            return (StatusCode::FORBIDDEN, "cross-origin request rejected").into_response();
        }
    }
    next.run(req).await
}

#[derive(Serialize, ToSchema)]
pub struct AuthConfig {
    /// no user exists yet: the first user can be created via `/auth/setup`
    pub setup_required: bool,
    pub local_login: bool,
    pub oidc: bool,
    pub oidc_label: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, ToSchema)]
pub struct Me {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub email: Option<String>,
    pub is_admin: bool,
    pub sso: bool,
}

impl From<&User> for Me {
    fn from(u: &User) -> Self {
        Me {
            id: u.id.clone(),
            username: u.username.clone(),
            display_name: u.display_name.clone(),
            email: u.email.clone(),
            is_admin: u.is_admin,
            sso: u.oidc_subject.is_some(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct LogoutResponse {
    /// URL of the identity provider to end the SSO session, if available
    pub redirect: Option<String>,
}

#[utoipa::path(get, path = "/auth/config", tag = "auth", responses((status = 200, body = AuthConfig)))]
async fn auth_config(State(state): State<SharedState>) -> ApiResult<Json<AuthConfig>> {
    Ok(Json(AuthConfig {
        setup_required: state.db.count_users().await? == 0,
        local_login: !state.config.disable_local_login,
        oidc: state.oidc.is_some(),
        oidc_label: state.config.oidc.as_ref().map(|o| o.button_label.clone()),
    }))
}

#[utoipa::path(post, path = "/auth/setup", tag = "auth", request_body = Credentials, responses((status = 200, body = Me)))]
async fn setup(
    State(state): State<SharedState>,
    jar: CookieJar,
    Json(c): Json<Credentials>,
) -> ApiResult<(CookieJar, Json<Me>)> {
    if state.db.count_users().await? > 0 {
        return Err(ApiError::Conflict(
            "Einrichtung bereits abgeschlossen".into(),
        ));
    }
    validate_credentials(&c)?;
    let hash = hash_password(&c.password)?;
    let user = state
        .db
        .create_user(NewUser {
            username: c.username.trim(),
            email: None,
            display_name: None,
            password_hash: Some(&hash),
            is_admin: true,
            oidc_issuer: None,
            oidc_subject: None,
        })
        .await?;
    let jar = start_session(&state, jar, &user, None).await?;
    Ok((jar, Json(Me::from(&user))))
}

fn validate_credentials(c: &Credentials) -> ApiResult<()> {
    if c.username.trim().len() < 2 {
        return Err(ApiError::BadRequest("Benutzername zu kurz".into()));
    }
    if c.password.len() < 8 {
        return Err(ApiError::BadRequest(
            "Passwort muss mindestens 8 Zeichen haben".into(),
        ));
    }
    Ok(())
}

#[utoipa::path(post, path = "/auth/login", tag = "auth", request_body = Credentials, responses((status = 200, body = Me), (status = 401)))]
async fn login(
    State(state): State<SharedState>,
    jar: CookieJar,
    Json(c): Json<Credentials>,
) -> ApiResult<(CookieJar, Json<Me>)> {
    if state.config.disable_local_login {
        return Err(ApiError::Forbidden);
    }
    let user = state.db.user_by_username(c.username.trim()).await?;
    let ok = match &user {
        Some(u) => u
            .password_hash
            .as_deref()
            .map(|h| verify_password(&c.password, h))
            .unwrap_or(false),
        None => {
            // constant-ish time: hash anyway
            let _ = hash_password(&c.password);
            false
        }
    };
    let Some(user) = user.filter(|_| ok) else {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        return Err(ApiError::Unauthorized);
    };
    state
        .db
        .update_user_profile(&user.id, None, None, None)
        .await?;
    let jar = start_session(&state, jar, &user, None).await?;
    Ok((jar, Json(Me::from(&user))))
}

#[utoipa::path(post, path = "/auth/logout", tag = "auth", responses((status = 200, body = LogoutResponse)))]
async fn logout(
    State(state): State<SharedState>,
    jar: CookieJar,
) -> ApiResult<(CookieJar, Json<LogoutResponse>)> {
    let mut redirect = None;
    if let Some(c) = jar.get(SESSION_COOKIE) {
        let hash = token_hash(c.value());
        if let Some((_, id_token)) = state.db.session_user(&hash).await?
            && let (Some(oidc), Some(id_token)) = (&state.oidc, id_token)
        {
            redirect = oidc.end_session_url(&id_token, &state.config.public_url);
        }
        state.db.delete_session(&hash).await?;
    }
    let jar = jar.remove(Cookie::build(SESSION_COOKIE).path("/").build());
    Ok((jar, Json(LogoutResponse { redirect })))
}

#[utoipa::path(get, path = "/auth/me", tag = "auth", responses((status = 200, body = Me), (status = 401)))]
async fn me(CurrentUser(user): CurrentUser) -> Json<Me> {
    Json(Me::from(&user))
}

pub fn router() -> OpenApiRouter<SharedState> {
    OpenApiRouter::new()
        .routes(routes!(auth_config))
        .routes(routes!(setup))
        .routes(routes!(login))
        .routes(routes!(logout))
        .routes(routes!(me))
        .merge(oidc::router())
}

/// Creates the admin user from `PP_ADMIN_USERNAME` / `PP_ADMIN_PASSWORD` if
/// no user exists.
pub async fn bootstrap_admin(state: &SharedState) -> anyhow::Result<()> {
    let (Some(u), Some(p)) = (&state.config.admin_username, &state.config.admin_password) else {
        return Ok(());
    };
    if state.db.count_users().await? > 0 {
        return Ok(());
    }
    let hash = hash_password(p)?;
    state
        .db
        .create_user(NewUser {
            username: u,
            email: None,
            display_name: None,
            password_hash: Some(&hash),
            is_admin: true,
            oidc_issuer: None,
            oidc_subject: None,
        })
        .await?;
    tracing::info!("admin user '{u}' created");
    Ok(())
}
