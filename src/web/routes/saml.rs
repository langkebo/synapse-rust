use crate::common::error::ApiError;
use crate::web::routes::AppState;
use crate::web::AuthenticatedUser;
use axum::{
    extract::{Path, Query, State},
    http::header,
    middleware,
    response::{IntoResponse, Redirect},
    Json,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct SamlLoginQuery {
    pub redirect_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SamlLoginResponse {
    pub redirect_url: String,
}

#[derive(Debug, Deserialize)]
pub struct SamlCallbackQuery {
    pub saml_response: Option<String>,
    pub saml_request: Option<String>,
    pub relay_state: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SamlCallbackBody {
    pub saml_response: Option<String>,
    pub saml_request: Option<String>,
    pub relay_state: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SamlAuthResult {
    pub user_id: String,
    pub access_token: String,
    pub device_id: String,
    pub expires_in: i64,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SamlMetadataResponse {
    pub entity_id: String,
    pub sso_url: String,
    pub slo_url: Option<String>,
    pub certificate: Option<String>,
}

pub async fn saml_login(
    State(state): State<AppState>,
    Query(query): Query<SamlLoginQuery>,
) -> Result<impl IntoResponse, ApiError> {
    if !state.services.sso.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let auth_request = state.services.sso.saml_service.get_auth_redirect(query.redirect_url.as_deref()).await?;

    Ok(Json(SamlLoginResponse { redirect_url: auth_request.redirect_url }))
}

pub async fn saml_login_redirect(
    State(state): State<AppState>,
    Query(query): Query<SamlLoginQuery>,
) -> Result<impl IntoResponse, ApiError> {
    if !state.services.sso.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let auth_request = state.services.sso.saml_service.get_auth_redirect(query.redirect_url.as_deref()).await?;

    Ok(Redirect::temporary(&auth_request.redirect_url))
}

pub async fn saml_callback_post(
    State(state): State<AppState>,
    Json(body): Json<SamlCallbackBody>,
) -> Result<impl IntoResponse, ApiError> {
    handle_saml_callback(&state, body.saml_response.as_deref(), body.relay_state.as_deref()).await
}

pub async fn saml_callback_get(
    State(state): State<AppState>,
    Query(query): Query<SamlCallbackQuery>,
) -> Result<impl IntoResponse, ApiError> {
    handle_saml_callback(&state, query.saml_response.as_deref(), query.relay_state.as_deref()).await
}

async fn handle_saml_callback(
    state: &AppState,
    saml_response: Option<&str>,
    relay_state: Option<&str>,
) -> Result<impl IntoResponse, ApiError> {
    if !state.services.sso.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let saml_response = saml_response.ok_or_else(|| ApiError::bad_request("Missing SAML response"))?;

    let auth_result =
        state.services.sso.saml_service.process_auth_response(saml_response, relay_state, None, None).await?;

    let user = state
        .services
        .account
        .account_identity_service
        .get_user_by_id(&auth_result.user_id)
        .await?
        .ok_or_else(|| ApiError::internal("User not found after SAML auth"))?;

    let device_id = format!("SAML_{}", uuid::Uuid::new_v4().as_simple());

    let access_token =
        state.services.core.auth_service.generate_access_token(&auth_result.user_id, &device_id, user.is_admin).await?;

    let expires_in = 3600_i64;

    let refresh_token = match state
        .services
        .core
        .auth_service
        .generate_refresh_token(&auth_result.user_id, &device_id)
        .await
    {
        Ok(token) => Some(token),
        Err(e) => {
            ::tracing::warn!(
                user_id = %auth_result.user_id,
                device_id = %device_id,
                error = %e,
                "Failed to generate refresh token after SAML login — access token is still valid but user will need to re-authenticate when it expires"
            );
            None
        }
    };

    Ok(Json(SamlAuthResult { user_id: auth_result.user_id, access_token, device_id, expires_in, refresh_token }))
}

pub async fn saml_logout(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    if !state.services.sso.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let mapping = state.services.sso.saml_service.get_user_mapping(&_auth_user.user_id).await?;

    if let Some(_mapping) = mapping {
        let sessions = state.services.sso.saml_service.get_session_by_user(&_auth_user.user_id).await?;

        if let Some(session) = sessions {
            let redirect_url = state
                .services
                .sso
                .saml_service
                .initiate_logout(&session.session_id, Some("User initiated logout"))
                .await?;

            return Ok(Json(serde_json::json!({
                "redirect_url": redirect_url
            })));
        }
    }

    Ok(Json(serde_json::json!({
        "message": "No active SAML session found"
    })))
}

pub async fn saml_logout_callback(
    State(state): State<AppState>,
    Query(query): Query<SamlCallbackQuery>,
) -> Result<impl IntoResponse, ApiError> {
    if !state.services.sso.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let saml_response = query.saml_response.ok_or_else(|| ApiError::bad_request("Missing SAML response"))?;

    state.services.sso.saml_service.process_logout_response(&saml_response).await?;

    Ok(Json(serde_json::json!({
        "message": "Logout successful"
    })))
}

pub async fn get_saml_metadata(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    if !state.services.sso.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let metadata = state.services.sso.saml_service.get_idp_metadata().await?;

    Ok(Json(SamlMetadataResponse {
        entity_id: metadata.entity_id,
        sso_url: metadata.sso_url,
        slo_url: metadata.slo_url,
        certificate: Some(metadata.certificate),
    }))
}

pub async fn get_sp_metadata(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    if !state.services.sso.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let config = state.services.sso.saml_service.get_config();
    let server_name = &state.services.core.server_name;

    let sp_entity_id = &config.sp_entity_id;
    let acs_url = config.get_sp_acs_url(server_name);
    let sls_url = config.get_sp_sls_url(server_name);

    let metadata = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<md:EntityDescriptor xmlns:md="urn:oasis:names:tc:SAML:2.0:metadata" entityID="{}">
    <md:SPSSODescriptor protocolSupportEnumeration="urn:oasis:names:tc:SAML:2.0:protocol">
        <md:AssertionConsumerService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST"
                                      Location="{}"
                                      index="1"/>
        {}
    </md:SPSSODescriptor>
</md:EntityDescriptor>"#,
        sp_entity_id,
        acs_url,
        sls_url
            .map(|url| format!(
                r#"<md:SingleLogoutService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect"
                                   Location="{}"/>"#,
                url
            ))
            .unwrap_or_default()
    );

    Ok(([(header::CONTENT_TYPE, "application/xml; charset=utf-8")], metadata))
}

pub async fn refresh_idp_metadata(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    if !state.services.sso.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let metadata = state.services.sso.saml_service.get_idp_metadata().await?;

    Ok(Json(SamlMetadataResponse {
        entity_id: metadata.entity_id,
        sso_url: metadata.sso_url,
        slo_url: metadata.slo_url,
        certificate: Some(metadata.certificate),
    }))
}

// ============================================================================
// Admin endpoints: SAML user-mapping CRUD + admin-initiated logout + runtime
// config (closes audit R2-SAML-01). All mounted under `/_synapse/admin/v1`
// and guarded by `admin_auth_middleware`.
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct SamlMappingListQuery {
    pub limit: Option<i64>,
    pub from: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SamlMappingView {
    pub name_id: String,
    pub user_id: String,
    pub issuer: String,
    pub first_seen_ts: i64,
    pub last_authenticated_ts: i64,
    pub authentication_count: i32,
    pub attributes: serde_json::Value,
}

impl From<crate::storage::saml::SamlUserMapping> for SamlMappingView {
    fn from(m: crate::storage::saml::SamlUserMapping) -> Self {
        Self {
            name_id: m.name_id,
            user_id: m.user_id,
            issuer: m.issuer,
            first_seen_ts: m.first_seen_ts,
            last_authenticated_ts: m.last_authenticated_ts,
            authentication_count: m.authentication_count,
            attributes: m.attributes,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SamlMappingPage {
    pub mappings: Vec<SamlMappingView>,
    pub next_token: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSamlMappingBody {
    pub user_id: Option<String>,
    pub attributes: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct SamlLogoutAdminBody {
    pub user_id: String,
}

#[derive(Debug, Serialize)]
pub struct SamlLogoutAdminResponse {
    pub user_id: String,
    pub redirect_url: Option<String>,
    pub sessions_invalidated: u32,
}

pub async fn list_saml_mappings_admin(
    State(state): State<AppState>,
    Query(query): Query<SamlMappingListQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 500);
    let rows = state.services.sso.saml_service.list_user_mappings(limit, query.from.as_deref()).await?;

    let next_token = if rows.len() as i64 == limit { rows.last().map(|r| r.name_id.clone()) } else { None };

    Ok(Json(SamlMappingPage { mappings: rows.into_iter().map(SamlMappingView::from).collect(), next_token }))
}

pub async fn get_saml_mapping_admin(
    State(state): State<AppState>,
    Path(name_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let row = state
        .services
        .sso
        .saml_service
        .get_user_mapping_any_issuer(&name_id)
        .await?
        .ok_or_else(|| ApiError::not_found("SAML user mapping not found"))?;
    Ok(Json(SamlMappingView::from(row)))
}

pub async fn update_saml_mapping_admin(
    State(state): State<AppState>,
    Path(name_id): Path<String>,
    Json(body): Json<UpdateSamlMappingBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.user_id.is_none() && body.attributes.is_none() {
        return Err(ApiError::bad_request("At least one of user_id / attributes must be provided"));
    }
    let row = state
        .services
        .sso
        .saml_service
        .update_user_mapping_by_name_id(&name_id, body.user_id.as_deref(), body.attributes.as_ref())
        .await?
        .ok_or_else(|| ApiError::not_found("SAML user mapping not found"))?;
    Ok(Json(SamlMappingView::from(row)))
}

pub async fn delete_saml_mapping_admin(
    State(state): State<AppState>,
    Path(name_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let removed = state.services.sso.saml_service.delete_user_mapping_by_name_id(&name_id).await?;
    if removed == 0 {
        return Err(ApiError::not_found("SAML user mapping not found"));
    }
    Ok(Json(serde_json::json!({ "removed": removed })))
}

pub async fn saml_logout_admin(
    State(state): State<AppState>,
    Json(body): Json<SamlLogoutAdminBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.user_id.is_empty() {
        return Err(ApiError::bad_request("user_id is required"));
    }
    if !state.services.sso.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let session = state.services.sso.saml_service.get_session_by_user(&body.user_id).await?;

    let Some(session) = session else {
        return Ok(Json(SamlLogoutAdminResponse {
            user_id: body.user_id,
            redirect_url: None,
            sessions_invalidated: 0,
        }));
    };

    let redirect_url =
        state.services.sso.saml_service.initiate_logout(&session.session_id, Some("Admin initiated logout")).await.ok();

    Ok(Json(SamlLogoutAdminResponse { user_id: body.user_id, redirect_url, sessions_invalidated: 1 }))
}

pub async fn get_saml_admin_config(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(state.services.sso.saml_service.effective_config()))
}

pub async fn update_saml_admin_config(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    let merged = state.services.sso.saml_service.apply_runtime_overrides(body).await?;
    Ok(Json(merged))
}

pub fn create_saml_router(state: AppState) -> axum::Router<AppState> {
    use axum::routing::*;

    let public_routes = axum::Router::new()
        .route("/_matrix/client/r0/login/sso/redirect/saml", get(saml_login_redirect))
        .route("/_matrix/client/r0/login/sso/redirect/saml", post(saml_login))
        .route("/_matrix/client/r0/login/saml/callback", get(saml_callback_get))
        .route("/_matrix/client/r0/login/saml/callback", post(saml_callback_post))
        .route("/_matrix/client/r0/logout/saml", get(saml_logout))
        .route("/_matrix/client/r0/logout/saml/callback", get(saml_logout_callback))
        .route("/_matrix/client/r0/saml/metadata", get(get_saml_metadata))
        .route("/_matrix/client/r0/saml/sp_metadata", get(get_sp_metadata))
        .route("/_matrix/client/v3/login/sso/redirect/saml", get(saml_login_redirect))
        .route("/_matrix/client/v3/login/saml/callback", get(saml_callback_get))
        .route("/_matrix/client/v3/login/saml/callback", post(saml_callback_post))
        .route("/_matrix/client/v3/saml/metadata", get(get_saml_metadata))
        .route("/_matrix/client/v3/saml/sp_metadata", get(get_sp_metadata));

    let admin_routes = axum::Router::new()
        .route("/_synapse/admin/v1/saml/metadata/refresh", post(refresh_idp_metadata))
        .route("/_synapse/admin/v1/saml/config", get(get_saml_admin_config).put(update_saml_admin_config))
        .route("/_synapse/admin/v1/saml/mappings", get(list_saml_mappings_admin))
        .route(
            "/_synapse/admin/v1/saml/mapping/{name_id}",
            get(get_saml_mapping_admin).put(update_saml_mapping_admin).delete(delete_saml_mapping_admin),
        )
        .route("/_synapse/admin/v1/saml/logout", post(saml_logout_admin))
        .route_layer(middleware::from_fn_with_state(state.clone(), crate::web::middleware::admin_auth_middleware));

    public_routes.merge(admin_routes).with_state(state)
}

pub fn saml_route_manifest() -> Vec<crate::web::routes::route_ledger::RouteEntry> {
    use crate::web::routes::route_ledger::RouteEntry;
    use axum::http::Method;

    [
        (Method::GET, "/_matrix/client/r0/login/sso/redirect/saml"),
        (Method::POST, "/_matrix/client/r0/login/sso/redirect/saml"),
        (Method::GET, "/_matrix/client/r0/login/saml/callback"),
        (Method::POST, "/_matrix/client/r0/login/saml/callback"),
        (Method::GET, "/_matrix/client/r0/logout/saml"),
        (Method::GET, "/_matrix/client/r0/logout/saml/callback"),
        (Method::GET, "/_matrix/client/r0/saml/metadata"),
        (Method::GET, "/_matrix/client/r0/saml/sp_metadata"),
        (Method::POST, "/_synapse/admin/v1/saml/metadata/refresh"),
        (Method::GET, "/_synapse/admin/v1/saml/config"),
        (Method::PUT, "/_synapse/admin/v1/saml/config"),
        (Method::GET, "/_synapse/admin/v1/saml/mappings"),
        (Method::GET, "/_synapse/admin/v1/saml/mapping/{name_id}"),
        (Method::PUT, "/_synapse/admin/v1/saml/mapping/{name_id}"),
        (Method::DELETE, "/_synapse/admin/v1/saml/mapping/{name_id}"),
        (Method::POST, "/_synapse/admin/v1/saml/logout"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, "saml"))
    .collect()
}
