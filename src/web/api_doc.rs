//! OpenAPI / Swagger documentation for the Synapse-Rust Matrix homeserver.
//!
//! Enabled via the `openapi-docs` feature flag. When enabled, the Swagger UI
//! is served at `/_swagger` and the OpenAPI JSON schema at `/_api-doc/openapi.json`.
//!
//! Route annotation is progressive — health, versions, capabilities, and
//! well-known endpoints are annotated as canonical examples. Additional routes
//! should be annotated incrementally through follow-up patches.

use crate::web::routes::AppState;

#[cfg(feature = "openapi-docs")]
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct ApiHealthCheckResult {
    status: String,
    message: String,
    duration_ms: u64,
}

#[cfg(feature = "openapi-docs")]
#[derive(utoipa::ToSchema)]
#[allow(dead_code)]
pub struct ApiHealthStatus {
    status: String,
    version: String,
    timestamp: i64,
    checks: std::collections::HashMap<String, ApiHealthCheckResult>,
}

/// Build the Swagger UI router for the given OpenAPI schema.
///
/// The UI is mounted at `/_swagger` with a redirect from `/_swagger/` for
/// convenience. The raw OpenAPI JSON is served at `/_api-doc/openapi.json`.
#[cfg(feature = "openapi-docs")]
pub fn swagger_ui_router(_state: AppState) -> axum::Router<AppState> {
    use utoipa::OpenApi;
    use utoipa_swagger_ui::SwaggerUi;

    #[derive(OpenApi)]
    #[openapi(
        info(
            title = "Synapse-Rust Matrix Homeserver API",
            version = env!("CARGO_PKG_VERSION"),
            description = "Matrix Client-Server API implementation in Rust. \
                Compliant with Matrix Spec v1.13."
        ),
        servers(
            (url = "/", description = "Local Synapse-Rust instance"),
        ),
        tags(
            (name = "Health", description = "Server health and version endpoints"),
            (name = "Authentication", description = "Login, registration, and token management"),
            (name = "Client-Server", description = "Matrix Client-Server API (v3)"),
            (name = "Admin", description = "Server administration endpoints"),
            (name = "Federation", description = "Server-to-server federation API"),
        ),
        paths(
            health_check,
            detailed_health_check,
            get_client_versions,
            get_server_version,
            get_capabilities,
            get_well_known_server,
            get_well_known_client,
            get_well_known_support,
            list_account_data,
            get_account_data,
            set_account_data_doc,
            delete_account_data_doc,
            get_room_account_data,
            set_room_account_data_doc,
            delete_room_account_data_doc,
            create_filter_doc,
            get_filter,
            delete_filter_doc,
            get_openid_token_doc,
            get_pushers,
            get_push_rules,
            get_push_rules_scope,
            get_push_rules_kind,
            get_push_rule,
            get_devices,
            get_device,
            update_device_doc,
            delete_device_doc,
            delete_devices_doc,
            get_global_tags,
            get_room_tags,
            put_room_tag_doc,
            delete_room_tag_doc,
            get_profile_info,
            get_profile_displayname,
            get_profile_avatar_url,
            update_avatar_url_doc,
            get_presence_status,
            get_presence_list_current_user,
            get_presence_list_for_user,
            get_whoami,
            get_threepids,
            get_dehydrated_device_status_doc,
            get_rtc_transports_doc,
            get_qr_status,
            get_media_config,
            list_users_admin_doc,
            list_rooms_admin_doc,
            admin_delete_user_doc,
            admin_evict_user_doc,
            admin_set_user_admin_doc,
            admin_deactivate_user_doc,
            admin_reset_user_password_doc,
            admin_user_v2_doc,
            admin_upsert_user_v2_doc,
            admin_user_rooms_doc,
            admin_user_devices_doc,
            admin_delete_user_device_doc,
            admin_login_as_user_doc,
            admin_logout_user_devices_doc,
            admin_user_stats_doc,
            admin_single_user_stats_doc,
            admin_batch_create_users_doc,
            admin_batch_deactivate_users_doc,
            admin_user_sessions_doc,
            admin_invalidate_user_sessions_doc,
            admin_account_details_doc,
            admin_update_account_doc,
            admin_room_doc,
            admin_room_members_doc,
            admin_room_state_doc,
            admin_spaces_doc,
            admin_space_doc,
            admin_delete_space_doc,
            admin_space_users_doc,
            admin_space_rooms_doc,
            admin_space_stats_doc,
            admin_room_stats_doc,
            admin_single_room_stats_doc,
            admin_room_listings_doc,
            admin_set_room_public_doc,
            admin_set_room_private_doc,
            admin_room_block_status_doc,
            admin_block_room_doc,
            admin_unblock_room_doc,
            admin_make_room_admin_doc,
            admin_purge_history_doc,
            admin_purge_room_doc,
            admin_join_room_member_doc,
            admin_remove_room_member_doc,
            admin_cleanup_abnormal_rooms_doc,
            admin_server_version_doc,
            admin_info_doc,
            admin_whoami_doc,
            admin_statistics_doc,
            admin_status_doc,
            admin_whois_doc,
            admin_whois_device_doc,
            admin_purge_media_cache_doc,
            admin_health_doc,
            admin_config_doc,
            admin_jitsi_config_doc,
            admin_invite_blocklist_doc,
            admin_invite_allowlist_doc,
            admin_federation_destinations_doc,
            admin_federation_destination_doc,
            admin_federation_destination_rooms_doc,
            admin_reports_doc,
            admin_report_doc,
            admin_retention_policy_doc,
            admin_set_retention_policy_doc,
            admin_room_retention_policy_doc,
            admin_retention_status_doc,
            admin_registration_tokens_doc,
            admin_create_registration_token_doc,
            admin_registration_token_doc,
            admin_delete_registration_token_doc,
            admin_update_registration_token_doc,
            admin_user_tokens_doc,
            admin_delete_user_token_doc,
            admin_user_refresh_tokens_doc,
            admin_delete_refresh_token_doc,
            admin_media_list_doc,
            admin_media_info_doc,
            admin_delete_media_doc,
            admin_media_quota_doc,
            admin_user_media_doc,
            admin_delete_user_media_doc,
            admin_shadow_ban_user_doc,
            admin_unshadow_ban_user_doc,
            admin_user_rate_limit_doc,
            admin_set_user_rate_limit_doc,
            admin_delete_user_rate_limit_doc,
            admin_override_rate_limit_doc,
            admin_set_override_rate_limit_doc,
            admin_delete_override_rate_limit_doc,
            get_joined_rooms_doc,
            get_public_rooms_doc,
            register_doc,
            login_doc,
            logout_doc,
            refresh_token_doc,
            create_room_doc,
            send_message_doc,
            join_room_doc,
            leave_room_doc,
            forget_room_doc,
            invite_user_doc,
            get_joined_members_doc,
            update_displayname_doc,
            get_register_flows_doc,
            check_username_availability_doc,
            get_login_flows_doc,
            logout_all_doc,
            change_password_doc,
            deactivate_account_doc,
            add_threepid_doc,
            delete_threepid_doc,
            unbind_threepid_doc,
            get_user_directory_profile_doc,
            search_user_directory_doc,
            get_room_visibility_doc,
            set_room_visibility_doc,
            get_room_by_alias_doc,
            set_room_alias_direct_doc,
            delete_room_alias_direct_doc,
            query_public_rooms_doc,
            get_room_aliases_doc,
            set_room_alias_doc,
            delete_room_alias_doc,
            sync_doc,
            get_events_doc,
            get_my_rooms_doc,
            search_room_events_doc,
            search_recipients_doc,
            search_rooms_doc,
            get_event_context_doc,
            get_room_hierarchy_doc,
            timestamp_to_event_doc,
            upload_media_v3_doc,
            download_media_doc,
            get_thumbnail_doc,
            preview_url_doc,
            report_room_doc,
            report_event_doc,
            update_report_score_doc,
            get_relations_by_event_doc,
            get_relations_doc,
            send_relation_doc,
            get_aggregations_doc,
            add_reaction_doc,
            get_client_config_doc,
            register_guest_doc,
            get_guest_info_doc,
            upgrade_guest_doc,
            get_qr_code_doc,
            confirm_qr_login_doc,
            start_qr_login_doc,
            get_qr_status_doc,
            invalidate_qr_login_doc,
            get_qrcode_new_doc,
            get_qrcode_status_alias_doc,
            get_thirdparty_protocols_doc,
            get_thirdparty_protocol_doc,
            get_thirdparty_location_doc,
            get_thirdparty_user_doc,
            get_thirdparty_location_by_alias_doc,
            get_thirdparty_user_by_id_doc,
            set_push_rule_actions_doc,
            get_push_rule_enabled_doc,
            set_push_rule_enabled_doc,
            get_push_rules_default_doc,
            get_push_rules_global_default_doc,
            get_push_rules_default_r0_doc,
            get_push_rules_global_default_r0_doc,
            get_presence_status_v1_doc,
            set_presence_status_v1_doc,
            get_presence_status_r0_doc,
            set_presence_status_r0_doc,
            get_typing_users_doc,
            get_user_typing_doc,
            set_typing_doc,
            bulk_get_typing_doc,
            get_typing_users_r0_doc,
            get_user_typing_r0_doc,
            set_typing_r0_doc,
            bulk_get_typing_r0_doc,
            create_rendezvous_session_doc,
            get_rendezvous_session_doc,
            update_rendezvous_session_doc,
            delete_rendezvous_session_doc,
            send_rendezvous_message_doc,
            get_rendezvous_messages_doc,
            get_push_devices_doc,
            register_push_device_doc,
            unregister_push_device_doc,
            send_push_notification_doc,
            get_push_notification_rules_doc,
            create_push_notification_rule_doc,
            delete_push_notification_rule_doc,
            send_captcha_doc,
            verify_captcha_doc,
            get_captcha_status_doc,
            send_captcha_r0_doc,
            verify_captcha_r0_doc,
            get_captcha_status_r0_doc,
            cleanup_captcha_doc,
            get_thirdparty_location_r0_doc,
            get_thirdparty_user_r0_doc,
            login_sso_redirect_v3_doc,
            login_sso_redirect_r0_doc,
            login_sso_userinfo_v3_doc,
            login_sso_userinfo_r0_doc,
            login_sso_redirect_cas_v3_doc,
            login_sso_redirect_cas_r0_doc,
            login_sso_redirect_saml_v3_doc,
            login_sso_redirect_saml_r0_get_doc,
            login_sso_redirect_saml_r0_post_doc,
            login_saml_callback_v3_get_doc,
            login_saml_callback_v3_post_doc,
            login_saml_callback_r0_get_doc,
            login_saml_callback_r0_post_doc,
            logout_saml_r0_doc,
            logout_saml_callback_r0_doc,
            oidc_userinfo_v3_doc,
            oidc_token_v3_doc,
            oidc_logout_v3_doc,
            oidc_authorize_v3_doc,
            oidc_register_v3_doc,
            oidc_callback_v3_doc,
            oidc_userinfo_r0_doc,
            oidc_token_r0_doc,
            oidc_logout_r0_doc,
            oidc_authorize_r0_doc,
            oidc_register_r0_doc,
            oidc_callback_r0_doc,
            oidc_login_v3_doc,
            appservice_ping_doc,
            appservice_transactions_doc,
            appservice_user_query_doc,
            appservice_room_alias_query_doc,
            appservice_query_doc,
            key_rotation_status_get_doc,
            key_rotation_status_post_doc,
            key_rotation_rotate_doc,
            key_rotation_history_doc,
            key_rotation_revoke_doc,
            key_rotation_config_put_doc,
            key_rotation_config_post_doc,
            key_rotation_check_get_doc,
            key_rotation_check_post_doc,
            get_saml_metadata_r0_doc,
            get_sp_metadata_r0_doc,
            get_saml_metadata_v3_doc,
            get_sp_metadata_v3_doc,
            get_auth_metadata_doc,
            get_dehydrated_device_doc,
            put_dehydrated_device_doc,
            delete_dehydrated_device_doc,
            post_dehydrated_device_events_doc,
            get_client_versions_v3_doc,
            get_server_version_r0_doc,
            sliding_sync_v1_doc,
            login_fallback_page_doc,
            get_ephemeral_events_doc,
            redact_thread_reply_doc,
            get_federation_version_doc,
            get_federation_discovery_doc,
            get_public_rooms_federation_doc,
            query_destination_doc,
            openid_userinfo_doc,
            get_room_members_doc,
            get_joined_room_members_doc,
            get_user_devices_doc,
            get_room_auth_doc,
            knock_room_doc,
            thirdparty_invite_doc,
            get_joining_rules_doc,
            invite_v2_doc,
            send_transaction_doc,
            make_join_doc,
            make_leave_doc,
            send_join_doc,
            send_leave_doc,
            invite_doc,
            get_missing_events_federation_doc,
            get_room_event_doc,
            timestamp_to_event_federation_doc,
            get_event_auth_doc,
            query_auth_doc,
            event_auth_doc,
            get_state_federation_doc,
            get_event_federation_doc,
            get_state_ids_federation_doc,
            room_directory_query_federation_doc,
            profile_query_federation_doc,
            profile_query_legacy_federation_doc,
            get_room_hierarchy_federation_doc,
            backfill_federation_doc,
            legacy_keys_claim_federation_doc,
            legacy_keys_query_federation_doc,
            keys_upload_federation_doc,
            keys_claim_federation_doc,
            keys_query_federation_doc,
            send_join_v2_federation_doc,
            send_leave_v2_federation_doc,
            post_public_rooms_federation_doc,
            query_directory_federation_doc,
            media_download_federation_doc,
            media_thumbnail_federation_doc,
            exchange_third_party_invite_federation_doc,
            server_key_federation_doc,
            key_clone_federation_doc,
            key_query_federation_doc,
            user_keys_upload_federation_doc,
            v2_keys_query_federation_doc,
            server_key_v2_federation_doc,
            key_query_v2_federation_doc,
            sliding_sync_msc3575_doc,
            sliding_sync_simplified_msc3575_doc,
            get_extended_profile_msc4133_doc,
            get_extended_profile_field_msc4133_doc,
            put_extended_profile_field_msc4133_doc,
            delete_extended_profile_field_msc4133_doc,
            create_dm_doc,
            get_direct_map_doc,
            update_direct_map_doc,
            get_voice_config_doc,
            upload_voice_message_doc,
            get_voice_stats_doc,
            get_room_voice_stats_doc,
            get_user_voice_stats_doc,
            get_room_voice_messages_doc,
            get_user_voice_messages_doc,
            get_voice_message_content_doc,
            convert_voice_message_doc,
            optimize_voice_message_doc,
            transcribe_voice_message_doc,
            create_widget_doc,
            get_widget_doc,
            update_widget_doc,
            delete_widget_doc,
            get_widget_config_doc,
            get_room_widgets_doc,
            get_jitsi_config_doc,
            get_room_widget_capabilities_doc,
            set_room_widget_capabilities_doc,
            send_room_widget_message_doc,
            set_widget_permission_doc,
            get_widget_permissions_doc,
            delete_widget_permission_doc,
            create_widget_session_doc,
            get_widget_sessions_doc,
            get_widget_session_doc,
            terminate_widget_session_doc,
            register_external_service_doc,
            list_external_services_doc,
            get_external_service_health_doc,
            check_service_health_doc,
            unregister_external_service_doc,
            update_external_service_doc,
            get_external_service_doc,
            enable_burn_doc,
            get_burn_settings_doc,
            get_pending_burns_doc,
            mark_burn_read_doc,
            cancel_burn_doc,
            set_global_burn_config_doc,
            get_burn_stats_doc,
            get_friends_doc,
            send_friend_request_doc,
            search_friend_directory_doc,
            get_incoming_requests_doc,
            get_outgoing_requests_doc,
            get_received_requests_doc,
            accept_friend_request_doc,
            reject_friend_request_doc,
            cancel_friend_request_doc,
            check_friendship_doc,
            get_friend_suggestions_doc,
            remove_friend_doc,
            update_friend_note_doc,
            get_friend_status_doc,
            update_friend_status_doc,
            get_friend_info_doc,
            update_friend_displayname_doc,
            get_friend_groups_doc,
            create_friend_group_doc,
            delete_friend_group_doc,
            rename_friend_group_doc,
            get_friend_group_friends_doc,
            add_friend_to_group_doc,
            remove_friend_from_group_doc,
            get_friend_groups_for_user_doc,
            get_friend_dm_doc,
        ),
        components(
            schemas(
                ApiHealthStatus,
                ApiHealthCheckResult,
            ),
        ),
    )]
    struct ApiDoc;

    let openapi = ApiDoc::openapi();

    axum::Router::new().merge(SwaggerUi::new("/_swagger").url("/_api-doc/openapi.json", openapi)).with_state(_state)
}

/// Stub for when `openapi-docs` is not enabled.
#[cfg(not(feature = "openapi-docs"))]
pub fn swagger_ui_router(_state: AppState) -> axum::Router<AppState> {
    axum::Router::new()
}

// ==========================
// Path annotations (canonical examples)
// ==========================

