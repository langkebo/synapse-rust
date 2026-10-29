/// The `admin` module.
pub mod admin;
/// The `api` module.
pub(crate) mod api;
/// The `models` module.
pub(crate) mod models;

pub use api::RoomStoreApi;
pub use models::*;

// Room domain group — re-exports room-related storage modules under `room::`.
// Consumers should prefer `synapse_storage::room::RoomMemberStorage` over the
// flat `synapse_storage::RoomMemberStorage`.
pub use crate::membership::{MemberStoreApi, RoomMember, RoomMemberStorage, UserRoomMembership};
pub use crate::room_account_data::{RoomAccountDataRecord, RoomAccountDataStorage, RoomAccountDataStoreApi};
pub use crate::thread::{
    CreateThreadReplyParams, CreateThreadRootParams, ThreadListParams, ThreadReadReceipt, ThreadRelation, ThreadReply,
    ThreadRoot, ThreadStatistics, ThreadStorage, ThreadStoreApi, ThreadSubscription, ThreadSummary, ThreadWithReplies,
};

// P7.3: relations, retention, room_summary, room_tag, beacon, widget,
// burn_after_read, and friend_room are room-related storage modules — group
// them under `room::` so they are flat-re-exported via `pub use room::*;`
// rather than via explicit flat re-exports in lib.rs.
#[cfg(feature = "beacons")]
pub use crate::beacon::{
    BeaconInfo, BeaconInfoWithLocations, BeaconLocation, BeaconStorage, CreateBeaconInfoParams,
    CreateBeaconLocationParams,
};
#[cfg(feature = "burn-after-read")]
pub use crate::burn_after_read::*;
#[cfg(feature = "friends")]
pub use crate::friend_room::{
    AddFriendToGroupParams, CreateFriendGroupParams, DirectRoomFallbackLink, DmPartnerRecord, FriendDmLink,
    FriendRequestRecord, FriendRoomStorage, RemoveFriendFromGroupParams, RenameFriendGroupParams,
};
pub use crate::relations::*;
pub use crate::retention::*;
pub use crate::room_summary::*;
pub use crate::room_tag::*;
#[cfg(feature = "widgets")]
pub use crate::widget::{CreateWidgetParams, Widget, WidgetPermission, WidgetSession, WidgetStorage, WidgetStoreApi};

use serde_json::json;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use tracing;

use synapse_common::room_versions::DEFAULT_ROOM_VERSION;

impl RoomStorage {
    /// See [`encryption_from_is_encrypted`].
    pub(crate) fn encryption_from_is_encrypted(is_encrypted: Option<bool>) -> Option<String> {
        if is_encrypted.unwrap_or(false) {
            Some("m.megolm.v1.aes-sha2".to_string())
        } else {
            None
        }
    }

    /// B-7: Normalize a room alias by lowercasing the server_name portion.
    ///
    /// Per Matrix spec v1.11 § 4.3, the `server_name` part of a room alias is
    /// case-insensitive and is always lowercased before processing. The
    /// `localpart` is left as-is (spec-mandated case-sensitive).
    ///
    /// Example: `#Foo:Example.com` → `#Foo:example.com`
    pub(crate) fn normalize_alias_server(alias: &str) -> String {
        if let Some((local, server)) = alias.rsplit_once(':') {
            format!("{}:{}", local, server.to_ascii_lowercase())
        } else {
            // No `:` found; treat the whole string as localpart (edge case).
            alias.to_string()
        }
    }

