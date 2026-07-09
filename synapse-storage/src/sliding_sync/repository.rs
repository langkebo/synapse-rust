use super::models::*;
use sqlx::{Pool, Postgres, QueryBuilder};
use std::sync::Arc;

#[derive(Clone)]
pub struct SlidingSyncStorage {
    pool: Arc<Pool<Postgres>>,
}

impl SlidingSyncStorage {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    pub async fn create_or_update_token(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<SlidingSyncToken, sqlx::Error> {
        self.ensure_schema()?;
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = now + 7 * 24 * 3600 * 1000;

        let token = uuid::Uuid::new_v4().to_string();

        sqlx::query_as::<_, SlidingSyncToken>(
            r"
            INSERT INTO sliding_sync_tokens (user_id, device_id, token, conn_id, pos, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, nextval('sliding_sync_pos_seq'), $5, $6)
            ON CONFLICT (user_id, device_id, COALESCE(conn_id, ''::text)) DO UPDATE SET
                pos = nextval('sliding_sync_pos_seq'),
                expires_at = EXCLUDED.expires_at
            RETURNING *
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(&token)
        .bind(conn_id)
        .bind(now)
        .bind(expires_at)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_token(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncToken>, sqlx::Error> {
        self.ensure_schema()?;
        sqlx::query_as::<_, SlidingSyncToken>(
            r"
            SELECT id, user_id, device_id, conn_id, token, pos, created_ts, expires_at FROM sliding_sync_tokens
            WHERE user_id = $1 AND device_id = $2 AND (conn_id = $3 OR ($3 IS NULL AND conn_id IS NULL))
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(conn_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn validate_pos(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        pos: &str,
    ) -> Result<bool, sqlx::Error> {
        self.ensure_schema()?;
        let result: Option<(bool,)> = sqlx::query_as(
            r"
            SELECT (pos = $4) FROM sliding_sync_tokens
            WHERE user_id = $1 AND device_id = $2 AND (conn_id = $3 OR ($3 IS NULL AND conn_id IS NULL))
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(conn_id)
        .bind(pos.parse::<i64>().unwrap_or(0))
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.is_some_and(|r| r.0))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn save_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        sort: &[String],
        filters: Option<&SlidingSyncFilters>,
        room_subscription: Option<&serde_json::Value>,
        ranges: &[(u32, u32)],
    ) -> Result<SlidingSyncList, sqlx::Error> {
        self.ensure_schema()?;
        let now = chrono::Utc::now().timestamp_millis();
        let sort_json = serde_json::to_value(sort).unwrap_or(serde_json::json!([]));
        let filters_json =
            filters.map(|f| serde_json::to_value(f).unwrap_or(serde_json::json!({}))).unwrap_or(serde_json::json!({}));
        let ranges_json = serde_json::to_value(ranges).unwrap_or(serde_json::json!([]));

        sqlx::query_as::<_, SlidingSyncList>(
            r"
            INSERT INTO sliding_sync_lists
                (user_id, device_id, conn_id, list_key, sort, filters, room_subscription, ranges, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $9)
            ON CONFLICT (user_id, device_id, COALESCE(conn_id, ''), list_key) DO UPDATE SET
                sort = EXCLUDED.sort,
                filters = EXCLUDED.filters,
                room_subscription = EXCLUDED.room_subscription,
                ranges = EXCLUDED.ranges,
                updated_ts = EXCLUDED.updated_ts
            RETURNING *
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(conn_id)
        .bind(list_key)
        .bind(&sort_json)
        .bind(&filters_json)
        .bind(room_subscription)
        .bind(&ranges_json)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_lists(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Vec<SlidingSyncList>, sqlx::Error> {
        self.ensure_schema()?;
        sqlx::query_as::<_, SlidingSyncList>(
            r"
            SELECT id, user_id, device_id, conn_id, list_key, sort, filters, room_subscription, ranges, created_ts, updated_ts FROM sliding_sync_lists
            WHERE user_id = $1 AND device_id = $2 AND (conn_id = $3 OR ($3 IS NULL AND conn_id IS NULL))
            ORDER BY created_ts ASC
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(conn_id)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn delete_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
    ) -> Result<(), sqlx::Error> {
        self.ensure_schema()?;
        sqlx::query(
            r"
            DELETE FROM sliding_sync_lists
            WHERE user_id = $1 AND device_id = $2 AND (conn_id = $3 OR ($3 IS NULL AND conn_id IS NULL)) AND list_key = $4
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(conn_id)
        .bind(list_key)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        list_key: Option<&str>,
        bump_stamp: i64,
        highlight_count: i32,
        notification_count: i32,
        is_dm: bool,
        is_encrypted: bool,
        is_tombstoned: bool,
        invited: bool,
        name: Option<&str>,
        avatar: Option<&str>,
        timestamp: i64,
    ) -> Result<SlidingSyncRoom, sqlx::Error> {
        self.ensure_schema()?;
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as::<_, SlidingSyncRoom>(
            r"
            INSERT INTO sliding_sync_rooms
                (user_id, device_id, room_id, conn_id, list_key, bump_stamp, highlight_count, notification_count,
                 is_dm, is_encrypted, is_tombstoned, invited, name, avatar, timestamp, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $16)
            ON CONFLICT (user_id, device_id, room_id, COALESCE(conn_id, '')) DO UPDATE SET
                list_key = COALESCE(EXCLUDED.list_key, sliding_sync_rooms.list_key),
                bump_stamp = GREATEST(sliding_sync_rooms.bump_stamp, EXCLUDED.bump_stamp),
                highlight_count = EXCLUDED.highlight_count,
                notification_count = EXCLUDED.notification_count,
                is_dm = EXCLUDED.is_dm,
                is_encrypted = EXCLUDED.is_encrypted,
                is_tombstoned = EXCLUDED.is_tombstoned,
                invited = EXCLUDED.invited,
                name = COALESCE(EXCLUDED.name, sliding_sync_rooms.name),
                avatar = COALESCE(EXCLUDED.avatar, sliding_sync_rooms.avatar),
                timestamp = EXCLUDED.timestamp,
                updated_ts = EXCLUDED.updated_ts
            RETURNING *
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(room_id)
        .bind(conn_id)
        .bind(list_key)
        .bind(bump_stamp)
        .bind(highlight_count)
        .bind(notification_count)
        .bind(is_dm)
        .bind(is_encrypted)
        .bind(is_tombstoned)
        .bind(invited)
        .bind(name)
        .bind(avatar)
        .bind(timestamp)
        .bind(now)
        .fetch_one(&*self.pool)
        .await
    }

    pub async fn get_rooms_for_list(
        &self,
        query_params: SlidingSyncListQuery<'_>,
    ) -> Result<Vec<SlidingSyncRoom>, sqlx::Error> {
        let SlidingSyncListQuery { user_id, device_id, conn_id, list_key, start, end, filters } = query_params;
        self.ensure_schema()?;
        let mut query = QueryBuilder::<Postgres>::new(
            r"
            SELECT id, user_id, device_id, room_id, conn_id, list_key, bump_stamp, highlight_count, notification_count, is_dm, is_encrypted, is_tombstoned, invited, name, avatar, timestamp, created_ts, updated_ts FROM sliding_sync_rooms
            WHERE user_id = ",
        );
        query.push_bind(user_id);
        query.push(" AND device_id = ");
        query.push_bind(device_id);
        query.push(" AND (conn_id = ");
        query.push_bind(conn_id);
        query.push(" OR conn_id IS NULL) AND (list_key = ");
        query.push_bind(list_key);
        query.push(" OR list_key IS NULL)");

        Self::push_room_filters(&mut query, filters);

        query.push(" ORDER BY bump_stamp DESC");
        query.push(" LIMIT ");
        query.push_bind((end.saturating_sub(start) + 1) as i64);
        query.push(" OFFSET ");
        query.push_bind(start as i64);

        query.build_query_as::<SlidingSyncRoom>().fetch_all(&*self.pool).await
    }

    pub async fn count_rooms_for_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        filters: Option<&SlidingSyncFilters>,
    ) -> Result<i64, sqlx::Error> {
        self.ensure_schema()?;
        let mut query = QueryBuilder::<Postgres>::new(
            r"
            SELECT COUNT(*) FROM sliding_sync_rooms
            WHERE user_id = ",
        );
        query.push_bind(user_id);
        query.push(" AND device_id = ");
        query.push_bind(device_id);
        query.push(" AND (conn_id = ");
        query.push_bind(conn_id);
        query.push(" OR (");
        query.push_bind(conn_id);
        query.push(" IS NULL AND conn_id IS NULL)) AND list_key = ");
        query.push_bind(list_key);

        Self::push_room_filters(&mut query, filters);

        query.build_query_scalar().fetch_one(&*self.pool).await
    }

    pub async fn get_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncRoom>, sqlx::Error> {
        self.ensure_schema()?;
        sqlx::query_as::<_, SlidingSyncRoom>(
            r"
            SELECT id, user_id, device_id, room_id, conn_id, list_key, bump_stamp, highlight_count, notification_count, is_dm, is_encrypted, is_tombstoned, invited, name, avatar, timestamp, created_ts, updated_ts FROM sliding_sync_rooms
            WHERE user_id = $1 AND device_id = $2 AND room_id = $3 AND (conn_id = $4 OR ($4 IS NULL AND conn_id IS NULL))
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(room_id)
        .bind(conn_id)
        .fetch_optional(&*self.pool)
        .await
    }

    pub async fn materialize_room_from_activity(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncRoom>, sqlx::Error> {
        self.ensure_schema()?;

        let is_member = sqlx::query_scalar::<_, bool>(
            r"
            SELECT EXISTS(
                SELECT 1
                FROM room_memberships
                WHERE room_id = $1 AND user_id = $2 AND membership = 'join'
            )
            ",
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await?;

        if !is_member {
            return Ok(None);
        }

        let now = chrono::Utc::now().timestamp_millis();
        let bump_stamp = sqlx::query_scalar::<_, Option<i64>>(
            r"
            SELECT MAX(origin_server_ts)
            FROM events
            WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await?
        .unwrap_or(now);

        let existing_room = self.get_room(user_id, device_id, room_id, conn_id).await?;

        let room_info = sqlx::query_scalar::<_, Option<String>>(
            r"
            SELECT name FROM rooms WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?
        .flatten();

        let avatar_info = sqlx::query_scalar::<_, Option<String>>(
            r"
            SELECT avatar_url FROM rooms WHERE room_id = $1
            ",
        )
        .bind(room_id)
        .fetch_optional(&*self.pool)
        .await?
        .flatten();

        self.upsert_room(
            user_id,
            device_id,
            room_id,
            conn_id,
            None,
            bump_stamp,
            existing_room.as_ref().map_or(0, |room| room.highlight_count),
            existing_room.as_ref().map_or(0, |room| room.notification_count),
            existing_room.as_ref().is_some_and(|room| room.is_dm),
            existing_room.as_ref().is_some_and(|room| room.is_encrypted),
            existing_room.as_ref().is_some_and(|room| room.is_tombstoned),
            existing_room.as_ref().is_some_and(|room| room.is_invited),
            room_info.as_deref().or(existing_room.as_ref().and_then(|room| room.name.as_deref())),
            avatar_info.as_deref().or(existing_room.as_ref().and_then(|room| room.avatar.as_deref())),
            now,
        )
        .await?;

        self.get_room(user_id, device_id, room_id, conn_id).await
    }

    pub(crate) fn push_room_filters(query: &mut QueryBuilder<Postgres>, filters: Option<&SlidingSyncFilters>) {
        let Some(filters) = filters else {
            return;
        };

        if let Some(is_dm) = filters.is_dm {
            query.push(" AND is_dm = ");
            query.push_bind(is_dm);
        }

        if let Some(is_encrypted) = filters.is_encrypted {
            query.push(" AND is_encrypted = ");
            query.push_bind(is_encrypted);
        }

        if let Some(is_invite) = filters.is_invite {
            query.push(" AND invited = ");
            query.push_bind(is_invite);
        }

        if let Some(is_tombstoned) = filters.is_tombstoned {
            query.push(" AND is_tombstoned = ");
            query.push_bind(is_tombstoned);
        }

        if let Some(room_name_like) = filters.room_name_like.as_deref() {
            query.push(" AND COALESCE(name, '') ILIKE ");
            query.push_bind(format!("%{room_name_like}%"));
        }
    }

    pub async fn delete_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.ensure_schema()?;
        sqlx::query(
            r"
            DELETE FROM sliding_sync_rooms
            WHERE user_id = $1 AND device_id = $2 AND room_id = $3 AND (conn_id = $4 OR ($4 IS NULL AND conn_id IS NULL))
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(room_id)
        .bind(conn_id)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn update_notification_counts(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        highlight_count: i32,
        notification_count: i32,
    ) -> Result<(), sqlx::Error> {
        self.ensure_schema()?;
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r"
            UPDATE sliding_sync_rooms
            SET highlight_count = $5, notification_count = $6, updated_ts = $7
            WHERE user_id = $1 AND device_id = $2 AND room_id = $3 AND (conn_id = $4 OR ($4 IS NULL AND conn_id IS NULL))
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(room_id)
        .bind(conn_id)
        .bind(highlight_count)
        .bind(notification_count)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn bump_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        bump_stamp: i64,
    ) -> Result<(), sqlx::Error> {
        self.ensure_schema()?;
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r"
            UPDATE sliding_sync_rooms
            SET bump_stamp = GREATEST(bump_stamp, $5), updated_ts = $6
            WHERE user_id = $1 AND device_id = $2 AND room_id = $3 AND (conn_id = $4 OR ($4 IS NULL AND conn_id IS NULL))
            ",
        )
        .bind(user_id)
        .bind(device_id)
        .bind(room_id)
        .bind(conn_id)
        .bind(bump_stamp)
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(())
    }

    pub async fn cleanup_expired_tokens(&self) -> Result<u64, sqlx::Error> {
        self.ensure_schema()?;
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r"
            DELETE FROM sliding_sync_tokens
            WHERE expires_at IS NOT NULL AND expires_at < $1
            ",
        )
        .bind(now)
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn list_room_token_sync(
        &self,
        room_id: &str,
        limit: i64,
        from: Option<&RoomTokenSyncCursor>,
    ) -> Result<Vec<AdminRoomTokenSyncEntry>, sqlx::Error> {
        self.ensure_schema()?;
        let now = chrono::Utc::now().timestamp_millis();
        // Fetch one extra row so callers can detect "more pages available",
        // but truncate back to `limit` before returning so the public contract
        // (length <= limit) holds.
        let fetch_limit = limit.saturating_add(1);

        let mut rows = if let Some(cursor) = from {
            sqlx::query_as::<_, AdminRoomTokenSyncEntry>(
                r"
                SELECT
                    rooms.user_id,
                    rooms.device_id,
                    rooms.conn_id,
                    rooms.list_key,
                    tokens.pos,
                    tokens.created_ts AS token_created_ts,
                    tokens.expires_at AS token_expires_at,
                    rooms.timestamp AS room_timestamp,
                    rooms.updated_ts AS room_updated_ts,
                    rooms.bump_stamp,
                    rooms.highlight_count,
                    rooms.notification_count,
                    rooms.is_dm,
                    rooms.is_encrypted,
                    rooms.is_tombstoned,
                    rooms.invited,
                    rooms.name,
                    rooms.avatar,
                    COALESCE(tokens.expires_at IS NOT NULL AND tokens.expires_at < $2, FALSE) AS is_expired
                FROM sliding_sync_rooms rooms
                LEFT JOIN sliding_sync_tokens tokens
                    ON tokens.user_id = rooms.user_id
                   AND tokens.device_id = rooms.device_id
                   AND COALESCE(tokens.conn_id, '') = COALESCE(rooms.conn_id, '')
                WHERE rooms.room_id = $1
                  AND (
                    rooms.updated_ts < $3
                    OR (rooms.updated_ts = $3 AND rooms.user_id > $4)
                    OR (rooms.updated_ts = $3 AND rooms.user_id = $4 AND rooms.device_id > $5)
                    OR (
                        rooms.updated_ts = $3
                        AND rooms.user_id = $4
                        AND rooms.device_id = $5
                        AND COALESCE(rooms.conn_id, '') > $6
                    )
                  )
                ORDER BY rooms.updated_ts DESC, rooms.user_id ASC, rooms.device_id ASC, COALESCE(rooms.conn_id, '') ASC
                LIMIT $7
                ",
            )
            .bind(room_id)
            .bind(now)
            .bind(cursor.room_updated_ts)
            .bind(&cursor.user_id)
            .bind(&cursor.device_id)
            .bind(cursor.conn_id.as_deref().unwrap_or(""))
            .bind(fetch_limit)
            .fetch_all(&*self.pool)
            .await?
        } else {
            sqlx::query_as::<_, AdminRoomTokenSyncEntry>(
                r"
                SELECT
                    rooms.user_id,
                    rooms.device_id,
                    rooms.conn_id,
                    rooms.list_key,
                    tokens.pos,
                    tokens.created_ts AS token_created_ts,
                    tokens.expires_at AS token_expires_at,
                    rooms.timestamp AS room_timestamp,
                    rooms.updated_ts AS room_updated_ts,
                    rooms.bump_stamp,
                    rooms.highlight_count,
                    rooms.notification_count,
                    rooms.is_dm,
                    rooms.is_encrypted,
                    rooms.is_tombstoned,
                    rooms.invited,
                    rooms.name,
                    rooms.avatar,
                    COALESCE(tokens.expires_at IS NOT NULL AND tokens.expires_at < $2, FALSE) AS is_expired
                FROM sliding_sync_rooms rooms
                LEFT JOIN sliding_sync_tokens tokens
                    ON tokens.user_id = rooms.user_id
                   AND tokens.device_id = rooms.device_id
                   AND COALESCE(tokens.conn_id, '') = COALESCE(rooms.conn_id, '')
                WHERE rooms.room_id = $1
                ORDER BY rooms.updated_ts DESC, rooms.user_id ASC, rooms.device_id ASC, COALESCE(rooms.conn_id, '') ASC
                LIMIT $3
                ",
            )
            .bind(room_id)
            .bind(now)
            .bind(fetch_limit)
            .fetch_all(&*self.pool)
            .await?
        };

        // Truncate to the requested limit; the extra row (if any) was only
        // fetched to signal "more pages available".
        if (rows.len() as i64) > limit {
            rows.truncate(limit as usize);
        }

        Ok(rows)
    }

    pub async fn count_room_token_sync(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        self.ensure_schema()?;
        sqlx::query_scalar("SELECT COUNT(*) FROM sliding_sync_rooms WHERE room_id = $1")
            .bind(room_id)
            .fetch_one(&*self.pool)
            .await
    }

    pub async fn get_global_account_data(&self, user_id: &str) -> Result<serde_json::Value, sqlx::Error> {
        self.ensure_schema()?;
        let rows = sqlx::query(
            r"
            SELECT data_type, content
            FROM account_data
            WHERE user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await?;

        let mut map = serde_json::Map::new();
        for row in rows {
            let data_type: String = sqlx::Row::get(&row, "data_type");
            let content: serde_json::Value = sqlx::Row::get(&row, "content");
            map.insert(data_type, content);
        }
        Ok(serde_json::Value::Object(map))
    }

    pub async fn get_room_account_data(
        &self,
        user_id: &str,
        room_ids: &[String],
    ) -> Result<serde_json::Value, sqlx::Error> {
        self.ensure_schema()?;
        if room_ids.is_empty() {
            return Ok(serde_json::json!({}));
        }

        let rows = sqlx::query(
            r"
            SELECT room_id, data_type, data
            FROM room_account_data
            WHERE user_id = $1 AND room_id = ANY($2::text[])
            ",
        )
        .bind(user_id)
        .bind(room_ids)
        .fetch_all(&*self.pool)
        .await?;

        let mut rooms_map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        for row in rows {
            let room_id: String = sqlx::Row::get(&row, "room_id");
            let data_type: String = sqlx::Row::get(&row, "data_type");
            let data: serde_json::Value = sqlx::Row::get(&row, "data");

            let entry = rooms_map.entry(room_id).or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if let Some(obj) = entry.as_object_mut() {
                obj.insert(data_type, data);
            }
        }

        Ok(serde_json::Value::Object(rooms_map))
    }

    #[allow(clippy::expect_used)]
    pub async fn get_receipts_for_rooms(&self, room_ids: &[String]) -> Result<serde_json::Value, sqlx::Error> {
        self.ensure_schema()?;
        if room_ids.is_empty() {
            return Ok(serde_json::json!({}));
        }

        let rows = sqlx::query(
            r"
            SELECT room_id, event_id, user_id, receipt_type, ts, data
            FROM event_receipts
            WHERE room_id = ANY($1::text[])
            ",
        )
        .bind(room_ids)
        .fetch_all(&*self.pool)
        .await?;

        let mut rooms_map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        for row in rows {
            let room_id: String = sqlx::Row::get(&row, "room_id");
            let event_id: String = sqlx::Row::get(&row, "event_id");
            let user_id: String = sqlx::Row::get(&row, "user_id");
            let receipt_type: String = sqlx::Row::get(&row, "receipt_type");
            let ts: i64 = sqlx::Row::get(&row, "ts");
            let data: serde_json::Value = sqlx::Row::get(&row, "data");

            let room_obj = rooms_map
                .entry(room_id)
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
                .as_object_mut()
                .expect("or_insert_with always inserts Value::Object");

            let receipt_type_obj = room_obj
                .entry(receipt_type)
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
                .as_object_mut()
                .expect("or_insert_with always inserts Value::Object");

            let event_obj = receipt_type_obj
                .entry(event_id)
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()))
                .as_object_mut()
                .expect("or_insert_with always inserts Value::Object");

            event_obj.insert(
                user_id,
                serde_json::json!({
                    "ts": ts,
                    "data": data
                }),
            );
        }

        Ok(serde_json::Value::Object(rooms_map))
    }

    fn ensure_schema(&self) -> Result<(), sqlx::Error> {
        Ok(())
    }

    pub async fn delete_connection_data(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.ensure_schema()?;

        sqlx::query!(
            r#"
            DELETE FROM sliding_sync_tokens
            WHERE user_id = $1 AND device_id = $2 AND (conn_id = $3 OR ($3 IS NULL AND conn_id IS NULL))
            "#,
            user_id,
            device_id,
            conn_id
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query!(
            r#"
            DELETE FROM sliding_sync_lists
            WHERE user_id = $1 AND device_id = $2 AND (conn_id = $3 OR ($3 IS NULL AND conn_id IS NULL))
            "#,
            user_id,
            device_id,
            conn_id
        )
        .execute(&*self.pool)
        .await?;

        sqlx::query!(
            r#"
            DELETE FROM sliding_sync_rooms
            WHERE user_id = $1 AND device_id = $2 AND (conn_id = $3 OR ($3 IS NULL AND conn_id IS NULL))
            "#,
            user_id,
            device_id,
            conn_id
        )
        .execute(&*self.pool)
        .await?;

        Ok(())
    }
}
