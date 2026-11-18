//! Investment plans (Sparpläne).

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::{NaiveDate, NaiveDateTime};
use pp_core::plan::{InvestmentPlan, PlanType};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, SharedState};

#[derive(Serialize, ToSchema)]
pub struct PlanDto {
    #[serde(flatten)]
    pub plan: InvestmentPlan,
    pub next_date: NaiveDate,
    pub executions: usize,
    pub security_name: Option<String>,
    pub account_name: Option<String>,
    pub portfolio_name: Option<String>,
}

#[derive(Deserialize, ToSchema)]
pub struct PlanInput {
    pub name: String,
    pub note: Option<String>,
    pub kind: PlanType,
    pub security: Option<String>,
    pub portfolio: Option<String>,
    pub account: Option<String>,
    #[serde(default)]
    pub auto_generate: bool,
    pub start: NaiveDateTime,
    /// months (1..99) or weeks + 100
    pub interval: i32,
    pub amount: i64,
    #[serde(default)]
    pub fees: i64,
    #[serde(default)]
    pub taxes: i64,
}

#[derive(Serialize, ToSchema)]
pub struct ExecuteResult {
    pub created: usize,
    pub errors: Vec<String>,
}

/// Creates the due transactions of the plans of a client.
pub async fn execute(
    state: &Arc<AppState>,
    client_id: &str,
    only: Option<&str>,
    auto_only: bool,
) -> anyhow::Result<ExecuteResult> {
    let client = state.db.load_client(client_id).await?;
    let provider = crate::state::build_provider(&state.exchange_rate_series().await, &client);
    let conv = pp_core::CurrencyConverter::new(Arc::new(provider), client.base_currency.clone());
    let today = crate::api::today();
    let mut created = 0;
    let mut errors = Vec::new();
    for plan in &client.plans {
        if only.is_some_and(|o| o != plan.id) || (auto_only && !plan.auto_generate) {
            continue;
        }
        let bookings = match plan.due_bookings(&client, &conv, today) {
            Ok(b) => b,
            Err(e) => {
                errors.push(format!("{}: {e}", plan.name));
                continue;
            }
        };
        if bookings.is_empty() {
            continue;
        }
        let now = Some(chrono::Utc::now().to_rfc3339());
        let mut txs = Vec::new();
        let mut ids = Vec::new();
        let mut failed = false;
        for b in bookings {
            match b.build(&client, None, now.clone()) {
                Ok(mut v) => {
                    ids.push(v[0].id().to_string());
                    txs.append(&mut v);
                }
                Err(e) => {
                    errors.push(format!("{}: {e}", plan.name));
                    failed = true;
                    break;
                }
            }
        }
        if failed {
            continue;
        }
        state.db.replace_transactions(client_id, &[], &txs).await?;
        let mut p = plan.clone();
        created += ids.len();
        p.transactions.extend(ids);
        state.db.save_plan(client_id, &p).await?;
    }
    Ok(ExecuteResult { created, errors })
}

