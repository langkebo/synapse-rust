use crate::common::error::ApiError;
use crate::web::routes::context::SsoContext;
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

/// The `SamlLoginQuery` struct.
///
/// Accepts the Matrix SSO spec-canonical `redirectUrl` plus a `redirect_url`
/// alias for tolerance; the Rust field stays snake_case.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamlLoginQuery {
    /// The `redirect_url` field.
    #[serde(rename = "redirectUrl", alias = "redirect_url")]
    pub redirect_url: Option<String>,
}

/// The `SamlLoginBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamlLoginBody {
    /// The `redirectUrl` field.
    #[serde(rename = "redirectUrl", alias = "redirect_url")]
    pub redirect_url: Option<String>,
}

/// The `SamlLoginResponse` struct.
#[derive(Debug, Serialize)]
pub struct SamlLoginResponse {
    /// The `redirect_url` field.
    pub redirect_url: String,
}

/// The `SamlCallbackQuery` struct.
///
/// SAML 标准 POST/Redirect 绑定使用 `SAMLResponse`、`SAMLRequest` 与
/// `RelayState` 的 PascalCase 字段名（真实 IdP 回调与 SDK 均采用此形状）。
/// 保留 `saml_response`/`saml_request`/`relay_state` 作为别名以兼容既有调用。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamlCallbackQuery {
    /// The `SAMLResponse` field.
    #[serde(rename = "SAMLResponse", alias = "saml_response")]
    pub saml_response: Option<String>,
    /// The `SAMLRequest` field.
    #[serde(rename = "SAMLRequest", alias = "saml_request")]
    pub saml_request: Option<String>,
    /// The `RelayState` field.
    #[serde(rename = "RelayState", alias = "relay_state")]
    pub relay_state: Option<String>,
}

/// The `SamlCallbackBody` struct.
///
/// 与 [`SamlCallbackQuery`] 同理：标准 SAML 字段名为 PascalCase，同时保留
/// snake_case 别名以兼容历史调用方。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamlCallbackBody {
    /// The `SAMLResponse` field.
    #[serde(rename = "SAMLResponse", alias = "saml_response")]
    pub saml_response: Option<String>,
    /// The `SAMLRequest` field.
    #[serde(rename = "SAMLRequest", alias = "saml_request")]
    pub saml_request: Option<String>,
    /// The `RelayState` field.
    #[serde(rename = "RelayState", alias = "relay_state")]
    pub relay_state: Option<String>,
}

/// The `SamlAuthResult` struct.
#[derive(Debug, Serialize)]
pub struct SamlAuthResult {
    /// The `user_id` field.
    pub user_id: String,
    /// The `access_token` field.
    pub access_token: String,
    /// The `device_id` field.
    pub device_id: String,
    /// The `expires_in` field.
    pub expires_in: i64,
    /// The `refresh_token` field.
    pub refresh_token: Option<String>,
}

/// The `SamlMetadataResponse` struct.
#[derive(Debug, Serialize)]
pub struct SamlMetadataResponse {
    /// The `entity_id` field.
    pub entity_id: String,
    /// The `sso_url` field.
    pub sso_url: String,
    /// The `slo_url` field.
    pub slo_url: Option<String>,
    /// The `certificate` field.
    pub certificate: Option<String>,
}

/// See [`saml_login`].
pub async fn saml_login(
    State(ctx): State<SsoContext>,
    Json(body): Json<SamlLoginBody>,
) -> Result<impl IntoResponse, ApiError> {
    if !ctx.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let auth_request = ctx.saml_service.get_auth_redirect(body.redirect_url.as_deref()).await?;

    Ok(Json(SamlLoginResponse { redirect_url: auth_request.redirect_url }))
}

/// See [`saml_login_redirect`].
pub async fn saml_login_redirect(
    State(ctx): State<SsoContext>,
    Query(query): Query<SamlLoginQuery>,
) -> Result<impl IntoResponse, ApiError> {
    if !ctx.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let auth_request = ctx.saml_service.get_auth_redirect(query.redirect_url.as_deref()).await?;

    Ok(Redirect::temporary(&auth_request.redirect_url))
}

/// See [`saml_callback_post`].
pub async fn saml_callback_post(
    State(ctx): State<SsoContext>,
    Json(body): Json<SamlCallbackBody>,
) -> Result<impl IntoResponse, ApiError> {
    handle_saml_callback(&ctx, body.saml_response.as_deref(), body.relay_state.as_deref()).await
}

