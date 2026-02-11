pub mod admin;
pub mod e2ee_routes;
pub mod federation;
pub mod friend;
pub mod key_backup;
pub mod media;
pub mod voice;

pub use admin::create_admin_router;
pub use e2ee_routes::create_e2ee_router;
pub use federation::create_federation_router;
pub use friend::create_friend_router;
pub use key_backup::create_key_backup_router;
pub use media::create_media_router;
pub use voice::create_voice_router;

use crate::cache::*;
use crate::common::*;
use crate::services::*;
use crate::storage::CreateEventParams;
use crate::web::middleware::rate_limit_middleware;
use axum::extract::rejection::JsonRejection;
use axum::{
    extract::{FromRequestParts, Json, Path, Query, State},
    http::{request::Parts, HeaderMap},
    routing::{delete, get, post, put},
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower_http::compression::CompressionLayer;

// Custom JSON extractor to provide friendly error messages
pub struct MatrixJson<T>(pub T);

impl<S, T> axum::extract::FromRequest<S> for MatrixJson<T>
where
    S: Send + Sync,
    T: serde::de::DeserializeOwned + Send,
{
    type Rejection = ApiError;

    async fn from_request(req: axum::extract::Request, state: &S) -> Result<Self, Self::Rejection> {
        match axum::extract::Json::<T>::from_request(req, state).await {
            Ok(axum::extract::Json(value)) => Ok(MatrixJson(value)),
            Err(rejection) => {
                let message = match rejection {
                    JsonRejection::JsonDataError(e) => format!("Invalid JSON data: {}", e),
                    JsonRejection::JsonSyntaxError(e) => format!("JSON syntax error: {}", e),
                    JsonRejection::MissingJsonContentType(e) => {
                        format!("Missing Content-Type: application/json: {}", e)
                    }
                    _ => format!("JSON error: {}", rejection),
                };
                Err(ApiError::bad_request(message))
            }
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub services: ServiceContainer,
    pub cache: Arc<CacheManager>,
    pub health_checker: Arc<crate::common::health::HealthChecker>,
}

impl AppState {
    pub fn new(services: ServiceContainer, cache: Arc<CacheManager>) -> Self {
        let mut health_checker = crate::common::health::HealthChecker::new("0.1.0".to_string());

        // Add DB health check
        health_checker.add_check(Box::new(crate::common::health::DatabaseHealthCheck::new(
            (*services.user_storage.pool).clone(),
        )));

        // Add Cache health check
        health_checker.add_check(Box::new(crate::common::health::CacheHealthCheck::new(
            (*cache).clone(),
        )));

        Self {
            services,
            cache,
            health_checker: Arc::new(health_checker),
        }
    }
}


#[derive(Clone)]
pub struct AuthenticatedUser {
    pub user_id: String,
    pub device_id: Option<String>,
    pub is_admin: bool,
    pub access_token: String,
}

#[derive(Clone)]
pub struct AdminUser {
    pub user_id: String,
    pub device_id: Option<String>,
    pub access_token: String,
}

impl FromRequestParts<AppState> for AuthenticatedUser {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let token_result = extract_token_from_headers(&parts.headers);
        let state = state.clone();

        async move {
            let token = token_result?;
            let (user_id, device_id, is_admin) =
                state.services.auth_service.validate_token(&token).await?;

            Ok(AuthenticatedUser {
                user_id,
                device_id,
                is_admin,
                access_token: token,
            })
        }
    }
}

impl FromRequestParts<AppState> for AdminUser {
    type Rejection = ApiError;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let auth_future = AuthenticatedUser::from_request_parts(parts, state);
        
        async move {
            let auth = auth_future.await?;
            if !auth.is_admin {
                return Err(ApiError::forbidden("Admin access required".to_string()));
            }
            Ok(AdminUser {
                user_id: auth.user_id,
                device_id: auth.device_id,
                access_token: auth.access_token,
            })
        }
    }
}

pub trait AuthExtractor {
    fn extract_token(&self) -> Result<String, ApiError>;
}

impl AuthExtractor for HeaderMap {
    fn extract_token(&self) -> Result<String, ApiError> {
        extract_token_from_headers(self)
    }
}

fn extract_token_from_headers(headers: &HeaderMap) -> Result<String, ApiError> {
    headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .ok_or_else(|| {
            ApiError::unauthorized("Missing or invalid authorization header".to_string())
        })
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .without_v07_checks()
        .route(
            "/",
            get(|| async {
                Json(json!({"msg": "Synapse Rust Matrix Server", "version": "0.1.0"}))
            }),
        )
        .route("/health", get(health_check))
        .route("/_matrix/client/versions", get(get_client_versions))
        .route("/_matrix/client/r0/version", get(get_server_version))
        .route("/_matrix/client/r0/register", post(register))
        .route(
            "/_matrix/client/r0/register/available",
            get(check_username_availability),
        )
        .route(
            "/_matrix/client/r0/register/email/requestToken",
            post(request_email_verification),
        )
        .route(
            "/_matrix/client/r0/register/email/submitToken",
            post(submit_email_token),
        )
        .route("/_matrix/client/r0/login", post(login))
        .route("/_matrix/client/r0/logout", post(logout))
        .route("/_matrix/client/r0/logout/all", post(logout_all))
        .route("/_matrix/client/r0/refresh", post(refresh_token))
        .route("/_matrix/client/r0/account/whoami", get(whoami))
        .route(
            "/_matrix/client/r0/account/profile/{user_id}",
            get(get_profile),
        )
        .route(
            "/_matrix/client/r0/account/profile/{user_id}/displayname",
            put(update_displayname),
        )
        .route(
            "/_matrix/client/r0/account/profile/{user_id}/avatar_url",
            put(update_avatar),
        )
        .route("/_matrix/client/r0/account/password", post(change_password))
        .route(
            "/_matrix/client/r0/account/deactivate",
            post(deactivate_account),
        )
        .route(
            "/_matrix/client/r0/user_directory/search",
            post(search_user_directory),
        )
        .route(
            "/_matrix/client/r0/user_directory/list",
            post(list_user_directory),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/report/{event_id}",
            post(report_event),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/report/{event_id}/score",
            put(update_report_score),
        )
        .route("/_matrix/client/r0/sync", get(sync))
        .route(
            "/_matrix/client/r0/rooms/{room_id}/messages",
            get(get_messages),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/typing/{user_id}",
            put(set_typing),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/receipt/{receipt_type}/{event_id}",
            post(send_receipt),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/read_markers",
            post(set_read_markers),
        )
        .route("/_matrix/client/r0/rooms/{room_id}/join", post(join_room))
        .route("/_matrix/client/r0/rooms/{room_id}/leave", post(leave_room))
        .route(
            "/_matrix/client/r0/rooms/{room_id}/members",
            get(get_room_members),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/invite",
            post(invite_user),
        )
        .route("/_matrix/client/r0/createRoom", post(create_room))
        .route(
            "/_matrix/client/r0/directory/room/alias/{room_alias}",
            get(get_room_by_alias),
        )
        .route(
            "/_matrix/client/r0/directory/room/{param}",
            get(get_room)
                .put(set_room_directory)
                .delete(delete_room_directory),
        )
        .route(
            "/_matrix/client/r0/directory/room/{room_id}/alias",
            get(get_room_aliases),
        )
        .route(
            "/_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}",
            put(set_room_alias),
        )
        .route(
            "/_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}",
            delete(delete_room_alias),
        )
        .route("/_matrix/client/r0/publicRooms", get(get_public_rooms))
        .route("/_matrix/client/r0/publicRooms", post(create_public_room))
        .route(
            "/_matrix/client/r0/user/{user_id}/rooms",
            get(get_user_rooms),
        )
        .route("/_matrix/client/r0/devices", get(get_devices))
        .route("/_matrix/client/r0/delete_devices", post(delete_devices))
        .route("/_matrix/client/r0/devices/{device_id}", get(get_device))
        .route("/_matrix/client/r0/devices/{device_id}", put(update_device))
        .route(
            "/_matrix/client/r0/devices/{device_id}",
            delete(delete_device),
        )
        .route(
            "/_matrix/client/r0/presence/{user_id}/status",
            get(get_presence),
        )
        .route(
            "/_matrix/client/r0/presence/{user_id}/status",
            put(set_presence),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}",
            put(put_state_event),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/state",
            get(get_room_state),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/state/{event_type}",
            get(get_state_by_type),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}",
            get(get_state_event),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/state/{event_type}",
            post(send_state_event),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/get_membership_events",
            post(get_membership_events),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/redact/{event_id}",
            put(redact_event),
        )
        .route("/_matrix/client/r0/rooms/{room_id}/kick", post(kick_user))
        .route("/_matrix/client/r0/rooms/{room_id}/ban", post(ban_user))
        .route("/_matrix/client/r0/rooms/{room_id}/unban", post(unban_user))
        .route(
            "/_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}",
            put(send_message),
        )
        // 合并子路由器
        .merge(create_friend_router(state.clone()))
        .merge(create_voice_router(state.clone()))
        .merge(create_media_router(state.clone()))
        .merge(create_e2ee_router(state.clone()))
        .merge(create_key_backup_router(state.clone()))
        .merge(create_admin_router(state.clone()))
        .merge(create_federation_router(state.clone()))
        .layer(CompressionLayer::new())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .with_state(state)
}

