//! Import of bank documents (PDF) with preview, duplicate detection and
//! attachments.

use std::collections::HashMap;
use std::path::PathBuf;

use axum::Json;
use axum::body::Body;
use axum::extract::{Multipart, Path, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use chrono::NaiveDateTime;
use pp_core::booking::{Booking, BookingKind};
use pp_core::model::{AccountTransactionType, Client, PortfolioTransactionType, Security};
use pp_import::pdf::{self, ExtractedTx};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::state::{LoadedClient, SharedState};

fn files_dir(state: &SharedState) -> PathBuf {
    PathBuf::from(&state.config.data_dir).join("files")
}

fn rules(state: &SharedState) -> (Vec<pdf::BankRules>, Vec<String>) {
    let mut r = pdf::builtin_rules();
    let (custom, errors) =
        pdf::load_rules_dir(&PathBuf::from(&state.config.data_dir).join("pdf-rules"));
    // custom rules first, so they can override builtin ones
    let mut all = custom;
    all.append(&mut r);
    (all, errors)
}

#[derive(Serialize, Deserialize, ToSchema, Clone)]
pub struct PreviewTx {
    /// BUY, SELL, DIVIDENDS, …
    pub kind: String,
    pub date_time: NaiveDateTime,
    pub name: Option<String>,
    pub isin: Option<String>,
    pub wkn: Option<String>,
    pub shares: i64,
    pub amount: i64,
    pub currency: String,
    pub fees: i64,
    pub taxes: i64,
    pub security_currency: Option<String>,
    /// security currency → account currency
    pub exchange_rate: Option<String>,
    /// matched existing security (otherwise a new one is created)
    pub security_id: Option<String>,
    /// an existing transaction that looks identical
    pub duplicate_of: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct PdfPreviewItem {
    pub file_name: String,
    pub file_hash: String,
    pub bank: Option<String>,
    pub document: Option<String>,
    pub errors: Vec<String>,
    pub transactions: Vec<PreviewTx>,
    /// extracted text if the document could not be read (for writing rules)
    pub text: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct PdfPreview {
    pub items: Vec<PdfPreviewItem>,
    pub rule_errors: Vec<String>,
}

fn find_security<'a>(
    c: &'a Client,
    isin: Option<&str>,
    wkn: Option<&str>,
    name: Option<&str>,
) -> Option<&'a Security> {
    if let Some(i) = isin
        && let Some(s) = c
            .securities
            .iter()
            .find(|s| s.isin.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(i)))
    {
        return Some(s);
    }
    if let Some(w) = wkn
        && let Some(s) = c
            .securities
            .iter()
            .find(|s| s.wkn.as_deref().is_some_and(|x| x.eq_ignore_ascii_case(w)))
    {
        return Some(s);
    }
    let n = name?.to_lowercase();
    c.securities.iter().find(|s| s.name.to_lowercase() == n)
}

/// Finds an existing transaction with the same date, kind, security and amount.
fn find_duplicate(c: &Client, t: &ExtractedTx, security: Option<&str>) -> Option<String> {
    let date = t.date_time.date();
    let p_kind = match t.kind.as_str() {
        "BUY" => Some(PortfolioTransactionType::Buy),
        "SELL" => Some(PortfolioTransactionType::Sell),
        _ => None,
    };
    if let Some(k) = p_kind {
        for p in &c.portfolios {
            if let Some(x) = p.transactions.iter().find(|x| {
                x.kind == k
                    && x.date() == date
                    && x.security.as_deref() == security
                    && (x.amount == t.amount || x.shares == t.shares)
            }) {
                return Some(x.id.clone());
            }
        }
        return None;
    }
    let a_kind = AccountTransactionType::parse(&t.kind)?;
    c.accounts.iter().find_map(|a| {
        a.transactions
            .iter()
            .find(|x| {
                x.kind == a_kind
                    && x.date() == date
                    && x.amount == t.amount
                    && (security.is_none() || x.security.as_deref() == security)
            })
            .map(|x| x.id.clone())
    })
}

