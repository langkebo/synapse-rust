pub mod account_data;
pub mod admin;
pub mod app_service;
pub mod background_update;
pub mod burn_after_read;
pub mod captcha;
pub mod cas;
pub mod device;
pub mod directory;
pub mod dm;
pub mod e2ee_routes;
pub mod ephemeral;
pub mod event_report;
pub mod external_service;
pub mod federation;
pub mod friend_room;
pub mod guest;
pub mod invite_blocklist;
pub mod key_backup;
pub mod key_rotation;
pub mod media;
pub mod module;
pub mod oidc;
pub mod push;
pub mod push_notification;
pub mod qr_login;
pub mod reactions;
pub mod relations;
pub mod rendezvous;
pub mod room_summary;
pub mod saml;
pub mod search;
pub mod sliding_sync;
pub mod space;
pub mod sticky_event;
pub mod tags;
pub mod telemetry;
pub mod thirdparty;
pub mod thread;
pub mod typing;
pub mod verification_routes;
pub mod voice;
pub mod voip;
pub mod widget;
pub mod worker;

pub use account_data::create_account_data_router;
pub use admin::create_admin_module_router;
pub use app_service::create_app_service_router;
pub use background_update::create_background_update_router;
pub use burn_after_read::create_burn_after_read_router;
pub use captcha::create_captcha_router;
pub use cas::cas_routes;
pub use device::create_device_router;
pub use dm::create_dm_router;
pub use e2ee_routes::create_e2ee_router;
pub use event_report::create_event_report_router;
pub use external_service::create_external_service_router;
pub use federation::create_federation_router;
pub use friend_room::create_friend_router;
pub use guest::create_guest_router;
pub use key_backup::create_key_backup_router;
pub use key_rotation::create_key_rotation_router;
pub use media::create_media_router;
pub use module::create_module_router;
pub use oidc::create_oidc_router;
pub use push::create_push_router;
pub use push_notification::create_push_notification_router;
pub use reactions::create_reactions_router;
pub use relations::create_relations_router;
pub use rendezvous::create_rendezvous_router;
pub use room_summary::create_room_summary_router;
pub use saml::create_saml_router;
pub use search::create_search_router;
pub use sliding_sync::create_sliding_sync_router;
pub use space::create_space_router;
pub use tags::create_tags_router;
pub use telemetry::create_telemetry_router;
pub use thirdparty::create_thirdparty_router;
pub use thread::create_thread_routes;
pub use verification_routes::create_verification_router;
pub use voice::create_voice_router;
pub use voip::call_answer;
pub use voip::call_candidates;
pub use voip::call_hangup;
pub use voip::call_invite;
pub use voip::get_call_session;
pub use voip::get_turn_credentials_guest;
pub use voip::get_turn_server;
pub use voip::get_voip_config;
pub use widget::create_widget_router;
pub use worker::create_worker_router;

