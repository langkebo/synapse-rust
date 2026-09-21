use super::route_ledger::RouteLedger;
use super::route_module::{route_modules, ProfileFlags};
use super::{dm, ephemeral, handlers, media, typing, worker, *};
use crate::middleware::{
    cors_middleware, csrf_middleware, method_not_allowed_middleware, rate_limit_middleware, request_id_middleware,
    security_headers_middleware, shadow_ban_middleware,
};
use axum::{
    routing::{get, post, put},
    Json, Router,
};
use serde_json::json;
use tower_http::compression::{predicate::SizeAbove, CompressionLayer};

/// Manifest of every `(method, absolute_path)` tuple the assembled top-level
/// [`axum::Router`] exposes.
///
/// This is the substitute for the axum route-walker API we don't have — see
/// R4 / O2 in `docs/synapse-rust/SPEC_ALIGNMENT_PLAN_2026-05-01.md`. It is
/// **derived**, not hand-written: `scripts/contract/extract_registered.py`
/// scrapes the real `.route(...)` surface and
/// `scripts/contract/gen_derived_routes.py` materialises it into
/// [`derived_routes`]. `create_router` validates this ledger at startup and
/// aborts on duplicates.
///
/// Adding a new router to the top-level assembly needs no manifest work at
/// all — register the route and regenerate the derived table:
///
/// ```text
/// python3 scripts/contract/gen_derived_routes.py
/// ```
///
/// Leaving that out makes `scripts/contract/check_route_contract.sh` fail, so
/// a router can no longer silently clash with an existing one.
pub fn declared_ledger_for(state: &AppState) -> RouteLedger {
    declared_ledger_for_profile(&ProfileFlags::from_state(state))
}

/// Pure-data flavour of [`declared_ledger_for`] for offline tools
/// (e.g. the `synapse_ledger_export` binary) that need the ledger without
/// constructing a live `AppState`. The live `create_router` path projects
/// `AppState` down to the same [`ProfileFlags`] and calls this function, so the
/// offline and online views cannot drift apart.
pub fn declared_ledger_for_profile(flags: &ProfileFlags) -> RouteLedger {
    let mut ledger = RouteLedger::new();
    ledger.extend(super::derived_routes::derived_route_manifest(flags));
    ledger
}

/// The whole `#[cfg]`-visible route surface, at the widest feature ceiling.
///
/// Capability gating and contract tests need *every* route the build can
/// serve, not one runtime profile's slice of it, so they lift the ceiling with
/// [`ProfileFlags::ALL`] instead of reading a per-module manifest.
pub fn declared_ledger_all() -> RouteLedger {
    declared_ledger_for_profile(&ProfileFlags::ALL)
}

// Handlers extracted to dedicated modules:
// - get_client_config       → handlers::client_config::get_client_config
// - dehydrated_device       → handlers::dehydrated_device::*
// - get_rtc_transports      → handlers::rtc_transports::get_rtc_transports
// - extended_profile        → handlers::extended_profile::*
// - auth_metadata/issuer    → handlers::auth_discovery::*

fn create_client_capabilities_router() -> Router<AppState> {
    Router::new().route("/capabilities", get(get_capabilities))
}

fn create_client_media_config_router() -> Router<AppState> {
    Router::new().route("/media/config", get(media::media_config))
}

fn create_voip_compat_router() -> Router<AppState> {
    // `mut` needed when `voip-tracking` feature is enabled; unused otherwise.
    #[allow(unused_mut)]
    let mut router = Router::new()
        .route("/voip/turnServer", get(get_turn_server).post(get_turn_server))
        .route("/voip/config", get(get_voip_config))
        .route("/voip/turnServer/guest", get(get_turn_credentials_guest));
    #[cfg(feature = "voip-tracking")]
    {
        router = router
            .route("/rooms/{room_id}/send/m.call.invite/{txn_id}", put(voip::call_invite))
            .route("/rooms/{room_id}/send/m.call.candidates/{txn_id}", put(voip::call_candidates))
            .route("/rooms/{room_id}/send/m.call.answer/{txn_id}", put(voip::call_answer))
            .route("/rooms/{room_id}/send/m.call.hangup/{txn_id}", put(voip::call_hangup))
            .route("/rooms/{room_id}/call/{call_id}", get(voip::get_call_session));
    }
    router
}

