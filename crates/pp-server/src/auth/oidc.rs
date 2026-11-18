//! OpenID Connect login (authorization code flow with PKCE).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::cookie::CookieJar;
use base64::Engine;
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet, EndpointNotSet,
    EndpointSet, IssuerUrl, Nonce, OAuth2TokenResponse, PkceCodeChallenge, PkceCodeVerifier,
    RedirectUrl, Scope, TokenResponse,
};
use pp_db::NewUser;
use serde::Deserialize;
use tokio::sync::Mutex;
use utoipa::IntoParams;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::config::OidcConfig;
use crate::state::SharedState;

type OidcClient = CoreClient<
    EndpointSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointNotSet,
    EndpointMaybeSet,
    EndpointMaybeSet,
>;

struct Pending {
    nonce: Nonce,
    verifier: PkceCodeVerifier,
    return_to: String,
    created: Instant,
}

pub struct Oidc {
    client: OidcClient,
    http: openidconnect::reqwest::Client,
    pub config: OidcConfig,
    issuer: String,
    end_session_endpoint: Option<String>,
    userinfo_endpoint: Option<String>,
    pending: Mutex<HashMap<String, Pending>>,
}

const PENDING_TTL: Duration = Duration::from_secs(600);

impl Oidc {
    pub async fn discover(config: OidcConfig) -> anyhow::Result<Self> {
        let http = openidconnect::reqwest::ClientBuilder::new()
            .redirect(openidconnect::reqwest::redirect::Policy::none())
            .timeout(Duration::from_secs(20))
            .build()?;
        let issuer_url = IssuerUrl::new(config.issuer.clone())?;
        let metadata = CoreProviderMetadata::discover_async(issuer_url, &http).await?;

        // fields not covered by the core metadata
        let raw: serde_json::Value = http
            .get(format!(
                "{}/.well-known/openid-configuration",
                config.issuer.trim_end_matches('/')
            ))
            .send()
            .await?
            .bytes()
            .await
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let end_session_endpoint = raw
            .get("end_session_endpoint")
            .and_then(|v| v.as_str())
            .map(String::from);
        let userinfo_endpoint = raw
            .get("userinfo_endpoint")
            .and_then(|v| v.as_str())
            .map(String::from);

        let client = CoreClient::from_provider_metadata(
            metadata,
            ClientId::new(config.client_id.clone()),
            config.client_secret.clone().map(ClientSecret::new),
        )
        .set_redirect_uri(RedirectUrl::new(config.redirect_url.clone())?);

        Ok(Self {
            client,
            http,
            issuer: config.issuer.clone(),
            config,
            end_session_endpoint,
            userinfo_endpoint,
            pending: Mutex::new(HashMap::new()),
        })
    }

    pub fn end_session_url(&self, id_token: &str, public_url: &str) -> Option<String> {
        let ep = self.end_session_endpoint.as_ref()?;
        let mut url = url::Url::parse(ep).ok()?;
        url.query_pairs_mut()
            .append_pair("id_token_hint", id_token)
            .append_pair("post_logout_redirect_uri", &format!("{public_url}/login"));
        Some(url.to_string())
    }

    async fn begin(&self, return_to: String) -> String {
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        let mut req = self.client.authorize_url(
            CoreAuthenticationFlow::AuthorizationCode,
            CsrfToken::new_random,
            Nonce::new_random,
        );
        for s in self.config.scopes.iter().filter(|s| *s != "openid") {
            req = req.add_scope(Scope::new(s.clone()));
        }
        let (url, csrf, nonce) = req.set_pkce_challenge(challenge).url();
        let mut pending = self.pending.lock().await;
        pending.retain(|_, p| p.created.elapsed() < PENDING_TTL);
        pending.insert(
            csrf.secret().clone(),
            Pending {
                nonce,
                verifier,
                return_to,
                created: Instant::now(),
            },
        );
        url.to_string()
    }
}

#[derive(Deserialize, IntoParams)]
struct LoginQuery {
    /// relative path to return to after login
    return_to: Option<String>,
}

fn safe_return_to(r: Option<String>) -> String {
    match r {
        Some(r) if r.starts_with('/') && !r.starts_with("//") => r,
        _ => "/".into(),
    }
}

/// Starts the SSO login and redirects to the identity provider.
#[utoipa::path(get, path = "/auth/oidc/login", tag = "auth", params(LoginQuery), responses((status = 303)))]
async fn oidc_login(State(state): State<SharedState>, Query(q): Query<LoginQuery>) -> Response {
    let Some(oidc) = &state.oidc else {
        return Redirect::to("/login?error=sso_disabled").into_response();
    };
    Redirect::to(&oidc.begin(safe_return_to(q.return_to)).await).into_response()
}

#[derive(Deserialize, IntoParams)]
struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

fn fail(code: &str) -> Response {
    Redirect::to(&format!("/login?error={code}")).into_response()
}

