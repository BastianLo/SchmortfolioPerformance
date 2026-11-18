//! Master data and transactions.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{NaiveDate, NaiveDateTime};
use pp_core::booking::Booking;
use pp_core::model::*;
use pp_core::security::SecurityPerformance;
use pp_core::snapshot::ClientSnapshot;
use serde::{Deserialize, Serialize};
use utoipa::{IntoParams, ToSchema};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::dto::today;
use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::state::{LoadedClient, SharedState};

// ------------------------------------------------------------- securities

#[derive(Serialize, ToSchema)]
pub struct SecurityDto {
    pub id: String,
    pub name: String,
    pub currency: Option<String>,
    pub isin: Option<String>,
    pub wkn: Option<String>,
    pub ticker: Option<String>,
    pub feed: Option<String>,
    pub note: Option<String>,
    pub retired: bool,
    pub prices: usize,
    pub first_price_date: Option<NaiveDate>,
    pub latest_price: Option<i64>,
    pub latest_price_date: Option<NaiveDate>,
    /// change of the latest price against the previous one
    pub latest_change: Option<f64>,
    /// shares held today across all portfolios
    pub shares: i64,
    pub market_value: i64,
    /// result of the last automatic quote update
    pub feed_status: Option<FeedStatusDto>,
}

#[derive(Serialize, ToSchema, Clone)]
pub struct FeedStatusDto {
    pub last_run: String,
    pub ok: bool,
    pub message: Option<String>,
    pub new_prices: i64,
}

#[derive(Serialize, Deserialize, ToSchema, Clone, Default)]
pub struct FeedConfigDto {
    /// JSON path to the dates, e.g. `$.data[*].date`
    pub date_path: Option<String>,
    /// JSON path to the close values
    pub close_path: Option<String>,
    /// divide values by this factor (e.g. 100 for pence)
    pub factor: Option<f64>,
}

fn latest_change(s: &Security, date: NaiveDate) -> Option<f64> {
    let p = s.price_at(date);
    let prev = s.prices.iter().rev().find(|x| x.date < p.date)?;
    if prev.value == 0 {
        return None;
    }
    Some(p.value as f64 / prev.value as f64 - 1.0)
}

fn security_dto(
    s: &Security,
    snap: &ClientSnapshot,
    status: Option<&pp_db::FeedStatus>,
) -> SecurityDto {
    let today = snap.date;
    let price = s.price_at(today);
    let pos = snap
        .joint_positions()
        .iter()
        .find(|p| p.security_id == s.id);
    SecurityDto {
        id: s.id.clone(),
        name: s.name.clone(),
        currency: s.currency.clone(),
        isin: s.isin.clone(),
        wkn: s.wkn.clone(),
        ticker: s.ticker.clone(),
        feed: s.feed.clone(),
        note: s.note.clone(),
        retired: s.retired,
        prices: s.prices.len(),
        first_price_date: s.prices.first().map(|p| p.date),
        latest_price: (price.value != 0).then_some(price.value),
        latest_price_date: (price.value != 0).then_some(price.date),
        latest_change: latest_change(s, today),
        shares: pos.map(|p| p.shares).unwrap_or(0),
        market_value: pos.map(|p| p.value_converted).unwrap_or(0),
        feed_status: status.map(|f| FeedStatusDto {
            last_run: f.last_run.clone(),
            ok: f.ok,
            message: f.message.clone(),
            new_prices: f.new_prices,
        }),
    }
}

