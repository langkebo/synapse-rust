use async_trait::async_trait;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{de::Deserializer, Deserialize, Serialize};
use sqlx::{Pool, Postgres, QueryBuilder};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SlidingSyncToken {
    pub id: i64,
    pub user_id: String,
    pub device_id: String,
    pub conn_id: Option<String>,
    pub token: String,
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
    #[serde(rename = "invited")]
    #[sqlx(rename = "invited")]
    pub is_invited: bool,
    pub name: Option<String>,
    pub avatar: Option<String>,
    pub timestamp: i64,
    pub created_ts: i64,
    pub updated_ts: i64,
}

pub struct SlidingSyncListQuery<'a> {
    pub user_id: &'a str,
    pub device_id: &'a str,
    pub conn_id: Option<&'a str>,
    pub list_key: &'a str,
    pub start: u32,
    pub end: u32,
    pub filters: Option<&'a SlidingSyncFilters>,
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
    #[serde(rename = "invited")]
    #[sqlx(rename = "invited")]
    pub is_invited: bool,
    pub name: Option<String>,
    pub avatar: Option<String>,
    pub is_expired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoomTokenSyncCursor {
    pub room_updated_ts: i64,
    pub user_id: String,
    pub device_id: String,
    pub conn_id: Option<String>,
}

pub fn encode_room_token_sync_cursor(cursor: &RoomTokenSyncCursor) -> String {
    let encoded_user_id = URL_SAFE_NO_PAD.encode(cursor.user_id.as_bytes());
    let encoded_device_id = URL_SAFE_NO_PAD.encode(cursor.device_id.as_bytes());
    let is_conn_id_null = if cursor.conn_id.is_none() { 1 } else { 0 };
    let encoded_conn_id = URL_SAFE_NO_PAD.encode(cursor.conn_id.as_deref().unwrap_or("").as_bytes());

    format!(
        "{}|{}|{}|{}|{}",
        cursor.room_updated_ts, encoded_user_id, encoded_device_id, is_conn_id_null, encoded_conn_id
    )
}

