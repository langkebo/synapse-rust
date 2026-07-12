use super::*;

use crate::room_account_data::{RoomAccountDataRecord, RoomAccountDataStoreApi};

/// In-memory [`RoomAccountDataStoreApi`] backed by a `HashMap` keyed on
/// `(user_id, room_id, data_type)`; the value holds the content plus the last
/// update timestamp. Raw-`PgRow` methods are intentionally unsupported.
#[derive(Clone, Debug, Default)]
pub struct InMemoryRoomAccountDataStore {
    #[allow(clippy::type_complexity)]
    data: Arc<RwLock<HashMap<(String, String, String), (serde_json::Value, Option<i64>)>>>,
}

impl InMemoryRoomAccountDataStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl RoomAccountDataStoreApi for InMemoryRoomAccountDataStore {
    async fn get_room_account_data_content(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<Option<serde_json::Value>, ApiError> {
        Ok(self
            .data
            .read()
            .await
            .get(&(user_id.to_string(), room_id.to_string(), data_type.to_string()))
            .map(|(content, _)| content.clone()))
    }

    async fn get_room_account_data_with_ts(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
    ) -> Result<Option<(serde_json::Value, Option<i64>)>, ApiError> {
        Ok(self.data.read().await.get(&(user_id.to_string(), room_id.to_string(), data_type.to_string())).cloned())
    }

    async fn get_room_account_data(
        &self,
        _user_id: &str,
        _room_id: &str,
        _data_type: &str,
    ) -> Result<Option<sqlx::postgres::PgRow>, sqlx::Error> {
        unimplemented!(
            "in-memory mock does not support raw-row method get_room_account_data; use get_room_account_data_content"
        )
    }

    async fn list_room_account_data(
        &self,
        user_id: &str,
        room_id: &str,
    ) -> Result<Vec<RoomAccountDataRecord>, ApiError> {
        let mut records: Vec<_> = self
            .data
            .read()
            .await
            .iter()
            .filter(|((uid, rid, _), _)| uid == user_id && rid == room_id)
            .map(|((_, rid, data_type), (content, _))| RoomAccountDataRecord {
                room_id: rid.clone(),
                data_type: data_type.clone(),
                content: content.clone(),
            })
            .collect();
        records.sort_by(|a, b| a.data_type.cmp(&b.data_type));
        Ok(records)
    }

    async fn list_room_account_data_batch(
        &self,
        user_id: &str,
        room_ids: &[String],
    ) -> Result<Vec<RoomAccountDataRecord>, ApiError> {
        let room_set: HashSet<&str> = room_ids.iter().map(|s| s.as_str()).collect();
        let mut records: Vec<_> = self
            .data
            .read()
            .await
            .iter()
            .filter(|((uid, rid, _), _)| uid == user_id && room_set.contains(rid.as_str()))
            .map(|((_, rid, data_type), (content, _))| RoomAccountDataRecord {
                room_id: rid.clone(),
                data_type: data_type.clone(),
                content: content.clone(),
            })
            .collect();
        records.sort_by(|a, b| {
            (a.room_id.as_str(), a.data_type.as_str()).cmp(&(b.room_id.as_str(), b.data_type.as_str()))
        });
        Ok(records)
    }

    async fn get_room_vault_data(
        &self,
        _user_id: &str,
        _room_id: &str,
    ) -> Result<Option<sqlx::postgres::PgRow>, sqlx::Error> {
        unimplemented!(
            "in-memory mock does not support raw-row method get_room_vault_data; use get_room_account_data_content"
        )
    }

    async fn upsert_room_account_data(
        &self,
        user_id: &str,
        room_id: &str,
        data_type: &str,
        data: &serde_json::Value,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        self.data
            .write()
            .await
            .insert((user_id.to_string(), room_id.to_string(), data_type.to_string()), (data.clone(), Some(now)));
        Ok(())
    }

    async fn delete_room_account_data(&self, user_id: &str, room_id: &str, data_type: &str) -> Result<bool, ApiError> {
        Ok(self.data.write().await.remove(&(user_id.to_string(), room_id.to_string(), data_type.to_string())).is_some())
    }
}
