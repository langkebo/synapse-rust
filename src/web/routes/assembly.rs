use super::*;
use crate::web::middleware::{
    cors_middleware, csrf_middleware, rate_limit_middleware, security_headers_middleware,
};
use axum::{
    routing::{get, post, put},
    Json, Router,
};
use serde_json::json;
use tower_http::compression::CompressionLayer;

async fn get_client_config() -> Result<Json<serde_json::Value>, ApiError> {
    Err(ApiError::unrecognized(
        "Client config endpoint is not supported".to_string(),
    ))
}

fn create_client_capabilities_router() -> Router<AppState> {
    Router::new().route("/capabilities", get(handlers::get_capabilities))
}

fn create_client_media_config_router() -> Router<AppState> {
    Router::new().route("/media/config", get(media::media_config))
}

fn create_voip_compat_router() -> Router<AppState> {
    Router::new()
        .route(
            "/voip/turnServer",
            get(get_turn_server).post(get_turn_server),
        )
        .route("/voip/config", get(get_voip_config))
        .route("/voip/turnServer/guest", get(get_turn_credentials_guest))
        .route(
            "/rooms/{room_id}/send/m.call.invite/{txn_id}",
            put(voip::call_invite),
        )
        .route(
            "/rooms/{room_id}/send/m.call.candidates/{txn_id}",
            put(voip::call_candidates),
        )
        .route(
            "/rooms/{room_id}/send/m.call.answer/{txn_id}",
            put(voip::call_answer),
        )
        .route(
            "/rooms/{room_id}/send/m.call.hangup/{txn_id}",
            put(voip::call_hangup),
        )
        .route(
            "/rooms/{room_id}/call/{call_id}",
            get(voip::get_call_session),
        )
}

pub fn create_router(state: AppState) -> Router {
    let mut router = Router::new()
        .without_v07_checks()
        .route(
            "/",
            get(|| async {
                Json(json!({
                    "msg": "Synapse Rust Matrix Server",
                    "version": env!("CARGO_PKG_VERSION")
                }))
            }),
        )
        .route("/health", get(handlers::health_check))
        .route(
            "/_matrix/client/versions",
            get(handlers::get_client_versions),
        )
        .route(
            "/_matrix/client/v3/versions",
            get(handlers::get_client_versions),
        )
        .route(
            "/_matrix/client/r0/version",
            get(handlers::get_server_version),
        )
        .route("/_matrix/server_version", get(handlers::get_server_version))
        .route("/_matrix/client/v1/config/client", get(get_client_config))
        .route("/_matrix/client/v3/pushrules/", get(get_push_rules_default))
        .route(
            "/_matrix/client/v3/pushrules/global/",
            get(get_push_rules_global_default),
        )
        .route(
            "/.well-known/matrix/server",
            get(handlers::get_well_known_server),
        )
        .route(
            "/.well-known/matrix/client",
            get(handlers::get_well_known_client),
        )
        .route(
            "/.well-known/matrix/support",
            get(handlers::get_well_known_support),
        )
        .merge(create_auth_router())
        .merge(create_account_router())
        .merge(create_account_data_router(state.clone()))
        .merge(create_directory_router(state.clone()))
        .merge(create_room_router())
        .merge(create_sync_router())
        .merge(create_presence_router())
        .merge(create_moderation_router())
        .merge(create_device_router())
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
        .merge(crate::web::routes::handlers::search::create_search_router(
            state.clone(),
        ))
        .merge(create_sliding_sync_router(state.clone()))
        .merge(create_space_router(state.clone()))
        .merge(create_app_service_router(state.clone()))
        .merge(create_worker_router(state.clone()))
        .merge(create_room_summary_router(state.clone()))
        .merge(create_event_report_router(state.clone()))
        .merge(create_feature_flags_router())
        .merge(create_background_update_router(state.clone()))
        .merge(create_module_router());

    // Optional authentication capabilities - only expose when enabled
    if state.services.saml_service.is_enabled() {
        router = router.merge(create_saml_router());
    }
    if state.services.oidc_service.is_some() {
        router = router.merge(create_oidc_router(state.clone()));
    }

    router
        .merge(cas_routes())
        .merge(create_captcha_router())
        .merge(create_push_notification_router())
        .merge(create_telemetry_router())
        .merge(create_thirdparty_router(state.clone()))
        .merge(create_tags_router(state.clone()))
        .nest("/_matrix/client/r0", create_client_capabilities_router())
        .nest("/_matrix/client/v3", create_client_capabilities_router())
        .nest("/_matrix/client/r0", create_voip_compat_router())
        .nest("/_matrix/client/v3", create_voip_compat_router())
        .nest("/_matrix/client/v1", create_client_media_config_router())
        .nest("/_matrix/client/r0", create_client_media_config_router())
        .nest("/_matrix/client/v3", create_client_media_config_router())
        .merge(dm::create_dm_router(state.clone()))
        .merge(typing::create_typing_router(state.clone()))
        .merge(ephemeral::create_ephemeral_router(state.clone()))
        .merge(create_external_service_router(state.clone()))
        .merge(create_burn_after_read_router(state.clone()))
        .merge(crate::web::routes::handlers::thread::create_thread_routes(
            state.clone(),
        ))
        .merge(create_widget_router())
        .merge(create_rendezvous_router(state.clone()))
        .merge(create_ai_connection_router())
        .route("/_matrix/client/v3/createRoom", post(create_room))
        .layer(axum::middleware::from_fn(cors_middleware))
        .layer(axum::middleware::from_fn(security_headers_middleware))
        .layer(CompressionLayer::new())
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            csrf_middleware,
        ))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .with_state(state)
}

