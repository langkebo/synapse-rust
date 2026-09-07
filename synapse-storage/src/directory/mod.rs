//! Directory storage — persists public-room directory entries to PostgreSQL.
//!
//! This module provides [`DirectoryStorage`] (Postgres) and the
//! [`DirectoryStoreApi`] trait so that [`synapse_services::DirectoryService`]
//! can delegate public-room directory operations to the database instead of
//! an in-memory `HashMap`.
//!
//! The `room_directory` table (created in `00000000_unified_schema_v10.sql`
//! and extended in `20260810140000_add_directory_metadata_columns.sql`) stores
//! both the legacy visibility flag (`is_public`) and the directory metadata
//! columns (`name`, `topic`, `avatar_url`, `canonical_alias`, `join_rule`,
//! `world_readable`, `guest_can_join`, `member_count`).

use async_trait::async_trait;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

use synapse_common::current_timestamp_millis;

/// A public-room directory entry stored in the `room_directory` table.
///
/// This is the storage-level model; [`synapse_services::DirectoryService`]
/// converts between this and its own `DirectoryRoom` service-level type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomDirectoryEntry {
    /// Room ID (e.g. `!room:example.com`)
    pub room_id: String,
    /// Room display name
    pub name: Option<String>,
    /// Room topic
    pub topic: Option<String>,
    /// Room avatar URL (mxc://)
    pub avatar_url: Option<String>,
    /// Canonical alias (e.g. `#room:example.com`)
    pub canonical_alias: Option<String>,
    /// Join rule (defaults to `"public"`)
    pub join_rule: String,
    /// Whether the room history is world-readable
    pub world_readable: bool,
    /// Whether guests can join
    pub guest_can_join: bool,
    /// Cached member count
    pub member_count: i64,
}

impl RoomDirectoryEntry {
    /// Create a new entry with default values for a public room.
    pub fn new(room_id: impl Into<String>) -> Self {
        Self {
            room_id: room_id.into(),
            name: None,
            topic: None,
            avatar_url: None,
            canonical_alias: None,
            join_rule: "public".to_string(),
            world_readable: false,
            guest_can_join: false,
            member_count: 0,
        }
    }
}

/// Storage-agnostic API for public-room directory persistence.
///
/// Implemented by [`DirectoryStorage`] (Postgres) and
/// [`crate::test_mocks::InMemoryDirectoryStore`] (in-memory test double).
/// Services should accept `Arc<dyn DirectoryStoreApi>` so tests can swap in
/// the in-memory backend without a database.
#[async_trait]
pub trait DirectoryStoreApi: Send + Sync {
    /// Upsert a room directory entry.
    ///
    /// If a row for `entry.room_id` already exists, all metadata columns are
    /// updated and `is_public` is set to `true`. If the row does not exist, a
    /// new row is inserted.
    async fn upsert_directory_entry(&self, entry: &RoomDirectoryEntry) -> Result<(), sqlx::Error>;

    /// Remove a room from the public directory (DELETE the row).
    async fn remove_from_directory(&self, room_id: &str) -> Result<(), sqlx::Error>;

    /// List public rooms ordered by member count descending.
    ///
    /// `limit` / `offset` control pagination.
    async fn list_public_rooms(&self, limit: i64, offset: i64) -> Result<Vec<RoomDirectoryEntry>, sqlx::Error>;

    /// Search public rooms by case-insensitive substring match on name or topic.
    async fn search_public_rooms(&self, filter: &str, limit: i64) -> Result<Vec<RoomDirectoryEntry>, sqlx::Error>;
}

/// PostgreSQL-backed implementation of [`DirectoryStoreApi`].
pub struct DirectoryStorage {
    pool: Arc<Pool<Postgres>>,
}

impl DirectoryStorage {
    /// See [`new`].
    /// See [`new`].
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }
}