/// ISSUE-13: Vendor-prefixed router for private/non-standard endpoints.
///
/// Everything here is non-standard, so `/_matrix/vendor/v1` is its only home —
/// there is no `/_matrix/client/v3` twin. (`my_rooms`, `search_rooms` and
/// `search_recipients` are additionally reachable under their legacy `/v3`
/// paths via `create_sync_router` / `create_search_router`.)
fn create_vendor_router() -> Router<AppState> {
    Router::new()
        .route("/my_rooms", get(get_my_rooms))
        .route("/search_rooms", post(handlers::search::search::search_rooms))
        .route("/search_recipients", post(handlers::search::search::search_recipients))
        // MSC4380 room invite lists. Non-standard, so they live under the
        // vendor prefix rather than `/_matrix/client/v3`.
        .route(
            "/rooms/{room_id}/invite_blocklist",
            get(invite_blocklist::get_invite_blocklist).post(invite_blocklist::set_invite_blocklist),
        )
        .route(
            "/rooms/{room_id}/invite_allowlist",
            get(invite_blocklist::get_invite_allowlist).post(invite_blocklist::set_invite_allowlist),
        )
}

/// See [`create_router`].
pub fn create_router(state: AppState) -> Router {
    // Validate the declared route manifest before assembling the live router.
    // A duplicate (method, path) here is the exact class of bug that made
    // the key_backup routes dead for months in §1.1/§1.2 of the spec plan.
    let ledger = declared_ledger_for(&state);
    match ledger.validate() {
        Ok(report) => {
            let registered_by_counts = ledger.registered_by_counts();
            let registered_by_summary = registered_by_counts
                .iter()
                .map(|count| format!("{}={}", count.registered_by, count.entries))
                .collect::<Vec<_>>()
                .join(", ");
            ::tracing::info!(
                target: "synapse_rust::routes",
                unique_tuples = report.unique_tuples,
                total_entries = report.total_entries,
                registered_by_namespaces = registered_by_counts.len(),
                registered_by_summary = %registered_by_summary,
                "route manifest validated: {} declared (method, path) tuples, 0 duplicates",
                report.unique_tuples,
            );
            // NOTE (B1-3): deliberately **no** deprecation warning here.
            //
            // ISSUE-13's legacy `/v3` aliases are annotated at each declaration
            // site (`sync.rs` for `/my_rooms`, `handlers/search/mod.rs` for
            // `/search_rooms` + `/search_recipients`), which is where a
            // maintainer actually reads. A WARN emitted on every boot about
            // state the operator cannot change is noise, and it previously
            // dragged in a bespoke `suppress_vendor_endpoint_warning` config
            // field — never read by any code path — plus a compose env
            // passthrough, purely to silence it.
        }
        Err(err) => {
            tracing::error!("route manifest contains duplicate entries — refusing to start:\n{err}");
            std::process::exit(1);
        }
    }

    // B-4: Auto-derive the rate limit exemption list from route metadata.
    // Routes marked `rate_limit_exempt = true` in their manifest (sync and
    // sliding-sync endpoints) are collected here so the rate limit middleware
    // can skip them without hardcoding paths.
    let rate_limit_exempt_paths: Vec<&'static str> =
        ledger.iter().filter(|e| e.rate_limit_exempt).map(|e| e.path).collect();
    tracing::info!(
        target: "synapse_rust::routes",
        count = rate_limit_exempt_paths.len(),
        paths = ?rate_limit_exempt_paths,
        "auto-derived rate limit exempt paths from route ledger"
    );
    let state = state.with_rate_limit_exempt_paths(rate_limit_exempt_paths);

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
        .route("/_health", get(handlers::detailed_health_check))
        .route("/_matrix/client/versions", get(handlers::get_client_versions))
        .route("/_matrix/client/v3/versions", get(handlers::get_client_versions))
        .route("/_matrix/server_version", get(handlers::get_server_version))
        .route("/_matrix/client/v1/config/client", get(handlers::client_config::get_client_config))
        .route("/_matrix/client/v3/pushrules/", get(get_push_rules_default))
        .route("/_matrix/client/v3/pushrules/global/", get(get_push_rules_global_default))
        .route("/.well-known/matrix/server", get(handlers::get_well_known_server))
        .route("/.well-known/matrix/client", get(handlers::get_well_known_client))
        .route("/.well-known/matrix/support", get(handlers::get_well_known_support))
        .route(
            "/_matrix/client/unstable/org.matrix.msc2965/auth_metadata",
            get(handlers::auth_discovery::get_auth_metadata),
        )
        .route(
            "/_matrix/client/unstable/org.matrix.msc2965/auth_issuer",
            get(handlers::auth_discovery::get_auth_issuer),
        )
        // MSC2965 stable path: same handler as unstable path above.
        .route(
            "/_matrix/client/v1/auth_metadata",
            get(handlers::auth_discovery::get_auth_metadata),
        )
        .route(
            "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device",
            get(handlers::dehydrated_device::get_dehydrated_device)
                .put(handlers::dehydrated_device::put_dehydrated_device)
                .delete(handlers::dehydrated_device::delete_dehydrated_device),
        )
        .route(
            "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/status",
            get(handlers::dehydrated_device::get_dehydrated_device_status),
        )
        .route(
            "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/{device_id}/events",
            post(handlers::dehydrated_device::post_dehydrated_device_events),
        )
        .route(
            "/_matrix/client/unstable/org.matrix.msc4143/rtc/transports",
            get(handlers::rtc_transports::get_rtc_transports),
        )
        .route(
            "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}",
            get(handlers::extended_profile::get_extended_profile),
        )
        .route(
            "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}",
            get(handlers::extended_profile::get_extended_profile_field)
                .put(handlers::extended_profile::put_extended_profile_field)
                .delete(handlers::extended_profile::delete_extended_profile_field),
        )
        .merge(create_auth_router())
        .merge(create_account_router())
        .merge(create_account_data_router(state.clone()))
        .merge(create_directory_router(state.clone()))
        .merge(create_sync_router(state.clone()))
        .merge(create_moderation_router())
        // MSC4140 — Cancellable delayed events (unstable namespace).
        .nest(
            "/_matrix/client/unstable/org.matrix.msc4140",
            create_delayed_events_router(),
        )
        .merge(create_device_router())
        .merge(create_media_router(&state))
        .merge(create_e2ee_router(state.clone()))
        .merge(create_key_backup_router(state.clone()))
        .merge(create_key_rotation_router(state.clone()))
        .merge(create_verification_router(state.clone()))
        .merge(create_relations_router(state.clone()))
        .merge(create_reactions_router(state.clone()))
        .merge(create_admin_module_router(state.clone()))
        .merge(create_push_router(state.clone()))
        .merge(crate::routes::handlers::search::create_search_router(state.clone()))
        .merge(create_sliding_sync_router(state.clone()))
        .merge(create_space_router(state.clone()))
        .merge(create_app_service_router(&state))
        .merge(create_room_summary_router(state.clone()))
        .merge(create_event_report_router(state.clone()))
        .merge(create_feature_flags_router(state.clone()))
        .merge(create_background_update_router(state.clone()))
        .merge(create_module_router(state.clone()));

    router = router.merge(worker::create_worker_admin_router(&state));

    // Optional authentication capabilities - only expose when enabled
    for module in route_modules() {
        router = module.merge_into(router, state.clone());
    }
    router = router
        .merge(create_captcha_router(&state))
        .merge(create_push_notification_router(state.clone()))
        .merge(create_telemetry_router(state.clone()))
        .merge(create_thirdparty_router(state.clone()))
        .merge(create_tags_router(state.clone()))
        .nest("/_matrix/client/v3", create_client_capabilities_router())
        .nest("/_matrix/client/v3", media::create_upload_provider_router())
        .nest("/_matrix/client/v3", create_voip_compat_router())
        .nest("/_matrix/client/v1", create_client_media_config_router())
        .nest("/_matrix/client/v3", create_client_media_config_router())
        // ISSUE-13: Private/non-standard endpoints under vendor prefix.
        .nest("/_matrix/vendor/v1", create_vendor_router())
        .merge(dm::create_dm_router(state.clone()))
        .merge(typing::create_typing_router(state.clone()))
        .merge(ephemeral::create_ephemeral_router(state.clone()))
        .merge(crate::routes::handlers::thread::create_thread_routes(state.clone()))
        .merge(create_rendezvous_router(state.clone()))
        .merge(create_msc4108_rendezvous_router(state.clone()))
        .merge(create_presence_router());

    // Fallback handler: unmatched routes return M_UNRECOGNIZED per Matrix spec.
    // Without this, axum returns an empty-body 404 which breaks client error handling.
    // Uses pre-serialized Bytes to avoid per-request allocation from serde_json::json!().
    const FALLBACK_BODY: &[u8] = b"{\"errcode\":\"M_UNRECOGNIZED\",\"error\":\"Unrecognized request\"}";
    router = router.fallback(|| async {
        (
            axum::http::StatusCode::NOT_FOUND,
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            axum::body::Bytes::from_static(FALLBACK_BODY),
        )
    });

    let core_ctx = <crate::routes::context::CoreContext as axum::extract::FromRef<AppState>>::from_ref(&state);

    // WEB-04: 中间件顺序修正。axum 的 .layer() 先注册者在**内层**（后执行），
    // 因此下方调用顺序即「请求到达的逆序」。修正后的请求流向（外→内）：
    //   request_id → cors → security_headers → method_not_allowed → compression
    //   → shadow_ban → csrf → rate_limit → routes
    // 修正点：
    // 1. CORS 从最内层移到近最外层——此前 rate_limit/csrf 短路返回的 429/403
    //    不带 CORS 头，浏览器客户端连错误都读不到；预检 OPTIONS 也会先撞
    //    CSRF/限流。现在所有错误响应都会经过 CORS 后处理。
    // 2. csrf 移到 rate_limit 之前——此前 CSRF 必失败的请求也消耗限流配额，
    //    且响应语义错误（应 403 而非 429）。
    router
        .layer(axum::middleware::from_fn_with_state(core_ctx.clone(), rate_limit_middleware))
        .layer(axum::middleware::from_fn_with_state(core_ctx.clone(), csrf_middleware))
        .layer(axum::middleware::from_fn_with_state(core_ctx, shadow_ban_middleware))
        .layer(CompressionLayer::new().compress_when(SizeAbove::new(1024)))
        .layer(axum::middleware::from_fn(method_not_allowed_middleware))
        .layer(axum::middleware::from_fn(security_headers_middleware))
        .layer(axum::middleware::from_fn(cors_middleware))
        .layer(axum::middleware::from_fn(request_id_middleware))
        .with_state(state)
}