async fn get_client_versions() -> Json<Value> {
    Json(json!({
        "versions": ["r0.0.1", "r0.1.0", "r0.2.0", "r0.3.0", "r0.4.0", "r0.5.0", "r0.6.0"],
        "unstable_features": {
            "m.lazy_load_members": true,
            "m.require_identity_server": false,
            "m.supports_login_via_phone_number": true
        }
    }))
}

async fn get_server_version() -> Json<Value> {
    Json(json!({
        "version": "0.1.0"
    }))
}

async fn health_check(State(state): State<AppState>) -> Json<Value> {
    let status = state.health_checker.check_readiness().await;
    Json(
        serde_json::to_value(status)
            .unwrap_or_else(|_| json!({"status": "unhealthy", "error": "serialization error"})),
    )
}

async fn register(
    State(state): State<AppState>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let username = body
        .get("username")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Username required".to_string()))?;
    let password = body
        .get("password")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Password required".to_string()))?;

    // P1 Quality: Validate username and password
    state
        .services
        .auth_service
        .validator
        .validate_username(username)?;
    state
        .services
        .auth_service
        .validator
        .validate_password(password)?;

    // Matrix spec: check for registration type if provided
    if let Some(auth_type) = body
        .get("auth")
        .and_then(|a| a.get("type"))
        .and_then(|t| t.as_str())
    {
        if auth_type != "m.login.password" && auth_type != "m.login.dummy" {
            return Err(ApiError::bad_request(format!(
                "Unsupported authentication type: {}",
                auth_type
            )));
        }
    }

    // P0 Security: Standard registration cannot request admin status.
    // Admin registration must go through the dedicated admin registration flow (HMAC-based).
    let admin = false;
    let displayname = body.get("displayname").and_then(|v| v.as_str());

    Ok(Json(
        state
            .services
            .registration_service
            .register_user(username, password, admin, displayname)
            .await?,
    ))
}

