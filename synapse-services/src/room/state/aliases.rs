//! Room alias and directory operations.

use crate::common::error::{ApiError, ApiResult};
use serde_json::json;
use synapse_common::MatrixErrorCode;

use super::super::utils::validate_room_alias_input;
use super::service::RoomStateService;

/// `true` 表示这次 `set_room_alias` 命中了已存在的别名（即"别名已被占用"）。
///
/// 判据同 `room::lifecycle::create_events::is_room_id_already_taken`：
/// [`sqlx::Error::RowNotFound`]（`ON CONFLICT ... DO NOTHING` 的 0 行受影响形态）与 SQLSTATE
/// `23505`（unique violation，裸 INSERT 或实现替身）都要认，避免"换一种写法就漏判"。
fn is_alias_taken(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::RowNotFound) || err.as_database_error().map(|e| e.is_unique_violation()).unwrap_or(false)
}

impl RoomStateService {
    /// See [`get_room_aliases`].
    pub async fn get_room_aliases(&self, room_id: &str) -> ApiResult<Vec<String>> {
        self.room_storage
            .get_room_aliases(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get room aliases", e))
    }

    /// See [`set_room_alias`].
    ///
    /// ELEMENTSEC-2026-1729：别名已被占用时存储层返回可判别的冲突错误（0 行受影响），
    /// 这里把它翻译成 **409 `M_ROOM_IN_USE`**，而不是 500。绝不静默改指。
    pub async fn set_room_alias(&self, room_id: &str, alias: &str, created_by: &str) -> ApiResult<()> {
        validate_room_alias_input(alias)?;
        self.room_storage.set_room_alias(room_id, alias, created_by).await.map_err(|e| {
            if is_alias_taken(&e) {
                tracing::warn!(
                    room_id = %room_id,
                    room_alias = %alias,
                    "别名已被占用 ⇒ 409 M_ROOM_IN_USE（拒绝静默改指）"
                );
                ApiError::conflict_with(MatrixErrorCode::RoomInUse, format!("Room alias {alias} is already taken"))
            } else {
                ApiError::internal_with_cause("Failed to set room alias", e)
            }
        })
    }

    /// See [`get_room_by_alias`].
    pub async fn get_room_by_alias(&self, alias: &str) -> ApiResult<Option<String>> {
        validate_room_alias_input(alias)?;
        self.room_storage
            .get_room_by_alias(alias)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get room by alias", e))
    }

    /// See [`remove_room_alias`].
    pub async fn remove_room_alias(&self, room_id: &str) -> ApiResult<()> {
        self.room_storage
            .remove_room_alias(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to remove room alias", e))
    }

    /// See [`remove_room_alias_by_name`].
    pub async fn remove_room_alias_by_name(&self, alias: &str) -> ApiResult<()> {
        self.room_storage
            .remove_room_alias_by_name(alias)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to remove room alias by name", e))
    }

    /// See [`set_room_directory`].
    pub async fn set_room_directory(&self, room_id: &str, is_public: bool) -> ApiResult<()> {
        self.room_storage
            .set_room_directory(room_id, is_public)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to set room directory", e))
    }

    /// See [`get_room_visibility`].
    pub async fn get_room_visibility(&self, room_id: &str) -> ApiResult<String> {
        let is_public = self
            .room_storage
            .is_room_in_directory(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get room visibility", e))?;
        Ok(if is_public { "public".to_string() } else { "private".to_string() })
    }

    /// See [`remove_room_directory`].
    pub async fn remove_room_directory(&self, room_id: &str) -> ApiResult<()> {
        self.room_storage
            .remove_room_directory(room_id)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to remove room from directory", e))
    }

    /// See [`get_public_rooms`].
    pub async fn get_public_rooms(&self, limit: i64) -> ApiResult<serde_json::Value> {
        let rooms = self
            .room_storage
            .get_public_rooms(limit)
            .await
            .map_err(|e| ApiError::internal_with_cause("Failed to get public rooms", e))?;

        let room_list: Vec<serde_json::Value> = rooms
            .iter()
            .map(|r| {
                json!({
                    "room_id": r.room_id,
                    "name": r.name,
                    "topic": r.topic,
                    "canonical_alias": r.canonical_alias,
                    "is_public": r.is_public,
                    "join_rule": r.join_rule
                })
            })
            .collect();

        Ok(json!({
            "chunk": room_list,
            "total_room_count_estimate": room_list.len() as i64
        }))
    }
}

#[cfg(test)]
mod tests {
    use crate::room::state::service::{RoomStateService, RoomStateServiceConfig};
    use std::sync::Arc;
    use synapse_storage::test_mocks::{
        FakeUserStore, InMemoryEventStore, InMemoryMemberStore, InMemoryRoomStore, InMemoryRoomTagStore,
    };

    fn make_service() -> RoomStateService {
        let user_store: Arc<dyn synapse_storage::UserStore> = Arc::new(FakeUserStore::new());
        RoomStateService::new(RoomStateServiceConfig {
            room_storage: Arc::new(InMemoryRoomStore::new()),
            member_storage: Arc::new(InMemoryMemberStore::new()),
            event_reader: Arc::new(InMemoryEventStore::new()),
            event_writer: Arc::new(InMemoryEventStore::new()),
            room_tag_storage: Arc::new(InMemoryRoomTagStore::new()),
            user_storage: user_store.clone(),
            server_name: "test.example.com".to_string(),
        })
    }

    #[tokio::test]
    async fn get_room_aliases_returns_empty_for_no_aliases() {
        let svc = make_service();
        let aliases = svc.get_room_aliases("!room:ex.com").await.unwrap();
        assert!(aliases.is_empty());
    }

    #[tokio::test]
    async fn set_room_alias_validates_input() {
        let svc = make_service();
        // Invalid alias format should fail validation
        let result = svc.set_room_alias("!room:ex.com", "invalid_no_hash", "@alice:ex.com").await;
        assert!(result.is_err(), "set_room_alias should reject invalid alias format");
    }

    /// ELEMENTSEC-2026-1729：别名已被占用时必须是 409 `M_ROOM_IN_USE`，不能静默改指，
    /// 也不能降级成 500。
    #[tokio::test]
    async fn set_room_alias_conflict_maps_to_409_room_in_use() {
        use synapse_common::error::{ApiErrorKind, MatrixErrorCode};

        let store = Arc::new(InMemoryRoomStore::new());
        store.create_room("!room:ex.com", "@alice:ex.com", "invite", "1", false).await.unwrap();
        let svc = RoomStateService::new(RoomStateServiceConfig {
            room_storage: store.clone(),
            member_storage: Arc::new(InMemoryMemberStore::new()),
            event_reader: Arc::new(InMemoryEventStore::new()),
            event_writer: Arc::new(InMemoryEventStore::new()),
            room_tag_storage: Arc::new(InMemoryRoomTagStore::new()),
            user_storage: Arc::new(FakeUserStore::new()) as Arc<dyn synapse_storage::UserStore>,
            server_name: "test.example.com".to_string(),
        });

        svc.set_room_alias("!room:ex.com", "#dup:ex.com", "@alice:ex.com").await.unwrap();

        let err = svc
            .set_room_alias("!room:ex.com", "#dup:ex.com", "@alice:ex.com")
            .await
            .expect_err("re-setting an occupied alias must fail");
        assert_eq!(err.kind, ApiErrorKind::Conflict, "must be 409, not 500");
        assert_eq!(err.code, MatrixErrorCode::RoomInUse, "must carry M_ROOM_IN_USE");

        // The original mapping is untouched (no silent re-point).
        assert_eq!(store.get_room_by_alias("#dup:ex.com").await.unwrap().as_deref(), Some("!room:ex.com"));
    }

    #[tokio::test]
    async fn get_room_by_alias_validates_input() {
        let svc = make_service();
        let result = svc.get_room_by_alias("invalid_no_hash").await;
        assert!(result.is_err(), "get_room_by_alias should reject invalid alias format");
    }

    #[tokio::test]
    async fn get_room_by_alias_returns_none_for_unknown_alias() {
        let svc = make_service();
        let result = svc.get_room_by_alias("#nonexistent:ex.com").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn remove_room_alias_succeeds() {
        let svc = make_service();
        let result = svc.remove_room_alias("!room:ex.com").await;
        assert!(result.is_ok(), "remove_room_alias should not fail: {:?}", result);
    }

    #[tokio::test]
    async fn remove_room_alias_by_name_succeeds() {
        let svc = make_service();
        let result = svc.remove_room_alias_by_name("#nonexistent:ex.com").await;
        assert!(result.is_ok(), "remove_room_alias_by_name should not fail: {:?}", result);
    }

    #[tokio::test]
    async fn set_room_directory_succeeds() {
        let svc = make_service();
        let result = svc.set_room_directory("!room:ex.com", true).await;
        assert!(result.is_ok(), "set_room_directory should not fail: {:?}", result);
    }

    #[tokio::test]
    async fn get_room_visibility_returns_private_when_not_in_directory() {
        let svc = make_service();
        let visibility = svc.get_room_visibility("!missing:ex.com").await.unwrap();
        assert_eq!(visibility, "private");
    }

    #[tokio::test]
    async fn remove_room_directory_succeeds() {
        let svc = make_service();
        let result = svc.remove_room_directory("!room:ex.com").await;
        assert!(result.is_ok(), "remove_room_directory should not fail: {:?}", result);
    }

    #[tokio::test]
    async fn get_public_rooms_returns_empty_chunk_when_no_public_rooms() {
        let svc = make_service();
        let result = svc.get_public_rooms(10).await.unwrap();
        let chunk = result.get("chunk").unwrap().as_array().unwrap();
        assert!(chunk.is_empty());
        assert_eq!(result.get("total_room_count_estimate").unwrap().as_i64().unwrap(), 0);
    }
}