    /// See [`new`].
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    /// 往 `rooms` 表落一条房间记录。
    ///
    /// # 契约（改动前先读）
    ///
    /// - **冲突必须上抛**：`room_id` 已被占用时返回一个**可判别**的错误（C88 起是
    ///   [`sqlx::Error::RowNotFound`]，来自 `ON CONFLICT (room_id) DO NOTHING` 的 0 行受影响），
    ///   绝不静默当成功。把它翻译成 409 `M_ROOM_IN_USE` 是服务层的职责，见
    ///   `synapse-services/src/room/lifecycle/create_events.rs` 的 `is_room_id_already_taken`。
    /// - **刻意不返回房间快照**：签名是 `Result<(), _>` 而非 `Result<Room, _>`。存储层只负责
    ///   "落库"；顺手构造一个 `Room` 返回，只能靠臆造字段（`member_count` / `created_ts` /
    ///   `is_federatable` …）拼出与真实行不一致的假数据，调用方读到的是幻觉。需要快照请读回
    ///   （[`Self::get_room`]）。
    ///
    /// 回归用例见 `db_tests::test_create_room_duplicate_room_id_is_a_distinguishable_conflict`。
    pub async fn create_room(
        &self,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<(), sqlx::Error> {
        Self::create_room_with_executor(&*self.pool, room_id, creator, join_rule, version, is_public).await
    }

    /// See [`create_room`] —— 契约完全相同（冲突上抛；不返回快照）。
    pub async fn create_room_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<(), sqlx::Error> {
        Self::create_room_with_executor(&mut **tx, room_id, creator, join_rule, version, is_public).await
    }

    /// 插入 `rooms` 行；**派生 id 已被占用时返回 [`sqlx::Error::RowNotFound`]**。
    ///
    /// ⚠️ C88：`ON CONFLICT (room_id) DO NOTHING` + `rows_affected()` 检查是**有意**的 ——
    /// v12 的 room_id 由 create 事件的 reference hash 决定（MSC4291），同一毫秒 + 同一内容的
    /// 并发 createRoom 会派生出同一个 id。此时**不能**当成功返回：调用方随后还要在同一事务里
    /// 写 `m.room.create` 事件与成员关系，而它们引用的房间行属于另一次创建（事件 id 也会撞主键）。
    /// 因此这里返回一个可判别的错误，由 `create_room_in_db` 转成显式 409（`M_ROOM_IN_USE`）。
    ///
    /// `#[tracing::instrument]` 而不是手写 `span.enter()`：`Entered` 守卫不是 `Send`，
    /// 跨 `.await` 持有会让整个 future 失去 `Send`（调用方多在 `tokio::spawn` 里）。
    #[tracing::instrument(
        skip_all,
        fields(room_id = %room_id, creator = %creator, join_rule = %join_rule, is_public = is_public)
    )]
    async fn create_room_with_executor<'a, E>(
        executor: E,
        room_id: &str,
        creator: &str,
        join_rule: &str,
        version: &str,
        is_public: bool,
    ) -> Result<(), sqlx::Error>
    where
        E: sqlx::Executor<'a, Database = Postgres>,
    {
        let started = std::time::Instant::now();
        let now = current_timestamp_millis();
        let inserted = sqlx::query!(
            r"
            INSERT INTO rooms (room_id, creator, join_rules, room_version, is_public, history_visibility, created_ts, last_activity_ts)
            VALUES ($1, $2, $3, $4, $5, 'joined', $6, $6)
            ON CONFLICT (room_id) DO NOTHING
            ",
            room_id,
            creator,
            join_rule,
            version,
            is_public,
            now
        )
        .execute(executor)
        .await?;

        if inserted.rows_affected() == 0 {
            // 这里**故意不**静默成功：0 行 = 该 room_id 已存在（并发/重试造成的派生 id 撞车）。
            tracing::warn!(
                room_id = %room_id,
                creator = %creator,
                elapsed_ms = %started.elapsed().as_millis(),
                "rooms INSERT 未插入任何行：派生的 room_id 已被占用"
            );
            return Err(sqlx::Error::RowNotFound);
        }

        // P2 埋点：房间创建（含事务内路径）的耗时，供 P50/P95/P99 观测（C88）。
        tracing::debug!(
            room_id = %room_id,
            creator = %creator,
            join_rule = %join_rule,
            is_public = is_public,
            elapsed_ms = %started.elapsed().as_millis(),
            "Room row created"
        );

        // 只承诺"落库"。这里曾顺手构造一份 `Room` 快照返回，但它的字段（`member_count: 1`、
        // `created_ts: now`、`is_federatable: true` …）全是臆造值，与真实行不一致；
        // 需要快照的调用方请用 `get_room` 读回。
        Ok(())
    }

    /// See [`get_room`].
    pub async fn get_room(&self, room_id: &str) -> Result<Option<Room>, sqlx::Error> {
        tracing::debug!(room_id = %room_id, "Querying room");
        let row = sqlx::query_as!(
            RoomRecord,
            r#"
            SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules AS join_rule, r.creator AS creator_user_id, r.room_version,
                  COALESCE(rs.member_count, joined.joined_members, 0) as member_count, rs.is_encrypted AS "is_encrypted?", r.is_public, r.history_visibility, r.created_ts
            FROM rooms r
            LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
            LEFT JOIN (
                SELECT room_id, COUNT(*)::BIGINT AS joined_members
                FROM room_memberships
                WHERE membership = 'join'
                GROUP BY room_id
            ) joined ON joined.room_id = r.room_id
            WHERE r.room_id = $1
            "#,
            room_id
        )
        .fetch_optional(&*self.pool)
        .await?;
        if let Some(row) = row {
            Ok(Some(Room {
                room_id: row.room_id,
                name: row.name,
                topic: row.topic,
                avatar_url: row.avatar_url,
                canonical_alias: row.canonical_alias,
                join_rule: row.join_rule.unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                creator_user_id: row.creator_user_id,
                room_version: row.room_version.unwrap_or_else(|| DEFAULT_ROOM_VERSION.to_string()),
                encryption: Self::encryption_from_is_encrypted(row.is_encrypted),
                is_public: row.is_public.unwrap_or(false),
                member_count: row.member_count.unwrap_or(0),
                history_visibility: row.history_visibility.unwrap_or_else(|| DEFAULT_HISTORY_VISIBILITY.to_string()),
                created_ts: row.created_ts,
            }))
        } else {
            tracing::warn!(room_id = %room_id, "Room not found");
            Ok(None)
        }
    }

    /// See [`get_rooms_batch`].
    pub async fn get_rooms_batch(&self, room_ids: &[String]) -> Result<Vec<Room>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows: Vec<RoomRecord> = sqlx::query_as!(
            RoomRecord,
            r#"
            SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules AS join_rule, r.creator AS creator_user_id, r.room_version,
                  r.is_public, rs.member_count as member_count, rs.is_encrypted AS "is_encrypted?", r.history_visibility, r.created_ts
            FROM rooms r
            LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
            WHERE r.room_id = ANY($1)
            "#,
            room_ids
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .iter()
            .map(|row| Room {
                room_id: row.room_id.clone(),
                name: row.name.clone(),
                topic: row.topic.clone(),
                avatar_url: row.avatar_url.clone(),
                canonical_alias: row.canonical_alias.clone(),
                join_rule: row.join_rule.clone().unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                creator_user_id: row.creator_user_id.clone(),
                room_version: row.room_version.clone().unwrap_or_else(|| DEFAULT_ROOM_VERSION.to_string()),
                encryption: Self::encryption_from_is_encrypted(row.is_encrypted),
                is_public: row.is_public.unwrap_or(false),
                member_count: row.member_count.unwrap_or(0),
                history_visibility: row
                    .history_visibility
                    .clone()
                    .unwrap_or_else(|| DEFAULT_HISTORY_VISIBILITY.to_string()),
                created_ts: row.created_ts,
            })
            .collect())
    }

    /// See [`get_room_creator`].
    pub async fn get_room_creator(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        let creator: Option<String> =
            sqlx::query_scalar!(r#"SELECT creator AS "creator!" FROM rooms WHERE room_id = $1"#, room_id)
                .fetch_optional(&*self.pool)
                .await?;
        Ok(creator)
    }

    /// See [`room_exists`].
    pub async fn room_exists(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(r#"SELECT 1 AS "exists" FROM rooms WHERE room_id = $1 LIMIT 1"#, room_id)
            .fetch_optional(&*self.pool)
            .await?;
        Ok(result.is_some())
    }

    /// See [`get_public_rooms`].
    pub async fn get_public_rooms(&self, limit: i64) -> Result<Vec<Room>, sqlx::Error> {
        self.get_public_rooms_paginated(limit, None, None, &crate::room::RoomTypeFilter::default()).await
    }

    /// Paginated public rooms list. Uses Keyset pagination (created_ts, room_id).
    pub async fn get_public_rooms_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_room_id: Option<&str>,
        types: &crate::room::RoomTypeFilter,
    ) -> Result<Vec<Room>, sqlx::Error> {
        let rows: Vec<RoomRecord> = if let (Some(ts), Some(room_id)) = (since_ts, since_room_id) {
            sqlx::query_as!(
                RoomRecord,
                r#"
                SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules AS join_rule, r.creator AS creator_user_id, r.room_version,
                      r.is_public, rs.member_count as member_count, rs.is_encrypted AS "is_encrypted?", r.history_visibility, r.created_ts
                FROM rooms r
                LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
                WHERE r.is_public = TRUE AND (r.created_ts < $2 OR (r.created_ts = $2 AND r.room_id < $3))
                  AND ($4::text[] IS NULL
                       OR rs.room_type = ANY($4)
                       OR ($5::boolean AND rs.room_type IS NULL))
                ORDER BY r.created_ts DESC, r.room_id DESC
                LIMIT $1
                "#,
                limit,
                ts,
                room_id,
                types.types.as_deref(),
                types.include_normal,
            )
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as!(
                RoomRecord,
                r#"
                SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules AS join_rule, r.creator AS creator_user_id, r.room_version,
                      r.is_public, rs.member_count as member_count, rs.is_encrypted AS "is_encrypted?", r.history_visibility, r.created_ts
                FROM rooms r
                LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
                WHERE r.is_public = TRUE
                  AND ($2::text[] IS NULL
                       OR rs.room_type = ANY($2)
                       OR ($3::boolean AND rs.room_type IS NULL))
                ORDER BY r.created_ts DESC, r.room_id DESC
                LIMIT $1
                "#,
                limit,
                types.types.as_deref(),
                types.include_normal,
            )
            .fetch_all(&*self.pool)
            .await?
        };
        Ok(rows
            .iter()
            .map(|row| Room {
                room_id: row.room_id.clone(),
                name: row.name.clone(),
                topic: row.topic.clone(),
                avatar_url: row.avatar_url.clone(),
                canonical_alias: row.canonical_alias.clone(),
                join_rule: row.join_rule.clone().unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                creator_user_id: row.creator_user_id.clone(),
                room_version: row.room_version.clone().unwrap_or_else(|| DEFAULT_ROOM_VERSION.to_string()),
                encryption: Self::encryption_from_is_encrypted(row.is_encrypted),
                is_public: row.is_public.unwrap_or(false),
                member_count: row.member_count.unwrap_or(0),
                history_visibility: row
                    .history_visibility
                    .clone()
                    .unwrap_or_else(|| DEFAULT_HISTORY_VISIBILITY.to_string()),
                created_ts: row.created_ts,
            })
            .collect())
    }

    /// Returns the total number of public rooms, for the `total_room_count_estimate` field.
    pub async fn count_public_rooms(&self, types: &crate::room::RoomTypeFilter) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!"
            FROM rooms r
            LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
            WHERE r.is_public = TRUE
              AND ($1::text[] IS NULL
                   OR rs.room_type = ANY($1)
                   OR ($2::boolean AND rs.room_type IS NULL))
            "#,
            types.types.as_deref(),
            types.include_normal,
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    /// See [`get_all_rooms_with_members`].
    pub async fn get_all_rooms_with_members(
        &self,
        limit: i64,
        from: Option<RoomSearchCursor>,
        order_by: RoomSearchOrder,
    ) -> Result<(Vec<(Room, i64)>, Option<String>), sqlx::Error> {
        let mut query_builder: sqlx::QueryBuilder<Postgres> = sqlx::QueryBuilder::new(
            r"
            SELECT r.room_id, r.name, r.topic, r.avatar_url, r.canonical_alias, r.join_rules, r.creator,
                r.room_version, r.is_public, rs.member_count as member_count, rs.is_encrypted as is_encrypted, r.history_visibility,
                r.created_ts, COUNT(rm.user_id) as joined_members
            FROM rooms r
            LEFT JOIN room_memberships rm ON r.room_id = rm.room_id AND rm.membership = 'join'
            LEFT JOIN room_summaries rs ON rs.room_id = r.room_id
            ",
        );

        query_builder.push(" WHERE 1 = 1 ");

        match (order_by, from) {
            (RoomSearchOrder::Created, Some(RoomSearchCursor::Created { created_ts, room_id })) => {
                query_builder.push(" AND (r.created_ts, r.room_id) < (");
                query_builder.push_bind(created_ts);
                query_builder.push(", ");
                query_builder.push_bind(room_id);
                query_builder.push(")");
            }
            (RoomSearchOrder::Name, Some(RoomSearchCursor::Name { name, created_ts, room_id })) => {
                query_builder.push(" AND (r.name, r.created_ts, r.room_id) < (");
                query_builder.push_bind(name);
                query_builder.push(", ");
                query_builder.push_bind(created_ts);
                query_builder.push(", ");
                query_builder.push_bind(room_id);
                query_builder.push(")");
            }
            (RoomSearchOrder::Size, Some(RoomSearchCursor::Size { member_count, created_ts, room_id })) => {
                query_builder.push(" AND (rs.member_count, r.created_ts, r.room_id) < (");
                query_builder.push_bind(member_count);
                query_builder.push(", ");
                query_builder.push_bind(created_ts);
                query_builder.push(", ");
                query_builder.push_bind(room_id);
                query_builder.push(")");
            }
            (_, None) => { /* No cursor */ }
            _ => { /* Mismatched cursor and order_by, treat as no cursor */ }
        };

        query_builder.push(" GROUP BY r.room_id, rs.member_count, rs.is_encrypted ");

        match order_by {
            RoomSearchOrder::Created => {
                query_builder.push(" ORDER BY r.created_ts DESC, r.room_id DESC");
            }
            RoomSearchOrder::Name => {
                query_builder.push(" ORDER BY r.name DESC, r.created_ts DESC, r.room_id DESC");
            }
            RoomSearchOrder::Size => {
                query_builder.push(" ORDER BY rs.member_count DESC, r.created_ts DESC, r.room_id DESC");
            }
        }

        query_builder.push(" LIMIT ");
        query_builder.push_bind(limit + 1); // Fetch one extra to check for next_batch

        let rows: Vec<RoomWithMembersRecord> = query_builder.build_query_as().fetch_all(&*self.pool).await?;

        // P5-fix: anchor the cursor to the LAST RETURNED row, not `get(limit)`.
        // The query fetches `LIMIT limit + 1` to detect a further page, and the
        // continue predicate is the strict `... < cursor`. Using the (limit+1)-th
        // row — which was never returned — made the next page skip it entirely
        // (5 rows with limit=2 yielded pages of 2 + 2 + 0). See P5 report §3.5.
        let next_batch = if rows.len() > limit as usize {
            rows.get(limit.saturating_sub(1) as usize).map(|last_room| {
                let cursor = match order_by {
                    RoomSearchOrder::Created => RoomSearchCursor::Created {
                        created_ts: last_room.created_ts,
                        room_id: last_room.room_id.clone(),
                    },
                    RoomSearchOrder::Name => RoomSearchCursor::Name {
                        name: last_room.name.clone(),
                        created_ts: last_room.created_ts,
                        room_id: last_room.room_id.clone(),
                    },
                    RoomSearchOrder::Size => RoomSearchCursor::Size {
                        member_count: last_room.member_count.unwrap_or(0),
                        created_ts: last_room.created_ts,
                        room_id: last_room.room_id.clone(),
                    },
                };
                encode_room_search_cursor(&cursor)
            })
        } else {
            None
        };

        let rooms = rows
            .into_iter()
            .take(limit as usize)
            .map(|row| {
                (
                    Room {
                        room_id: row.room_id.clone(),
                        name: row.name.clone(),
                        topic: row.topic.clone(),
                        avatar_url: row.avatar_url.clone(),
                        canonical_alias: row.canonical_alias.clone(),
                        join_rule: row.join_rule.clone().unwrap_or_else(|| DEFAULT_JOIN_RULE.to_string()),
                        creator_user_id: row.creator_user_id.clone(),
                        room_version: row.room_version.clone().unwrap_or_else(|| DEFAULT_ROOM_VERSION.to_string()),
                        encryption: Self::encryption_from_is_encrypted(row.is_encrypted),
                        is_public: row.is_public.unwrap_or(false),
                        member_count: row.member_count.unwrap_or(0),
                        history_visibility: row
                            .history_visibility
                            .clone()
                            .unwrap_or_else(|| DEFAULT_HISTORY_VISIBILITY.to_string()),
                        created_ts: row.created_ts,
                    },
                    row.joined_members.unwrap_or(0),
                )
            })
            .collect();
        Ok((rooms, next_batch))
    }

    /// See [`get_user_rooms`].
    pub async fn get_user_rooms(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows: Vec<String> = sqlx::query_scalar!(
            r"
            SELECT room_id FROM room_memberships WHERE user_id = $1 AND membership = 'join'
            LIMIT 1000
            ",
            user_id
        )
        .fetch_all(&*self.pool)
        .await?;
        Ok(rows)
    }

    /// See [`search_rooms_for_user`].
    pub async fn search_rooms_for_user(
        &self,
        user_id: &str,
        search_pattern: &str,
        limit: i64,
    ) -> Result<Vec<(String, Option<String>, Option<String>, Option<String>, bool)>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT room_id, name, topic, avatar_url, is_public AS "is_public!"
            FROM rooms
            WHERE
                (name ILIKE $1 OR topic ILIKE $1)
                AND (
                    is_public = true
                    OR EXISTS (
                        SELECT 1
                        FROM room_memberships
                        WHERE room_memberships.room_id = rooms.room_id
                          AND room_memberships.user_id = $2
                          AND room_memberships.membership = 'join'
                    )
                )
            ORDER BY name
            LIMIT $3
            "#,
            search_pattern,
            user_id,
            limit
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| (r.room_id, r.name, r.topic, r.avatar_url, r.is_public)).collect())
    }

    /// See [`get_user_room_list_summary`].
    pub async fn get_user_room_list_summary(
        &self,
        user_id: &str,
    ) -> Result<Vec<(String, String, String, String)>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT rm.room_id, rm.membership,
                   COALESCE(r.name, '') AS "name!",
                   COALESCE(r.avatar_url, '') AS "avatar_url!"
            FROM room_memberships rm
            LEFT JOIN rooms r ON rm.room_id = r.room_id
            WHERE rm.user_id = $1
            ORDER BY rm.updated_ts DESC
            "#,
            user_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| (r.room_id, r.membership, r.name, r.avatar_url)).collect())
    }

    /// See [`get_user_rooms_paginated`].
    pub async fn get_user_rooms_paginated(
        &self,
        user_id: &str,
        limit: i64,
        from_room_id: Option<&str>,
    ) -> Result<Vec<String>, sqlx::Error> {
        if let Some(room_id) = from_room_id {
            sqlx::query_scalar!(
                r"
                SELECT room_id FROM room_memberships
                WHERE user_id = $1 AND membership = 'join' AND room_id > $2
                ORDER BY room_id ASC
                LIMIT $3
                ",
                user_id,
                room_id,
                limit
            )
            .fetch_all(&*self.pool)
            .await
        } else {
            sqlx::query_scalar!(
                r"
                SELECT room_id FROM room_memberships
                WHERE user_id = $1 AND membership = 'join'
                ORDER BY room_id ASC
                LIMIT $2
                ",
                user_id,
                limit
            )
            .fetch_all(&*self.pool)
            .await
        }
    }

    /// See [`update_room_name`].
    pub async fn update_room_name(&self, room_id: &str, name: &str) -> Result<(), sqlx::Error> {
        Self::update_room_name_with_executor(&*self.pool, room_id, name).await
    }

    /// See [`update_room_name_in_tx`].
    pub async fn update_room_name_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        room_id: &str,
        name: &str,
    ) -> Result<(), sqlx::Error> {
        Self::update_room_name_with_executor(&mut **tx, room_id, name).await
    }

    async fn update_room_name_with_executor<'a, E>(executor: E, room_id: &str, name: &str) -> Result<(), sqlx::Error>
    where
        E: sqlx::Executor<'a, Database = Postgres>,
    {
        sqlx::query!(
            r"
            UPDATE rooms SET name = $1 WHERE room_id = $2
            ",
            name,
            room_id
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    /// See [`update_room_topic`].
    pub async fn update_room_topic(&self, room_id: &str, topic: &str) -> Result<(), sqlx::Error> {
        Self::update_room_topic_with_executor(&*self.pool, room_id, topic).await
    }

    /// See [`update_room_topic_in_tx`].
    pub async fn update_room_topic_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        room_id: &str,
        topic: &str,
    ) -> Result<(), sqlx::Error> {
        Self::update_room_topic_with_executor(&mut **tx, room_id, topic).await
    }

    async fn update_room_topic_with_executor<'a, E>(executor: E, room_id: &str, topic: &str) -> Result<(), sqlx::Error>
    where
        E: sqlx::Executor<'a, Database = Postgres>,
    {
        sqlx::query!(
            r"
            UPDATE rooms SET topic = $1 WHERE room_id = $2
            ",
            topic,
            room_id
        )
        .execute(executor)
        .await?;
        Ok(())
    }

    /// See [`update_room_avatar`].
    pub async fn update_room_avatar(&self, room_id: &str, avatar_url: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            UPDATE rooms SET avatar_url = $1 WHERE room_id = $2
            ",
            avatar_url,
            room_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`set_canonical_alias`].
    pub async fn set_canonical_alias(&self, room_id: &str, alias: Option<&str>) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            UPDATE rooms SET canonical_alias = $1 WHERE room_id = $2
            ",
            alias,
            room_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`update_join_rule_in_tx`].
    pub async fn update_join_rule_in_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, Postgres>,
        room_id: &str,
        join_rule: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            UPDATE rooms SET join_rules = $1 WHERE room_id = $2
            ",
            join_rule,
            room_id
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// See [`update_canonical_alias`].
    pub async fn update_canonical_alias(&self, room_id: &str, alias: &str) -> Result<(), sqlx::Error> {
        self.set_canonical_alias(room_id, Some(alias)).await
    }

    /// See [`increment_member_count`].
    pub async fn increment_member_count(
        &self,
        room_id: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<(), sqlx::Error> {
        // v11: removed `joined_member_count = joined_member_count + 1` and
        // `member_count = member_count + 1` updates. These counts are now
        // maintained by the `trg_sync_member_count` trigger on
        // `room_memberships`. We only update `updated_ts` here.
        // When `tx` is provided the UPDATE runs in the caller's transaction
        // (used by join so the summary refresh stays atomic with the
        // add_member write).
        let query = sqlx::query!(
            r"
            UPDATE room_summaries
            SET updated_ts = $2
            WHERE room_id = $1
            ",
            room_id,
            current_timestamp_millis()
        );
        if let Some(tx) = tx {
            query.execute(&mut **tx).await?;
        } else {
            query.execute(&*self.pool).await?;
        }
        Ok(())
    }

    /// See [`decrement_member_count`].
    pub async fn decrement_member_count(
        &self,
        room_id: &str,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<(), sqlx::Error> {
        // v11: see `increment_member_count` for the trigger-based design.
        // When `tx` is provided the UPDATE runs in the caller's transaction
        // (used by MSC4267 leave+forget so the count decrement stays atomic
        // with the remove_member + forget_member writes).
        let query = sqlx::query!(
            r"
            UPDATE room_summaries
            SET updated_ts = $2
            WHERE room_id = $1
            ",
            room_id,
            current_timestamp_millis()
        );
        if let Some(tx) = tx {
            query.execute(&mut **tx).await?;
        } else {
            query.execute(&*self.pool).await?;
        }
        Ok(())
    }

    /// See [`get_room_count`].
    pub async fn get_room_count(&self) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar!(r#"SELECT COALESCE(COUNT(*), 0) AS "count!" FROM rooms"#)
            .fetch_one(&*self.pool)
            .await?;
        Ok(count)
    }

    /// See [`set_room_visibility`].
    pub async fn set_room_visibility(&self, room_id: &str, visibility: &str) -> Result<(), sqlx::Error> {
        // Normalize the visibility string and derive the boolean is_public
        // flag from it. `is_public` is what most callers actually read, so we
        // keep it in sync with the visibility column.
        let (visibility_value, is_public) = match visibility {
            "public" => ("public", true),
            "private" => ("private", false),
            _ => ("private", false),
        };
        sqlx::query!(
            r"
            UPDATE rooms SET visibility = $1, is_public = $2 WHERE room_id = $3
            ",
            visibility_value,
            is_public,
            room_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`set_room_alias`].
    pub async fn set_room_alias(&self, room_id: &str, alias: &str, _created_by: &str) -> Result<(), sqlx::Error> {
        let creation_ts = current_timestamp_millis();
        // B-7: Lowercase the server_name portion of the alias so that
        // `#Foo:Example.com` and `#foo:example.com` collide on the same row.
        // Matches Matrix spec v1.11 § 4.3 and element-hq/synapse's
        // `RoomAlias.create` behavior. The `localpart` is left untouched.
        let normalized_alias = Self::normalize_alias_server(alias);
        let server_name: String = normalized_alias
            .rsplit_once(':')
            .map(|(_, s)| s.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "localhost".to_string());
        // ELEMENTSEC-2026-1729: an alias that already points at another room must
        // never be silently repointed. `DO NOTHING` turns the clash into a
        // *detectable* conflict — zero rows affected — which the caller maps to
        // 409 `M_ROOM_IN_USE` instead of moving the alias out from under its
        // current room.
        let result = sqlx::query!(
            r"
            INSERT INTO room_aliases (room_alias, room_id, server_name, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (room_alias) DO NOTHING
            ",
            normalized_alias,
            room_id,
            server_name,
            creation_ts
        )
        .execute(&*self.pool)
        .await?;
        if result.rows_affected() == 0 {
            tracing::warn!(
                alias = %normalized_alias,
                room_id,
                "refusing to repoint room alias already bound to another room"
            );
            return Err(sqlx::Error::RowNotFound);
        }
        Ok(())
    }

    /// See [`remove_room_alias`].
    pub async fn remove_room_alias(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            DELETE FROM room_aliases WHERE room_id = $1
            ",
            room_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`remove_room_alias_by_name`].
    pub async fn remove_room_alias_by_name(&self, alias: &str) -> Result<(), sqlx::Error> {
        // B-7: Normalize the alias before deleting so that
        // `remove_room_alias_by_name("#foo:EXAMPLE.com")` still removes the
        // canonical row stored under `#foo:example.com`.
        let normalized = RoomStorage::normalize_alias_server(alias);
        sqlx::query!(
            r"
            DELETE FROM room_aliases WHERE room_alias = $1
            ",
            normalized
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Delete a room and all its events.
    ///
    /// DB-04-b: replaces the old `DELETE FROM rooms WHERE room_id = $1` which
    /// relied on the `events.room_id ON DELETE CASCADE` FK to cascade-delete
    /// events. CASCADE would acquire an AccessExclusiveLock on the entire
    /// `events` table for the duration of the delete, blocking all concurrent
    /// reads and writes to events while a large room was being deleted.
    ///
    /// The replacement strategy:
    ///
    /// 1. Open a single transaction.
    /// 2. In batches of 1000 events, `DELETE FROM events WHERE room_id = $1 LIMIT 1000`
    ///    until 0 rows are affected. Each batch holds only row-level locks,
    ///    not the table-wide AccessExclusiveLock.
    /// 3. The CASCADE FKs on the 32 other tables (room_memberships,
    ///    room_aliases, room_summaries, etc.) remain and clean up small
    ///    auxiliary tables in the same transaction — their row counts are
    ///    small enough that this is not a hot spot.
    /// 4. `DELETE FROM rooms WHERE room_id = $1` — this is now the last
    ///    operation; the FK constraint (now `ON DELETE NO ACTION`) will
    ///    pass because all events for this room are already gone.
    ///
    /// Why 1000? Empirically chosen: small enough that each batch's locks
    /// are held for <100ms even on warm cache, large enough that the
    /// loop completes in O(N/1000) round trips where N is the room's
    /// event count. For a 1M-event room this is ~1000 round trips,
    /// well under any connection timeout.
    ///
    /// Returns the total number of events deleted (for logging / metrics).
    pub async fn delete_room(&self, room_id: &str) -> Result<u64, sqlx::Error> {
        tracing::info!(room_id = %room_id, "Deleting room with batched event cleanup");

        let mut tx = self.pool.begin().await?;

        // Step 1: Batch-delete events for this room in chunks of 1000.
        let mut total_events_deleted: u64 = 0;
        let batch_size: i64 = 1000;

        loop {
            let result = sqlx::query!(
                r"
                DELETE FROM events
                WHERE event_id IN (
                    SELECT event_id FROM events
                    WHERE room_id = $1
                    LIMIT $2
                )
                ",
                room_id,
                batch_size
            )
            .execute(&mut *tx)
            .await?;

            let rows = result.rows_affected();
            if rows == 0 {
                break;
            }
            total_events_deleted += rows;

            tracing::debug!(
                room_id = %room_id,
                batch_deleted = rows,
                total_deleted = total_events_deleted,
                "Deleted batch of events"
            );

            // Safety valve: avoid infinite loops if the predicate changes
            // mid-loop (extremely defensive — shouldn't happen).
            if total_events_deleted > 100_000_000 {
                tracing::error!(
                    room_id = %room_id,
                    total_deleted = total_events_deleted,
                    "Safety valve: aborting after 100M events deleted"
                );
                tx.rollback().await?;
                return Err(sqlx::Error::Protocol(format!(
                    "delete_room aborted after 100M events deleted (room_id={room_id})"
                )));
            }
        }

        // Step 2: CASCADE-cleanup of small auxiliary tables (memberships,
        // aliases, summaries, etc.) happens automatically because their
        // ON DELETE CASCADE FKs are still in place.

        // Step 3: Now delete the room itself. This will succeed because
        // all events for this room have been removed (the FK is NO ACTION,
        // which is satisfied since no child rows exist).
        let room_result = sqlx::query!(r"DELETE FROM rooms WHERE room_id = $1", room_id).execute(&mut *tx).await?;

        if room_result.rows_affected() == 0 {
            // Room did not exist. Roll back to avoid leaving the transaction
            // half-open in case downstream callers rely on existence.
            tx.rollback().await?;
            tracing::warn!(
                room_id = %room_id,
                total_events_deleted,
                "Room not found during delete (rolled back batch)"
            );
            return Ok(total_events_deleted);
        }

        tx.commit().await?;

        tracing::info!(
            room_id = %room_id,
            total_events_deleted,
            "Room deleted with batched event cleanup"
        );

        Ok(total_events_deleted)
    }

    /// See [`shutdown_room`].
    pub async fn shutdown_room(&self, room_id: &str) -> Result<(), sqlx::Error> {
        tracing::info!(room_id = %room_id, "Shutting down room");
        // Mark room as inactive or delete it. For simplicity, we delete it from directory
        // and mark its name to indicate it's shutdown.
        sqlx::query!(
            "UPDATE rooms SET is_public = false, name = COALESCE(name, '') || ' (SHUTDOWN)' WHERE room_id = $1",
            room_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`set_room_version`].
    pub async fn set_room_version(&self, room_id: &str, version: &str) -> Result<(), sqlx::Error> {
        sqlx::query!("UPDATE rooms SET room_version = $1 WHERE room_id = $2", version, room_id)
            .execute(&*self.pool)
            .await?;
        Ok(())
    }

    /// See [`get_room_alias`].
    pub async fn get_room_alias(&self, room_id: &str) -> Result<Option<String>, sqlx::Error> {
        let alias: Option<String> =
            sqlx::query_scalar!(r"SELECT room_alias FROM room_aliases WHERE room_id = $1 LIMIT 1", room_id)
                .fetch_optional(&*self.pool)
                .await?;
        Ok(alias)
    }

    /// See [`get_room_aliases`].
    pub async fn get_room_aliases(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let aliases: Vec<String> =
            sqlx::query_scalar!(r"SELECT room_alias FROM room_aliases WHERE room_id = $1", room_id)
                .fetch_all(&*self.pool)
                .await?;
        Ok(aliases)
    }

    /// See [`get_room_by_alias`].
    pub async fn get_room_by_alias(&self, alias: &str) -> Result<Option<String>, sqlx::Error> {
        // B-7: Normalize the alias so that `#foo:example.com` queries match rows
        // stored under `#Foo:EXAMPLE.com` (and vice versa).
        let normalized = RoomStorage::normalize_alias_server(alias);
        let room_id: Option<String> =
            sqlx::query_scalar!(r"SELECT room_id FROM room_aliases WHERE room_alias = $1", normalized)
                .fetch_optional(&*self.pool)
                .await?;
        Ok(room_id)
    }

    /// See [`is_room_in_directory`].
    pub async fn is_room_in_directory(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let result: Option<bool> =
            sqlx::query_scalar!(r#"SELECT is_public AS "is_public!" FROM room_directory WHERE room_id = $1"#, room_id)
                .fetch_optional(&*self.pool)
                .await?;
        Ok(result.is_some_and(|v| v))
    }

    /// See [`set_room_directory`].
    ///
    /// `room_directory` is the authoritative membership set of **public** rooms:
    /// a public room has a row, a private room has none. This keeps the standard
    /// visibility path (`PUT /directory/list/room/{id}`) consistent with the admin
    /// path (`set_room_private_with_directory`, which deletes the row), so that an
    /// `EXISTS (SELECT 1 FROM room_directory ...)` check reports the same
    /// "in directory" state regardless of which write path was used. Setting the
    /// room private therefore deletes the directory row rather than upserting
    /// `is_public = false`.
    pub async fn set_room_directory(&self, room_id: &str, is_public: bool) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        if is_public {
            sqlx::query!(
                r"
                INSERT INTO room_directory (room_id, is_public, added_ts)
                VALUES ($1, TRUE, $2)
                ON CONFLICT (room_id) DO UPDATE SET is_public = TRUE
                ",
                room_id,
                now
            )
            .execute(&*self.pool)
            .await?;
        } else {
            // 复用既有的 `remove_room_directory`（铁律 2：同一职责只允许一份实现），
            // 避免为同一句 DELETE 维护第二份 SQL。
            self.remove_room_directory(room_id).await?;
        }

        sqlx::query!(
            r"
            UPDATE rooms SET is_public = $1 WHERE room_id = $2
            ",
            is_public,
            room_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`remove_room_directory`].
    pub async fn remove_room_directory(&self, room_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r"
            DELETE FROM room_directory WHERE room_id = $1
            ",
            room_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Look up the `origin_server_ts` of an event by id. Returns `None` if the
    /// event does not exist (e.g. already purged). Used to populate the
    /// redundant `read_markers.origin_server_ts` column so that
    /// `get_unread_counts` can recover `last_read_ts` after `purge_history`
    /// deletes the referenced event (P1-7).
    async fn lookup_event_origin_server_ts(&self, event_id: &str) -> Result<Option<i64>, sqlx::Error> {
        let ts = sqlx::query_scalar!("SELECT origin_server_ts FROM events WHERE event_id = $1", event_id)
            .fetch_optional(&*self.pool)
            .await?;
        Ok(ts)
    }

    /// See [`update_read_marker`].
    pub async fn update_read_marker(&self, room_id: &str, user_id: &str, event_id: &str) -> Result<(), sqlx::Error> {
        let now: i64 = current_timestamp_millis();
        // P1-7: cache origin_server_ts so get_unread_counts survives purge_history
        let event_ts = self.lookup_event_origin_server_ts(event_id).await?;
        sqlx::query!(
            r"
            INSERT INTO read_markers (room_id, user_id, event_id, marker_type, created_ts, updated_ts, origin_server_ts)
            VALUES ($1, $2, $3, 'm.fully_read', $4, $4, $5)
            ON CONFLICT (room_id, user_id, marker_type) DO UPDATE SET
                event_id = EXCLUDED.event_id,
                updated_ts = EXCLUDED.updated_ts,
                origin_server_ts = EXCLUDED.origin_server_ts
            ",
            room_id,
            user_id,
            event_id,
            now,
            event_ts
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// Update read marker with specific marker type
    /// Supports: m.fully_read, m.read, m.read.private, m.marked_unread
    pub async fn update_read_marker_with_type(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        marker_type: &str,
    ) -> Result<(), sqlx::Error> {
        let mut conn = self.pool.acquire().await?;
        Self::upsert_read_marker(&mut conn, room_id, user_id, event_id, marker_type).await
    }

    /// Upsert a read marker on a caller-provided connection/transaction.
    ///
    /// Factored out of [`update_read_marker_with_type`] so that
    /// [`update_read_marker_monotonic`] can perform its read-check-write
    /// sequence atomically inside a single transaction + row lock (CONC-01).
    async fn upsert_read_marker(
        conn: &mut sqlx::PgConnection,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        marker_type: &str,
    ) -> Result<(), sqlx::Error> {
        let now: i64 = current_timestamp_millis();
        // P1-7: cache origin_server_ts so get_unread_counts survives purge_history
        let event_ts = sqlx::query_scalar!("SELECT origin_server_ts FROM events WHERE event_id = $1", event_id)
            .fetch_optional(&mut *conn)
            .await?;
        sqlx::query!(
            r"
            INSERT INTO read_markers (room_id, user_id, event_id, marker_type, created_ts, updated_ts, origin_server_ts)
            VALUES ($1, $2, $3, $4, $5, $5, $6)
            ON CONFLICT (room_id, user_id, marker_type) DO UPDATE SET
                event_id = EXCLUDED.event_id,
                updated_ts = EXCLUDED.updated_ts,
                origin_server_ts = EXCLUDED.origin_server_ts
            ",
            room_id,
            user_id,
            event_id,
            marker_type,
            now,
            event_ts
        )
        .execute(&mut *conn)
        .await?;
        Ok(())
    }

    /// Get read marker for a specific type
    pub async fn get_read_marker(
        &self,
        room_id: &str,
        user_id: &str,
        marker_type: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r"
            SELECT event_id FROM read_markers
            WHERE room_id = $1 AND user_id = $2 AND marker_type = $3
            ",
            room_id,
            user_id,
            marker_type
        )
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result)
    }

    /// MSC4446: Update read marker with monotonicity check.
    ///
    /// When `allow_backward` is `false` (the default per MSC4446), the marker
    /// is only updated if the new event is at the same position or more
    /// recent than the current marker's event. Backward moves are silently
    /// dropped (return `false`, no update, HTTP 200).
    ///
    /// When `allow_backward` is `true`, backward moves are accepted **only**
    /// for `m.fully_read` markers. Other marker types (e.g. `m.read`) always
    /// enforce monotonicity regardless of the flag.
    ///
    /// Event ordering uses `stream_ordering` when both events have it;
    /// otherwise falls back to `origin_server_ts`.
    ///
    /// Returns `true` if the marker was updated, `false` if silently dropped.
    ///
    /// CONC-01: the read-check-write sequence runs inside a single transaction
    /// with a `SELECT ... FOR UPDATE` row lock on the marker, so concurrent
    /// calls for the same `(room_id, user_id, marker_type)` cannot
    /// interleave and lose a forward update (or wrongly accept a backward one).
    pub async fn update_read_marker_monotonic(
        &self,
        room_id: &str,
        user_id: &str,
        event_id: &str,
        marker_type: &str,
        allow_backward: bool,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        // Lock the marker row (if any) for the duration of the transaction.
        // A missing row is not locked; the subsequent upsert's ON CONFLICT
        // resolves the insertion race between concurrent writers.
        let current_event_id = sqlx::query_scalar!(
            r"
            SELECT event_id FROM read_markers
            WHERE room_id = $1 AND user_id = $2 AND marker_type = $3
            FOR UPDATE
            ",
            room_id,
            user_id,
            marker_type
        )
        .fetch_optional(&mut *tx)
        .await?;

        match current_event_id {
            None => {
                // No existing marker → allow update
                Self::upsert_read_marker(&mut tx, room_id, user_id, event_id, marker_type).await?;
                tx.commit().await?;
                Ok(true)
            }
            Some(current_id) => {
                if current_id == event_id {
                    // Same event → idempotent, no update needed
                    return Ok(true);
                }

                // Fetch event positions for comparison
                let new_pos =
                    sqlx::query!(r"SELECT stream_ordering, origin_server_ts FROM events WHERE event_id = $1", event_id)
                        .fetch_optional(&mut *tx)
                        .await?
                        .map(|r| (r.stream_ordering, r.origin_server_ts));

                let current_pos = sqlx::query!(
                    r"SELECT stream_ordering, origin_server_ts FROM events WHERE event_id = $1",
                    current_id.as_str()
                )
                .fetch_optional(&mut *tx)
                .await?
                .map(|r| (r.stream_ordering, r.origin_server_ts));

                // If either event is missing from the store, fail-open (allow update)
                let (new_pos, current_pos) = match (new_pos, current_pos) {
                    (Some(n), Some(c)) => (n, c),
                    _ => {
                        Self::upsert_read_marker(&mut tx, room_id, user_id, event_id, marker_type).await?;
                        tx.commit().await?;
                        return Ok(true);
                    }
                };

                // Determine if this is a backward move
                let is_backward = match (current_pos.0, new_pos.0) {
                    (Some(curr_so), Some(new_so)) => new_so < curr_so,
                    _ => new_pos.1 < current_pos.1, // fallback to origin_server_ts
                };

                if !is_backward || (allow_backward && marker_type == "m.fully_read") {
                    // Forward/equal move, or MSC4446 backward move for m.fully_read
                    Self::upsert_read_marker(&mut tx, room_id, user_id, event_id, marker_type).await?;
                    tx.commit().await?;
                    Ok(true)
                } else {
                    // Backward move not allowed → silently drop (release lock)
                    tx.rollback().await?;
                    Ok(false)
                }
            }
        }
    }

    /// Get all read markers for a user in a room
    pub async fn get_all_read_markers(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> Result<std::collections::HashMap<String, String>, sqlx::Error> {
        let rows = sqlx::query!(
            r"
            SELECT marker_type, event_id FROM read_markers
            WHERE room_id = $1 AND user_id = $2
            ",
            room_id,
            user_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| (r.marker_type, r.event_id)).collect())
    }

    /// See [`add_receipt`].
    pub async fn add_receipt(
        &self,
        user_id: &str,
        _sent_to: &str,
        room_id: &str,
        event_id: &str,
        receipt_type: &str,
        data: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        let now: i64 = current_timestamp_millis();
        let receipt_data = if data.is_object() { data.clone() } else { json!({}) };

        // STO-02: DELETE + INSERT 必须在同一事务中，否则 DELETE 成功而 INSERT
        // 失败时用户已读状态被静默清空
        let mut tx = self.pool.begin().await?;

        sqlx::query!(
            r"
            DELETE FROM event_receipts
            WHERE room_id = $1
              AND user_id = $2
              AND receipt_type = $3
              AND event_id <> $4
            ",
            room_id,
            user_id,
            receipt_type,
            event_id
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            r"
            INSERT INTO event_receipts (event_id, room_id, user_id, receipt_type, ts, data, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $5, $5)
            ON CONFLICT (event_id, room_id, user_id, receipt_type) DO UPDATE
            SET ts = EXCLUDED.ts, data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
            ",
            event_id,
            room_id,
            user_id,
            receipt_type,
            now,
            &receipt_data
        )
        .execute(&mut *tx)
        .await?;

        // 上面的当前状态行不能当流位置用（更新会重写同一行、`id` 不前移），
        // 因此每次变更都同时往日志表追加一行。必须与状态行同一事务：状态表可见
        // 而日志缺失的回执，会让所有桥接永久看不到这次变更。
        sqlx::query!(
            r"
            INSERT INTO event_receipts_stream (room_id, event_id, user_id, receipt_type, ts, data, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $5)
            ",
            room_id,
            event_id,
            user_id,
            receipt_type,
            now,
            &receipt_data
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    /// See [`get_receipts`].
    pub async fn get_receipts(
        &self,
        room_id: &str,
        receipt_type: &str,
        event_id: &str,
    ) -> Result<Vec<Receipt>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT user_id, event_id, receipt_type, ts, data AS "data!" FROM event_receipts
            WHERE room_id = $1 AND receipt_type = $2 AND event_id = $3
            "#,
            room_id,
            receipt_type,
            event_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|r| Receipt {
                user_id: r.user_id,
                event_id: r.event_id,
                receipt_type: r.receipt_type,
                ts: r.ts,
                data: r.data,
            })
            .collect())
    }

    /// Returns the current head of the receipt stream log (max `stream_id`), or 0
    /// when no receipt exists. Used as the `new_token` upper bound for MSC2409
    /// appservice ephemeral (receipt) delivery.
    ///
    /// This must read `event_receipts_stream`, not `event_receipts`: the
    /// current-state table rewrites a row in place on update without moving its
    /// `id`, so a cursor over it would miss every re-receipt of the same
    /// `(event_id, room_id, user_id, receipt_type)`.
    pub async fn get_latest_receipt_position(&self) -> Result<i64, sqlx::Error> {
        let position: Option<i64> =
            sqlx::query_scalar!(r#"SELECT COALESCE(MAX(stream_id), 0) AS "max!" FROM event_receipts_stream"#)
                .fetch_optional(&*self.pool)
                .await?;

        Ok(position.unwrap_or(0))
    }

    /// Fetches the page of receipt-stream rows in `(from_exclusive, to_inclusive]`,
    /// ordered ascending, capped at `limit`. Mirrors upstream Synapse's
    /// `_handle_receipts` paging (bounded batch → `reached_token`).
    pub async fn get_receipts_since(
        &self,
        from_exclusive: i64,
        to_inclusive: i64,
        limit: i64,
    ) -> Result<Vec<ReceiptStreamEntry>, sqlx::Error> {
        sqlx::query_as!(
            ReceiptStreamEntry,
            r#"
            SELECT stream_id, room_id, user_id, event_id, receipt_type, ts, data
            FROM event_receipts_stream
            WHERE stream_id > $1 AND stream_id <= $2
            ORDER BY stream_id ASC
            LIMIT $3
            "#,
            from_exclusive,
            to_inclusive,
            limit
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// 注销清理：删除该用户在所有房间的回执。
    ///
    /// 两张表必须同事务：`event_receipts` 是当前态，`event_receipts_stream` 是
    /// MSC2409 的递增日志；只删前者会让 appservice 继续从日志里读到已注销用户的
    /// 回执。返回 `(event_receipts, event_receipts_stream)` 分表计数，供审计日志
    /// 指出是哪张表仍有残留。
    pub async fn delete_all_receipts_for_user(&self, user_id: &str) -> Result<(u64, u64), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        let receipts = sqlx::query!("DELETE FROM event_receipts WHERE user_id = $1", user_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();

        let stream = sqlx::query!("DELETE FROM event_receipts_stream WHERE user_id = $1", user_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();

        tx.commit().await?;
        Ok((receipts, stream))
    }

    /// See [`get_rooms_map`].
    pub async fn get_rooms_map(
        &self,
        room_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Room>, sqlx::Error> {
        if room_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let rooms = self.get_rooms_batch(room_ids).await?;

        Ok(rooms.into_iter().map(|r| (r.room_id.clone(), r)).collect())
    }
}

#[cfg(test)]
mod db_tests;

#[cfg(test)]
mod api_dyn_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encryption_from_is_encrypted_true() {
        let result = RoomStorage::encryption_from_is_encrypted(Some(true));
        assert_eq!(result, Some("m.megolm.v1.aes-sha2".to_string()));
    }

    #[test]
    fn encryption_from_is_encrypted_false() {
        let result = RoomStorage::encryption_from_is_encrypted(Some(false));
        assert_eq!(result, None);
    }

    #[test]
    fn encryption_from_is_encrypted_none() {
        let result = RoomStorage::encryption_from_is_encrypted(None);
        assert_eq!(result, None);
    }
}
