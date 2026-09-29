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
    ///
    /// D-99：原先还接一个 `algorithm` 形参并写入同名列。该列**从来没有读者**
    /// （两份 `EventSignature` 结构体都不含它），而且它的语义与 `key_id` 的前缀重复
    /// （路由的默认值就是 `key_id.split(':').next()`）⇒ 列与形参一并删除。
    #[allow(clippy::too_many_arguments)]
    pub async fn save_event_signature(
        &self,
        event_id: &str,
        user_id: &str,
        device_id: &str,
        signature: &str,
        key_id: &str,
        created_ts: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            INSERT INTO event_signatures (id, event_id, user_id, device_id, signature, key_id, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (event_id, user_id, device_id, key_id) DO UPDATE
            SET signature = EXCLUDED.signature,
                created_ts = EXCLUDED.created_ts
            ",
            uuid::Uuid::new_v4(),
            event_id,
            user_id,
            device_id,
            signature,
            key_id,
            created_ts,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Get all signatures for an event.
    pub async fn get_event_signatures(&self, event_id: &str) -> Result<Vec<EventSignature>, sqlx::Error> {
        // 7 列与 `EventSignature` 的 7 个字段一一对应（D-99 已把"只写不读"的 `algorithm`
        // 列整列删除，不再有列与字段的错位）。`created_ts` 列是 `BIGINT NOT NULL` 而字段是
        // `Option<i64>`：R4 明确该方向**不报错**（NOT NULL 列配 `Option` 字段合法），故结构体不动。
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
