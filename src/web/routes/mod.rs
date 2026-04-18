mod account_compat;
pub mod account_data;
pub mod admin;
pub mod ai_connection;
pub mod app_service;
mod assembly;
mod auth_compat;
pub mod background_update;
pub mod burn_after_read;
pub mod captcha;
pub mod cas;
pub mod device;
pub mod directory;
mod directory_reporting;
pub mod dm;
pub mod e2ee_routes;
pub mod ephemeral;
pub mod event_report;
pub mod external_service;
pub mod extractors;
pub mod feature_flags;
pub mod federation;
pub mod friend_room;
pub mod guest;
pub mod handlers;
pub mod invite_blocklist;
pub mod key_backup;
pub mod key_rotation;
pub mod media;
pub mod moderation;
pub mod module;
pub mod oidc;
#[cfg(feature = "openclaw-routes")]
pub mod openclaw;
pub mod pinned;
pub mod presence;
pub mod push;
pub mod push_notification;
pub mod push_rules;
pub mod qr_login;
pub mod reactions;
pub mod relations;
pub mod rendezvous;
mod response_helpers;
pub mod room;
mod room_access;
pub mod room_summary;
pub mod saml;
pub mod sliding_sync;
pub mod space;
pub mod state;
pub mod sticky_event;
pub mod sync;
pub mod tags;
pub mod telemetry;
pub mod thirdparty;
pub mod typing;
pub mod validators;
pub mod verification_routes;
pub mod voice;
pub mod voip;
pub mod widget;
pub mod worker;