#[utoipa::path(get, path = "/clients/{id}/securities", tag = "data", params(("id" = String, Path)), responses((status = 200, body = Vec<SecurityDto>)))]
async fn list_securities(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<SecurityDto>>> {
    let c = state.client(&user, &id).await?;
    let snap = ClientSnapshot::create(&c.client, &c.converter(), today());
    let status = state.db.feed_status(&id).await?;
    Ok(Json(
        c.client
            .securities
            .iter()
            .map(|s| security_dto(s, &snap, status.get(&s.id)))
            .collect(),
    ))
}

#[derive(Serialize, ToSchema)]
pub struct PricePoint {
    pub date: NaiveDate,
    pub value: i64,
}

#[derive(Serialize, ToSchema)]
pub struct SecurityDetail {
    pub security: SecurityDto,
    pub feed_config: FeedConfigDto,
    pub calendar: Option<String>,
    pub feed_url: Option<String>,
    pub target_currency: Option<String>,
    pub events: Vec<EventDto>,
    pub transactions: Vec<TransactionRow>,
    pub performance: Option<SecurityPerformanceDto>,
}

#[derive(Serialize, ToSchema)]
pub struct EventDto {
    pub date: NaiveDate,
    pub kind: String,
    pub details: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct SecurityPerformanceDto {
    pub shares: i64,
    pub market_value: i64,
    pub fifo_cost: i64,
    pub fifo_cost_net: i64,
    pub moving_average_cost: i64,
    pub unrealized_gains: i64,
    pub unrealized_gains_percent: f64,
    pub realized_gains: i64,
    pub dividends: i64,
    pub fees: i64,
    pub taxes: i64,
    pub delta: i64,
    pub irr: Option<f64>,
}

impl From<&pp_core::security::SecurityRecord> for SecurityPerformanceDto {
    fn from(r: &pp_core::security::SecurityRecord) -> Self {
        Self {
            shares: r.shares,
            market_value: r.market_value,
            fifo_cost: r.fifo_cost,
            fifo_cost_net: r.fifo_cost_net,
            moving_average_cost: r.moving_average_cost,
            unrealized_gains: r.unrealized_gains,
            unrealized_gains_percent: r.unrealized_gains_percent,
            realized_gains: r.realized_gains,
            dividends: r.dividends,
            fees: r.fees,
            taxes: r.taxes,
            delta: r.delta,
            irr: r.irr.is_finite().then_some(r.irr),
        }
    }
}

#[utoipa::path(get, path = "/clients/{id}/securities/{sid}", tag = "data", params(("id" = String, Path), ("sid" = String, Path)), responses((status = 200, body = SecurityDetail)))]
async fn get_security(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, sid)): Path<(String, String)>,
) -> ApiResult<Json<SecurityDetail>> {
    let c = state.client(&user, &id).await?;
    let s = c.client.security(&sid).ok_or(ApiError::NotFound)?;
    let conv = c.converter();
    let snap = ClientSnapshot::create(&c.client, &conv, today());
    let records = SecurityPerformance::calculate(&c.client, &conv, NaiveDate::MIN, today());
    let rows = transaction_rows(
        &c,
        &TxFilter {
            security: Some(sid.clone()),
            ..Default::default()
        },
    );
    let status = state.db.feed_status(&id).await?;
    let cfg = crate::quotes::feed_config(s);
    Ok(Json(SecurityDetail {
        security: security_dto(s, &snap, status.get(&sid)),
        feed_config: FeedConfigDto {
            date_path: cfg.date_path,
            close_path: cfg.close_path,
            factor: cfg.factor,
        },
        calendar: s.calendar.clone(),
        feed_url: s.feed_url.clone(),
        target_currency: s.target_currency.clone(),
        events: s
            .events
            .iter()
            .map(|e| EventDto {
                date: e.date,
                kind: format!("{:?}", e.kind),
                details: e.details.clone(),
            })
            .collect(),
        transactions: rows,
        performance: records
            .iter()
            .find(|r| r.security_id == sid)
            .map(SecurityPerformanceDto::from),
    }))
}

#[derive(Deserialize, IntoParams)]
struct PriceQuery {
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
}