fn decode_payload(jwt: &str) -> serde_json::Value {
    jwt.split('.')
        .nth(1)
        .and_then(|p| {
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(p.trim_end_matches('='))
                .ok()
        })
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn claim_list(v: &serde_json::Value, claim: &str) -> Option<Vec<String>> {
    // supports nested claims like "realm_access.roles"
    let mut cur = v;
    for part in claim.split('.') {
        cur = cur.get(part)?;
    }
    match cur {
        serde_json::Value::Array(a) => Some(
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect(),
        ),
        serde_json::Value::String(s) => Some(
            s.split([',', ' '])
                .filter(|x| !x.is_empty())
                .map(String::from)
                .collect(),
        ),
        _ => None,
    }
}

/// Redirect target of the identity provider.
#[utoipa::path(get, path = "/auth/oidc/callback", tag = "auth", params(CallbackQuery), responses((status = 303)))]
async fn oidc_callback(
    State(state): State<SharedState>,
    jar: CookieJar,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let Some(oidc) = &state.oidc else {
        return fail("sso_disabled");
    };
    if let Some(e) = q.error {
        tracing::warn!("OIDC provider returned error: {e}");
        return fail("sso_denied");
    }
    let (Some(code), Some(csrf)) = (q.code, q.state) else {
        return fail("sso_invalid");
    };
    let Some(pending) = oidc.pending.lock().await.remove(&csrf) else {
        return fail("sso_expired");
    };

    let exchange = match oidc.client.exchange_code(AuthorizationCode::new(code)) {
        Ok(e) => e,
        Err(e) => {
            tracing::error!("OIDC token endpoint missing: {e}");
            return fail("sso_config");
        }
    };
    let token = match exchange
        .set_pkce_verifier(pending.verifier)
        .request_async(&oidc.http)
        .await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("OIDC code exchange failed: {e:?}");
            return fail("sso_token");
        }
    };
    let Some(id_token) = token.id_token() else {
        return fail("sso_token");
    };
    let claims = match id_token.claims(&oidc.client.id_token_verifier(), &pending.nonce) {
        Ok(c) => c.clone(),
        Err(e) => {
            tracing::warn!("OIDC id token invalid: {e}");
            return fail("sso_token");
        }
    };
    let id_token_str = id_token.to_string();
    let mut raw = decode_payload(&id_token_str);

    let cfg = &oidc.config;
    let needs_groups = !cfg.allowed_groups.is_empty() || !cfg.admin_groups.is_empty();
    if needs_groups
        && claim_list(&raw, &cfg.groups_claim).is_none()
        && let Some(ep) = &oidc.userinfo_endpoint
        && let Ok(resp) = oidc
            .http
            .get(ep)
            .bearer_auth(token.access_token().secret())
            .send()
            .await
        && let Ok(bytes) = resp.bytes().await
        && let Ok(info) = serde_json::from_slice::<serde_json::Value>(&bytes)
    {
        // the subject of the userinfo response must match the id token
        if info.get("sub").and_then(|s| s.as_str()) == Some(claims.subject().as_str()) {
            raw = info;
        }
    }
    let groups = claim_list(&raw, &cfg.groups_claim).unwrap_or_default();
    if !cfg.allowed_groups.is_empty() && !groups.iter().any(|g| cfg.allowed_groups.contains(g)) {
        return fail("sso_forbidden");
    }
    let is_admin = if cfg.admin_groups.is_empty() {
        None
    } else {
        Some(groups.iter().any(|g| cfg.admin_groups.contains(g)))
    };

    let subject = claims.subject().as_str().to_string();
    let email = claims.email().map(|e| e.as_str().to_string());
    let email_verified = claims.email_verified().unwrap_or(false);
    let preferred = claims.preferred_username().map(|u| u.as_str().to_string());
    let name = claims
        .name()
        .and_then(|n| n.get(None))
        .map(|n| n.as_str().to_string());

    let db = &state.db;
    let user = match db.user_by_oidc(&oidc.issuer, &subject).await {
        Ok(Some(u)) => Some(u),
        Ok(None) => None,
        Err(e) => {
            tracing::error!("{e}");
            return fail("internal");
        }
    };
    let user = match user {
        Some(u) => u,
        None => {
            let linked = if cfg.link_by_email && email_verified {
                match &email {
                    Some(e) => db.user_by_email(e).await.ok().flatten(),
                    None => None,
                }
            } else {
                None
            };
            match linked {
                Some(u) => {
                    if db.link_oidc(&u.id, &oidc.issuer, &subject).await.is_err() {
                        return fail("internal");
                    }
                    u
                }
                None if cfg.auto_register => {
                    let base = preferred
                        .clone()
                        .or_else(|| email.clone())
                        .unwrap_or_else(|| subject.clone());
                    let mut username = base.clone();
                    let mut i = 2;
                    while let Ok(Some(_)) = db.user_by_username(&username).await {
                        username = format!("{base}-{i}");
                        i += 1;
                    }
                    let first_user = db.count_users().await.unwrap_or(1) == 0;
                    match db
                        .create_user(NewUser {
                            username: &username,
                            email: email.as_deref(),
                            display_name: name.as_deref(),
                            password_hash: None,
                            is_admin: is_admin.unwrap_or(first_user),
                            oidc_issuer: Some(&oidc.issuer),
                            oidc_subject: Some(&subject),
                        })
                        .await
                    {
                        Ok(u) => u,
                        Err(e) => {
                            tracing::error!("creating OIDC user failed: {e}");
                            return fail("internal");
                        }
                    }
                }
                None => return fail("sso_not_registered"),
            }
        }
    };
    let _ = db
        .update_user_profile(&user.id, email.as_deref(), name.as_deref(), is_admin)
        .await;

    match crate::auth::start_session(&state, jar, &user, Some(&id_token_str)).await {
        Ok(jar) => (jar, Redirect::to(&pending.return_to)).into_response(),
        Err(_) => fail("internal"),
    }
}

pub fn router() -> OpenApiRouter<SharedState> {
    OpenApiRouter::new()
        .routes(routes!(oidc_login))
        .routes(routes!(oidc_callback))
}
