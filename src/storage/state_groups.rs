use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tracing;

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
        let row = sqlx::query_scalar!(
            r#"
            INSERT INTO state_groups (room_id, event_id, state_hash, created_ts)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (state_hash) DO UPDATE SET event_id = $2
            RETURNING id AS "id!"
            "#,
            room_id,
            event_id,
            state_hash,
            created_ts
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(row)
    }

    pub async fn get_state_group(&self, id: i64) -> Result<Option<StateGroup>, sqlx::Error> {
        sqlx::query_as!(
            StateGroup,
            r#"SELECT id AS "id!", room_id AS "room_id!", event_id AS "event_id!",
                      state_hash AS "state_hash!", created_ts AS "created_ts!"
               FROM state_groups WHERE id = $1"#,
            id
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn get_state_group_by_event(&self, event_id: &str) -> Result<Option<StateGroup>, sqlx::Error> {
        sqlx::query_as!(
            StateGroup,
            r#"SELECT id AS "id!", room_id AS "room_id!", event_id AS "event_id!",
                      state_hash AS "state_hash!", created_ts AS "created_ts!"
               FROM state_groups WHERE event_id = $1"#,
            event_id
        )
        .fetch_optional(&self.pool)
        .await
    }

    pub async fn get_room_state_groups(&self, room_id: &str, limit: i64) -> Result<Vec<StateGroup>, sqlx::Error> {
        sqlx::query_as!(
            StateGroup,
            r#"SELECT id AS "id!", room_id AS "room_id!", event_id AS "event_id!",
                      state_hash AS "state_hash!", created_ts AS "created_ts!"
               FROM state_groups WHERE room_id = $1 ORDER BY id DESC LIMIT $2"#,
            room_id,
            limit
        )
        .fetch_all(&self.pool)
        .await
    }

    // ---- state_group_edges ---- //

    /// 添加 state_group 边关系
    pub async fn add_state_group_edge(&self, state_group_id: i64, prev_state_group_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO state_group_edges (state_group_id, prev_state_group_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
            state_group_id,
            prev_state_group_id
        )
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
        sqlx::query!(
            r#"
            INSERT INTO state_group_edges (state_group_id, prev_state_group_id)
            SELECT $1, unnest($2::bigint[])
            ON CONFLICT DO NOTHING
            "#,
            state_group_id,
            prev_state_group_ids
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_prev_state_groups(&self, state_group_id: i64) -> Result<Vec<i64>, sqlx::Error> {
        let rows = sqlx::query_scalar!(
            r#"SELECT prev_state_group_id AS "prev_state_group_id!" FROM state_group_edges WHERE state_group_id = $1"#,
            state_group_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    pub async fn get_next_state_groups(&self, prev_state_group_id: i64) -> Result<Vec<i64>, sqlx::Error> {
        let rows = sqlx::query_scalar!(
            r#"SELECT state_group_id AS "state_group_id!" FROM state_group_edges WHERE prev_state_group_id = $1"#,
            prev_state_group_id
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows)
    }

    // ---- event_to_state_groups ---- //

    /// 绑定事件到 state_group
    pub async fn bind_event_to_state_group(&self, event_id: &str, state_group_id: i64) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO event_to_state_groups (event_id, state_group_id)
            VALUES ($1, $2)
            ON CONFLICT (event_id) DO UPDATE SET state_group_id = $2
            "#,
            event_id,
            state_group_id
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_state_group_for_event(&self, event_id: &str) -> Result<Option<i64>, sqlx::Error> {
        let row = sqlx::query_scalar!(
            r#"SELECT state_group_id AS "state_group_id!" FROM event_to_state_groups WHERE event_id = $1"#,
            event_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
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
        sqlx::query!(
            r#"
            INSERT INTO event_to_state_groups (event_id, state_group_id)
            SELECT unnest($1::text[]), $2
            ON CONFLICT (event_id) DO UPDATE SET state_group_id = $2
            "#,
            event_ids,
            state_group_id
        )
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
        sqlx::query!(
            r#"
            INSERT INTO state_group_state (state_group_id, event_type, state_key, event_id)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (state_group_id, event_type, state_key) DO UPDATE SET event_id = $4
            "#,
            state_group_id,
            event_type,
            state_key,
            event_id
        )
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

        let event_types: Vec<String> = entries.iter().map(|e| e.event_type.clone()).collect();
        let state_keys: Vec<String> = entries.iter().map(|e| e.state_key.clone()).collect();
        let event_ids: Vec<String> = entries.iter().map(|e| e.event_id.clone()).collect();

        sqlx::query!(
            r#"
            INSERT INTO state_group_state (state_group_id, event_type, state_key, event_id)
            SELECT $1, unnest($2::text[]), unnest($3::text[]), unnest($4::text[])
            ON CONFLICT (state_group_id, event_type, state_key) DO UPDATE SET event_id = EXCLUDED.event_id
            "#,
            state_group_id,
            &event_types,
            &state_keys,
            &event_ids
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get_state_at_group(&self, state_group_id: i64) -> Result<Vec<StateGroupState>, sqlx::Error> {
        sqlx::query_as!(
            StateGroupState,
            r#"SELECT state_group_id AS "state_group_id!", event_type AS "event_type!",
                      state_key AS "state_key!", event_id AS "event_id!"
               FROM state_group_state WHERE state_group_id = $1"#,
            state_group_id
        )
        .fetch_all(&self.pool)
        .await
    }

    pub async fn get_state_entry(
        &self,
        state_group_id: i64,
        event_type: &str,
        state_key: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        let row = sqlx::query_scalar!(
            r#"SELECT event_id AS "event_id!" FROM state_group_state
               WHERE state_group_id = $1 AND event_type = $2 AND state_key = $3"#,
            state_group_id,
            event_type,
            state_key
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row)
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
            let state_rows = sqlx::query_as!(
                StateGroupState,
                r#"SELECT state_group_id AS "state_group_id!", event_type AS "event_type!",
                          state_key AS "state_key!", event_id AS "event_id!"
                   FROM state_group_state WHERE state_group_id = $1"#,
                sg_id
            )
            .fetch_all(&self.pool)
            .await?;

            for row in state_rows {
                let key = (row.event_type, row.state_key);
                result.entry(key).or_insert(row.event_id);
            }

            // Load prev groups
            let prev_rows = sqlx::query_scalar!(
                r#"SELECT prev_state_group_id AS "prev_state_group_id!" FROM state_group_edges WHERE state_group_id = $1"#,
                sg_id
            )
            .fetch_all(&self.pool)
            .await?;

            for prev_id in prev_rows {
                if !visited.contains(&prev_id) {
                    queue.push_back(prev_id);
                }
            }
        }

        Ok(result)
    }
}
