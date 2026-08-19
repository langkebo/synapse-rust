use super::route_ledger::{RouteEntry, RouteLedger};
use super::route_module::{route_modules, ProfileFlags};
use super::{
    account_data, background_update, captcha, delayed_events, device, dm, e2ee, ephemeral, event_report, feature_flags,
    guest, handlers, key_backup, key_rotation, media, moderation, presence, push, push_notification, reactions,
    relations, rendezvous, room_summary, sliding_sync, space, sync, tags, telemetry, thirdparty, typing,
    verification_routes, worker, *,
};
use crate::web::middleware::{
    cors_middleware, csrf_middleware, method_not_allowed_middleware, rate_limit_middleware, request_id_middleware,
    security_headers_middleware, shadow_ban_middleware,
};
use axum::{
    http::Method,
    routing::{get, post, put},
    Json, Router,
};
use serde_json::json;
use tower_http::compression::{predicate::SizeAbove, CompressionLayer};

/// Manifest of every `(method, absolute_path)` tuple the assembled top-level
/// [`Router`] is supposed to expose.
///
/// This is the substitute for the axum route-walker API we don't have — see
/// R4 / O2 in `docs/synapse-rust/SPEC_ALIGNMENT_PLAN_2026-05-01.md`. Today it
/// covers the inline routes defined directly in [`create_router`] plus the
/// static always-on router modules with explicit `*_route_manifest()` helpers.
/// State-aware modules such as `room`, `federation`, `oidc`, and selected
/// feature-gated routers are appended by [`declared_route_manifest_for`] so
/// the ledger reflects the actual router assembly path for the current
/// [`AppState`]. `create_router` validates this ledger at startup and aborts
/// on duplicates.
///
/// Adding a new always-registered router to the top-level assembly? Define
/// `fn my_route_manifest() -> Vec<RouteEntry>` in the router module and push
/// its output into `base_route_manifest` or wire it through `route_module`.
/// Leaving it out means the duplicate detector won't catch your router
/// clashing with an existing one.
fn base_route_manifest() -> RouteLedger {
    let mut ledger = RouteLedger::new();
    ledger.extend(top_level_inline_manifest());
    ledger.extend(assembly_compat_manifest());
    ledger.extend(key_backup::key_backup_route_manifest());
    ledger.extend(device::device_route_manifest());
    ledger.extend(e2ee::e2ee_route_manifest());
    ledger.extend(verification_routes::verification_route_manifest());
    ledger.extend(sync::sync_route_manifest());
    ledger.extend(account_data::account_data_route_manifest());
    ledger.extend(push::push_route_manifest());
    ledger.extend(tags::tags_route_manifest());
    ledger.extend(reactions::reactions_route_manifest());
    ledger.extend(relations::relations_route_manifest());
    ledger.extend(presence::presence_route_manifest());
    ledger.extend(typing::typing_route_manifest());
    ledger.extend(ephemeral::ephemeral_route_manifest());
    ledger.extend(sliding_sync::sliding_sync_route_manifest());
    ledger.extend(dm::dm_route_manifest());
    ledger.extend(key_rotation::key_rotation_route_manifest());
    ledger.extend(room_summary::room_summary_route_manifest());
    ledger.extend(feature_flags::feature_flags_route_manifest());
    ledger.extend(event_report::event_report_route_manifest());
    ledger.extend(space::space_route_manifest());
    ledger.extend(moderation::moderation_route_manifest());
    ledger.extend(delayed_events::delayed_events_route_manifest());
    ledger.extend(guest::guest_route_manifest());
    ledger.extend(captcha::captcha_route_manifest());
    ledger.extend(rendezvous::rendezvous_route_manifest());
    ledger.extend(msc4108_rendezvous::msc4108_route_manifest());
    ledger.extend(telemetry::telemetry_route_manifest());
    ledger.extend(thirdparty::thirdparty_route_manifest());
    ledger.extend(background_update::background_update_route_manifest());
    ledger.extend(push_notification::push_notification_route_manifest());
    ledger.extend(media::media_route_manifest());
    ledger.extend(worker::worker_route_manifest());
    ledger.extend(crate::web::routes::admin::admin_module_route_manifest());
    ledger.extend(module::module_route_manifest());
    ledger.extend(app_service::app_service_route_manifest());
    ledger.extend(crate::web::routes::handlers::thread::thread_route_manifest());
    ledger.extend(crate::web::routes::handlers::search::search_route_manifest());
    ledger.extend(vendor_route_manifest());
    ledger
}