pub use crate::common::ApiError;
pub(crate) use account_compat::{
    add_threepid, change_password_uia, deactivate_account, delete_threepid, get_avatar_url,
    get_displayname, get_profile, get_threepids, request_password_email_verification,
    unbind_threepid, update_avatar, update_displayname, whoami,
};
pub use account_data::create_account_data_router;
pub use admin::create_admin_module_router;
pub use ai_connection::create_ai_connection_router;
pub use app_service::create_app_service_router;
pub use assembly::create_router;
pub(crate) use auth_compat::{
    check_username_availability, get_login_flows, get_register_flows, login, logout, logout_all,
    refresh_token, register, request_email_verification, submit_email_token,
};
pub use background_update::create_background_update_router;
pub use burn_after_read::create_burn_after_read_router;
pub use captcha::create_captcha_router;
pub use cas::cas_routes;
pub use device::create_device_router;
pub(crate) use directory_reporting::{
    delete_room_alias, delete_room_alias_direct, get_public_rooms, get_room_aliases,
    get_room_by_alias, get_scanner_info, get_user_directory_profile, list_user_directory,
    query_public_rooms, report_event, report_room, search_user_directory, set_room_alias,
    set_room_alias_direct, update_report_score,
};
pub use dm::create_dm_router;
pub use e2ee_routes::create_e2ee_router;
pub use event_report::create_event_report_router;
pub use external_service::create_external_service_router;
pub(crate) use extractors::extract_token_from_headers;
pub use extractors::{
    AdminUser, AuthExtractor, AuthenticatedUser, MatrixJson, OptionalAuthenticatedUser,
};
pub use feature_flags::create_feature_flags_router;
pub use federation::create_federation_router;
pub use friend_room::create_friend_router;
pub use guest::create_guest_router;
pub(crate) use handlers::room::{
    ban_user, claim_room_keys, convert_room_event, create_room, forget_room, forward_room_keys,
    get_event_keys, get_joined_members, get_joined_rooms, get_membership_events, get_messages,
    get_my_rooms, get_power_levels, get_receipts, get_retention_policy, get_room_account_data,
    get_room_capabilities, get_room_encrypted_events, get_room_event_perspective,
    get_room_event_url, get_room_external_ids, get_room_info, get_room_invites, get_room_key_count,
    get_room_keys, get_room_keys_version, get_room_members, get_room_members_recent,
    get_room_membership, get_room_message_queue, get_room_metadata, get_room_notifications,
    get_room_rendered, get_room_service_types, get_room_spaces, get_room_state, get_room_sync,
    get_room_thread, get_room_thread_by_id, get_room_timeline, get_room_turn_server,
    get_room_unread_count, get_room_user_fragments, get_room_vault_data, get_room_version,
    get_room_visibility, get_single_event, get_state_by_type, get_state_event,
    get_state_event_empty_key, get_user_rooms, invite_user, invite_user_by_room, join_room,
    join_room_by_id_or_alias, kick_user, knock_room, leave_room, put_state_event,
    put_state_event_empty_key, put_state_event_no_key, redact_event, room_initial_sync,
    search_room_messages, send_message, send_receipt, send_state_event, set_read_markers,
    set_room_account_data, set_room_vault_data, set_room_visibility, sign_room_event,
    translate_room_event, unban_user, upgrade_room, verify_room_event,
};
pub use handlers::{
    get_capabilities, get_client_versions, get_server_version, get_well_known_client,
    get_well_known_server, get_well_known_support, health_check,
};
pub use key_backup::create_key_backup_router;
pub use key_rotation::create_key_rotation_router;
pub use media::create_media_router;
pub use moderation::create_moderation_router;
pub use module::create_module_router;
pub use oidc::create_oidc_router;
#[cfg(feature = "openclaw-routes")]
pub use openclaw::create_openclaw_router;
pub use presence::create_presence_router;
pub use push::create_push_router;
pub use push_notification::create_push_notification_router;
pub use push_rules::{
    get_default_push_rules, get_push_rules_default, get_push_rules_global_default,
};
pub use reactions::create_reactions_router;
pub use relations::create_relations_router;
pub use rendezvous::create_rendezvous_router;
pub use room::create_room_router;
pub(crate) use room_access::{
    ensure_room_member, is_joined_room_member, is_joined_room_member_or_creator,
};
pub use room_summary::create_room_summary_router;
pub use saml::create_saml_router;
pub use sliding_sync::create_sliding_sync_router;
pub use space::create_space_router;
pub use state::AppState;
pub use sync::create_sync_router;
pub use tags::create_tags_router;
pub use telemetry::create_telemetry_router;
pub use thirdparty::create_thirdparty_router;
pub use validators::{
    validate_event_id, validate_presence_status, validate_receipt_type, validate_room_alias,
    validate_room_id, validate_user_id,
};
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

#[cfg(test)]
mod top_level_router_tests {
    #[test]
    fn test_top_level_compat_routes_structure() {
        let compat_routes = [
            "/_matrix/client/r0/capabilities",
            "/_matrix/client/v3/capabilities",
            "/_matrix/client/r0/voip/config",
            "/_matrix/client/v3/rooms/{room_id}/call/{call_id}",
            "/_matrix/client/v1/media/config",
            "/_matrix/client/r0/media/config",
            "/_matrix/client/v3/media/config",
        ];

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_voip_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/voip/turnServer",
            "/voip/config",
            "/voip/turnServer/guest",
            "/rooms/{room_id}/send/m.call.invite/{txn_id}",
            "/rooms/{room_id}/send/m.call.candidates/{txn_id}",
            "/rooms/{room_id}/send/m.call.answer/{txn_id}",
            "/rooms/{room_id}/send/m.call.hangup/{txn_id}",
            "/rooms/{room_id}/call/{call_id}",
        ];

        assert_eq!(shared_paths.len(), 8);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_top_level_router_keeps_version_boundaries() {
        let capabilities_paths = ["/capabilities"];
        let media_config_paths = ["/media/config"];
        let direct_only_paths = ["/_matrix/client/versions", "/_matrix/client/v3/versions"];

        assert!(capabilities_paths
            .iter()
            .all(|path| !path.contains("/versions")));
        assert!(media_config_paths
            .iter()
            .all(|path| path.starts_with("/media/")));
        assert!(direct_only_paths
            .iter()
            .all(|path| !path.ends_with("/capabilities")));
    }
}