/// `GET /_matrix/client/versions` — Return supported Matrix protocol versions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/versions",
    tag = "Health",
    responses(
        (status = 200, description = "Supported Matrix protocol versions",
            body = serde_json::Value,
            example = json!({
                "versions": ["r0.0.1", "r0.1.0", "r0.2.0", "r0.3.0", "r0.4.0",
                    "r0.5.0", "r0.6.0", "r0.6.1", "v1.1", "v1.2", "v1.3", "v1.4",
                    "v1.5", "v1.6", "v1.7", "v1.8", "v1.9", "v1.10", "v1.11", "v1.12", "v1.13"
                ],
                "unstable_features": {}
            })
        ),
    ),
)]
pub fn get_client_versions() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /health` — Basic liveness check.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/health",
    tag = "Health",
    responses(
        (status = 200, description = "Service is alive",
            body = serde_json::Value,
            example = json!({"status": "ok"})
        ),
    ),
)]
pub fn health_check() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_health` — Detailed health check with component statuses.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_health",
    tag = "Health",
    responses(
        (status = 200, description = "Detailed health status",
            body = ApiHealthStatus,
        ),
    ),
)]
pub fn detailed_health_check() -> axum::Json<ApiHealthStatus> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /.well-known/matrix/server` — Server discovery.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/.well-known/matrix/server",
    tag = "Health",
    responses(
        (status = 200, description = "Server well-known information",
            body = serde_json::Value,
            example = json!({"m.server": "matrix.example.com:443"})
        ),
    ),
)]
pub fn get_well_known_server() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/server_version` — Return homeserver version metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/server_version",
    tag = "Health",
    responses(
        (status = 200, description = "Homeserver version metadata",
            body = serde_json::Value,
            example = json!({
                "server_version": "6.0.4",
                "python_version": "Rust",
                "server_name": "example.com"
            })
        ),
    ),
)]
pub fn get_server_version() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/capabilities` — Return client capability surface.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/capabilities",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Client capabilities",
            body = serde_json::Value,
            example = json!({
                "capabilities": {
                    "m.change_password": { "enabled": true },
                    "m.set_displayname": { "enabled": true },
                    "m.set_avatar_url": { "enabled": true },
                    "m.3pid_changes": { "enabled": true },
                    "m.room_versions": {
                        "default": "10",
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
                            "11": "stable",
                            "12": "stable",
                            "13": "stable"
                        }
                    }
                },
                "unstable_features": {
                    "org.matrix.msc3886.sliding_sync": true
                }
            })
        ),
    ),
)]
pub fn get_capabilities() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /.well-known/matrix/client` — Client discovery.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/.well-known/matrix/client",
    tag = "Health",
    responses(
        (status = 200, description = "Client well-known information",
            body = serde_json::Value,
            example = json!({
                "m.homeserver": {
                    "base_url": "https://matrix.example.com"
                }
            })
        ),
    ),
)]
pub fn get_well_known_client() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /.well-known/matrix/support` — Support discovery.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/.well-known/matrix/support",
    tag = "Health",
    responses(
        (status = 200, description = "Support metadata",
            body = serde_json::Value,
            example = json!({
                "url": "https://matrix.org"
            })
        ),
    ),
)]
pub fn get_well_known_support() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/account_data/` — List account data for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/account_data/",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID")
    ),
    responses(
        (status = 200, description = "Account data map",
            body = serde_json::Value,
            example = json!({
                "account_data": {
                    "m.push_rules": {
                        "global": {
                            "override": []
                        }
                    }
                }
            })
        ),
    ),
)]
pub fn list_account_data() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/account_data/{type}` — Read one account data event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/account_data/{type}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("type" = String, Path, description = "Account data event type")
    ),
    responses(
        (status = 200, description = "Account data content", body = serde_json::Value),
        (status = 404, description = "Account data not found")
    ),
)]
pub fn get_account_data() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/user/{user_id}/account_data/{type}` — Set one account data event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/user/{user_id}/account_data/{type}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("type" = String, Path, description = "Account data event type")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Account data updated", body = serde_json::Value),
        (status = 403, description = "Cannot modify account data for another user")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_account_data_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v3/user/{user_id}/account_data/{type}` — Delete one account data event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v3/user/{user_id}/account_data/{type}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("type" = String, Path, description = "Account data event type")
    ),
    responses(
        (status = 200, description = "Account data deleted", body = serde_json::Value),
        (status = 404, description = "Account data not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_account_data_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}` — Read one room-scoped account data event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("type" = String, Path, description = "Room account data event type")
    ),
    responses(
        (status = 200, description = "Room account data content", body = serde_json::Value),
        (status = 404, description = "Room account data not found")
    ),
)]
pub fn get_room_account_data() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}` — Set one room-scoped account data event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("type" = String, Path, description = "Room account data event type")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Room account data updated", body = serde_json::Value),
        (status = 403, description = "Cannot modify room account data for another user")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_room_account_data_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}` — Delete one room-scoped account data event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("type" = String, Path, description = "Room account data event type")
    ),
    responses(
        (status = 200, description = "Room account data deleted", body = serde_json::Value),
        (status = 404, description = "Room account data not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_room_account_data_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/user/{user_id}/filter` — Save a new sync filter.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/user/{user_id}/filter",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Filter created",
            body = serde_json::Value,
            example = json!({
                "filter_id": "12345"
            })
        ),
        (status = 403, description = "Cannot create a filter for another user")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn create_filter_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/filter/{filter_id}` — Read one saved sync filter.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/filter/{filter_id}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("filter_id" = String, Path, description = "Filter ID")
    ),
    responses(
        (status = 200, description = "Saved filter document", body = serde_json::Value),
        (status = 404, description = "Filter not found")
    ),
)]
pub fn get_filter() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v3/user/{user_id}/filter/{filter_id}` — Delete one saved sync filter.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v3/user/{user_id}/filter/{filter_id}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("filter_id" = String, Path, description = "Filter ID")
    ),
    responses(
        (status = 200, description = "Filter deleted", body = serde_json::Value),
        (status = 404, description = "Filter not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_filter_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/user/{user_id}/openid/request_token` — Issue an OpenID token for the caller.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/user/{user_id}/openid/request_token",
    tag = "Authentication",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID")
    ),
    responses(
        (status = 200, description = "OpenID token issued",
            body = serde_json::Value,
            example = json!({
                "access_token": "openid_access_token",
                "token_type": "Bearer",
                "matrix_server_name": "example.com",
                "expires_in": 3600
            })
        ),
        (status = 403, description = "Cannot issue a token for another user")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_openid_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushers` — List pushers for the authenticated device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushers",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Pushers",
            body = serde_json::Value,
            example = json!({
                "pushers": [{
                    "pushkey": "push-key",
                    "kind": "http",
                    "app_id": "com.example.app"
                }]
            })
        ),
    ),
)]
pub fn get_pushers() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushrules` — Read all push rules.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushrules",
    tag = "Client-Server",
    responses(
        (status = 200, description = "All push rules", body = serde_json::Value)
    ),
)]
pub fn get_push_rules() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushrules/{scope}` — Read one push rule scope.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushrules/{scope}",
    tag = "Client-Server",
    params(
        ("scope" = String, Path, description = "Push rule scope, for example global")
    ),
    responses(
        (status = 200, description = "Push rule scope", body = serde_json::Value)
    ),
)]
pub fn get_push_rules_scope() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushrules/{scope}/{kind}` — Read rules of one kind within a scope.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushrules/{scope}/{kind}",
    tag = "Client-Server",
    params(
        ("scope" = String, Path, description = "Push rule scope"),
        ("kind" = String, Path, description = "Push rule kind")
    ),
    responses(
        (status = 200, description = "Push rule kind listing", body = serde_json::Value)
    ),
)]
pub fn get_push_rules_kind() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}` — Read one push rule.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}",
    tag = "Client-Server",
    params(
        ("scope" = String, Path, description = "Push rule scope"),
        ("kind" = String, Path, description = "Push rule kind"),
        ("rule_id" = String, Path, description = "Push rule ID")
    ),
    responses(
        (status = 200, description = "Push rule", body = serde_json::Value),
        (status = 404, description = "Push rule not found")
    ),
)]
pub fn get_push_rule() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/devices` — List devices for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/devices",
    tag = "Client-Server",
    responses(
        (status = 200, description = "User devices",
            body = serde_json::Value,
            example = json!({
                "devices": [{
                    "device_id": "DEVICEID",
                    "display_name": "Primary phone",
                    "last_seen_ts": 1718000000000_i64,
                    "last_seen_ip": "203.0.113.10"
                }]
            })
        ),
    ),
)]
pub fn get_devices() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/devices/{device_id}` — Read one device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/devices/{device_id}",
    tag = "Client-Server",
    params(
        ("device_id" = String, Path, description = "Matrix device ID")
    ),
    responses(
        (status = 200, description = "Device details",
            body = serde_json::Value,
            example = json!({
                "device": {
                    "device_id": "DEVICEID",
                    "display_name": "Primary phone",
                    "last_seen_ts": 1718000000000_i64
                },
                "device_id": "DEVICEID",
                "display_name": "Primary phone",
                "last_seen_ts": 1718000000000_i64
            })
        ),
        (status = 404, description = "Device not found")
    ),
)]
pub fn get_device() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/devices/{device_id}` — Update device metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/devices/{device_id}",
    tag = "Client-Server",
    params(
        ("device_id" = String, Path, description = "Matrix device ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Device updated",
            body = serde_json::Value,
            example = json!({
                "device_id": "DEVICEID",
                "display_name": "Updated device name",
                "updated_ts": 1718000000000_i64
            })
        ),
        (status = 404, description = "Device not found"),
        (status = 400, description = "Invalid display_name")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v3/devices/{device_id}` — Delete one device with UIA.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v3/devices/{device_id}",
    tag = "Client-Server",
    params(
        ("device_id" = String, Path, description = "Matrix device ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Device deleted", body = serde_json::Value),
        (status = 401, description = "UIA challenge required"),
        (status = 404, description = "Device not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/delete_devices` — Delete multiple devices with UIA.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/delete_devices",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Devices deleted", body = serde_json::Value),
        (status = 401, description = "UIA challenge required"),
        (status = 400, description = "Missing or invalid device_ids")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_devices_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/tags` — List all tags grouped by room for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/tags",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID")
    ),
    responses(
        (status = 200, description = "All room tags for the user",
            body = serde_json::Value,
            example = json!({
                "tags": {
                    "!room:example.com": {
                        "m.favourite": {
                            "order": 0.5
                        }
                    }
                }
            })
        ),
        (status = 403, description = "Access denied")
    ),
)]
pub fn get_global_tags() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags` — List tags for one room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room tags",
            body = serde_json::Value,
            example = json!({
                "tags": {
                    "m.favourite": {
                        "order": 0.5
                    },
                    "u.work": {
                        "order": 1.0
                    }
                }
            })
        ),
        (status = 403, description = "Access denied")
    ),
)]
pub fn get_room_tags() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}` — Add or update one room tag.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("tag" = String, Path, description = "Tag name")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Room tag updated", body = serde_json::Value),
        (status = 403, description = "Access denied")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn put_room_tag_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}` — Remove one room tag.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v3/user/{user_id}/rooms/{room_id}/tags/{tag}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Authenticated Matrix user ID"),
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("tag" = String, Path, description = "Tag name")
    ),
    responses(
        (status = 200, description = "Room tag deleted", body = serde_json::Value),
        (status = 403, description = "Access denied")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_room_tag_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/profile/{user_id}` — Read the public profile for a Matrix user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/profile/{user_id}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User profile",
            body = serde_json::Value,
            example = json!({
                "displayname": "Alice",
                "avatar_url": "mxc://example.com/avatar"
            })
        ),
        (status = 403, description = "Profile is not visible to the caller"),
        (status = 404, description = "User not found")
    ),
)]
pub fn get_profile_info() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/profile/{user_id}/displayname` — Read the public display name for a Matrix user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/profile/{user_id}/displayname",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Display name",
            body = serde_json::Value,
            example = json!({
                "displayname": "Alice"
            })
        ),
        (status = 403, description = "Profile is not visible to the caller"),
        (status = 404, description = "User not found")
    ),
)]
pub fn get_profile_displayname() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/profile/{user_id}/avatar_url` — Read the public avatar URL for a Matrix user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/profile/{user_id}/avatar_url",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Avatar URL",
            body = serde_json::Value,
            example = json!({
                "avatar_url": "mxc://example.com/avatar"
            })
        ),
        (status = 403, description = "Profile is not visible to the caller"),
        (status = 404, description = "User not found")
    ),
)]
pub fn get_profile_avatar_url() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/profile/{user_id}/avatar_url` — Set user avatar URL.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/profile/{user_id}/avatar_url",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "The user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Avatar URL updated", body = serde_json::Value),
        (status = 403, description = "Cannot update another user's profile")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_avatar_url_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/presence/{user_id}/status` — Read presence for one user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/presence/{user_id}/status",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Presence state",
            body = serde_json::Value,
            example = json!({
                "presence": "online",
                "status_msg": "Available",
                "last_active_ago": 1200,
                "currently_active": true
            })
        ),
        (status = 403, description = "Presence is not visible to the caller"),
        (status = 404, description = "User not found")
    ),
)]
pub fn get_presence_status() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/presence/list` — Read the caller's current presence subscriptions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/presence/list",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Presence subscription list",
            body = serde_json::Value,
            example = json!({
                "presences": [{
                    "user_id": "@alice:example.com",
                    "presence": "online",
                    "status_msg": "Working",
                    "last_active_ago": 500,
                    "currently_active": true
                }]
            })
        )
    ),
)]
pub fn get_presence_list_current_user() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/presence/list/{user_id}` — Read presence subscriptions for a specific user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/presence/list/{user_id}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Presence subscription list for the target user",
            body = serde_json::Value,
            example = json!({
                "presences": [{
                    "user_id": "@bob:example.com",
                    "presence": "offline",
                    "status_msg": null,
                    "last_active_ago": null,
                    "currently_active": null
                }]
            })
        ),
        (status = 403, description = "Access denied")
    ),
)]
pub fn get_presence_list_for_user() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/account/whoami` — Return the authenticated user and device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/account/whoami",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Authenticated Matrix principal",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "device_id": "DEVICEID",
                "is_guest": false
            })
        )
    ),
)]
pub fn get_whoami() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/account/3pid` — List bound third-party identifiers for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/account/3pid",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Bound third-party identifiers",
            body = serde_json::Value,
            example = json!({
                "threepids": [{
                    "medium": "email",
                    "address": "alice@example.com",
                    "validated_ts": 1718000000000_i64,
                    "added_at": 1717990000000_i64
                }]
            })
        )
    ),
)]
pub fn get_threepids() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/status` — Read dehydrated device status for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/status",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Dehydrated device status",
            body = serde_json::Value,
            example = json!({
                "device_id": "DEHYDRATEDDEVICE",
                "device_data": {
                    "algorithm": "m.dehydration.v1.olm"
                }
            })
        ),
        (status = 404, description = "No dehydrated device exists")
    ),
)]
pub fn get_dehydrated_device_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/unstable/org.matrix.msc4143/rtc/transports` — Read configured MatrixRTC transport information.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/unstable/org.matrix.msc4143/rtc/transports",
    tag = "Client-Server",
    responses(
        (status = 200, description = "RTC transports",
            body = serde_json::Value,
            example = json!({
                "transports": [{
                    "type": "org.matrix.msc4403.ice-server-transport",
                    "ice_servers": [{
                        "urls": ["stun:stun.example.com:3478"]
                    }, {
                        "urls": ["turn:turn.example.com:3478?transport=udp"],
                        "username": "@alice:example.com",
                        "credential": "turn-secret"
                    }]
                }]
            })
        )
    ),
)]
pub fn get_rtc_transports_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/login/qrcode/{session_id}/status` — Get QR login transaction status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/login/qrcode/{session_id}/status",
    tag = "Client-Server",
    params(
        ("session_id" = String, Path, description = "QR login transaction ID")
    ),
    responses(
        (status = 200, description = "QR login transaction status",
            body = serde_json::Value,
            example = json!({
                "transaction_id": "qr_1234",
                "user_id": "@alice:example.com",
                "status": "pending_confirmation"
            })
        ),
        (status = 404, description = "Transaction not found")
    ),
)]
pub fn get_qr_status() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/media/config` — Get Matrix media configuration (max upload size).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/media/config",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Media configuration",
            body = serde_json::Value,
            example = json!({
                "m.upload.size": 52428800
            })
        )
    ),
)]
pub fn get_media_config() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users` — List registered users (Admin only).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users",
    tag = "Admin",
    params(
        ("from" = Option<String>, Query, description = "Pagination token"),
        ("limit" = Option<i64>, Query, description = "Maximum number of users to return"),
        ("name" = Option<String>, Query, description = "Filter by username (wildcard)")
    ),
    responses(
        (status = 200, description = "List of users",
            body = serde_json::Value,
            example = json!({
                "users": [
                    {
                        "user_id": "@alice:example.com",
                        "displayname": "Alice",
                        "is_admin": false,
                        "deactivated": false
                    }
                ],
                "next_token": "12345",
                "total": 1
            })
        ),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn list_users_admin_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/rooms` — List rooms on the server (Admin only).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/rooms",
    tag = "Admin",
    params(
        ("from" = Option<String>, Query, description = "Pagination token"),
        ("limit" = Option<i64>, Query, description = "Maximum number of rooms to return"),
        ("order_by" = Option<String>, Query, description = "Field to order by (name, canonical_alias, joined_members, joined_local_members, version, creator, encryption, is_public)"),
        ("dir" = Option<String>, Query, description = "Direction (f, b)")
    ),
    responses(
        (status = 200, description = "List of rooms",
            body = serde_json::Value,
            example = json!({
                "rooms": [
                    {
                        "room_id": "!abc:example.com",
                        "name": "General",
                        "canonical_alias": "#general:example.com",
                        "joined_members": 42,
                        "is_public": true
                    }
                ],
                "next_batch": "56789",
                "total_rooms": 1
            })
        ),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn list_rooms_admin_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}` — Delete a user account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User deleted",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "deleted": true
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/{user_id}/evict` — Evict a user from all joined rooms.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/{user_id}/evict",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User evicted from joined rooms",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "rooms_evicted": 2,
                "rooms": ["!room1:example.com", "!room2:example.com"],
                "failures": []
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_evict_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_synapse/admin/v1/users/{user_id}/admin` — Update whether a user is an admin.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_synapse/admin/v1/users/{user_id}/admin",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Admin flag updated", body = serde_json::Value, example = json!({"success": true})),
        (status = 400, description = "Missing admin field"),
        (status = 403, description = "Only super_admin can change privileges"),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_set_user_admin_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/{user_id}/deactivate` — Deactivate a user account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/{user_id}/deactivate",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User deactivated",
            body = serde_json::Value,
            example = json!({"id_server_unbind_result": "success"})
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_deactivate_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/{user_id}/password` — Reset a user's password.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/{user_id}/password",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Password reset completed", body = serde_json::Value),
        (status = 400, description = "Password does not satisfy validation requirements"),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_reset_user_password_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v2/users/{user_id}` — Read detailed information for one user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v2/users/{user_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Detailed user information",
            body = serde_json::Value,
            example = json!({
                "name": "@alice:example.com",
                "user_id": "@alice:example.com",
                "is_guest": false,
                "admin": false,
                "deactivated": false,
                "displayname": "Alice",
                "avatar_url": "mxc://example.com/alice",
                "created_ts": 1718000000000_i64,
                "user_type": null,
                "devices": [{
                    "device_id": "DEVICEID",
                    "display_name": "Alice phone",
                    "last_seen_ts": 1718000000000_i64
                }],
                "threepids": [],
                "external_ids": []
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_v2_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_synapse/admin/v2/users/{user_id}` — Create or update one user's admin-facing profile.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_synapse/admin/v2/users/{user_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "User created or updated", body = serde_json::Value),
        (status = 403, description = "Only super_admin can change admin or user_type")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_upsert_user_v2_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/rooms` — List rooms joined by a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/rooms",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID"),
        ("limit" = Option<i64>, Query, description = "Maximum number of rooms to return"),
        ("from" = Option<String>, Query, description = "Pagination token")
    ),
    responses(
        (status = 200, description = "Rooms joined by the user",
            body = serde_json::Value,
            example = json!({
                "joined_rooms": ["!room:example.com", "!other:example.com"],
                "total": 2,
                "next_batch": null
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/devices` — List devices owned by a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/devices",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User devices",
            body = serde_json::Value,
            example = json!({
                "devices": [{
                    "device_id": "DEVICEID",
                    "display_name": "Alice phone",
                    "last_seen_ts": 1718000000000_i64,
                    "last_seen_ip": "203.0.113.10"
                }],
                "total": 1
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_devices_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}/devices/{device_id}` — Revoke one user device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}/devices/{device_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID"),
        ("device_id" = String, Path, description = "Matrix device ID")
    ),
    responses(
        (status = 200, description = "Device revoked", body = serde_json::Value),
        (status = 404, description = "User or device not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_user_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/{user_id}/login` — Mint an access token to impersonate a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/{user_id}/login",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Impersonation token issued",
            body = serde_json::Value,
            example = json!({
                "access_token": "access_token_value",
                "device_id": "ABCDEFGHIJ",
                "user_id": "@alice:example.com"
            })
        ),
        (status = 400, description = "User is deactivated"),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_login_as_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/{user_id}/logout` — Invalidate all sessions for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/{user_id}/logout",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "All user sessions invalidated",
            body = serde_json::Value,
            example = json!({"devices_deleted": 3})
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_logout_user_devices_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/user_stats` — Read aggregate user statistics.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/user_stats",
    tag = "Admin",
    responses(
        (status = 200, description = "Aggregate user statistics",
            body = serde_json::Value,
            example = json!({
                "total_users": 120,
                "active_users": 80,
                "admin_users": 2,
                "deactivated_users": 5,
                "guest_users": 10,
                "average_rooms_per_user": 3.5,
                "user_registration_enabled": true
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/stats` — Read one user's statistics dashboard.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/stats",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Per-user statistics dashboard",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "rooms_joined": 12,
                "messages_sent": 345,
                "last_seen_ts": 1718000000000_i64,
                "creation_ts": 1717000000000_i64,
                "is_admin": false,
                "dashboard": {
                    "total_rooms": 12,
                    "total_messages": 345,
                    "last_seen": 1718000000000_i64
                }
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_single_user_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/batch` — Create multiple users in one request.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/batch",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Batch user creation result",
            body = serde_json::Value,
            example = json!({
                "created": 3,
                "failed": 0,
                "total": 3
            })
        ),
        (status = 403, description = "Only super_admin can create admin users")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_batch_create_users_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/batch_deactivate` — Deactivate multiple users in one request.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/batch_deactivate",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Batch deactivation result",
            body = serde_json::Value,
            example = json!({
                "deactivated": 3,
                "failed": 0,
                "total": 3
            })
        ),
        (status = 400, description = "Too many users in batch request")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_batch_deactivate_users_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/user_sessions/{user_id}` — List active sessions for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/user_sessions/{user_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User session listing",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "sessions": [{
                    "device_id": "DEVICEID",
                    "display_name": "Alice phone",
                    "last_seen_ts": 1718000000000_i64,
                    "last_seen_ip": "203.0.113.10",
                    "session_id": "DEVICEID"
                }],
                "total": 1
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_sessions_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/user_sessions/{user_id}/invalidate` — Invalidate all sessions for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/user_sessions/{user_id}/invalidate",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User sessions invalidated",
            body = serde_json::Value,
            example = json!({
                "invalidated": true,
                "sessions_removed": 3
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_invalidate_user_sessions_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/account/{user_id}` — Read admin-facing account details for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/account/{user_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Admin-facing account details",
            body = serde_json::Value,
            example = json!({
                "name": "alice",
                "user_id": "@alice:example.com",
                "displayname": "Alice",
                "admin": false,
                "deactivated": false,
                "creation_ts": 1717000000000_i64,
                "device_count": 2,
                "room_count": 12
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_account_details_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/account/{user_id}` — Update admin-facing account details for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/account/{user_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Account updated",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "updated": true
            })
        ),
        (status = 403, description = "Only super_admin can change admin privilege"),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_update_account_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/rooms/{room_id}` — Read admin-visible room details.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/rooms/{room_id}",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room details",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "name": "General",
                "topic": "Main room",
                "creator": "@admin:example.com",
                "member_count": 42,
                "room_version": "11",
                "encryption": "m.megolm.v1.aes-sha2",
                "is_public": true,
                "join_rule": "public"
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/rooms/{room_id}/members` — List room members for moderation.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/rooms/{room_id}/members",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("limit" = Option<i64>, Query, description = "Maximum number of members to return"),
        ("from" = Option<String>, Query, description = "Pagination token")
    ),
    responses(
        (status = 200, description = "Room members",
            body = serde_json::Value,
            example = json!({
                "members": [{
                    "user_id": "@alice:example.com",
                    "displayname": "Alice",
                    "avatar_url": "mxc://example.com/alice",
                    "membership": "join"
                }],
                "total": 42,
                "next_batch": null
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_room_members_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/rooms/{room_id}/state` — List state events for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/rooms/{room_id}/state",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room state events",
            body = serde_json::Value,
            example = json!({
                "state": [{
                    "type": "m.room.name",
                    "state_key": "",
                    "content": { "name": "General" },
                    "sender": "@admin:example.com",
                    "event_id": "$event:example.com"
                }]
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_room_state_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/spaces` — List all spaces for administration.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/spaces",
    tag = "Admin",
    responses(
        (status = 200, description = "Spaces",
            body = serde_json::Value,
            example = json!({
                "spaces": [{
                    "space_id": "!space:example.com",
                    "room_id": "!space:example.com",
                    "name": "Workspace",
                    "topic": "Team space",
                    "creator": "@admin:example.com",
                    "created_ts": 1718000000000_i64
                }],
                "total": 1
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_spaces_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/spaces/{space_id}` — Read one space.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/spaces/{space_id}",
    tag = "Admin",
    params(
        ("space_id" = String, Path, description = "Space ID or identifier")
    ),
    responses(
        (status = 200, description = "Space details",
            body = serde_json::Value,
            example = json!({
                "space_id": "!space:example.com",
                "room_id": "!space:example.com",
                "name": "Workspace",
                "topic": "Team space",
                "creator": "@admin:example.com",
                "created_ts": 1718000000000_i64
            })
        ),
        (status = 404, description = "Space not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_space_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/spaces/{space_id}` — Delete a space.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/spaces/{space_id}",
    tag = "Admin",
    params(
        ("space_id" = String, Path, description = "Space ID or identifier")
    ),
    responses(
        (status = 200, description = "Space deleted",
            body = serde_json::Value,
            example = json!({
                "deleted": true
            })
        ),
        (status = 404, description = "Space not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_space_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/spaces/{space_id}/users` — List users in a space.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/spaces/{space_id}/users",
    tag = "Admin",
    params(
        ("space_id" = String, Path, description = "Space ID or identifier")
    ),
    responses(
        (status = 200, description = "Space users",
            body = serde_json::Value,
            example = json!({
                "users": ["@alice:example.com", "@bob:example.com"],
                "total": 2
            })
        ),
        (status = 404, description = "Space not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_space_users_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/spaces/{space_id}/rooms` — List rooms under a space.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/spaces/{space_id}/rooms",
    tag = "Admin",
    params(
        ("space_id" = String, Path, description = "Space ID or identifier")
    ),
    responses(
        (status = 200, description = "Space rooms",
            body = serde_json::Value,
            example = json!({
                "rooms": ["!room:example.com", "!other:example.com"],
                "total": 2
            })
        ),
        (status = 404, description = "Space not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_space_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/spaces/{space_id}/stats` — Read aggregate statistics for a space.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/spaces/{space_id}/stats",
    tag = "Admin",
    params(
        ("space_id" = String, Path, description = "Space ID or identifier")
    ),
    responses(
        (status = 200, description = "Space statistics",
            body = serde_json::Value,
            example = json!({
                "space_id": "!space:example.com",
                "member_count": 12,
                "child_room_count": 4
            })
        ),
        (status = 404, description = "Space not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_space_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/room_stats` — Read global room statistics overview.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/room_stats",
    tag = "Admin",
    responses(
        (status = 200, description = "Global room statistics overview", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_room_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/room_stats/{room_id}` — Read statistics for one room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/room_stats/{room_id}",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room statistics", body = serde_json::Value),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_single_room_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/rooms/{room_id}/listings` — Read room directory visibility status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/rooms/{room_id}/listings",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room listing status",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "public": true,
                "in_directory": true
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_room_listings_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_synapse/admin/v1/rooms/{room_id}/listings/public` — Set a room as public.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_synapse/admin/v1/rooms/{room_id}/listings/public",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room visibility updated",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "public": true
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_set_room_public_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/rooms/{room_id}/listings/public` — Set a room as private.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/rooms/{room_id}/listings/public",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room visibility updated",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "public": false
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_set_room_private_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/rooms/{room_id}/block` — Read block status for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/rooms/{room_id}/block",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room block status",
            body = serde_json::Value,
            example = json!({
                "block": true,
                "blocked_at": 1718000000000_i64
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_room_block_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/rooms/{room_id}/block` — Block a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/rooms/{room_id}/block",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Room block state updated",
            body = serde_json::Value,
            example = json!({
                "block": true
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_block_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/rooms/{room_id}/unblock` — Unblock a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/rooms/{room_id}/unblock",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room block state updated",
            body = serde_json::Value,
            example = json!({
                "block": false
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_unblock_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/rooms/{room_id}/make_admin` — Grant room admin power to a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/rooms/{room_id}/make_admin",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Power levels updated", body = serde_json::Value),
        (status = 404, description = "Room or user not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_make_room_admin_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/purge_history` — Purge room history before a timestamp.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/purge_history",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "History purged",
            body = serde_json::Value,
            example = json!({
                "success": true,
                "deleted_events": 123
            })
        ),
        (status = 400, description = "Missing room_id"),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_purge_history_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/purge_room` — Permanently delete a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/purge_room",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Room purged",
            body = serde_json::Value,
            example = json!({
                "purge_id": "550e8400-e29b-41d4-a716-446655440000",
                "success": true
            })
        ),
        (status = 400, description = "Missing room_id"),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_purge_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_synapse/admin/v1/rooms/{room_id}/members/{user_id}` — Force-join a user to a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_synapse/admin/v1/rooms/{room_id}/members/{user_id}",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Membership updated",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "room_id": "!room:example.com",
                "membership": "join"
            })
        ),
        (status = 404, description = "Room or user not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_join_room_member_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/rooms/{room_id}/members/{user_id}` — Remove a user from a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/rooms/{room_id}/members/{user_id}",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID"),
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Membership updated",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "room_id": "!room:example.com",
                "removed": true
            })
        ),
        (status = 404, description = "Room or user not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_remove_room_member_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/rooms/cleanup` — Clean up abnormal room data.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/rooms/cleanup",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Cleanup results", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_cleanup_abnormal_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/server_version` — Return admin-visible server version metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/server_version",
    tag = "Admin",
    responses(
        (status = 200, description = "Admin server version metadata",
            body = serde_json::Value,
            example = json!({
                "server_version": "6.0.4",
                "python_version": "Rust",
                "server_name": "example.com"
            })
        ),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_server_version_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/info` — Return privileged homeserver metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/info",
    tag = "Admin",
    responses(
        (status = 200, description = "Privileged homeserver metadata",
            body = serde_json::Value,
            example = json!({
                "server_name": "example.com",
                "server_version": "6.0.4",
                "implementation": "synapse-rust"
            })
        ),
        (status = 403, description = "Only super_admin can access server information")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_info_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/whoami` — Return the current admin principal.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/whoami",
    tag = "Admin",
    responses(
        (status = 200, description = "Authenticated admin principal",
            body = serde_json::Value,
            example = json!({
                "user_id": "@admin:example.com",
                "name": "@admin:example.com",
                "is_admin": true,
                "role": "super_admin"
            })
        ),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_whoami_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/statistics` — Return aggregate homeserver statistics.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/statistics",
    tag = "Admin",
    responses(
        (status = 200, description = "Aggregate server statistics",
            body = serde_json::Value,
            example = json!({
                "total_users": 120,
                "total_rooms": 45,
                "daily_active_users": 120,
                "monthly_active_users": 120,
                "r30_users": 120,
                "r30v2_users": 120
            })
        ),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_statistics_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/status` — Return high-level server health status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/status",
    tag = "Admin",
    responses(
        (status = 200, description = "Admin health status",
            body = serde_json::Value,
            example = json!({
                "db_ok": true,
                "server_ok": true,
                "up": true
            })
        ),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/whois/{user_id}` — Inspect devices and connections for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/whois/{user_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User connection summary",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "devices": [{
                    "device_id": "DEVICEID",
                    "display_name": "Alice phone",
                    "last_seen": 1718000000000_i64,
                    "ip": "203.0.113.10"
                }]
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_whois_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/whois/{user_id}/{device_id}` — Inspect one device connection for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/whois/{user_id}/{device_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID"),
        ("device_id" = String, Path, description = "Matrix device ID")
    ),
    responses(
        (status = 200, description = "Device connection summary",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "device_id": "DEVICEID",
                "display_name": "Alice phone",
                "last_seen": 1718000000000_i64,
                "ip": "203.0.113.10"
            })
        ),
        (status = 404, description = "User or device not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_whois_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/purge_media_cache` — Purge cached media older than a given timestamp.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/purge_media_cache",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Media cache purge summary",
            body = serde_json::Value,
            example = json!({
                "deleted": 42
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_purge_media_cache_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/health` — Read admin health probe output.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/health",
    tag = "Admin",
    responses(
        (status = 200, description = "Admin health probe",
            body = serde_json::Value,
            example = json!({
                "status": "ok",
                "database": "ok"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_health_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/config` — Read selected homeserver configuration values.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/config",
    tag = "Admin",
    responses(
        (status = 200, description = "Selected homeserver configuration",
            body = serde_json::Value,
            example = json!({
                "server_name": "example.com",
                "public_baseurl": "https://matrix.example.com",
                "registration_enabled": true,
                "max_upload_size": 10485760
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_config_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/jitsi/config` — Read Jitsi integration settings.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/jitsi/config",
    tag = "Admin",
    responses(
        (status = 200, description = "Jitsi integration configuration",
            body = serde_json::Value,
            example = json!({
                "domain": "meet.jit.si",
                "app_id": null,
                "jwt_enabled": false,
                "jwt_asap_enabled": false,
                "jwt_auth_type": "none",
                "server_name": "example.com"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_jitsi_config_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/invite/blocklist` — Read the global invite blocklist.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/invite/blocklist",
    tag = "Admin",
    responses(
        (status = 200, description = "Global invite blocklist",
            body = serde_json::Value,
            example = json!({
                "blocklist": ["bad.example.com", "spam.example.net"]
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_invite_blocklist_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/invite/allowlist` — Read the global invite allowlist.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/invite/allowlist",
    tag = "Admin",
    responses(
        (status = 200, description = "Global invite allowlist",
            body = serde_json::Value,
            example = json!({
                "allowlist": ["trusted.example.com"]
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_invite_allowlist_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/federation/destinations` — List known federation destinations.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/federation/destinations",
    tag = "Admin",
    params(
        ("from" = Option<String>, Query, description = "Keyset pagination cursor"),
        ("limit" = Option<i32>, Query, description = "Maximum number of destinations to return")
    ),
    responses(
        (status = 200, description = "Federation destination list",
            body = serde_json::Value,
            example = json!({
                "destinations": [{
                    "destination": "matrix.org",
                    "retry_last_ts": 1718000000000_i64,
                    "retry_interval": 0,
                    "failure_ts": null,
                    "last_successful_stream_ordering": 12345
                }],
                "total": 1,
                "total_count": 1,
                "next_batch": null
            })
        ),
        (status = 400, description = "Invalid pagination cursor"),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_federation_destinations_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/federation/destinations/{destination}` — Read one federation destination.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/federation/destinations/{destination}",
    tag = "Admin",
    params(
        ("destination" = String, Path, description = "Remote server name")
    ),
    responses(
        (status = 200, description = "Federation destination details", body = serde_json::Value),
        (status = 404, description = "Destination not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_federation_destination_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/federation/destinations/{destination}/rooms` — List rooms tied to a destination.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/federation/destinations/{destination}/rooms",
    tag = "Admin",
    params(
        ("destination" = String, Path, description = "Remote server name")
    ),
    responses(
        (status = 200, description = "Destination room list",
            body = serde_json::Value,
            example = json!({
                "rooms": ["!room:example.com"],
                "total": 1
            })
        ),
        (status = 403, description = "Admin only")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_federation_destination_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/reports` — List moderation reports.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/reports",
    tag = "Admin",
    params(
        ("limit" = Option<i32>, Query, description = "Maximum number of reports to return"),
        ("since_score" = Option<i32>, Query, description = "Score cursor"),
        ("since_ts" = Option<i64>, Query, description = "Timestamp cursor"),
        ("since_id" = Option<i64>, Query, description = "Report ID cursor")
    ),
    responses(
        (status = 200, description = "Moderation reports",
            body = serde_json::Value,
            example = json!({
                "reports": [{
                    "id": 1,
                    "room_id": "!room:example.com",
                    "event_id": "$event:example.com",
                    "user_id": "@alice:example.com",
                    "reported_user_id": "@bob:example.com",
                    "reason": "spam",
                    "content": "Unwanted content",
                    "status": "open",
                    "score": -100,
                    "received_ts": 1718000000000_i64
                }],
                "total": 1
            })
        ),
        (status = 400, description = "Legacy offset pagination is not supported")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_reports_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/reports/{report_id}` — Read one moderation report.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/reports/{report_id}",
    tag = "Admin",
    params(
        ("report_id" = i64, Path, description = "Moderation report ID")
    ),
    responses(
        (status = 200, description = "Moderation report details", body = serde_json::Value),
        (status = 404, description = "Report not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_report_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/retention/policy` — Read the server retention policy.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/retention/policy",
    tag = "Admin",
    responses(
        (status = 200, description = "Server retention policy",
            body = serde_json::Value,
            example = json!({
                "max_lifetime": 7776000000_i64,
                "min_lifetime": 86400000_i64,
                "is_expire_on_clients": false
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_retention_policy_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/retention/policy` — Update the server retention policy.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/retention/policy",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Server retention policy updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_set_retention_policy_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/retention/policy/{room_id}` — Read room-specific retention policy.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/retention/policy/{room_id}",
    tag = "Admin",
    params(
        ("room_id" = String, Path, description = "Matrix room ID")
    ),
    responses(
        (status = 200, description = "Room retention policy",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "max_lifetime": 2592000000_i64,
                "min_lifetime": null,
                "is_expire_on_clients": false
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_room_retention_policy_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/retention/status` — Read retention subsystem status summary.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/retention/status",
    tag = "Admin",
    responses(
        (status = 200, description = "Retention subsystem status",
            body = serde_json::Value,
            example = json!({
                "server_policy_enabled": true,
                "rooms_with_custom_policy": 5,
                "lifecycle_cleanup_enabled": true,
                "cleanup_batch_size": 500,
                "audit_retention_days": 30,
                "queue_retention_days": 7,
                "last_run": {
                    "started_ts": 1718000000000_i64,
                    "completed_ts": 1718000005000_i64,
                    "duration_ms": 5000,
                    "expired_events_deleted": 100,
                    "expired_beacons_deleted": 0,
                    "expired_uploads_deleted": 0,
                    "expired_audit_events_deleted": 0,
                    "cleanup_queue_items_processed": 10,
                    "cleanup_queue_rows_pruned": 10,
                    "failed_tasks": 0
                }
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_retention_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/registration_tokens` — List registration tokens with cursor pagination.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/registration_tokens",
    tag = "Admin",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of tokens to return"),
        ("from" = Option<String>, Query, description = "Opaque cursor for the next page")
    ),
    responses(
        (status = 200, description = "Registration token listing",
            body = serde_json::Value,
            example = json!({
                "registration_tokens": [{
                    "token": "invite-token",
                    "uses_allowed": 10,
                    "pending": 0,
                    "completed": 2,
                    "expiry_time": 1719000000000_i64,
                    "created_ts": 1718000000000_i64
                }],
                "next_batch": "cursor"
            })
        ),
        (status = 400, description = "Invalid from cursor")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_registration_tokens_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/registration_tokens` — Create a registration token.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/registration_tokens",
    tag = "Admin",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Registration token created",
            body = serde_json::Value,
            example = json!({
                "token": "invite-token",
                "uses_allowed": 10,
                "pending": 0,
                "completed": 0,
                "expiry_time": 1719000000000_i64,
                "created_ts": 1718000000000_i64
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_create_registration_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/registration_tokens/{token}` — Read one registration token.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/registration_tokens/{token}",
    tag = "Admin",
    params(
        ("token" = String, Path, description = "Registration token string")
    ),
    responses(
        (status = 200, description = "Registration token details", body = serde_json::Value),
        (status = 404, description = "Token not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_registration_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/registration_tokens/{token}` — Delete one registration token.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/registration_tokens/{token}",
    tag = "Admin",
    params(
        ("token" = String, Path, description = "Registration token string")
    ),
    responses(
        (status = 200, description = "Registration token deleted", body = serde_json::Value),
        (status = 404, description = "Token not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_registration_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/registration_tokens/{token}` — Update one registration token.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/registration_tokens/{token}",
    tag = "Admin",
    params(
        ("token" = String, Path, description = "Registration token string")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Registration token updated", body = serde_json::Value),
        (status = 404, description = "Token not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_update_registration_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/tokens` — List access tokens for one user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/tokens",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Access token listing",
            body = serde_json::Value,
            example = json!({
                "tokens": [{
                    "id": 1,
                    "device_id": "DEVICEID",
                    "created_ts": 1718000000000_i64,
                    "expires_at": 1719000000000_i64,
                    "is_revoked": false
                }],
                "total": 1
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_tokens_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}/tokens/{token_id}` — Delete one access token for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}/tokens/{token_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID"),
        ("token_id" = i64, Path, description = "Access token ID")
    ),
    responses(
        (status = 200, description = "Access token deleted", body = serde_json::Value),
        (status = 404, description = "User or token not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_user_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/refresh_tokens` — List refresh tokens for one user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/refresh_tokens",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Refresh token listing",
            body = serde_json::Value,
            example = json!({
                "refresh_tokens": [{
                    "id": 1,
                    "device_id": "DEVICEID",
                    "created_ts": 1718000000000_i64,
                    "expires_at": 1719000000000_i64,
                    "is_revoked": false
                }],
                "total": 1
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_refresh_tokens_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}/refresh_tokens/{token_id}` — Delete one refresh token for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}/refresh_tokens/{token_id}",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID"),
        ("token_id" = i64, Path, description = "Refresh token ID")
    ),
    responses(
        (status = 200, description = "Refresh token deleted", body = serde_json::Value),
        (status = 404, description = "User or token not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_refresh_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/media` — List uploaded media with cursor pagination.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/media",
    tag = "Admin",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of media objects to return"),
        ("from" = Option<String>, Query, description = "Opaque cursor for the next page")
    ),
    responses(
        (status = 200, description = "Media listing",
            body = serde_json::Value,
            example = json!({
                "media": [{
                    "media_id": "abcdef",
                    "media_type": "image/png",
                    "upload_name": "avatar.png",
                    "created_ts": 1718000000000_i64,
                    "last_access_ts": 1718000100000_i64,
                    "media_length": 2048,
                    "user_id": "@alice:example.com",
                    "quarantined": false
                }],
                "total": 1,
                "next_batch": "cursor"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_media_list_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/media/{media_id}` — Read one media object's metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/media/{media_id}",
    tag = "Admin",
    params(
        ("media_id" = String, Path, description = "Media identifier")
    ),
    responses(
        (status = 200, description = "Media metadata", body = serde_json::Value),
        (status = 404, description = "Media not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_media_info_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/media/{media_id}` — Delete one media object.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/media/{media_id}",
    tag = "Admin",
    params(
        ("media_id" = String, Path, description = "Media identifier")
    ),
    responses(
        (status = 200, description = "Media deleted", body = serde_json::Value),
        (status = 404, description = "Media not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_media_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/media/quota` — Read global media quota statistics.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/media/quota",
    tag = "Admin",
    responses(
        (status = 200, description = "Media quota summary",
            body = serde_json::Value,
            example = json!({
                "total_size": 1048576,
                "total_count": 120,
                "default_size_limit": 10000000000_i64,
                "default_count_limit": 100
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_media_quota_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/media` — List media uploaded by one user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/media",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User media listing",
            body = serde_json::Value,
            example = json!({
                "media": [{
                    "media_id": "abcdef",
                    "media_type": "image/png",
                    "upload_name": "avatar.png",
                    "created_ts": 1718000000000_i64,
                    "media_length": 2048
                }],
                "total": 1
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_media_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}/media` — Delete all media uploaded by one user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}/media",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User media deleted",
            body = serde_json::Value,
            example = json!({
                "deleted": 3
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_user_media_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/{user_id}/shadow_ban` — Enable shadow-ban mode for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/{user_id}/shadow_ban",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Shadow-ban enabled", body = serde_json::Value),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_shadow_ban_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}/shadow_ban` — Disable shadow-ban mode for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}/shadow_ban",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Shadow-ban disabled", body = serde_json::Value),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_unshadow_ban_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/rate_limit` — Read the per-user rate limit.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/rate_limit",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Per-user rate limit",
            body = serde_json::Value,
            example = json!({
                "messages_per_second": 5.0,
                "burst_count": 10
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_user_rate_limit_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_synapse/admin/v1/users/{user_id}/rate_limit` — Update the per-user rate limit.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_synapse/admin/v1/users/{user_id}/rate_limit",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Per-user rate limit updated", body = serde_json::Value),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_set_user_rate_limit_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}/rate_limit` — Remove the per-user rate limit override.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}/rate_limit",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Per-user rate limit deleted", body = serde_json::Value),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_user_rate_limit_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/admin/v1/users/{user_id}/override_ratelimit` — Read the legacy override rate-limit view for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/admin/v1/users/{user_id}/override_ratelimit",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Override rate-limit view",
            body = serde_json::Value,
            example = json!({
                "messages_per_second": 5.0,
                "burst_count": 10
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_override_rate_limit_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/admin/v1/users/{user_id}/override_ratelimit` — Update the legacy override rate-limit view for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/admin/v1/users/{user_id}/override_ratelimit",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Override rate-limit updated", body = serde_json::Value),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_set_override_rate_limit_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_synapse/admin/v1/users/{user_id}/override_ratelimit` — Remove the legacy override rate-limit view for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_synapse/admin/v1/users/{user_id}/override_ratelimit",
    tag = "Admin",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "Override rate-limit deleted", body = serde_json::Value),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn admin_delete_override_rate_limit_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/joined_rooms` — List rooms the user is joined to.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/joined_rooms",
    tag = "Client-Server",
    responses(
        (status = 200, description = "List of joined rooms",
            body = serde_json::Value,
            example = json!({
                "joined_rooms": ["!room1:example.com", "!room2:example.com"]
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_joined_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/publicRooms` — Get the public room directory.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/publicRooms",
    tag = "Client-Server",
    params(
        ("limit" = Option<i32>, Query, description = "Maximum number of rooms to return"),
        ("since" = Option<String>, Query, description = "Pagination token")
    ),
    responses(
        (status = 200, description = "Public room directory",
            body = serde_json::Value,
            example = json!({
                "chunk": [
                    {
                        "room_id": "!room:example.com",
                        "name": "Public Room",
                        "num_joined_members": 100,
                        "world_readable": true,
                        "guest_can_join": false
                    }
                ],
                "next_batch": "next_token",
                "total_room_count_estimate": 10
            })
        )
    ),
)]
pub fn get_public_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/register` — Register a new account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/register",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Registration successful",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "access_token": "access_token_value",
                "device_id": "ABCDEFGHIJ"
            })
        ),
        (status = 401, description = "Interactive authentication required"),
        (status = 400, description = "Invalid request or username taken")
    ),
)]
pub fn register_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/login` — Log in to an account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/login",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Login successful",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "access_token": "access_token_value",
                "device_id": "ABCDEFGHIJ"
            })
        ),
        (status = 401, description = "Invalid credentials"),
        (status = 403, description = "Account deactivated")
    ),
)]
pub fn login_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/logout` — Log out of a session.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/logout",
    tag = "Authentication",
    responses(
        (status = 200, description = "Logout successful", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn logout_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/refresh` — Refresh an access token.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/refresh",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Token refreshed",
            body = serde_json::Value,
            example = json!({
                "access_token": "new_access_token",
                "refresh_token": "new_refresh_token",
                "expires_in_ms": 3600000
            })
        ),
        (status = 401, description = "Invalid or expired refresh token")
    ),
)]
pub fn refresh_token_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/createRoom` — Create a new room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/createRoom",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Room created",
            body = serde_json::Value,
            example = json!({
                "room_id": "!abc:example.com"
            })
        ),
        (status = 400, description = "Invalid parameters")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn create_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}` — Send a message event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/rooms/{room_id}/send/{event_type}/{txn_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "The room ID"),
        ("event_type" = String, Path, description = "The type of event (e.g. m.room.message)"),
        ("txn_id" = String, Path, description = "Client-generated transaction ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Event sent",
            body = serde_json::Value,
            example = json!({
                "event_id": "$event:example.com"
            })
        ),
        (status = 403, description = "Not in room"),
        (status = 400, description = "Invalid event content")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn send_message_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/{room_id}/join` — Join a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/{room_id}/join",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "The room ID or alias")
    ),
    request_body = Option<serde_json::Value>,
    responses(
        (status = 200, description = "Joined successfully",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com"
            })
        ),
        (status = 403, description = "Banned or join rules restricted"),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn join_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/{room_id}/leave` — Leave a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/{room_id}/leave",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "The room ID")
    ),
    responses(
        (status = 200, description = "Left room", body = serde_json::Value),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn leave_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/{room_id}/forget` — Forget a room after leaving.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/{room_id}/forget",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "The room ID")
    ),
    responses(
        (status = 200, description = "Room forgotten",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "is_forgotten": true,
                "updated_ts": 1718000000000_i64
            })
        ),
        (status = 404, description = "Room not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn forget_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/{room_id}/invite` — Invite a user to a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/{room_id}/invite",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "The room ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "User invited",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "invited_user_id": "@bob:example.com",
                "invited_ts": 1718000000000_i64
            })
        ),
        (status = 400, description = "Missing or invalid user_id"),
        (status = 403, description = "Caller cannot invite into this room")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn invite_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/joined_members` — List joined members with profile snippets.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/joined_members",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "The room ID")
    ),
    responses(
        (status = 200, description = "Joined members",
            body = serde_json::Value,
            example = json!({
                "joined": {
                    "@alice:example.com": {
                        "display_name": "Alice",
                        "avatar_url": "mxc://example.com/alice"
                    },
                    "@bob:example.com": {
                        "display_name": "Bob",
                        "avatar_url": null
                    }
                }
            })
        ),
        (status = 403, description = "Not allowed to view joined members")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_joined_members_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/profile/{user_id}/displayname` — Set user display name.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/profile/{user_id}/displayname",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "The user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Display name updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_displayname_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/register` — Get supported registration flows.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/register",
    tag = "Authentication",
    responses(
        (status = 200, description = "Supported registration stages",
            body = serde_json::Value,
            example = json!({
                "flows": [
                    {"type": "m.login.dummy"},
                    {"type": "m.login.password"}
                ],
                "params": {}
            })
        )
    )
)]
pub fn get_register_flows_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/register/available` — Check username availability.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/register/available",
    tag = "Authentication",
    params(
        ("username" = String, Query, description = "Desired localpart")
    ),
    responses(
        (status = 200, description = "Availability result",
            body = serde_json::Value,
            example = json!({
                "available": true,
                "username": "alice"
            })
        ),
        (status = 400, description = "Invalid username")
    )
)]
pub fn check_username_availability_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/login` — Get supported login flows.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/login",
    tag = "Authentication",
    responses(
        (status = 200, description = "Supported login flows",
            body = serde_json::Value,
            example = json!({
                "flows": [
                    {"type": "m.login.password"},
                    {"type": "m.login.token"}
                ]
            })
        )
    )
)]
pub fn get_login_flows_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/logout/all` — Log out every device for the caller.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/logout/all",
    tag = "Authentication",
    responses(
        (status = 200, description = "All sessions revoked", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn logout_all_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/account/password` — Change the current password.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/account/password",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Password changed", body = serde_json::Value),
        (status = 401, description = "Authentication failed"),
        (status = 400, description = "Invalid password payload")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn change_password_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/account/deactivate` — Deactivate the current account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/account/deactivate",
    tag = "Authentication",
    request_body = Option<serde_json::Value>,
    responses(
        (status = 200, description = "Account deactivated",
            body = serde_json::Value,
            example = json!({
                "id_server_unbind_result": "no-support"
            })
        ),
        (status = 401, description = "Authentication failed")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn deactivate_account_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/account/3pid` — Add a third-party identifier.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/account/3pid",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Third-party identifier added", body = serde_json::Value),
        (status = 400, description = "Invalid 3PID payload")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn add_threepid_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/account/3pid/delete` — Remove a third-party identifier.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/account/3pid/delete",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Third-party identifier removed",
            body = serde_json::Value,
            example = json!({
                "id_server_unbind_result": "success"
            })
        ),
        (status = 400, description = "Invalid request")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_threepid_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/account/3pid/unbind` — Unbind a third-party identifier from an identity server.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/account/3pid/unbind",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Third-party identifier unbound",
            body = serde_json::Value,
            example = json!({
                "id_server_unbind_result": "success"
            })
        ),
        (status = 400, description = "Invalid request")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn unbind_threepid_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/user_directory/profiles/{user_id}` — Fetch a visible user profile snapshot.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/user_directory/profiles/{user_id}",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Target user ID")
    ),
    responses(
        (status = 200, description = "User directory profile",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "displayname": "Alice",
                "avatar_url": "mxc://example.com/alice"
            })
        ),
        (status = 404, description = "User not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_user_directory_profile_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/user_directory/search` — Search discoverable users.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/user_directory/search",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Search results",
            body = serde_json::Value,
            example = json!({
                "limited": false,
                "results": [
                    {
                        "user_id": "@alice:example.com",
                        "display_name": "Alice",
                        "avatar_url": "mxc://example.com/alice"
                    }
                ]
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn search_user_directory_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/directory/list/room/{room_id}` — Read room directory visibility.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/directory/list/room/{room_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID")
    ),
    responses(
        (status = 200, description = "Visibility value",
            body = serde_json::Value,
            example = json!({
                "visibility": "public"
            })
        ),
        (status = 404, description = "Room not found")
    )
)]
pub fn get_room_visibility_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/directory/list/room/{room_id}` — Update room directory visibility.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/directory/list/room/{room_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Visibility updated",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "visibility": "private"
            })
        ),
        (status = 403, description = "Caller cannot update visibility")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_room_visibility_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/directory/room/{room_alias}` — Resolve a room alias.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/directory/room/{room_alias}",
    tag = "Client-Server",
    params(
        ("room_alias" = String, Path, description = "Room alias, e.g. #room:example.com")
    ),
    responses(
        (status = 200, description = "Alias target room",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com"
            })
        ),
        (status = 404, description = "Alias not found")
    )
)]
pub fn get_room_by_alias_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/directory/room/{room_alias}` — Create or replace a room alias mapping.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/directory/room/{room_alias}",
    tag = "Client-Server",
    params(
        ("room_alias" = String, Path, description = "Room alias to create")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Alias created",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "alias": "#room:example.com",
                "created_ts": 1718000000000_i64
            })
        ),
        (status = 403, description = "Caller cannot manage aliases")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_room_alias_direct_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v3/directory/room/{room_alias}` — Delete a room alias mapping.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v3/directory/room/{room_alias}",
    tag = "Client-Server",
    params(
        ("room_alias" = String, Path, description = "Room alias to delete")
    ),
    responses(
        (status = 200, description = "Alias removed",
            body = serde_json::Value,
            example = json!({
                "removed": true,
                "alias": "#room:example.com"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_room_alias_direct_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/publicRooms` — Query the public room directory with a request body.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/publicRooms",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Public room directory",
            body = serde_json::Value,
            example = json!({
                "chunk": [
                    {
                        "room_id": "!room:example.com",
                        "name": "Public Room",
                        "num_joined_members": 42,
                        "world_readable": true,
                        "guest_can_join": true
                    }
                ],
                "total_room_count_estimate": 1,
                "next_batch": null
            })
        )
    )
)]
pub fn query_public_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/directory/room/{room_id}/alias` — List aliases bound to a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/directory/room/{room_id}/alias",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID")
    ),
    responses(
        (status = 200, description = "Aliases for the room",
            body = serde_json::Value,
            example = json!({
                "aliases": ["#room:example.com"]
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_room_aliases_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}` — Add an alias to a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("room_alias" = String, Path, description = "Alias to add")
    ),
    responses(
        (status = 200, description = "Alias created",
            body = serde_json::Value,
            example = json!({
                "room_id": "!room:example.com",
                "alias": "#room:example.com",
                "created_ts": 1718000000000_i64
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_room_alias_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}` — Remove a room alias.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("room_alias" = String, Path, description = "Alias to remove")
    ),
    responses(
        (status = 200, description = "Alias removed", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_room_alias_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/sync` — Perform an incremental or initial sync.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/sync",
    tag = "Client-Server",
    params(
        ("since" = Option<String>, Query, description = "Opaque sync token"),
        ("timeout" = Option<u64>, Query, description = "Long-poll timeout in milliseconds"),
        ("filter" = Option<String>, Query, description = "Filter ID or inline JSON filter"),
        ("full_state" = Option<bool>, Query, description = "Whether to force a full-state sync"),
        ("set_presence" = Option<String>, Query, description = "Presence override")
    ),
    responses(
        (status = 200, description = "Sync response", body = serde_json::Value),
        (status = 429, description = "Rate limited")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn sync_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/events` — Poll event updates using the legacy events stream.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/events",
    tag = "Client-Server",
    params(
        ("from" = Option<String>, Query, description = "Pagination token"),
        ("timeout" = Option<u64>, Query, description = "Long-poll timeout in milliseconds")
    ),
    responses(
        (status = 200, description = "Event stream chunk", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_events_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/my_rooms` — Return the caller's room list summary.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/my_rooms",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Rooms visible to the caller",
            body = serde_json::Value,
            example = json!({
                "rooms": [
                    {
                        "room_id": "!room:example.com",
                        "name": "General"
                    }
                ],
                "total": 1
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_my_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/search` — Search room events and users.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/search",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Search response", body = serde_json::Value),
        (status = 400, description = "Invalid search request")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn search_room_events_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/search_recipients` — Search DM recipients.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/search_recipients",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Recipient search response", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn search_recipients_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/search_rooms` — Search room metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/search_rooms",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Room search response", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn search_rooms_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/context/{event_id}` — Fetch event context around a target event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/context/{event_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room ID"),
        ("event_id" = String, Path, description = "Anchor event ID"),
        ("limit" = Option<i64>, Query, description = "Context event limit")
    ),
    responses(
        (status = 200, description = "Context events around the anchor", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_event_context_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/hierarchy` — Return hierarchy information for a space.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/hierarchy",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Space room ID"),
        ("from" = Option<String>, Query, description = "Pagination token"),
        ("limit" = Option<u32>, Query, description = "Maximum number of children"),
        ("max_depth" = Option<u32>, Query, description = "Maximum traversal depth"),
        ("suggested_only" = Option<bool>, Query, description = "Return suggested rooms only")
    ),
    responses(
        (status = 200, description = "Space hierarchy", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_room_hierarchy_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/rooms/{room_id}/timestamp_to_event` — Resolve the closest event for a timestamp.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/rooms/{room_id}/timestamp_to_event",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room ID"),
        ("ts" = i64, Query, description = "Target timestamp in milliseconds"),
        ("dir" = String, Query, description = "Search direction, such as `f` or `b`")
    ),
    responses(
        (status = 200, description = "Closest event",
            body = serde_json::Value,
            example = json!({
                "event_id": "$event:example.com",
                "origin_server_ts": 1718000000000_i64
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn timestamp_to_event_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/media/v3/upload` — Upload media content.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/media/v3/upload",
    tag = "Client-Server",
    params(
        ("filename" = Option<String>, Query, description = "Original filename"),
        ("content_type" = Option<String>, Query, description = "Declared media content type")
    ),
    request_body(content = String, content_type = "application/octet-stream", description = "Raw media bytes"),
    responses(
        (status = 200, description = "Media uploaded",
            body = serde_json::Value,
            example = json!({
                "content_uri": "mxc://example.com/abcdef"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn upload_media_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/media/v3/download/{server_name}/{media_id}` — Download media content.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/media/v3/download/{server_name}/{media_id}",
    tag = "Client-Server",
    params(
        ("server_name" = String, Path, description = "Owning homeserver name"),
        ("media_id" = String, Path, description = "Opaque media ID")
    ),
    responses(
        (status = 200, description = "Raw media bytes"),
        (status = 404, description = "Media not found")
    )
)]
pub fn download_media_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/media/v3/thumbnail/{server_name}/{media_id}` — Fetch a media thumbnail.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/media/v3/thumbnail/{server_name}/{media_id}",
    tag = "Client-Server",
    params(
        ("server_name" = String, Path, description = "Owning homeserver name"),
        ("media_id" = String, Path, description = "Opaque media ID"),
        ("width" = Option<u32>, Query, description = "Desired width"),
        ("height" = Option<u32>, Query, description = "Desired height"),
        ("method" = Option<String>, Query, description = "Thumbnail method, e.g. `crop` or `scale`")
    ),
    responses(
        (status = 200, description = "Thumbnail bytes"),
        (status = 404, description = "Media not found")
    )
)]
pub fn get_thumbnail_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/media/v3/preview_url` — Generate or fetch a URL preview.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/media/v3/preview_url",
    tag = "Client-Server",
    params(
        ("url" = String, Query, description = "Absolute URL to preview"),
        ("ts" = Option<i64>, Query, description = "Optional preview timestamp override")
    ),
    responses(
        (status = 200, description = "Preview metadata", body = serde_json::Value),
        (status = 400, description = "Invalid URL")
    )
)]
pub fn preview_url_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/{room_id}/report` — Report a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/{room_id}/report",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room to report")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Room report accepted", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn report_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/{room_id}/report/{event_id}` — Report a specific event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/{room_id}/report/{event_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room containing the event"),
        ("event_id" = String, Path, description = "Reported event ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Event report accepted", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn report_event_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/rooms/{room_id}/report/{event_id}/score` — Update a report score.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/rooms/{room_id}/report/{event_id}/score",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room containing the event"),
        ("event_id" = String, Path, description = "Reported event ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Report score updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_report_score_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/relations/{event_id}` — List all relations for an event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/relations/{event_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room containing the event"),
        ("event_id" = String, Path, description = "Target event ID"),
        ("limit" = Option<i64>, Query, description = "Maximum relations to return"),
        ("from" = Option<String>, Query, description = "Pagination token"),
        ("dir" = Option<String>, Query, description = "Direction")
    ),
    responses(
        (status = 200, description = "Relations response", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_relations_by_event_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/relations/{event_id}/{rel_type}` — List relations of one type.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/relations/{event_id}/{rel_type}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room containing the event"),
        ("event_id" = String, Path, description = "Target event ID"),
        ("rel_type" = String, Path, description = "Relation type"),
        ("limit" = Option<i64>, Query, description = "Maximum relations to return"),
        ("from" = Option<String>, Query, description = "Pagination token"),
        ("dir" = Option<String>, Query, description = "Direction")
    ),
    responses(
        (status = 200, description = "Relations response", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_relations_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/rooms/{room_id}/relations/{event_id}/{rel_type}/{txn_id}` — Send a related event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/rooms/{room_id}/relations/{event_id}/{rel_type}/{txn_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room containing the target event"),
        ("event_id" = String, Path, description = "Target event ID"),
        ("rel_type" = String, Path, description = "Relation type"),
        ("txn_id" = String, Path, description = "Client transaction ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Relation event created",
            body = serde_json::Value,
            example = json!({
                "event_id": "$relation:example.com",
                "room_id": "!room:example.com"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn send_relation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/aggregations/{event_id}/{rel_type}` — Aggregate relation counts.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/aggregations/{event_id}/{rel_type}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Room containing the event"),
        ("event_id" = String, Path, description = "Target event ID"),
        ("rel_type" = String, Path, description = "Relation type"),
        ("limit" = Option<i64>, Query, description = "Maximum groups to return"),
        ("from" = Option<String>, Query, description = "Pagination token"),
        ("dir" = Option<String>, Query, description = "Direction")
    ),
    responses(
        (status = 200, description = "Aggregation response", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_aggregations_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/rooms/{room_id}/send/m.reaction/{txn_id}` — Send an `m.reaction` event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/rooms/{room_id}/send/m.reaction/{txn_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("txn_id" = String, Path, description = "Client transaction ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Reaction event created",
            body = serde_json::Value,
            example = json!({
                "event_id": "$reaction:example.com"
            })
        ),
        (status = 400, description = "Invalid relation payload")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn add_reaction_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/config/client` — Return homeserver client configuration.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/config/client",
    tag = "Health",
    responses(
        (status = 200, description = "Client configuration",
            body = serde_json::Value,
            example = json!({
                "homeserver": {
                    "base_url": "https://example.com",
                    "server_name": "example.com"
                },
                "identity_server": {
                    "base_url": "https://example.com"
                }
            })
        )
    )
)]
pub fn get_client_config_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/register/guest` — Register a guest account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/register/guest",
    tag = "Authentication",
    responses(
        (status = 200, description = "Guest registration successful",
            body = serde_json::Value,
            example = json!({
                "access_token": "guest_access_token",
                "device_id": "GUESTDEVICE",
                "user_id": "@guest-1:example.com",
                "expires_in": 3600000
            })
        ),
        (status = 403, description = "Guest registration disabled")
    )
)]
pub fn register_guest_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/account/guest` — Return information about the current guest account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/account/guest",
    tag = "Authentication",
    responses(
        (status = 200, description = "Guest account information",
            body = serde_json::Value,
            example = json!({
                "user_id": "@guest-1:example.com",
                "is_guest": true
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_guest_info_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/account/guest/upgrade` — Upgrade a guest account into a regular account.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/account/guest/upgrade",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Guest account upgraded",
            body = serde_json::Value,
            example = json!({
                "success": true,
                "user_id": "@guest-1:example.com",
                "access_token": "new_access_token"
            })
        ),
        (status = 400, description = "Invalid upgrade payload")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn upgrade_guest_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/login/get_qr_code` — Generate a QR login challenge for the current user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/login/get_qr_code",
    tag = "Authentication",
    responses(
        (status = 200, description = "QR login challenge",
            body = serde_json::Value,
            example = json!({
                "transaction_id": "qr_xxx",
                "mode": "login",
                "challenge": "uuid-challenge",
                "expires_in": 300
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_qr_code_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/login/qr/confirm` — Confirm a QR login on the source device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/login/qr/confirm",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "QR login confirmed",
            body = serde_json::Value,
            example = json!({
                "transaction_id": "qr_xxx",
                "status": "confirmed"
            })
        ),
        (status = 404, description = "Transaction not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn confirm_qr_login_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/login/qr/start` — Start QR login on the scanning device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/login/qr/start",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "QR login started",
            body = serde_json::Value,
            example = json!({
                "transaction_id": "qr_xxx",
                "user_id": "@alice:example.com",
                "device_id": "DEVICEID",
                "initial_display_name": "Alice iPhone",
                "status": "pending_confirmation"
            })
        ),
        (status = 404, description = "Transaction not found")
    )
)]
pub fn start_qr_login_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/login/qr/{transaction_id}/status` — Fetch QR login transaction status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/login/qr/{transaction_id}/status",
    tag = "Authentication",
    params(
        ("transaction_id" = String, Path, description = "QR login transaction ID")
    ),
    responses(
        (status = 200, description = "QR login status",
            body = serde_json::Value,
            example = json!({
                "transaction_id": "qr_xxx",
                "user_id": "@alice:example.com",
                "status": "pending"
            })
        ),
        (status = 404, description = "Transaction not found")
    )
)]
pub fn get_qr_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/login/qr/invalidate` — Cancel a QR login transaction.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/login/qr/invalidate",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "QR login invalidated",
            body = serde_json::Value,
            example = json!({
                "transaction_id": "qr_xxx",
                "status": "invalidated"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn invalidate_qr_login_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/login/qrcode/new` — Frontend compatibility alias for QR login generation.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/login/qrcode/new",
    tag = "Authentication",
    responses(
        (status = 200, description = "QR login challenge", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_qrcode_new_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/login/qrcode/{session_id}` — Frontend compatibility alias for QR login status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/login/qrcode/{session_id}",
    tag = "Authentication",
    params(
        ("session_id" = String, Path, description = "QR login session ID")
    ),
    responses(
        (status = 200, description = "QR login status", body = serde_json::Value)
    )
)]
pub fn get_qrcode_status_alias_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/thirdparty/protocols` — List supported third-party protocols.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/thirdparty/protocols",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Third-party protocol registry", body = serde_json::Value)
    )
)]
pub fn get_thirdparty_protocols_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/thirdparty/protocol/{protocol}` — Get metadata for one third-party protocol.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/thirdparty/protocol/{protocol}",
    tag = "Client-Server",
    params(
        ("protocol" = String, Path, description = "Third-party protocol identifier")
    ),
    responses(
        (status = 200, description = "Third-party protocol metadata",
            body = serde_json::Value,
            example = json!({
                "instances": [],
                "user_fields": [],
                "location_fields": []
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_thirdparty_protocol_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/thirdparty/location/{protocol}` — Search bridged locations via a protocol adapter.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/thirdparty/location/{protocol}",
    tag = "Client-Server",
    params(
        ("protocol" = String, Path, description = "Third-party protocol identifier"),
        ("alias" = Option<String>, Query, description = "Alias to look up"),
        ("search" = Option<String>, Query, description = "Search term"),
        ("server" = Option<String>, Query, description = "Third-party server"),
        ("channel" = Option<String>, Query, description = "Third-party channel")
    ),
    responses(
        (status = 200, description = "Location search result", body = serde_json::Value),
        (status = 404, description = "No bridge configured")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_thirdparty_location_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/thirdparty/user/{protocol}` — Search bridged users via a protocol adapter.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/thirdparty/user/{protocol}",
    tag = "Client-Server",
    params(
        ("protocol" = String, Path, description = "Third-party protocol identifier"),
        ("userid" = Option<String>, Query, description = "Third-party user identifier"),
        ("search" = Option<String>, Query, description = "Search term"),
        ("nickname" = Option<String>, Query, description = "Nickname"),
        ("server" = Option<String>, Query, description = "Third-party server")
    ),
    responses(
        (status = 200, description = "User search result", body = serde_json::Value),
        (status = 404, description = "No bridge configured")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_thirdparty_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/thirdparty/location` — Search bridged locations by query only.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/thirdparty/location",
    tag = "Client-Server",
    params(
        ("alias" = Option<String>, Query, description = "Alias to look up"),
        ("search" = Option<String>, Query, description = "Search term"),
        ("server" = Option<String>, Query, description = "Third-party server"),
        ("channel" = Option<String>, Query, description = "Third-party channel")
    ),
    responses(
        (status = 200, description = "Location search result", body = serde_json::Value),
        (status = 404, description = "No bridge configured")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_thirdparty_location_by_alias_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/thirdparty/user` — Search bridged users by query only.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/thirdparty/user",
    tag = "Client-Server",
    params(
        ("userid" = Option<String>, Query, description = "Third-party user identifier"),
        ("search" = Option<String>, Query, description = "Search term"),
        ("nickname" = Option<String>, Query, description = "Nickname"),
        ("server" = Option<String>, Query, description = "Third-party server")
    ),
    responses(
        (status = 200, description = "User search result", body = serde_json::Value),
        (status = 404, description = "No bridge configured")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_thirdparty_user_by_id_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/actions` — Update push rule actions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/actions",
    tag = "Client-Server",
    params(
        ("scope" = String, Path, description = "Push rule scope"),
        ("kind" = String, Path, description = "Push rule kind"),
        ("rule_id" = String, Path, description = "Push rule identifier")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Push rule actions updated",
            body = serde_json::Value,
            example = json!({
                "rule_id": ".m.rule.message",
                "actions": ["notify"],
                "updated_ts": 1718000000000_i64
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_push_rule_actions_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled` — Read push rule enabled state.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled",
    tag = "Client-Server",
    params(
        ("scope" = String, Path, description = "Push rule scope"),
        ("kind" = String, Path, description = "Push rule kind"),
        ("rule_id" = String, Path, description = "Push rule identifier")
    ),
    responses(
        (status = 200, description = "Push rule enabled flag",
            body = serde_json::Value,
            example = json!({
                "enabled": true
            })
        ),
        (status = 404, description = "Push rule not found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_push_rule_enabled_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled` — Update push rule enabled state.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}/enabled",
    tag = "Client-Server",
    params(
        ("scope" = String, Path, description = "Push rule scope"),
        ("kind" = String, Path, description = "Push rule kind"),
        ("rule_id" = String, Path, description = "Push rule identifier")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Push rule enabled flag updated",
            body = serde_json::Value,
            example = json!({
                "rule_id": ".m.rule.message",
                "enabled": true,
                "updated_ts": 1718000000000_i64
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_push_rule_enabled_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushrules/` — Default push rule collection entrypoint.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushrules/",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Default push rules", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_push_rules_default_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/pushrules/global/` — Default global push rule collection entrypoint.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/pushrules/global/",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Default global push rules", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_push_rules_global_default_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/pushrules/` — Default push rule collection entrypoint on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/pushrules/",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Default push rules", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_push_rules_default_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/pushrules/global/` — Default global push rule collection entrypoint on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/pushrules/global/",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Default global push rules", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_push_rules_global_default_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/presence/{user_id}/status` — Read presence status using the v1 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/presence/{user_id}/status",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Target user ID")
    ),
    responses(
        (status = 200, description = "Presence status", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_presence_status_v1_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/presence/{user_id}/status` — Update presence status using the v1 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/presence/{user_id}/status",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Caller user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Presence updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_presence_status_v1_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/presence/{user_id}/status` — Read presence status using the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/presence/{user_id}/status",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Target user ID")
    ),
    responses(
        (status = 200, description = "Presence status", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_presence_status_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/r0/presence/{user_id}/status` — Update presence status using the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/r0/presence/{user_id}/status",
    tag = "Client-Server",
    params(
        ("user_id" = String, Path, description = "Caller user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Presence updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_presence_status_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/typing` — List currently typing users in a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/typing",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID")
    ),
    responses(
        (status = 200, description = "Typing users",
            body = serde_json::Value,
            example = json!({
                "typing": ["@alice:example.com", "@bob:example.com"]
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_typing_users_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/typing/{user_id}` — Read a single user's typing state.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/typing/{user_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("user_id" = String, Path, description = "Target user ID")
    ),
    responses(
        (status = 200, description = "Typing flag",
            body = serde_json::Value,
            example = json!({
                "typing": true
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_user_typing_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/rooms/{room_id}/typing/{user_id}` — Set a user's typing state.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/rooms/{room_id}/typing/{user_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("user_id" = String, Path, description = "Caller user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Typing state updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_typing_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/typing` — Bulk fetch typing state for multiple rooms.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/typing",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Per-room typing states",
            body = serde_json::Value,
            example = json!({
                "!room:example.com": {
                    "typing": ["@alice:example.com"]
                }
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn bulk_get_typing_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/rooms/{room_id}/typing` — List currently typing users in a room on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/rooms/{room_id}/typing",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID")
    ),
    responses(
        (status = 200, description = "Typing users",
            body = serde_json::Value,
            example = json!({
                "typing": ["@alice:example.com", "@bob:example.com"]
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_typing_users_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/rooms/{room_id}/typing/{user_id}` — Read a single user's typing state on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/rooms/{room_id}/typing/{user_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("user_id" = String, Path, description = "Target user ID")
    ),
    responses(
        (status = 200, description = "Typing flag",
            body = serde_json::Value,
            example = json!({
                "typing": true
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_user_typing_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/r0/rooms/{room_id}/typing/{user_id}` — Set a user's typing state on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/r0/rooms/{room_id}/typing/{user_id}",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("user_id" = String, Path, description = "Caller user ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Typing state updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_typing_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/rooms/typing` — Bulk fetch typing state for multiple rooms on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/rooms/typing",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Per-room typing states",
            body = serde_json::Value,
            example = json!({
                "!room:example.com": {
                    "typing": ["@alice:example.com"]
                }
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn bulk_get_typing_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/rendezvous` — Create a rendezvous session.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/rendezvous",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Rendezvous session created",
            body = serde_json::Value,
            example = json!({
                "url": "matrix://rendezvous/example.com/session123",
                "session_id": "session123",
                "key": "secret"
            })
        ),
        (status = 400, description = "Invalid rendezvous request")
    )
)]
pub fn create_rendezvous_session_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/rendezvous/{session_id}` — Fetch rendezvous session metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/rendezvous/{session_id}",
    tag = "Authentication",
    params(
        ("session_id" = String, Path, description = "Rendezvous session ID"),
        ("x-matrix-rendezvous-key" = Option<String>, Header, description = "Session access key")
    ),
    responses(
        (status = 200, description = "Rendezvous session",
            body = serde_json::Value,
            example = json!({
                "session_id": "session123",
                "intent": "login.start",
                "transport": "http.v1",
                "status": "pending",
                "created_ts": 1718000000000_i64,
                "expires_at": 1718000300000_i64
            })
        ),
        (status = 404, description = "Session not found")
    )
)]
pub fn get_rendezvous_session_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/rendezvous/{session_id}` — Update rendezvous session state.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/rendezvous/{session_id}",
    tag = "Authentication",
    params(
        ("session_id" = String, Path, description = "Rendezvous session ID"),
        ("x-matrix-rendezvous-key" = Option<String>, Header, description = "Session access key")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Rendezvous session updated", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_rendezvous_session_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v1/rendezvous/{session_id}` — Delete a rendezvous session.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v1/rendezvous/{session_id}",
    tag = "Authentication",
    params(
        ("session_id" = String, Path, description = "Rendezvous session ID"),
        ("x-matrix-rendezvous-key" = Option<String>, Header, description = "Session access key")
    ),
    responses(
        (status = 200, description = "Rendezvous session deleted", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_rendezvous_session_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/rendezvous/{session_id}/messages` — Send a rendezvous message.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/rendezvous/{session_id}/messages",
    tag = "Authentication",
    params(
        ("session_id" = String, Path, description = "Rendezvous session ID"),
        ("x-matrix-rendezvous-key" = Option<String>, Header, description = "Session access key")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Rendezvous message stored",
            body = serde_json::Value,
            example = json!({
                "session_id": "session123",
                "message_id": "session123_1718000000000",
                "sent_ts": 1718000000000_i64
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn send_rendezvous_message_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/rendezvous/{session_id}/messages` — Fetch rendezvous messages.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/rendezvous/{session_id}/messages",
    tag = "Authentication",
    params(
        ("session_id" = String, Path, description = "Rendezvous session ID"),
        ("x-matrix-rendezvous-key" = Option<String>, Header, description = "Session access key")
    ),
    responses(
        (status = 200, description = "Rendezvous messages", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_rendezvous_messages_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/push/devices` — List registered push devices.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/push/devices",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Registered push devices", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_push_devices_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/push/devices` — Register a push device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/push/devices",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Push device registered", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn register_push_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/r0/push/devices/{device_id}` — Unregister a push device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/r0/push/devices/{device_id}",
    tag = "Client-Server",
    params(
        ("device_id" = String, Path, description = "Push device identifier")
    ),
    responses(
        (status = 200, description = "Push device unregistered", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn unregister_push_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/push/send` — Queue a push notification for delivery.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/push/send",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Push notification queued",
            body = serde_json::Value,
            example = json!({
                "message": "Notification queued"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn send_push_notification_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/push/rules` — List custom push notification rules.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/push/rules",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Push notification rules", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_push_notification_rules_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/push/rules` — Create a custom push notification rule.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/push/rules",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Push notification rule created", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn create_push_notification_rule_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/r0/push/rules/{scope}/{kind}/{rule_id}` — Delete a custom push notification rule.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/r0/push/rules/{scope}/{kind}/{rule_id}",
    tag = "Client-Server",
    params(
        ("scope" = String, Path, description = "Push rule scope"),
        ("kind" = String, Path, description = "Push rule kind"),
        ("rule_id" = String, Path, description = "Push rule identifier")
    ),
    responses(
        (status = 200, description = "Push notification rule deleted", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_push_notification_rule_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/register/captcha/send` — Request a captcha challenge.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/register/captcha/send",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Captcha issued",
            body = serde_json::Value,
            example = json!({
                "captcha_id": "captcha-123",
                "expires_in": 300,
                "captcha_type": "email"
            })
        )
    )
)]
pub fn send_captcha_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/register/captcha/verify` — Verify a captcha response.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/register/captcha/verify",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Captcha verification result",
            body = serde_json::Value,
            example = json!({
                "verified": true
            })
        )
    )
)]
pub fn verify_captcha_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/register/captcha/status` — Query captcha state.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/register/captcha/status",
    tag = "Authentication",
    params(
        ("captcha_id" = String, Query, description = "Captcha identifier")
    ),
    responses(
        (status = 200, description = "Captcha status", body = serde_json::Value),
        (status = 404, description = "Captcha not found")
    )
)]
pub fn get_captcha_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/register/captcha/send` — Request a captcha challenge on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/register/captcha/send",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Captcha issued",
            body = serde_json::Value,
            example = json!({
                "captcha_id": "captcha-123",
                "expires_in": 300,
                "captcha_type": "email"
            })
        )
    )
)]
pub fn send_captcha_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/register/captcha/verify` — Verify a captcha response on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/register/captcha/verify",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Captcha verification result",
            body = serde_json::Value,
            example = json!({
                "verified": true
            })
        )
    )
)]
pub fn verify_captcha_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/register/captcha/status` — Query captcha state on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/register/captcha/status",
    tag = "Authentication",
    params(
        ("captcha_id" = String, Query, description = "Captcha identifier")
    ),
    responses(
        (status = 200, description = "Captcha status", body = serde_json::Value),
        (status = 404, description = "Captcha not found")
    )
)]
pub fn get_captcha_status_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/thirdparty/location` — Query third-party locations without specifying a protocol.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/thirdparty/location",
    tag = "Client-Server",
    params(
        ("alias" = Option<String>, Query, description = "Third-party room alias"),
        ("search" = Option<String>, Query, description = "Free-text search string"),
        ("server" = Option<String>, Query, description = "Remote server hint"),
        ("channel" = Option<String>, Query, description = "Third-party channel identifier")
    ),
    responses(
        (status = 200, description = "Third-party locations", body = serde_json::Value),
        (status = 400, description = "No location bridge configured")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_thirdparty_location_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/thirdparty/user` — Query third-party users without specifying a protocol.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/thirdparty/user",
    tag = "Client-Server",
    params(
        ("userid" = Option<String>, Query, description = "Third-party user identifier"),
        ("search" = Option<String>, Query, description = "Free-text search string"),
        ("nickname" = Option<String>, Query, description = "Third-party nickname"),
        ("server" = Option<String>, Query, description = "Remote server hint")
    ),
    responses(
        (status = 200, description = "Third-party users", body = serde_json::Value),
        (status = 400, description = "No user bridge configured")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_thirdparty_user_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/login/sso/redirect` — Redirect to the configured SSO provider.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/login/sso/redirect",
    tag = "Authentication",
    params(
        ("redirectUrl" = Option<String>, Query, description = "Preferred redirect URL after SSO login"),
        ("redirect_url" = Option<String>, Query, description = "Compatibility alias for redirectUrl")
    ),
    responses(
        (status = 307, description = "Temporary redirect to the SSO provider"),
        (status = 400, description = "SSO is not enabled")
    )
)]
pub fn login_sso_redirect_v3_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/login/sso/redirect` — Redirect to the configured SSO provider on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/login/sso/redirect",
    tag = "Authentication",
    params(
        ("redirectUrl" = Option<String>, Query, description = "Preferred redirect URL after SSO login"),
        ("redirect_url" = Option<String>, Query, description = "Compatibility alias for redirectUrl")
    ),
    responses(
        (status = 307, description = "Temporary redirect to the SSO provider"),
        (status = 400, description = "SSO is not enabled")
    )
)]
pub fn login_sso_redirect_r0_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/login/sso/userinfo` — Read the authenticated user's SSO profile.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/login/sso/userinfo",
    tag = "Authentication",
    responses(
        (status = 200, description = "OIDC-compatible userinfo",
            body = serde_json::Value,
            example = json!({
                "sub": "@alice:example.com",
                "name": "Alice",
                "picture": "mxc://example.com/avatar",
                "email": "alice@example.com"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn login_sso_userinfo_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/login/sso/userinfo` — Read the authenticated user's SSO profile on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/login/sso/userinfo",
    tag = "Authentication",
    responses(
        (status = 200, description = "OIDC-compatible userinfo",
            body = serde_json::Value,
            example = json!({
                "sub": "@alice:example.com",
                "name": "Alice",
                "picture": "mxc://example.com/avatar",
                "email": "alice@example.com"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn login_sso_userinfo_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/login/sso/redirect/cas` — Redirect to the configured CAS entrypoint.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/login/sso/redirect/cas",
    tag = "Authentication",
    params(
        ("redirect_after" = Option<String>, Query, description = "Where to continue after CAS login")
    ),
    responses(
        (status = 302, description = "Redirect to CAS login"),
        (status = 400, description = "CAS SSO is not configured")
    )
)]
pub fn login_sso_redirect_cas_v3_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/login/sso/redirect/cas` — Redirect to the configured CAS entrypoint on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/login/sso/redirect/cas",
    tag = "Authentication",
    params(
        ("redirect_after" = Option<String>, Query, description = "Where to continue after CAS login")
    ),
    responses(
        (status = 302, description = "Redirect to CAS login"),
        (status = 400, description = "CAS SSO is not configured")
    )
)]
pub fn login_sso_redirect_cas_r0_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/login/sso/redirect/saml` — Redirect to the configured SAML IdP.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/login/sso/redirect/saml",
    tag = "Authentication",
    params(
        ("redirect_url" = Option<String>, Query, description = "Optional post-login redirect URL")
    ),
    responses(
        (status = 307, description = "Temporary redirect to the SAML IdP"),
        (status = 400, description = "SAML authentication is not enabled")
    )
)]
pub fn login_sso_redirect_saml_v3_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/login/sso/redirect/saml` — Redirect to the configured SAML IdP on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/login/sso/redirect/saml",
    tag = "Authentication",
    params(
        ("redirect_url" = Option<String>, Query, description = "Optional post-login redirect URL")
    ),
    responses(
        (status = 307, description = "Temporary redirect to the SAML IdP"),
        (status = 400, description = "SAML authentication is not enabled")
    )
)]
pub fn login_sso_redirect_saml_r0_get_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/login/sso/redirect/saml` — Request a SAML login redirect URL as JSON.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/login/sso/redirect/saml",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "SAML redirect URL",
            body = serde_json::Value,
            example = json!({
                "redirect_url": "https://idp.example.com/sso?SAMLRequest=..."
            })
        ),
        (status = 400, description = "SAML authentication is not enabled")
    )
)]
pub fn login_sso_redirect_saml_r0_post_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/login/saml/callback` — Handle a browser-based SAML login callback.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/login/saml/callback",
    tag = "Authentication",
    params(
        ("saml_response" = Option<String>, Query, description = "SAML response payload"),
        ("saml_request" = Option<String>, Query, description = "SAML request payload"),
        ("relay_state" = Option<String>, Query, description = "SAML relay state")
    ),
    responses(
        (status = 200, description = "Completed SAML login",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "access_token": "syt_abcdef",
                "device_id": "SAML_DEVICE",
                "expires_in": 3600,
                "refresh_token": "syr_refresh"
            })
        ),
        (status = 400, description = "Missing or invalid SAML response")
    )
)]
pub fn login_saml_callback_v3_get_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/login/saml/callback` — Handle a posted SAML login callback.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/login/saml/callback",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Completed SAML login",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "access_token": "syt_abcdef",
                "device_id": "SAML_DEVICE",
                "expires_in": 3600,
                "refresh_token": "syr_refresh"
            })
        ),
        (status = 400, description = "Missing or invalid SAML response")
    )
)]
pub fn login_saml_callback_v3_post_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/login/saml/callback` — Handle a browser-based SAML login callback on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/login/saml/callback",
    tag = "Authentication",
    params(
        ("saml_response" = Option<String>, Query, description = "SAML response payload"),
        ("saml_request" = Option<String>, Query, description = "SAML request payload"),
        ("relay_state" = Option<String>, Query, description = "SAML relay state")
    ),
    responses(
        (status = 200, description = "Completed SAML login",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "access_token": "syt_abcdef",
                "device_id": "SAML_DEVICE",
                "expires_in": 3600,
                "refresh_token": "syr_refresh"
            })
        ),
        (status = 400, description = "Missing or invalid SAML response")
    )
)]
pub fn login_saml_callback_r0_get_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/login/saml/callback` — Handle a posted SAML login callback on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/login/saml/callback",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Completed SAML login",
            body = serde_json::Value,
            example = json!({
                "user_id": "@alice:example.com",
                "access_token": "syt_abcdef",
                "device_id": "SAML_DEVICE",
                "expires_in": 3600,
                "refresh_token": "syr_refresh"
            })
        ),
        (status = 400, description = "Missing or invalid SAML response")
    )
)]
pub fn login_saml_callback_r0_post_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/logout/saml` — Initiate SAML single logout for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/logout/saml",
    tag = "Authentication",
    responses(
        (status = 200, description = "SAML logout status",
            body = serde_json::Value,
            example = json!({
                "redirect_url": "https://idp.example.com/slo?SAMLRequest=..."
            })
        ),
        (status = 400, description = "SAML authentication is not enabled")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn logout_saml_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/logout/saml/callback` — Complete SAML single logout.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/logout/saml/callback",
    tag = "Authentication",
    params(
        ("saml_response" = Option<String>, Query, description = "SAML logout response"),
        ("relay_state" = Option<String>, Query, description = "SAML relay state")
    ),
    responses(
        (status = 200, description = "Logout completed",
            body = serde_json::Value,
            example = json!({
                "message": "Logout successful"
            })
        ),
        (status = 400, description = "SAML authentication is not enabled or response is missing")
    )
)]
pub fn logout_saml_callback_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/oidc/userinfo` — Read OpenID Connect-compatible user information.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/oidc/userinfo",
    tag = "Authentication",
    responses(
        (status = 200, description = "OIDC userinfo",
            body = serde_json::Value,
            example = json!({
                "sub": "@alice:example.com",
                "name": "Alice",
                "picture": "mxc://example.com/avatar",
                "email": "alice@example.com"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn oidc_userinfo_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/oidc/token` — Exchange an authorization code or refresh token for a Matrix-compatible token response.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/oidc/token",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "OIDC token response",
            body = serde_json::Value,
            example = json!({
                "access_token": "syt_abcdef",
                "token_type": "Bearer",
                "expires_in": 3600,
                "refresh_token": "syr_refresh",
                "scope": "openid profile email",
                "matrix_user_id": "@alice:example.com",
                "device_id": "OIDC1234"
            })
        ),
        (status = 400, description = "OIDC is not enabled or request validation failed")
    )
)]
pub fn oidc_token_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/oidc/logout` — Revoke OIDC session state for the authenticated user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/oidc/logout",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "OIDC logout completed",
            body = serde_json::Value,
            example = json!({
                "success": true
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn oidc_logout_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/oidc/authorize` — Build an upstream OIDC authorization request.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/oidc/authorize",
    tag = "Authentication",
    params(
        ("response_type" = String, Query, description = "Must be `code`"),
        ("client_id" = String, Query, description = "OIDC client identifier"),
        ("redirect_uri" = String, Query, description = "OIDC callback URL"),
        ("scope" = Option<String>, Query, description = "Requested scopes"),
        ("state" = Option<String>, Query, description = "Optional client-supplied state"),
        ("nonce" = Option<String>, Query, description = "Optional OIDC nonce")
    ),
    responses(
        (status = 200, description = "Authorization request details",
            body = serde_json::Value,
            example = json!({
                "authorization_url": "https://idp.example.com/authorize?...",
                "state": "state123",
                "nonce": "nonce123",
                "code_verifier": "pkce-verifier"
            })
        ),
        (status = 400, description = "OIDC is not enabled or response_type is unsupported")
    )
)]
pub fn oidc_authorize_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/oidc/register` — Dynamic client registration compatibility endpoint.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/oidc/register",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 400, description = "Dynamic client registration is not supported")
    )
)]
pub fn oidc_register_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/oidc/callback` — Consume the upstream OIDC authorization callback.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/oidc/callback",
    tag = "Authentication",
    params(
        ("code" = Option<String>, Query, description = "Authorization code"),
        ("state" = Option<String>, Query, description = "Opaque state value"),
        ("error" = Option<String>, Query, description = "OIDC provider error"),
        ("error_description" = Option<String>, Query, description = "OIDC provider error details")
    ),
    responses(
        (status = 200, description = "Completed OIDC login",
            body = serde_json::Value,
            example = json!({
                "access_token": "syt_abcdef",
                "refresh_token": "syr_refresh",
                "expires_in": 3600,
                "device_id": "OIDC1234",
                "user_id": "@alice:example.com"
            })
        ),
        (status = 400, description = "OIDC callback parameters are invalid"),
        (status = 401, description = "OIDC state is missing, expired, or invalid")
    )
)]
pub fn oidc_callback_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/oidc/userinfo` — Read OpenID Connect-compatible user information on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/oidc/userinfo",
    tag = "Authentication",
    responses(
        (status = 200, description = "OIDC userinfo",
            body = serde_json::Value,
            example = json!({
                "sub": "@alice:example.com",
                "name": "Alice",
                "picture": "mxc://example.com/avatar",
                "email": "alice@example.com"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn oidc_userinfo_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/oidc/token` — Exchange an authorization code or refresh token on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/oidc/token",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "OIDC token response",
            body = serde_json::Value,
            example = json!({
                "access_token": "syt_abcdef",
                "token_type": "Bearer",
                "expires_in": 3600,
                "refresh_token": "syr_refresh",
                "scope": "openid profile email",
                "matrix_user_id": "@alice:example.com",
                "device_id": "OIDC1234"
            })
        ),
        (status = 400, description = "OIDC is not enabled or request validation failed")
    )
)]
pub fn oidc_token_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/oidc/logout` — Revoke OIDC session state on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/oidc/logout",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "OIDC logout completed",
            body = serde_json::Value,
            example = json!({
                "success": true
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn oidc_logout_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/oidc/authorize` — Build an upstream OIDC authorization request on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/oidc/authorize",
    tag = "Authentication",
    params(
        ("response_type" = String, Query, description = "Must be `code`"),
        ("client_id" = String, Query, description = "OIDC client identifier"),
        ("redirect_uri" = String, Query, description = "OIDC callback URL"),
        ("scope" = Option<String>, Query, description = "Requested scopes"),
        ("state" = Option<String>, Query, description = "Optional client-supplied state"),
        ("nonce" = Option<String>, Query, description = "Optional OIDC nonce")
    ),
    responses(
        (status = 200, description = "Authorization request details",
            body = serde_json::Value,
            example = json!({
                "authorization_url": "https://idp.example.com/authorize?...",
                "state": "state123",
                "nonce": "nonce123",
                "code_verifier": "pkce-verifier"
            })
        ),
        (status = 400, description = "OIDC is not enabled or response_type is unsupported")
    )
)]
pub fn oidc_authorize_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/oidc/register` — Dynamic client registration compatibility endpoint on the r0 path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/oidc/register",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 400, description = "Dynamic client registration is not supported")
    )
)]
pub fn oidc_register_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/oidc/callback` — Consume the upstream OIDC authorization callback on the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/oidc/callback",
    tag = "Authentication",
    params(
        ("code" = Option<String>, Query, description = "Authorization code"),
        ("state" = Option<String>, Query, description = "Opaque state value"),
        ("error" = Option<String>, Query, description = "OIDC provider error"),
        ("error_description" = Option<String>, Query, description = "OIDC provider error details")
    ),
    responses(
        (status = 200, description = "Completed OIDC login",
            body = serde_json::Value,
            example = json!({
                "access_token": "syt_abcdef",
                "refresh_token": "syr_refresh",
                "expires_in": 3600,
                "device_id": "OIDC1234",
                "user_id": "@alice:example.com"
            })
        ),
        (status = 400, description = "OIDC callback parameters are invalid"),
        (status = 401, description = "OIDC state is missing, expired, or invalid")
    )
)]
pub fn oidc_callback_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/oidc/login` — Built-in OIDC provider login helper.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/oidc/login",
    tag = "Authentication",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Authorization code issued by the built-in OIDC provider",
            body = serde_json::Value,
            example = json!({
                "code": "oidc-auth-code"
            })
        ),
        (status = 400, description = "Built-in OIDC provider is not enabled"),
        (status = 401, description = "Authorization failed")
    )
)]
pub fn oidc_login_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/app/v1/ping` — Verify application service access token validity.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/app/v1/ping",
    tag = "Application Service",
    responses(
        (status = 200, description = "Application service identity",
            body = serde_json::Value,
            example = json!({
                "as_id": "bridge_example"
            })
        ),
        (status = 401, description = "Missing or invalid bearer token")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn appservice_ping_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/app/v1/transactions/{as_id}/{txn_id}` — Receive a transaction for an application service.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/app/v1/transactions/{as_id}/{txn_id}",
    tag = "Application Service",
    params(
        ("as_id" = String, Path, description = "Application service ID"),
        ("txn_id" = String, Path, description = "Transaction ID")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Transaction accepted", body = serde_json::Value),
        (status = 401, description = "Missing or invalid bearer token"),
        (status = 403, description = "Application service ID mismatch")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn appservice_transactions_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/app/v1/users/{user_id}` — Query whether a user is in the application service namespace.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/app/v1/users/{user_id}",
    tag = "Application Service",
    params(
        ("user_id" = String, Path, description = "Matrix user ID")
    ),
    responses(
        (status = 200, description = "User exists in the namespace", body = serde_json::Value),
        (status = 401, description = "Missing or invalid bearer token"),
        (status = 403, description = "User not in application service namespace")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn appservice_user_query_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/app/v1/rooms/{alias}` — Query whether a room alias is in the application service namespace.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/app/v1/rooms/{alias}",
    tag = "Application Service",
    params(
        ("alias" = String, Path, description = "Room alias")
    ),
    responses(
        (status = 200, description = "Alias exists in the namespace", body = serde_json::Value),
        (status = 401, description = "Missing or invalid bearer token"),
        (status = 403, description = "Room alias not in application service namespace")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn appservice_room_alias_query_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/app/v1/{as_id}` — Query application service metadata by ID.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/app/v1/{as_id}",
    tag = "Application Service",
    params(
        ("as_id" = String, Path, description = "Application service ID")
    ),
    responses(
        (status = 200, description = "Application service metadata",
            body = serde_json::Value,
            example = json!({
                "id": "bridge_example",
                "url": "https://bridge.example.com",
                "sender": "@bridgebot:example.com",
                "description": "Example bridge",
                "is_enabled": true,
                "protocols": ["irc"]
            })
        ),
        (status = 404, description = "Application service not found")
    )
)]
pub fn appservice_query_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/keys/rotation/status` — Read key rotation status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/keys/rotation/status",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Key rotation status",
            body = serde_json::Value,
            example = json!({
                "enabled": true,
                "status": {
                    "rotation_enabled": true
                },
                "user_last_rotation": 1718000000000_i64
            })
        ),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_status_get_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/keys/rotation/status` — POST compatibility variant for reading key rotation status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/keys/rotation/status",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Key rotation status", body = serde_json::Value),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_status_post_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/keys/rotation/rotate` — Force a key rotation.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/keys/rotation/rotate",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Rotation result",
            body = serde_json::Value,
            example = json!({
                "success": true,
                "message": "Keys rotated successfully",
                "has_new_key": true
            })
        ),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_rotate_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/keys/rotation/history/{device_id}` — Read key rotation history for a device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/keys/rotation/history/{device_id}",
    tag = "Client-Server",
    params(
        ("device_id" = String, Path, description = "Target device ID")
    ),
    responses(
        (status = 200, description = "Rotation history",
            body = serde_json::Value,
            example = json!({
                "device_id": "DEVICE123",
                "rotations": [
                    {
                        "key_id": "ed25519:1",
                        "rotated_ts": 1718000000000_i64
                    }
                ]
            })
        ),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_history_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/keys/rotation/revoke` — Revoke an old key.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/keys/rotation/revoke",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Revocation result",
            body = serde_json::Value,
            example = json!({
                "success": true,
                "revoked": 1,
                "message": "Successfully revoked key ed25519:1"
            })
        ),
        (status = 400, description = "key_id is required"),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_revoke_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/keys/rotation/config` — Update key rotation configuration.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/keys/rotation/config",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Persisted key rotation configuration",
            body = serde_json::Value,
            example = json!({
                "enabled": true,
                "interval_ms": 86400000_i64,
                "rotation_interval_days": 7,
                "rotation_threshold_days": 1,
                "grace_period_minutes": 30
            })
        ),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_config_put_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/keys/rotation/config` — POST compatibility variant for updating key rotation configuration.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/keys/rotation/config",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Persisted key rotation configuration", body = serde_json::Value),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_config_post_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/keys/rotation/check` — Check whether key rotation is needed.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/keys/rotation/check",
    tag = "Client-Server",
    params(
        ("key_id" = Option<String>, Query, description = "Optional key ID to check")
    ),
    responses(
        (status = 200, description = "Rotation requirement",
            body = serde_json::Value,
            example = json!({
                "needs_rotation": true,
                "last_rotation": 1718000000000_i64,
                "interval_ms": 86400000
            })
        ),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_check_get_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/keys/rotation/check` — POST compatibility variant for checking whether key rotation is needed.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/keys/rotation/check",
    tag = "Client-Server",
    params(
        ("key_id" = Option<String>, Query, description = "Optional key ID to check")
    ),
    responses(
        (status = 200, description = "Rotation requirement", body = serde_json::Value),
        (status = 403, description = "Admin privileges required")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn key_rotation_check_post_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/saml/metadata` — Fetch SAML IdP metadata using the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/saml/metadata",
    tag = "Authentication",
    responses(
        (status = 200, description = "SAML IdP metadata",
            body = serde_json::Value,
            example = json!({
                "entity_id": "https://idp.example.com/metadata",
                "sso_url": "https://idp.example.com/sso",
                "slo_url": "https://idp.example.com/slo",
                "certificate": "MIIC..."
            })
        ),
        (status = 400, description = "SAML authentication is not enabled")
    )
)]
pub fn get_saml_metadata_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/saml/sp_metadata` — Fetch generated SP metadata using the r0 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/saml/sp_metadata",
    tag = "Authentication",
    responses(
        (status = 200, description = "SAML SP metadata XML", body = String),
        (status = 400, description = "SAML authentication is not enabled")
    )
)]
pub fn get_sp_metadata_r0_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/saml/metadata` — Fetch SAML IdP metadata using the v3 path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/saml/metadata",
    tag = "Authentication",
    responses(
        (status = 200, description = "SAML IdP metadata",
            body = serde_json::Value,
            example = json!({
                "entity_id": "https://idp.example.com/metadata",
                "sso_url": "https://idp.example.com/sso",
                "slo_url": "https://idp.example.com/slo",
                "certificate": "MIIC..."
            })
        ),
        (status = 400, description = "SAML authentication is not enabled")
    )
)]
pub fn get_saml_metadata_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/saml/sp_metadata` — Fetch generated SP metadata using the v3 path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/saml/sp_metadata",
    tag = "Authentication",
    responses(
        (status = 200, description = "SAML SP metadata XML", body = String),
        (status = 400, description = "SAML authentication is not enabled")
    )
)]
pub fn get_sp_metadata_v3_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v3/register/captcha/clean` — Clean expired captchas.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v3/register/captcha/clean",
    tag = "Authentication",
    responses(
        (status = 200, description = "Expired captchas cleaned", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn cleanup_captcha_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/unstable/org.matrix.msc2965/auth_metadata` — Return OAuth/OIDC auth metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/unstable/org.matrix.msc2965/auth_metadata",
    tag = "Authentication",
    responses(
        (status = 200, description = "OIDC discovery metadata", body = serde_json::Value),
        (status = 404, description = "OIDC not configured")
    )
)]
pub fn get_auth_metadata_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device` — Fetch the caller's dehydrated device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Dehydrated device", body = serde_json::Value),
        (status = 404, description = "No dehydrated device")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_dehydrated_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device` — Upload or replace a dehydrated device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Dehydrated device stored",
            body = serde_json::Value,
            example = json!({
                "device_id": "DEHYDRATED1"
            })
        ),
        (status = 403, description = "Cross-signing or secret storage prerequisites not met")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn put_dehydrated_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device` — Delete the caller's dehydrated device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device",
    tag = "Client-Server",
    responses(
        (status = 200, description = "Dehydrated device deleted", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_dehydrated_device_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/{device_id}/events` — Claim queued to-device events for a dehydrated device.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/{device_id}/events",
    tag = "Client-Server",
    params(
        ("device_id" = String, Path, description = "Dehydrated device identifier")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Queued to-device events", body = serde_json::Value)
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn post_dehydrated_device_events_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/versions` — Compatibility alias for supported Matrix protocol versions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/versions",
    tag = "Health",
    responses(
        (status = 200, description = "Supported Matrix protocol versions",
            body = serde_json::Value,
            example = json!({
                "versions": ["v1.1", "v1.2", "v1.3", "v1.4", "v1.5", "v1.6", "v1.7", "v1.8", "v1.9", "v1.10", "v1.11", "v1.12", "v1.13"],
                "unstable_features": {}
            })
        )
    )
)]
pub fn get_client_versions_v3_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/version` — Compatibility alias for server version metadata.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/version",
    tag = "Health",
    responses(
        (status = 200, description = "Server implementation version",
            body = serde_json::Value,
            example = json!({
                "server": {
                    "name": "synapse-rust",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })
        )
    )
)]
pub fn get_server_version_r0_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/sync` — Sliding sync entrypoint exposed on the v1 compatibility path.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/sync",
    tag = "Client-Server",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Sliding sync response", body = serde_json::Value),
        (status = 429, description = "Rate limited")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn sliding_sync_v1_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/static/client/login/` — Browser login fallback page.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/static/client/login/",
    tag = "Authentication",
    responses(
        (status = 200, description = "HTML login fallback page", body = String)
    )
)]
pub fn login_fallback_page_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/ephemeral` — Fetch ephemeral room events.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/ephemeral",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("limit" = Option<i64>, Query, description = "Maximum number of ephemeral events to return")
    ),
    responses(
        (status = 200, description = "Ephemeral room events",
            body = serde_json::Value,
            example = json!({
                "chunk": [
                    {
                        "type": "m.typing",
                        "content": {
                            "user_ids": ["@alice:example.com"]
                        }
                    }
                ],
                "start": null,
                "end": null
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_ephemeral_events_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/rooms/{room_id}/replies/{event_id}/redact` — Redact a thread reply.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/rooms/{room_id}/replies/{event_id}/redact",
    tag = "Client-Server",
    params(
        ("room_id" = String, Path, description = "Target room ID"),
        ("event_id" = String, Path, description = "Reply event ID to redact")
    ),
    request_body = Option<serde_json::Value>,
    responses(
        (status = 200, description = "Reply redacted",
            body = serde_json::Value,
            example = json!({
                "redacted": true,
                "event_id": "$reply:example.com"
            })
        )
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn redact_thread_reply_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/version` — Get the server version.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/version",
    tag = "Federation",
    responses(
        (status = 200, description = "Success",
            body = serde_json::Value,
            example = json!({
                "server": {
                    "name": "synapse-rust",
                    "version": "0.1.0"
                }
            })
        )
    )
)]
pub fn get_federation_version_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1` — Get server capabilities and discovery information.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1",
    tag = "Federation",
    responses(
        (status = 200, description = "Success",
            body = serde_json::Value,
            example = json!({
                "version": "0.1.0",
                "server_name": "example.com",
                "capabilities": {
                    "m.change_password": true,
                    "m.room_versions": {
                        "default": "10",
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
                            "11": "stable",
                            "12": "stable",
                            "13": "stable"
                        }
                    }
                }
            })
        )
    )
)]
pub fn get_federation_discovery_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/publicRooms` — Get the public room list from this server.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/publicRooms",
    tag = "Federation",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of rooms to return"),
        ("since" = Option<String>, Query, description = "Pagination token")
    ),
    responses(
        (status = 200, description = "Public rooms list", body = serde_json::Value)
    )
)]
pub fn get_public_rooms_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/query/destination` — Query if a destination is reachable.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/query/destination",
    tag = "Federation",
    params(
        ("server_name" = String, Query, description = "The server name to query")
    ),
    responses(
        (status = 200, description = "Success")
    )
)]
pub fn query_destination_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/openid/userinfo` — Exchange an OpenID token for user information.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/openid/userinfo",
    tag = "Federation",
    params(
        ("access_token" = String, Query, description = "The OpenID access token")
    ),
    responses(
        (status = 200, description = "User information",
            body = serde_json::Value,
            example = json!({ "sub": "@alice:example.com" })
        ),
        (status = 401, description = "Invalid or expired token")
    )
)]
pub fn openid_userinfo_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/members/{room_id}` — Get the full membership list for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/members/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room")
    ),
    responses(
        (status = 200, description = "Membership list", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_room_members_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/members/{room_id}/joined` — Get the joined membership list for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/members/{room_id}/joined",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room")
    ),
    responses(
        (status = 200, description = "Joined membership list", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_joined_room_members_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/user/devices/{user_id}` — Get the device list for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/user/devices/{user_id}",
    tag = "Federation",
    params(
        ("user_id" = String, Path, description = "The ID of the user")
    ),
    responses(
        (status = 200, description = "Device list", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_user_devices_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/room_auth/{room_id}` — Get the auth chain for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/federation/v1/room_auth/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("auth_event_ids" = Vec<String>, Query, description = "The event IDs to get auth for")
    ),
    responses(
        (status = 200, description = "Auth chain", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_room_auth_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v1/knock/{room_id}/{user_id}` — Submit a knock request to a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v1/knock/{room_id}/{user_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("user_id" = String, Path, description = "The ID of the user knocking")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Knock accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn knock_room_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v1/thirdparty/invite` — Submit a third-party invite.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v1/thirdparty/invite",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Invite accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn thirdparty_invite_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/get_joining_rules/{room_id}` — Get the joining rules for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/federation/v1/get_joining_rules/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room")
    ),
    responses(
        (status = 200, description = "Joining rules", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_joining_rules_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v2/invite/{room_id}/{event_id}` — Submit an invite (v2).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v2/invite/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the invite event")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Invite accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn invite_v2_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v1/send/{txn_id}` — Submit a transaction of events.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v1/send/{txn_id}",
    tag = "Federation",
    params(
        ("txn_id" = String, Path, description = "The ID of the transaction")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Transaction accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn send_transaction_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/make_join/{room_id}/{user_id}` — Request to join a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/make_join/{room_id}/{user_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("user_id" = String, Path, description = "The ID of the user joining"),
        ("ver" = Option<Vec<String>>, Query, description = "Supported room versions")
    ),
    responses(
        (status = 200, description = "Join template", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not Found")
    )
)]
pub fn make_join_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/make_leave/{room_id}/{user_id}` — Request to leave a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/make_leave/{room_id}/{user_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("user_id" = String, Path, description = "The ID of the user leaving")
    ),
    responses(
        (status = 200, description = "Leave template", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not Found")
    )
)]
pub fn make_leave_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v1/send_join/{room_id}/{event_id}` — Submit a join event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v1/send_join/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the join event")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Join accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn send_join_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v1/send_leave/{room_id}/{event_id}` — Submit a leave event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v1/send_leave/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the leave event")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Leave accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn send_leave_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v1/invite/{room_id}/{event_id}` — Submit an invite event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v1/invite/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the invite event")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Invite accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn invite_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v1/get_missing_events/{room_id}` — Get missing events for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v1/get_missing_events/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Missing events", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_missing_events_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/room/{room_id}/{event_id}` — Get a specific event from a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/room/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the event")
    ),
    responses(
        (status = 200, description = "Event data", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not Found")
    )
)]
pub fn get_room_event_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/timestamp_to_event/{room_id}` — Get the event closest to a timestamp.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/timestamp_to_event/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("ts" = i64, Query, description = "The timestamp in milliseconds"),
        ("dir" = String, Query, description = "Direction to search: f or b")
    ),
    responses(
        (status = 200, description = "Event ID", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not Found")
    )
)]
pub fn timestamp_to_event_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/get_event_auth/{room_id}/{event_id}` — Get the auth chain for an event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/get_event_auth/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the event")
    ),
    responses(
        (status = 200, description = "Auth chain", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_event_auth_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/query/auth` — Query auth for an event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/query/auth",
    tag = "Federation",
    params(
        ("room_id" = String, Query, description = "The ID of the room"),
        ("event_id" = String, Query, description = "The ID of the event")
    ),
    responses(
        (status = 200, description = "Auth response", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn query_auth_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_synapse/federation/v1/event_auth` — Get auth events for a list of events.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_synapse/federation/v1/event_auth",
    tag = "Federation",
    params(
        ("room_id" = String, Query, description = "The ID of the room"),
        ("event_ids" = Vec<String>, Query, description = "The event IDs to get auth for")
    ),
    responses(
        (status = 200, description = "Auth events", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn event_auth_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/state/{room_id}` — Get the state of a room at a given event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/state/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Query, description = "The ID of the event to get state at")
    ),
    responses(
        (status = 200, description = "Room state", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_state_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/event/{event_id}` — Get a specific event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/event/{event_id}",
    tag = "Federation",
    params(
        ("event_id" = String, Path, description = "The ID of the event")
    ),
    responses(
        (status = 200, description = "Event data", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not Found")
    )
)]
pub fn get_event_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/state_ids/{room_id}` — Get the state IDs of a room at a given event.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/state_ids/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Query, description = "The ID of the event to get state at")
    ),
    responses(
        (status = 200, description = "State IDs", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_state_ids_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/query/directory/room/{room_id}` — Query the room directory for a room ID.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/query/directory/room/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room")
    ),
    responses(
        (status = 200, description = "Room aliases", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn room_directory_query_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/query/profile` — Query a user's profile.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/query/profile",
    tag = "Federation",
    params(
        ("user_id" = String, Query, description = "The ID of the user"),
        ("field" = Option<String>, Query, description = "The profile field to query")
    ),
    responses(
        (status = 200, description = "Profile data", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn profile_query_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/query/profile/{user_id}` — Query a user's profile (legacy).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/query/profile/{user_id}",
    tag = "Federation",
    params(
        ("user_id" = String, Path, description = "The ID of the user")
    ),
    responses(
        (status = 200, description = "Profile data", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn profile_query_legacy_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/hierarchy/{room_id}` — Get the room hierarchy.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/hierarchy/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room")
    ),
    responses(
        (status = 200, description = "Room hierarchy", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn get_room_hierarchy_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/backfill/{room_id}` — Backfill events for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/backfill/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("v" = Vec<String>, Query, description = "The event IDs to backfill from"),
        ("limit" = i64, Query, description = "Maximum number of events to return")
    ),
    responses(
        (status = 200, description = "Backfilled events", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn backfill_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/federation/v1/keys/claim` — Claim one-time keys (legacy extension).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/federation/v1/keys/claim",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Claimed keys", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn legacy_keys_claim_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/federation/v1/keys/query` — Query device keys (legacy extension).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/federation/v1/keys/query",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Device keys", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn legacy_keys_query_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/federation/v1/keys/upload` — Upload device keys (legacy extension).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/federation/v1/keys/upload",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Success", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn keys_upload_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v1/user/keys/upload` — Upload device keys (user path).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v1/user/keys/upload",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Success", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn user_keys_upload_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v1/user/keys/claim` — Claim one-time keys.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v1/user/keys/claim",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Claimed keys", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn keys_claim_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v1/user/keys/query` — Query device keys.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v1/user/keys/query",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Device keys", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn keys_query_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v2/user/keys/query` — Query device keys (v2).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v2/user/keys/query",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Device keys", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn v2_keys_query_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v2/send_join/{room_id}/{event_id}` — Submit a join event (v2).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v2/send_join/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the join event")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Join accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn send_join_v2_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v2/send_leave/{room_id}/{event_id}` — Submit a leave event (v2).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v2/send_leave/{room_id}/{event_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the leave event")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Leave accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn send_leave_v2_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/federation/v1/publicRooms` — Query the public room list.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/federation/v1/publicRooms",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Public rooms list", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn post_public_rooms_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/query/directory` — Query the room directory for an alias.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/query/directory",
    tag = "Federation",
    params(
        ("room_alias" = String, Query, description = "The room alias to query")
    ),
    responses(
        (status = 200, description = "Room ID", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn query_directory_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/media/download/{server_name}/{media_id}` — Download media from a remote server.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/media/download/{server_name}/{media_id}",
    tag = "Federation",
    params(
        ("server_name" = String, Path, description = "The server name"),
        ("media_id" = String, Path, description = "The media ID")
    ),
    responses(
        (status = 200, description = "Media content"),
        (status = 404, description = "Not Found")
    )
)]
pub fn media_download_federation_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v1/media/thumbnail/{server_name}/{media_id}` — Get a thumbnail for media from a remote server.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v1/media/thumbnail/{server_name}/{media_id}",
    tag = "Federation",
    params(
        ("server_name" = String, Path, description = "The server name"),
        ("media_id" = String, Path, description = "The media ID"),
        ("width" = Option<i64>, Query, description = "Thumbnail width"),
        ("height" = Option<i64>, Query, description = "Thumbnail height"),
        ("method" = Option<String>, Query, description = "Thumbnailing method")
    ),
    responses(
        (status = 200, description = "Thumbnail content"),
        (status = 404, description = "Not Found")
    )
)]
pub fn media_thumbnail_federation_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/federation/v1/exchange_third_party_invite/{room_id}` — Exchange a third-party invite.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/federation/v1/exchange_third_party_invite/{room_id}",
    tag = "Federation",
    params(
        ("room_id" = String, Path, description = "The ID of the room")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Exchange accepted", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    )
)]
pub fn exchange_third_party_invite_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v2/server` — Get the server's signing keys (v2).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v2/server",
    tag = "Federation",
    responses(
        (status = 200, description = "Server keys", body = serde_json::Value)
    )
)]
pub fn server_key_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/key/v2/server` — Get the server's signing keys (v2, key path).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/key/v2/server",
    tag = "Federation",
    responses(
        (status = 200, description = "Server keys", body = serde_json::Value)
    )
)]
pub fn server_key_v2_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_synapse/federation/v2/key/clone` — Clone server keys (trusted extension).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_synapse/federation/v2/key/clone",
    tag = "Federation",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Success")
    )
)]
pub fn key_clone_federation_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/federation/v2/query/{server_name}/{key_id}` — Query server keys (v2).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/federation/v2/query/{server_name}/{key_id}",
    tag = "Federation",
    params(
        ("server_name" = String, Path, description = "The server name"),
        ("key_id" = String, Path, description = "The key ID")
    ),
    responses(
        (status = 200, description = "Server keys", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn key_query_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/key/v2/query/{server_name}/{key_id}` — Query server keys (v2, key path).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/key/v2/query/{server_name}/{key_id}",
    tag = "Federation",
    params(
        ("server_name" = String, Path, description = "The server name"),
        ("key_id" = String, Path, description = "The key ID")
    ),
    responses(
        (status = 200, description = "Server keys", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn key_query_v2_federation_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/unstable/org.matrix.msc3575/sync` — Sliding sync endpoint (MSC3575).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/unstable/org.matrix.msc3575/sync",
    tag = "Unstable MSC",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Sync response", body = serde_json::Value),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn sliding_sync_msc3575_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/unstable/org.matrix.simplified_msc3575/sync` — Simplified sliding sync endpoint.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/unstable/org.matrix.simplified_msc3575/sync",
    tag = "Unstable MSC",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Sync response", body = serde_json::Value),
        (status = 401, description = "Unauthorized")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn sliding_sync_simplified_msc3575_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}` — Get extended profile (MSC4133).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}",
    tag = "Unstable MSC",
    params(
        ("user_id" = String, Path, description = "The ID of the user")
    ),
    responses(
        (status = 200, description = "Profile document", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn get_extended_profile_msc4133_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}` — Get extended profile field.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}",
    tag = "Unstable MSC",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
        ("key_name" = String, Path, description = "The field name")
    ),
    responses(
        (status = 200, description = "Field value", body = serde_json::Value),
        (status = 404, description = "Not Found")
    )
)]
pub fn get_extended_profile_field_msc4133_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}` — Update extended profile field.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}",
    tag = "Unstable MSC",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
        ("key_name" = String, Path, description = "The field name")
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Updated", body = serde_json::Value),
        (status = 403, description = "Forbidden")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn put_extended_profile_field_msc4133_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}` — Delete extended profile field.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/unstable/uk.tcpip.msc4133/profile/{user_id}/{key_name}",
    tag = "Unstable MSC",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
        ("key_name" = String, Path, description = "The field name")
    ),
    responses(
        (status = 200, description = "Deleted", body = serde_json::Value),
        (status = 403, description = "Forbidden"),
        (status = 404, description = "Not Found")
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_extended_profile_field_msc4133_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

// =======================================
// Private Extensions (DM/Voice/Widget/Burn/Friends/ExternalServices)
// =======================================

/// `POST /_matrix/client/r0/create_dm` — Create a direct message room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/create_dm",
    tag = "Private Extension - DM",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "DM room created", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn create_dm_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/direct` — Get direct message rooms map.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/direct",
    tag = "Private Extension - DM",
    responses(
        (status = 200, description = "Direct rooms map", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_direct_map_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/r0/direct/{room_id}` — Update direct message room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/r0/direct/{room_id}",
    tag = "Private Extension - DM",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Updated", body = serde_json::Value),
        (status = 401, description = "Unauthorized"),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_direct_map_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/r0/voice/config` — Get voice configuration.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/r0/voice/config",
    tag = "Private Extension - Voice",
    responses(
        (status = 200, description = "Voice config", body = serde_json::Value),
    ),
)]
pub fn get_voice_config_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/r0/voice/upload` — Upload a voice message.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/r0/voice/upload",
    tag = "Private Extension - Voice",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Uploaded", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn upload_voice_message_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/voice/stats` — Get voice stats for current user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/voice/stats",
    tag = "Private Extension - Voice",
    responses(
        (status = 200, description = "Voice stats", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_voice_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/voice/room/{room_id}/stats` — Get voice stats for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/voice/room/{room_id}/stats",
    tag = "Private Extension - Voice",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
    ),
    responses(
        (status = 200, description = "Room voice stats", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_room_voice_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/voice/user/{user_id}/stats` — Get voice stats for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/voice/user/{user_id}/stats",
    tag = "Private Extension - Voice",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "User voice stats", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_user_voice_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/voice/room/{room_id}` — List voice messages in a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/voice/room/{room_id}",
    tag = "Private Extension - Voice",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("limit" = Option<i64>, Query, description = "Maximum messages to return"),
        ("from" = Option<i64>, Query, description = "Pagination offset"),
    ),
    responses(
        (status = 200, description = "Voice messages", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_room_voice_messages_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/voice/user/{user_id}` — List voice messages for a user.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/voice/user/{user_id}",
    tag = "Private Extension - Voice",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
        ("limit" = Option<i64>, Query, description = "Maximum messages to return"),
        ("from" = Option<i64>, Query, description = "Pagination offset"),
    ),
    responses(
        (status = 200, description = "Voice messages", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_user_voice_messages_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/voice/{media_id}` — Get a voice message by media ID.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/voice/{media_id}",
    tag = "Private Extension - Voice",
    params(
        ("media_id" = String, Path, description = "The media ID"),
    ),
    responses(
        (status = 200, description = "Voice content"),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_voice_message_content_doc() -> String {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/voice/{media_id}/convert` — Convert a voice message.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/voice/{media_id}/convert",
    tag = "Private Extension - Voice",
    params(
        ("media_id" = String, Path, description = "The media ID"),
    ),
    responses(
        (status = 200, description = "Converted", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn convert_voice_message_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/voice/{media_id}/optimize` — Optimize a voice message.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/voice/{media_id}/optimize",
    tag = "Private Extension - Voice",
    params(
        ("media_id" = String, Path, description = "The media ID"),
    ),
    responses(
        (status = 200, description = "Optimized", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn optimize_voice_message_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/voice/{media_id}/transcribe` — Transcribe a voice message.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/voice/{media_id}/transcribe",
    tag = "Private Extension - Voice",
    params(
        ("media_id" = String, Path, description = "The media ID"),
    ),
    responses(
        (status = 200, description = "Transcribed", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn transcribe_voice_message_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/widgets` — Create a widget.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/widgets",
    tag = "Private Extension - Widget",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Widget created", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn create_widget_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/widgets/{widget_id}` — Get a widget.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/widgets/{widget_id}",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    responses(
        (status = 200, description = "Widget details", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_widget_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/widgets/{widget_id}` — Update a widget.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/widgets/{widget_id}",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Widget updated", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_widget_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v1/widgets/{widget_id}` — Delete a widget.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v1/widgets/{widget_id}",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    responses(
        (status = 200, description = "Widget deleted", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_widget_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/widgets/{widget_id}/config` — Get widget configuration.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/widgets/{widget_id}/config",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    responses(
        (status = 200, description = "Widget config", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_widget_config_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/rooms/{room_id}/widgets` — Get room widgets.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/rooms/{room_id}/widgets",
    tag = "Private Extension - Widget",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
    ),
    responses(
        (status = 200, description = "Room widgets", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_room_widgets_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/rooms/{room_id}/widgets/jitsi/config` — Get Jitsi config.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/rooms/{room_id}/widgets/jitsi/config",
    tag = "Private Extension - Widget",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
    ),
    responses(
        (status = 200, description = "Jitsi config", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_jitsi_config_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/capabilities` — Get widget capabilities.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/capabilities",
    tag = "Private Extension - Widget",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    responses(
        (status = 200, description = "Widget capabilities", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_room_widget_capabilities_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/capabilities` — Set widget capabilities.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/capabilities",
    tag = "Private Extension - Widget",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Capabilities set", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_room_widget_capabilities_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/send` — Send a widget message.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/send",
    tag = "Private Extension - Widget",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Message sent", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn send_room_widget_message_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/widgets/{widget_id}/permissions` — Set widget permissions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/widgets/{widget_id}/permissions",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Permissions set", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_widget_permission_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/widgets/{widget_id}/permissions` — Get widget permissions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/widgets/{widget_id}/permissions",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    responses(
        (status = 200, description = "Permissions", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_widget_permissions_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v1/widgets/{widget_id}/permissions/{user_id}` — Delete widget permissions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v1/widgets/{widget_id}/permissions/{user_id}",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Deleted", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_widget_permission_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/widgets/{widget_id}/sessions` — Create a widget session.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/widgets/{widget_id}/sessions",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Session created", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn create_widget_session_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/widgets/{widget_id}/sessions` — Get widget sessions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/widgets/{widget_id}/sessions",
    tag = "Private Extension - Widget",
    params(
        ("widget_id" = String, Path, description = "The ID of the widget"),
    ),
    responses(
        (status = 200, description = "Session list", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_widget_sessions_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/widgets/sessions/{session_id}` — Get a widget session.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/widgets/sessions/{session_id}",
    tag = "Private Extension - Widget",
    params(
        ("session_id" = String, Path, description = "The ID of the session"),
    ),
    responses(
        (status = 200, description = "Session details", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_widget_session_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v1/widgets/sessions/{session_id}` — Terminate a widget session.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v1/widgets/sessions/{session_id}",
    tag = "Private Extension - Widget",
    params(
        ("session_id" = String, Path, description = "The ID of the session"),
    ),
    responses(
        (status = 200, description = "Session terminated", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn terminate_widget_session_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/admin/v1/external_services` — Register an external service.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/admin/v1/external_services",
    tag = "Private Extension - External Services",
    request_body = serde_json::Value,
    responses(
        (status = 201, description = "Registered", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn register_external_service_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/admin/v1/external_services` — List external services.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/admin/v1/external_services",
    tag = "Private Extension - External Services",
    params(
        ("service_type" = Option<String>, Query, description = "Filter by service type"),
    ),
    responses(
        (status = 200, description = "Service list", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn list_external_services_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/admin/v1/external_services/{as_id}/health` — Get external service health.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/admin/v1/external_services/{as_id}/health",
    tag = "Private Extension - External Services",
    params(
        ("as_id" = String, Path, description = "The ID of the service"),
    ),
    responses(
        (status = 200, description = "Health status", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_external_service_health_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/external_services/{as_id}/health` — Check external service health (client endpoint).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/external_services/{as_id}/health",
    tag = "Private Extension - External Services",
    params(
        ("as_id" = String, Path, description = "The ID of the service"),
    ),
    responses(
        (status = 200, description = "Health check", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn check_service_health_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/admin/v1/external_services/{as_id}` — Unregister an external service.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/admin/v1/external_services/{as_id}",
    tag = "Private Extension - External Services",
    params(
        ("as_id" = String, Path, description = "The ID of the service"),
    ),
    responses(
        (status = 200, description = "Unregistered", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn unregister_external_service_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/admin/v1/external_services/{as_id}` — Update an external service.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/admin/v1/external_services/{as_id}",
    tag = "Private Extension - External Services",
    params(
        ("as_id" = String, Path, description = "The ID of the service"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Updated", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_external_service_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/admin/v1/external_services/{as_id}` — Get an external service.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/admin/v1/external_services/{as_id}",
    tag = "Private Extension - External Services",
    params(
        ("as_id" = String, Path, description = "The ID of the service"),
    ),
    responses(
        (status = 200, description = "Service details", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_external_service_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/rooms/{room_id}/burn` — Enable burn after read in a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/rooms/{room_id}/burn",
    tag = "Private Extension - Burn",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Updated", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn enable_burn_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/rooms/{room_id}/burn` — Get burn settings for a room.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/rooms/{room_id}/burn",
    tag = "Private Extension - Burn",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
    ),
    responses(
        (status = 200, description = "Burn settings", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_burn_settings_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/rooms/{room_id}/burn/pending` — Get pending burn messages.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/rooms/{room_id}/burn/pending",
    tag = "Private Extension - Burn",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
    ),
    responses(
        (status = 200, description = "Pending burns", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_pending_burns_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/rooms/{room_id}/burn/{event_id}` — Mark message as read (trigger burn).
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/rooms/{room_id}/burn/{event_id}",
    tag = "Private Extension - Burn",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the event"),
    ),
    responses(
        (status = 200, description = "Marked as read", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn mark_burn_read_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v1/rooms/{room_id}/burn/{event_id}` — Cancel a pending burn.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v1/rooms/{room_id}/burn/{event_id}",
    tag = "Private Extension - Burn",
    params(
        ("room_id" = String, Path, description = "The ID of the room"),
        ("event_id" = String, Path, description = "The ID of the event"),
    ),
    responses(
        (status = 200, description = "Cancelled", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn cancel_burn_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/user/burn/config` — Set global burn config.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/user/burn/config",
    tag = "Private Extension - Burn",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Updated", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn set_global_burn_config_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/user/burn/stats` — Get user burn stats.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/user/burn/stats",
    tag = "Private Extension - Burn",
    responses(
        (status = 200, description = "Burn stats", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_burn_stats_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends` — Get friends list.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends",
    tag = "Private Extension - Friends",
    responses(
        (status = 200, description = "Friends list", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friends_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/friends` — Send a friend request.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/friends",
    tag = "Private Extension - Friends",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Request sent", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn send_friend_request_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/search` — Search the friend directory.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/search",
    tag = "Private Extension - Friends",
    responses(
        (status = 200, description = "Search results", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn search_friend_directory_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/requests/incoming` — Get incoming friend requests.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/requests/incoming",
    tag = "Private Extension - Friends",
    responses(
        (status = 200, description = "Incoming requests", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_incoming_requests_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/requests/outgoing` — Get outgoing friend requests.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/requests/outgoing",
    tag = "Private Extension - Friends",
    responses(
        (status = 200, description = "Outgoing requests", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_outgoing_requests_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/request/received` — Get received friend requests.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/request/received",
    tag = "Private Extension - Friends",
    responses(
        (status = 200, description = "Received requests", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_received_requests_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/friends/request/{user_id}/accept` — Accept a friend request.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/friends/request/{user_id}/accept",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Accepted", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn accept_friend_request_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/friends/request/{user_id}/reject` — Reject a friend request.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/friends/request/{user_id}/reject",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Rejected", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn reject_friend_request_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/friends/request/{user_id}/cancel` — Cancel a friend request.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/friends/request/{user_id}/cancel",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Cancelled", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn cancel_friend_request_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/check/{user_id}` — Check friendship status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/check/{user_id}",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Status", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn check_friendship_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/suggestions` — Get friend suggestions.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/suggestions",
    tag = "Private Extension - Friends",
    responses(
        (status = 200, description = "Suggestions", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friend_suggestions_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v1/friends/{user_id}` — Remove a friend.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v1/friends/{user_id}",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Removed", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn remove_friend_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/friends/{user_id}/note` — Update a friend note.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/friends/{user_id}/note",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Updated", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_friend_note_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/{user_id}/status` — Get a friend status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/{user_id}/status",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Status", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friend_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/friends/{user_id}/status` — Update a friend status.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/friends/{user_id}/status",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Updated", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_friend_status_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/{user_id}/info` — Get a friend info.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/{user_id}/info",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Info", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friend_info_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/friends/{user_id}/displayname` — Update a friend display name.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/friends/{user_id}/displayname",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Updated", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn update_friend_displayname_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/groups` — Get friend groups.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/groups",
    tag = "Private Extension - Friends",
    responses(
        (status = 200, description = "Groups list", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friend_groups_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/friends/groups` — Create a friend group.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/friends/groups",
    tag = "Private Extension - Friends",
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Group created", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn create_friend_group_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `DELETE /_matrix/client/v1/friends/groups/{group_id}` — Delete a friend group.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    delete,
    path = "/_matrix/client/v1/friends/groups/{group_id}",
    tag = "Private Extension - Friends",
    params(
        ("group_id" = String, Path, description = "The ID of the group"),
    ),
    responses(
        (status = 200, description = "Deleted", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn delete_friend_group_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `PUT /_matrix/client/v1/friends/groups/{group_id}/name` — Rename a friend group.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    put,
    path = "/_matrix/client/v1/friends/groups/{group_id}/name",
    tag = "Private Extension - Friends",
    params(
        ("group_id" = String, Path, description = "The ID of the group"),
    ),
    request_body = serde_json::Value,
    responses(
        (status = 200, description = "Renamed", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn rename_friend_group_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/groups/{group_id}/friends` — Get friends in a group.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/groups/{group_id}/friends",
    tag = "Private Extension - Friends",
    params(
        ("group_id" = String, Path, description = "The ID of the group"),
    ),
    responses(
        (status = 200, description = "Friends in group", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friend_group_friends_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/friends/groups/{group_id}/add/{user_id}` — Add a friend to a group.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/friends/groups/{group_id}/add/{user_id}",
    tag = "Private Extension - Friends",
    params(
        ("group_id" = String, Path, description = "The ID of the group"),
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Added", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn add_friend_to_group_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `POST /_matrix/client/v1/friends/groups/{group_id}/remove/{user_id}` — Remove a friend from a group.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    post,
    path = "/_matrix/client/v1/friends/groups/{group_id}/remove/{user_id}",
    tag = "Private Extension - Friends",
    params(
        ("group_id" = String, Path, description = "The ID of the group"),
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Removed", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn remove_friend_from_group_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/{user_id}/groups` — Get groups a friend is in.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/{user_id}/groups",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "Groups list", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friend_groups_for_user_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}

/// `GET /_matrix/client/v1/friends/dm/{user_id}` — Get or create a DM with a friend.
#[cfg(feature = "openapi-docs")]
#[utoipa::path(
    get,
    path = "/_matrix/client/v1/friends/dm/{user_id}",
    tag = "Private Extension - Friends",
    params(
        ("user_id" = String, Path, description = "The ID of the user"),
    ),
    responses(
        (status = 200, description = "DM room", body = serde_json::Value),
    ),
    security(
        ("BearerAuth" = [])
    )
)]
pub fn get_friend_dm_doc() -> axum::Json<serde_json::Value> {
    unreachable!("This function exists only for OpenAPI documentation purposes")
}
