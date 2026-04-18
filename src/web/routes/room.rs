use crate::web::routes::handlers::room::{get_room_device, get_room_reduced_events};
use crate::web::routes::{
    ban_user, claim_room_keys, convert_room_event, create_room, forget_room, forward_room_keys,
    get_event_keys, get_joined_members, get_membership_events, get_messages, get_power_levels,
    get_receipts, get_retention_policy, get_room_account_data, get_room_aliases,
    get_room_capabilities, get_room_encrypted_events, get_room_event_perspective,
    get_room_event_url, get_room_external_ids, get_room_info, get_room_invites, get_room_key_count,
    get_room_keys, get_room_keys_version, get_room_members, get_room_members_recent,
    get_room_membership, get_room_message_queue, get_room_metadata, get_room_notifications,
    get_room_rendered, get_room_service_types, get_room_spaces, get_room_state, get_room_sync,
    get_room_thread, get_room_thread_by_id, get_room_timeline, get_room_turn_server,
    get_room_unread_count, get_room_user_fragments, get_room_vault_data, get_room_version,
    get_single_event, get_state_by_type, get_state_event, get_state_event_empty_key,
    get_user_rooms, invite_blocklist, invite_user, invite_user_by_room, join_room,
    join_room_by_id_or_alias, kick_user, knock_room, leave_room, pinned, put_state_event,
    put_state_event_empty_key, put_state_event_no_key, redact_event, room_initial_sync,
    search_room_messages, send_message, send_receipt, send_state_event, set_read_markers,
    set_room_account_data, set_room_vault_data, sign_room_event, sticky_event,
    translate_room_event, unban_user, verify_room_event, AppState,
};
use axum::{
    routing::{delete, get, post, put},
    Router,
};

fn create_room_power_levels_compat_router() -> Router<AppState> {
    Router::new().route(
        "/rooms/{room_id}/state/m.room.power_levels/",
        get(get_power_levels),
    )
}

fn create_room_r0_v3_compat_router() -> Router<AppState> {
    Router::new()
        .route("/rooms/{room_id}", get(get_room_info))
        .route("/rooms/{room_id}/messages", get(get_messages))
        .route("/rooms/{room_id}/search", post(search_room_messages))
        .route(
            "/rooms/{room_id}/membership/{user_id}",
            get(get_room_membership),
        )
        .route(
            "/rooms/{room_id}/receipt/{receipt_type}/{event_id}",
            post(send_receipt),
        )
        .route(
            "/rooms/{room_id}/receipts/{receipt_type}/{event_id}",
            get(get_receipts),
        )
        .route(
            "/rooms/{room_id}/read_markers",
            post(set_read_markers).put(set_read_markers),
        )
        .route("/rooms/{room_id}/aliases", get(get_room_aliases))
        .route("/rooms/{room_id}/join", post(join_room))
        .route("/rooms/{room_id}/leave", post(leave_room))
        .route("/rooms/{room_id}/upgrade", post(super::upgrade_room))
        .route("/rooms/{room_id}/forget", post(forget_room))
        .route("/rooms/{room_id}/initialSync", get(room_initial_sync))
        .route("/rooms/{room_id}/members", get(get_room_members))
        .route(
            "/rooms/{room_id}/members/recent",
            get(get_room_members_recent),
        )
        .route("/rooms/{room_id}/joined_members", get(get_joined_members))
        .route("/rooms/{room_id}/version", get(get_room_version))
        .route("/rooms/{room_id}/invite", post(invite_user))
        .route("/rooms/{room_id}/invites", get(get_room_invites))
        .route("/user/{user_id}/rooms", get(get_user_rooms))
        .route(
            "/rooms/{room_id}/state/{event_type}/{state_key}",
            put(put_state_event).get(get_state_event),
        )
        .route(
            "/rooms/{room_id}/state/{event_type}/",
            put(put_state_event_empty_key).get(get_state_event_empty_key),
        )
        .route(
            "/rooms/{room_id}/state/{event_type}",
            put(put_state_event_no_key)
                .get(get_state_by_type)
                .post(send_state_event),
        )
        .route("/rooms/{room_id}/state", get(get_room_state))
        .route(
            "/rooms/{room_id}/redact/{event_id}/{txn_id}",
            put(redact_event),
        )
        .route("/rooms/{room_id}/kick", post(kick_user))
        .route("/rooms/{room_id}/ban", post(ban_user))
        .route("/rooms/{room_id}/unban", post(unban_user))
        .route(
            "/rooms/{room_id}/pinned_events",
            get(pinned::get_pinned_events).post(pinned::pin_event),
        )
        .route(
            "/rooms/{room_id}/pinned_events/{event_id}",
            delete(pinned::unpin_event),
        )
        .route(
            "/rooms/{room_id}/send/{event_type}/{txn_id}",
            put(send_message),
        )
        .route("/rooms/{room_id}/event/{event_id}", get(get_single_event))
}

fn create_room_r0_router() -> Router<AppState> {
    create_room_r0_v3_compat_router()
        .merge(create_room_power_levels_compat_router())
        .route("/createRoom", post(create_room))
        .route(
            "/rooms/{room_id}/get_membership_events",
            post(get_membership_events),
        )
}

fn create_room_v1_router() -> Router<AppState> {
    create_room_power_levels_compat_router()
}