pub fn decode_room_token_sync_cursor(cursor: Option<&str>) -> Option<RoomTokenSyncCursor> {
    let cursor = cursor?;
    let mut parts = cursor.split('|');
    let room_updated_ts = parts.next()?.parse::<i64>().ok()?;
    let encoded_user_id = parts.next()?;
    let encoded_device_id = parts.next()?;
    let is_conn_id_null = parts.next()?.parse::<u8>().ok()?;
    let encoded_conn_id = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let user_id = String::from_utf8(URL_SAFE_NO_PAD.decode(encoded_user_id).ok()?).ok()?;
    let device_id = String::from_utf8(URL_SAFE_NO_PAD.decode(encoded_device_id).ok()?).ok()?;
    let conn_id = if is_conn_id_null == 1 {
        None
    } else {
        Some(String::from_utf8(URL_SAFE_NO_PAD.decode(encoded_conn_id).ok()?).ok()?)
    };

    if user_id.is_empty() || device_id.is_empty() {
        return None;
    }

    Some(RoomTokenSyncCursor { room_updated_ts, user_id, device_id, conn_id })
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

fn deserialize_sliding_sync_lists<'de, D>(deserializer: D) -> Result<HashMap<String, SlidingSyncListData>, D::Error>
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
                let ranges = list.ranges.into_iter().map(|(start, end)| vec![start, end]).collect();
                map.insert(
                    list.list_key,
                    SlidingSyncListData {
                        ranges,
                        sort: list.sort,
                        filters: list.filters,
                        timeline_limit: list.limit,
                        required_state: None,
                        slow_by: None,
                        bump_event_types: None,
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
    #[serde(rename = "timeline_limit", alias = "timelineLimit", default)]
    pub timeline_limit: Option<u32>,
    #[serde(rename = "required_state", alias = "requiredState", default)]
    pub required_state: Option<Vec<Vec<String>>>,
    #[serde(default)]
    pub slow_by: Option<u32>,
    #[serde(default)]
    pub bump_event_types: Option<Vec<String>>,
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
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub is_dm: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub is_encrypted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub is_invite: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub is_tombstoned: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub room_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub not_room_types: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub room_name_like: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub not_tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub room_state_types: Option<Vec<String>>,
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

    fn push_room_filters(query: &mut QueryBuilder<Postgres>, filters: Option<&SlidingSyncFilters>) {
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

#[cfg(test)]
mod cursor_tests {
    use super::{decode_room_token_sync_cursor, encode_room_token_sync_cursor, RoomTokenSyncCursor};

    #[test]
    fn room_token_sync_cursor_round_trip() {
        let cursor = RoomTokenSyncCursor {
            room_updated_ts: 1_700_000_000_000,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE".to_string(),
            conn_id: Some("main|conn".to_string()),
        };

        let encoded = encode_room_token_sync_cursor(&cursor);
        assert_eq!(decode_room_token_sync_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn room_token_sync_cursor_rejects_invalid_values() {
        assert_eq!(decode_room_token_sync_cursor(Some("bad")), None);
        assert_eq!(decode_room_token_sync_cursor(Some("123|||")), None);
    }

    #[test]
    fn room_token_sync_cursor_round_trip_no_conn_id() {
        let cursor = RoomTokenSyncCursor {
            room_updated_ts: 1_700_000_000_000,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE".to_string(),
            conn_id: None,
        };

        let encoded = encode_room_token_sync_cursor(&cursor);
        assert_eq!(decode_room_token_sync_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn room_token_sync_cursor_round_trip_empty_conn_id() {
        let cursor = RoomTokenSyncCursor {
            room_updated_ts: 1_700_000_000_000,
            user_id: "@alice:example.com".to_string(),
            device_id: "DEVICE".to_string(),
            conn_id: Some(String::new()),
        };

        let encoded = encode_room_token_sync_cursor(&cursor);
        assert_eq!(decode_room_token_sync_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn room_token_sync_cursor_rejects_empty_user_id() {
        let cursor = RoomTokenSyncCursor {
            room_updated_ts: 1_700_000_000_000,
            user_id: String::new(),
            device_id: "DEVICE".to_string(),
            conn_id: None,
        };

        let encoded = encode_room_token_sync_cursor(&cursor);
        assert_eq!(decode_room_token_sync_cursor(Some(&encoded)), None);
    }

    #[test]
    fn room_token_sync_cursor_rejects_empty_device_id() {
        let cursor = RoomTokenSyncCursor {
            room_updated_ts: 1_700_000_000_000,
            user_id: "@alice:example.com".to_string(),
            device_id: String::new(),
            conn_id: None,
        };

        let encoded = encode_room_token_sync_cursor(&cursor);
        assert_eq!(decode_room_token_sync_cursor(Some(&encoded)), None);
    }

    #[test]
    fn room_token_sync_cursor_rejects_extra_parts() {
        // 6 pipe-separated segments, the 6th triggers the parts.next().is_some() guard
        let encoded = "123|dXNlcg==|ZGV2|0||extra_part";
        assert_eq!(decode_room_token_sync_cursor(Some(encoded)), None);
    }

    #[test]
    fn room_token_sync_cursor_none_input() {
        assert_eq!(decode_room_token_sync_cursor(None), None);
    }

    #[test]
    fn room_token_sync_cursor_rejects_invalid_base64() {
        assert_eq!(decode_room_token_sync_cursor(Some("123|!!!invalid!!!|ZGV2|0|")), None);
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
            token: "test_token".to_string(),
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
                slow_by: None,
                bump_event_types: None,
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
            is_invited: false,
            name: Some("Test Room".to_string()),
            avatar: None,
            timestamp: 1234567890000,
            created_ts: 1234567890000,
            updated_ts: 1234567890000,
        };

        assert_eq!(room.highlight_count, 5);
        assert!(room.is_dm);
    }

    // ── push_room_filters tests ──

    /// Helper: build SQL from a base query and optional filters, returning the SQL string.
    /// push_room_filters is an associated function (no self) so no storage instance is needed.
    fn build_filtered_sql(filters: Option<&SlidingSyncFilters>) -> String {
        let mut query = QueryBuilder::<Postgres>::new("SELECT * FROM t WHERE 1=1");
        SlidingSyncStorage::push_room_filters(&mut query, filters);
        query.sql().to_string()
    }

    #[test]
    fn test_push_room_filters_none() {
        let sql = build_filtered_sql(None);
        assert_eq!(sql, "SELECT * FROM t WHERE 1=1");
    }

    #[test]
    fn test_push_room_filters_default_empty() {
        let sql = build_filtered_sql(Some(&SlidingSyncFilters::default()));
        assert_eq!(sql, "SELECT * FROM t WHERE 1=1");
    }

    #[test]
    fn test_push_room_filters_is_dm_true() {
        let filters = SlidingSyncFilters { is_dm: Some(true), ..Default::default() };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.contains("AND is_dm = $1"), "expected is_dm filter in SQL: {sql}");
    }

    #[test]
    fn test_push_room_filters_is_dm_false() {
        let filters = SlidingSyncFilters { is_dm: Some(false), ..Default::default() };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.contains("AND is_dm = $1"), "expected is_dm filter in SQL: {sql}");
    }

    #[test]
    fn test_push_room_filters_is_encrypted() {
        let filters = SlidingSyncFilters { is_encrypted: Some(true), ..Default::default() };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.contains("AND is_encrypted = $1"), "expected is_encrypted filter in SQL: {sql}");
    }

    #[test]
    fn test_push_room_filters_is_invite() {
        let filters = SlidingSyncFilters { is_invite: Some(true), ..Default::default() };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.contains("AND invited = $1"), "expected invited filter in SQL: {sql}");
    }

    #[test]
    fn test_push_room_filters_is_tombstoned() {
        let filters = SlidingSyncFilters { is_tombstoned: Some(true), ..Default::default() };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.contains("AND is_tombstoned = $1"), "expected is_tombstoned filter in SQL: {sql}");
    }

    #[test]
    fn test_push_room_filters_room_name_like() {
        let filters = SlidingSyncFilters { room_name_like: Some("test".to_string()), ..Default::default() };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.contains("AND COALESCE(name, '') ILIKE $1"), "expected room_name_like filter in SQL: {sql}");
    }

    #[test]
    fn test_push_room_filters_all_combined() {
        let filters = SlidingSyncFilters {
            is_dm: Some(true),
            is_encrypted: Some(true),
            is_invite: Some(false),
            is_tombstoned: Some(false),
            room_name_like: Some("chat".to_string()),
            ..Default::default()
        };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.starts_with("SELECT * FROM t WHERE 1=1"), "expected base query preserved: {sql}");
        assert!(sql.contains("AND is_dm = $1"), "missing is_dm: {sql}");
        assert!(sql.contains("AND is_encrypted = $2"), "missing is_encrypted: {sql}");
        assert!(sql.contains("AND invited = $3"), "missing invited: {sql}");
        assert!(sql.contains("AND is_tombstoned = $4"), "missing is_tombstoned: {sql}");
        assert!(sql.contains("AND COALESCE(name, '') ILIKE $5"), "missing room_name_like: {sql}");
    }

    #[test]
    fn test_push_room_filters_partial() {
        // Only set is_dm and room_name_like; all others remain None → not pushed
        let filters =
            SlidingSyncFilters { is_dm: Some(true), room_name_like: Some("office".to_string()), ..Default::default() };
        let sql = build_filtered_sql(Some(&filters));
        assert!(sql.contains("AND is_dm = $1"), "missing is_dm: {sql}");
        assert!(sql.contains("AND COALESCE(name, '') ILIKE $2"), "missing room_name_like: {sql}");
        assert!(!sql.contains("is_encrypted"), "unexpected is_encrypted: {sql}");
        assert!(!sql.contains("invited"), "unexpected invited: {sql}");
        assert!(!sql.contains("is_tombstoned"), "unexpected is_tombstoned: {sql}");
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;

    async fn test_pool() -> Arc<Pool<Postgres>> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    fn unique_id(prefix: &str) -> String {
        format!("{prefix}_{}", uuid::Uuid::new_v4().simple())
    }

    #[tokio::test]
    async fn test_create_or_update_token_insert_then_update() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");

        let token = storage
            .create_or_update_token(&user_id, &device_id, None)
            .await
            .expect("create_or_update_token should insert");
        assert_eq!(token.user_id, user_id);
        assert_eq!(token.device_id, device_id);
        assert!(token.conn_id.is_none());
        let first_pos = token.pos;

        // Calling again with the same (user, device, conn) should update the existing row
        let updated = storage
            .create_or_update_token(&user_id, &device_id, None)
            .await
            .expect("create_or_update_token should update");
        assert!(updated.pos > first_pos, "pos should advance on update, got {} -> {}", first_pos, updated.pos);

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_create_or_update_token_with_conn_id() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");
        let conn_id = unique_id("conn");

        let token = storage
            .create_or_update_token(&user_id, &device_id, Some(&conn_id))
            .await
            .expect("create_or_update_token with conn_id should succeed");
        assert_eq!(token.conn_id.as_deref(), Some(conn_id.as_str()));

        storage.delete_connection_data(&user_id, &device_id, Some(&conn_id)).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_get_token_returns_none_when_absent() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@missing");
        let device_id = unique_id("DEV");

        let result = storage.get_token(&user_id, &device_id, None).await.expect("get_token should succeed");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_token_returns_inserted_token() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");

        storage.create_or_update_token(&user_id, &device_id, None).await.unwrap();
        let fetched = storage.get_token(&user_id, &device_id, None).await.expect("get_token should succeed");
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().user_id, user_id);

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_validate_pos_valid_and_invalid() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");

        let token = storage.create_or_update_token(&user_id, &device_id, None).await.unwrap();
        let valid_pos = token.pos.to_string();
        assert!(storage.validate_pos(&user_id, &device_id, None, &valid_pos).await.expect("validate_pos valid"));
        // A wrong pos should not validate
        assert!(!storage.validate_pos(&user_id, &device_id, None, "999999999").await.expect("validate_pos invalid"));
        // Nonexistent token should not validate
        assert!(!storage.validate_pos("@nobody:x", "NOPE", None, "1").await.expect("validate_pos nonexistent"));

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_save_list_insert_and_update() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");
        let list_key = "main";

        let sort = vec!["by_recency".to_string()];
        let ranges = vec![(0u32, 10u32)];

        let list = storage
            .save_list(&user_id, &device_id, None, list_key, &sort, None, None, &ranges)
            .await
            .expect("save_list should insert");
        assert_eq!(list.list_key, list_key);

        // Update with new sort/ranges
        let new_sort = vec!["by_name".to_string()];
        let new_ranges = vec![(0u32, 20u32)];
        let updated = storage
            .save_list(&user_id, &device_id, None, list_key, &new_sort, None, None, &new_ranges)
            .await
            .expect("save_list should update");
        assert_eq!(updated.id, list.id, "upsert should keep the same id");

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_get_lists_returns_saved_lists() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");

        let sort = vec!["by_recency".to_string()];
        let ranges = vec![(0u32, 10u32)];
        storage.save_list(&user_id, &device_id, None, "main", &sort, None, None, &ranges).await.unwrap();
        storage.save_list(&user_id, &device_id, None, "archive", &sort, None, None, &ranges).await.unwrap();

        let lists = storage.get_lists(&user_id, &device_id, None).await.expect("get_lists should succeed");
        assert_eq!(lists.len(), 2);

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_get_lists_empty() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");

        let lists = storage.get_lists(&user_id, &device_id, None).await.expect("get_lists should succeed");
        assert!(lists.is_empty());
    }

    #[tokio::test]
    async fn test_delete_list_removes_single_list() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");

        let sort = vec!["by_recency".to_string()];
        let ranges = vec![(0u32, 10u32)];
        storage.save_list(&user_id, &device_id, None, "main", &sort, None, None, &ranges).await.unwrap();
        storage.save_list(&user_id, &device_id, None, "archive", &sort, None, None, &ranges).await.unwrap();

        storage.delete_list(&user_id, &device_id, None, "main").await.expect("delete_list should succeed");

        let lists = storage.get_lists(&user_id, &device_id, None).await.unwrap();
        assert_eq!(lists.len(), 1);
        assert_eq!(lists[0].list_key, "archive");

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_upsert_room_insert_and_update() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");
        let room_id = unique_id("!room");

        let room = storage
            .upsert_room(
                &user_id,
                &device_id,
                &room_id,
                None,
                Some("main"),
                1000,
                1,
                5,
                false,
                false,
                false,
                false,
                Some("First"),
                None,
                1000,
            )
            .await
            .expect("upsert_room should insert");
        assert_eq!(room.room_id, room_id);
        assert_eq!(room.bump_stamp, 1000);
        assert_eq!(room.highlight_count, 1);

        // Update with higher bump stamp; bump_stamp uses GREATEST so it should not decrease
        let updated = storage
            .upsert_room(
                &user_id,
                &device_id,
                &room_id,
                None,
                Some("main"),
                500,
                2,
                8,
                true,
                true,
                false,
                false,
                Some("Second"),
                Some("avatar"),
                2000,
            )
            .await
            .expect("upsert_room should update");
        assert_eq!(updated.id, room.id, "upsert should keep the same id");
        assert_eq!(updated.bump_stamp, 1000, "bump_stamp uses GREATEST so should remain 1000");
        assert_eq!(updated.highlight_count, 2);
        assert_eq!(updated.name.as_deref(), Some("Second"));
        assert_eq!(updated.avatar.as_deref(), Some("avatar"));
        assert!(updated.is_dm);
        assert!(updated.is_encrypted);

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_get_room_returns_none_when_absent() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");
        let room_id = unique_id("!room");

        let result = storage.get_room(&user_id, &device_id, &room_id, None).await.expect("get_room should succeed");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_room_returns_inserted_room() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");
        let room_id = unique_id("!room");

        storage
            .upsert_room(
                &user_id,
                &device_id,
                &room_id,
                None,
                Some("main"),
                1000,
                0,
                0,
                false,
                false,
                false,
                false,
                Some("My Room"),
                None,
                1000,
            )
            .await
            .unwrap();

        let fetched = storage.get_room(&user_id, &device_id, &room_id, None).await.unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().name.as_deref(), Some("My Room"));

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_get_rooms_for_list_pagination() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");
        let list_key = "main";

        // Insert 3 rooms with descending bump stamps
        for i in 0..3 {
            let room_id = unique_id("!room");
            storage
                .upsert_room(
                    &user_id,
                    &device_id,
                    &room_id,
                    None,
                    Some(list_key),
                    1000 - i as i64,
                    0,
                    0,
                    false,
                    false,
                    false,
                    false,
                    None,
                    None,
                    1000,
                )
                .await
                .unwrap();
        }

        let query = SlidingSyncListQuery {
            user_id: &user_id,
            device_id: &device_id,
            conn_id: None,
            list_key,
            start: 0,
            end: 1,
            filters: None,
        };
        let rooms = storage.get_rooms_for_list(query).await.expect("get_rooms_for_list should succeed");
        assert_eq!(rooms.len(), 2, "should return start..end inclusive (2 rooms)");

        // With offset
        let query = SlidingSyncListQuery {
            user_id: &user_id,
            device_id: &device_id,
            conn_id: None,
            list_key,
            start: 2,
            end: 2,
            filters: None,
        };
        let rooms = storage.get_rooms_for_list(query).await.expect("get_rooms_for_list with offset should succeed");
        assert_eq!(rooms.len(), 1, "should return only the 3rd room");

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_count_rooms_for_list() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");
        let list_key = "main";

        let count_before = storage.count_rooms_for_list(&user_id, &device_id, None, list_key, None).await.unwrap();
        assert_eq!(count_before, 0);

        for i in 0..3 {
            let room_id = unique_id("!room");
            storage
                .upsert_room(
                    &user_id,
                    &device_id,
                    &room_id,
                    None,
                    Some(list_key),
                    1000 - i,
                    0,
                    0,
                    false,
                    false,
                    false,
                    false,
                    None,
                    None,
                    1000,
                )
                .await
                .unwrap();
        }

        let count_after = storage.count_rooms_for_list(&user_id, &device_id, None, list_key, None).await.unwrap();
        assert_eq!(count_after, 3);

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_count_rooms_for_list_with_filters() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");
        let list_key = "main";

        // Two DM rooms, one non-DM
        for (is_dm, bump) in [(true, 1000), (true, 900), (false, 800)] {
            let room_id = unique_id("!room");
            storage
                .upsert_room(
                    &user_id,
                    &device_id,
                    &room_id,
                    None,
                    Some(list_key),
                    bump,
                    0,
                    0,
                    is_dm,
                    false,
                    false,
                    false,
                    None,
                    None,
                    1000,
                )
                .await
                .unwrap();
        }

        let filters = SlidingSyncFilters { is_dm: Some(true), ..Default::default() };
        let count = storage.count_rooms_for_list(&user_id, &device_id, None, list_key, Some(&filters)).await.unwrap();
        assert_eq!(count, 2);

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_delete_room_removes_room() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");
        let room_id = unique_id("!room");

        storage
            .upsert_room(
                &user_id,
                &device_id,
                &room_id,
                None,
                Some("main"),
                1000,
                0,
                0,
                false,
                false,
                false,
                false,
                None,
                None,
                1000,
            )
            .await
            .unwrap();
        assert!(storage.get_room(&user_id, &device_id, &room_id, None).await.unwrap().is_some());

        storage.delete_room(&user_id, &device_id, &room_id, None).await.expect("delete_room should succeed");
        assert!(storage.get_room(&user_id, &device_id, &room_id, None).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_update_notification_counts() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");
        let room_id = unique_id("!room");

        storage
            .upsert_room(
                &user_id,
                &device_id,
                &room_id,
                None,
                Some("main"),
                1000,
                0,
                0,
                false,
                false,
                false,
                false,
                None,
                None,
                1000,
            )
            .await
            .unwrap();

        storage
            .update_notification_counts(&user_id, &device_id, &room_id, None, 7, 42)
            .await
            .expect("update_notification_counts should succeed");

        let room = storage.get_room(&user_id, &device_id, &room_id, None).await.unwrap().unwrap();
        assert_eq!(room.highlight_count, 7);
        assert_eq!(room.notification_count, 42);

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_bump_room_does_not_decrease() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");
        let room_id = unique_id("!room");

        storage
            .upsert_room(
                &user_id,
                &device_id,
                &room_id,
                None,
                Some("main"),
                1000,
                0,
                0,
                false,
                false,
                false,
                false,
                None,
                None,
                1000,
            )
            .await
            .unwrap();

        // Lower bump should not decrease (uses GREATEST)
        storage.bump_room(&user_id, &device_id, &room_id, None, 500).await.expect("bump_room lower");
        let room = storage.get_room(&user_id, &device_id, &room_id, None).await.unwrap().unwrap();
        assert_eq!(room.bump_stamp, 1000, "bump_room should not decrease bump_stamp");

        // Higher bump should increase
        storage.bump_room(&user_id, &device_id, &room_id, None, 2000).await.expect("bump_room higher");
        let room = storage.get_room(&user_id, &device_id, &room_id, None).await.unwrap().unwrap();
        assert_eq!(room.bump_stamp, 2000, "bump_room should increase bump_stamp");

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_cleanup_expired_tokens_removes_only_expired() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");

        // Create a valid (non-expired) token
        storage.create_or_update_token(&user_id, &device_id, None).await.unwrap();

        // Manually expire one token row
        let past_ts = chrono::Utc::now().timestamp_millis() - 1000;
        sqlx::query("UPDATE sliding_sync_tokens SET expires_at = $1 WHERE user_id = $2 AND device_id = $3")
            .bind(past_ts)
            .bind(&user_id)
            .bind(&device_id)
            .execute(&*pool)
            .await
            .expect("should expire token");

        let removed = storage.cleanup_expired_tokens().await.expect("cleanup_expired_tokens should succeed");
        assert!(removed >= 1, "should have removed at least the expired token");

        // The expired token should no longer be retrievable
        let fetched = storage.get_token(&user_id, &device_id, None).await.unwrap();
        assert!(fetched.is_none());
    }

    #[tokio::test]
    async fn test_count_room_token_sync() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let room_id = unique_id("!room");
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");

        let count_before = storage.count_room_token_sync(&room_id).await.expect("count_room_token_sync should succeed");
        assert_eq!(count_before, 0);

        storage
            .upsert_room(
                &user_id,
                &device_id,
                &room_id,
                None,
                Some("main"),
                1000,
                0,
                0,
                false,
                false,
                false,
                false,
                None,
                None,
                1000,
            )
            .await
            .unwrap();

        let count_after = storage.count_room_token_sync(&room_id).await.unwrap();
        assert_eq!(count_after, 1);

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_list_room_token_sync_without_cursor() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let room_id = unique_id("!room");
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");

        storage
            .upsert_room(
                &user_id,
                &device_id,
                &room_id,
                None,
                Some("main"),
                1000,
                1,
                2,
                false,
                false,
                false,
                false,
                Some("Room"),
                None,
                1000,
            )
            .await
            .unwrap();

        let entries =
            storage.list_room_token_sync(&room_id, 10, None).await.expect("list_room_token_sync should succeed");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].user_id, user_id);
        assert_eq!(entries[0].device_id, device_id);
        assert_eq!(entries[0].name.as_deref(), Some("Room"));
        assert!(!entries[0].is_expired);

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_list_room_token_sync_limit_truncates() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let room_id = unique_id("!room");

        // Insert 3 entries for the same room
        for i in 0..3 {
            let user_id = unique_id(&format!("@user{i}"));
            let device_id = unique_id("DEV");
            storage
                .upsert_room(
                    &user_id,
                    &device_id,
                    &room_id,
                    None,
                    Some("main"),
                    1000 - i as i64,
                    0,
                    0,
                    false,
                    false,
                    false,
                    false,
                    None,
                    None,
                    1000,
                )
                .await
                .unwrap();
        }

        let entries = storage.list_room_token_sync(&room_id, 2, None).await.unwrap();
        assert_eq!(entries.len(), 2, "limit should truncate to 2");

        // Cleanup all inserted rows for this room
        sqlx::query("DELETE FROM sliding_sync_rooms WHERE room_id = $1").bind(&room_id).execute(&*pool).await.unwrap();
    }

    #[tokio::test]
    async fn test_get_global_account_data_empty() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");

        let data = storage.get_global_account_data(&user_id).await.expect("get_global_account_data should succeed");
        assert!(data.as_object().map_or(true, |m| m.is_empty()));
    }

    #[tokio::test]
    async fn test_get_global_account_data_with_rows() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO account_data (user_id, data_type, content, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $4)
            ON CONFLICT (user_id, data_type) DO UPDATE SET content = EXCLUDED.content, updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(&user_id)
        .bind("m.direct")
        .bind(serde_json::json!({"@bob:example.com": ["!room:example.com"]}))
        .bind(now)
        .execute(&*pool)
        .await
        .expect("should insert account_data");

        let data = storage.get_global_account_data(&user_id).await.expect("get_global_account_data should succeed");
        let obj = data.as_object().expect("should be object");
        assert!(obj.contains_key("m.direct"));

        sqlx::query("DELETE FROM account_data WHERE user_id = $1").bind(&user_id).execute(&*pool).await.unwrap();
    }

    #[tokio::test]
    async fn test_get_room_account_data_empty_input() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());

        let data = storage.get_room_account_data("@nobody:x", &[]).await.expect("get_room_account_data empty");
        assert_eq!(data, serde_json::json!({}));
    }

    #[tokio::test]
    async fn test_get_room_account_data_with_rows() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let room_id = unique_id("!room");
        let now = chrono::Utc::now().timestamp();

        sqlx::query(
            r#"
            INSERT INTO room_account_data (user_id, room_id, data_type, data, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $5)
            ON CONFLICT (user_id, room_id, data_type) DO UPDATE SET data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(&user_id)
        .bind(&room_id)
        .bind("m.fully_read")
        .bind(serde_json::json!({"event_id": "$event:example.com"}))
        .bind(now)
        .execute(&*pool)
        .await
        .expect("should insert room_account_data");

        let data = storage
            .get_room_account_data(&user_id, &[room_id.clone()])
            .await
            .expect("get_room_account_data should succeed");
        let obj = data.as_object().expect("should be object");
        assert!(obj.contains_key(&room_id), "should contain the room_id key");

        sqlx::query("DELETE FROM room_account_data WHERE user_id = $1").bind(&user_id).execute(&*pool).await.unwrap();
    }

    #[tokio::test]
    async fn test_get_receipts_for_rooms_empty_input() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());

        let data = storage.get_receipts_for_rooms(&[]).await.expect("get_receipts_for_rooms empty");
        assert_eq!(data, serde_json::json!({}));
    }

    #[tokio::test]
    async fn test_get_receipts_for_rooms_with_rows() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let room_id = unique_id("!room");
        let user_id = unique_id("@user");
        let event_id = unique_id("$event");
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            r#"
            INSERT INTO event_receipts (event_id, room_id, user_id, receipt_type, ts, data, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $5, $5)
            ON CONFLICT (event_id, room_id, user_id, receipt_type) DO UPDATE SET ts = EXCLUDED.ts, data = EXCLUDED.data, updated_ts = EXCLUDED.updated_ts
            "#,
        )
        .bind(&event_id)
        .bind(&room_id)
        .bind(&user_id)
        .bind("m.read")
        .bind(now)
        .bind(serde_json::json!({}))
        .execute(&*pool)
        .await
        .expect("should insert event_receipt");

        let data =
            storage.get_receipts_for_rooms(&[room_id.clone()]).await.expect("get_receipts_for_rooms should succeed");
        let obj = data.as_object().expect("should be object");
        assert!(obj.contains_key(&room_id), "should contain the room_id key");

        sqlx::query("DELETE FROM event_receipts WHERE room_id = $1").bind(&room_id).execute(&*pool).await.unwrap();
    }

    #[tokio::test]
    async fn test_delete_connection_data_removes_all() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");

        // Seed token, list, and room
        storage.create_or_update_token(&user_id, &device_id, None).await.unwrap();
        let sort = vec!["by_recency".to_string()];
        let ranges = vec![(0u32, 10u32)];
        storage.save_list(&user_id, &device_id, None, "main", &sort, None, None, &ranges).await.unwrap();
        storage
            .upsert_room(
                &user_id,
                &device_id,
                &unique_id("!room"),
                None,
                Some("main"),
                1000,
                0,
                0,
                false,
                false,
                false,
                false,
                None,
                None,
                1000,
            )
            .await
            .unwrap();

        storage
            .delete_connection_data(&user_id, &device_id, None)
            .await
            .expect("delete_connection_data should succeed");

        // All three should be gone
        assert!(storage.get_token(&user_id, &device_id, None).await.unwrap().is_none());
        assert!(storage.get_lists(&user_id, &device_id, None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_get_rooms_for_list_with_filters() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");
        let list_key = "main";

        // One encrypted room, one unencrypted
        for (is_encrypted, bump) in [(true, 1000), (false, 900)] {
            let room_id = unique_id("!room");
            storage
                .upsert_room(
                    &user_id,
                    &device_id,
                    &room_id,
                    None,
                    Some(list_key),
                    bump,
                    0,
                    0,
                    false,
                    is_encrypted,
                    false,
                    false,
                    None,
                    None,
                    1000,
                )
                .await
                .unwrap();
        }

        let filters = SlidingSyncFilters { is_encrypted: Some(true), ..Default::default() };
        let query = SlidingSyncListQuery {
            user_id: &user_id,
            device_id: &device_id,
            conn_id: None,
            list_key,
            start: 0,
            end: 9,
            filters: Some(&filters),
        };
        let rooms = storage.get_rooms_for_list(query).await.expect("get_rooms_for_list with filters");
        assert_eq!(rooms.len(), 1);
        assert!(rooms[0].is_encrypted);

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }

    #[tokio::test]
    async fn test_get_rooms_for_list_with_room_name_like_filter() {
        let pool = test_pool().await;
        let storage = SlidingSyncStorage::new(pool.clone());
        let user_id = unique_id("@user");
        let device_id = unique_id("DEV");
        let list_key = "main";

        for (name, bump) in [("Project Alpha", 1000), ("Project Beta", 900), ("Other", 800)] {
            let room_id = unique_id("!room");
            storage
                .upsert_room(
                    &user_id,
                    &device_id,
                    &room_id,
                    None,
                    Some(list_key),
                    bump,
                    0,
                    0,
                    false,
                    false,
                    false,
                    false,
                    Some(name),
                    None,
                    1000,
                )
                .await
                .unwrap();
        }

        let filters = SlidingSyncFilters { room_name_like: Some("project".to_string()), ..Default::default() };
        let query = SlidingSyncListQuery {
            user_id: &user_id,
            device_id: &device_id,
            conn_id: None,
            list_key,
            start: 0,
            end: 9,
            filters: Some(&filters),
        };
        let rooms = storage.get_rooms_for_list(query).await.expect("get_rooms_for_list with name filter");
        assert_eq!(rooms.len(), 2, "should match both 'Project' rooms");

        storage.delete_connection_data(&user_id, &device_id, None).await.expect("cleanup");
    }
}

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait SlidingSyncStoreApi: Send + Sync {
    async fn create_or_update_token(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<SlidingSyncToken, sqlx::Error>;
    async fn get_token(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncToken>, sqlx::Error>;
    async fn validate_pos(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        pos: &str,
    ) -> Result<bool, sqlx::Error>;
    async fn save_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        sort: &[String],
        filters: Option<&SlidingSyncFilters>,
        room_subscription: Option<&serde_json::Value>,
        ranges: &[(u32, u32)],
    ) -> Result<SlidingSyncList, sqlx::Error>;
    async fn get_lists(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Vec<SlidingSyncList>, sqlx::Error>;
    async fn delete_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
    ) -> Result<(), sqlx::Error>;
    async fn upsert_room(
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
    ) -> Result<SlidingSyncRoom, sqlx::Error>;
    async fn get_rooms_for_list(
        &self,
        query_params: SlidingSyncListQuery<'_>,
    ) -> Result<Vec<SlidingSyncRoom>, sqlx::Error>;
    async fn count_rooms_for_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        filters: Option<&SlidingSyncFilters>,
    ) -> Result<i64, sqlx::Error>;
    async fn get_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncRoom>, sqlx::Error>;
    async fn materialize_room_from_activity(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncRoom>, sqlx::Error>;
    async fn delete_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), sqlx::Error>;
    async fn update_notification_counts(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        highlight_count: i32,
        notification_count: i32,
    ) -> Result<(), sqlx::Error>;
    async fn bump_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        bump_stamp: i64,
    ) -> Result<(), sqlx::Error>;
    async fn cleanup_expired_tokens(&self) -> Result<u64, sqlx::Error>;
    async fn list_room_token_sync(
        &self,
        room_id: &str,
        limit: i64,
        from: Option<&RoomTokenSyncCursor>,
    ) -> Result<Vec<AdminRoomTokenSyncEntry>, sqlx::Error>;
    async fn count_room_token_sync(&self, room_id: &str) -> Result<i64, sqlx::Error>;
    async fn get_global_account_data(&self, user_id: &str) -> Result<serde_json::Value, sqlx::Error>;
    async fn get_room_account_data(&self, user_id: &str, room_ids: &[String])
        -> Result<serde_json::Value, sqlx::Error>;
    async fn get_receipts_for_rooms(&self, room_ids: &[String]) -> Result<serde_json::Value, sqlx::Error>;
    async fn delete_connection_data(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), sqlx::Error>;
}

