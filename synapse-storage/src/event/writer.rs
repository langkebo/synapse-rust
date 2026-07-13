use async_trait::async_trait;
use std::sync::Arc;

use super::models::*;

/// Write-only interface for event mutations.
#[async_trait]
pub trait EventWriter: Send + Sync {
    /// Returns a reference to the database connection pool.
    /// In-memory implementations may return `unimplemented!()`.
    fn pool(&self) -> &Arc<sqlx::PgPool>;

    // ── mutation ──────────────────────────────────────────────────────

    async fn create_event(
        &self,
        params: CreateEventParams,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error>;

    async fn update_event_signatures_and_hashes(
        &self,
        event_id: &str,
        signatures: &serde_json::Value,
        hashes: &serde_json::Value,
    ) -> Result<(), sqlx::Error>;

    async fn redact_event_content(&self, event_id: &str, redacted_by: Option<&str>) -> Result<(), sqlx::Error>;

    // ── mutation: graph / signatures / reports ─────────────────────────

    #[allow(clippy::too_many_arguments)]
    async fn create_event_with_graph(
        &self,
        params: CreateEventParams,
        prev_events: &[String],
        auth_events: &[String],
        depth: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error>;

    #[allow(clippy::too_many_arguments)]
    async fn save_event_signature(
        &self,
        event_id: &str,
        user_id: &str,
        device_id: &str,
        signature: &str,
        key_id: &str,
        algorithm: &str,
        created_ts: i64,
    ) -> Result<(), sqlx::Error>;

    async fn report_event(
        &self,
        event_id: &str,
        room_id: &str,
        reported_user_id: &str,
        reporter_user_id: &str,
        reason: Option<&str>,
        score: i32,
    ) -> Result<i64, sqlx::Error>;

    // ── ephemeral mutations ─────────────────────────────────────────────

    async fn add_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
        stream_id: i64,
    ) -> Result<(), sqlx::Error>;

    #[allow(clippy::too_many_arguments)]
    async fn upsert_ephemeral_event(
        &self,
        room_id: &str,
        user_id: &str,
        event_type: &str,
        content: &serde_json::Value,
        stream_id: i64,
        created_ts: i64,
        expires_at: Option<i64>,
    ) -> Result<(), sqlx::Error>;

    async fn delete_ephemeral_event(&self, room_id: &str, event_type: &str, user_id: &str) -> Result<(), sqlx::Error>;

    // ── encryption / retention ─────────────────────────────────────────────

    async fn delete_events_before(&self, room_id: &str, timestamp: i64) -> Result<u64, sqlx::Error>;

    async fn upsert_power_levels_event(
        &self,
        event_id: &str,
        room_id: &str,
        user_id: &str,
        content: serde_json::Value,
        origin_server_ts: i64,
        sender: &str,
    ) -> Result<(), sqlx::Error>;
}
