use serde::{de::Deserializer, Deserialize, Serialize};
use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SlidingSyncToken {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub conn_id: Option<String>,
    pub pos: i64,
    pub created_ts: i64,
    pub expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SlidingSyncList {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub conn_id: Option<String>,
    pub list_key: String,
    pub sort: serde_json::Value,
    pub filters: Option<serde_json::Value>,
    pub room_subscription: Option<serde_json::Value>,
    pub ranges: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SlidingSyncRoom {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub room_id: String,
    pub conn_id: Option<String>,
    pub list_key: Option<String>,
    pub bump_stamp: i64,
    pub highlight_count: i32,
    pub notification_count: i32,
    pub is_dm: bool,
    pub is_encrypted: bool,
    pub is_tombstoned: bool,
    pub invited: bool,
    pub name: Option<String>,
    pub avatar: Option<String>,
    pub timestamp: i64,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdminRoomTokenSyncEntry {
    pub user_id: String,
    pub device_id: String,
    pub conn_id: Option<String>,
    pub list_key: Option<String>,
    pub pos: Option<i64>,
    pub token_created_ts: Option<i64>,
    pub token_expires_at: Option<i64>,
    pub room_timestamp: i64,
    pub room_updated_ts: i64,
    pub bump_stamp: i64,
    pub highlight_count: i32,
    pub notification_count: i32,
    pub is_dm: bool,
    pub is_encrypted: bool,
    pub is_tombstoned: bool,
    pub invited: bool,
    pub name: Option<String>,
    pub avatar: Option<String>,
    pub is_expired: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlidingSyncRequest {
    pub conn_id: Option<String>,
    #[serde(default, deserialize_with = "deserialize_sliding_sync_lists")]
    pub lists: HashMap<String, SlidingSyncListData>,
    pub room_subscriptions: Option<serde_json::Value>,
    #[serde(default)]
    pub unsubscribe_rooms: Option<Vec<String>>,
    pub extensions: Option<serde_json::Value>,
    pub pos: Option<String>,
    pub timeout: Option<u32>,
    #[serde(rename = "clientTimeout")]
    pub client_timeout: Option<u32>,
}

fn deserialize_sliding_sync_lists<'de, D>(
    deserializer: D,
) -> Result<HashMap<String, SlidingSyncListData>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum ListsPayload {
        Map(HashMap<String, SlidingSyncListData>),
        Vec(Vec<SlidingSyncListRequest>),
    }

    match ListsPayload::deserialize(deserializer)? {
        ListsPayload::Map(map) => Ok(map),
        ListsPayload::Vec(list_requests) => {
            let mut map = HashMap::new();
            for list in list_requests {
                let ranges = list
                    .ranges
                    .into_iter()
                    .map(|(start, end)| vec![start, end])
                    .collect();
                map.insert(
                    list.list_key,
                    SlidingSyncListData {
                        ranges,
                        sort: list.sort,
                        filters: list.filters,
                        timeline_limit: list.limit,
                        required_state: None,
                    },
                );
            }
            Ok(map)
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlidingSyncListData {
    #[serde(default)]
    pub ranges: Vec<Vec<u32>>,
    #[serde(default)]
    pub sort: Vec<String>,
    pub filters: Option<SlidingSyncFilters>,
    #[serde(rename = "timeline_limit")]
    pub timeline_limit: Option<u32>,
    #[serde(rename = "required_state")]
    pub required_state: Option<Vec<Vec<String>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlidingSyncListRequest {
    pub list_key: String,
    pub sort: Vec<String>,
    pub filters: Option<SlidingSyncFilters>,
    pub room_subscription: Option<serde_json::Value>,
    pub ranges: Vec<(u32, u32)>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SlidingSyncFilters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_dm: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_encrypted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_invite: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_tombstoned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_room_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub room_name_like: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlidingSyncResponse {
    pub pos: String,
    pub conn_id: Option<String>,
    pub lists: serde_json::Value,
    pub rooms: serde_json::Value,
    pub extensions: Option<serde_json::Value>,
}

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
        self.ensure_schema().await?;
        let now = chrono::Utc::now().timestamp_millis();
        let expires_at = now + 7 * 24 * 3600 * 1000;

        let token = uuid::Uuid::new_v4().to_string();

        sqlx::query_as::<_, SlidingSyncToken>(
            r#"
            INSERT INTO sliding_sync_tokens (user_id, device_id, token, conn_id, pos, created_ts, expires_at)
            VALUES ($1, $2, $3, $4, nextval('sliding_sync_pos_seq'), $5, $6)
            ON CONFLICT (user_id, device_id, COALESCE(conn_id, ''::text)) DO UPDATE SET
                pos = nextval('sliding_sync_pos_seq'),
                expires_at = EXCLUDED.expires_at
            RETURNING *
            "#,
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
        self.ensure_schema().await?;
        sqlx::query_as::<_, SlidingSyncToken>(
            r#"
            SELECT * FROM sliding_sync_tokens 
            WHERE user_id = $1 AND device_id = $2 AND (conn_id = $3 OR ($3 IS NULL AND conn_id IS NULL))
            "#,
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
        self.ensure_schema().await?;
        let result: Option<(bool,)> = sqlx::query_as(
            r#"
            SELECT (pos = $4) FROM sliding_sync_tokens 
            WHERE user_id = $1 AND device_id = $2 AND (conn_id = $3 OR ($3 IS NULL AND conn_id IS NULL))
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(conn_id)
        .bind(pos.parse::<i64>().unwrap_or(0))
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.map(|r| r.0).unwrap_or(false))
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
        self.ensure_schema().await?;
        let now = chrono::Utc::now().timestamp_millis();
        let sort_json = serde_json::to_value(sort).unwrap_or(serde_json::json!([]));
        let filters_json =
            filters.map(|f| serde_json::to_value(f).unwrap_or(serde_json::json!({})));
        let ranges_json = serde_json::to_value(ranges).unwrap_or(serde_json::json!([]));

        sqlx::query_as::<_, SlidingSyncList>(
            r#"
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
            "#,
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
        self.ensure_schema().await?;
        sqlx::query_as::<_, SlidingSyncList>(
            r#"
            SELECT * FROM sliding_sync_lists 
            WHERE user_id = $1 AND device_id = $2 AND (conn_id = $3 OR ($3 IS NULL AND conn_id IS NULL))
            ORDER BY created_ts ASC
            "#,
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
        self.ensure_schema().await?;
        sqlx::query(
            r#"
            DELETE FROM sliding_sync_lists 
            WHERE user_id = $1 AND device_id = $2 AND (conn_id = $3 OR ($3 IS NULL AND conn_id IS NULL)) AND list_key = $4
            "#,
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
        self.ensure_schema().await?;
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as::<_, SlidingSyncRoom>(
            r#"
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
            "#,
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
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        start: u32,
        end: u32,
    ) -> Result<Vec<SlidingSyncRoom>, sqlx::Error> {
        self.ensure_schema().await?;
        sqlx::query_as::<_, SlidingSyncRoom>(
            r#"
            SELECT * FROM sliding_sync_rooms 
            WHERE user_id = $1 AND device_id = $2 AND (conn_id = $3 OR ($3 IS NULL AND conn_id IS NULL)) AND list_key = $4
            ORDER BY bump_stamp DESC
            LIMIT $6 OFFSET $5
            "#,
        )
        .bind(user_id)
        .bind(device_id)
        .bind(conn_id)
        .bind(list_key)
        .bind(start as i64)
        .bind((end - start + 1) as i64)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn get_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncRoom>, sqlx::Error> {
        self.ensure_schema().await?;
        sqlx::query_as::<_, SlidingSyncRoom>(
            r#"
            SELECT * FROM sliding_sync_rooms 
            WHERE user_id = $1 AND device_id = $2 AND room_id = $3 AND (conn_id = $4 OR ($4 IS NULL AND conn_id IS NULL))
            "#,
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
        self.ensure_schema().await?;

        let is_member = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM room_memberships
                WHERE room_id = $1 AND user_id = $2 AND membership = 'join'
            )
            "#,
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
            r#"
            SELECT MAX(origin_server_ts)
            FROM events
            WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .fetch_one(&*self.pool)
        .await?
        .unwrap_or(now);

        self.upsert_room(
            user_id, device_id, room_id, conn_id, None, bump_stamp, 0, 0, false, false, false,
            false, None, None, now,
        )
        .await?;

        self.get_room(user_id, device_id, room_id, conn_id).await
    }

    pub async fn delete_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.ensure_schema().await?;
        sqlx::query(
            r#"
            DELETE FROM sliding_sync_rooms 
            WHERE user_id = $1 AND device_id = $2 AND room_id = $3 AND (conn_id = $4 OR ($4 IS NULL AND conn_id IS NULL))
            "#,
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
        self.ensure_schema().await?;
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE sliding_sync_rooms 
            SET highlight_count = $5, notification_count = $6, updated_ts = $7
            WHERE user_id = $1 AND device_id = $2 AND room_id = $3 AND (conn_id = $4 OR ($4 IS NULL AND conn_id IS NULL))
            "#,
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
        self.ensure_schema().await?;
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            UPDATE sliding_sync_rooms 
            SET bump_stamp = GREATEST(bump_stamp, $5), updated_ts = $6
            WHERE user_id = $1 AND device_id = $2 AND room_id = $3 AND (conn_id = $4 OR ($4 IS NULL AND conn_id IS NULL))
            "#,
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
        self.ensure_schema().await?;
        let now = chrono::Utc::now().timestamp_millis();

        let result = sqlx::query(
            r#"
            DELETE FROM sliding_sync_tokens 
            WHERE expires_at IS NOT NULL AND expires_at < $1
            "#,
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
        offset: i64,
    ) -> Result<Vec<AdminRoomTokenSyncEntry>, sqlx::Error> {
        self.ensure_schema().await?;
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query_as::<_, AdminRoomTokenSyncEntry>(
            r#"
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
            ORDER BY rooms.updated_ts DESC, rooms.user_id ASC, rooms.device_id ASC, rooms.conn_id ASC
            LIMIT $3 OFFSET $4
            "#,
        )
        .bind(room_id)
        .bind(now)
        .bind(limit)
        .bind(offset)
        .fetch_all(&*self.pool)
        .await
    }

    pub async fn count_room_token_sync(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        self.ensure_schema().await?;
        sqlx::query_scalar("SELECT COUNT(*) FROM sliding_sync_rooms WHERE room_id = $1")
            .bind(room_id)
            .fetch_one(&*self.pool)
            .await
    }

    pub async fn get_global_account_data(
        &self,
        user_id: &str,
    ) -> Result<serde_json::Value, sqlx::Error> {
        self.ensure_schema().await?;
        let rows = sqlx::query(
            r#"
            SELECT data_type, content
            FROM account_data
            WHERE user_id = $1
            "#,
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
        self.ensure_schema().await?;
        if room_ids.is_empty() {
            return Ok(serde_json::json!({}));
        }

        let rows = sqlx::query(
            r#"
            SELECT room_id, data_type, data
            FROM room_account_data
            WHERE user_id = $1 AND room_id = ANY($2::text[])
            "#,
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

            let entry = rooms_map
                .entry(room_id)
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if let Some(obj) = entry.as_object_mut() {
                obj.insert(data_type, data);
            }
        }

        Ok(serde_json::Value::Object(rooms_map))
    }

    pub async fn get_receipts_for_rooms(
        &self,
        room_ids: &[String],
    ) -> Result<serde_json::Value, sqlx::Error> {
        self.ensure_schema().await?;
        if room_ids.is_empty() {
            return Ok(serde_json::json!({}));
        }

        let rows = sqlx::query(
            r#"
            SELECT room_id, event_id, user_id, receipt_type, ts, data
            FROM event_receipts
            WHERE room_id = ANY($1::text[])
            "#,
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

            let room_entry = rooms_map
                .entry(room_id)
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            let room_obj = room_entry
                .as_object_mut()
                .expect("room entry must be object");

            let receipt_type_entry = room_obj
                .entry(receipt_type)
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            let receipt_type_obj = receipt_type_entry
                .as_object_mut()
                .expect("receipt type entry must be object");

            let event_entry = receipt_type_obj
                .entry(event_id)
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            let event_obj = event_entry
                .as_object_mut()
                .expect("event entry must be object");

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

    async fn ensure_schema(&self) -> Result<(), sqlx::Error> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sliding_sync_token_struct() {
        let token = SlidingSyncToken {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            conn_id: Some("conn456".to_string()),
            pos: 100,
            created_ts: 1234567890000,
            expires_at: Some(1235172690000),
        };

        assert_eq!(token.user_id, "@alice:example.com");
        assert_eq!(token.pos, 100);
        assert!(token.conn_id.is_some());
    }

    #[test]
    fn test_sliding_sync_filters_default() {
        let filters = SlidingSyncFilters::default();
        assert!(filters.is_dm.is_none());
        assert!(filters.is_encrypted.is_none());
    }

    #[test]
    fn test_sliding_sync_filters_with_values() {
        let filters = SlidingSyncFilters {
            is_dm: Some(true),
            is_encrypted: Some(true),
            is_invite: Some(false),
            room_name_like: Some("test".to_string()),
            ..Default::default()
        };

        assert_eq!(filters.is_dm, Some(true));
        assert_eq!(filters.room_name_like, Some("test".to_string()));
    }

    #[test]
    fn test_sliding_sync_request() {
        let mut lists = std::collections::HashMap::new();
        lists.insert(
            "main".to_string(),
            SlidingSyncListData {
                ranges: vec![vec![0, 20]],
                sort: vec!["by_recency".to_string()],
                filters: None,
                timeline_limit: Some(100),
                required_state: None,
            },
        );

        let request = SlidingSyncRequest {
            conn_id: Some("test_conn".to_string()),
            lists,
            room_subscriptions: None,
            unsubscribe_rooms: None,
            extensions: None,
            pos: None,
            timeout: Some(30000),
            client_timeout: None,
        };

        assert!(request.conn_id.is_some());
        assert_eq!(request.lists.len(), 1);
    }

    #[test]
    fn test_sliding_sync_response() {
        let response = SlidingSyncResponse {
            pos: "12345".to_string(),
            conn_id: Some("conn123".to_string()),
            lists: serde_json::json!({}),
            rooms: serde_json::json!({}),
            extensions: None,
        };

        assert_eq!(response.pos, "12345");
    }

    #[test]
    fn test_sliding_sync_room_struct() {
        let room = SlidingSyncRoom {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE123".to_string(),
            room_id: "!room:example.com".to_string(),
            conn_id: None,
            list_key: Some("main".to_string()),
            bump_stamp: 1234567890000,
            highlight_count: 5,
            notification_count: 10,
            is_dm: true,
            is_encrypted: true,
            is_tombstoned: false,
            invited: false,
            name: Some("Test Room".to_string()),
            avatar: None,
            timestamp: 1234567890000,
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
        };

        assert_eq!(room.highlight_count, 5);
        assert!(room.is_dm);
    }
}
