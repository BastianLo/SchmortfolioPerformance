//! Clients (one per PP file), import and demo data.

use axum::Json;
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

use super::dto::{ClientDto, Counts};
use crate::auth::CurrentUser;
use crate::error::{ApiError, ApiResult};
use crate::state::SharedState;

#[utoipa::path(get, path = "/clients", tag = "clients", responses((status = 200, body = Vec<ClientDto>)))]
async fn list_clients(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<Json<Vec<ClientDto>>> {
    Ok(Json(
        state
            .db
            .list_clients(&user.id)
            .await?
            .iter()
            .map(ClientDto::from)
            .collect(),
    ))
}

#[derive(Deserialize, ToSchema)]
pub struct CreateClient {
    pub name: String,
    #[serde(default = "eur")]
    pub base_currency: String,
}

fn eur() -> String {
    "EUR".into()
}

#[utoipa::path(post, path = "/clients", tag = "clients", request_body = CreateClient, responses((status = 201, body = ClientDto)))]
async fn create_client(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Json(req): Json<CreateClient>,
) -> ApiResult<(StatusCode, Json<ClientDto>)> {
    if req.name.trim().is_empty() {
        return Err(ApiError::BadRequest("Name fehlt".into()));
    }
    let client = pp_core::Client {
        version: pp_import::MAX_SUPPORTED_VERSION,
        base_currency: req.base_currency,
        ..Default::default()
    };
    let meta = state
        .db
        .insert_client(&user.id, req.name.trim(), &client)
        .await?;
    Ok((StatusCode::CREATED, Json(ClientDto::from(&meta))))
}

#[derive(Serialize, ToSchema)]
pub struct ClientDetail {
    #[serde(flatten)]
    pub client: ClientDto,
    pub counts: Counts,
    pub first_transaction: Option<chrono::NaiveDate>,
    pub currencies: Vec<String>,
    pub missing_exchange_rates: Vec<String>,
}

#[utoipa::path(get, path = "/clients/{id}", tag = "clients", params(("id" = String, Path)), responses((status = 200, body = ClientDetail)))]
async fn get_client(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<Json<ClientDetail>> {
    let c = state.client(&user, &id).await?;
    let mut currencies: Vec<String> = c
        .client
        .accounts
        .iter()
        .map(|a| a.currency.clone())
        .chain(
            c.client
                .securities
                .iter()
                .filter_map(|s| s.currency.clone()),
        )
        .collect();
    currencies.sort();
    currencies.dedup();
    let conv = c.converter();
    let probe = super::dto::today();
    let missing = currencies
        .iter()
        .filter(|cur| {
            **cur != c.client.base_currency
                && c.provider
                    .rate(probe, cur, &c.client.base_currency)
                    .is_none()
        })
        .map(|cur| format!("{cur}/{}", conv.term()))
        .collect();
    Ok(Json(ClientDetail {
        client: ClientDto::from(&c.meta),
        counts: Counts::of(&c.client),
        first_transaction: super::dto::first_transaction_date(&c.client),
        currencies,
        missing_exchange_rates: missing,
    }))
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateClient {
    pub name: String,
}

#[utoipa::path(patch, path = "/clients/{id}", tag = "clients", params(("id" = String, Path)), request_body = UpdateClient, responses((status = 200, body = ClientDto)))]
async fn update_client(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
    Json(req): Json<UpdateClient>,
) -> ApiResult<Json<ClientDto>> {
    state.client(&user, &id).await?;
    state.db.rename_client(&id, req.name.trim()).await?;
    state.invalidate(&id).await;
    Ok(Json(ClientDto::from(&state.db.client_meta(&id).await?)))
}

#[utoipa::path(delete, path = "/clients/{id}", tag = "clients", params(("id" = String, Path)), responses((status = 204)))]
async fn delete_client(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    state.client(&user, &id).await?;
    state.db.delete_client(&id).await?;
    state.invalidate(&id).await;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(ToSchema)]
#[allow(dead_code)]
pub struct ImportForm {
    /// PP file (.xml, .portfolio, .zip)
    #[schema(value_type = String, format = Binary)]
    pub file: Vec<u8>,
    /// password for encrypted files
    pub password: Option<String>,
    /// name of the new client (default: file name)
    pub name: Option<String>,
    /// only analyse the file, do not store it
    pub dry_run: Option<bool>,
}

#[derive(Serialize, ToSchema)]
pub struct ImportResponse {
    /// created client (absent for dry runs)
    pub client: Option<ClientDto>,
    pub format: String,
    pub encrypted: bool,
    pub compressed: bool,
    pub file_version: i32,
    pub base_currency: String,
    pub counts: Counts,
    pub warnings: Vec<String>,
}

/// Imports a Portfolio Performance file as new client.
#[utoipa::path(post, path = "/clients/import", tag = "clients",
    request_body(content = ImportForm, content_type = "multipart/form-data"),
    responses((status = 200, body = ImportResponse), (status = 400), (status = 422, description = "password required or wrong")))]
async fn import_client(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    mut multipart: Multipart,
) -> ApiResult<(StatusCode, Json<ImportResponse>)> {
    let mut file: Option<(String, Vec<u8>)> = None;
    let mut password = None;
    let mut name = None;
    let mut dry_run = false;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(e.to_string()))?
    {
        match field.name().unwrap_or_default() {
            "file" => {
                let fname = field.file_name().unwrap_or("Import").to_string();
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::BadRequest(e.to_string()))?;
                file = Some((fname, bytes.to_vec()));
            }
            "password" => password = Some(field.text().await.unwrap_or_default()),
            "name" => {
                name = Some(field.text().await.unwrap_or_default()).filter(|n| !n.trim().is_empty())
            }
            "dry_run" => {
                dry_run = field
                    .text()
                    .await
                    .map(|t| t == "true" || t == "1")
                    .unwrap_or(false)
            }
            _ => {}
        }
    }
    let (fname, bytes) = file.ok_or_else(|| ApiError::BadRequest("Datei fehlt".into()))?;

    let result =
        tokio::task::spawn_blocking(move || pp_import::read_client(&bytes, password.as_deref()))
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
    let result = match result {
        Ok(r) => r,
        Err(
            e @ (pp_import::ImportError::PasswordRequired | pp_import::ImportError::WrongPassword),
        ) => {
            return Err(ApiError::BadRequest(e.to_string()));
        }
        Err(e) => return Err(ApiError::BadRequest(e.to_string())),
    };

    let client_dto = if dry_run {
        None
    } else {
        let default_name = fname
            .rsplit_once('.')
            .map(|(n, _)| n.to_string())
            .unwrap_or(fname);
        let meta = state
            .db
            .insert_client(
                &user.id,
                name.as_deref().unwrap_or(&default_name),
                &result.client,
            )
            .await?;
        Some(ClientDto::from(&meta))
    };

    let info = &result.info;
    Ok((
        if dry_run {
            StatusCode::OK
        } else {
            StatusCode::CREATED
        },
        Json(ImportResponse {
            client: client_dto,
            format: format!("{:?}", info.format).to_lowercase(),
            encrypted: info.encrypted,
            compressed: info.compressed,
            file_version: info.version,
            base_currency: result.client.base_currency.clone(),
            counts: Counts::of(&result.client),
            warnings: info.warnings.clone(),
        }),
    ))
}