/// Background job: executes plans with automatic generation.
pub async fn execute_all(state: &Arc<AppState>) -> anyhow::Result<()> {
    for id in state.db.all_client_ids().await? {
        match execute(state, &id, None, true).await {
            Ok(r) if r.created > 0 || !r.errors.is_empty() => {
                tracing::info!(
                    "investment plans of {id}: {} transactions, errors {:?}",
                    r.created,
                    r.errors
                )
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("investment plans of {id} failed: {e:#}"),
        }
    }
    Ok(())
}

#[utoipa::path(get, path = "/clients/{id}/plans", tag = "data", params(("id" = String, Path)), responses((status = 200, body = Vec<PlanDto>)))]
async fn list_plans(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Vec<PlanDto>>> {
    let c = state.client(&user, &id).await?;
    let cl = &c.client;
    Ok(Json(
        cl.plans
            .iter()
            .map(|p| PlanDto {
                next_date: p.next_date(cl),
                executions: p
                    .transactions
                    .iter()
                    .filter(|t| pp_core::booking::find(cl, t).is_some())
                    .count(),
                security_name: p
                    .security
                    .as_deref()
                    .and_then(|s| cl.security(s))
                    .map(|s| s.name.clone()),
                account_name: p
                    .account
                    .as_deref()
                    .and_then(|s| cl.account(s))
                    .map(|s| s.name.clone()),
                portfolio_name: p
                    .portfolio
                    .as_deref()
                    .and_then(|s| cl.portfolio(s))
                    .map(|s| s.name.clone()),
                plan: p.clone(),
            })
            .collect(),
    ))
}

fn validate(c: &pp_core::Client, i: &PlanInput) -> ApiResult<()> {
    if i.name.trim().is_empty() || i.amount <= 0 || i.interval <= 0 {
        return Err(ApiError::BadRequest(
            "Name, Betrag und Intervall erforderlich".into(),
        ));
    }
    match i.kind {
        PlanType::PurchaseOrDelivery => {
            if i.security.as_deref().and_then(|s| c.security(s)).is_none()
                || i.portfolio
                    .as_deref()
                    .and_then(|p| c.portfolio(p))
                    .is_none()
            {
                return Err(ApiError::BadRequest(
                    "Wertpapier und Depot erforderlich".into(),
                ));
            }
        }
        _ => {
            if i.account.as_deref().and_then(|a| c.account(a)).is_none() {
                return Err(ApiError::BadRequest("Konto erforderlich".into()));
            }
        }
    }
    Ok(())
}

fn apply(p: &mut InvestmentPlan, i: PlanInput) {
    p.name = i.name.trim().into();
    p.note = i.note.filter(|n| !n.trim().is_empty());
    p.kind = i.kind;
    p.security = i
        .security
        .filter(|_| i.kind == PlanType::PurchaseOrDelivery);
    p.portfolio = i
        .portfolio
        .filter(|_| i.kind == PlanType::PurchaseOrDelivery);
    p.account = i.account;
    p.auto_generate = i.auto_generate;
    p.start = i.start;
    p.interval = i.interval;
    p.amount = i.amount;
    p.fees = i.fees;
    p.taxes = i.taxes;
}

#[utoipa::path(post, path = "/clients/{id}/plans", tag = "data", params(("id" = String, Path)), request_body = PlanInput, responses((status = 201, body = String)))]
async fn create_plan(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(input): Json<PlanInput>,
) -> ApiResult<(StatusCode, Json<String>)> {
    let c = state.client(&user, &id).await?;
    validate(&c.client, &input)?;
    let mut p = InvestmentPlan {
        id: uuid::Uuid::new_v4().to_string(),
        name: String::new(),
        note: None,
        kind: PlanType::Deposit,
        security: None,
        portfolio: None,
        account: None,
        auto_generate: false,
        start: input.start,
        interval: 1,
        amount: 0,
        fees: 0,
        taxes: 0,
        transactions: vec![],
    };
    apply(&mut p, input);
    state.db.save_plan(&id, &p).await?;
    Ok((StatusCode::CREATED, Json(p.id)))
}

#[utoipa::path(put, path = "/clients/{id}/plans/{pid}", tag = "data", params(("id" = String, Path), ("pid" = String, Path)), request_body = PlanInput, responses((status = 204)))]
async fn update_plan(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, pid)): Path<(String, String)>,
    Json(input): Json<PlanInput>,
) -> ApiResult<StatusCode> {
    let c = state.client(&user, &id).await?;
    validate(&c.client, &input)?;
    let mut p = c
        .client
        .plans
        .iter()
        .find(|p| p.id == pid)
        .cloned()
        .ok_or(ApiError::NotFound)?;
    apply(&mut p, input);
    state.db.save_plan(&id, &p).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete, path = "/clients/{id}/plans/{pid}", tag = "data", params(("id" = String, Path), ("pid" = String, Path)), responses((status = 204)))]
async fn delete_plan(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, pid)): Path<(String, String)>,
) -> ApiResult<StatusCode> {
    state.client(&user, &id).await?;
    state.db.delete_plan(&id, &pid).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Books all due executions of a plan until today.
#[utoipa::path(post, path = "/clients/{id}/plans/{pid}/execute", tag = "data", params(("id" = String, Path), ("pid" = String, Path)), responses((status = 200, body = ExecuteResult)))]
async fn execute_plan(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, pid)): Path<(String, String)>,
) -> ApiResult<Json<ExecuteResult>> {
    let c = state.client(&user, &id).await?;
    if !c.client.plans.iter().any(|p| p.id == pid) {
        return Err(ApiError::NotFound);
    }
    Ok(Json(execute(&state, &id, Some(&pid), false).await?))
}

pub fn router() -> OpenApiRouter<SharedState> {
    OpenApiRouter::new()
        .routes(routes!(list_plans, create_plan))
        .routes(routes!(update_plan, delete_plan))
        .routes(routes!(execute_plan))
}