#[utoipa::path(get, path = "/clients/{id}/securities/{sid}/prices", tag = "data", params(("id" = String, Path), ("sid" = String, Path), PriceQuery), responses((status = 200, body = Vec<PricePoint>)))]
async fn get_prices(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, sid)): Path<(String, String)>,
    Query(q): Query<PriceQuery>,
) -> ApiResult<Json<Vec<PricePoint>>> {
    let c = state.client(&user, &id).await?;
    let s = c.client.security(&sid).ok_or(ApiError::NotFound)?;
    Ok(Json(
        s.prices
            .iter()
            .filter(|p| q.from.is_none_or(|f| p.date >= f) && q.to.is_none_or(|t| p.date <= t))
            .map(|p| PricePoint {
                date: p.date,
                value: p.value,
            })
            .collect(),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct SecurityInput {
    pub name: String,
    pub currency: Option<String>,
    pub isin: Option<String>,
    pub wkn: Option<String>,
    pub ticker: Option<String>,
    pub note: Option<String>,
    #[serde(default)]
    pub retired: bool,
    /// quote feed for historical prices (e.g. YAHOO, GENERIC_JSON, GENERIC_HTML_TABLE, MANUAL)
    pub feed: Option<String>,
    pub feed_url: Option<String>,
    /// feed for the latest quote (defaults to the historical feed)
    pub latest_feed: Option<String>,
    pub latest_feed_url: Option<String>,
    pub calendar: Option<String>,
    pub target_currency: Option<String>,
    /// settings for GENERIC-JSON feeds
    pub feed_config: Option<FeedConfigDto>,
}

fn opt(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

impl SecurityInput {
    fn apply(self, s: &mut Security) {
        s.name = self.name.trim().to_string();
        s.currency = opt(self.currency)
            .map(|c| c.to_uppercase())
            .or(s.currency.take());
        s.isin = opt(self.isin).map(|c| c.to_uppercase());
        s.wkn = opt(self.wkn);
        s.ticker = opt(self.ticker);
        s.note = opt(self.note);
        s.retired = self.retired;
        s.feed = opt(self.feed);
        s.feed_url = opt(self.feed_url);
        s.latest_feed = opt(self.latest_feed);
        s.latest_feed_url = opt(self.latest_feed_url);
        s.calendar = opt(self.calendar);
        s.target_currency = opt(self.target_currency);
        if let Some(cfg) = self.feed_config {
            if !s.attributes.is_object() {
                s.attributes = serde_json::json!({});
            }
            s.attributes[crate::quotes::FEED_CONFIG_KEY] =
                serde_json::to_value(cfg).unwrap_or_default();
        }
        s.updated_at = Some(chrono::Utc::now().to_rfc3339());
    }
}

/// Updates the master data of a security (prices and events are kept).
#[utoipa::path(put, path = "/clients/{id}/securities/{sid}", tag = "data", params(("id" = String, Path), ("sid" = String, Path)), request_body = SecurityInput, responses((status = 204)))]
async fn update_security(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, sid)): Path<(String, String)>,
    Json(input): Json<SecurityInput>,
) -> ApiResult<StatusCode> {
    let c = state.client(&user, &id).await?;
    let mut s = c.client.security(&sid).ok_or(ApiError::NotFound)?.clone();
    if input.name.trim().is_empty() {
        return Err(ApiError::BadRequest("Name fehlt".into()));
    }
    input.apply(&mut s);
    state.db.save_security(&id, &s).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/clients/{id}/securities", tag = "data", params(("id" = String, Path)), request_body = SecurityInput, responses((status = 201, body = String)))]
async fn create_security(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(input): Json<SecurityInput>,
) -> ApiResult<(StatusCode, Json<String>)> {
    let c = state.client(&user, &id).await?;
    let sid = uuid::Uuid::new_v4().to_string();
    let mut s = Security {
        id: sid.clone(),
        currency: Some(c.client.base_currency.clone()),
        ..Default::default()
    };
    if input.name.trim().is_empty() {
        return Err(ApiError::BadRequest("Name fehlt".into()));
    }
    input.apply(&mut s);
    state.db.save_security(&id, &s).await?;
    Ok((StatusCode::CREATED, Json(sid)))
}

#[derive(Deserialize, ToSchema)]
pub struct PriceInput {
    pub date: NaiveDate,
    /// quote with factor 10^8
    pub value: i64,
}

#[utoipa::path(put, path = "/clients/{id}/securities/{sid}/prices", tag = "data", params(("id" = String, Path), ("sid" = String, Path)), request_body = Vec<PriceInput>, responses((status = 204)))]
async fn put_prices(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, sid)): Path<(String, String)>,
    Json(prices): Json<Vec<PriceInput>>,
) -> ApiResult<StatusCode> {
    let c = state.client(&user, &id).await?;
    c.client.security(&sid).ok_or(ApiError::NotFound)?;
    let prices: Vec<SecurityPrice> = prices
        .iter()
        .map(|p| SecurityPrice {
            date: p.date,
            value: p.value,
        })
        .collect();
    state.db.upsert_prices(&id, &sid, &prices).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete, path = "/clients/{id}/securities/{sid}", tag = "data", params(("id" = String, Path), ("sid" = String, Path)), responses((status = 204), (status = 409)))]