use crate::cache::*;
use crate::common::*;
use crate::services::*;
use crate::storage::CreateEventParams;
use crate::web::middleware::{cors_middleware, rate_limit_middleware, security_headers_middleware};
use axum::extract::rejection::JsonRejection;
use axum::{
    extract::{FromRequestParts, Json, Path, Query, State},
    http::{request::Parts, HeaderMap},
    routing::{get, post, put},
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{types::JsonValue, Row};
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
    pub federation_signature_cache: Arc<crate::cache::FederationSignatureCache>,
    pub rate_limit_config_manager:
        Option<Arc<crate::common::rate_limit_config::RateLimitConfigManager>>,
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

        let federation_signature_cache = Arc::new(crate::cache::FederationSignatureCache::new(
            crate::cache::SignatureCacheConfig::from_federation_config(
                services.config.federation.signature_cache_ttl,
                services.config.federation.key_cache_ttl,
                services.config.federation.key_rotation_grace_period_ms,
            ),
        ));

        Self {
            services,
            cache,
            health_checker: Arc::new(health_checker),
            federation_signature_cache,
            rate_limit_config_manager: None,
        }
    }

    pub fn with_rate_limit_config(
        mut self,
        manager: Arc<crate::common::rate_limit_config::RateLimitConfigManager>,
    ) -> Self {
        self.rate_limit_config_manager = Some(manager);
        self
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
pub struct OptionalAuthenticatedUser {
    pub user_id: Option<String>,
    pub device_id: Option<String>,
    pub is_admin: bool,
    pub access_token: Option<String>,
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

impl FromRequestParts<AppState> for OptionalAuthenticatedUser {
    type Rejection = std::convert::Infallible;

    fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> impl std::future::Future<Output = Result<Self, Self::Rejection>> + Send {
        let token_result = extract_token_from_headers(&parts.headers);
        let state = state.clone();

        async move {
            match token_result {
                Ok(token) => match state.services.auth_service.validate_token(&token).await {
                    Ok((user_id, device_id, is_admin)) => Ok(OptionalAuthenticatedUser {
                        user_id: Some(user_id),
                        device_id,
                        is_admin,
                        access_token: Some(token),
                    }),
                    Err(_) => Ok(OptionalAuthenticatedUser {
                        user_id: None,
                        device_id: None,
                        is_admin: false,
                        access_token: None,
                    }),
                },
                Err(_) => Ok(OptionalAuthenticatedUser {
                    user_id: None,
                    device_id: None,
                    is_admin: false,
                    access_token: None,
                }),
            }
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
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .map(|s| s.to_string())
        .ok_or_else(|| {
            ApiError::unauthorized("Missing or invalid authorization header".to_string())
        })?;

    if token.trim().is_empty() {
        return Err(ApiError::unauthorized(
            "Empty authorization token".to_string(),
        ));
    }

    Ok(token)
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .without_v07_checks()
        // v3 routes first to avoid being overridden
        .route(
            "/",
            get(|| async {
                Json(json!({
                    "msg": "Synapse Rust Matrix Server",
                    "version": "0.1.0"
                }))
            }),
        )
        .route("/health", get(health_check))
        .route("/_matrix/client/versions", get(get_client_versions))
        .route("/_matrix/client/v3/versions", get(get_client_versions))
        .route("/_matrix/client/r0/version", get(get_server_version))
        .route("/_matrix/server_version", get(get_server_version))
        .route("/_matrix/client/r0/capabilities", get(get_capabilities))
        .route("/_matrix/client/v3/capabilities", get(get_capabilities))
        // Push rules - return default rules (SDK requires this endpoint)
        // Push rules - handled by push router; add trailing-slash aliases
        .route("/_matrix/client/v3/pushrules/", get(get_push_rules_default))
        .route(
            "/_matrix/client/v3/pushrules/global/",
            get(get_push_rules_global_default),
        )
        .route("/.well-known/matrix/server", get(get_well_known_server))
        .route("/.well-known/matrix/client", get(get_well_known_client))
        .route("/.well-known/matrix/support", get(get_well_known_support))
        .merge(create_auth_router())
        .merge(create_account_router())
        .merge(create_account_data_router(state.clone()))
        .merge(create_directory_router(state.clone()))
        .merge(create_room_router())
        .merge(create_presence_router())
        .merge(create_device_router())
        // Merge sub-routers
        .merge(create_voice_router(state.clone()))
        .merge(create_media_router(state.clone()))
        .merge(create_e2ee_router(state.clone()))
        .merge(create_key_backup_router(state.clone()))
        .merge(create_verification_router(state.clone()))
        .merge(create_relations_router(state.clone()))
        .merge(create_reactions_router(state.clone()))
        .merge(create_admin_module_router(state.clone()))
        .merge(create_federation_router(state.clone()))
        .merge(create_friend_router(state.clone()))
        .merge(create_push_router(state.clone()))
        .merge(create_search_router(state.clone()))
        .merge(create_sliding_sync_router(state.clone()))
        .merge(create_space_router(state.clone()))
        .merge(create_app_service_router(state.clone()))
        .merge(create_worker_router(state.clone()))
        .merge(create_room_summary_router(state.clone()))
        .merge(create_event_report_router(state.clone()))
        .merge(create_background_update_router(state.clone()))
        .merge(create_module_router())
        .merge(create_saml_router())
        .merge(create_oidc_router(state.clone()))
        .merge(cas_routes())
        .merge(create_captcha_router())
        .merge(create_push_notification_router())
        .merge(create_telemetry_router())
        .merge(create_thirdparty_router(state.clone()))
        .merge(create_tags_router(state.clone()))
        .merge(dm::create_dm_router(state.clone()))
        .merge(typing::create_typing_router(state.clone()))
        .merge(ephemeral::create_ephemeral_router(state.clone()))
        .merge(create_external_service_router(state.clone()))
        .merge(create_burn_after_read_router(state.clone()))
        .merge(create_thread_routes(state.clone()))
        .merge(create_widget_router())
        .merge(create_rendezvous_router(state.clone()))
        .route("/_matrix/client/r0/voip/turnServer", get(get_turn_server))
        .route("/_matrix/client/r0/voip/turnServer", post(get_turn_server))
        .route("/_matrix/client/v3/voip/turnServer", get(get_turn_server))
        .route("/_matrix/client/v3/voip/turnServer", post(get_turn_server))
        .route("/_matrix/client/r0/voip/config", get(get_voip_config))
        .route("/_matrix/client/v3/voip/config", get(get_voip_config))
        .route(
            "/_matrix/client/r0/voip/turnServer/guest",
            get(get_turn_credentials_guest),
        )
        .route(
            "/_matrix/client/v3/voip/turnServer/guest",
            get(get_turn_credentials_guest),
        )
        // Call event routes (MSC3079)
        .route(
            "/_matrix/client/r0/rooms/{room_id}/send/m.call.invite/{txn_id}",
            put(voip::call_invite),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/send/m.call.invite/{txn_id}",
            put(voip::call_invite),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/send/m.call.candidates/{txn_id}",
            put(voip::call_candidates),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/send/m.call.candidates/{txn_id}",
            put(voip::call_candidates),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/send/m.call.answer/{txn_id}",
            put(voip::call_answer),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/send/m.call.answer/{txn_id}",
            put(voip::call_answer),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/send/m.call.hangup/{txn_id}",
            put(voip::call_hangup),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/send/m.call.hangup/{txn_id}",
            put(voip::call_hangup),
        )
        // Get call session
        .route(
            "/_matrix/client/r0/rooms/{room_id}/call/{call_id}",
            get(voip::get_call_session),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/call/{call_id}",
            get(voip::get_call_session),
        )
        .route("/_matrix/client/v3/account/whoami", get(whoami))
        .route(
            "/_matrix/client/v3/account/3pid",
            get(get_threepids).post(add_threepid),
        )
        .route("/_matrix/client/v3/account/3pid/add", post(add_threepid))
        .route("/_matrix/client/v3/account/3pid/bind", post(add_threepid))
        .route(
            "/_matrix/client/v3/account/3pid/delete",
            post(delete_threepid),
        )
        .route(
            "/_matrix/client/v3/account/3pid/unbind",
            post(unbind_threepid),
        )
        .route("/_matrix/client/v3/sync", get(sync))
        .route("/_matrix/client/v3/createRoom", post(create_room))
        .route("/_matrix/client/v3/joined_rooms", get(get_joined_rooms))
        // Media config - client API
        .route("/_matrix/client/v3/media/config", get(media::media_config))
        .route("/_matrix/client/r0/media/config", get(media::media_config))
        .layer(axum::middleware::from_fn(cors_middleware))
        .layer(axum::middleware::from_fn(security_headers_middleware))
        .layer(CompressionLayer::new())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .with_state(state)
}

fn create_auth_router() -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/r0/register",
            get(get_register_flows).post(register),
        )
        .route(
            "/_matrix/client/v3/register",
            get(get_register_flows).post(register),
        )
        .route(
            "/_matrix/client/r0/register/available",
            get(check_username_availability),
        )
        .route(
            "/_matrix/client/v3/register/available",
            get(check_username_availability),
        )
        .route(
            "/_matrix/client/r0/register/email/requestToken",
            post(request_email_verification),
        )
        .route(
            "/_matrix/client/v3/register/email/requestToken",
            post(request_email_verification),
        )
        .route(
            "/_matrix/client/r0/register/email/submitToken",
            post(submit_email_token),
        )
        .route(
            "/_matrix/client/v3/register/email/submitToken",
            post(submit_email_token),
        )
        .route("/_matrix/client/r0/login", get(get_login_flows).post(login))
        .route("/_matrix/client/v3/login", get(get_login_flows).post(login))
        .route("/_matrix/client/r0/logout", post(logout))
        .route("/_matrix/client/r0/logout/all", post(logout_all))
        .route("/_matrix/client/v3/logout", post(logout))
        .route("/_matrix/client/v3/logout/all", post(logout_all))
        // QR Login (MSC4388)
        .route(
            "/_matrix/client/v1/login/get_qr_code",
            get(qr_login::get_qr_code),
        )
        .route(
            "/_matrix/client/v1/login/qr/confirm",
            post(qr_login::confirm_qr_login),
        )
        .route(
            "/_matrix/client/v1/login/qr/start",
            post(qr_login::start_qr_login),
        )
        .route(
            "/_matrix/client/v1/login/qr/{transaction_id}/status",
            get(qr_login::get_qr_status),
        )
        .route(
            "/_matrix/client/v1/login/qr/invalidate",
            post(qr_login::invalidate_qr_login),
        )
        .route("/_matrix/client/r0/refresh", post(refresh_token))
        .route("/_matrix/client/v3/refresh", post(refresh_token))
}

fn create_account_router() -> Router<AppState> {
    Router::new()
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
        .route(
            "/_matrix/client/r0/account/password",
            post(change_password_uia),
        )
        .route(
            "/_matrix/client/v3/account/password",
            post(change_password_uia),
        )
        .route(
            "/_matrix/client/r0/account/deactivate",
            post(deactivate_account),
        )
        .route(
            "/_matrix/client/v3/account/deactivate",
            post(deactivate_account),
        )
        .route(
            "/_matrix/client/r0/account/3pid",
            get(get_threepids).post(add_threepid),
        )
        .route("/_matrix/client/r0/account/3pid/add", post(add_threepid))
        .route("/_matrix/client/r0/account/3pid/bind", post(add_threepid))
        .route(
            "/_matrix/client/r0/account/3pid/delete",
            post(delete_threepid),
        )
        .route(
            "/_matrix/client/r0/account/3pid/unbind",
            post(unbind_threepid),
        )
        .route("/_matrix/client/r0/profile/{user_id}", get(get_profile))
        .route("/_matrix/client/v3/profile/{user_id}", get(get_profile))
        .route(
            "/_matrix/client/r0/profile/{user_id}/displayname",
            get(get_displayname).put(update_displayname),
        )
        .route(
            "/_matrix/client/v3/profile/{user_id}/displayname",
            get(get_displayname).put(update_displayname),
        )
        .route(
            "/_matrix/client/r0/profile/{user_id}/avatar_url",
            get(get_avatar_url).put(update_avatar),
        )
        .route(
            "/_matrix/client/v3/profile/{user_id}/avatar_url",
            get(get_avatar_url).put(update_avatar),
        )
}

fn create_directory_router(state: AppState) -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/r0/user_directory/search",
            post(search_user_directory),
        )
        .route(
            "/_matrix/client/v3/user_directory/search",
            post(search_user_directory),
        )
        .route(
            "/_matrix/client/r0/user_directory/list",
            post(list_user_directory),
        )
        .route(
            "/_matrix/client/v3/user_directory/list",
            post(list_user_directory),
        )
        .route(
            "/_matrix/client/r0/directory/list/room/{room_id}",
            get(get_room_visibility).put(set_room_visibility),
        )
        .route(
            "/_matrix/client/v3/directory/list/room/{room_id}",
            get(get_room_visibility).put(set_room_visibility),
        )
        .route(
            "/_matrix/client/r0/directory/room/{room_alias}",
            get(get_room_by_alias)
                .put(set_room_alias_direct)
                .delete(delete_room_alias_direct),
        )
        .route(
            "/_matrix/client/v3/directory/room/{room_alias}",
            get(get_room_by_alias)
                .put(set_room_alias_direct)
                .delete(delete_room_alias_direct),
        )
        .route(
            "/_matrix/client/r0/publicRooms",
            get(get_public_rooms).post(query_public_rooms),
        )
        .route(
            "/_matrix/client/v3/publicRooms",
            get(get_public_rooms).post(query_public_rooms),
        )
        .route(
            "/_matrix/client/r0/directory/room/{room_id}/alias",
            get(get_room_aliases),
        )
        .route(
            "/_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}",
            put(set_room_alias).delete(delete_room_alias),
        )
        .with_state(state)
}