async fn check_username_availability(
    State(state): State<AppState>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let username = params
        .get("username")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Username required".to_string()))?;

    if let Err(e) = state
        .services
        .auth_service
        .validator
        .validate_username(username)
    {
        return Err(e.into());
    }

    let user_id = format!("@{}:{}", username, state.services.server_name);
    let exists = state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "available": !exists,
        "username": username
    })))
}

async fn request_email_verification(
    State(state): State<AppState>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let email = body
        .get("email")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Email is required".to_string()))?;

    if state
        .services
        .auth_service
        .validator
        .validate_email(email)
        .is_err()
    {
        return Err(ApiError::bad_request(
            "Invalid email address format".to_string(),
        ));
    }

    let token = state
        .services
        .auth_service
        .generate_email_verification_token()
        .map_err(|e| ApiError::internal(format!("Failed to generate token: {}", e)))?;

    let session_data = body.get("client_secret").cloned();

    let token_id = state
        .services
        .email_verification_storage
        .create_verification_token(email, &token, 3600, session_data)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to store token: {}", e)))?;

    let sid = format!("{}", token_id);

    let submit_url = format!(
        "https://{}:{}/_matrix/client/r0/register/email/submitToken",
        state.services.config.server.host,
        state.services.config.server.port
    );

    Ok(Json(json!({
        "sid": sid,
        "submit_url": submit_url,
        "expires_in": 3600
    })))
}

async fn submit_email_token(
    State(state): State<AppState>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let sid = body
        .get("sid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Session ID (sid) is required".to_string()))?;

    let client_secret = body
        .get("client_secret")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Client secret is required".to_string()))?;

    let token = body
        .get("token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Verification token is required".to_string()))?;

    let sid_int: i64 = sid
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid session ID format".to_string()))?;

    let verification_token = state
        .services
        .email_verification_storage
        .get_verification_token_by_id(sid_int)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get verification token: {}", e)))?;

    let verification_token = match verification_token {
        Some(t) => t,
        None => {
            return Err(ApiError::bad_request(
                "Invalid session ID or session not found".to_string(),
            ))
        }
    };

    if verification_token.used {
        return Err(ApiError::bad_request(
            "Verification token has already been used".to_string(),
        ));
    }

    if verification_token.expires_at < chrono::Utc::now().timestamp() {
        return Err(ApiError::bad_request(
            "Verification token has expired".to_string(),
        ));
    }

    if verification_token.token != token {
        return Err(ApiError::bad_request(
            "Invalid verification token".to_string(),
        ));
    }

    if verification_token.session_data != Some(serde_json::Value::String(client_secret.to_string()))
        && verification_token.session_data.as_ref().map(|v| v.as_str()) != Some(Some(client_secret))
    {
        return Err(ApiError::bad_request("Client secret mismatch".to_string()));
    }

    state
        .services
        .email_verification_storage
        .mark_token_used(sid_int)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to mark token as used: {}", e)))?;

    Ok(Json(json!({
        "success": true
    })))
}

async fn login(
    State(state): State<AppState>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let username = body
        .get("user")
        .or(body.get("username"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Username required".to_string()))?;
    let password = body
        .get("password")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Password required".to_string()))?;

    // P1 Quality: Basic validation
    if username.is_empty() || password.is_empty() {
        return Err(ApiError::bad_request(
            "Username and password are required".to_string(),
        ));
    }

    if username.len() > 255 {
        return Err(ApiError::bad_request("Username too long".to_string()));
    }
    
    // Check password length to prevent DoS - must match Validator::validate_password max (128)
    if password.len() > 128 {
        return Err(ApiError::bad_request("Password too long (max 128 characters)".to_string()));
    }

    let device_id = body.get("device_id").and_then(|v| v.as_str());
    let initial_display_name = body.get("initial_display_name").and_then(|v| v.as_str());

    let (user, access_token, refresh_token, device_id) = state
        .services
        .auth_service
        .login(username, password, device_id, initial_display_name)
        .await?;

    Ok(Json(json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "expires_in": state.services.auth_service.token_expiry,
        "device_id": device_id,
        "user_id": user.user_id(),
        "well_known": {
            "m.homeserver": {
                "base_url": format!(
                    "http://{}:{}",
                    state.services.config.server.host,
                    state.services.config.server.port
                )
            }
        }
    })))
}