#[async_trait]
impl SlidingSyncStoreApi for SlidingSyncStorage {
    async fn create_or_update_token(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<SlidingSyncToken, sqlx::Error> {
        self.create_or_update_token(user_id, device_id, conn_id).await
    }
    async fn get_token(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncToken>, sqlx::Error> {
        self.get_token(user_id, device_id, conn_id).await
    }
    async fn validate_pos(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        pos: &str,
    ) -> Result<bool, sqlx::Error> {
        self.validate_pos(user_id, device_id, conn_id, pos).await
    }
    async fn save_list(
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
        self.save_list(user_id, device_id, conn_id, list_key, sort, filters, room_subscription, ranges).await
    }
    async fn get_lists(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Vec<SlidingSyncList>, sqlx::Error> {
        self.get_lists(user_id, device_id, conn_id).await
    }
    async fn delete_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
    ) -> Result<(), sqlx::Error> {
        self.delete_list(user_id, device_id, conn_id, list_key).await
    }
    async fn upsert_room(
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
        self.upsert_room(
            user_id,
            device_id,
            room_id,
            conn_id,
            list_key,
            bump_stamp,
            highlight_count,
            notification_count,
            is_dm,
            is_encrypted,
            is_tombstoned,
            invited,
            name,
            avatar,
            timestamp,
        )
        .await
    }
    async fn get_rooms_for_list(
        &self,
        query_params: SlidingSyncListQuery<'_>,
    ) -> Result<Vec<SlidingSyncRoom>, sqlx::Error> {
        self.get_rooms_for_list(query_params).await
    }
    async fn count_rooms_for_list(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        list_key: &str,
        filters: Option<&SlidingSyncFilters>,
    ) -> Result<i64, sqlx::Error> {
        self.count_rooms_for_list(user_id, device_id, conn_id, list_key, filters).await
    }
    async fn get_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncRoom>, sqlx::Error> {
        self.get_room(user_id, device_id, room_id, conn_id).await
    }
    async fn materialize_room_from_activity(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<Option<SlidingSyncRoom>, sqlx::Error> {
        self.materialize_room_from_activity(user_id, device_id, room_id, conn_id).await
    }
    async fn delete_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.delete_room(user_id, device_id, room_id, conn_id).await
    }
    async fn update_notification_counts(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        highlight_count: i32,
        notification_count: i32,
    ) -> Result<(), sqlx::Error> {
        self.update_notification_counts(user_id, device_id, room_id, conn_id, highlight_count, notification_count).await
    }
    async fn bump_room(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        conn_id: Option<&str>,
        bump_stamp: i64,
    ) -> Result<(), sqlx::Error> {
        self.bump_room(user_id, device_id, room_id, conn_id, bump_stamp).await
    }
    async fn cleanup_expired_tokens(&self) -> Result<u64, sqlx::Error> {
        self.cleanup_expired_tokens().await
    }
    async fn list_room_token_sync(
        &self,
        room_id: &str,
        limit: i64,
        from: Option<&RoomTokenSyncCursor>,
    ) -> Result<Vec<AdminRoomTokenSyncEntry>, sqlx::Error> {
        self.list_room_token_sync(room_id, limit, from).await
    }
    async fn count_room_token_sync(&self, room_id: &str) -> Result<i64, sqlx::Error> {
        self.count_room_token_sync(room_id).await
    }
    async fn get_global_account_data(&self, user_id: &str) -> Result<serde_json::Value, sqlx::Error> {
        self.get_global_account_data(user_id).await
    }
    async fn get_room_account_data(
        &self,
        user_id: &str,
        room_ids: &[String],
    ) -> Result<serde_json::Value, sqlx::Error> {
        self.get_room_account_data(user_id, room_ids).await
    }
    async fn get_receipts_for_rooms(&self, room_ids: &[String]) -> Result<serde_json::Value, sqlx::Error> {
        self.get_receipts_for_rooms(room_ids).await
    }
    async fn delete_connection_data(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.delete_connection_data(user_id, device_id, conn_id).await
    }
}