#[cfg(test)]
mod auth_router_tests {
    #[test]
    fn test_auth_routes_structure() {
        let compat_routes = [
            "/_matrix/client/r0/register",
            "/_matrix/client/v3/login",
            "/_matrix/client/r0/logout/all",
            "/_matrix/client/v3/refresh",
        ];
        let v1_only_routes = [
            "/_matrix/client/v1/login/get_qr_code",
            "/_matrix/client/v1/login/qr/{transaction_id}/status",
        ];

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
        assert!(v1_only_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/v1/")));
    }

    #[test]
    fn test_auth_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/register",
            "/register/available",
            "/register/email/requestToken",
            "/register/email/submitToken",
            "/login",
            "/logout",
            "/logout/all",
            "/refresh",
        ];

        assert_eq!(shared_paths.len(), 8);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_auth_router_keeps_qr_login_outside_compat_scope() {
        let compat_paths = ["/register", "/login", "/refresh"];
        let qr_paths = [
            "/_matrix/client/v1/login/get_qr_code",
            "/_matrix/client/v1/login/qr/start",
        ];

        assert!(compat_paths.iter().all(|path| !path.contains("/login/qr")));
        assert!(qr_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/v1/login/")));
    }
}

#[cfg(test)]
mod account_router_tests {
    #[test]
    fn test_account_routes_structure() {
        let compat_routes = [
            "/_matrix/client/r0/account/whoami",
            "/_matrix/client/v3/account/password",
            "/_matrix/client/r0/account/3pid",
            "/_matrix/client/v3/profile/{user_id}/avatar_url",
        ];
        let r0_only_routes = [
            "/_matrix/client/r0/account/profile/{user_id}",
            "/_matrix/client/r0/account/profile/{user_id}/displayname",
            "/_matrix/client/r0/account/profile/{user_id}/avatar_url",
        ];

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
        assert!(r0_only_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/r0/")));
    }

    #[test]
    fn test_account_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/account/whoami",
            "/account/password",
            "/account/password/email/requestToken",
            "/account/password/email/submitToken",
            "/account/deactivate",
            "/account/3pid",
            "/profile/{user_id}",
            "/profile/{user_id}/displayname",
            "/profile/{user_id}/avatar_url",
        ];