async fn logout(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .auth_service
        .logout(&auth_user.access_token, auth_user.device_id.as_deref())
        .await?;

    Ok(Json(json!({})))
}

async fn logout_all(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .auth_service
        .logout_all(&auth_user.user_id)
        .await?;

    Ok(Json(json!({})))
}

async fn refresh_token(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let refresh_token = body
        .get("refresh_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Refresh token required".to_string()))?;

    let (new_access, new_refresh, device_id) = state
        .services
        .auth_service
        .refresh_token(refresh_token)
        .await?;

    Ok(Json(json!({
        "access_token": new_access,
        "refresh_token": new_refresh,
        "expires_in": state.services.auth_service.token_expiry,
        "device_id": device_id
    })))
}

async fn whoami(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let profile = state
        .services
        .user_storage
        .get_user_profile(&auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    Ok(Json(json!({
        "user_id": auth_user.user_id,
        "displayname": profile.as_ref().and_then(|p| p.displayname.clone()),
        "avatar_url": profile.as_ref().and_then(|p| p.avatar_url.clone()),
        "admin": auth_user.is_admin
    })))
}

fn validate_user_id(user_id: &str) -> Result<(), ApiError> {
    // Basic format check
    if user_id.is_empty() {
        return Err(ApiError::bad_request("user_id is required".to_string()));
    }
    
    // Detailed validation using common validator logic regex
    // We can't access state here easily without changing signature, so we keep basic logic consistent with validator
    // or we just instantiate a local regex if needed, but simple string parsing is faster for basic checks
    
    if !user_id.starts_with('@') {
        return Err(ApiError::bad_request("Invalid user_id format: must start with @".to_string()));
    }
    
    if user_id.len() > 255 {
        return Err(ApiError::bad_request("user_id too long (max 255 characters)".to_string()));
    }
    
    let parts: Vec<&str> = user_id.split(':').collect();
    if parts.len() < 2 {
        return Err(ApiError::bad_request("Invalid user_id format: must be @username:server".to_string()));
    }
    
    let username = &parts[0][1..];
    if username.is_empty() {
        return Err(ApiError::bad_request("Invalid user_id format: username cannot be empty".to_string()));
    }
    
    if parts[1].is_empty() {
        return Err(ApiError::bad_request("Invalid user_id format: server cannot be empty".to_string()));
    }
    
    Ok(())
}

fn validate_room_id(room_id: &str) -> Result<(), ApiError> {
    if room_id.is_empty() {
        return Err(ApiError::bad_request("room_id is required".to_string()));
    }
    if !room_id.starts_with('!') {
        return Err(ApiError::bad_request("Invalid room_id format: must start with !".to_string()));
    }
    if room_id.len() > 255 {
        return Err(ApiError::bad_request("room_id too long (max 255 characters)".to_string()));
    }
    Ok(())
}

fn validate_event_id(event_id: &str) -> Result<(), ApiError> {
    if event_id.is_empty() {
        return Err(ApiError::bad_request("event_id is required".to_string()));
    }
    if !event_id.starts_with('$') {
        return Err(ApiError::bad_request("Invalid event_id format: must start with $".to_string()));
    }
    if event_id.len() > 255 {
        return Err(ApiError::bad_request("event_id too long (max 255 characters)".to_string()));
    }
    Ok(())
}

async fn get_profile(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    
    Ok(Json(
        state
            .services
            .registration_service
            .get_profile(&user_id)
            .await?,
    ))
}

async fn update_displayname(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let displayname = body
        .get("displayname")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Displayname required".to_string()))?;

    if displayname.len() > 255 {
        return Err(ApiError::bad_request("Displayname too long (max 255 characters)".to_string()));
    }

    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let user_exists = state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?;
    
    if !user_exists {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .registration_service
        .update_user_profile(&user_id, Some(displayname), None)
        .await?;
    Ok(Json(json!({})))
}

async fn update_avatar(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let avatar_url = body
        .get("avatar_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Avatar URL required".to_string()))?;

    if avatar_url.len() > 255 {
        return Err(ApiError::bad_request("Avatar URL too long (max 255 characters)".to_string()));
    }

    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let user_exists = state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?;
    
    if !user_exists {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .registration_service
        .update_user_profile(&user_id, None, Some(avatar_url))
        .await?;
    Ok(Json(json!({})))
}

async fn change_password(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let new_password = body
        .get("new_password")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("New password required".to_string()))?;

    state
        .services
        .auth_service
        .validator
        .validate_password(new_password)?;

    state
        .services
        .registration_service
        .change_password(&auth_user.user_id, new_password)
        .await?;

    Ok(Json(json!({})))
}

async fn deactivate_account(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = auth_user.user_id.clone();

    state
        .services
        .registration_service
        .deactivate_account(&user_id)
        .await?;

    state
        .services
        .cache
        .delete(&format!("user:active:{}", user_id))
        .await;

    state
        .services
        .cache
        .delete(&format!("token:{}", auth_user.access_token))
        .await;

    Ok(Json(json!({})))
}

async fn search_user_directory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (_, _, _) = state.services.auth_service.validate_token(&token).await?;

    let search_query = body
        .get("search_term")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as i64;

    let results = state
        .services
        .user_storage
        .search_users(&search_query, limit)
        .await?;

    let users: Vec<Value> = results
        .into_iter()
        .map(|u| {
            json!({
                "user_id": u.user_id,
                "display_name": u.displayname.unwrap_or_else(|| u.username.clone()),
                "avatar_url": u.avatar_url
            })
        })
        .collect();

    Ok(Json(json!({
        "limited": users.len() >= limit as usize,
        "results": users
    })))
}

