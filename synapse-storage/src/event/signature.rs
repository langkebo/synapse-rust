//! Event signature methods for [`EventStorage`].

use super::models::EventSignature;
use super::EventStorage;

impl EventStorage {
    /// Update the `signatures` and `hashes` JSONB columns for an event after
    /// it has been signed locally.  This is the persistence counterpart of
    /// `synapse_federation::signing::sign_and_hash_event`.
    pub async fn update_event_signatures_and_hashes(
        &self,
        event_id: &str,
        signatures: &serde_json::Value,
        hashes: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            UPDATE events SET signatures = $2, hashes = $3 WHERE event_id = $1
            ",
            event_id,
            signatures,
            hashes,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Save (upsert) an event signature.
    #[allow(clippy::too_many_arguments)]
    pub async fn save_event_signature(
        &self,
        event_id: &str,
        user_id: &str,
        device_id: &str,
        signature: &str,
        key_id: &str,
        algorithm: &str,
        created_ts: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            INSERT INTO event_signatures (id, event_id, user_id, device_id, signature, key_id, algorithm, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            ON CONFLICT (event_id, user_id, device_id, key_id) DO UPDATE
            SET signature = EXCLUDED.signature,
                algorithm = EXCLUDED.algorithm,
                created_ts = EXCLUDED.created_ts
            ",
            uuid::Uuid::new_v4(),
            event_id,
            user_id,
            device_id,
            signature,
            key_id,
            algorithm,
            created_ts,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Get all signatures for an event.
    pub async fn get_event_signatures(&self, event_id: &str) -> Result<Vec<EventSignature>, sqlx::Error> {
        // 7 列与 `EventSignature` 的 7 个字段一一对应（`algorithm` 列不被读 —— 见 D-99 的
        // "只写不读"观察）。`created_ts` 列是 `BIGINT NOT NULL` 而字段是 `Option<i64>`：
        // R4 明确该方向**不报错**（NOT NULL 列配 `Option` 字段合法），故保持原样不动结构体。
        sqlx::query_as!(
            EventSignature,
            r"
            SELECT id, event_id, user_id, device_id, signature, key_id, created_ts
            FROM event_signatures
            WHERE event_id = $1
            ",
            event_id,
        )
        .fetch_all(&*self.pool)
        .await
    }
}