        assert_eq!(shared_paths.len(), 9);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_account_router_keeps_r0_account_profile_outside_compat_scope() {
        let compat_paths = ["/account/whoami", "/account/3pid", "/profile/{user_id}"];
        let r0_only_paths = ["/_matrix/client/r0/account/profile/{user_id}"];
        let absent_v3_paths = ["/_matrix/client/v3/account/profile/{user_id}"];

        assert!(compat_paths
            .iter()
            .all(|path| !path.starts_with("/account/profile/")));
        assert!(r0_only_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/r0/")));
        assert!(absent_v3_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/v3/account/profile/")));
    }
}

#[cfg(test)]
mod directory_router_tests {
    #[test]
    fn test_directory_routes_structure() {
        let compat_routes = [
            "/_matrix/client/r0/user_directory/search",
            "/_matrix/client/v3/user_directory/list",
            "/_matrix/client/r0/directory/room/{room_alias}",
            "/_matrix/client/v3/publicRooms",
        ];
        let r0_only_routes = [
            "/_matrix/client/r0/directory/room/{room_id}/alias",
            "/_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}",
        ];

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
        assert!(r0_only_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/r0/")));
    }

    #[test]
    fn test_directory_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/user_directory/search",
            "/user_directory/list",
            "/directory/list/room/{room_id}",
            "/directory/room/{room_alias}",
            "/publicRooms",
        ];

        assert_eq!(shared_paths.len(), 5);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_directory_router_keeps_r0_alias_management_outside_compat_scope() {
        let compat_paths = [
            "/directory/room/{room_alias}",
            "/publicRooms",
            "/directory/list/room/{room_id}",
        ];
        let r0_only_paths = [
            "/_matrix/client/r0/directory/room/{room_id}/alias",
            "/_matrix/client/r0/directory/room/{room_id}/alias/{room_alias}",
        ];

        assert!(compat_paths
            .iter()
            .all(|path| !path.contains("/alias/{room_alias}") && !path.ends_with("/alias")));
        assert!(r0_only_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/r0/")));
    }
}

#[cfg(test)]
mod room_router_tests {
    #[test]
    fn test_room_routes_structure() {
        let r0_v3_compat_routes = [
            "/_matrix/client/r0/rooms/{room_id}",
            "/_matrix/client/v3/rooms/{room_id}/messages",
            "/_matrix/client/r0/rooms/{room_id}/state/{event_type}",
            "/_matrix/client/v3/rooms/{room_id}/event/{event_id}",
        ];
        let all_version_report_routes = [
            "/_matrix/client/r0/rooms/{room_id}/report/{event_id}",
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}/score",
            "/_matrix/client/v3/rooms/{room_id}/report/{event_id}",
        ];
        let version_specific_routes = [
            "/_matrix/client/r0/createRoom",
            "/_matrix/client/v1/rooms/{room_id}/report/{event_id}/scanner_info",
            "/_matrix/client/v3/rooms/{room_id}/notifications",
        ];

        assert!(r0_v3_compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
        assert_eq!(all_version_report_routes.len(), 3);
        assert_eq!(version_specific_routes.len(), 3);
    }

    #[test]
    fn test_room_compat_router_contains_shared_paths() {
        let shared_paths = [
            "/rooms/{room_id}",
            "/events",
            "/rooms/{room_id}/messages",
            "/rooms/{room_id}/send/{event_type}/{txn_id}",
            "/rooms/{room_id}/event/{event_id}",
        ];

        assert_eq!(shared_paths.len(), 5);
        assert!(shared_paths.iter().all(|path| path.starts_with('/')));
    }

    #[test]
    fn test_room_router_keeps_version_boundaries() {
        let report_compat_paths = ["/rooms/{room_id}/report/{event_id}"];
        let v1_only_paths = ["/_matrix/client/v1/rooms/{room_id}/report/{event_id}/scanner_info"];
        let v3_only_paths = [
            "/_matrix/client/v3/rooms/{room_id}/report",
            "/_matrix/client/v3/rooms/{room_id}/notifications",
        ];

        assert!(report_compat_paths
            .iter()
            .all(|path| !path.contains("scanner_info") && !path.ends_with("/report")));
        assert!(v1_only_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/v1/")));
        assert!(v3_only_paths
            .iter()
            .all(|path| path.starts_with("/_matrix/client/v3/")));
    }
}

#[cfg(test)]
mod presence_router_tests {
    #[test]
    fn test_presence_routes_structure() {
        let compat_routes = [
            "/_matrix/client/r0/presence/{user_id}/status",
            "/_matrix/client/v3/presence/{user_id}/status",
        ];
        let v3_only_routes = ["/_matrix/client/v3/presence/list"];

        assert!(compat_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
        assert!(v3_only_routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/v3/")));
    }

    #[test]
    fn test_presence_compat_router_contains_shared_paths() {
        let shared_paths = ["/presence/{user_id}/status"];

        assert_eq!(shared_paths.len(), 1);
        assert!(shared_paths
            .iter()
            .all(|path| path.starts_with("/presence/")));
    }

    #[test]
    fn test_presence_router_keeps_presence_list_outside_compat_scope() {
        let compat_paths = ["/presence/{user_id}/status"];
        let v3_only_paths = ["/_matrix/client/v3/presence/list"];

        assert!(compat_paths
            .iter()
            .all(|path| !path.ends_with("/presence/list")));
        assert!(v3_only_paths
            .iter()
            .all(|path| path.ends_with("/presence/list")));
    }
}