async fn list_user_directory(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (_, _, _) = state.services.auth_service.validate_token(&token).await?;

    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as i64;

    let offset = body.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as i64;

    let total_count = state.services.user_storage.get_user_count().await?;

    let users = state
        .services
        .user_storage
        .get_users_paginated(limit, offset)
        .await?;

    let users_json: Vec<Value> = users
        .into_iter()
        .map(|u| {
            json!({
                "user_id": u.user_id,
                "display_name": u.displayname.unwrap_or_else(|| u.username.clone()),
                "avatar_url": u.avatar_url
            })
        })
        .collect();

    Ok(Json(json!({
        "total": total_count,
        "offset": offset,
        "users": users_json
    })))
}

async fn report_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    let reason = body.get("reason").and_then(|v| v.as_str());
    let score = body.get("score").and_then(|v| v.as_i64()).unwrap_or(-100) as i32;

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::bad_request(
            "Event does not belong to the specified room".to_string(),
        ));
    }

    let report_id = state
        .services
        .event_storage
        .report_event(
            &event_id,
            &room_id,
            &event.user_id,
            &auth_user.user_id,
            reason,
            score,
        )
        .await?;

    Ok(Json(json!({
        "report_id": report_id
    })))
}

async fn update_report_score(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((_room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_event_id(&event_id)?;

    let score =
        body.get("score")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| ApiError::bad_request("Score is required".to_string()))? as i32;

    state
        .services
        .event_storage
        .update_event_report_score_by_event(&event_id, score)
        .await?;

    Ok(Json(json!({})))
}

async fn sync(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let timeout = params
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);
    let full_state = params
        .get("full_state")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let set_presence = params
        .get("set_presence")
        .and_then(|v| v.as_str())
        .unwrap_or("online");

    Ok(Json(
        state
            .services
            .sync_service
            .sync(&user_id, timeout, full_state, set_presence)
            .await?,
    ))
}

async fn get_messages(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let from = params
        .get("from")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
    let direction = params.get("dir").and_then(|v| v.as_str()).unwrap_or("b");

    Ok(Json(
        state
            .services
            .room_service
            .get_room_messages(&room_id, from as i64, limit as i64, direction)
            .await?,
    ))
}

async fn send_message(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, _event_type, _txn_id)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let msgtype = body
        .get("msgtype")
        .and_then(|v| v.as_str())
        .unwrap_or("m.room.message");
    let content = body
        .get("body")
        .ok_or_else(|| ApiError::bad_request("Message body required".to_string()))?;

    // Validate content length to prevent DoS
    if let Some(s) = content.as_str() {
        if s.len() > 65536 {
             return Err(ApiError::bad_request("Message body too long (max 64KB)".to_string()));
        }
    } else {
        let s = content.to_string();
        if s.len() > 65536 {
             return Err(ApiError::bad_request("Message body too long (max 64KB)".to_string()));
        }
    }

    Ok(Json(
        state
            .services
            .room_service
            .send_message(&room_id, &auth_user.user_id, msgtype, content)
            .await?,
    ))
}

async fn join_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    state
        .services
        .room_service
        .join_room(&room_id, &user_id)
        .await?;
    Ok(Json(json!({})))
}

async fn leave_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    state
        .services
        .room_service
        .leave_room(&room_id, &auth_user.user_id)
        .await?;
    Ok(Json(json!({})))
}

async fn get_room_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let members = state
        .services
        .room_service
        .get_room_members(&room_id, &user_id)
        .await?;
    Ok(Json(members))
}

async fn invite_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let invitee = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(invitee)?;

    state
        .services
        .room_service
        .invite_user(&room_id, &auth_user.user_id, invitee)
        .await?;
    Ok(Json(json!({})))
}

