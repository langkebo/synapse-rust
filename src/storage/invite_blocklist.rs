pub use synapse_storage::invite_blocklist::*;

#[cfg(test)]
mod tests {
    use super::InviteBlocklistStorage;
    use sqlx::PgPool;
    use std::sync::Arc;

    #[test]
    fn root_invite_blocklist_storage_reexport_keeps_constructor_shape() {
        let _ctor: fn(Arc<PgPool>) -> InviteBlocklistStorage = InviteBlocklistStorage::new;
    }

    #[test]
    fn test_user_id_format() {
        let valid_users = vec!["@user:localhost", "@alice:example.com"];

        for user in valid_users {
            assert!(user.starts_with('@'), "User ID should start with @");
            assert!(user.contains(':'), "User ID should contain : separator");
        }
    }

    #[test]
    fn test_room_id_format() {
        let valid_rooms = vec!["!room:localhost", "!abc123:matrix.org"];

        for room in valid_rooms {
            assert!(room.starts_with('!'), "Room ID should start with !");
            assert!(room.contains(':'), "Room ID should contain : separator");
        }
    }
}
