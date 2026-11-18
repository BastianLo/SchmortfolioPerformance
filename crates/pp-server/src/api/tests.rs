//! API integration tests (in-memory database, no network).

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::config::{Config, OidcConfig};
use crate::state::AppState;

fn test_config() -> Config {
    Config {
        bind: "127.0.0.1:0".into(),
        data_dir: std::env::temp_dir()
            .join(format!("pp-test-{}", uuid::Uuid::new_v4()))
            .to_string_lossy()
            .into(),
        database_url: "sqlite::memory:".into(),
        public_url: "http://localhost:8080".into(),
        session_days: 30,
        cookie_secure: false,
        admin_username: None,
        admin_password: None,
        disable_local_login: false,
        ecb_update: false,
        quote_update: false,
        max_upload_mb: 50,
        oidc: None,
    }
}

async fn app_with(
    config: Config,
    oidc: Option<crate::auth::oidc::Oidc>,
) -> (Router, Arc<AppState>) {
    let db = pp_db::Db::connect(&config.database_url).await.unwrap();
    let state = Arc::new(AppState::new(config, db, oidc).await.unwrap());
    (super::app(state.clone()), state)
}

struct Resp {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    json: Value,
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
    cookie: Option<&str>,
) -> Resp {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    let req = match body {
        Some(v) => b
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(v.to_string()))
            .unwrap(),
        None => b.body(Body::empty()).unwrap(),
    };
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    Resp {
        status,
        headers,
        json,
    }
}

fn session_cookie(r: &Resp) -> String {
    let c = r
        .headers
        .get(header::SET_COOKIE)
        .expect("set-cookie")
        .to_str()
        .unwrap();
    c.split(';').next().unwrap().to_string()
}

async fn setup_admin(app: &Router) -> String {
    let r = call(
        app,
        "POST",
        "/api/v1/auth/setup",
        Some(json!({"username": "admin", "password": "geheim123"})),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    session_cookie(&r)
}

async fn upload(app: &Router, cookie: &str, file: &str, password: Option<&str>) -> Resp {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/pp")
        .join(file);
    let bytes = std::fs::read(path).unwrap();
    let boundary = "XBOUNDARYX";
    let mut body = Vec::new();
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{file}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes());
    body.extend_from_slice(&bytes);
    body.extend_from_slice(b"\r\n");
    if let Some(p) = password {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"password\"\r\n\r\n{p}\r\n"
            )
            .as_bytes(),
        );
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/clients/import")
        .header(header::COOKIE, cookie)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    Resp {
        status,
        headers,
        json: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    }
}

