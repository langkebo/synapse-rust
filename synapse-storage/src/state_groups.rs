use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tracing;

/// SELECT list for the `state_groups` table.
const STATE_GROUP_COLS: &str = "id, room_id, event_id, state_hash, created_ts";

/// SELECT list for the `state_group_edges` table.
#[allow(dead_code)]
const STATE_GROUP_EDGE_COLS: &str = "state_group_id, prev_state_group_id";

/// Columns for `event_to_state_groups`.
#[allow(dead_code)]
const EVENT_TO_STATE_GROUP_COLS: &str = "event_id, state_group_id";

/// Columns for `state_group_state`.
const STATE_GROUP_STATE_COLS: &str = "state_group_id, event_type, state_key, event_id";

/// Inner columns for `state_group_state` (without state_group_id).
const STATE_GROUP_STATE_INNER_COLS: &str = "event_type, state_key, event_id";

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StateGroup {
    pub id: i64,
    pub room_id: String,
    pub event_id: String,
    pub state_hash: String,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StateGroupEdge {
    pub state_group_id: i64,
    pub prev_state_group_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EventToStateGroup {
    pub event_id: String,
    pub state_group_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StateGroupState {
    pub state_group_id: i64,
    pub event_type: String,
    pub state_key: String,
    pub event_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateGroupStateEntry {
    pub event_type: String,
    pub state_key: String,
    pub event_id: String,
}

pub struct StateGroupStorage {
    pool: PgPool,
}

impl StateGroupStorage {
    pub fn new(pool: &Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.as_ref().clone() }
    }

    // ---- state_groups ---- //

    /// 创建一个 state_group 并返回其 ID 和状态哈希
    pub async fn create_state_group(
        &self,
        room_id: &str,
        event_id: &str,
        state_hash: &str,
        created_ts: i64,
    ) -> Result<i64, sqlx::Error> {
        tracing::debug!(room_id = %room_id, state_hash = %state_hash, "Creating state group");
        let row = sqlx::query_as::<_, (i64,)>(
            r#"
            INSERT INTO state_groups (room_id, event_id, state_hash, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (state_hash) DO UPDATE SET event_id = $2
            RETURNING id
            "#,
        )
        .bind(room_id)
        .bind(event_id)
        .bind(state_hash)
        .bind(created_ts)
        .fetch_one(&self.pool)
        .await?;

        Ok(row.0)
    }

    pub async fn get_state_group(&self, id: i64) -> Result<Option<StateGroup>, sqlx::Error> {
        sqlx::query_as::<_, StateGroup>(&format!("SELECT {} FROM state_groups WHERE id = $1", STATE_GROUP_COLS))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn get_state_group_by_event(&self, event_id: &str) -> Result<Option<StateGroup>, sqlx::Error> {
        sqlx::query_as::<_, StateGroup>(&format!("SELECT {} FROM state_groups WHERE event_id = $1", STATE_GROUP_COLS))
            .bind(event_id)
            .fetch_optional(&self.pool)
            .await
    }

    pub async fn get_room_state_groups(&self, room_id: &str, limit: i64) -> Result<Vec<StateGroup>, sqlx::Error> {
        sqlx::query_as::<_, StateGroup>(&format!(
            "SELECT {}
                 FROM state_groups WHERE room_id = $1 ORDER BY id DESC LIMIT $2",
            STATE_GROUP_COLS
        ))
        .bind(room_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
    }

    // ---- state_group_edges ---- //

    /// 添加 state_group 边关系
    pub async fn add_state_group_edge(&self, state_group_id: i64, prev_state_group_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO state_group_edges (state_group_id, prev_state_group_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(state_group_id)
        .bind(prev_state_group_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 批量添加 state_group 边
    pub async fn add_state_group_edges(
        &self,
        state_group_id: i64,
        prev_state_group_ids: &[i64],
    ) -> Result<(), sqlx::Error> {
        if prev_state_group_ids.is_empty() {
            return Ok(());
        }
        sqlx::query(
            r#"
            INSERT INTO state_group_edges (state_group_id, prev_state_group_id)
            SELECT $1, unnest($2::bigint[])
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(state_group_id)
        .bind(prev_state_group_ids)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_prev_state_groups(&self, state_group_id: i64) -> Result<Vec<i64>, sqlx::Error> {
        let rows: Vec<(i64,)> =
            sqlx::query_as(r#"SELECT prev_state_group_id FROM state_group_edges WHERE state_group_id = $1"#)
                .bind(state_group_id)
                .fetch_all(&self.pool)
                .await?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    pub async fn get_next_state_groups(&self, prev_state_group_id: i64) -> Result<Vec<i64>, sqlx::Error> {
        let rows: Vec<(i64,)> =
            sqlx::query_as(r#"SELECT state_group_id FROM state_group_edges WHERE prev_state_group_id = $1"#)
                .bind(prev_state_group_id)
                .fetch_all(&self.pool)
                .await?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    // ---- event_to_state_groups ---- //

    /// 绑定事件到 state_group
    pub async fn bind_event_to_state_group(&self, event_id: &str, state_group_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO event_to_state_groups (event_id, state_group_id)
            VALUES ($1, $2)
            ON CONFLICT (event_id) DO UPDATE SET state_group_id = $2
            "#,
        )
        .bind(event_id)
        .bind(state_group_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_state_group_for_event(&self, event_id: &str) -> Result<Option<i64>, sqlx::Error> {
        let row: Option<(i64,)> =
            sqlx::query_as(r#"SELECT state_group_id FROM event_to_state_groups WHERE event_id = $1"#)
                .bind(event_id)
                .fetch_optional(&self.pool)
                .await?;

        Ok(row.map(|r| r.0))
    }

    /// 批量绑定事件到 state_group（单次 INSERT，避免 N+1）
    pub async fn batch_bind_events_to_state_group(
        &self,
        event_ids: &[String],
        state_group_id: i64,
    ) -> Result<(), sqlx::Error> {
        if event_ids.is_empty() {
            return Ok(());
        }
        tracing::debug!(
            state_group_id = state_group_id,
            count = event_ids.len(),
            "Batch binding events to state group"
        );
        sqlx::query(
            r#"
            INSERT INTO event_to_state_groups (event_id, state_group_id)
            SELECT unnest($1::text[]), $2
            ON CONFLICT (event_id) DO UPDATE SET state_group_id = $2
            "#,
        )
        .bind(event_ids)
        .bind(state_group_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // ---- state_group_state ---- //

    /// 设置 state_group 中某个状态条目的 event
    pub async fn set_state_entry(
        &self,
        state_group_id: i64,
        event_type: &str,
        state_key: &str,
        event_id: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO state_group_state (state_group_id, event_type, state_key, event_id)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (state_group_id, event_type, state_key) DO UPDATE SET event_id = $4
            "#,
        )
        .bind(state_group_id)
        .bind(event_type)
        .bind(state_key)
        .bind(event_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 批量设置一个 state_group 的状态条目（单次 INSERT，避免 N+1）
    pub async fn set_state_entries(
        &self,
        state_group_id: i64,
        entries: &[StateGroupStateEntry],
    ) -> Result<(), sqlx::Error> {
        if entries.is_empty() {
            return Ok(());
        }

        tracing::debug!(state_group_id = state_group_id, count = entries.len(), "Batch setting state entries");

        let event_types: Vec<&str> = entries.iter().map(|e| e.event_type.as_str()).collect();
        let state_keys: Vec<&str> = entries.iter().map(|e| e.state_key.as_str()).collect();
        let event_ids: Vec<&str> = entries.iter().map(|e| e.event_id.as_str()).collect();

        sqlx::query(
            r#"
            INSERT INTO state_group_state (state_group_id, event_type, state_key, event_id)
            SELECT $1, unnest($2::text[]), unnest($3::text[]), unnest($4::text[])
            ON CONFLICT (state_group_id, event_type, state_key) DO UPDATE SET event_id = EXCLUDED.event_id
            "#,
        )
        .bind(state_group_id)
        .bind(event_types)
        .bind(state_keys)
        .bind(event_ids)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_state_at_group(&self, state_group_id: i64) -> Result<Vec<StateGroupState>, sqlx::Error> {
        sqlx::query_as::<_, StateGroupState>(&format!(
            "SELECT {}
                 FROM state_group_state WHERE state_group_id = $1",
            STATE_GROUP_STATE_COLS
        ))
        .bind(state_group_id)
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_state_entry(
        &self,
        state_group_id: i64,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let row: Option<(String,)> = sqlx::query_as(
            r#"SELECT event_id FROM state_group_state
               WHERE state_group_id = $1 AND event_type = $2 AND state_key = $3"#,
        )
        .bind(state_group_id)
        .bind(event_type)
        .bind(state_key)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| r.0))
    }

    /// 递归获取某个 state_group 的状态（沿 DAG 边向上查找）
    pub async fn resolve_state_for_group(
        &self,
        state_group_id: i64,
    ) -> Result<std::collections::HashMap<(String, String), String>, sqlx::Error> {
        use std::collections::{HashMap, HashSet, VecDeque};

        tracing::debug!(state_group_id = state_group_id, "Resolving state for group");

        let mut result: HashMap<(String, String), String> = HashMap::new();
        let mut visited: HashSet<i64> = HashSet::new();
        let mut queue: VecDeque<i64> = VecDeque::new();
        queue.push_back(state_group_id);

        let mut depth = 0i64;

        while let Some(sg_id) = queue.pop_front() {
            depth += 1;
            if depth > 100 {
                tracing::warn!(
                    state_group_id = state_group_id,
                    depth = depth,
                    "State resolution depth exceeds 100, possible cycle"
                );
            }

            if !visited.insert(sg_id) {
                continue;
            }

            // Load state entries for this group
            let state_rows: Vec<(String, String, String)> = sqlx::query_as(&format!(
                "SELECT {}
                     FROM state_group_state WHERE state_group_id = $1",
                STATE_GROUP_STATE_INNER_COLS
            ))
            .bind(sg_id)
            .fetch_all(&self.pool)
            .await?;

            for (event_type, state_key, event_id) in state_rows {
                let key = (event_type, state_key);
                result.entry(key).or_insert(event_id);
            }

            // Load prev groups
            let prev_rows: Vec<(i64,)> =
                sqlx::query_as(r#"SELECT prev_state_group_id FROM state_group_edges WHERE state_group_id = $1"#)
                    .bind(sg_id)
                    .fetch_all(&self.pool)
                    .await?;

            for (prev_id,) in prev_rows {
                if !visited.contains(&prev_id) {
                    queue.push_back(prev_id);
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::postgres::PgPoolOptions;
    use sqlx::{Pool, Postgres};

    async fn test_pool() -> Arc<PgPool> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn ensure_test_room(pool: &Pool<Postgres>, room_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"INSERT INTO rooms (room_id, room_version, is_public, creator, created_ts)
               VALUES ($1, '10', false, $2, $3)
               ON CONFLICT (room_id) DO NOTHING"#,
        )
        .bind(room_id)
        .bind("@test:localhost")
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test room");
    }

    async fn ensure_test_event(pool: &Pool<Postgres>, event_id: &str, room_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            r#"INSERT INTO events (event_id, room_id, sender, event_type, content, origin_server_ts, state_key, depth)
               VALUES ($1, $2, '@test:localhost', 'm.room.message', '{}', $3, '', 0)
               ON CONFLICT (event_id) DO NOTHING"#,
        )
        .bind(event_id)
        .bind(room_id)
        .bind(now)
        .execute(pool)
        .await
        .expect("failed to create test event");
    }

    async fn ensure_test_room_and_event(pool: &Pool<Postgres>, room_id: &str, event_id: &str) {
        ensure_test_room(pool, room_id).await;
        ensure_test_event(pool, event_id, room_id).await;
    }

    async fn cleanup_test_data(pool: &Pool<Postgres>, room_id: &str) {
        // Delete FK children first, then parents
        sqlx::query(
            "DELETE FROM state_group_state WHERE state_group_id IN (SELECT id FROM state_groups WHERE room_id = $1)",
        )
        .bind(room_id)
        .execute(pool)
        .await
        .ok();
        sqlx::query("DELETE FROM event_to_state_groups WHERE state_group_id IN (SELECT id FROM state_groups WHERE room_id = $1)")
            .bind(room_id).execute(pool).await.ok();
        sqlx::query(
            "DELETE FROM state_group_edges WHERE state_group_id IN (SELECT id FROM state_groups WHERE room_id = $1)",
        )
        .bind(room_id)
        .execute(pool)
        .await
        .ok();
        sqlx::query("DELETE FROM state_group_edges WHERE prev_state_group_id IN (SELECT id FROM state_groups WHERE room_id = $1)")
            .bind(room_id).execute(pool).await.ok();
        sqlx::query("DELETE FROM state_groups WHERE room_id = $1").bind(room_id).execute(pool).await.ok();
        sqlx::query("DELETE FROM events WHERE room_id = $1").bind(room_id).execute(pool).await.ok();
        sqlx::query("DELETE FROM rooms WHERE room_id = $1").bind(room_id).execute(pool).await.ok();
    }

    // ---- state_groups CRUD ---- //

    #[tokio::test]
    async fn test_create_state_group() {
        let pool = test_pool().await;
        let storage = StateGroupStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!test_create_sg_{suffix}:localhost");
        let event_id = format!("$create_sg_ev_{suffix}");

        cleanup_test_data(&pool, &room_id).await;
        ensure_test_room_and_event(&pool, &room_id, &event_id).await;

        let now = chrono::Utc::now().timestamp_millis();
        let state_hash = format!("hash_create_{suffix}");
        let id = storage
            .create_state_group(&room_id, &event_id, &state_hash, now)
            .await
            .expect("create_state_group should succeed");

        assert!(id > 0, "created state group ID should be positive");

        cleanup_test_data(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_get_state_group_found() {
        let pool = test_pool().await;
        let storage = StateGroupStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!test_get_sg_{suffix}:localhost");
        let event_id = format!("$get_sg_ev_{suffix}");

        cleanup_test_data(&pool, &room_id).await;
        ensure_test_room_and_event(&pool, &room_id, &event_id).await;

        let now = chrono::Utc::now().timestamp_millis();
        let state_hash = format!("hash_get_{suffix}");
        let id =
            storage.create_state_group(&room_id, &event_id, &state_hash, now).await.expect("create should succeed");

        let found = storage
            .get_state_group(id)
            .await
            .expect("get_state_group query should succeed")
            .expect("state group should be found");

        assert_eq!(found.id, id);
        assert_eq!(found.room_id, room_id);
        assert_eq!(found.event_id, event_id);
        assert_eq!(found.state_hash, state_hash);

        cleanup_test_data(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_get_state_group_not_found() {
        let pool = test_pool().await;
        let storage = StateGroupStorage::new(&pool);

        let result = storage.get_state_group(99999999).await.expect("query should succeed");

        assert!(result.is_none(), "nonexistent state group should return None");
    }

    #[tokio::test]
    async fn test_get_state_group_by_event() {
        let pool = test_pool().await;
        let storage = StateGroupStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!test_sg_by_ev_{suffix}:localhost");
        let event_id = format!("$sg_by_ev_{suffix}");

        cleanup_test_data(&pool, &room_id).await;
        ensure_test_room_and_event(&pool, &room_id, &event_id).await;

        let now = chrono::Utc::now().timestamp_millis();
        let state_hash = format!("hash_by_ev_{suffix}");
        let id =
            storage.create_state_group(&room_id, &event_id, &state_hash, now).await.expect("create should succeed");

        let found = storage
            .get_state_group_by_event(&event_id)
            .await
            .expect("query should succeed")
            .expect("state group should be found by event_id");

        assert_eq!(found.id, id);
        assert_eq!(found.event_id, event_id);

        // Test not found by nonexistent event_id
        let not_found = storage.get_state_group_by_event("$nonexistent_event").await.expect("query should succeed");
        assert!(not_found.is_none());

        cleanup_test_data(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_get_room_state_groups() {
        let pool = test_pool().await;
        let storage = StateGroupStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!test_room_sgs_{suffix}:localhost");
        let event_id1 = format!("$room_sg_ev1_{suffix}");
        let event_id2 = format!("$room_sg_ev2_{suffix}");

        cleanup_test_data(&pool, &room_id).await;
        ensure_test_room_and_event(&pool, &room_id, &event_id1).await;
        ensure_test_event(&pool, &event_id2, &room_id).await;

        let now = chrono::Utc::now().timestamp_millis();
        let id1 = storage
            .create_state_group(&room_id, &event_id1, &format!("hash_a_{suffix}"), now)
            .await
            .expect("create sg1");
        let id2 = storage
            .create_state_group(&room_id, &event_id2, &format!("hash_b_{suffix}"), now)
            .await
            .expect("create sg2");

        let results = storage.get_room_state_groups(&room_id, 10).await.expect("get_room_state_groups should succeed");

        assert!(results.len() >= 2);
        let ids: Vec<i64> = results.iter().map(|sg| sg.id).collect();
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));

        // Test with limit
        let limited = storage.get_room_state_groups(&room_id, 1).await.expect("limited query should succeed");
        assert_eq!(limited.len(), 1);

        // Test empty room
        let empty_room = format!("!empty_room_{suffix}:localhost");
        ensure_test_room(&pool, &empty_room).await;
        let empty_results = storage.get_room_state_groups(&empty_room, 10).await.expect("query should succeed");
        assert!(empty_results.is_empty());

        cleanup_test_data(&pool, &room_id).await;
        cleanup_test_data(&pool, &empty_room).await;
    }

    // ---- state_group_edges ---- //

    #[tokio::test]
    async fn test_add_state_group_edge() {
        let pool = test_pool().await;
        let storage = StateGroupStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!test_edge_{suffix}:localhost");
        let ev_a = format!("$edge_ev_a_{suffix}");
        let ev_b = format!("$edge_ev_b_{suffix}");

        cleanup_test_data(&pool, &room_id).await;
        ensure_test_room_and_event(&pool, &room_id, &ev_a).await;
        ensure_test_event(&pool, &ev_b, &room_id).await;

        let now = chrono::Utc::now().timestamp_millis();
        let sg_a = storage
            .create_state_group(&room_id, &ev_a, &format!("edge_hash_a_{suffix}"), now)
            .await
            .expect("create sg_a");
        let sg_b = storage
            .create_state_group(&room_id, &ev_b, &format!("edge_hash_b_{suffix}"), now)
            .await
            .expect("create sg_b");

        storage.add_state_group_edge(sg_a, sg_b).await.expect("add_state_group_edge should succeed");

        let prev = storage.get_prev_state_groups(sg_a).await.expect("get_prev_state_groups should succeed");
        assert_eq!(prev, vec![sg_b]);

        cleanup_test_data(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_add_state_group_edges_batch() {
        let pool = test_pool().await;
        let storage = StateGroupStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!test_batch_edges_{suffix}:localhost");
        let ev_main = format!("$batch_ev_main_{suffix}");
        let ev_p1 = format!("$batch_ev_p1_{suffix}");
        let ev_p2 = format!("$batch_ev_p2_{suffix}");
        let ev_p3 = format!("$batch_ev_p3_{suffix}");

        cleanup_test_data(&pool, &room_id).await;
        ensure_test_room_and_event(&pool, &room_id, &ev_main).await;
        for ev in &[&ev_p1, &ev_p2, &ev_p3] {
            ensure_test_event(&pool, ev, &room_id).await;
        }

        let now = chrono::Utc::now().timestamp_millis();
        let sg_main = storage
            .create_state_group(&room_id, &ev_main, &format!("batch_main_{suffix}"), now)
            .await
            .expect("create sg_main");
        let sg_p1 = storage
            .create_state_group(&room_id, &ev_p1, &format!("batch_p1_{suffix}"), now)
            .await
            .expect("create sg_p1");
        let sg_p2 = storage
            .create_state_group(&room_id, &ev_p2, &format!("batch_p2_{suffix}"), now)
            .await
            .expect("create sg_p2");
        let sg_p3 = storage
            .create_state_group(&room_id, &ev_p3, &format!("batch_p3_{suffix}"), now)
            .await
            .expect("create sg_p3");

        let prev_ids = vec![sg_p1, sg_p2, sg_p3];
        storage.add_state_group_edges(sg_main, &prev_ids).await.expect("batch add should succeed");

        let mut prev = storage.get_prev_state_groups(sg_main).await.expect("get_prev should succeed");
        prev.sort();
        let mut expected = prev_ids.clone();
        expected.sort();
        assert_eq!(prev, expected);

        // Test empty batch (no-op)
        storage.add_state_group_edges(sg_main, &[]).await.expect("empty batch should succeed");

        cleanup_test_data(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_get_prev_state_groups() {
        let pool = test_pool().await;
        let storage = StateGroupStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!test_prev_{suffix}:localhost");
        let ev_cur = format!("$prev_ev_cur_{suffix}");
        let ev_old = format!("$prev_ev_old_{suffix}");

        cleanup_test_data(&pool, &room_id).await;
        ensure_test_room_and_event(&pool, &room_id, &ev_cur).await;
        ensure_test_event(&pool, &ev_old, &room_id).await;

        let now = chrono::Utc::now().timestamp_millis();
        let sg_cur = storage
            .create_state_group(&room_id, &ev_cur, &format!("prev_cur_{suffix}"), now)
            .await
            .expect("create sg_cur");
        let sg_old = storage
            .create_state_group(&room_id, &ev_old, &format!("prev_old_{suffix}"), now)
            .await
            .expect("create sg_old");

        storage.add_state_group_edge(sg_cur, sg_old).await.expect("add edge");

        let prev = storage.get_prev_state_groups(sg_cur).await.expect("get_prev should succeed");
        assert_eq!(prev, vec![sg_old]);

        // State group with no previous groups
        let no_prev = storage.get_prev_state_groups(sg_old).await.expect("query should succeed");
        assert!(no_prev.is_empty());

        cleanup_test_data(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_get_next_state_groups() {
        let pool = test_pool().await;
        let storage = StateGroupStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!test_next_{suffix}:localhost");
        let ev_a = format!("$next_ev_a_{suffix}");
        let ev_b = format!("$next_ev_b_{suffix}");

        cleanup_test_data(&pool, &room_id).await;
        ensure_test_room_and_event(&pool, &room_id, &ev_a).await;
        ensure_test_event(&pool, &ev_b, &room_id).await;

        let now = chrono::Utc::now().timestamp_millis();
        let sg_a =
            storage.create_state_group(&room_id, &ev_a, &format!("next_a_{suffix}"), now).await.expect("create sg_a");
        let sg_b =
            storage.create_state_group(&room_id, &ev_b, &format!("next_b_{suffix}"), now).await.expect("create sg_b");

        // sg_b -> sg_a (sg_a is prev of sg_b)
        storage.add_state_group_edge(sg_b, sg_a).await.expect("add edge");

        let next = storage.get_next_state_groups(sg_a).await.expect("get_next should succeed");
        assert_eq!(next, vec![sg_b]);

        // State group with no next groups
        let no_next = storage.get_next_state_groups(sg_b).await.expect("query should succeed");
        assert!(no_next.is_empty());

        cleanup_test_data(&pool, &room_id).await;
    }

    // ---- event_to_state_groups ---- //

    #[tokio::test]
    async fn test_bind_event_to_state_group() {
        let pool = test_pool().await;
        let storage = StateGroupStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!test_bind_{suffix}:localhost");
        let sg_ev = format!("$bind_sg_ev_{suffix}");
        let bind_ev = format!("$bind_ev_{suffix}");

        cleanup_test_data(&pool, &room_id).await;
        ensure_test_room_and_event(&pool, &room_id, &sg_ev).await;
        ensure_test_event(&pool, &bind_ev, &room_id).await;

        let now = chrono::Utc::now().timestamp_millis();
        let sg_id =
            storage.create_state_group(&room_id, &sg_ev, &format!("bind_hash_{suffix}"), now).await.expect("create sg");

        storage.bind_event_to_state_group(&bind_ev, sg_id).await.expect("bind should succeed");

        let found = storage
            .get_state_group_for_event(&bind_ev)
            .await
            .expect("lookup should succeed")
            .expect("should find state group for event");
        assert_eq!(found, sg_id);

        // Re-binding the same event should update (upsert)
        let sg_id2 = storage
            .create_state_group(&room_id, &bind_ev, &format!("bind_hash2_{suffix}"), now)
            .await
            .expect("create sg2");
        storage.bind_event_to_state_group(&bind_ev, sg_id2).await.expect("re-bind should succeed");
        let updated = storage
            .get_state_group_for_event(&bind_ev)
            .await
            .expect("lookup should succeed")
            .expect("should find updated mapping");
        assert_eq!(updated, sg_id2);

        cleanup_test_data(&pool, &room_id).await;
    }

    // ---- state_group_state ---- //

    #[tokio::test]
    async fn test_set_state_entry() {
        let pool = test_pool().await;
        let storage = StateGroupStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!test_state_entry_{suffix}:localhost");
        let sg_ev = format!("$state_entry_sg_ev_{suffix}");
        let state_ev = format!("$state_entry_ev_{suffix}");

        cleanup_test_data(&pool, &room_id).await;
        ensure_test_room_and_event(&pool, &room_id, &sg_ev).await;
        ensure_test_event(&pool, &state_ev, &room_id).await;

        let now = chrono::Utc::now().timestamp_millis();
        let sg_id =
            storage.create_state_group(&room_id, &sg_ev, &format!("se_hash_{suffix}"), now).await.expect("create sg");

        storage.set_state_entry(sg_id, "m.room.name", "", &state_ev).await.expect("set_state_entry should succeed");

        let event_id = storage
            .get_state_entry(sg_id, "m.room.name", "")
            .await
            .expect("get_state_entry query should succeed")
            .expect("should find state entry");
        assert_eq!(event_id, state_ev);

        // Test missing entry
        let missing = storage.get_state_entry(sg_id, "m.room.topic", "").await.expect("query should succeed");
        assert!(missing.is_none());

        // Test upsert (update existing)
        let state_ev2 = format!("$state_entry_ev2_{suffix}");
        ensure_test_event(&pool, &state_ev2, &room_id).await;
        storage.set_state_entry(sg_id, "m.room.name", "", &state_ev2).await.expect("upsert should succeed");
        let updated = storage
            .get_state_entry(sg_id, "m.room.name", "")
            .await
            .expect("query should succeed")
            .expect("should find updated entry");
        assert_eq!(updated, state_ev2);

        cleanup_test_data(&pool, &room_id).await;
    }

    #[tokio::test]
    async fn test_set_state_entries_batch() {
        let pool = test_pool().await;
        let storage = StateGroupStorage::new(&pool);
        let suffix = uuid::Uuid::new_v4();
        let room_id = format!("!test_batch_state_{suffix}:localhost");
        let sg_ev = format!("$batch_state_sg_ev_{suffix}");
        let ev1 = format!("$batch_state_ev1_{suffix}");
        let ev2 = format!("$batch_state_ev2_{suffix}");
        let ev3 = format!("$batch_state_ev3_{suffix}");

        cleanup_test_data(&pool, &room_id).await;
        ensure_test_room_and_event(&pool, &room_id, &sg_ev).await;
        for ev in &[&ev1, &ev2, &ev3] {
            ensure_test_event(&pool, ev, &room_id).await;
        }

        let now = chrono::Utc::now().timestamp_millis();
        let sg_id =
            storage.create_state_group(&room_id, &sg_ev, &format!("bs_hash_{suffix}"), now).await.expect("create sg");

        let entries = vec![
            StateGroupStateEntry {
                event_type: "m.room.name".to_string(),
                state_key: "".to_string(),
                event_id: ev1.clone(),
            },
            StateGroupStateEntry {
                event_type: "m.room.topic".to_string(),
                state_key: "".to_string(),
                event_id: ev2.clone(),
            },
            StateGroupStateEntry {
                event_type: "m.room.member".to_string(),
                state_key: "@user:localhost".to_string(),
                event_id: ev3.clone(),
            },
        ];

        storage.set_state_entries(sg_id, &entries).await.expect("batch set_state_entries should succeed");

        let all_state = storage.get_state_at_group(sg_id).await.expect("get_state_at_group should succeed");
        assert_eq!(all_state.len(), 3);

        let found_ev1 = storage.get_state_entry(sg_id, "m.room.name", "").await.expect("query").expect("should find");
        assert_eq!(found_ev1, ev1);

        let found_ev2 = storage.get_state_entry(sg_id, "m.room.topic", "").await.expect("query").expect("should find");
        assert_eq!(found_ev2, ev2);

        let found_ev3 = storage
            .get_state_entry(sg_id, "m.room.member", "@user:localhost")
            .await
            .expect("query")
            .expect("should find");
        assert_eq!(found_ev3, ev3);

        // Test empty batch (no-op)
        storage.set_state_entries(sg_id, &[]).await.expect("empty batch should succeed");

        cleanup_test_data(&pool, &room_id).await;
    }
}