async fn delete_security(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, sid)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    state.client(&user, &id).await?;
    state.db.delete_entity(&id, "securities", &sid).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------ accounts/portfolios

#[derive(Serialize, ToSchema)]
pub struct AccountDto {
    pub id: String,
    pub name: String,
    pub currency: String,
    pub note: Option<String>,
    pub retired: bool,
    pub balance: i64,
    /// balance in the base currency of the client
    pub balance_converted: i64,
    pub transactions: usize,
}

#[utoipa::path(get, path = "/clients/{id}/accounts", tag = "data", params(("id" = String, Path)), responses((status = 200, body = Vec<AccountDto>)))]
async fn list_accounts(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<AccountDto>>> {
    let c = state.client(&user, &id).await?;
    let snap = ClientSnapshot::create(&c.client, &c.converter(), today());
    Ok(Json(
        c.client
            .accounts
            .iter()
            .map(|a| {
                let s = snap.accounts.iter().find(|x| x.account_id == a.id);
                AccountDto {
                    id: a.id.clone(),
                    name: a.name.clone(),
                    currency: a.currency.clone(),
                    note: a.note.clone(),
                    retired: a.retired,
                    balance: s.map(|s| s.funds).unwrap_or(0),
                    balance_converted: s.map(|s| s.funds_converted).unwrap_or(0),
                    transactions: a.transactions.len(),
                }
            })
            .collect(),
    ))
}

#[derive(Serialize, ToSchema)]
pub struct PortfolioDto {
    pub id: String,
    pub name: String,
    pub note: Option<String>,
    pub retired: bool,
    pub reference_account: Option<String>,
    pub value: i64,
    pub positions: usize,
    pub transactions: usize,
}

#[utoipa::path(get, path = "/clients/{id}/portfolios", tag = "data", params(("id" = String, Path)), responses((status = 200, body = Vec<PortfolioDto>)))]
async fn list_portfolios(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<PortfolioDto>>> {
    let c = state.client(&user, &id).await?;
    let snap = ClientSnapshot::create(&c.client, &c.converter(), today());
    Ok(Json(
        c.client
            .portfolios
            .iter()
            .map(|p| {
                let pos: Vec<_> = snap
                    .positions
                    .iter()
                    .filter(|x| x.portfolio_id == p.id)
                    .collect();
                PortfolioDto {
                    id: p.id.clone(),
                    name: p.name.clone(),
                    note: p.note.clone(),
                    retired: p.retired,
                    reference_account: p.reference_account.clone(),
                    value: pos.iter().map(|x| x.value_converted).sum(),
                    positions: pos.len(),
                    transactions: p.transactions.len(),
                }
            })
            .collect(),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct AccountInput {
    pub name: String,
    pub currency: Option<String>,
    pub note: Option<String>,
    #[serde(default)]
    pub retired: bool,
}

#[utoipa::path(put, path = "/clients/{id}/accounts/{aid}", tag = "data", params(("id" = String, Path), ("aid" = String, Path)), request_body = AccountInput, responses((status = 204), (status = 409)))]
async fn update_account(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, aid)): Path<(String, String)>,
    Json(input): Json<AccountInput>,
) -> ApiResult<StatusCode> {
    let c = state.client(&user, &id).await?;
    let mut a = c.client.account(&aid).ok_or(ApiError::NotFound)?.clone();
    if let Some(cur) = opt(input.currency).map(|c| c.to_uppercase())
        && cur != a.currency
    {
        if !a.transactions.is_empty() {
            return Err(ApiError::Conflict(
                "Die Währung kann nur ohne Buchungen geändert werden".into(),
            ));
        }
        a.currency = cur;
    }
    a.name = input.name.trim().to_string();
    a.note = opt(input.note);
    a.retired = input.retired;
    a.updated_at = Some(chrono::Utc::now().to_rfc3339());
    a.transactions.clear();
    state.db.save_account(&id, &a).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/clients/{id}/accounts", tag = "data", params(("id" = String, Path)), request_body = AccountInput, responses((status = 201, body = String)))]
async fn create_account(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(input): Json<AccountInput>,
) -> ApiResult<(StatusCode, Json<String>)> {
    let c = state.client(&user, &id).await?;
    let aid = uuid::Uuid::new_v4().to_string();
    let a = Account {
        id: aid.clone(),
        name: input.name,
        currency: input
            .currency
            .unwrap_or_else(|| c.client.base_currency.clone()),
        note: input.note,
        updated_at: Some(chrono::Utc::now().to_rfc3339()),
        ..Default::default()
    };
    state.db.save_account(&id, &a).await?;
    Ok((StatusCode::CREATED, Json(aid)))
}

#[derive(Deserialize, ToSchema)]
pub struct PortfolioInput {
    pub name: String,
    pub reference_account: Option<String>,
    pub note: Option<String>,
    #[serde(default)]
    pub retired: bool,
}

#[utoipa::path(put, path = "/clients/{id}/portfolios/{pid}", tag = "data", params(("id" = String, Path), ("pid" = String, Path)), request_body = PortfolioInput, responses((status = 204)))]
async fn update_portfolio(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, pid)): Path<(String, String)>,
    Json(input): Json<PortfolioInput>,
) -> ApiResult<StatusCode> {
    let c = state.client(&user, &id).await?;
    let mut p = c.client.portfolio(&pid).ok_or(ApiError::NotFound)?.clone();
    if let Some(r) = &input.reference_account {
        c.client
            .account(r)
            .ok_or_else(|| ApiError::BadRequest("Referenzkonto unbekannt".into()))?;
    }
    p.name = input.name.trim().to_string();
    p.note = opt(input.note);
    p.retired = input.retired;
    p.reference_account = input.reference_account;
    p.updated_at = Some(chrono::Utc::now().to_rfc3339());
    p.transactions.clear();
    state.db.save_portfolio(&id, &p).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/clients/{id}/portfolios", tag = "data", params(("id" = String, Path)), request_body = PortfolioInput, responses((status = 201, body = String)))]
async fn create_portfolio(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(input): Json<PortfolioInput>,
) -> ApiResult<(StatusCode, Json<String>)> {
    let c = state.client(&user, &id).await?;
    if let Some(r) = &input.reference_account {
        c.client
            .account(r)
            .ok_or_else(|| ApiError::BadRequest("Referenzkonto unbekannt".into()))?;
    }
    let pid = uuid::Uuid::new_v4().to_string();
    let p = Portfolio {
        id: pid.clone(),
        name: input.name,
        reference_account: input.reference_account,
        note: input.note,
        updated_at: Some(chrono::Utc::now().to_rfc3339()),
        ..Default::default()
    };
    state.db.save_portfolio(&id, &p).await?;
    Ok((StatusCode::CREATED, Json(pid)))
}

#[utoipa::path(delete, path = "/clients/{id}/accounts/{aid}", tag = "data", params(("id" = String, Path), ("aid" = String, Path)), responses((status = 204), (status = 409)))]
async fn delete_account(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, aid)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    state.client(&user, &id).await?;
    state.db.delete_entity(&id, "accounts", &aid).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete, path = "/clients/{id}/portfolios/{pid}", tag = "data", params(("id" = String, Path), ("pid" = String, Path)), responses((status = 204), (status = 409)))]
async fn delete_portfolio(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, pid)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    state.client(&user, &id).await?;
    state.db.delete_entity(&id, "portfolios", &pid).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------ transactions

#[derive(Serialize, ToSchema, Clone)]
pub struct TransactionRow {
    pub id: String,
    /// `account` or `portfolio`
    pub owner_kind: String,
    pub owner_id: String,
    pub owner_name: String,
    #[schema(example = "BUY")]
    pub kind: String,
    pub date_time: NaiveDateTime,
    pub currency: String,
    pub amount: i64,
    pub shares: i64,
    pub security_id: Option<String>,
    pub security_name: Option<String>,
    pub fees: i64,
    pub taxes: i64,
    /// gross value (before fees and taxes)
    pub gross_value: i64,
    pub note: Option<String>,
    pub source: Option<String>,
    pub other_owner_name: Option<String>,
    /// signed effect on the owner: cash (accounts) or shares (portfolios)
    pub sign: i32,
}

#[derive(Deserialize, IntoParams, Default)]
pub struct TxFilter {
    /// account or portfolio id
    pub owner: Option<String>,
    pub security: Option<String>,
    /// `account` or `portfolio`
    pub owner_kind: Option<String>,
    /// comma separated transaction types
    pub types: Option<String>,
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    /// full text search in note, security and owner name
    pub q: Option<String>,
}

pub(crate) fn transaction_rows(c: &LoadedClient, f: &TxFilter) -> Vec<TransactionRow> {
    let names: HashMap<&str, &str> = c
        .client
        .accounts
        .iter()
        .map(|a| (a.id.as_str(), a.name.as_str()))
        .chain(
            c.client
                .portfolios
                .iter()
                .map(|p| (p.id.as_str(), p.name.as_str())),
        )
        .collect();
    let sec_names: HashMap<&str, &str> = c
        .client
        .securities
        .iter()
        .map(|s| (s.id.as_str(), s.name.as_str()))
        .collect();
    let types: Option<Vec<&str>> = f.types.as_deref().map(|t| t.split(',').collect());
    let q = f.q.as_deref().map(str::to_lowercase);

    let mut rows = Vec::new();
    let mut push = |kind_str: &str,
                    owner_kind: &str,
                    owner_id: &str,
                    t: &TransactionCommon,
                    gross: i64,
                    sign: i32| {
        if f.owner.as_deref().is_some_and(|o| o != owner_id)
            || f.owner_kind.as_deref().is_some_and(|k| k != owner_kind)
            || f.security
                .as_deref()
                .is_some_and(|s| t.security.as_deref() != Some(s))
            || types.as_ref().is_some_and(|ty| !ty.contains(&kind_str))
            || f.from.is_some_and(|d| t.date() < d)
            || f.to.is_some_and(|d| t.date() > d)
        {
            return;
        }
        let security_name = t
            .security
            .as_deref()
            .and_then(|s| sec_names.get(s))
            .map(|s| s.to_string());
        let owner_name = names
            .get(owner_id)
            .map(|s| s.to_string())
            .unwrap_or_default();
        if let Some(q) = &q {
            let hay = format!(
                "{} {} {} {}",
                t.note.as_deref().unwrap_or_default(),
                security_name.as_deref().unwrap_or_default(),
                owner_name,
                kind_str
            )
            .to_lowercase();
            if !hay.contains(q) {
                return;
            }
        }
        rows.push(TransactionRow {
            id: t.id.clone(),
            owner_kind: owner_kind.into(),
            owner_id: owner_id.into(),
            owner_name,
            kind: kind_str.into(),
            date_time: t.date_time,
            currency: t.currency.clone(),
            amount: t.amount,
            shares: t.shares,
            security_id: t.security.clone(),
            security_name,
            fees: t.unit_sum(UnitType::Fee),
            taxes: t.unit_sum(UnitType::Tax),
            gross_value: gross,
            note: t.note.clone(),
            source: t.source.clone(),
            other_owner_name: t
                .cross_entry
                .as_ref()
                .and_then(|ce| names.get(ce.other_owner.as_str()))
                .map(|s| s.to_string()),
            sign,
        });
    };
    for a in &c.client.accounts {
        for t in &a.transactions {
            let tf = t.unit_sum(UnitType::Fee) + t.unit_sum(UnitType::Tax);
            let gross = match t.kind {
                AccountTransactionType::Dividends | AccountTransactionType::Interest => {
                    t.amount + tf
                }
                _ => t.amount,
            };
            push(
                t.kind.as_str(),
                "account",
                &a.id,
                &t.common,
                gross,
                if t.kind.is_credit() { 1 } else { -1 },
            );
        }
    }
    for p in &c.client.portfolios {
        for t in &p.transactions {
            push(
                t.kind.as_str(),
                "portfolio",
                &p.id,
                &t.common,
                t.gross_value_amount(),
                if t.kind.is_purchase() { 1 } else { -1 },
            );
        }
    }
    rows.sort_by(|a, b| b.date_time.cmp(&a.date_time).then_with(|| a.id.cmp(&b.id)));
    rows
}

#[derive(Deserialize, IntoParams)]
struct PageQuery {
    #[serde(default)]
    offset: usize,
    limit: Option<usize>,
}

#[derive(Serialize, ToSchema)]
pub struct TransactionPage {
    pub total: usize,
    pub offset: usize,
    pub items: Vec<TransactionRow>,
}

#[utoipa::path(get, path = "/clients/{id}/transactions", tag = "data", params(("id" = String, Path), TxFilter, PageQuery), responses((status = 200, body = TransactionPage)))]
async fn list_transactions(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Query(f): Query<TxFilter>,
    Query(page): Query<PageQuery>,
) -> ApiResult<Json<TransactionPage>> {
    let c = state.client(&user, &id).await?;
    let rows = transaction_rows(&c, &f);
    let total = rows.len();
    let limit = page.limit.unwrap_or(500).min(10_000);
    let items = rows.into_iter().skip(page.offset).take(limit).collect();
    Ok(Json(TransactionPage {
        total,
        offset: page.offset,
        items,
    }))
}

fn store_booking_error(e: String) -> ApiError {
    ApiError::BadRequest(e)
}

/// Creates a transaction (and its counterpart for buys, sells, transfers).
#[utoipa::path(post, path = "/clients/{id}/transactions", tag = "data", params(("id" = String, Path)), request_body = Booking, responses((status = 201, body = String), (status = 400)))]
async fn create_transaction(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(booking): Json<Booking>,
) -> ApiResult<(StatusCode, Json<String>)> {
    let c = state.client(&user, &id).await?;
    let txs = booking
        .build(&c.client, None, Some(chrono::Utc::now().to_rfc3339()))
        .map_err(store_booking_error)?;
    let primary = txs[0].id().to_string();
    state.db.replace_transactions(&id, &[], &txs).await?;
    Ok((StatusCode::CREATED, Json(primary)))
}

#[derive(Serialize, ToSchema)]
pub struct BookingDto {
    /// id of the primary transaction of the pair
    pub id: String,
    pub booking: Booking,
}

/// Returns the booking of an existing transaction for editing.
#[utoipa::path(get, path = "/clients/{id}/transactions/{tid}", tag = "data", params(("id" = String, Path), ("tid" = String, Path)), responses((status = 200, body = BookingDto)))]
async fn get_transaction(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, tid)): Path<(String, String)>,
) -> ApiResult<Json<BookingDto>> {
    let c = state.client(&user, &id).await?;
    let (booking, primary) =
        pp_core::booking::reconstruct(&c.client, &tid).ok_or(ApiError::NotFound)?;
    Ok(Json(BookingDto {
        id: primary,
        booking,
    }))
}

