//! Editing of watchlists and taxonomies.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use pp_core::model::{Assignment, Classification, Watchlist};
use serde::Deserialize;
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::state::SharedState;

#[derive(Deserialize, ToSchema)]
pub struct WatchlistInput {
    pub name: String,
    pub securities: Vec<String>,
}

/// Replaces all watchlists of a client.
#[utoipa::path(put, path = "/clients/{id}/watchlists", tag = "data", params(("id" = String, Path)), request_body = Vec<WatchlistInput>, responses((status = 204)))]
async fn put_watchlists(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(input): Json<Vec<WatchlistInput>>,
) -> ApiResult<StatusCode> {
    let c = state.client(&user, &id).await?;
    let lists: Vec<Watchlist> = input
        .into_iter()
        .filter(|w| !w.name.trim().is_empty())
        .map(|w| Watchlist {
            name: w.name.trim().into(),
            securities: w
                .securities
                .into_iter()
                .filter(|s| c.client.security(s).is_some())
                .collect(),
        })
        .collect();
    state.db.set_watchlists(&id, &lists).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize, ToSchema)]
pub struct AssignmentInput {
    /// security or account id
    pub vehicle: String,
    /// basis 10000 = 100 %
    pub weight: i32,
}

#[derive(Deserialize, ToSchema)]
pub struct ClassificationInput {
    pub name: String,
    pub color: Option<String>,
    pub note: Option<String>,
    /// target weight within the parent (basis 10000)
    pub weight: i32,
    /// for new classifications
    pub parent_id: Option<String>,
    pub assignments: Option<Vec<AssignmentInput>>,
}

fn check(i: &ClassificationInput) -> ApiResult<()> {
    if i.name.trim().is_empty() || !(0..=10000).contains(&i.weight) {
        return Err(ApiError::BadRequest(
            "Name und Gewicht (0–100 %) erforderlich".into(),
        ));
    }
    Ok(())
}

#[utoipa::path(put, path = "/clients/{id}/taxonomies/{tid}/classifications/{cid}", tag = "data",
    params(("id" = String, Path), ("tid" = String, Path), ("cid" = String, Path)), request_body = ClassificationInput, responses((status = 204)))]
async fn update_classification(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, tid, cid)): Path<(String, String, String)>,
    Json(input): Json<ClassificationInput>,
) -> ApiResult<StatusCode> {
    check(&input)?;
    let c = state.client(&user, &id).await?;
    let t = c
        .client
        .taxonomies
        .iter()
        .find(|t| t.id == tid)
        .ok_or(ApiError::NotFound)?;
    let mut cl = t
        .classifications
        .iter()
        .find(|x| x.id == cid)
        .cloned()
        .ok_or(ApiError::NotFound)?;
    cl.name = input.name.trim().into();
    cl.color = input.color;
    cl.note = input.note.filter(|n| !n.trim().is_empty());
    cl.weight = input.weight;
    if let Some(a) = input.assignments {
        cl.assignments = a
            .into_iter()
            .filter(|x| {
                c.client.security(&x.vehicle).is_some() || c.client.account(&x.vehicle).is_some()
            })
            .enumerate()
            .map(|(rank, x)| Assignment {
                vehicle: x.vehicle,
                weight: x.weight.clamp(0, 10000),
                rank: rank as i32,
            })
            .collect();
    }
    state.db.update_classification(&id, &tid, &cl).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/clients/{id}/taxonomies/{tid}/classifications", tag = "data",
    params(("id" = String, Path), ("tid" = String, Path)), request_body = ClassificationInput, responses((status = 201, body = String)))]
async fn create_classification(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, tid)): Path<(String, String)>,
    Json(input): Json<ClassificationInput>,
) -> ApiResult<(StatusCode, Json<String>)> {
    check(&input)?;
    let c = state.client(&user, &id).await?;
    let t = c
        .client
        .taxonomies
        .iter()
        .find(|t| t.id == tid)
        .ok_or(ApiError::NotFound)?;
    let parent = input
        .parent_id
        .clone()
        .or_else(|| {
            t.classifications
                .iter()
                .find(|x| x.parent_id.is_none())
                .map(|x| x.id.clone())
        })
        .ok_or_else(|| ApiError::BadRequest("Übergeordnete Kategorie fehlt".into()))?;
    if !t.classifications.iter().any(|x| x.id == parent) {
        return Err(ApiError::BadRequest(
            "Übergeordnete Kategorie unbekannt".into(),
        ));
    }
    let cl = Classification {
        id: uuid::Uuid::new_v4().to_string(),
        parent_id: Some(parent.clone()),
        name: input.name.trim().into(),
        note: input.note,
        color: input.color,
        weight: input.weight,
        rank: t
            .classifications
            .iter()
            .filter(|x| x.parent_id.as_deref() == Some(parent.as_str()))
            .count() as i32,
        assignments: vec![],
        data: serde_json::Value::Null,
    };
    state.db.insert_classification(&id, &tid, &cl).await?;
    Ok((StatusCode::CREATED, Json(cl.id)))
}

#[utoipa::path(delete, path = "/clients/{id}/taxonomies/{tid}/classifications/{cid}", tag = "data",
    params(("id" = String, Path), ("tid" = String, Path), ("cid" = String, Path)), responses((status = 204), (status = 409)))]
async fn delete_classification(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path((id, tid, cid)): Path<(String, String, String)>,
) -> ApiResult<StatusCode> {
    state.client(&user, &id).await?;
    state.db.delete_classification(&id, &tid, &cid).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub fn router() -> OpenApiRouter<SharedState> {
    OpenApiRouter::new()
        .routes(routes!(put_watchlists))
        .routes(routes!(create_classification))
        .routes(routes!(update_classification, delete_classification))
}
