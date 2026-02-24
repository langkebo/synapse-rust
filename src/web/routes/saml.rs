use crate::common::error::ApiError;
use crate::web::routes::AppState;
use crate::web::routes::AuthenticatedUser;
use axum::{
    extract::{Query, State},
    response::{Html, IntoResponse, Redirect},
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
    if !state.services.saml_service.is_enabled() {
        return Err(ApiError::bad_request("SAML authentication is not enabled"));
    }

    let auth_request = state
        .services
        .saml_service
        .get_auth_redirect(query.redirect_url.as_deref())
        .await?;

    Ok(Json(SamlLoginResponse {
        redirect_url: auth_request.redirect_url,
    }))
}

pub async fn saml_login_redirect(
    State(state): State<AppState>,
    Query(query): Query<SamlLoginQuery>,
) -> Result<impl IntoResponse, ApiError> {
    if !state.services.saml_service.is_enabled() {
        return Err(ApiError::bad_request("SAML authentication is not enabled"));
    }

    let auth_request = state
        .services
        .saml_service
        .get_auth_redirect(query.redirect_url.as_deref())
        .await?;

    Ok(Redirect::temporary(&auth_request.redirect_url))
}

pub async fn saml_callback_post(
    State(state): State<AppState>,
    Json(body): Json<SamlCallbackBody>,
) -> Result<impl IntoResponse, ApiError> {
    handle_saml_callback(
        &state,
        body.saml_response.as_deref(),
        body.relay_state.as_deref(),
    )
    .await
}

pub async fn saml_callback_get(
    State(state): State<AppState>,
    Query(query): Query<SamlCallbackQuery>,
) -> Result<impl IntoResponse, ApiError> {
    handle_saml_callback(
        &state,
        query.saml_response.as_deref(),
        query.relay_state.as_deref(),
    )
    .await
}

async fn handle_saml_callback(
    state: &AppState,
    saml_response: Option<&str>,
    relay_state: Option<&str>,
) -> Result<impl IntoResponse, ApiError> {
    if !state.services.saml_service.is_enabled() {
        return Err(ApiError::bad_request("SAML authentication is not enabled"));
    }

    let saml_response =
        saml_response.ok_or_else(|| ApiError::bad_request("Missing SAML response"))?;

    let auth_result = state
        .services
        .saml_service
        .process_auth_response(saml_response, relay_state, None, None)
        .await?;

    let user = state
        .services
        .user_storage
        .get_user_by_id(&auth_result.user_id)
        .await?
        .ok_or_else(|| ApiError::internal("User not found after SAML auth"))?;

    let device_id = "SAML_DEVICE".to_string();

    let access_token = state
        .services
        .auth_service
        .generate_access_token(
            &auth_result.user_id,
            &device_id,
            user.is_admin.unwrap_or(false),
        )
        .await?;

    let expires_in = 3600_i64;

    Ok(Json(SamlAuthResult {
        user_id: auth_result.user_id,
        access_token,
        device_id,
        expires_in,
        refresh_token: None,
    }))
}

pub async fn saml_logout(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    if !state.services.saml_service.is_enabled() {
        return Err(ApiError::bad_request("SAML authentication is not enabled"));
    }

    let mapping = state
        .services
        .saml_service
        .get_user_mapping(&_auth_user.user_id)
        .await?;

    if let Some(_mapping) = mapping {
        let sessions = state
            .services
            .saml_storage
            .get_session_by_user(&_auth_user.user_id)
            .await?;

        if let Some(session) = sessions {
            let redirect_url = state
                .services
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
    if !state.services.saml_service.is_enabled() {
        return Err(ApiError::bad_request("SAML authentication is not enabled"));
    }

    let saml_response = query
        .saml_response
        .ok_or_else(|| ApiError::bad_request("Missing SAML response"))?;

    state
        .services
        .saml_service
        .process_logout_response(&saml_response)
        .await?;

    Ok(Json(serde_json::json!({
        "message": "Logout successful"
    })))
}

pub async fn get_saml_metadata(
    State(state): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    if !state.services.saml_service.is_enabled() {
        return Err(ApiError::bad_request("SAML authentication is not enabled"));
    }

    let metadata = state.services.saml_service.get_idp_metadata().await?;

    Ok(Json(SamlMetadataResponse {
        entity_id: metadata.entity_id,
        sso_url: metadata.sso_url,
        slo_url: metadata.slo_url,
        certificate: Some(metadata.certificate),
    }))
}

pub async fn get_sp_metadata(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    if !state.services.saml_service.is_enabled() {
        return Err(ApiError::bad_request("SAML authentication is not enabled"));
    }

    let config = state.services.saml_service.get_config();
    let server_name = &state.services.server_name;

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
        sls_url.map(|url| format!(
            r#"<md:SingleLogoutService Binding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-Redirect"
                                   Location="{}"/>"#,
            url
        )).unwrap_or_default()
    );

    Ok(Html(metadata))
}

pub async fn refresh_idp_metadata(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    if !state.services.saml_service.is_enabled() {
        return Err(ApiError::bad_request("SAML authentication is not enabled"));
    }

    let metadata = state.services.saml_service.get_idp_metadata().await?;

    Ok(Json(SamlMetadataResponse {
        entity_id: metadata.entity_id,
        sso_url: metadata.sso_url,
        slo_url: metadata.slo_url,
        certificate: Some(metadata.certificate),
    }))
}

pub fn create_saml_router() -> axum::Router<AppState> {
    use axum::routing::*;

    axum::Router::new()
        .route(
            "/_matrix/client/r0/login/sso/redirect/saml",
            get(saml_login_redirect),
        )
        .route(
            "/_matrix/client/r0/login/sso/redirect/saml",
            post(saml_login),
        )
        .route(
            "/_matrix/client/r0/login/saml/callback",
            get(saml_callback_get),
        )
        .route(
            "/_matrix/client/r0/login/saml/callback",
            post(saml_callback_post),
        )
        .route("/_matrix/client/r0/logout/saml", get(saml_logout))
        .route(
            "/_matrix/client/r0/logout/saml/callback",
            get(saml_logout_callback),
        )
        .route("/_matrix/client/r0/saml/metadata", get(get_saml_metadata))
        .route("/_matrix/client/r0/saml/sp_metadata", get(get_sp_metadata))
        .route(
            "/_synapse/admin/v1/saml/metadata/refresh",
            post(refresh_idp_metadata),
        )
}