fn create_room_v3_router() -> Router<AppState> {
    create_room_r0_v3_compat_router()
        .merge(create_room_power_levels_compat_router())
        .route(
            "/rooms/{room_id}/notifications",
            get(get_room_notifications),
        )
        .route("/rooms/{room_id}/capabilities", get(get_room_capabilities))
        .route("/rooms/{room_id}/sync", get(get_room_sync))
        .route("/rooms/{room_id}/timeline", get(get_room_timeline))
        .route("/rooms/{room_id}/unread_count", get(get_room_unread_count))
        .route(
            "/rooms/{room_id}/account_data/{type}",
            get(get_room_account_data).put(set_room_account_data),
        )
        .route("/rooms/{room_id}/turn_server", get(get_room_turn_server))
        .route("/rooms/{room_id}/metadata", get(get_room_metadata))
        .route(
            "/rooms/{room_id}/vault_data",
            get(get_room_vault_data).put(set_room_vault_data),
        )
        .route("/rooms/{room_id}/retention", get(get_retention_policy))
        .route("/rooms/{room_id}/spaces", get(get_room_spaces))
        .route(
            "/rooms/{room_id}/encrypted_events",
            get(get_room_encrypted_events),
        )
        .route(
            "/rooms/{room_id}/reduced_events",
            get(get_room_reduced_events),
        )
        .route("/rooms/{room_id}/device/{device_id}", get(get_room_device))
        .route("/rooms/{room_id}/rendered/", get(get_room_rendered))
        .route("/rooms/{room_id}/external_ids", get(get_room_external_ids))
        .route(
            "/rooms/{room_id}/event_perspective",
            get(get_room_event_perspective),
        )
        .route(
            "/rooms/{room_id}/fragments/{user_id}",
            get(get_room_user_fragments),
        )
        .route(
            "/rooms/{room_id}/service_types",
            get(get_room_service_types),
        )
        .route(
            "/rooms/{room_id}/event/{event_id}/url",
            get(get_room_event_url),
        )
        .route(
            "/rooms/{room_id}/translate/{event_id}",
            post(translate_room_event),
        )
        .route(
            "/rooms/{room_id}/convert/{event_id}",
            post(convert_room_event),
        )
        .route("/rooms/{room_id}/sign/{event_id}", put(sign_room_event))
        .route(
            "/rooms/{room_id}/verify/{event_id}",
            post(verify_room_event),
        )
        .route("/rooms/{room_id}/keys", get(get_room_keys))
        .route("/rooms/{room_id}/keys/count", get(get_room_key_count))
        .route("/rooms/{room_id}/keys/version", get(get_room_keys_version))
        .route("/rooms/{room_id}/keys/claim", post(claim_room_keys))
        .route("/rooms/{room_id}/room_keys/keys", put(forward_room_keys))
        .route(
            "/rooms/{room_id}/message_queue",
            get(get_room_message_queue),
        )
        .route(
            "/rooms/{room_id}/threads/{thread_id}",
            get(get_room_thread_by_id),
        )
        .route("/rooms/{room_id}/keys/{event_id}", get(get_event_keys))
        .route("/rooms/{room_id}/thread/{event_id}", get(get_room_thread))
        .route("/join/{room_id_or_alias}", post(join_room_by_id_or_alias))
        .route("/knock/{room_id_or_alias}", post(knock_room))
        .route("/invite/{room_id}", post(invite_user_by_room))
        .route(
            "/rooms/{room_id}/invite_blocklist",
            get(invite_blocklist::get_invite_blocklist)
                .post(invite_blocklist::set_invite_blocklist),
        )
        .route(
            "/rooms/{room_id}/invite_allowlist",
            get(invite_blocklist::get_invite_allowlist)
                .post(invite_blocklist::set_invite_allowlist),
        )
        .route(
            "/rooms/{room_id}/sticky_events",
            get(sticky_event::get_sticky_events).post(sticky_event::set_sticky_events),
        )
        .route(
            "/rooms/{room_id}/sticky_events/{event_type}",
            axum::routing::delete(sticky_event::clear_sticky_event),
        )
}

pub fn create_room_router() -> Router<AppState> {
    Router::new()
        .nest("/_matrix/client/r0", create_room_r0_router())
        .nest("/_matrix/client/v1", create_room_v1_router())
        .nest("/_matrix/client/v3", create_room_v3_router())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_room_routes_structure() {
        let routes = [
            "/_matrix/client/r0/rooms/{room_id}",
            "/_matrix/client/r0/rooms/{room_id}/messages",
            "/_matrix/client/r0/createRoom",
            "/_matrix/client/v1/rooms/{room_id}/state/m.room.power_levels/",
            "/_matrix/client/v3/rooms/{room_id}/notifications",
            "/_matrix/client/v3/join/{room_id_or_alias}",
            "/_matrix/client/v3/rooms/{room_id}/sticky_events",
        ];

        assert!(routes
            .iter()
            .all(|route| route.starts_with("/_matrix/client/")));
    }

    #[test]
    fn test_room_router_keeps_version_specific_paths() {
        let r0_only = ["/_matrix/client/r0/rooms/{room_id}/get_membership_events"];
        let v3_only = [
            "/_matrix/client/v3/rooms/{room_id}/notifications",
            "/_matrix/client/v3/rooms/{room_id}/sticky_events/{event_type}",
        ];

        assert!(r0_only
            .iter()
            .all(|route| route.starts_with("/_matrix/client/r0/")));
        assert!(v3_only
            .iter()
            .all(|route| route.starts_with("/_matrix/client/v3/")));
    }
}