fn create_auth_compat_router() -> Router<AppState> {
    Router::new()
        .route("/register", get(get_register_flows).post(register))
        .route("/register/available", get(check_username_availability))
        .route(
            "/register/email/requestToken",
            post(request_email_verification),
        )
        .route("/register/email/submitToken", post(submit_email_token))
        .route("/login", get(get_login_flows).post(login))
        .route("/logout", post(logout))
        .route("/logout/all", post(logout_all))
        .route("/refresh", post(refresh_token))
}

fn create_auth_router() -> Router<AppState> {
    Router::new()
        .nest("/_matrix/client/r0", create_auth_compat_router())
        .nest("/_matrix/client/v3", create_auth_compat_router())
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
}

fn create_account_compat_router() -> Router<AppState> {
    Router::new()
        .route("/account/whoami", get(whoami))
        .route("/account/password", post(change_password_uia))
        .route("/account/deactivate", post(deactivate_account))
        .route("/account/3pid", get(get_threepids).post(add_threepid))
        .route("/account/3pid/add", post(add_threepid))
        .route("/account/3pid/bind", post(add_threepid))
        .route("/account/3pid/delete", post(delete_threepid))
        .route("/account/3pid/unbind", post(unbind_threepid))
        .route("/profile/{user_id}", get(get_profile))
        .route(
            "/profile/{user_id}/displayname",
            get(get_displayname).put(update_displayname),
        )
        .route(
            "/profile/{user_id}/avatar_url",
            get(get_avatar_url).put(update_avatar),
        )
}

fn create_account_r0_only_router() -> Router<AppState> {
    Router::new()
        .route("/account/profile/{user_id}", get(get_profile))
        .route(
            "/account/profile/{user_id}/displayname",
            put(update_displayname),
        )
        .route("/account/profile/{user_id}/avatar_url", put(update_avatar))
}

fn create_account_router() -> Router<AppState> {
    Router::new()
        .nest("/_matrix/client/v1", create_account_compat_router())
        .nest(
            "/_matrix/client/r0",
            create_account_compat_router().merge(create_account_r0_only_router()),
        )
        .nest("/_matrix/client/v3", create_account_compat_router())
}

fn create_directory_compat_router() -> Router<AppState> {
    Router::new()
        .route("/user_directory/search", post(search_user_directory))
        .route("/user_directory/list", post(list_user_directory))
        .route(
            "/user_directory/profiles/{user_id}",
            get(get_user_directory_profile),
        )
        .route(
            "/directory/list/room/{room_id}",
            get(get_room_visibility).put(set_room_visibility),
        )
        .route(
            "/directory/room/{room_alias}",
            get(get_room_by_alias)
                .put(set_room_alias_direct)
                .delete(delete_room_alias_direct),
        )
        .route(
            "/publicRooms",
            get(get_public_rooms).post(query_public_rooms),
        )
}

fn create_directory_r0_only_router() -> Router<AppState> {
    Router::new()
        .route("/directory/room/{room_id}/alias", get(get_room_aliases))
        .route(
            "/directory/room/{room_id}/alias/{room_alias}",
            put(set_room_alias).delete(delete_room_alias),
        )
}

fn create_directory_router(state: AppState) -> Router<AppState> {
    Router::new()
        .nest(
            "/_matrix/client/r0",
            create_directory_compat_router().merge(create_directory_r0_only_router()),
        )
        .nest("/_matrix/client/v3", create_directory_compat_router())
        .merge(create_guest_router(state.clone()))
        .with_state(state)
}