/// Replaces a transaction (and its counterpart), keeping the ids.
#[utoipa::path(put, path = "/clients/{id}/transactions/{tid}", tag = "data", params(("id" = String, Path), ("tid" = String, Path)), request_body = Booking, responses((status = 200, body = String), (status = 400)))]
async fn update_transaction(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, tid)): Path<(String, String)>,
    Json(booking): Json<Booking>,
) -> ApiResult<Json<String>> {
    let c = state.client(&user, &id).await?;
    let (_, primary) = pp_core::booking::reconstruct(&c.client, &tid).ok_or(ApiError::NotFound)?;
    // build against a client without the old transactions (not needed for
    // validation, ids are kept)
    let txs = booking
        .build(
            &c.client,
            Some(primary.clone()),
            Some(chrono::Utc::now().to_rfc3339()),
        )
        .map_err(store_booking_error)?;
    state
        .db
        .replace_transactions(&id, std::slice::from_ref(&primary), &txs)
        .await?;
    Ok(Json(primary))
}

#[utoipa::path(delete, path = "/clients/{id}/transactions/{tid}", tag = "data", params(("id" = String, Path), ("tid" = String, Path)), responses((status = 204)))]
async fn delete_transaction(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, tid)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    state.client(&user, &id).await?;
    state.db.delete_transaction(&id, &tid).await?;
    Ok(StatusCode::NO_CONTENT)
}