fn preview_tx(c: &Client, t: &ExtractedTx) -> PreviewTx {
    let sec = find_security(c, t.isin.as_deref(), t.wkn.as_deref(), t.name.as_deref());
    PreviewTx {
        kind: t.kind.clone(),
        date_time: t.date_time,
        name: t.name.clone(),
        isin: t.isin.clone(),
        wkn: t.wkn.clone(),
        shares: t.shares,
        amount: t.amount,
        currency: t.currency.clone(),
        fees: t.fees,
        taxes: t.taxes,
        security_currency: t.security_currency.clone(),
        exchange_rate: t.exchange_rate.map(|r| r.to_string()),
        security_id: sec.map(|s| s.id.clone()),
        duplicate_of: find_duplicate(c, t, sec.map(|s| s.id.as_str())),
    }
}

/// Reads PDF (or text) documents and returns the recognised transactions.
#[utoipa::path(post, path = "/clients/{id}/imports/pdf/preview", tag = "clients", params(("id" = String, Path)),
    request_body(content_type = "multipart/form-data", description = "one or more `files` (PDF or extracted text)"),
    responses((status = 200, body = PdfPreview)))]
async fn preview(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> ApiResult<Json<PdfPreview>> {
    let c = state.client(&user, &id).await?;
    let (rules, rule_errors) = rules(&state);
    let dir = files_dir(&state);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let mut items = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
    {
        let file_name = field.file_name().unwrap_or("dokument.pdf").to_string();
        let bytes = field
            .bytes()
            .await
            .map_err(|e| ApiError::BadRequest(e.to_string()))?;
        let hash: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let is_pdf = bytes.starts_with(b"%PDF");
        if is_pdf {
            let path = dir.join(&hash);
            if !path.exists() {
                tokio::fs::write(&path, &bytes)
                    .await
                    .map_err(|e| ApiError::Internal(e.to_string()))?;
            }
        }
        let text = if is_pdf {
            let b = bytes.to_vec();
            tokio::task::spawn_blocking(move || pdf::pdf_text(&b))
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?
        } else {
            Ok(String::from_utf8_lossy(&bytes).to_string())
        };
        let item = match text {
            Err(e) => PdfPreviewItem {
                file_name,
                file_hash: hash,
                bank: None,
                document: None,
                errors: vec![e],
                transactions: vec![],
                text: None,
            },
            Ok(text) => {
                let r = pdf::extract(&text, &rules);
                let failed = r.transactions.is_empty();
                PdfPreviewItem {
                    file_name,
                    file_hash: if is_pdf { hash } else { String::new() },
                    bank: r.bank,
                    document: r.document,
                    errors: r.errors,
                    transactions: r
                        .transactions
                        .iter()
                        .map(|t| preview_tx(&c.client, t))
                        .collect(),
                    text: failed.then(|| text.chars().take(20_000).collect()),
                }
            }
        };
        items.push(item);
    }
    Ok(Json(PdfPreview { items, rule_errors }))
}

#[derive(Deserialize, ToSchema)]
pub struct MatchItem {
    pub kind: String,
    pub date_time: NaiveDateTime,
    pub name: Option<String>,
    pub isin: Option<String>,
    pub wkn: Option<String>,
    #[serde(default)]
    pub shares: i64,
    pub amount: i64,
    pub currency: String,
    #[serde(default)]
    pub fees: i64,
    #[serde(default)]
    pub taxes: i64,
    pub security_currency: Option<String>,
    pub exchange_rate: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct MatchRequest {
    pub items: Vec<MatchItem>,
}

/// Matches transactions (e.g. from a CSV file) with existing securities and
/// detects duplicates.
#[utoipa::path(post, path = "/clients/{id}/imports/match", tag = "clients", params(("id" = String, Path)), request_body = MatchRequest, responses((status = 200, body = Vec<PreviewTx>)))]
async fn match_items(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<MatchRequest>,
) -> ApiResult<Json<Vec<PreviewTx>>> {
    let c = state.client(&user, &id).await?;
    Ok(Json(
        req.items
            .into_iter()
            .map(|m| {
                let t = ExtractedTx {
                    kind: m.kind,
                    date_time: m.date_time,
                    name: m.name.filter(|x| !x.trim().is_empty()),
                    isin: m.isin.filter(|x| !x.trim().is_empty()),
                    wkn: m.wkn.filter(|x| !x.trim().is_empty()),
                    shares: m.shares,
                    amount: m.amount,
                    currency: m.currency,
                    fees: m.fees,
                    taxes: m.taxes,
                    security_currency: m.security_currency,
                    exchange_rate: m.exchange_rate.and_then(|r| r.parse().ok()),
                };
                preview_tx(&c.client, &t)
            })
            .collect(),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct CommitItem {
    pub tx: PreviewTx,
    pub file_name: Option<String>,
    /// hash of the uploaded PDF (attached to the transaction)
    pub file_hash: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct CommitRequest {
    pub account_id: String,
    pub portfolio_id: Option<String>,
    pub items: Vec<CommitItem>,
}

#[derive(Serialize, ToSchema)]
pub struct CommitResult {
    pub created: usize,
    pub securities_created: usize,
    pub errors: Vec<String>,
}

fn booking_for(
    c: &LoadedClient,
    req: &CommitRequest,
    t: &PreviewTx,
    security: Option<&Security>,
) -> Result<Booking, String> {
    let account = c.client.account(&req.account_id).ok_or("Konto unbekannt")?;
    if account.currency != t.currency {
        return Err(format!(
            "Beleg in {}, Konto in {}",
            t.currency, account.currency
        ));
    }
    let kind = match t.kind.as_str() {
        "BUY" => BookingKind::Buy,
        "SELL" => BookingKind::Sell,
        "DIVIDENDS" => BookingKind::Dividends,
        "INTEREST" => BookingKind::Interest,
        "FEES" => BookingKind::Fees,
        "TAXES" => BookingKind::Taxes,
        "TAX_REFUND" => BookingKind::TaxRefund,
        "DEPOSIT" => BookingKind::Deposit,
        "REMOVAL" => BookingKind::Removal,
        k => return Err(format!("Buchungsart {k} nicht unterstützt")),
    };
    let needs_security = matches!(
        kind,
        BookingKind::Buy | BookingKind::Sell | BookingKind::Dividends
    );
    if needs_security && security.is_none() {
        return Err("Wertpapier nicht gefunden".into());
    }
    let sec_cur = security
        .map(|s| s.currency_or(&account.currency).to_string())
        .unwrap_or_else(|| account.currency.clone());
    let mut rate = t
        .exchange_rate
        .as_deref()
        .and_then(|r| r.parse::<Decimal>().ok());
    if sec_cur != account.currency
        && (rate.is_none() || t.security_currency.as_deref() != Some(sec_cur.as_str()))
    {
        // use the exchange rate of the day
        rate = c
            .provider
            .rate(t.date_time.date(), &sec_cur, &account.currency);
    }
    let (fees, taxes) = if t.taxes < 0 {
        (t.fees, 0)
    } else {
        (t.fees, t.taxes)
    };
    let trade = matches!(kind, BookingKind::Buy | BookingKind::Sell);
    Ok(Booking {
        kind,
        date_time: t.date_time,
        account_id: Some(account.id.clone()),
        portfolio_id: if trade {
            Some(req.portfolio_id.clone().ok_or("Depot fehlt")?)
        } else {
            None
        },
        target_account_id: None,
        target_portfolio_id: None,
        security_id: security
            .filter(|_| {
                needs_security
                    || matches!(
                        kind,
                        BookingKind::Fees | BookingKind::Taxes | BookingKind::TaxRefund
                    )
            })
            .map(|s| s.id.clone()),
        shares: t.shares,
        amount: t.amount,
        fees,
        taxes,
        exchange_rate: if sec_cur != account.currency {
            rate
        } else {
            None
        },
        target_amount: None,
        note: if t.taxes < 0 {
            Some(format!("Steuererstattung {:.2}", -t.taxes as f64 / 100.0))
        } else {
            None
        },
        source: None,
    })
}

/// Creates the transactions (and missing securities) of a preview.
#[utoipa::path(post, path = "/clients/{id}/imports/commit", tag = "clients", params(("id" = String, Path)), request_body = CommitRequest, responses((status = 200, body = CommitResult)))]
async fn commit(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<CommitRequest>,
) -> ApiResult<Json<CommitResult>> {
    let c = state.client(&user, &id).await?;
    c.client
        .account(&req.account_id)
        .ok_or_else(|| ApiError::BadRequest("Konto unbekannt".into()))?;

    // 1. create missing securities
    let mut created_secs = 0;
    let mut new_ids: HashMap<String, String> = HashMap::new();
    for it in &req.items {
        let t = &it.tx;
        if !matches!(t.kind.as_str(), "BUY" | "SELL" | "DIVIDENDS")
            || t.security_id.is_some()
            || find_security(
                &c.client,
                t.isin.as_deref(),
                t.wkn.as_deref(),
                t.name.as_deref(),
            )
            .is_some()
        {
            continue;
        }
        let key = t
            .isin
            .clone()
            .or(t.wkn.clone())
            .or(t.name.clone())
            .unwrap_or_default();
        if key.is_empty() || new_ids.contains_key(&key) {
            continue;
        }
        let s = Security {
            id: uuid::Uuid::new_v4().to_string(),
            name: t.name.clone().unwrap_or_else(|| key.clone()),
            isin: t.isin.clone(),
            wkn: t.wkn.clone(),
            currency: Some(
                t.security_currency
                    .clone()
                    .unwrap_or_else(|| t.currency.clone()),
            ),
            feed: Some("YAHOO".into()),
            note: Some("Beim PDF-Import angelegt".into()),
            updated_at: Some(chrono::Utc::now().to_rfc3339()),
            ..Default::default()
        };
        new_ids.insert(key, s.id.clone());
        state.db.save_security(&id, &s).await?;
        created_secs += 1;
    }
    let c = state.client(&user, &id).await?;

    // 2. bookings
    let mut errors = Vec::new();
    let mut txs = Vec::new();
    let mut attach: Vec<(String, &CommitItem)> = Vec::new();
    let now = Some(chrono::Utc::now().to_rfc3339());
    for it in &req.items {
        let t = &it.tx;
        let label = format!(
            "{} {}",
            t.date_time.date(),
            t.name.as_deref().unwrap_or_default()
        );
        let sec = t
            .security_id
            .as_deref()
            .and_then(|s| c.client.security(s))
            .or_else(|| {
                find_security(
                    &c.client,
                    t.isin.as_deref(),
                    t.wkn.as_deref(),
                    t.name.as_deref(),
                )
            });
        let built = booking_for(&c, &req, t, sec).and_then(|mut b| {
            b.source = it.file_name.clone();
            b.build(&c.client, None, now.clone())
        });
        match built {
            Ok(mut v) => {
                attach.push((v[0].id().to_string(), it));
                txs.append(&mut v);
            }
            Err(e) => errors.push(format!("{label}: {e}")),
        }
    }
    let created = attach.len();
    if !txs.is_empty() {
        state.db.replace_transactions(&id, &[], &txs).await?;
    }
    for (tx_id, it) in attach {
        if let Some(hash) = it
            .file_hash
            .as_deref()
            .filter(|h| !h.is_empty() && h.chars().all(|c| c.is_ascii_hexdigit()))
        {
            let path = files_dir(&state).join(hash);
            let size = tokio::fs::metadata(&path)
                .await
                .map(|m| m.len() as i64)
                .unwrap_or(0);
            if size > 0 {
                state
                    .db
                    .add_attachment(
                        &id,
                        &tx_id,
                        &pp_db::Attachment {
                            file_hash: hash.to_string(),
                            file_name: it.file_name.clone().unwrap_or_else(|| "beleg.pdf".into()),
                            content_type: "application/pdf".into(),
                            size,
                        },
                    )
                    .await?;
            }
        }
    }
    Ok(Json(CommitResult {
        created,
        securities_created: created_secs,
        errors,
    }))
}

/// Downloads an attached document.
#[utoipa::path(get, path = "/clients/{id}/files/{hash}", tag = "clients", params(("id" = String, Path), ("hash" = String, Path)), responses((status = 200, content_type = "application/pdf")))]
async fn download(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, hash)): Path<(String, String)>,
) -> ApiResult<Response> {
    state.client(&user, &id).await?;
    let a = state
        .db
        .attachment_by_hash(&id, &hash)
        .await?
        .ok_or(ApiError::NotFound)?;
    if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ApiError::NotFound);
    }
    let bytes = tokio::fs::read(files_dir(&state).join(&hash))
        .await
        .map_err(|_| ApiError::NotFound)?;
    let name = a.file_name.replace('"', "");
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, a.content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{name}\""),
        )
        .body(Body::from(bytes))
        .unwrap())
}

pub fn router() -> OpenApiRouter<SharedState> {
    OpenApiRouter::new()
        .routes(routes!(preview))
        .routes(routes!(match_items))
        .routes(routes!(commit))
        .routes(routes!(download))
}