fn create_room_router() -> Router<AppState> {
    Router::new()
        // Room info endpoint
        .route("/_matrix/client/v3/rooms/{room_id}", get(get_room_info))
        .route("/_matrix/client/r0/rooms/{room_id}", get(get_room_info))
        // Report events
        .route(
            "/_matrix/client/r0/rooms/{room_id}/report/{event_id}",
            post(report_event),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}",
            post(report_event),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/report/{event_id}",
            post(report_event),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/report/{event_id}/score",
            put(update_report_score),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}/score",
            put(update_report_score),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/report/{event_id}/score",
            put(update_report_score),
        )
        // Report room (without event_id) - MSC3891
        .route(
            "/_matrix/client/v3/rooms/{room_id}/report",
            post(report_room),
        )
        // Content scanner info - MSC3891
        .route(
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}/scanner_info",
            get(get_scanner_info),
        )
        // Room notifications - MSC3891
        .route(
            "/_matrix/client/v3/rooms/{room_id}/notifications",
            get(get_room_notifications),
        )
        // Sync & Events
        .route("/_matrix/client/r0/sync", get(sync))
        .route("/_matrix/client/r0/events", get(get_events))
        .route("/_matrix/client/v3/events", get(get_events))
        .route("/_matrix/client/r0/joined_rooms", get(get_joined_rooms))
        // Messages
        .route(
            "/_matrix/client/r0/rooms/{room_id}/messages",
            get(get_messages),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/messages",
            get(get_messages),
        )
        // Receipts
        .route(
            "/_matrix/client/r0/rooms/{room_id}/receipt/{receipt_type}/{event_id}",
            post(send_receipt),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/receipt/{receipt_type}/{event_id}",
            post(send_receipt),
        )
        // Read markers
        .route(
            "/_matrix/client/r0/rooms/{room_id}/read_markers",
            post(set_read_markers).put(set_read_markers),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/read_markers",
            post(set_read_markers).put(set_read_markers),
        )
        // Aliases
        .route(
            "/_matrix/client/r0/rooms/{room_id}/aliases",
            get(get_room_aliases),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/aliases",
            get(get_room_aliases),
        )
        // Join
        .route("/_matrix/client/r0/rooms/{room_id}/join", post(join_room))
        .route("/_matrix/client/v3/rooms/{room_id}/join", post(join_room))
        .route(
            "/_matrix/client/v3/join/{room_id_or_alias}",
            post(join_room_by_id_or_alias),
        )
        // Knock
        .route(
            "/_matrix/client/v3/knock/{room_id_or_alias}",
            post(knock_room),
        )
        // Invite by room ID (standalone endpoint)
        .route(
            "/_matrix/client/v3/invite/{room_id}",
            post(invite_user_by_room),
        )
        // Leave
        .route("/_matrix/client/r0/rooms/{room_id}/leave", post(leave_room))
        .route("/_matrix/client/v3/rooms/{room_id}/leave", post(leave_room))
        // Upgrade Room (MSC2174)
        .route(
            "/_matrix/client/r0/rooms/{room_id}/upgrade",
            post(upgrade_room),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/upgrade",
            post(upgrade_room),
        )
        // Forget
        .route(
            "/_matrix/client/r0/rooms/{room_id}/forget",
            post(forget_room),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/forget",
            post(forget_room),
        )
        // Initial Sync
        .route(
            "/_matrix/client/r0/rooms/{room_id}/initialSync",
            get(room_initial_sync),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/initialSync",
            get(room_initial_sync),
        )
        // Members
        .route(
            "/_matrix/client/r0/rooms/{room_id}/members",
            get(get_room_members),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/members",
            get(get_room_members),
        )
        // Joined Members
        .route(
            "/_matrix/client/r0/rooms/{room_id}/joined_members",
            get(get_joined_members),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/joined_members",
            get(get_joined_members),
        )
        // Invite
        .route(
            "/_matrix/client/r0/rooms/{room_id}/invite",
            post(invite_user),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/invite",
            post(invite_user),
        )
        // Invite blocklist (MSC4380)
        .route(
            "/_matrix/client/v3/rooms/{room_id}/invite_blocklist",
            get(invite_blocklist::get_invite_blocklist),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/invite_blocklist",
            post(invite_blocklist::set_invite_blocklist),
        )
        // Invite allowlist (MSC4380)
        .route(
            "/_matrix/client/v3/rooms/{room_id}/invite_allowlist",
            get(invite_blocklist::get_invite_allowlist),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/invite_allowlist",
            post(invite_blocklist::set_invite_allowlist),
        )
        // Sticky Events (MSC4354)
        .route(
            "/_matrix/client/v3/rooms/{room_id}/sticky_events",
            get(sticky_event::get_sticky_events),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/sticky_events",
            post(sticky_event::set_sticky_events),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/sticky_events/{event_type}",
            axum::routing::delete(sticky_event::clear_sticky_event),
        )
        // Create room
        .route("/_matrix/client/r0/createRoom", post(create_room))
        // User rooms
        .route(
            "/_matrix/client/r0/user/{user_id}/rooms",
            get(get_user_rooms),
        )
        .route(
            "/_matrix/client/v3/user/{user_id}/rooms",
            get(get_user_rooms),
        )
        // State events - with state_key
        .route(
            "/_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}",
            put(put_state_event).get(get_state_event),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/state/{event_type}/{state_key}",
            put(put_state_event).get(get_state_event),
        )
        // State events - empty state_key (URL ends with /)
        .route(
            "/_matrix/client/r0/rooms/{room_id}/state/{event_type}/",
            put(put_state_event_empty_key).get(get_state_event_empty_key),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/state/{event_type}/",
            put(put_state_event_empty_key).get(get_state_event_empty_key),
        )
        // Special handling for m.room.power_levels with trailing slash (SDK compatibility)
        .route(
            "/_matrix/client/r0/rooms/{room_id}/state/m.room.power_levels/",
            get(get_power_levels),
        )
        .route(
            "/_matrix/client/v1/rooms/{room_id}/state/m.room.power_levels/",
            get(get_power_levels),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/state/m.room.power_levels/",
            get(get_power_levels),
        )
        // State events - without state_key
        .route(
            "/_matrix/client/r0/rooms/{room_id}/state/{event_type}",
            put(put_state_event_no_key)
                .get(get_state_by_type)
                .post(send_state_event),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/state/{event_type}",
            put(put_state_event_no_key)
                .get(get_state_by_type)
                .post(send_state_event),
        )
        .route(
            "/_matrix/client/r0/rooms/{room_id}/state",
            get(get_room_state),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/state",
            get(get_room_state),
        )
        // Membership events
        .route(
            "/_matrix/client/r0/rooms/{room_id}/get_membership_events",
            post(get_membership_events),
        )
        // Redact
        .route(
            "/_matrix/client/r0/rooms/{room_id}/redact/{event_id}/{txn_id}",
            put(redact_event),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/redact/{event_id}/{txn_id}",
            put(redact_event),
        )
        // Kick / Ban / Unban
        .route("/_matrix/client/r0/rooms/{room_id}/kick", post(kick_user))
        .route("/_matrix/client/v3/rooms/{room_id}/kick", post(kick_user))
        .route("/_matrix/client/r0/rooms/{room_id}/ban", post(ban_user))
        .route("/_matrix/client/v3/rooms/{room_id}/ban", post(ban_user))
        .route("/_matrix/client/r0/rooms/{room_id}/unban", post(unban_user))
        .route("/_matrix/client/v3/rooms/{room_id}/unban", post(unban_user))
        // Send message
        .route(
            "/_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}",
            put(send_message),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}",
            put(send_message),
        )
        // Get single event
        .route(
            "/_matrix/client/r0/rooms/{room_id}/event/{event_id}",
            get(get_single_event),
        )
        .route(
            "/_matrix/client/v3/rooms/{room_id}/event/{event_id}",
            get(get_single_event),
        )
}

fn create_presence_router() -> Router<AppState> {
    Router::new()
        .route(
            "/_matrix/client/r0/presence/{user_id}/status",
            get(get_presence).put(set_presence),
        )
        .route(
            "/_matrix/client/v3/presence/{user_id}/status",
            get(get_presence).put(set_presence),
        )
        // Presence list endpoint (MSC2776)
        .route("/_matrix/client/v3/presence/list", post(presence_list))
}

// ============================================================================
// SECTION: Server Info & Discovery
// ============================================================================

async fn get_client_versions() -> Json<Value> {
    Json(json!({
        "versions": ["r0.5.0", "r0.6.0", "v1.1", "v1.2", "v1.3", "v1.4", "v1.5", "v1.6"],
        "unstable_features": {
            "m.lazy_load_members": true,
            "m.require_identity_server": false,
            "m.supports_login_via_phone_number": true,
            "org.matrix.msc3882": true,
            "org.matrix.msc3983": true,
            "org.matrix.msc3245": true,
            "org.matrix.msc3266": true
        }
    }))
}

async fn get_server_version() -> Json<Value> {
    Json(json!({
        "server_version": "0.1.0",
        "python_version": "3.11"
    }))
}

/// Default push rules response for /_matrix/client/v3/pushrules/
/// Returns the Matrix spec-compliant default rules
async fn get_push_rules_default(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    // Try to load user-specific push rules from DB
    let rows = sqlx::query(
        "SELECT content FROM account_data WHERE user_id = $1 AND data_type = 'm.push_rules'",
    )
    .bind(&auth_user.user_id)
    .fetch_optional(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get push rules: {}", e)))?;

    if let Some(row) = rows {
        use sqlx::Row;
        let content: Value = row.get("content");
        return Ok(Json(content));
    }

    // Return Matrix spec default push rules
    Ok(Json(get_default_push_rules()))
}

/// Default push rules for /_matrix/client/v3/pushrules/global/
async fn get_push_rules_global_default(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let rules = get_push_rules_default(State(state), auth_user).await?;
    if let Some(global) = rules.0.get("global") {
        Ok(Json(global.clone()))
    } else {
        Ok(Json(get_default_push_rules()["global"].clone()))
    }
}

/// Matrix spec-compliant default push rules
fn get_default_push_rules() -> Value {
    json!({
        "global": {
            "content": [
                {
                    "rule_id": ".m.rule.contains_display_name",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "contains_display_name"}],
                    "actions": ["notify", {"set_tweak": "highlight"}, {"set_tweak": "sound", "value": "default"}]
                }
            ],
            "override": [
                {
                    "rule_id": ".m.rule.master",
                    "default": true,
                    "enabled": false,
                    "conditions": [],
                    "actions": []
                },
                {
                    "rule_id": ".m.rule.suppress_notices",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "event_match", "key": "content.msgtype", "pattern": "m.notice"}],
                    "actions": ["dont_notify"]
                },
                {
                    "rule_id": ".m.rule.invite_for_me",
                    "default": true,
                    "enabled": true,
                    "conditions": [
                        {"kind": "event_match", "key": "type", "pattern": "m.room.member"},
                        {"kind": "event_match", "key": "content.membership", "pattern": "invite"},
                        {"kind": "event_state_key_is_me"}
                    ],
                    "actions": ["notify", {"set_tweak": "sound", "value": "default"}]
                },
                {
                    "rule_id": ".m.rule.member_event",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.room.member"}],
                    "actions": ["dont_notify"]
                },
                {
                    "rule_id": ".m.rule.call",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.call.invite"}],
                    "actions": ["notify", {"set_tweak": "sound", "value": "ring"}]
                }
            ],
            "room": [],
            "sender": [],
            "underride": [
                {
                    "rule_id": ".m.rule.message",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.room.message"}],
                    "actions": ["notify", {"set_tweak": "sound", "value": "default"}]
                },
                {
                    "rule_id": ".m.rule.encrypted",
                    "default": true,
                    "enabled": true,
                    "conditions": [{"kind": "event_match", "key": "type", "pattern": "m.room.encrypted"}],
                    "actions": ["notify", {"set_tweak": "sound", "value": "default"}]
                },
                {
                    "rule_id": ".m.rule.room_one_to_one",
                    "default": true,
                    "enabled": true,
                    "conditions": [
                        {"kind": "room_member_count", "is": "2"},
                        {"kind": "event_match", "key": "type", "pattern": "m.room.message"}
                    ],
                    "actions": ["notify", {"set_tweak": "sound", "value": "default"}]
                }
            ]
        }
    })
}