#[tokio::test]
async fn setup_login_and_access_control() {
    let (app, _) = app_with(test_config(), None).await;
    let r = call(&app, "GET", "/api/v1/auth/config", None, None).await;
    assert_eq!(r.json["setup_required"], true);
    assert_eq!(
        call(&app, "GET", "/api/v1/clients", None, None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );

    let cookie = setup_admin(&app).await;
    // setup only once
    let r = call(
        &app,
        "POST",
        "/api/v1/auth/setup",
        Some(json!({"username": "x", "password": "geheim123"})),
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    let me = call(&app, "GET", "/api/v1/auth/me", None, Some(&cookie)).await;
    assert_eq!(me.json["username"], "admin");
    assert_eq!(me.json["is_admin"], true);

    let bad = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        Some(json!({"username": "admin", "password": "falsch"})),
        None,
    )
    .await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
    let ok = call(
        &app,
        "POST",
        "/api/v1/auth/login",
        Some(json!({"username": "Admin", "password": "geheim123"})),
        None,
    )
    .await;
    assert_eq!(ok.status, StatusCode::OK);
    let cookie2 = session_cookie(&ok);

    let out = call(&app, "POST", "/api/v1/auth/logout", None, Some(&cookie2)).await;
    assert_eq!(out.status, StatusCode::OK);
    assert_eq!(
        call(&app, "GET", "/api/v1/auth/me", None, Some(&cookie2))
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    // first session still valid
    assert_eq!(
        call(&app, "GET", "/api/v1/auth/me", None, Some(&cookie))
            .await
            .status,
        StatusCode::OK
    );
}

#[tokio::test]
async fn cross_origin_post_is_rejected() {
    let (app, _) = app_with(test_config(), None).await;
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header("origin", "https://evil.example")
        .header("host", "localhost:8080")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(r#"{"username":"a","password":"b"}"#))
        .unwrap();
    assert_eq!(
        app.oneshot(req).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test]
async fn import_and_reports() {
    let (app, _) = app_with(test_config(), None).await;
    let cookie = setup_admin(&app).await;

    let r = upload(
        &app,
        &cookie,
        "fileversions/client69.binary+pwd.portfolio",
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "password required");
    let r = upload(
        &app,
        &cookie,
        "fileversions/client69.binary+pwd.portfolio",
        Some("123456"),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    assert_eq!(r.json["encrypted"], true);

    let r = upload(&app, &cookie, "kommer.xml", None).await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    assert_eq!(r.json["counts"]["securities"], 15);
    let id = r.json["client"]["id"].as_str().unwrap().to_string();

    let list = call(&app, "GET", "/api/v1/clients", None, Some(&cookie)).await;
    assert_eq!(list.json.as_array().unwrap().len(), 2);

    for path in [
        format!("/api/v1/clients/{id}"),
        format!("/api/v1/clients/{id}/securities"),
        format!("/api/v1/clients/{id}/accounts"),
        format!("/api/v1/clients/{id}/portfolios"),
        format!("/api/v1/clients/{id}/transactions?limit=10"),
        format!("/api/v1/clients/{id}/reports/dashboard"),
        format!("/api/v1/clients/{id}/reports/holdings"),
        format!("/api/v1/clients/{id}/reports/performance?interval=month"),
        format!("/api/v1/clients/{id}/reports/earnings"),
        format!("/api/v1/clients/{id}/taxonomies"),
    ] {
        let r = call(&app, "GET", &path, None, Some(&cookie)).await;
        assert_eq!(r.status, StatusCode::OK, "{path}: {:?}", r.json);
    }
    let tax = call(
        &app,
        "GET",
        &format!("/api/v1/clients/{id}/taxonomies"),
        None,
        Some(&cookie),
    )
    .await;
    let tid = tax.json[0]["id"].as_str().unwrap();
    let r = call(
        &app,
        "GET",
        &format!("/api/v1/clients/{id}/reports/taxonomies/{tid}"),
        None,
        Some(&cookie),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(!r.json["nodes"].as_array().unwrap().is_empty());

    // an invalid session must not see the client
    let other = call(
        &app,
        "GET",
        &format!("/api/v1/clients/{id}"),
        None,
        Some("pp_session=invalid"),
    )
    .await;
    assert_eq!(other.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn demo_client_and_transactions() {
    let (app, _) = app_with(test_config(), None).await;
    let cookie = setup_admin(&app).await;
    let r = call(&app, "POST", "/api/v1/clients/demo", None, Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::CREATED);
    let id = r.json["id"].as_str().unwrap().to_string();

    let dash = call(
        &app,
        "GET",
        &format!("/api/v1/clients/{id}/reports/dashboard"),
        None,
        Some(&cookie),
    )
    .await;
    assert_eq!(dash.status, StatusCode::OK);
    assert!(
        dash.json["total"].as_i64().unwrap() > 100_000_00,
        "{}",
        dash.json["total"]
    );
    assert!(dash.json["series"].as_array().unwrap().len() > 100);
    assert!(dash.json["kpis"]["irr"].as_f64().is_some());
    assert_eq!(dash.json["allocation"].as_array().unwrap().len(), 3);

    let accounts = call(
        &app,
        "GET",
        &format!("/api/v1/clients/{id}/accounts"),
        None,
        Some(&cookie),
    )
    .await;
    let acc = accounts.json[0]["id"].as_str().unwrap().to_string();
    let before = accounts.json[0]["balance"].as_i64().unwrap();

    let r = call(
        &app,
        "POST",
        &format!("/api/v1/clients/{id}/transactions"),
        Some(json!({"kind": "DEPOSIT", "account_id": acc, "date_time": "2024-05-02T00:00:00", "amount": 100000})),
        Some(&cookie),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    let tx = r.json.as_str().unwrap().to_string();
    let accounts = call(
        &app,
        "GET",
        &format!("/api/v1/clients/{id}/accounts"),
        None,
        Some(&cookie),
    )
    .await;
    assert_eq!(
        accounts.json[0]["balance"].as_i64().unwrap(),
        before + 100000
    );

    // buy creates a pair, delete removes both
    let r = call(
        &app,
        "POST",
        &format!("/api/v1/clients/{id}/transactions"),
        Some(json!({"kind": "BUY", "account_id": acc, "portfolio_id": "demo-pf", "security_id": "demo-sec-0",
                    "date_time": "2024-05-03T00:00:00", "amount": 50150, "shares": 500000000, "fees": 150})),
        Some(&cookie),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    let buy = r.json.as_str().unwrap().to_string();
    let txs = call(
        &app,
        "GET",
        &format!("/api/v1/clients/{id}/transactions?types=BUY&from=2024-05-03&to=2024-05-03"),
        None,
        Some(&cookie),
    )
    .await;
    assert_eq!(txs.json["total"], 2, "{:?}", txs.json);
    assert_eq!(
        call(
            &app,
            "DELETE",
            &format!("/api/v1/clients/{id}/transactions/{buy}"),
            None,
            Some(&cookie)
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
    let txs = call(
        &app,
        "GET",
        &format!("/api/v1/clients/{id}/transactions?types=BUY&from=2024-05-03&to=2024-05-03"),
        None,
        Some(&cookie),
    )
    .await;
    assert_eq!(txs.json["total"], 0);
    assert_eq!(
        call(
            &app,
            "DELETE",
            &format!("/api/v1/clients/{id}/transactions/{tx}"),
            None,
            Some(&cookie)
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );

    // forex buy of the USD security needs an exchange rate
    let body = json!({"kind": "BUY", "account_id": acc, "portfolio_id": "demo-pf", "security_id": "demo-sec-4",
                      "date_time": "2024-05-06T00:00:00", "amount": 100000, "shares": 600000000, "fees": 500});
    let r = call(
        &app,
        "POST",
        &format!("/api/v1/clients/{id}/transactions"),
        Some(body.clone()),
        Some(&cookie),
    )
    .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let mut body = body;
    body["exchange_rate"] = json!("0.92");
    let r = call(
        &app,
        "POST",
        &format!("/api/v1/clients/{id}/transactions"),
        Some(body),
        Some(&cookie),
    )
    .await;
    assert_eq!(r.status, StatusCode::CREATED, "{:?}", r.json);
    let fx = r.json.as_str().unwrap().to_string();
    // editing via the account side returns the portfolio side booking
    let txs = call(&app, "GET", &format!("/api/v1/clients/{id}/transactions?types=BUY&owner_kind=account&from=2024-05-06&to=2024-05-06"), None, Some(&cookie)).await;
    let account_side = txs.json["items"][0]["id"].as_str().unwrap().to_string();
    let b = call(
        &app,
        "GET",
        &format!("/api/v1/clients/{id}/transactions/{account_side}"),
        None,
        Some(&cookie),
    )
    .await;
    assert_eq!(b.json["id"], fx);
    assert_eq!(b.json["booking"]["exchange_rate"], "0.92");
    let mut booking = b.json["booking"].clone();
    booking["amount"] = json!(200000);
    let r = call(
        &app,
        "PUT",
        &format!("/api/v1/clients/{id}/transactions/{account_side}"),
        Some(booking),
        Some(&cookie),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let txs = call(
        &app,
        "GET",
        &format!("/api/v1/clients/{id}/transactions?types=BUY&from=2024-05-06&to=2024-05-06"),
        None,
        Some(&cookie),
    )
    .await;
    assert_eq!(txs.json["total"], 2);
    assert!(
        txs.json["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|t| t["amount"] == 200000)
    );
    assert_eq!(
        call(
            &app,
            "DELETE",
            &format!("/api/v1/clients/{id}/transactions/{fx}"),
            None,
            Some(&cookie)
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );

    // master data
    let r = call(&app, "PUT", &format!("/api/v1/clients/{id}/securities/demo-sec-0"),
        Some(json!({"name": "Welt ETF", "isin": "zzdemo000011", "retired": false, "feed": "YAHOO", "ticker": "WLD.DE"})), Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let sec = call(
        &app,
        "GET",
        &format!("/api/v1/clients/{id}/securities/demo-sec-0"),
        None,
        Some(&cookie),
    )
    .await;
    assert_eq!(sec.json["security"]["name"], "Welt ETF");
    assert_eq!(sec.json["security"]["isin"], "ZZDEMO000011");
    assert!(
        sec.json["security"]["prices"].as_u64().unwrap() > 1000,
        "prices kept"
    );
    let r = call(
        &app,
        "PUT",
        &format!("/api/v1/clients/{id}/accounts/{acc}"),
        Some(json!({"name": "Giro", "currency": "USD"})),
        Some(&cookie),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);
    let r = call(
        &app,
        "PUT",
        &format!("/api/v1/clients/{id}/accounts/{acc}"),
        Some(json!({"name": "Giro"})),
        Some(&cookie),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let accounts = call(
        &app,
        "GET",
        &format!("/api/v1/clients/{id}/accounts"),
        None,
        Some(&cookie),
    )
    .await;
    assert_eq!(accounts.json[0]["name"], "Giro");
    assert!(
        accounts.json[0]["transactions"].as_u64().unwrap() > 50,
        "transactions kept"
    );

    // deleting an account with transactions is refused
    let r = call(
        &app,
        "DELETE",
        &format!("/api/v1/clients/{id}/accounts/{acc}"),
        None,
        Some(&cookie),
    )
    .await;
    assert_eq!(r.status, StatusCode::CONFLICT);

    assert_eq!(
        call(
            &app,
            "DELETE",
            &format!("/api/v1/clients/{id}"),
            None,
            Some(&cookie)
        )
        .await
        .status,
        StatusCode::NO_CONTENT
    );
}

#[tokio::test]
async fn openapi_document_is_served() {
    let (app, _) = app_with(test_config(), None).await;
    let r = call(&app, "GET", "/api/v1/openapi.json", None, None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert!(r.json["paths"]["/clients/{id}/reports/dashboard"].is_object());
}

// ------------------------------------------------------------------- OIDC

mod mock_idp {
    use std::sync::{Arc, Mutex};

    use axum::extract::State;
    use axum::routing::{get, post};
    use axum::{Form, Json, Router};
    use openidconnect::core::{
        CoreIdToken, CoreIdTokenClaims, CoreJsonWebKeySet, CoreJwsSigningAlgorithm,
        CoreRsaPrivateSigningKey,
    };
    use openidconnect::{
        AccessToken, Audience, EmptyAdditionalClaims, EndUserEmail, EndUserUsername, IssuerUrl,
        JsonWebKeyId, Nonce, PrivateSigningKey, StandardClaims, SubjectIdentifier,
    };
    use serde_json::json;

    #[derive(Default)]
    pub struct Shared {
        pub nonce: Option<String>,
        pub groups: Vec<String>,
    }

    #[derive(Clone)]
    struct Ctx {
        issuer: String,
        key_pem: String,
        shared: Arc<Mutex<Shared>>,
    }

    fn key(pem: &str) -> CoreRsaPrivateSigningKey {
        CoreRsaPrivateSigningKey::from_pem(pem, Some(JsonWebKeyId::new("k1".into()))).unwrap()
    }

    pub async fn start() -> (String, Arc<Mutex<Shared>>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let issuer = format!("http://{}", listener.local_addr().unwrap());
        let shared = Arc::new(Mutex::new(Shared::default()));
        let ctx = Ctx {
            issuer: issuer.clone(),
            key_pem: include_str!("../../tests/fixtures/oidc-test-key.pem").into(),
            shared: shared.clone(),
        };
        let app = Router::new()
            .route(
                "/.well-known/openid-configuration",
                get(|State(c): State<Ctx>| async move {
                    Json(json!({
                        "issuer": c.issuer,
                        "authorization_endpoint": format!("{}/authorize", c.issuer),
                        "token_endpoint": format!("{}/token", c.issuer),
                        "userinfo_endpoint": format!("{}/userinfo", c.issuer),
                        "end_session_endpoint": format!("{}/logout", c.issuer),
                        "jwks_uri": format!("{}/jwks", c.issuer),
                        "response_types_supported": ["code"],
                        "subject_types_supported": ["public"],
                        "id_token_signing_alg_values_supported": ["RS256"]
                    }))
                }),
            )
            .route(
                "/jwks",
                get(|State(c): State<Ctx>| async move { Json(CoreJsonWebKeySet::new(vec![key(&c.key_pem).as_verification_key()])) }),
            )
            .route(
                "/token",
                post(|State(c): State<Ctx>, Form(form): Form<std::collections::HashMap<String, String>>| async move {
                    assert_eq!(form.get("code").map(String::as_str), Some("the-code"));
                    assert!(form.contains_key("code_verifier"), "PKCE verifier missing");
                    let nonce = c.shared.lock().unwrap().nonce.clone().unwrap();
                    let access = AccessToken::new("at-123".into());
                    let claims = CoreIdTokenClaims::new(
                        IssuerUrl::new(c.issuer.clone()).unwrap(),
                        vec![Audience::new("pp-test".into())],
                        chrono::Utc::now() + chrono::Duration::minutes(5),
                        chrono::Utc::now(),
                        StandardClaims::new(SubjectIdentifier::new("user-42".into()))
                            .set_email(Some(EndUserEmail::new("jane@example.org".into())))
                            .set_email_verified(Some(true))
                            .set_preferred_username(Some(EndUserUsername::new("jane".into()))),
                        EmptyAdditionalClaims {},
                    )
                    .set_nonce(Some(Nonce::new(nonce)));
                    let id_token = CoreIdToken::new(claims, &key(&c.key_pem), CoreJwsSigningAlgorithm::RsaSsaPkcs1V15Sha256, Some(&access), None).unwrap();
                    Json(json!({"access_token": "at-123", "token_type": "Bearer", "expires_in": 300, "id_token": id_token.to_string()}))
                }),
            )
            .route(
                "/userinfo",
                get(|State(c): State<Ctx>| async move {
                    let groups = c.shared.lock().unwrap().groups.clone();
                    Json(json!({"sub": "user-42", "groups": groups}))
                }),
            )
            .with_state(ctx);
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (issuer, shared)
    }
}

fn oidc_config(issuer: &str, allowed: Vec<String>, admin: Vec<String>) -> OidcConfig {
    OidcConfig {
        issuer: issuer.into(),
        client_id: "pp-test".into(),
        client_secret: Some("secret".into()),
        redirect_url: "http://localhost:8080/api/v1/auth/oidc/callback".into(),
        scopes: vec!["openid".into(), "profile".into(), "email".into()],
        button_label: "SSO".into(),
        auto_register: true,
        link_by_email: false,
        groups_claim: "groups".into(),
        allowed_groups: allowed,
        admin_groups: admin,
    }
}

async fn oidc_login(app: &Router, shared: &Arc<std::sync::Mutex<mock_idp::Shared>>) -> Resp {
    let r = call(
        app,
        "GET",
        "/api/v1/auth/oidc/login?return_to=/dashboard",
        None,
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    let location = r
        .headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let url = url::Url::parse(&location).unwrap();
    let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(q["code_challenge_method"], "S256");
    assert!(q["scope"].contains("email"));
    shared.lock().unwrap().nonce = Some(q["nonce"].clone());
    call(
        app,
        "GET",
        &format!(
            "/api/v1/auth/oidc/callback?code=the-code&state={}",
            q["state"]
        ),
        None,
        None,
    )
    .await
}

#[tokio::test]
async fn oidc_login_flow() {
    let (issuer, shared) = mock_idp::start().await;
    shared.lock().unwrap().groups = vec!["portfolio-users".into(), "portfolio-admins".into()];
    let cfg = oidc_config(
        &issuer,
        vec!["portfolio-users".into()],
        vec!["portfolio-admins".into()],
    );
    let oidc = crate::auth::oidc::Oidc::discover(cfg.clone())
        .await
        .expect("discovery");
    let mut config = test_config();
    config.oidc = Some(cfg);
    let (app, _) = app_with(config, Some(oidc)).await;

    let auth = call(&app, "GET", "/api/v1/auth/config", None, None).await;
    assert_eq!(auth.json["oidc"], true);

    let r = oidc_login(&app, &shared).await;
    assert_eq!(r.status, StatusCode::SEE_OTHER);
    assert_eq!(r.headers.get(header::LOCATION).unwrap(), "/dashboard");
    let cookie = session_cookie(&r);
    let me = call(&app, "GET", "/api/v1/auth/me", None, Some(&cookie)).await;
    assert_eq!(me.json["username"], "jane");
    assert_eq!(me.json["email"], "jane@example.org");
    assert_eq!(me.json["is_admin"], true);
    assert_eq!(me.json["sso"], true);

    // second login maps to the same user
    let r = oidc_login(&app, &shared).await;
    let me2 = call(
        &app,
        "GET",
        "/api/v1/auth/me",
        None,
        Some(&session_cookie(&r)),
    )
    .await;
    assert_eq!(me2.json["id"], me.json["id"]);

    // logout returns the end session url of the provider
    let out = call(&app, "POST", "/api/v1/auth/logout", None, Some(&cookie)).await;
    assert!(
        out.json["redirect"]
            .as_str()
            .unwrap()
            .starts_with(&format!("{issuer}/logout?id_token_hint="))
    );

    // replayed state is rejected
    let r = call(
        &app,
        "GET",
        "/api/v1/auth/oidc/callback?code=the-code&state=unknown",
        None,
        None,
    )
    .await;
    assert_eq!(
        r.headers.get(header::LOCATION).unwrap(),
        "/login?error=sso_expired"
    );
}

#[tokio::test]
async fn oidc_group_restriction() {
    let (issuer, shared) = mock_idp::start().await;
    shared.lock().unwrap().groups = vec!["other".into()];
    let cfg = oidc_config(&issuer, vec!["portfolio-users".into()], vec![]);
    let oidc = crate::auth::oidc::Oidc::discover(cfg.clone())
        .await
        .unwrap();
    let mut config = test_config();
    config.oidc = Some(cfg);
    let (app, _) = app_with(config, Some(oidc)).await;
    let r = oidc_login(&app, &shared).await;
    assert_eq!(
        r.headers.get(header::LOCATION).unwrap(),
        "/login?error=sso_forbidden"
    );
    assert!(r.headers.get(header::SET_COOKIE).is_none());
}

#[tokio::test]
async fn quote_update_from_yahoo_mock() {
    use axum::routing::get;
    let today = crate::api::today();
    let ts = today.and_hms_opt(0, 0, 0).unwrap().and_utc().timestamp();
    let body = format!(
        r#"{{"chart":{{"result":[{{"meta":{{"regularMarketPrice":77.7,"regularMarketTime":{ts}}},"timestamp":[{ts}],"indicators":{{"quote":[{{"close":[77.5]}}]}}}}]}}}}"#
    );
    let mock = Router::new().route("/v8/finance/chart/{t}", get(move || async move { body }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, mock).await.unwrap() });
    // SAFETY: tests in this module do not read this variable concurrently
    unsafe { std::env::set_var("PP_YAHOO_URL", &base) };

    let (app, _) = app_with(test_config(), None).await;
    let cookie = setup_admin(&app).await;
    let id = call(&app, "POST", "/api/v1/clients/demo", None, Some(&cookie))
        .await
        .json["id"]
        .as_str()
        .unwrap()
        .to_string();
    let r = call(
        &app,
        "PUT",
        &format!("/api/v1/clients/{id}/securities/demo-sec-1"),
        Some(json!({"name": "Europa Dividenden ETF", "feed": "YAHOO", "ticker": "EUDIV.DE"})),
        Some(&cookie),
    )
    .await;
    assert_eq!(r.status, StatusCode::NO_CONTENT);
    let r = call(
        &app,
        "POST",
        &format!("/api/v1/clients/{id}/quotes/update"),
        Some(json!({})),
        Some(&cookie),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    let items = r.json.as_array().unwrap();
    assert_eq!(
        items.len(),
        1,
        "only securities with a supported feed: {items:?}"
    );
    assert_eq!(items[0]["ok"], true, "{items:?}");
    let sec = call(
        &app,
        "GET",
        &format!("/api/v1/clients/{id}/securities/demo-sec-1"),
        None,
        Some(&cookie),
    )
    .await;
    assert_eq!(sec.json["security"]["latest_price"], 7_770_000_000i64);
    assert_eq!(sec.json["security"]["feed_status"]["ok"], true);
}

#[tokio::test]
async fn pdf_import_preview_commit_and_attachment() {
    let (app, _) = app_with(test_config(), None).await;
    let cookie = setup_admin(&app).await;
    let id = call(&app, "POST", "/api/v1/clients/demo", None, Some(&cookie))
        .await
        .json["id"]
        .as_str()
        .unwrap()
        .to_string();

    // multipart with a real PDF and a text document
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../testdata/pdf");
    let files = [
        ("generated/traderepublic_Kauf01.pdf", "Kauf.pdf"),
        ("traderepublic/Dividende02.txt", "Dividende.txt"),
    ];
    let boundary = "XBOUNDARYX";
    let mut body = Vec::new();
    for (f, name) in files {
        body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"files\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes());
        body.extend_from_slice(&std::fs::read(root.join(f)).unwrap());
        body.extend_from_slice(b"\r\n");
    }
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/clients/{id}/imports/pdf/preview"))
        .header(header::COOKIE, &cookie)
        .header(
            header::CONTENT_TYPE,
            format!("multipart/form-data; boundary={boundary}"),
        )
        .body(Body::from(body))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let preview: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let items = preview["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["bank"], "Trade Republic");
    assert_eq!(items[0]["transactions"][0]["kind"], "BUY");
    assert!(
        items[0]["transactions"][0]["security_id"].is_null(),
        "new security"
    );
    assert_eq!(items[1]["transactions"][0]["kind"], "DIVIDENDS");

    let commit_items: Vec<Value> = items
        .iter()
        .map(|i| json!({"tx": i["transactions"][0], "file_name": i["file_name"], "file_hash": i["file_hash"]}))
        .collect();
    let r = call(&app, "POST", &format!("/api/v1/clients/{id}/imports/commit"),
        Some(json!({"account_id": "demo-acc-main", "portfolio_id": "demo-pf", "items": commit_items})), Some(&cookie)).await;
    assert_eq!(r.status, StatusCode::OK, "{:?}", r.json);
    assert_eq!(r.json["created"], 2, "{:?}", r.json);
    assert_eq!(r.json["securities_created"], 2);

    let txs = call(
        &app,
        "GET",
        &format!(
            "/api/v1/clients/{id}/transactions?from=2019-05-13&to=2019-05-13&owner_kind=portfolio"
        ),
        None,
        Some(&cookie),
    )
    .await;
    let t = &txs.json["items"][0];
    assert_eq!(t["amount"], 2505);
    assert_eq!(t["fees"], 100);
    let hash = t["attachments"][0]["file_hash"]
        .as_str()
        .expect("attachment")
        .to_string();
    let req = Request::builder()
        .uri(format!("/api/v1/clients/{id}/files/{hash}"))
        .header(header::COOKIE, &cookie)
        .body(Body::empty())
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(
        res.into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .starts_with(b"%PDF")
    );

    // the dividend in USD got an exchange rate (security currency USD)
    let div = call(
        &app,
        "GET",
        &format!("/api/v1/clients/{id}/transactions?types=DIVIDENDS&from=2019-09-12&to=2019-09-12"),
        None,
        Some(&cookie),
    )
    .await;
    assert_eq!(div.json["items"][0]["amount"], 310);
    assert_eq!(div.json["items"][0]["taxes"], 106);

    // importing again is detected as duplicate
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/clients/{id}/imports/pdf/preview"))
        .header(header::COOKIE, &cookie)
        .header(header::CONTENT_TYPE, format!("multipart/form-data; boundary={boundary}"))
        .body(Body::from({
            let mut b = Vec::new();
            b.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"files\"; filename=\"x.pdf\"\r\n\r\n").as_bytes());
            b.extend_from_slice(&std::fs::read(root.join(files[0].0)).unwrap());
            b.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
            b
        }))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let again: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(again["items"][0]["transactions"][0]["duplicate_of"].is_string());
    assert!(again["items"][0]["transactions"][0]["security_id"].is_string());
}