/// Creates a demo client with generated data.
#[utoipa::path(post, path = "/clients/demo", tag = "clients", responses((status = 201, body = ClientDto)))]
async fn create_demo(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
) -> ApiResult<(StatusCode, Json<ClientDto>)> {
    let client = crate::demo::generate(super::dto::today());
    let meta = state
        .db
        .insert_client(&user.id, "Demo-Portfolio", &client)
        .await?;
    Ok((StatusCode::CREATED, Json(ClientDto::from(&meta))))
}

/// Exports the client as Portfolio Performance XML file.
#[utoipa::path(get, path = "/clients/{id}/export/pp-xml", tag = "clients", params(("id" = String, Path)), responses((status = 200, content_type = "application/xml")))]
async fn export_pp_xml(
    State(state): State<SharedState>,
    CurrentUser(user): CurrentUser,
    Path(id): Path<String>,
) -> ApiResult<axum::response::Response> {
    let c = state.client(&user, &id).await?;
    let xml = tokio::task::spawn_blocking(move || pp_import::export::to_pp_xml(&c.client))
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;
    let meta = state.db.client_meta(&id).await?;
    let name: String = meta.name.chars().filter(|c| c.is_alphanumeric() || matches!(c, '-' | '_' | ' ')).collect();
    Ok(axum::response::Response::builder()
        .header("content-type", "application/xml; charset=utf-8")
        .header("content-disposition", format!("attachment; filename=\"{}.xml\"", name.trim()))
        .body(axum::body::Body::from(xml))
        .unwrap())
}

pub fn router() -> OpenApiRouter<SharedState> {
    OpenApiRouter::new()
        .routes(routes!(list_clients, create_client))
        .routes(routes!(import_client))
        .routes(routes!(create_demo))
        .routes(routes!(export_pp_xml))
        .routes(routes!(get_client, update_client, delete_client))
}