// --------------------------------------------------------------- watchlists

#[derive(Serialize, ToSchema)]
pub struct WatchlistDto {
    pub name: String,
    pub securities: Vec<String>,
}

#[utoipa::path(get, path = "/clients/{id}/watchlists", tag = "data", params(("id" = String, Path)), responses((status = 200, body = Vec<WatchlistDto>)))]
async fn list_watchlists(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<WatchlistDto>>> {
    let c = state.client(&user, &id).await?;
    Ok(Json(
        c.client
            .watchlists
            .iter()
            .map(|w| WatchlistDto {
                name: w.name.clone(),
                securities: w.securities.clone(),
            })
            .collect(),
    ))
}

#[derive(Deserialize, ToSchema, Default)]
pub struct QuoteUpdateRequest {
    /// only this security (default: all active securities with a feed)
    pub security_id: Option<String>,
}

/// Fetches new prices from the configured quote feeds.
#[utoipa::path(post, path = "/clients/{id}/quotes/update", tag = "data", params(("id" = String, Path)), request_body = QuoteUpdateRequest, responses((status = 200, body = Vec<crate::quotes::QuoteUpdateResult>)))]
async fn update_quotes(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<QuoteUpdateRequest>,
) -> ApiResult<Json<Vec<crate::quotes::QuoteUpdateResult>>> {
    state.client(&user, &id).await?;
    Ok(Json(
        crate::quotes::update_client(&state, &id, req.security_id.as_deref()).await?,
    ))
}

#[derive(Deserialize, Serialize, ToSchema)]
pub struct FeedSettings {
    /// write only; on read only whether a key is set
    pub alphavantage_api_key: Option<String>,
    #[serde(default)]
    pub alphavantage_api_key_set: bool,
}

#[utoipa::path(get, path = "/settings/feeds", tag = "system", responses((status = 200, body = FeedSettings)))]
async fn get_feed_settings(
    State(state): State<SharedState>,
    _user: CurrentUser,
) -> ApiResult<Json<FeedSettings>> {
    let key = state.db.setting("alphavantage_api_key").await?;
    Ok(Json(FeedSettings {
        alphavantage_api_key: None,
        alphavantage_api_key_set: key.is_some_and(|k| !k.is_empty()),
    }))
}

#[utoipa::path(put, path = "/settings/feeds", tag = "system", request_body = FeedSettings, responses((status = 204), (status = 403)))]
async fn put_feed_settings(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Json(s): Json<FeedSettings>,
) -> ApiResult<StatusCode> {
    if !user.is_admin {
        return Err(ApiError::Forbidden);
    }
    state
        .db
        .set_setting(
            "alphavantage_api_key",
            s.alphavantage_api_key.as_deref().unwrap_or_default().trim(),
        )
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> OpenApiRouter<SharedState> {
    OpenApiRouter::new()
        .routes(routes!(list_securities, create_security))
        .routes(routes!(get_security, update_security, delete_security))
        .routes(routes!(get_prices, put_prices))
        .routes(routes!(list_accounts, create_account))
        .routes(routes!(update_account, delete_account))
        .routes(routes!(list_portfolios, create_portfolio))
        .routes(routes!(update_portfolio, delete_portfolio))
        .routes(routes!(list_transactions, create_transaction))
        .routes(routes!(
            get_transaction,
            update_transaction,
            delete_transaction
        ))
        .routes(routes!(list_watchlists))
        .routes(routes!(update_quotes))
        .routes(routes!(get_feed_settings, put_feed_settings))
}