fn create_auth_compat_router() -> Router<AppState> {
    Router::new()
        .route("/register", get(get_register_flows).post(register))
        .route("/register/available", get(check_username_availability))
        .route("/register/email/requestToken", post(request_email_verification))
        .route("/register/email/submitToken", post(submit_email_token))
        .route("/login", get(get_login_flows).post(login))
        .route("/logout", post(logout))
        .route("/logout/all", post(logout_all))
        .route("/refresh", post(refresh_token))
        // Matrix spec: GET /_matrix/client/v3/auth/{authType}/fallback/web
        // returns an HTML page for clients that cannot handle a given
        // auth stage natively (Client-Server API §3.3.4).
        //
        // The capture group MUST use axum 0.8's `{param}` syntax: `:param` makes
        // `Router::route` panic at assembly time with "Path segments must not
        // start with `:`", which takes the whole server down at startup.
        // Guarded by `scripts/ci/check_axum_path_syntax.py`.
        .route("/auth/{auth_type}/fallback/web", get(auth_fallback_web))
}

fn create_auth_router() -> Router<AppState> {
    Router::new()
        .nest("/_matrix/client/v3", create_auth_compat_router())
        .route(
            "/_matrix/static/client/login/",
            get(auth_compat::login_fallback_page),
        )
        // MSC4108: existing device generates a short-lived login token that the
        // new device exchanges via m.login.token over the secure rendezvous channel.
        .route(
            "/_matrix/client/v1/login/qr_token",
            post(auth_compat::generate_qr_login_token),
        )
}