async fn create_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let visibility = body.get("visibility").and_then(|v| v.as_str());
    if let Some(v) = visibility {
        if v != "public" && v != "private" {
            return Err(ApiError::bad_request(
                "Visibility must be 'public' or 'private'".to_string(),
            ));
        }
    }

    let room_alias = body.get("room_alias_name").and_then(|v| v.as_str());
    if let Some(alias) = room_alias {
        if alias.len() > 255 {
            return Err(ApiError::bad_request("Room alias name too long".to_string()));
        }
        // Validate alias format (localpart only, usually)
        // But spec says room_alias_name is the local part.
        // Let's rely on basic char check if needed, but length is most important for DoS.
        if !alias.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.') {
             return Err(ApiError::bad_request("Invalid characters in room alias name".to_string()));
        }
    }

    let name = body.get("name").and_then(|v| v.as_str());
    if let Some(n) = name {
        if n.len() > 255 {
             return Err(ApiError::bad_request("Room name too long".to_string()));
        }
    }

    let topic = body.get("topic").and_then(|v| v.as_str());
    if let Some(t) = topic {
        if t.len() > 4096 {
             return Err(ApiError::bad_request("Room topic too long".to_string()));
        }
    }

    let invite = body.get("invite").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect()
    });
    let preset = body.get("preset").and_then(|v| v.as_str());

    let config = CreateRoomConfig {
        visibility: visibility.map(|s| s.to_string()),
        room_alias_name: room_alias.map(|s| s.to_string()),
        name: name.map(|s| s.to_string()),
        topic: topic.map(|s| s.to_string()),
        invite_list: invite,
        preset: preset.map(|s| s.to_string()),
        ..Default::default()
    };

    Ok(Json(
        state
            .services
            .room_service
            .create_room(&user_id, config)
            .await?,
    ))
}

async fn get_room(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(param): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let room_id = if param.starts_with('!') {
        param
    } else {
        return Err(ApiError::bad_request("Invalid room_id format".to_string()));
    };
    Ok(Json(state.services.room_service.get_room(&room_id).await?))
}

async fn delete_room_directory(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(param): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !param.starts_with('!') {
        return Err(ApiError::bad_request("Invalid room_id format".to_string()));
    }

    let is_creator = state
        .services
        .room_service
        .is_room_creator(&param, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room creator: {}", e)))?;

    if !auth_user.is_admin && !is_creator {
        return Err(ApiError::forbidden(
            "Only room creator or server admin can remove room from directory".to_string(),
        ));
    }

    state
        .services
        .room_service
        .remove_room_directory(&param)
        .await?;
    Ok(Json(json!({})))
}

async fn set_room_directory(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(param): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("room_id required".to_string()))?;

    let alias = if param.starts_with('#') {
        param
    } else {
        format!("#{}", param)
    };

    state
        .services
        .room_service
        .set_room_alias(room_id, &alias, &auth_user.user_id)
        .await?;

    state
        .services
        .room_service
        .set_room_directory(room_id, true)
        .await?;

    Ok(Json(json!({})))
}

async fn get_room_aliases(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let aliases = state
        .services
        .room_service
        .get_room_aliases(&room_id)
        .await?;
    Ok(Json(json!({ "aliases": aliases })))
}

async fn set_room_alias(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, room_alias)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .room_service
        .set_room_alias(&room_id, &room_alias, &auth_user.user_id)
        .await?;
    Ok(Json(json!({})))
}

async fn delete_room_alias(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((room_id, _room_alias)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .room_service
        .remove_room_alias(&room_id)
        .await?;
    Ok(Json(json!({})))
}

async fn get_room_by_alias(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_alias): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let room_id = state
        .services
        .room_service
        .get_room_by_alias(&room_alias)
        .await?;
    match room_id {
        Some(rid) => Ok(Json(json!({ "room_id": rid }))),
        None => Err(ApiError::not_found("Room alias not found".to_string())),
    }
}

async fn get_public_rooms(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
    let _since = params.get("since").and_then(|v| v.as_str());

    Ok(Json(
        state
            .services
            .room_service
            .get_public_rooms(limit as i64)
            .await?,
    ))
}

#[axum::debug_handler]
async fn create_public_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let visibility = body.get("visibility").and_then(|v| v.as_str());
    if let Some(v) = visibility {
        if v != "public" && v != "private" {
            return Err(ApiError::bad_request(
                "Visibility must be 'public' or 'private'".to_string(),
            ));
        }
    }

    let room_alias = body.get("room_alias_name").and_then(|v| v.as_str());
    let name = body.get("name").and_then(|v| v.as_str());
    let topic = body.get("topic").and_then(|v| v.as_str());
    let invite = body.get("invite").and_then(|v| v.as_array()).map(|arr| {
        arr.iter()
            .filter_map(|x| x.as_str().map(String::from))
            .collect()
    });
    let preset = body.get("preset").and_then(|v| v.as_str());

    let config = CreateRoomConfig {
        visibility: visibility.map(|s| s.to_string()),
        room_alias_name: room_alias.map(|s| s.to_string()),
        name: name.map(|s| s.to_string()),
        topic: topic.map(|s| s.to_string()),
        invite_list: invite,
        preset: preset.map(|s| s.to_string()),
        ..Default::default()
    };

    Ok(Json(
        state
            .services
            .room_service
            .create_room(&auth_user.user_id, config)
            .await?,
    ))
}

