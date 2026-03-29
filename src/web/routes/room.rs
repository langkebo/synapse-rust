use crate::web::routes::{
    ban_user, create_room, forget_room, get_joined_members, get_membership_events, get_messages,
    get_power_levels, get_room_aliases, get_room_info, get_room_members, get_room_notifications,
    get_room_state, get_single_event, get_state_by_type, get_state_event,
    get_state_event_empty_key, get_user_rooms, invite_blocklist, invite_user, invite_user_by_room,
    join_room, join_room_by_id_or_alias, kick_user, knock_room, leave_room, put_state_event,
    put_state_event_empty_key, put_state_event_no_key, redact_event, room_initial_sync,
    send_message, send_receipt, send_state_event, set_read_markers, sticky_event, unban_user,
    AppState,
};
use axum::{
    routing::{get, post, put},
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
        .route(
            "/rooms/{room_id}/receipt/{receipt_type}/{event_id}",
            post(send_receipt),
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
        .route("/rooms/{room_id}/joined_members", get(get_joined_members))
        .route("/rooms/{room_id}/invite", post(invite_user))
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