fn create_account_compat_router() -> Router<AppState> {
    Router::new()
        .route("/account/whoami", get(whoami))
        .route("/account/password", post(change_password_uia))
        .route("/account/password/email/requestToken", post(request_password_email_verification))
        .route("/account/password/email/submitToken", post(submit_email_token))
        .route("/account/deactivate", post(deactivate_account))
        .route("/account/3pid", get(get_threepids).post(add_threepid))
        .route("/account/3pid/add", post(add_threepid))
        .route("/account/3pid/bind", post(add_threepid))
        .route("/account/3pid/email/requestToken", post(request_3pid_add_email_verification))
        .route("/account/3pid/email/submitToken", post(submit_email_token))
        .route("/account/3pid/delete", post(delete_threepid))
        .route("/account/3pid/unbind", post(unbind_threepid))
        .route("/profile/{user_id}", get(get_profile))
        .route("/profile/{user_id}/displayname", get(get_displayname).put(update_displayname))
        .route("/profile/{user_id}/avatar_url", get(get_avatar_url).put(update_avatar))
}

fn create_account_router() -> Router<AppState> {
    Router::new()
        .nest("/_matrix/client/v1", create_account_compat_router())
        .nest("/_matrix/client/v3", create_account_compat_router())
}

fn create_directory_compat_router() -> Router<AppState> {
    Router::new()
        .route("/user_directory/search", post(search_user_directory))
        .route("/user_directory/list", post(list_user_directory))
        .route("/user_directory/profiles/{user_id}", get(get_user_directory_profile))
        .route("/directory/list/room/{room_id}", get(get_room_visibility).put(set_room_visibility))
        .route(
            "/directory/room/{room_alias}",
            get(get_room_by_alias).put(set_room_alias_direct).delete(delete_room_alias_direct),
        )
        .route("/publicRooms", get(get_public_rooms).post(query_public_rooms))
}