async fn get_user_rooms(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let rooms = state
        .services
        .room_service
        .get_joined_rooms(&user_id)
        .await?;

    Ok(Json(json!({
        "joined_rooms": rooms
    })))
}

async fn get_devices(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let devices = state
        .services
        .device_storage
        .get_user_devices(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get devices: {}", e)))?;

    let device_list: Vec<Value> = devices
        .iter()
        .map(|d| {
            json!({
                "device_id": d.device_id,
                "display_name": d.display_name,
                "last_seen_ts": d.last_seen_ts.unwrap_or(0),
                "user_id": d.user_id
            })
        })
        .collect();

    Ok(Json(json!({ "devices": device_list })))
}

async fn get_device(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let device = _state
        .services
        .device_storage
        .get_device(&device_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get device: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Device not found".to_string()))?;

    if device.user_id != _auth_user.user_id && !_auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    Ok(Json(json!({
        "device_id": device.device_id,
        "display_name": device.display_name,
        "last_seen_ts": device.last_seen_ts.unwrap_or(0),
        "user_id": device.user_id
    })))
}

async fn update_device(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let display_name = body
        .get("display_name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Display name required".to_string()))?;

    state
        .services
        .device_storage
        .update_device_display_name(&device_id, display_name)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update device: {}", e)))?;

    Ok(Json(json!({})))
}

async fn delete_device(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(device_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    _state
        .services
        .device_storage
        .delete_device(&device_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete device: {}", e)))?;

    Ok(Json(json!({})))
}

async fn delete_devices(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let devices = body
        .get("devices")
        .and_then(|v| v.as_array())
        .ok_or_else(|| ApiError::bad_request("Devices array required".to_string()))?;

    let device_ids: Vec<String> = devices
        .iter()
        .filter_map(|v| v.as_str())
        .map(String::from)
        .collect();

    if device_ids.is_empty() {
        return Ok(Json(json!({})));
    }

    state
        .services
        .device_storage
        .delete_devices_batch(&device_ids)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to delete devices: {}", e)))?;

    Ok(Json(json!({})))
}

async fn get_presence(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let presence = state
        .services
        .presence_storage
        .get_presence(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get presence: {}", e)))?;

    match presence {
        Some((presence, status_msg)) => Ok(Json(json!({
            "presence": presence,
            "status_msg": status_msg
        }))),
        _ => Ok(Json(json!({
            "presence": "offline",
            "status_msg": Option::<String>::None
        }))),
    }
}

async fn set_presence(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let presence = body
        .get("presence")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Presence required".to_string()))?;
    let status_msg = body.get("status_msg").and_then(|v| v.as_str());

    state
        .services
        .presence_storage
        .set_presence(&user_id, presence, status_msg)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to set presence: {}", e)))?;

    Ok(Json(json!({})))
}

async fn set_typing(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, user_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_user_id(&user_id)?;

    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let typing = body
        .get("typing")
        .and_then(|v| v.as_bool())
        .ok_or_else(|| ApiError::bad_request("Typing flag required".to_string()))?;

    state
        .services
        .presence_storage
        .set_typing(&room_id, &user_id, typing)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to set typing: {}", e)))?;

    Ok(Json(json!({})))
}

async fn get_room_state(
    State(state): State<AppState>,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room_exists = state
        .services
        .room_service
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{}' not found", room_id)));
    }

    let events = state
        .services
        .event_storage
        .get_state_events(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;
    let state_events: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "type": e.event_type,
                "event_id": e.event_id,
                "sender": e.user_id,
                "content": e.content,
                "state_key": e.state_key
            })
        })
        .collect();

    Ok(Json(json!({ "state": state_events })))
}

async fn get_state_by_type(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room_exists = state
        .services
        .room_service
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?;

    if !room_exists {
        return Err(ApiError::not_found(format!("Room '{}' not found", room_id)));
    }

    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, &event_type)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

    let state_events: Vec<Value> = events
        .iter()
        .map(|e| {
            json!({
                "type": e.event_type,
                "event_id": e.event_id,
                "sender": e.user_id,
                "content": e.content,
                "state_key": e.state_key
            })
        })
        .collect();

    Ok(Json(json!({ "events": state_events })))
}

async fn get_state_event(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((room_id, event_type, state_key)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, &event_type)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

    let event = events
        .iter()
        .find(|e| e.state_key == Some(state_key.clone()))
        .ok_or_else(|| ApiError::not_found("State event not found".to_string()))?;

    Ok(Json(json!({
        "type": event.event_type,
        "event_id": event.event_id,
        "sender": event.user_id,
        "content": event.content,
        "state_key": event.state_key
    })))
}

async fn send_state_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let content = body;

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let state_event = state
        .services
        .event_storage
        .create_event(CreateEventParams {
            event_id: new_event_id.clone(),
            room_id: room_id.clone(),
            user_id: auth_user.user_id.clone(),
            event_type: format!("m.room.{}", event_type),
            content,
            state_key: Some(auth_user.user_id),
            origin_server_ts: now,
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to send state event: {}", e)))?;

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": state_event.event_type,
        "state_key": state_event.state_key
    })))
}

