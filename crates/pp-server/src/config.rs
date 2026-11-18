//! Configuration from environment variables.

use std::env;

#[derive(Debug, Clone)]
pub struct OidcConfig {
    pub issuer: String,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub redirect_url: String,
    pub scopes: Vec<String>,
    pub button_label: String,
    pub auto_register: bool,
    pub link_by_email: bool,
    pub groups_claim: String,
    pub allowed_groups: Vec<String>,
    pub admin_groups: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub bind: String,
    pub data_dir: String,
    pub database_url: String,
    pub public_url: String,
    pub session_days: i64,
    pub cookie_secure: bool,
    pub admin_username: Option<String>,
    pub admin_password: Option<String>,
    pub disable_local_login: bool,
    pub ecb_update: bool,
    pub max_upload_mb: usize,
    pub oidc: Option<OidcConfig>,
}

fn var(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn flag(name: &str, default: bool) -> bool {
    match var(name).as_deref() {
        Some("1" | "true" | "yes" | "on") => true,
        Some("0" | "false" | "no" | "off") => false,
        _ => default,
    }
}

fn list(name: &str) -> Vec<String> {
    var(name)
        .map(|v| {
            v.split([',', ' '])
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

impl Config {
    pub fn from_env() -> Self {
        let data_dir = var("PP_DATA_DIR").unwrap_or_else(|| "./data".into());
        let public_url = var("PP_PUBLIC_URL").unwrap_or_else(|| "http://localhost:8080".into());
        let public_url = public_url.trim_end_matches('/').to_string();
        let oidc = var("PP_OIDC_ISSUER").map(|issuer| OidcConfig {
            issuer,
            client_id: var("PP_OIDC_CLIENT_ID").unwrap_or_default(),
            client_secret: var("PP_OIDC_CLIENT_SECRET"),
            redirect_url: var("PP_OIDC_REDIRECT_URL")
                .unwrap_or_else(|| format!("{public_url}/api/v1/auth/oidc/callback")),
            scopes: {
                let s = list("PP_OIDC_SCOPES");
                if s.is_empty() {
                    vec!["openid".into(), "profile".into(), "email".into()]
                } else {
                    s
                }
            },
            button_label: var("PP_OIDC_BUTTON_LABEL").unwrap_or_else(|| "Mit SSO anmelden".into()),
            auto_register: flag("PP_OIDC_AUTO_REGISTER", true),
            link_by_email: flag("PP_OIDC_LINK_BY_EMAIL", false),
            groups_claim: var("PP_OIDC_GROUPS_CLAIM").unwrap_or_else(|| "groups".into()),
            allowed_groups: list("PP_OIDC_ALLOWED_GROUPS"),
            admin_groups: list("PP_OIDC_ADMIN_GROUPS"),
        });
        Self {
            bind: var("PP_BIND").unwrap_or_else(|| "0.0.0.0:8080".into()),
            database_url: var("PP_DATABASE_URL")
                .unwrap_or_else(|| format!("sqlite://{data_dir}/schmortfolio.db")),
            data_dir,
            cookie_secure: flag("PP_COOKIE_SECURE", public_url.starts_with("https://")),
            public_url,
            session_days: var("PP_SESSION_DAYS")
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            admin_username: var("PP_ADMIN_USERNAME"),
            admin_password: var("PP_ADMIN_PASSWORD"),
            disable_local_login: flag("PP_DISABLE_LOCAL_LOGIN", false),
            ecb_update: flag("PP_ECB_UPDATE", true),
            max_upload_mb: var("PP_MAX_UPLOAD_MB")
                .and_then(|v| v.parse().ok())
                .unwrap_or(200),
            oidc,
        }
    }
}