async fn get_capabilities() -> Json<Value> {
    Json(json!({
        "capabilities": {
            "m.change_password": {
                "enabled": true
            },
            "m.room_versions": {
                "default": "6",
                "available": {
                    "1": "stable",
                    "2": "stable",
                    "3": "stable",
                    "4": "stable",
                    "5": "stable",
                    "6": "stable",
                    "7": "stable",
                    "8": "stable",
                    "9": "stable",
                    "10": "stable",
                    "11": "stable"
                }
            },
            "m.set_displayname": {
                "enabled": true
            },
            "m.set_avatar_url": {
                "enabled": true
            },
            "m.3pid_changes": {
                "enabled": true
            },
            "m.room.summary": {
                "enabled": true
            },
            "m.room.suggested": {
                "enabled": true
            }
        }
    }))
}

async fn get_well_known_server(State(state): State<AppState>) -> Json<Value> {
    let server_name = &state.services.config.server.name;
    let port = state.services.config.server.port;

    Json(json!({
        "m.server": format!("{}:{}", server_name, port)
    }))
}

async fn get_well_known_client(State(state): State<AppState>) -> Json<Value> {
    let base_url = state.services.config.server.get_public_baseurl();

    Json(json!({
        "m.homeserver": {
            "base_url": base_url
        },
        "m.identity_server": {
            "base_url": base_url
        }
    }))
}

async fn get_well_known_support(State(state): State<AppState>) -> Json<Value> {
    let server_name = &state.services.config.server.name;

    Json(json!({
        "contacts": [
            {
                "email_address": format!("admin@{}", server_name),
                "matrix_id": format!("@admin:{}", server_name),
                "role": "m.admin"
            }
        ],
        "support_page": format!("https://{}/support", server_name)
    }))
}

async fn health_check(State(state): State<AppState>) -> Json<Value> {
    let status = state.health_checker.check_readiness().await;
    Json(
        serde_json::to_value(status)
            .unwrap_or_else(|_| json!({"status": "unhealthy", "error": "serialization error"})),
    )
}

// ============================================================================
// SECTION: Authentication & Registration
// ============================================================================