#[async_trait]
impl DirectoryStoreApi for DirectoryStorage {
    async fn upsert_directory_entry(&self, entry: &RoomDirectoryEntry) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query(
            r#"
            INSERT INTO room_directory
                (room_id, is_public, name, topic, avatar_url, canonical_alias,
                 join_rule, world_readable, guest_can_join, member_count, added_ts, updated_ts)
            VALUES ($1, true, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
            ON CONFLICT (room_id) DO UPDATE SET
                is_public      = true,
                name           = EXCLUDED.name,
                topic          = EXCLUDED.topic,
                avatar_url     = EXCLUDED.avatar_url,
                canonical_alias = EXCLUDED.canonical_alias,
                join_rule      = EXCLUDED.join_rule,
                world_readable = EXCLUDED.world_readable,
                guest_can_join = EXCLUDED.guest_can_join,
                member_count   = EXCLUDED.member_count,
                updated_ts     = EXCLUDED.updated_ts
            "#,
        )
        .bind(&entry.room_id)
        .bind(&entry.name)
        .bind(&entry.topic)
        .bind(&entry.avatar_url)
        .bind(&entry.canonical_alias)
        .bind(&entry.join_rule)
        .bind(entry.world_readable)
        .bind(entry.guest_can_join)
        .bind(entry.member_count)
        .bind(now)
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    async fn remove_from_directory(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM room_directory WHERE room_id = $1").bind(room_id).execute(&*self.pool).await?;
        Ok(())
    }

    async fn list_public_rooms(&self, limit: i64, offset: i64) -> Result<Vec<RoomDirectoryEntry>, sqlx::Error> {
        let rows = sqlx::query_as::<_, RoomDirectoryEntryRow>(
            r#"
            SELECT room_id, name, topic, avatar_url, canonical_alias,
                   join_rule, world_readable, guest_can_join, member_count
            FROM room_directory
            WHERE is_public = true
            ORDER BY member_count DESC, room_id ASC
            LIMIT $1 OFFSET $2
            "#,
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(RoomDirectoryEntry::from).collect())
    }

    async fn search_public_rooms(&self, filter: &str, limit: i64) -> Result<Vec<RoomDirectoryEntry>, sqlx::Error> {
        let pattern = format!("%{filter}%");
        let rows = sqlx::query_as::<_, RoomDirectoryEntryRow>(
            r#"
            SELECT room_id, name, topic, avatar_url, canonical_alias,
                   join_rule, world_readable, guest_can_join, member_count
            FROM room_directory
            WHERE is_public = true
              AND (COALESCE(name, '') ILIKE $1 OR COALESCE(topic, '') ILIKE $1)
            ORDER BY member_count DESC, room_id ASC
            LIMIT $2
            "#,
        )
        .bind(&pattern)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(RoomDirectoryEntry::from).collect())
    }
}

/// Internal row type for sqlx::query_as mapping.
#[derive(sqlx::FromRow)]
struct RoomDirectoryEntryRow {
    room_id: String,
    name: Option<String>,
    topic: Option<String>,
    avatar_url: Option<String>,
    canonical_alias: Option<String>,
    join_rule: String,
    world_readable: bool,
    guest_can_join: bool,
    member_count: i64,
}

impl From<RoomDirectoryEntryRow> for RoomDirectoryEntry {
    fn from(row: RoomDirectoryEntryRow) -> Self {
        Self {
            room_id: row.room_id,
            name: row.name,
            topic: row.topic,
            avatar_url: row.avatar_url,
            canonical_alias: row.canonical_alias,
            join_rule: row.join_rule,
            world_readable: row.world_readable,
            guest_can_join: row.guest_can_join,
            member_count: row.member_count,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_room_directory_entry_new_defaults() {
        let entry = RoomDirectoryEntry::new("!room:example.com");
        assert_eq!(entry.room_id, "!room:example.com");
        assert_eq!(entry.join_rule, "public");
        assert!(!entry.world_readable);
        assert!(!entry.guest_can_join);
        assert_eq!(entry.member_count, 0);
        assert!(entry.name.is_none());
        assert!(entry.topic.is_none());
        assert!(entry.avatar_url.is_none());
        assert!(entry.canonical_alias.is_none());
    }
}
