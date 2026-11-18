//! Administration: users and backups (admins only).

use std::path::PathBuf;

use axum::Json;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use pp_db::NewUser;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use crate::auth::{CurrentUser, Me, hash_password};
use crate::error::{ApiError, ApiResult};
use crate::state::{AppState, SharedState};

fn admin(u: &pp_db::User) -> ApiResult<()> {
    if u.is_admin {
        Ok(())
    } else {
        Err(ApiError::Forbidden)
    }
}

#[utoipa::path(get, path = "/admin/users", tag = "system", responses((status = 200, body = Vec<Me>)))]
async fn list_users(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<Vec<Me>>> {
    admin(&user)?;
    Ok(Json(
        state.db.list_users().await?.iter().map(Me::from).collect(),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct UserInput {
    pub username: String,
    pub password: String,
    #[serde(default)]
    pub is_admin: bool,
}

#[utoipa::path(post, path = "/admin/users", tag = "system", request_body = UserInput, responses((status = 201, body = Me), (status = 409)))]
async fn create_user(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Json(u): Json<UserInput>,
) -> ApiResult<(StatusCode, Json<Me>)> {
    admin(&user)?;
    if u.username.trim().len() < 2 || u.password.len() < 8 {
        return Err(ApiError::BadRequest(
            "Benutzername (min. 2) und Passwort (min. 8 Zeichen) erforderlich".into(),
        ));
    }
    if state
        .db
        .user_by_username(u.username.trim())
        .await?
        .is_some()
    {
        return Err(ApiError::Conflict("Benutzername bereits vergeben".into()));
    }
    let hash = hash_password(&u.password)?;
    let created = state
        .db
        .create_user(NewUser {
            username: u.username.trim(),
            email: None,
            display_name: None,
            password_hash: Some(&hash),
            is_admin: u.is_admin,
            oidc_issuer: None,
            oidc_subject: None,
        })
        .await?;
    Ok((StatusCode::CREATED, Json(Me::from(&created))))
}

#[derive(Deserialize, ToSchema)]
pub struct UserUpdate {
    pub is_admin: Option<bool>,
    /// sets a new password and ends all sessions of the user
    pub password: Option<String>,
}

#[utoipa::path(put, path = "/admin/users/{uid}", tag = "system", params(("uid" = String, Path)), request_body = UserUpdate, responses((status = 204)))]
async fn update_user(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(uid): Path<String>,
    Json(u): Json<UserUpdate>,
) -> ApiResult<StatusCode> {
    admin(&user)?;
    state.db.user(&uid).await?;
    if let Some(a) = u.is_admin {
        if uid == user.id && !a {
            return Err(ApiError::Conflict(
                "Eigene Admin-Rechte können nicht entzogen werden".into(),
            ));
        }
        state.db.set_admin(&uid, a).await?;
    }
    if let Some(p) = u.password {
        if p.len() < 8 {
            return Err(ApiError::BadRequest(
                "Passwort muss mindestens 8 Zeichen haben".into(),
            ));
        }
        state.db.set_password(&uid, &hash_password(&p)?).await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(delete, path = "/admin/users/{uid}", tag = "system", params(("uid" = String, Path)), responses((status = 204)))]
async fn delete_user(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(uid): Path<String>,
) -> ApiResult<StatusCode> {
    admin(&user)?;
    if uid == user.id {
        return Err(ApiError::Conflict(
            "Der eigene Benutzer kann nicht gelöscht werden".into(),
        ));
    }
    state.db.delete_user(&uid).await?;
    Ok(StatusCode::NO_CONTENT)
}

// ------------------------------------------------------------------ backups

pub fn backup_dir(state: &AppState) -> PathBuf {
    PathBuf::from(&state.config.data_dir).join("backups")
}

/// Creates a backup and removes old ones (keeps `PP_BACKUP_KEEP`).
pub async fn create_backup(state: &AppState) -> anyhow::Result<String> {
    let dir = backup_dir(state);
    tokio::fs::create_dir_all(&dir).await?;
    let name = format!(
        "schmortfolio-{}.db",
        chrono::Local::now().format("%Y%m%d-%H%M%S")
    );
    let path = dir.join(&name);
    state.db.backup_to(&path.to_string_lossy()).await?;
    let mut list = list(state).await?;
    while list.len() > state.config.backup_keep {
        let old = list.pop().unwrap();
        let _ = tokio::fs::remove_file(dir.join(old.name)).await;
    }
    Ok(name)
}

#[derive(Serialize, ToSchema)]
pub struct BackupInfo {
    pub name: String,
    pub size: u64,
    pub created: String,
}

async fn list(state: &AppState) -> anyhow::Result<Vec<BackupInfo>> {
    let mut out = Vec::new();
    let Ok(mut rd) = tokio::fs::read_dir(backup_dir(state)).await else {
        return Ok(out);
    };
    while let Some(e) = rd.next_entry().await? {
        let name = e.file_name().to_string_lossy().to_string();
        if !name.starts_with("schmortfolio-") || !name.ends_with(".db") {
            continue;
        }
        let m = e.metadata().await?;
        let created: chrono::DateTime<chrono::Utc> = m.modified()?.into();
        out.push(BackupInfo {
            name,
            size: m.len(),
            created: created.to_rfc3339(),
        });
    }
    out.sort_by(|a, b| b.name.cmp(&a.name));
    Ok(out)
}

/// Whether a new automatic backup is due (last one older than a day).
pub async fn backup_due(state: &AppState) -> bool {
    match list(state).await {
        Ok(l) => l
            .first()
            .and_then(|b| chrono::DateTime::parse_from_rfc3339(&b.created).ok())
            .is_none_or(|t| {
                chrono::Utc::now().signed_duration_since(t) > chrono::Duration::hours(23)
            }),
        Err(_) => true,
    }
}

#[utoipa::path(get, path = "/admin/backups", tag = "system", responses((status = 200, body = Vec<BackupInfo>)))]
async fn list_backups(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<Vec<BackupInfo>>> {
    admin(&user)?;
    Ok(Json(list(&state).await?))
}

#[utoipa::path(post, path = "/admin/backups", tag = "system", responses((status = 201, body = String)))]
async fn post_backup(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<(StatusCode, Json<String>)> {
    admin(&user)?;
    Ok((StatusCode::CREATED, Json(create_backup(&state).await?)))
}

#[utoipa::path(get, path = "/admin/backups/{name}", tag = "system", params(("name" = String, Path)), responses((status = 200, content_type = "application/octet-stream")))]
async fn download_backup(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(name): Path<String>,
) -> ApiResult<Response> {
    admin(&user)?;
    if !name.starts_with("schmortfolio-")
        || !name.ends_with(".db")
        || name.contains('/')
        || name.contains("..")
    {
        return Err(ApiError::NotFound);
    }
    let bytes = tokio::fs::read(backup_dir(&state).join(&name))
        .await
        .map_err(|_| ApiError::NotFound)?;
    Ok(Response::builder()
        .header("content-type", "application/octet-stream")
        .header(
            "content-disposition",
            format!("attachment; filename=\"{name}\""),
        )
        .body(Body::from(bytes))
        .unwrap())
}

pub fn router() -> OpenApiRouter<SharedState> {
    OpenApiRouter::new()
        .routes(routes!(list_users, create_user))
        .routes(routes!(update_user, delete_user))
        .routes(routes!(list_backups, post_backup))
        .routes(routes!(download_backup))
}