async fn register(
    State(state): State<AppState>,
    MatrixJson(body): MatrixJson<Value>,
) -> Result<Json<Value>, ApiError> {
    let auth = body.get("auth").cloned();
    let auth_type = auth
        .as_ref()
        .and_then(|a| a.get("type"))
        .and_then(|t| t.as_str());

    let username = body.get("username").and_then(|v| v.as_str());
    let password = body.get("password").and_then(|v| v.as_str());

    if username.is_none() || password.is_none() {
        if auth_type == Some("m.login.dummy") || auth_type == Some("m.login.password") {
            return Err(ApiError::bad_request(
                "Username and password required".to_string(),
            ));
        }
        return Ok(Json(json!({
            "flows": [
                { "stages": ["m.login.dummy"] },
                { "stages": ["m.login.password"] }
            ],
            "params": {},
            "session": uuid::Uuid::new_v4().to_string()
        })));
    }

    let username =
        username.ok_or_else(|| ApiError::bad_request("Username required".to_string()))?;
    let password =
        password.ok_or_else(|| ApiError::bad_request("Password required".to_string()))?;

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

    let client_secret = body.get("client_secret").and_then(|v| v.as_str());
    if client_secret.is_none() {
        return Err(ApiError::bad_request(
            "client_secret is required".to_string(),
        ));
    }

    let _send_attempt = body
        .get("send_attempt")
        .and_then(|v| v.as_u64())
        .unwrap_or(1);

    let token = state
        .services
        .auth_service
        .generate_email_verification_token()
        .map_err(|e| {
            ::tracing::error!("Failed to generate email verification token: {}", e);
            ApiError::internal(
                "Failed to generate verification token. Please try again later.".to_string(),
            )
        })?;

    let session_data = body.get("client_secret").cloned();

    let token_id = state
        .services
        .email_verification_storage
        .create_verification_token(email, &token, 3600, None, session_data)
        .await
        .map_err(|e| {
            ::tracing::error!("Failed to store email verification token: {}", e);
            ApiError::internal(
                "Failed to store verification token. Please try again later.".to_string(),
            )
        })?;

    let sid = format!("{}", token_id);

    let submit_url = format!(
        "https://{}:{}/_matrix/client/r0/register/email/submitToken",
        state.services.config.server.host, state.services.config.server.port
    );

    ::tracing::info!(
        "Email verification token created for {}: sid={}",
        email,
        sid
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

async fn get_login_flows() -> Json<Value> {
    Json(json!({
        "flows": [
            {"type": "m.login.password"},
            {"type": "m.login.token"}
        ]
    }))
}

async fn get_register_flows() -> Json<Value> {
    Json(json!({
        "flows": [
            {"type": "m.login.dummy"},
            {"type": "m.login.password"}
        ],
        "params": {}
    }))
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
        return Err(ApiError::bad_request(
            "Password too long (max 128 characters)".to_string(),
        ));
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

async fn get_single_event(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    let is_member = state
        .services
        .member_storage
        .is_member(&room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if !is_member && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "You must be a member of this room to view events".to_string(),
        ));
    }

    let event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if event.room_id != room_id {
        return Err(ApiError::not_found(
            "Event not found in this room".to_string(),
        ));
    }

    Ok(Json(json!({
        "event_id": event.event_id,
        "room_id": event.room_id,
        "sender": event.user_id,
        "type": event.event_type,
        "content": event.content,
        "origin_server_ts": event.origin_server_ts,
        "state_key": event.state_key
    })))
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
    State(_state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    // Matrix spec: https://spec.matrix.org/v1.11/client-server-api/#get_matrixclientv3accountwhoami
    // Only user_id, device_id, and optionally is_guest
    Ok(Json(json!({
        "user_id": auth_user.user_id,
        "device_id": auth_user.device_id,
        "is_guest": false
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
        return Err(ApiError::bad_request(
            "Invalid user_id format: must start with @".to_string(),
        ));
    }

    if user_id.len() > 255 {
        return Err(ApiError::bad_request(
            "user_id too long (max 255 characters)".to_string(),
        ));
    }

    let parts: Vec<&str> = user_id.split(':').collect();
    if parts.len() < 2 {
        return Err(ApiError::bad_request(
            "Invalid user_id format: must be @username:server".to_string(),
        ));
    }

    let username = &parts[0][1..];
    if username.is_empty() {
        return Err(ApiError::bad_request(
            "Invalid user_id format: username cannot be empty".to_string(),
        ));
    }

    if parts[1].is_empty() {
        return Err(ApiError::bad_request(
            "Invalid user_id format: server cannot be empty".to_string(),
        ));
    }

    Ok(())
}

fn validate_room_id(room_id: &str) -> Result<(), ApiError> {
    if room_id.is_empty() {
        return Err(ApiError::bad_request("room_id is required".to_string()));
    }
    if !room_id.starts_with('!') {
        return Err(ApiError::bad_request(
            "Invalid room_id format: must start with !".to_string(),
        ));
    }
    if room_id.len() > 255 {
        return Err(ApiError::bad_request(
            "room_id too long (max 255 characters)".to_string(),
        ));
    }
    Ok(())
}

fn validate_event_id(event_id: &str) -> Result<(), ApiError> {
    if event_id.is_empty() {
        return Err(ApiError::bad_request("event_id is required".to_string()));
    }
    if !event_id.starts_with('$') {
        return Err(ApiError::bad_request(
            "Invalid event_id format: must start with $".to_string(),
        ));
    }
    if event_id.len() > 255 {
        return Err(ApiError::bad_request(
            "event_id too long (max 255 characters)".to_string(),
        ));
    }
    Ok(())
}

fn validate_presence_status(presence: &str) -> Result<(), ApiError> {
    let valid_statuses = ["online", "offline", "unavailable"];
    if !valid_statuses.contains(&presence) {
        return Err(ApiError::bad_request(format!(
            "Invalid presence status. Must be one of: {}",
            valid_statuses.join(", ")
        )));
    }
    Ok(())
}

fn validate_receipt_type(receipt_type: &str) -> Result<(), ApiError> {
    let valid_types = ["m.read", "m.read.private"];
    if !valid_types.contains(&receipt_type) {
        return Err(ApiError::bad_request(format!(
            "Invalid receipt type. Must be one of: {}",
            valid_types.join(", ")
        )));
    }
    Ok(())
}

async fn get_profile(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let token = extract_token_from_headers(&headers).ok();
    let requester_id = if let Some(t) = token {
        state
            .services
            .auth_service
            .validate_token(&t)
            .await
            .ok()
            .map(|(id, _, _)| id)
    } else {
        None
    };

    let privacy_settings = state
        .services
        .privacy_storage
        .get_settings(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if let Some(settings) = privacy_settings {
        let visibility = settings.profile_visibility.as_str();
        match visibility {
            "private" => {
                if requester_id.as_deref() != Some(user_id.as_str()) {
                    return Err(ApiError::forbidden("Profile is private".to_string()));
                }
            }
            "contacts" => {
                if requester_id.as_deref() != Some(user_id.as_str()) {
                    return Err(ApiError::forbidden(
                        "Profile is only visible to contacts".to_string(),
                    ));
                }
            }
            _ => {}
        }
    }

    Ok(Json(
        state
            .services
            .registration_service
            .get_profile(&user_id)
            .await?,
    ))
}

async fn get_displayname(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let profile = state
        .services
        .registration_service
        .get_profile(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get profile: {}", e)))?;

    let displayname = profile
        .get("displayname")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    Ok(Json(json!({ "displayname": displayname })))
}

async fn get_avatar_url(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let profile = state
        .services
        .registration_service
        .get_profile(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get profile: {}", e)))?;

    let avatar_url = profile
        .get("avatar_url")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    Ok(Json(json!({ "avatar_url": avatar_url })))
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
        return Err(ApiError::bad_request(
            "Displayname too long (max 255 characters)".to_string(),
        ));
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
        return Err(ApiError::bad_request(
            "Avatar URL too long (max 255 characters)".to_string(),
        ));
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

#[allow(dead_code)]
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

async fn change_password_uia(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let new_password = body
        .get("new_password")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("New password required".to_string()))?;

    let auth = body.get("auth").cloned().unwrap_or(serde_json::json!({}));
    let auth_type = auth.get("type").and_then(|v| v.as_str()).unwrap_or("");

    if auth_type == "m.login.dummy" {
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
    } else if auth_type == "m.login.password" {
        let password = auth
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ApiError::bad_request("Password required for m.login.password".to_string())
            })?;

        let user_identifier = auth
            .get("identifier")
            .and_then(|i| i.get("user"))
            .and_then(|u| u.as_str())
            .or_else(|| auth.get("user").and_then(|u| u.as_str()));

        if let Some(username) = user_identifier {
            let user_id = if username.starts_with('@') {
                username.to_string()
            } else {
                format!("@{}:{}", username, state.services.server_name)
            };

            if user_id != auth_user.user_id {
                return Err(ApiError::forbidden("User mismatch".to_string()));
            }

            state
                .services
                .auth_service
                .validator
                .validate_password(new_password)?;

            let user = state
                .services
                .user_storage
                .get_user_by_id(&user_id)
                .await
                .map_err(|e| ApiError::internal(format!("Failed to get user: {}", e)))?
                .ok_or_else(|| ApiError::not_found("User not found".to_string()))?;

            let password_hash = user
                .password_hash
                .ok_or_else(|| ApiError::forbidden("User has no password set".to_string()))?;

            let valid = crate::common::crypto::verify_password(password, &password_hash, false)
                .map_err(|e| ApiError::internal(format!("Password verification failed: {}", e)))?;

            if !valid {
                return Err(ApiError::forbidden("Invalid password".to_string()));
            }

            state
                .services
                .registration_service
                .change_password(&auth_user.user_id, new_password)
                .await?;

            Ok(Json(json!({})))
        } else {
            Err(ApiError::bad_request(
                "User identifier required".to_string(),
            ))
        }
    } else {
        Err(ApiError::unauthorized(
            "Authentication required".to_string(),
        ))
    }
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

    Ok(Json(json!({
        "id_server_unbind_result": "success"
    })))
}

async fn get_threepids(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    let threepids = sqlx::query(
        r#"
        SELECT medium, address, validated_ts, added_ts
        FROM user_threepids
        WHERE user_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get threepids: {}", e)))?;

    let threepids_list: Vec<Value> = threepids
        .iter()
        .map(|row| {
            json!({
                "medium": row.get::<String, _>("medium"),
                "address": row.get::<String, _>("address"),
                "validated_ts": row.get::<Option<i64>, _>("validated_ts").unwrap_or(0),
                "added_at": row.get::<Option<i64>, _>("added_ts").unwrap_or(0)
            })
        })
        .collect();

    Ok(Json(json!({
        "threepids": threepids_list
    })))
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct AddThreepidRequest {
    #[serde(rename = "threePidCreds")]
    three_pid_creds: Option<ThreepidCreds>,
    bind: Option<bool>,
}

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct ThreepidCreds {
    client_secret: Option<String>,
    sid: Option<String>,
}

async fn add_threepid(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;
    let now = chrono::Utc::now().timestamp_millis();

    let medium = body
        .get("medium")
        .and_then(|v| v.as_str())
        .unwrap_or("email");
    let address = body.get("address").and_then(|v| v.as_str()).unwrap_or("");

    if address.is_empty() {
        return Err(ApiError::bad_request("Address is required".to_string()));
    }

    sqlx::query(
        r#"
        INSERT INTO user_threepids (user_id, medium, address, validated_ts, added_ts)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (medium, address) DO UPDATE
        SET validated_ts = EXCLUDED.validated_ts
        "#,
    )
    .bind(user_id)
    .bind(medium)
    .bind(address)
    .bind(now)
    .bind(now)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to add threepid: {}", e)))?;

    Ok(Json(json!({})))
}

#[derive(Debug, serde::Deserialize)]
struct DeleteThreepidRequest {
    medium: String,
    address: String,
}

async fn delete_threepid(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<DeleteThreepidRequest>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    sqlx::query(
        r#"
        DELETE FROM user_threepids
        WHERE user_id = $1 AND medium = $2 AND address = $3
        "#,
    )
    .bind(user_id)
    .bind(&body.medium)
    .bind(&body.address)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to delete threepid: {}", e)))?;

    Ok(Json(json!({})))
}

async fn unbind_threepid(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<DeleteThreepidRequest>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    sqlx::query(
        r#"
        DELETE FROM user_threepids
        WHERE user_id = $1 AND medium = $2 AND address = $3
        "#,
    )
    .bind(user_id)
    .bind(&body.medium)
    .bind(&body.address)
    .execute(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to unbind threepid: {}", e)))?;

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

// Report room (MSC3891) - report entire room without specific event
async fn report_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    // Check if room exists by querying room members
    let members = sqlx::query("SELECT room_id FROM room_members WHERE room_id = $1 LIMIT 1")
        .bind(&room_id)
        .fetch_optional(&*state.services.user_storage.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    if members.is_none() {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    // Get reason and optional description from request body
    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("No reason provided");
    let description = body.get("description").and_then(|v| v.as_str());

    // Log the room report for moderation purposes
    ::tracing::info!(
        "Room report submitted: room_id={}, user_id={}, reason={}",
        room_id,
        auth_user.user_id,
        reason
    );

    // Return a report ID (in a full implementation, this would be a real DB entry)
    let report_id = format!("{}_{}", room_id, chrono::Utc::now().timestamp_millis());

    Ok(Json(json!({
        "report_id": report_id,
        "room_id": room_id,
        "reason": reason,
        "description": description,
        "status": "submitted"
    })))
}

// Get content scanner info (MSC3891) - get scanner information for an event
async fn get_scanner_info(
    State(_state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((room_id, event_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    // In a full implementation, this would check with an external content scanner service
    // For now, return placeholder data indicating scanner is not configured
    Ok(Json(json!({
        "scanner_enabled": false,
        "room_id": room_id,
        "event_id": event_id,
        "status": "not_configured",
        "message": "Content scanner is not enabled on this server"
    })))
}

// Get room notifications (MSC3891) - get notifications for a specific room
async fn get_room_notifications(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);

    let _from = params.get("from").cloned();

    // Get notifications for this specific room
    let notifications = sqlx::query(
        r#"
        SELECT event_id, room_id, ts, notification_type, is_read
        FROM notifications
        WHERE user_id = $1 AND room_id = $2
        ORDER BY ts DESC
        LIMIT $3
        "#,
    )
    .bind(&auth_user.user_id)
    .bind(&room_id)
    .bind(limit as i64)
    .fetch_all(&*state.services.user_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let notifications_list: Vec<Value> = notifications
        .iter()
        .map(|row| {
            let event_id = row.get::<Option<String>, _>("event_id").unwrap_or_default();
            json!({
                "event_id": event_id,
                "room_id": row.get::<Option<String>, _>("room_id"),
                "ts": row.get::<Option<i64>, _>("ts"),
                "profile_tag": row.get::<Option<String>, _>("notification_type"),
                "read": row.get::<Option<bool>, _>("is_read").unwrap_or(false),
                "room_name": None::<Value>,
                "sender": None::<Value>,
                "prio": "high",
                "client_action": "notify"
            })
        })
        .collect();

    Ok(Json(json!({
        "notifications": notifications_list,
        "next_token": None::<String>
    })))
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
    let since = params.get("since").and_then(|v| v.as_str());

    let sync_result = tokio::time::timeout(
        std::time::Duration::from_secs(60),
        state
            .services
            .sync_service
            .sync(&user_id, timeout, full_state, set_presence, since),
    )
    .await;

    match sync_result {
        Ok(Ok(result)) => Ok(Json(result)),
        Ok(Err(e)) => {
            ::tracing::error!("Sync error for user {}: {}", user_id, e);
            Err(e)
        }
        Err(_) => {
            ::tracing::error!("Sync timeout for user {}", user_id);
            Err(ApiError::internal("Sync operation timed out".to_string()))
        }
    }
}

#[derive(serde::Serialize)]
#[allow(dead_code)]
struct FilterResponse {
    filter_id: String,
    room: Option<Value>,
    presence: Option<Value>,
}

#[allow(dead_code)]
async fn create_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(_user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<FilterResponse>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (_, _, _) = state.services.auth_service.validate_token(&token).await?;

    let filter_id = format!("f{}", uuid::Uuid::new_v4());

    Ok(Json(FilterResponse {
        filter_id,
        room: body.get("room").cloned(),
        presence: body.get("presence").cloned(),
    }))
}

#[allow(dead_code)]
async fn get_filter(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path((_user_id, filter_id)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (_, _, _) = state.services.auth_service.validate_token(&token).await?;

    Ok(Json(json!({
        "filter_id": filter_id,
        "filter": {
            "room": {
                "state": {"limit": 50},
                "timeline": {"limit": 50}
            },
            "presence": {"limit": 100}
        }
    })))
}

async fn get_events(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let from = params.get("from").and_then(|v| v.as_str()).unwrap_or("0");
    let timeout = params
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(30000);

    let result = state
        .services
        .sync_service
        .get_events(&user_id, from, timeout)
        .await
        .unwrap_or(json!({
            "start": from,
            "end": from,
            "chunk": []
        }));

    Ok(Json(result))
}

// ============================================================================
// SECTION: Room Management
// ============================================================================

async fn get_room_info(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let user_id = &auth_user.user_id;

    let membership = sqlx::query(
        r#"
        SELECT membership 
        FROM room_memberships 
        WHERE room_id = $1 AND user_id = $2
        "#,
    )
    .bind(&room_id)
    .bind(user_id)
    .fetch_optional(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to check room membership: {}", e)))?;

    let membership = match membership {
        Some(row) => row.get::<Option<String>, _>("membership"),
        None => None,
    };

    if membership.is_none() {
        return Err(ApiError::not_found(
            "Room not found or not a member".to_string(),
        ));
    }

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    Ok(Json(json!({
        "room_id": room_id,
        "name": room.name,
        "avatar_url": room.avatar_url,
        "topic": room.topic,
        "canonical_alias": room.canonical_alias,
        "joined_members_count": room.member_count,
        "invited_members_count": 0,
        "world_readable": room.is_public,
        "guest_can_join": false,
        "membership": membership
    })))
}

async fn get_joined_rooms(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    let rooms = sqlx::query(
        r#"
        SELECT DISTINCT room_id 
        FROM room_memberships 
        WHERE user_id = $1 AND membership = 'join'
        ORDER BY room_id
        "#,
    )
    .bind(user_id)
    .fetch_all(&*state.services.room_storage.pool)
    .await
    .map_err(|e| ApiError::internal(format!("Failed to get joined rooms: {}", e)))?;

    let room_ids: Vec<String> = rooms
        .iter()
        .filter_map(|row| row.get::<Option<String>, _>("room_id"))
        .collect();

    Ok(Json(json!({
        "joined_rooms": room_ids
    })))
}

async fn get_messages(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Query(params): Query<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

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
            return Err(ApiError::bad_request(
                "Message body too long (max 64KB)".to_string(),
            ));
        }
    } else {
        let s = content.to_string();
        if s.len() > 65536 {
            return Err(ApiError::bad_request(
                "Message body too long (max 64KB)".to_string(),
            ));
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

async fn join_room_by_id_or_alias(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id_or_alias): Path<String>,
    body: Option<Json<serde_json::Value>>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let room_id = if room_id_or_alias.starts_with('!') {
        room_id_or_alias.clone()
    } else if room_id_or_alias.starts_with('#') {
        state
            .services
            .room_service
            .get_room_by_alias(&room_id_or_alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    } else {
        let alias = format!(
            "#{}:{}",
            room_id_or_alias, state.services.config.server.name
        );
        state
            .services
            .room_service
            .get_room_by_alias(&alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    };

    let via_servers = body
        .and_then(|b| b.get("via_servers").and_then(|v| v.as_array()).cloned())
        .unwrap_or_default();

    ::tracing::info!(
        "User {} joining room {} via {:?}",
        user_id,
        room_id,
        via_servers
    );

    state
        .services
        .room_service
        .join_room(&room_id, &user_id)
        .await?;

    Ok(Json(json!({
        "room_id": room_id
    })))
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

#[derive(Debug, Deserialize)]
struct UpgradeRoomRequest {
    new_version: String,
}

#[derive(Debug, Serialize)]
struct UpgradeRoomResponse {
    replacement_room: String,
}

/// Upgrade Room (MSC2174)
/// POST /_matrix/client/v3/rooms/{room_id}/upgrade
async fn upgrade_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<UpgradeRoomRequest>,
) -> Result<Json<UpgradeRoomResponse>, ApiError> {
    validate_room_id(&room_id)?;

    let new_room_id = state
        .services
        .room_service
        .upgrade_room(&room_id, &body.new_version, &auth_user.user_id)
        .await?;

    Ok(Json(UpgradeRoomResponse {
        replacement_room: new_room_id,
    }))
}

async fn forget_room(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    state
        .services
        .room_service
        .forget_room(&room_id, &auth_user.user_id)
        .await?;
    Ok(Json(json!({})))
}

/// Room Initial Sync
/// GET /_matrix/client/r0/rooms/{room_id}/initialSync
/// GET /_matrix/client/v3/rooms/{room_id}/initialSync
///
/// Room Initial Sync
/// GET /_matrix/client/r0/rooms/{room_id}/initialSync
/// GET /_matrix/client/v3/rooms/{room_id}/initialSync
///
/// Returns initial sync data for a room including:
/// - Room state events
/// - Timeline messages  
/// - Presence events
/// - Member list
/// - Account data
async fn room_initial_sync(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    // 检查房间是否存在
    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await?
        .ok_or_else(|| ApiError::not_found("Room not found"))?;

    // 检查用户是否是房间成员
    let member = state
        .services
        .member_storage
        .get_member(&room_id, &auth_user.user_id)
        .await?;

    if member.is_none() {
        return Err(ApiError::forbidden("You are not a member of this room"));
    }

    // 获取成员列表
    let members = state
        .services
        .member_storage
        .get_joined_members(&room_id)
        .await
        .unwrap_or_default();

    // 构建完整响应
    let mut response = json!({
        "room_id": room_id,
        "messages": {
            "chunk": [],
            "start": "s",
            "end": "e"
        },
        "state": [],
        "presence": [],
        "account_data": [],
        "members": members,
        "num_joined_members": members.len(),
    });

    // 添加房间元数据
    if let Some(name) = room.name {
        response["name"] = serde_json::Value::String(name);
    }
    if let Some(topic) = room.topic {
        response["topic"] = serde_json::Value::String(topic);
    }
    if let Some(avatar_url) = room.avatar_url {
        response["avatar_url"] = serde_json::Value::String(avatar_url);
    }

    // 添加创建信息
    response["created_by"] = serde_json::Value::String(room.creator_user_id.unwrap_or_default());
    response["created_ts"] = serde_json::Value::Number(serde_json::Number::from(room.created_ts));

    Ok(Json(response))
}

async fn get_room_members(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let room = room.ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let is_member = state
        .services
        .member_storage
        .get_member(&room_id, &user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .is_some();

    if !room.is_public && !is_member {
        ::tracing::warn!(
            target: "security_audit",
            event = "unauthorized_room_members_access",
            user_id = user_id,
            room_id = room_id,
            "User attempted to access members of private room without being a member"
        );
        return Err(ApiError::forbidden(
            "You must be a member to view the member list of this private room".to_string(),
        ));
    }

    let members = state
        .services
        .room_service
        .get_room_members(&room_id, &user_id)
        .await?;
    Ok(Json(members))
}

async fn get_joined_members(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let room = state
        .services
        .room_storage
        .get_room(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

    let room = room.ok_or_else(|| ApiError::not_found("Room not found".to_string()))?;

    let is_member = state
        .services
        .member_storage
        .get_member(&room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
        .is_some();

    if !room.is_public && !is_member {
        return Err(ApiError::forbidden(
            "You must be a member to view the joined members of this private room".to_string(),
        ));
    }

    let members = state
        .services
        .member_storage
        .get_joined_members(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get joined members: {}", e)))?;

    let joined: std::collections::HashMap<String, Value> = members
        .into_iter()
        .map(|m| {
            let user_id = m.user_id.clone();
            (
                user_id,
                json!({
                    "display_name": m.display_name,
                    "avatar_url": m.avatar_url
                }),
            )
        })
        .collect();

    Ok(Json(json!({
        "joined": joined
    })))
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

async fn knock_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id_or_alias): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    let room_id = if room_id_or_alias.starts_with('!') {
        room_id_or_alias.clone()
    } else if room_id_or_alias.starts_with('#') {
        state
            .services
            .room_service
            .get_room_by_alias(&room_id_or_alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    } else {
        let alias = format!(
            "#{}:{}",
            room_id_or_alias, state.services.config.server.name
        );
        state
            .services
            .room_service
            .get_room_by_alias(&alias)
            .await
            .map_err(|e| ApiError::not_found(format!("Room alias not found: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Room ID not found for alias".to_string()))?
    };

    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    ::tracing::info!("User {} knocking on room {}", user_id, room_id);

    state
        .services
        .room_service
        .knock_room(&room_id, &user_id, reason.as_deref())
        .await?;

    Ok(Json(json!({
        "room_id": room_id
    })))
}

async fn invite_user_by_room(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let token = extract_token_from_headers(&headers)?;
    let (user_id, _, _) = state.services.auth_service.validate_token(&token).await?;

    validate_room_id(&room_id)?;

    let invitee = body
        .get("user_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("User ID required".to_string()))?;

    validate_user_id(invitee)?;

    ::tracing::info!("User {} inviting {} to room {}", user_id, invitee, room_id);

    state
        .services
        .room_service
        .invite_user(&room_id, &user_id, invitee)
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
            return Err(ApiError::bad_request(
                "Room alias name too long".to_string(),
            ));
        }
        // Validate alias format (localpart only, usually)
        // But spec says room_alias_name is the local part.
        // Let's rely on basic char check if needed, but length is most important for DoS.
        if !alias
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
        {
            return Err(ApiError::bad_request(
                "Invalid characters in room alias name".to_string(),
            ));
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
            .collect::<Vec<String>>()
    });

    if let Some(ref inv) = invite {
        if inv.len() > 100 {
            return Err(ApiError::bad_request(
                "Too many invites (max 100)".to_string(),
            ));
        }
    }

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

    let result = state
        .services
        .room_service
        .create_room(&user_id, config)
        .await?;

    // Create room summary
    let room_id = result.get("room_id").and_then(|r| r.as_str()).unwrap_or("");
    if !room_id.is_empty() {
        let _ = state
            .services
            .room_summary_storage
            .create_summary(crate::storage::room_summary::CreateRoomSummaryRequest {
                room_id: room_id.to_string(),
                room_type: None,
                name: name.map(|s| s.to_string()),
                topic: topic.map(|s| s.to_string()),
                avatar_url: None,
                canonical_alias: None,
                join_rules: None,
                history_visibility: None,
                guest_access: None,
                is_direct: None,
                is_space: None,
            })
            .await;
    }

    Ok(Json(result))
}

#[axum::debug_handler]
async fn get_room_visibility(
    State(state): State<AppState>,
    _auth_user: OptionalAuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let visibility = state
        .services
        .room_service
        .get_room_visibility(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get room visibility: {}", e)))?;

    Ok(Json(json!({
        "visibility": visibility
    })))
}

#[axum::debug_handler]
async fn set_room_visibility(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let visibility = body
        .get("visibility")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing visibility field".to_string()))?;

    if visibility != "public" && visibility != "private" {
        return Err(ApiError::bad_request(
            "visibility must be 'public' or 'private'".to_string(),
        ));
    }

    let is_public = visibility == "public";

    if is_public {
        let is_creator = state
            .services
            .room_service
            .is_room_creator(&room_id, &auth_user.user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check room creator: {}", e)))?;

        if !auth_user.is_admin && !is_creator {
            return Err(ApiError::forbidden(
                "Only room creator or server admin can set room to public".to_string(),
            ));
        }
    }

    state
        .services
        .room_service
        .set_room_directory(&room_id, is_public)
        .await?;

    Ok(Json(json!({})))
}

async fn get_room_aliases(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

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
    if room_alias.len() > 255 {
        return Err(ApiError::bad_request(
            "Alias too long (max 255 characters)".to_string(),
        ));
    }

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let is_member = state
        .services
        .member_storage
        .is_member(&room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if !is_member && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "You must be a room member to set an alias".to_string(),
        ));
    }

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
    _auth_user: OptionalAuthenticatedUser,
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

#[axum::debug_handler]
async fn set_room_alias_direct(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(room_alias): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let room_id = body
        .get("room_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Missing room_id field".to_string()))?;

    if !room_id.starts_with('!') {
        return Err(ApiError::bad_request("Invalid room_id format".to_string()));
    }

    if room_alias.len() > 255 {
        return Err(ApiError::bad_request(
            "Alias too long (max 255 characters)".to_string(),
        ));
    }

    if !state
        .services
        .room_storage
        .room_exists(room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let is_member = state
        .services
        .member_storage
        .is_member(room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if !is_member && !auth_user.is_admin {
        return Err(ApiError::forbidden(
            "You must be a room member to set an alias".to_string(),
        ));
    }

    state
        .services
        .room_service
        .set_room_alias(room_id, &room_alias, &auth_user.user_id)
        .await?;
    Ok(Json(json!({})))
}

#[axum::debug_handler]
async fn delete_room_alias_direct(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_alias): Path<String>,
) -> Result<Json<Value>, ApiError> {
    state
        .services
        .room_service
        .remove_room_alias_by_name(&room_alias)
        .await?;
    Ok(Json(json!({})))
}

async fn get_public_rooms(
    State(state): State<AppState>,
    _auth_user: OptionalAuthenticatedUser,
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
async fn query_public_rooms(
    State(state): State<AppState>,
    _auth_user: OptionalAuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let limit = body.get("limit").and_then(|v| v.as_u64()).unwrap_or(10);
    let _since = body.get("since").and_then(|v| v.as_str());
    let _filter = body.get("filter");

    Ok(Json(
        state
            .services
            .room_service
            .get_public_rooms(limit as i64)
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

async fn get_presence(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;

    let user_exists = state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?;

    if !user_exists {
        return Err(ApiError::not_found("User not found".to_string()));
    }

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
    validate_user_id(&user_id)?;

    if auth_user.user_id != user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let presence = body
        .get("presence")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Presence required".to_string()))?;

    validate_presence_status(presence)?;

    let status_msg = body.get("status_msg").and_then(|v| v.as_str());

    if let Some(msg) = status_msg {
        if msg.len() > MAX_MESSAGE_LENGTH {
            return Err(ApiError::bad_request(format!(
                "Status message too long (max {} characters)",
                MAX_MESSAGE_LENGTH
            )));
        }
    }

    state
        .services
        .presence_storage
        .set_presence(&user_id, presence, status_msg)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to set presence: {}", e)))?;

    Ok(Json(json!({})))
}

/// Presence list endpoint (MSC2776)
/// Subscribe/unsubscribe to presence status updates for a list of users
async fn presence_list(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let user_id = &auth_user.user_id;

    // Handle subscriptions (subscribe to users' presence)
    if let Some(subscribe) = body.get("subscribe").and_then(|v| v.as_array()) {
        for target in subscribe {
            if let Some(target_id) = target.as_str() {
                validate_user_id(target_id)?;

                // Add subscription
                if let Err(e) = state
                    .services
                    .presence_storage
                    .add_subscription(user_id, target_id)
                    .await
                {
                    ::tracing::warn!("Failed to add presence subscription: {}", e);
                }
            }
        }
    }

    // Handle unsubscriptions (unsubscribe from users' presence)
    if let Some(unsubscribe) = body.get("unsubscribe").and_then(|v| v.as_array()) {
        for target in unsubscribe {
            if let Some(target_id) = target.as_str() {
                validate_user_id(target_id)?;

                // Remove subscription
                if let Err(e) = state
                    .services
                    .presence_storage
                    .remove_subscription(user_id, target_id)
                    .await
                {
                    ::tracing::warn!("Failed to remove presence subscription: {}", e);
                }
            }
        }
    }

    // Get current subscriptions and their presence
    let subscriptions = state
        .services
        .presence_storage
        .get_subscriptions(user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get subscriptions: {}", e)))?;

    // Batch fetch presence for all subscribed users
    let presence_batch = state
        .services
        .presence_storage
        .get_presence_batch(&subscriptions)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get presence batch: {}", e)))?;

    // Build response - presence_batch already contains all the presence info we need
    let mut presences = Vec::new();

    for (target_id, presence, status_msg) in presence_batch {
        // Calculate last_active_ago - we don't have exact timestamp but use presence update time
        let last_active_ago = if presence != "offline" {
            Some(0) // Simplified: indicates user is currently active
        } else {
            None
        };

        presences.push(json!({
            "user_id": target_id,
            "presence": presence,
            "status_msg": status_msg,
            "last_active_ago": last_active_ago
        }));
    }

    // Also include any users that were just subscribed but not in the database yet
    for target_id in &subscriptions {
        if !presences.iter().any(|p| p["user_id"] == *target_id) {
            presences.push(json!({
                "user_id": target_id,
                "presence": "offline",
                "status_msg": None::<String>,
                "last_active_ago": None::<i64>
            }));
        }
    }

    Ok(Json(json!({
        "presences": presences
    })))
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

    // Return array directly per Matrix spec
    Ok(Json(JsonValue::Array(state_events)))
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

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, &final_event_type)
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

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, &final_event_type)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

    let event = events
        .iter()
        .find(|e| {
            e.state_key.as_deref() == Some(state_key.as_str())
                || (e.state_key.as_ref().map(|s| s.is_empty()) == Some(true)
                    && state_key.is_empty())
        })
        .ok_or_else(|| ApiError::not_found("State event not found".to_string()))?;

    let mut response = json!({
        "type": event.event_type,
        "event_id": event.event_id,
        "sender": event.sender,
        "state_key": event.state_key
    });

    if let Some(content) = event.content.as_object() {
        for (k, v) in content {
            response[k] = v.clone();
        }
    }

    Ok(Json(response))
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

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let state_event = state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type.clone(),
                content,
                state_key: Some(auth_user.user_id.clone()),
                origin_server_ts: now,
            },
            None,
        )
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

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let event = state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type.clone(),
                content: body,
                state_key: Some(state_key),
                origin_server_ts: now,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to put state event: {}", e)))?;

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": event.event_type,
        "state_key": event.state_key
    })))
}

async fn get_state_event_empty_key(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, &final_event_type)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get state: {}", e)))?;

    let event = events
        .iter()
        .find(|e| e.state_key.as_ref().map(|s| s.is_empty()) == Some(true))
        .ok_or_else(|| ApiError::not_found("State event not found".to_string()))?;

    let mut response = json!({
        "type": event.event_type,
        "event_id": event.event_id,
        "sender": event.sender,
        "state_key": event.state_key
    });

    if let Some(content) = event.content.as_object() {
        for (k, v) in content {
            response[k] = v.clone();
        }
    }

    Ok(Json(response))
}

async fn get_power_levels(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(room_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let events = state
        .services
        .event_storage
        .get_state_events_by_type(&room_id, "m.room.power_levels")
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get power levels: {}", e)))?;

    let event = events
        .iter()
        .find(|e| e.state_key.as_ref().map(|s| s.is_empty()) == Some(true))
        .ok_or_else(|| ApiError::not_found("Power levels not found".to_string()))?;

    let power_levels_content = event.content.clone();

    Ok(Json(power_levels_content))
}

async fn put_state_event_empty_key(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let event = state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type.clone(),
                content: body,
                state_key: Some("".to_string()),
                origin_server_ts: now,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to put state event: {}", e)))?;

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": event.event_type,
        "state_key": event.state_key
    })))
}

async fn put_state_event_no_key(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, event_type)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let final_event_type = if event_type.starts_with("m.room.") || event_type.starts_with("m.") {
        event_type.clone()
    } else {
        format!("m.room.{}", event_type)
    };

    let event = state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id.clone(),
                event_type: final_event_type,
                content: body,
                state_key: Some("".to_string()),
                origin_server_ts: now,
            },
            None,
        )
        .await
        .map_err(|e| ApiError::internal(format!("Failed to put state event: {}", e)))?;

    Ok(Json(json!({
        "event_id": new_event_id,
        "type": event.event_type,
        "state_key": event.state_key
    })))
}

// ============================================================================
// SECTION: Receipts & Read Markers
// ============================================================================

async fn send_receipt(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path((room_id, receipt_type, event_id)): Path<(String, String, String)>,
    Json(_body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;
    validate_receipt_type(&receipt_type)?;

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

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    let is_member = state
        .services
        .member_storage
        .is_member(&room_id, &auth_user.user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check membership: {}", e)))?;

    if !is_member {
        return Err(ApiError::forbidden(
            "You are not a member of this room".to_string(),
        ));
    }

    // Handle m.fully_read (public read marker)
    if let Some(event_id) = body.get("m.fully_read").and_then(|v| v.as_str()) {
        validate_event_id(event_id)?;
        state
            .services
            .room_storage
            .update_read_marker_with_type(&room_id, &auth_user.user_id, event_id, "m.fully_read")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to set fully_read marker: {}", e)))?;
    }

    // Handle m.private_read (private read marker - MSC2654)
    if let Some(event_id) = body.get("m.private_read").and_then(|v| v.as_str()) {
        validate_event_id(event_id)?;
        state
            .services
            .room_storage
            .update_read_marker_with_type(&room_id, &auth_user.user_id, event_id, "m.private_read")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to set private_read marker: {}", e)))?;
    }

    // Handle m.marked_unread (mark as unread - MSC2654)
    if let Some(marked_unread) = body.get("m.marked_unread").and_then(|v| v.as_object()) {
        if let Some(events) = marked_unread.get("events").and_then(|v| v.as_array()) {
            for event in events {
                if let Some(event_id) = event.as_str() {
                    validate_event_id(event_id)?;
                    state
                        .services
                        .room_storage
                        .update_read_marker_with_type(
                            &room_id,
                            &auth_user.user_id,
                            event_id,
                            "m.marked_unread",
                        )
                        .await
                        .map_err(|e| {
                            ApiError::internal(format!("Failed to set marked_unread marker: {}", e))
                        })?;
                }
            }
        }
    }

    // Handle legacy m.read (treated as m.fully_read)
    if let Some(event_id) = body.get("m.read").and_then(|v| v.as_str()) {
        validate_event_id(event_id)?;
        state
            .services
            .room_storage
            .update_read_marker_with_type(&room_id, &auth_user.user_id, event_id, "m.fully_read")
            .await
            .map_err(|e| ApiError::internal(format!("Failed to set read marker: {}", e)))?;
    }

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
    Path((room_id, event_id, _txn_id)): Path<(String, String, String)>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_room_id(&room_id)?;
    validate_event_id(&event_id)?;

    let original_event = state
        .services
        .event_storage
        .get_event(&event_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to get event: {}", e)))?
        .ok_or_else(|| ApiError::not_found("Event not found".to_string()))?;

    if original_event.room_id != room_id {
        return Err(ApiError::bad_request(
            "Event does not belong to this room".to_string(),
        ));
    }

    state
        .services
        .auth_service
        .can_redact_event(
            &room_id,
            &auth_user.user_id,
            &original_event.user_id,
            auth_user.is_admin,
        )
        .await?;

    let reason = body.get("reason").and_then(|v| v.as_str());

    let new_event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let now = chrono::Utc::now().timestamp_millis();

    let content = json!({
        "reason": reason
    });

    state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id: new_event_id.clone(),
                room_id: room_id.clone(),
                user_id: auth_user.user_id,
                event_type: "m.room.redaction".to_string(),
                content,
                state_key: None,
                origin_server_ts: now,
            },
            None,
        )
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

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .user_storage
        .user_exists(target)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .auth_service
        .can_kick_user(&room_id, &auth_user.user_id, target, auth_user.is_admin)
        .await?;

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
        .create_event(
            CreateEventParams {
                event_id,
                room_id: room_id.clone(),
                user_id: auth_user.user_id,
                event_type: "m.room.member".to_string(),
                content,
                state_key: Some(target.to_string()),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
            },
            None,
        )
        .await
        .map_err(|e| {
            ::tracing::warn!(
                "Failed to create membership event for room {}: {}",
                room_id,
                e
            );
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

    if !state
        .services
        .room_storage
        .room_exists(&room_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check room existence: {}", e)))?
    {
        return Err(ApiError::not_found("Room not found".to_string()));
    }

    if !state
        .services
        .user_storage
        .user_exists(target)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?
    {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .auth_service
        .can_ban_user(&room_id, &auth_user.user_id, target, auth_user.is_admin)
        .await?;

    let event_id = crate::common::crypto::generate_event_id(&state.services.server_name);
    let content = json!({
        "membership": "ban",
        "reason": reason.unwrap_or("")
    });

    state
        .services
        .member_storage
        .add_member(&room_id, target, "ban", None, None, None)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to ban user: {}", e)))?;

    state
        .services
        .event_storage
        .create_event(
            CreateEventParams {
                event_id,
                room_id: room_id.clone(),
                user_id: auth_user.user_id,
                event_type: "m.room.member".to_string(),
                content,
                state_key: Some(target.to_string()),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
            },
            None,
        )
        .await
        .map_err(|e| {
            ::tracing::warn!(
                "Failed to create membership event for room {}: {}",
                room_id,
                e
            );
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
        .auth_service
        .verify_room_moderator(&room_id, &auth_user.user_id, auth_user.is_admin)
        .await?;

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
        .create_event(
            CreateEventParams {
                event_id,
                room_id: room_id.clone(),
                user_id: auth_user.user_id,
                event_type: "m.room.member".to_string(),
                content,
                state_key: Some(target.to_string()),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
            },
            None,
        )
        .await
        .map_err(|e| {
            ::tracing::warn!(
                "Failed to create membership event for room {}: {}",
                room_id,
                e
            );
        })
        .ok();

    Ok(Json(json!({})))
}