pub fn declared_route_manifest_for(state: &AppState) -> RouteLedger {
    declared_route_manifest_for_profile(&ProfileFlags::from_state(state))
}

/// Pure-data flavour of [`declared_route_manifest_for`] for offline tools
/// (e.g. the `synapse_ledger_export` binary) that need the manifest without
/// constructing a live `AppState`. Composes `base_route_manifest()` with the
/// profile-driven entries each `RouteModule` emits via
/// `manifest_for_profile`. The live `create_router` path goes through
/// [`declared_route_manifest_for`] so this and live routing stay aligned by
/// construction.
pub fn declared_route_manifest_for_profile(flags: &ProfileFlags) -> RouteLedger {
    let mut ledger = base_route_manifest();
    for module in route_modules() {
        ledger.extend(module.manifest_for_profile(flags));
    }
    ledger
}

/// Manifest for routes declared inline inside [`create_router`] — i.e. those
/// registered with `.route(...)` directly on the top-level `Router` rather
/// than through a `create_*_router()` helper.
pub fn top_level_inline_manifest() -> Vec<RouteEntry> {
    const MODULE: &str = "assembly::create_router";
    [
        (Method::GET, "/"),
        (Method::GET, "/health"),
        (Method::GET, "/_health"),
        (Method::GET, "/_matrix/client/versions"),
        (Method::GET, "/_matrix/client/v3/versions"),
        (Method::GET, "/_matrix/server_version"),
        (Method::GET, "/_matrix/client/v1/config/client"),
        (Method::GET, "/_matrix/client/v3/pushrules/"),
        (Method::GET, "/_matrix/client/v3/pushrules/global/"),
        (Method::GET, "/_matrix/client/r0/pushrules/"),
        (Method::GET, "/_matrix/client/r0/pushrules/global/"),
        (Method::GET, "/.well-known/matrix/server"),
        (Method::GET, "/.well-known/matrix/client"),
        (Method::GET, "/.well-known/matrix/support"),
        (Method::GET, "/_matrix/client/unstable/org.matrix.msc2965/auth_metadata"),
        (Method::GET, "/_matrix/client/unstable/org.matrix.msc2965/auth_issuer"),
        (Method::GET, "/_matrix/client/v1/auth_metadata"),
        (Method::GET, "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device"),
        (Method::GET, "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/status"),
        (Method::PUT, "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device"),
        (Method::DELETE, "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device"),
        (Method::POST, "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/{device_id}/events"),
        (Method::GET, "/_matrix/client/unstable/org.matrix.msc4143/rtc/transports"),
        (Method::GET, "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}"),
        (Method::GET, "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}"),
        (Method::PUT, "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}"),
        (Method::DELETE, "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}"),
    ]
    .into_iter()
    .map(|(m, p)| RouteEntry::new(m, p, MODULE))
    .collect()
}