/// Extra directory surface (room-scoped alias management) — v3 only.
fn create_directory_v3_extra_router() -> Router<AppState> {
    Router::new()
        .route("/directory/room/{room_id}/alias", get(get_room_aliases))
        .route("/directory/room/{room_id}/alias/{room_alias}", put(set_room_alias).delete(delete_room_alias))
}

fn create_directory_router(state: AppState) -> Router<AppState> {
    Router::new()
        .nest("/_matrix/client/v3", create_directory_compat_router().merge(create_directory_v3_extra_router()))
        .merge(create_guest_router(state.clone()))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    /// WEB-04: 中间件注册顺序守卫（源码扫描）。
    /// axum 的 .layer() 先注册者在**内层**，因此源文件中的出现顺序必须保持：
    /// rate_limit 先于 csrf 注册（csrf 更外层，先执行）、
    /// cors 晚于 security_headers 注册（cors 更外层，错误响应也带 CORS 头）、
    /// request_id 最后注册（最外层）。
    #[test]
    fn web04_middleware_layer_order_guard() {
        let source = include_str!("assembly.rs");
        let idx = |needle: &str| source.find(needle).unwrap_or_else(|| panic!("{needle} not found in assembly.rs"));

        let rate_limit = idx("rate_limit_middleware))");
        let csrf = idx("csrf_middleware))");
        let shadow_ban = idx("shadow_ban_middleware))");
        let cors = idx("from_fn(cors_middleware)");
        let security_headers = idx("from_fn(security_headers_middleware)");
        let request_id = idx("from_fn(request_id_middleware)");

        assert!(rate_limit < csrf, "rate_limit 必须注册在 csrf 内层（源文件中先出现）");
        assert!(csrf < shadow_ban, "csrf 必须注册在 shadow_ban 内层");
        assert!(cors > security_headers, "cors 必须比 security_headers 更外层（源文件中后出现）");
        assert!(request_id > cors, "request_id 必须注册在最外层");
    }
}