async fn put_state_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type, state_key)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let event = state
        .services
        .event_storage
        .create_event(CreateEventParams {
            event_id: new_event_id.clone(),
            room_id: room_id.clone(),
            user_id: auth_user.user_id.clone(),
            event_type: format!("m.room.{}", event_type),
            content: body,
            state_key: Some(state_key),
            origin_server_ts: now,
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to put state event: {}", e)))?;

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": event.event_type,
        "state_key": event.state_key
    })))
}

async fn send_receipt(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, receipt_type, event_id)): Path<(String, String, String)>,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    state
        .services
        .room_storage
        .add_receipt(
            &auth_user.user_id,
            &event.user_id,
            &room_id,
            &event_id,
            &receipt_type,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to store receipt: {}", e)))?;

    Ok(Json(json!({})))
}

async fn set_read_markers(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let event_id = body
        .get("event_id")
        .and_then(|v| v.as_str())
        .or_else(|| body.get("m.fully_read").and_then(|v| v.as_str()))
        .or_else(|| body.get("m.read").and_then(|v| v.as_str()))
        .ok_or_else(|| ApiError::bad_request("Event ID required".to_string()))?;

    state
        .services
        .room_storage
        .update_read_marker(&room_id, &auth_user.user_id, event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to set read marker: {}", e)))?;

    Ok(Json(json!({})))
}
async fn get_membership_events(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(100) as i64;

    let memberships = state
        .services
        .member_storage
        .get_membership_history(&room_id, limit)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get membership events: {}", e)))?;

    let events: Vec<Value> = memberships
        .into_iter()
        .map(|m| {
            json!({
                "event_id": m.event_id,
                "type": m.event_type,
                "sender": m.sender,
                "state_key": m.user_id,
                "content": {
                    "membership": m.membership
                },
                "origin_server_ts": m.joined_ts
            })
        })
        .collect();

    Ok(Json(json!({
        "events": events
    })))
}

async fn redact_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    let reason = body.get("reason").and_then(|v| v.as_str());

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let content = json!({
        "reason": reason
    });

    state
        .services
        .event_storage
        .create_event(CreateEventParams {
            event_id: new_event_id.clone(),
            room_id: room_id.clone(),
            user_id: auth_user.user_id,
            event_type: "m.room.redaction".to_string(),
            content,
            state_key: None,
            origin_server_ts: now,
        })
        .await
        .map_err(|e| ApiError::internal(format!("Failed to redact event: {}", e)))?;

    Ok(Json(json!({
        "event_id": new_event_id
    })))
}

async fn kick_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;
    
    validate_user_id(target)?;

    let reason = body.get("reason").and_then(|v| v.as_str());
    if let Some(r) = reason {
        if r.len() > 512 {
             return Err(ApiError::bad_request("Reason too long".to_string()));
        }
    }

    let event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let content = json!({
        "membership": "leave",
        "reason": reason.unwrap_or("")
    });

    state
        .services
        .member_storage
        .remove_member(&room_id, target)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to kick user: {}", e)))?;

    state
        .services
        .event_storage
        .create_event(CreateEventParams {
            event_id,
            room_id: room_id.clone(),
            user_id: auth_user.user_id,
            event_type: "m.room.member".to_string(),
            content,
            state_key: Some(target.to_string()),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
        })
        .await
        .map_err(|e| {
            ::tracing::warn!("Failed to create membership event for room {}: {}", room_id, e);
        })
        .ok();

    Ok(Json(json!({})))
}

async fn ban_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(target)?;

    let reason = body.get("reason").and_then(|v| v.as_str());
    if let Some(r) = reason {
        if r.len() > 512 {
             return Err(ApiError::bad_request("Reason too long".to_string()));
        }
    }

    let event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let content = json!({
        "membership": "ban",
        "reason": reason.unwrap_or("")
    });

    state
        .services
        .member_storage
        .add_member(&room_id, target, "ban", None, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to ban user: {}", e)))?;

    state
        .services
        .event_storage
        .create_event(CreateEventParams {
            event_id,
            room_id: room_id.clone(),
            user_id: auth_user.user_id,
            event_type: "m.room.member".to_string(),
            content,
            state_key: Some(target.to_string()),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
        })
        .await
        .map_err(|e| {
            ::tracing::warn!("Failed to create membership event for room {}: {}", room_id, e);
        })
        .ok();

    Ok(Json(json!({})))
}

async fn unban_user(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let target = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(target)?;

    state
        .services
        .member_storage
        .remove_member(&room_id, target)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to unban user: {}", e)))?;

    let event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let content = json!({
        "membership": "leave"
    });

    state
        .services
        .event_storage
        .create_event(CreateEventParams {
            event_id,
            room_id: room_id.clone(),
            user_id: auth_user.user_id,
            event_type: "m.room.member".to_string(),
            content,
            state_key: Some(target.to_string()),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
        })
        .await
        .map_err(|e| {
            ::tracing::warn!("Failed to create membership event for room {}: {}", room_id, e);
        })
        .ok();

    Ok(Json(json!({})))
}