/// Manifest for the inline compat sub-routers built directly inside this file
/// (`create_client_capabilities_router`, `create_client_media_config_router`,
/// `create_voip_compat_router`, `create_auth_router`, `create_account_router`,
/// `create_directory_router`). They are part of the assembly file rather than
/// independent route modules, so their manifests live here.
///
/// Notes:
/// - `create_directory_router` also merges the `guest` router; that surface
///   is manifested in `guest::guest_route_manifest` and is *not* duplicated
///   here.
fn assembly_compat_manifest() -> Vec<RouteEntry> {
    use crate::web::routes::route_ledger::expand_under_prefixes;
    let mut out = Vec::new();

    // /capabilities — under r0 + v3
    out.extend(expand_under_prefixes(
        "assembly::capabilities",
        &["/_matrix/client/r0", "/_matrix/client/v3"],
        &[(Method::GET, "/capabilities")],
    ));

    // /media/config — under v1 + r0 + v3
    out.extend(expand_under_prefixes(
        "assembly::media_config",
        &["/_matrix/client/v1", "/_matrix/client/r0", "/_matrix/client/v3"],
        &[(Method::GET, "/media/config")],
    ));

    // Base VoIP compat surface — under r0 + v3
    out.extend(expand_under_prefixes(
        "assembly::voip_compat",
        &["/_matrix/client/r0", "/_matrix/client/v3"],
        &[
            (Method::GET, "/voip/turnServer"),
            (Method::POST, "/voip/turnServer"),
            (Method::GET, "/voip/config"),
            (Method::GET, "/voip/turnServer/guest"),
        ],
    ));
    #[cfg(feature = "voip-tracking")]
    out.extend(expand_under_prefixes(
        "assembly::voip_tracking",
        &["/_matrix/client/r0", "/_matrix/client/v3"],
        &[
            (Method::PUT, "/rooms/{room_id}/send/m.call.invite/{txn_id}"),
            (Method::PUT, "/rooms/{room_id}/send/m.call.candidates/{txn_id}"),
            (Method::PUT, "/rooms/{room_id}/send/m.call.answer/{txn_id}"),
            (Method::PUT, "/rooms/{room_id}/send/m.call.hangup/{txn_id}"),
            (Method::GET, "/rooms/{room_id}/call/{call_id}"),
        ],
    ));

    // Auth compat — under r0 + v3
    out.extend(expand_under_prefixes(
        "assembly::auth_compat",
        &["/_matrix/client/r0", "/_matrix/client/v3"],
        &[
            (Method::GET, "/register"),
            (Method::POST, "/register"),
            (Method::GET, "/register/available"),
            (Method::POST, "/register/email/requestToken"),
            (Method::POST, "/register/email/submitToken"),
            (Method::GET, "/login"),
            (Method::POST, "/login"),
            (Method::POST, "/logout"),
            (Method::POST, "/logout/all"),
            (Method::POST, "/refresh"),
        ],
    ));

    // Auth standalone routes (login fallback + MSC4108 QR token) — absolute paths
    out.extend(
        [
            (Method::GET, "/_matrix/static/client/login/"),
            // MSC4108: short-lived login token generation for QR sign-in
            (Method::POST, "/_matrix/client/v1/login/qr_token"),
        ]
        .into_iter()
        .map(|(m, p)| RouteEntry::new(m, p, "assembly::auth_router")),
    );

    // Account compat — under v1 + r0 + v3
    out.extend(expand_under_prefixes(
        "assembly::account_compat",
        &["/_matrix/client/v1", "/_matrix/client/r0", "/_matrix/client/v3"],
        &[
            (Method::GET, "/account/whoami"),
            (Method::POST, "/account/password"),
            (Method::POST, "/account/password/email/requestToken"),
            (Method::POST, "/account/password/email/submitToken"),
            (Method::POST, "/account/deactivate"),
            (Method::GET, "/account/3pid"),
            (Method::POST, "/account/3pid"),
            (Method::POST, "/account/3pid/add"),
            (Method::POST, "/account/3pid/bind"),
            (Method::POST, "/account/3pid/email/requestToken"),
            (Method::POST, "/account/3pid/email/submitToken"),
            (Method::POST, "/account/3pid/delete"),
            (Method::POST, "/account/3pid/unbind"),
            (Method::GET, "/profile/{user_id}"),
            (Method::GET, "/profile/{user_id}/displayname"),
            (Method::PUT, "/profile/{user_id}/displayname"),
            (Method::GET, "/profile/{user_id}/avatar_url"),
            (Method::PUT, "/profile/{user_id}/avatar_url"),
        ],
    ));

    // Account r0-only extras
    out.extend(expand_under_prefixes(
        "assembly::account_r0_only",
        &["/_matrix/client/r0"],
        &[
            (Method::GET, "/account/profile/{user_id}"),
            (Method::PUT, "/account/profile/{user_id}/displayname"),
            (Method::PUT, "/account/profile/{user_id}/avatar_url"),
        ],
    ));

    // Directory compat — under r0 + v3
    out.extend(expand_under_prefixes(
        "assembly::directory_compat",
        &["/_matrix/client/r0", "/_matrix/client/v3"],
        &[
            (Method::POST, "/user_directory/search"),
            (Method::POST, "/user_directory/list"),
            (Method::GET, "/user_directory/profiles/{user_id}"),
            (Method::GET, "/directory/list/room/{room_id}"),
            (Method::PUT, "/directory/list/room/{room_id}"),
            (Method::GET, "/directory/room/{room_alias}"),
            (Method::PUT, "/directory/room/{room_alias}"),
            (Method::DELETE, "/directory/room/{room_alias}"),
            (Method::GET, "/publicRooms"),
            (Method::POST, "/publicRooms"),
        ],
    ));

    // Directory r0-only extras
    out.extend(expand_under_prefixes(
        "assembly::directory_r0_only",
        &["/_matrix/client/r0"],
        &[
            (Method::GET, "/directory/room/{room_id}/alias"),
            (Method::PUT, "/directory/room/{room_id}/alias/{room_alias}"),
            (Method::DELETE, "/directory/room/{room_id}/alias/{room_alias}"),
        ],
    ));

    out
}

