//! Master data and transactions.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::{NaiveDate, NaiveDateTime};
use pp_core::Money;
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
}

fn latest_change(s: &Security, date: NaiveDate) -> Option<f64> {
    let p = s.price_at(date);
    let prev = s.prices.iter().rev().find(|x| x.date < p.date)?;
    if prev.value == 0 {
        return None;
    }
    Some(p.value as f64 / prev.value as f64 - 1.0)
}

fn security_dto(s: &Security, snap: &ClientSnapshot) -> SecurityDto {
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
    Ok(Json(
        c.client
            .securities
            .iter()
            .map(|s| security_dto(s, &snap))
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
    Ok(Json(SecurityDetail {
        security: security_dto(s, &snap),
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
    let s = Security {
        id: sid.clone(),
        name: input.name,
        currency: input
            .currency
            .or_else(|| Some(c.client.base_currency.clone())),
        isin: input.isin,
        wkn: input.wkn,
        ticker: input.ticker,
        note: input.note,
        retired: input.retired,
        updated_at: Some(chrono::Utc::now().to_rfc3339()),
        ..Default::default()
    };
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

#[derive(Deserialize, ToSchema)]
pub struct TransactionInput {
    /// DEPOSIT, REMOVAL, INTEREST, INTEREST_CHARGE, DIVIDENDS, FEES, FEES_REFUND, TAXES, TAX_REFUND, BUY, SELL
    #[schema(example = "BUY")]
    pub kind: String,
    pub account_id: String,
    /// required for BUY / SELL
    pub portfolio_id: Option<String>,
    /// required for BUY / SELL / DIVIDENDS
    pub security_id: Option<String>,
    pub date_time: NaiveDateTime,
    /// total amount (factor 100); for buys including fees and taxes
    pub amount: i64,
    /// shares (factor 10^8)
    #[serde(default)]
    pub shares: i64,
    #[serde(default)]
    pub fees: i64,
    #[serde(default)]
    pub taxes: i64,
    pub note: Option<String>,
}

#[utoipa::path(post, path = "/clients/{id}/transactions", tag = "data", params(("id" = String, Path)), request_body = TransactionInput, responses((status = 201, body = String), (status = 400)))]
async fn create_transaction(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(input): Json<TransactionInput>,
) -> ApiResult<(StatusCode, Json<String>)> {
    let c = state.client(&user, &id).await?;
    let account = c
        .client
        .account(&input.account_id)
        .ok_or_else(|| ApiError::BadRequest("Konto unbekannt".into()))?;
    if input.amount <= 0 {
        return Err(ApiError::BadRequest("Betrag muss positiv sein".into()));
    }
    let currency = account.currency.clone();
    let mut units = Vec::new();
    if input.fees != 0 {
        units.push(TransactionUnit {
            kind: UnitType::Fee,
            amount: Money::new(&currency, input.fees),
            forex: None,
            exchange_rate: None,
        });
    }
    if input.taxes != 0 {
        units.push(TransactionUnit {
            kind: UnitType::Tax,
            amount: Money::new(&currency, input.taxes),
            forex: None,
            exchange_rate: None,
        });
    }
    if let Some(sid) = &input.security_id {
        c.client
            .security(sid)
            .ok_or_else(|| ApiError::BadRequest("Wertpapier unbekannt".into()))?;
    }
    let tx_id = uuid::Uuid::new_v4().to_string();
    let now = Some(chrono::Utc::now().to_rfc3339());
    let common =
        |tid: &str, units: Vec<TransactionUnit>, cross: Option<CrossEntry>| TransactionCommon {
            id: tid.into(),
            date_time: input.date_time,
            currency: currency.clone(),
            amount: input.amount,
            shares: input.shares,
            security: input.security_id.clone(),
            note: input.note.clone(),
            source: None,
            units,
            cross_entry: cross,
            updated_at: now.clone(),
        };

    match input.kind.as_str() {
        "BUY" | "SELL" => {
            let pid = input
                .portfolio_id
                .as_deref()
                .ok_or_else(|| ApiError::BadRequest("Depot fehlt".into()))?;
            c.client
                .portfolio(pid)
                .ok_or_else(|| ApiError::BadRequest("Depot unbekannt".into()))?;
            if input.security_id.is_none() || input.shares <= 0 {
                return Err(ApiError::BadRequest(
                    "Wertpapier und Stückzahl erforderlich".into(),
                ));
            }
            let sec = c
                .client
                .security(input.security_id.as_deref().unwrap())
                .unwrap();
            if sec.currency.as_deref().is_some_and(|sc| sc != currency) {
                return Err(ApiError::BadRequest(
                    "Fremdwährungsbuchungen werden noch nicht unterstützt".into(),
                ));
            }
            let buy = input.kind == "BUY";
            if buy && input.fees + input.taxes >= input.amount {
                return Err(ApiError::BadRequest(
                    "Gebühren und Steuern übersteigen den Betrag".into(),
                ));
            }
            let atx_id = uuid::Uuid::new_v4().to_string();
            let ptx = PortfolioTransaction {
                kind: if buy {
                    PortfolioTransactionType::Buy
                } else {
                    PortfolioTransactionType::Sell
                },
                common: common(
                    &tx_id,
                    units,
                    Some(CrossEntry {
                        kind: CrossEntryKind::BuySell,
                        other_owner: account.id.clone(),
                        other_transaction: atx_id.clone(),
                    }),
                ),
            };
            let mut ac = common(
                &atx_id,
                vec![],
                Some(CrossEntry {
                    kind: CrossEntryKind::BuySell,
                    other_owner: pid.to_string(),
                    other_transaction: tx_id.clone(),
                }),
            );
            ac.shares = 0;
            let atx = AccountTransaction {
                kind: if buy {
                    AccountTransactionType::Buy
                } else {
                    AccountTransactionType::Sell
                },
                common: ac,
                ex_date: None,
            };
            state
                .db
                .insert_buy_sell(&id, pid, &ptx, &account.id, &atx)
                .await?;
        }
        k => {
            let kind = AccountTransactionType::parse(k)
                .ok_or_else(|| ApiError::BadRequest(format!("Buchungstyp {k} unbekannt")))?;
            use AccountTransactionType as A;
            if matches!(kind, A::Buy | A::Sell | A::TransferIn | A::TransferOut) {
                return Err(ApiError::BadRequest(
                    "Umbuchungen werden noch nicht unterstützt".into(),
                ));
            }
            if kind == A::Dividends && input.security_id.is_none() {
                return Err(ApiError::BadRequest(
                    "Dividende benötigt ein Wertpapier".into(),
                ));
            }
            let atx = AccountTransaction {
                kind,
                common: common(&tx_id, units, None),
                ex_date: None,
            };
            state.db.insert_account_tx(&id, &account.id, &atx).await?;
        }
    }
    Ok((StatusCode::CREATED, Json(tx_id)))
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

pub fn router() -> OpenApiRouter<SharedState> {
    OpenApiRouter::new()
        .routes(routes!(list_securities, create_security))
        .routes(routes!(get_security, delete_security))
        .routes(routes!(get_prices, put_prices))
        .routes(routes!(list_accounts, create_account))
        .routes(routes!(delete_account))
        .routes(routes!(list_portfolios, create_portfolio))
        .routes(routes!(delete_portfolio))
        .routes(routes!(list_transactions, create_transaction))
        .routes(routes!(delete_transaction))
        .routes(routes!(list_watchlists))
}