/// See [`saml_callback_get`].
pub async fn saml_callback_get(
    State(ctx): State<SsoContext>,
    Query(query): Query<SamlCallbackQuery>,
) -> Result<impl IntoResponse, ApiError> {
    handle_saml_callback(&ctx, query.saml_response.as_deref(), query.relay_state.as_deref()).await
}

async fn handle_saml_callback(
    ctx: &SsoContext,
    saml_response: Option<&str>,
    relay_state: Option<&str>,
) -> Result<impl IntoResponse, ApiError> {
    if !ctx.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let saml_response = saml_response.ok_or_else(|| ApiError::bad_request("Missing SAML response"))?;

    let auth_result = ctx.saml_service.process_auth_response(saml_response, relay_state, None, None).await?;

    let user = ctx
        .account_identity_service
        .get_user_by_id(&auth_result.user_id)
        .await?
        .ok_or_else(|| ApiError::internal("User not found after SAML auth"))?;

    let device_id = format!("SAML_{}", uuid::Uuid::new_v4().as_simple());

    let access_token = ctx.token_auth.generate_access_token(&auth_result.user_id, &device_id, user.is_admin).await?;

    let expires_in = 3600_i64;

    let refresh_token = match ctx
        .token_auth
        .generate_refresh_token(&auth_result.user_id, &device_id, &access_token)
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

/// See [`saml_logout`].
pub async fn saml_logout(
    State(ctx): State<SsoContext>,
    _auth_user: AuthenticatedUser,
) -> Result<impl IntoResponse, ApiError> {
    if !ctx.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let mapping = ctx.saml_service.get_user_mapping(&_auth_user.user_id).await?;

    if let Some(_mapping) = mapping {
        let sessions = ctx.saml_service.get_session_by_user(&_auth_user.user_id).await?;

        if let Some(session) = sessions {
            let redirect_url =
                ctx.saml_service.initiate_logout(&session.session_id, Some("User initiated logout")).await?;

            return Ok(Json(serde_json::json!({
                "redirect_url": redirect_url
            })));
        }
    }

    Ok(Json(serde_json::json!({
        "message": "No active SAML session found"
    })))
}

/// See [`saml_logout_callback`].
pub async fn saml_logout_callback(
    State(ctx): State<SsoContext>,
    Query(query): Query<SamlCallbackQuery>,
) -> Result<impl IntoResponse, ApiError> {
    if !ctx.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let saml_response = query.saml_response.ok_or_else(|| ApiError::bad_request("Missing SAML response"))?;

    ctx.saml_service.process_logout_response(&saml_response).await?;

    Ok(Json(serde_json::json!({
        "message": "Logout successful"
    })))
}

/// See [`get_saml_metadata`].
pub async fn get_saml_metadata(State(ctx): State<SsoContext>) -> Result<impl IntoResponse, ApiError> {
    if !ctx.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let metadata = ctx.saml_service.get_idp_metadata().await?;

    Ok(Json(SamlMetadataResponse {
        entity_id: metadata.entity_id,
        sso_url: metadata.sso_url,
        slo_url: metadata.slo_url,
        certificate: Some(metadata.certificate),
    }))
}

/// See [`get_sp_metadata`].
pub async fn get_sp_metadata(State(ctx): State<SsoContext>) -> Result<impl IntoResponse, ApiError> {
    if !ctx.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let config = ctx.saml_service.get_config();
    let server_name = &ctx.server_name;

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

/// See [`refresh_idp_metadata`].
pub async fn refresh_idp_metadata(State(ctx): State<SsoContext>) -> Result<impl IntoResponse, ApiError> {
    if !ctx.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let metadata = ctx.saml_service.get_idp_metadata().await?;

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

/// The `SamlMappingListQuery` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamlMappingListQuery {
    /// The `limit` field.
    pub limit: Option<i64>,
    /// The `from` field.
    pub from: Option<String>,
}

/// The `SamlMappingView` struct.
#[derive(Debug, Serialize)]
pub struct SamlMappingView {
    /// The `name_id` field.
    pub name_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `issuer` field.
    pub issuer: String,
    /// The `first_seen_ts` field.
    pub first_seen_ts: i64,
    /// The `last_authenticated_ts` field.
    pub last_authenticated_ts: i64,
    /// The `authentication_count` field.
    pub authentication_count: i32,
    /// The `attributes` field.
    pub attributes: serde_json::Value,
}

impl From<synapse_services::saml_service::SamlUserMapping> for SamlMappingView {
    fn from(m: synapse_services::saml_service::SamlUserMapping) -> Self {
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

/// The `SamlMappingPage` struct.
#[derive(Debug, Serialize)]
pub struct SamlMappingPage {
    /// The `mappings` field.
    pub mappings: Vec<SamlMappingView>,
    /// The `next_token` field.
    pub next_token: Option<String>,
}

/// The `UpdateSamlMappingBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateSamlMappingBody {
    /// The `user_id` field.
    pub user_id: Option<String>,
    /// The `attributes` field.
    pub attributes: Option<serde_json::Value>,
}

/// The `SamlLogoutAdminBody` struct.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SamlLogoutAdminBody {
    /// The `user_id` field.
    pub user_id: String,
}

/// The `SamlLogoutAdminResponse` struct.
#[derive(Debug, Serialize)]
pub struct SamlLogoutAdminResponse {
    /// The `user_id` field.
    pub user_id: String,
    /// The `redirect_url` field.
    pub redirect_url: Option<String>,
    /// The `sessions_invalidated` field.
    pub sessions_invalidated: u32,
}

/// See [`list_saml_mappings_admin`].
pub async fn list_saml_mappings_admin(
    State(ctx): State<SsoContext>,
    Query(query): Query<SamlMappingListQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 500);
    let rows = ctx.saml_service.list_user_mappings(limit, query.from.as_deref()).await?;

    let next_token = if rows.len() as i64 == limit { rows.last().map(|r| r.name_id.clone()) } else { None };

    Ok(Json(SamlMappingPage { mappings: rows.into_iter().map(SamlMappingView::from).collect(), next_token }))
}

/// See [`get_saml_mapping_admin`].
pub async fn get_saml_mapping_admin(
    State(ctx): State<SsoContext>,
    Path(name_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let row = ctx
        .saml_service
        .get_user_mapping_any_issuer(&name_id)
        .await?
        .ok_or_else(|| ApiError::not_found("SAML user mapping not found"))?;
    Ok(Json(SamlMappingView::from(row)))
}

/// See [`update_saml_mapping_admin`].
pub async fn update_saml_mapping_admin(
    State(ctx): State<SsoContext>,
    Path(name_id): Path<String>,
    Json(body): Json<UpdateSamlMappingBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.user_id.is_none() && body.attributes.is_none() {
        return Err(ApiError::bad_request("At least one of user_id / attributes must be provided"));
    }
    let row = ctx
        .saml_service
        .update_user_mapping_by_name_id(&name_id, body.user_id.as_deref(), body.attributes.as_ref())
        .await?
        .ok_or_else(|| ApiError::not_found("SAML user mapping not found"))?;
    Ok(Json(SamlMappingView::from(row)))
}

/// See [`delete_saml_mapping_admin`].
pub async fn delete_saml_mapping_admin(
    State(ctx): State<SsoContext>,
    Path(name_id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let removed = ctx.saml_service.delete_user_mapping_by_name_id(&name_id).await?;
    if removed == 0 {
        return Err(ApiError::not_found("SAML user mapping not found"));
    }
    Ok(Json(serde_json::json!({ "removed": removed })))
}

/// See [`saml_logout_admin`].
pub async fn saml_logout_admin(
    State(ctx): State<SsoContext>,
    Json(body): Json<SamlLogoutAdminBody>,
) -> Result<impl IntoResponse, ApiError> {
    if body.user_id.is_empty() {
        return Err(ApiError::bad_request("user_id is required"));
    }
    if !ctx.saml_service.is_enabled() {
        return Err(ApiError::forbidden("SAML authentication is not enabled"));
    }

    let session = ctx.saml_service.get_session_by_user(&body.user_id).await?;

    let Some(session) = session else {
        return Ok(Json(SamlLogoutAdminResponse {
            user_id: body.user_id,
            redirect_url: None,
            sessions_invalidated: 0,
        }));
    };

    let redirect_url = ctx.saml_service.initiate_logout(&session.session_id, Some("Admin initiated logout")).await.ok();

    Ok(Json(SamlLogoutAdminResponse { user_id: body.user_id, redirect_url, sessions_invalidated: 1 }))
}

/// See [`get_saml_admin_config`].
pub async fn get_saml_admin_config(State(ctx): State<SsoContext>) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(ctx.saml_service.effective_config()))
}

/// See [`update_saml_admin_config`].
pub async fn update_saml_admin_config(
    State(ctx): State<SsoContext>,
    Json(body): Json<serde_json::Value>,
) -> Result<impl IntoResponse, ApiError> {
    let merged = ctx.saml_service.apply_runtime_overrides(body).await?;
    Ok(Json(merged))
}

/// See [`create_saml_router`].
pub fn create_saml_router(state: AppState) -> axum::Router<AppState> {
    use axum::routing::*;

    let public_routes = axum::Router::new()
        .route("/_matrix/client/v3/logout/saml", get(saml_logout))
        .route("/_matrix/client/v3/logout/saml/callback", get(saml_logout_callback))
        .route("/_matrix/client/v3/login/sso/redirect/saml", get(saml_login_redirect))
        .route("/_matrix/client/v3/login/sso/redirect/saml", post(saml_login))
        .route("/_matrix/client/v3/login/saml/callback", get(saml_callback_get))
        .route("/_matrix/client/v3/login/saml/callback", post(saml_callback_post))
        .route("/_matrix/client/v3/saml/metadata", get(get_saml_metadata))
        .route("/_matrix/client/v3/saml/sp_metadata", get(get_sp_metadata));

    let admin_routes =
        axum::Router::new()
            .route("/_synapse/admin/v1/saml/metadata/refresh", post(refresh_idp_metadata))
            .route("/_synapse/admin/v1/saml/config", get(get_saml_admin_config).put(update_saml_admin_config))
            .route("/_synapse/admin/v1/saml/mappings", get(list_saml_mappings_admin))
            .route(
                "/_synapse/admin/v1/saml/mapping/{name_id}",
                get(get_saml_mapping_admin).put(update_saml_mapping_admin).delete(delete_saml_mapping_admin),
            )
            .route("/_synapse/admin/v1/saml/logout", post(saml_logout_admin))
            .route_layer(
                middleware::from_fn_with_state(
                    <crate::web::routes::context::AdminContext as axum::extract::FromRef<
                        crate::web::routes::AppState,
                    >>::from_ref(&state),
                    crate::web::middleware::admin_auth_middleware,
                ),
            );

    public_routes.merge(admin_routes).with_state(state)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    #[test]
    fn saml_callback_body_accepts_standard_pascal_case() {
        let body: SamlCallbackBody = serde_json::from_str(r#"{"SAMLResponse":"abc","RelayState":"xyz"}"#).unwrap();
        assert_eq!(body.saml_response.as_deref(), Some("abc"));
        assert_eq!(body.relay_state.as_deref(), Some("xyz"));
        assert_eq!(body.saml_request, None);
    }

    #[test]
    fn saml_callback_body_accepts_legacy_snake_case() {
        let body: SamlCallbackBody = serde_json::from_str(r#"{"saml_response":"abc","relay_state":"xyz"}"#).unwrap();
        assert_eq!(body.saml_response.as_deref(), Some("abc"));
        assert_eq!(body.relay_state.as_deref(), Some("xyz"));
    }

    #[test]
    fn saml_callback_query_accepts_standard_pascal_case() {
        let query: SamlCallbackQuery = serde_json::from_str(r#"{"SAMLResponse":"abc"}"#).unwrap();
        assert_eq!(query.saml_response.as_deref(), Some("abc"));
    }

    #[test]
    fn saml_callback_query_accepts_legacy_snake_case() {
        let query: SamlCallbackQuery = serde_json::from_str(r#"{"saml_response":"abc","relay_state":"xyz"}"#).unwrap();
        assert_eq!(query.saml_response.as_deref(), Some("abc"));
        assert_eq!(query.relay_state.as_deref(), Some("xyz"));
    }

    #[test]
    fn saml_login_query_and_body_accept_camel_and_snake() {
        let query: SamlLoginQuery = serde_json::from_str(r#"{"redirectUrl":"https://app"}"#).unwrap();
        assert_eq!(query.redirect_url.as_deref(), Some("https://app"));

        let query_snake: SamlLoginQuery = serde_json::from_str(r#"{"redirect_url":"https://app"}"#).unwrap();
        assert_eq!(query_snake.redirect_url.as_deref(), Some("https://app"));

        let body: SamlLoginBody = serde_json::from_str(r#"{"redirectUrl":"https://app"}"#).unwrap();
        assert_eq!(body.redirect_url.as_deref(), Some("https://app"));
    }

    #[test]
    fn saml_callback_body_rejects_unknown_fields() {
        let err = serde_json::from_str::<SamlCallbackBody>(r#"{"SAMLResponse":"abc","bogus":1}"#);
        assert!(err.is_err(), "deny_unknown_fields must reject unexpected keys");
    }
}