/// Manifest for ISSUE-13 vendor-prefixed private endpoints.
///
/// Private/non-standard endpoints (`/my_rooms`, `/search_rooms`,
/// `/search_recipients`) are migrated from `/_matrix/client/v3` to
/// `/_matrix/vendor/v1` so they no longer pollute the standard Matrix
/// client-server API namespace. The legacy `/_matrix/client/v3/{path}`
/// routes are kept for backward compatibility (see the deprecation warning
/// in [`create_router`]) but new clients should use the vendor prefix.
fn vendor_route_manifest() -> Vec<RouteEntry> {
    use crate::web::routes::route_ledger::expand_under_prefixes;
    expand_under_prefixes(
        "vendor",
        &["/_matrix/vendor/v1"],
        &[(Method::GET, "/my_rooms"), (Method::POST, "/search_rooms"), (Method::POST, "/search_recipients")],
    )
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
/// Maps the same handlers as the legacy `/_matrix/client/v3/{path}` routes
/// under the new `/_matrix/vendor/v1` prefix. The old routes remain
/// registered (via `create_sync_router` and `create_search_router`) for
/// backward compatibility — see the deprecation warning in [`create_router`].
fn create_vendor_router() -> Router<AppState> {
    Router::new()
        .route("/my_rooms", get(get_my_rooms))
        .route("/search_rooms", post(handlers::search::search::search_rooms))
        .route("/search_recipients", post(handlers::search::search::search_recipients))
}

pub fn create_router(state: AppState) -> Router {
    // Validate the declared route manifest before assembling the live router.
    // A duplicate (method, path) here is the exact class of bug that made
    // the key_backup routes dead for months in §1.1/§1.2 of the spec plan.
    let ledger = declared_route_manifest_for(&state);
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
            // Emit r0 deprecation warning unless suppressed by configuration.
            // The config system maps `SYNAPSE__SERVER__SUPPRESS_R0_DEPRECATION_WARNING`
            // to `server.suppress_r0_deprecation_warning`; we check the env var
            // here because AppState does not carry the resolved Config.
            let suppress_r0_warning = std::env::var("SYNAPSE__SERVER__SUPPRESS_R0_DEPRECATION_WARNING")
                .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
                .unwrap_or(false);
            if report.r0_route_count > 0 && !suppress_r0_warning {
                ::tracing::warn!(
                    target: "synapse_rust::web::routes::route_ledger",
                    r0_routes = report.r0_route_count,
                    "{} r0 route(s) are deprecated and scheduled for removal. \
                     Clients should migrate to /v3/ paths. Set \
                     `server.suppress_r0_deprecation_warning: true` to suppress.",
                    report.r0_route_count,
                );
            }
            // ISSUE-13: Private endpoints migrated to /_matrix/vendor/v1.
            // The legacy /_matrix/client/v3/{my_rooms,search_rooms,search_recipients}
            // routes remain for backward compatibility but are deprecated.
            // Suppress via SYNAPSE__SERVER__SUPPRESS_VENDOR_ENDPOINT_WARNING=true.
            let suppress_vendor_warning = std::env::var("SYNAPSE__SERVER__SUPPRESS_VENDOR_ENDPOINT_WARNING")
                .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
                .unwrap_or(false);
            if !suppress_vendor_warning {
                ::tracing::warn!(
                    target: "synapse_rust::web::routes::route_ledger",
                    "ISSUE-13: Private endpoints (/my_rooms, /search_rooms, /search_recipients) \
                     are now served under /_matrix/vendor/v1/. The legacy \
                     /_matrix/client/v3/ aliases are deprecated and will be removed \
                     in a future release. Clients should migrate to the vendor prefix. \
                     Set `server.suppress_vendor_endpoint_warning: true` to suppress.",
                );
            }
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
        .route("/_matrix/client/r0/pushrules/", get(get_push_rules_default))
        .route("/_matrix/client/r0/pushrules/global/", get(get_push_rules_global_default))
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
        .merge(crate::web::routes::handlers::search::create_search_router(state.clone()))
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
        .nest("/_matrix/client/r0", create_client_capabilities_router())
        .nest("/_matrix/client/v3", create_client_capabilities_router())
        .nest("/_matrix/client/v3", media::create_upload_provider_router())
        .nest("/_matrix/client/r0", create_voip_compat_router())
        .nest("/_matrix/client/v3", create_voip_compat_router())
        .nest("/_matrix/client/v1", create_client_media_config_router())
        .nest("/_matrix/client/r0", create_client_media_config_router())
        .nest("/_matrix/client/v3", create_client_media_config_router())
        // ISSUE-13: Private/non-standard endpoints under vendor prefix.
        .nest("/_matrix/vendor/v1", create_vendor_router())
        .merge(dm::create_dm_router(state.clone()))
        .merge(typing::create_typing_router(state.clone()))
        .merge(ephemeral::create_ephemeral_router(state.clone()))
        .merge(crate::web::routes::handlers::thread::create_thread_routes(state.clone()))
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

    let core_ctx = <crate::web::routes::context::CoreContext as axum::extract::FromRef<AppState>>::from_ref(&state);

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
        .merge(crate::web::api_doc::swagger_ui_router(state.clone()))
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
}

fn create_auth_router() -> Router<AppState> {
    Router::new()
        .nest("/_matrix/client/r0", create_auth_compat_router())
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

fn create_account_r0_only_router() -> Router<AppState> {
    Router::new()
        .route("/account/profile/{user_id}", get(get_profile))
        .route("/account/profile/{user_id}/displayname", put(update_displayname))
        .route("/account/profile/{user_id}/avatar_url", put(update_avatar))
}

fn create_account_router() -> Router<AppState> {
    Router::new()
        .nest("/_matrix/client/v1", create_account_compat_router())
        .nest("/_matrix/client/r0", create_account_compat_router().merge(create_account_r0_only_router()))
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

fn create_directory_r0_only_router() -> Router<AppState> {
    Router::new()
        .route("/directory/room/{room_id}/alias", get(get_room_aliases))
        .route("/directory/room/{room_id}/alias/{room_alias}", put(set_room_alias).delete(delete_room_alias))
}

fn create_directory_router(state: AppState) -> Router<AppState> {
    Router::new()
        .nest("/_matrix/client/r0", create_directory_compat_router().merge(create_directory_r0_only_router()))
        .nest("/_matrix/client/v3", create_directory_compat_router())
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
