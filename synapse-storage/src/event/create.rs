//! Event creation methods for [`EventStorage`].

use super::models::{CreateEventParams, PduGraphFields, RoomEvent};
use super::EventStorage;

impl EventStorage {
    /// See [`create_event`].
    pub async fn create_event(
        &self,
        params: CreateEventParams,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        // 事务路径与连接池路径共用**一个**连接来源：宏的绑定实参属于调用点，
        // 所以 SQL 必须写在调用处（此前用 `let query = r"…"` 把静态 SQL 藏进变量，
        // 既抬高棘轮又绕过 literal 门禁 —— §7 D-59 / R1）。
        let mut owned;
        let conn: &mut sqlx::PgConnection = match tx {
            Some(tx) => &mut *tx,
            None => {
                owned = self.pool.acquire().await?;
                &mut owned
            }
        };

        sqlx::query_as!(
            RoomEvent,
            r#"
            INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, state_key, origin_server_ts, is_redacted, redacts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false, $9)
            RETURNING event_id, room_id, sender as user_id, event_type, content, state_key,
                      COALESCE(depth, 0) as "depth!", origin_server_ts as "processed_ts",
                      origin_server_ts, 0::BIGINT as "not_before!", 'pending' as "status?",
                      'self' as "origin!", stream_ordering, redacts
            "#,
            &params.event_id,
            &params.room_id,
            &params.user_id,
            &params.user_id,
            &params.event_type,
            &params.content,
            params.state_key.as_deref(),
            params.origin_server_ts,
            params.redacts.as_deref(),
        )
        .fetch_one(&mut *conn)
        .await
    }

    /// Single write path for the **13-column graph INSERT** shared by
    /// [`Self::create_event_with_pdu`] and [`Self::create_outlier_event`].
    ///
    /// The two callers differ only in what happens *after* the row lands:
    /// `create_event_with_pdu` appends the `event_edges` rows, while
    /// `create_outlier_event` must not (its parents are FK-unresolvable).  The
    /// INSERT text itself used to be duplicated verbatim in both bodies — the
    /// outlier copy even carried a "keep this byte-identical so the `.sqlx`
    /// cache entry is reused" comment.  That is a drift trap: edit one and not
    /// the other and the two paths silently diverge.  One literal, one owner.
    ///
    /// ⚠️ The SQL literal stays **inside** the `sqlx::query_as!` call below —
    /// never hoisted into a `let sql = r"…"` variable (§7 D-59 / R1): the macro
    /// requires a literal, and the dynamic-literal guard forbids non-macro
    /// `sqlx::query(..)` in production.
    async fn insert_event_row_with_graph(
        conn: &mut sqlx::PgConnection,
        params: &CreateEventParams,
        depth: Option<i64>,
        prev_events_json: &serde_json::Value,
        auth_events_json: &serde_json::Value,
    ) -> Result<RoomEvent, sqlx::Error> {
        sqlx::query_as!(
            RoomEvent,
            r#"
            INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, state_key, origin_server_ts, is_redacted, redacts, depth, prev_events, auth_events)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false, $9, $10, $11, $12)
            RETURNING event_id, room_id, sender as user_id, event_type, content, state_key,
                      COALESCE(depth, 0) as "depth!", origin_server_ts as "processed_ts",
                      origin_server_ts, 0::BIGINT as "not_before!", 'pending' as "status?",
                      'self' as "origin!", stream_ordering, redacts
            "#,
            &params.event_id,
            &params.room_id,
            &params.user_id,
            &params.user_id,
            &params.event_type,
            &params.content,
            params.state_key.as_deref(),
            params.origin_server_ts,
            params.redacts.as_deref(),
            depth,
            prev_events_json,
            auth_events_json,
        )
        .fetch_one(&mut *conn)
        .await
    }

    /// Create an event with complete PDU graph fields: `depth` / `prev_events` /
    /// `auth_events` in `events` **and** one `event_edges` row per parent.
    ///
    /// This is the single graph write path — [`Self::create_event_with_graph`]
    /// is the concrete-slice adapter that delegates here, so the two cannot
    /// drift.  Writing the graph columns without the edges (this method's
    /// former behaviour) left `event_edges` empty for every locally-created
    /// event, and `get_forward_extremities_in_room` derives the room's tips
    /// **solely** from that table: each local event then looked like a forward
    /// extremity forever, corrupting `prev_events` growth and
    /// `/get_missing_events`.
    ///
    /// Unlike [`create_event`] which writes SQL `NULL` for graph columns, this
    /// method persists whatever the caller supplied.  Callers must ensure the
    /// graph fields are compliant with the target room version (v12 requires
    /// ED25519-only auth rules, etc.).
    ///
    /// ⚠️ 本方法**不**计算 `depth`/`prev_events`/`auth_events` — it is the
    /// caller's responsibility to populate `PduGraphFields` before calling.
    ///
    /// P2-1 Optimization (2026-09-23):
    /// - Combined two-step insert in single transaction (event row + edges)
    /// - Uses unnest() to batch edge inserts in one round-trip
    ///
    /// ⚠️ 守卫必须写 `cardinality($2) > 0`，**不能**写 `$2 != '[]'`：`$2` 已被
    /// `unnest($2::text[])` 定为 `text[]`，PG 会把 `'[]'` 当**数组字面量**解析并在
    /// **prepare 阶段**就报 `22P02 malformed array literal: "[]"` —— 语句永远执行不了
    /// （`8489b4079` 引入，2026-09-25 由 `test_create_event_with_graph_with_prev_events`
    /// 抓出；`cardinality` 对 NULL 同样返回 NULL ⇒ 语义与原意一致）。
    pub async fn create_event_with_pdu(
        &self,
        params: CreateEventParams,
        pdu_graph: PduGraphFields,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        let prev_events_json = serde_json::to_value(&pdu_graph.prev_events).unwrap_or(serde_json::Value::Null);
        let auth_events_json = serde_json::to_value(&pdu_graph.auth_events).unwrap_or(serde_json::Value::Null);

        // 连接来源收敛成一个：宏的绑定实参属于调用点，SQL 必须写在调用处
        // （此前把静态 SQL 藏进 `let … = r"…"` 变量 —— §7 D-59 / R1）。
        let mut owned_tx: Option<sqlx::Transaction<'_, sqlx::Postgres>> = None;
        let conn: &mut sqlx::PgConnection = match tx {
            Some(tx) => &mut *tx,
            None => {
                // 无调用方事务：事件行与其 DAG 边必须在**同一本地事务**里落库，
                // 否则 `event_edges` 插入失败会留下孤立 `events` 行（B8）。这条
                // 不变式与 `create_event_with_graph` 关掉的是同一个半写窗口。
                let begun = self.pool.begin().await?;
                &mut *owned_tx.insert(begun)
            }
        };

        // P2-1: Insert event row first, then batch edge inserts in same txn
        let event =
            Self::insert_event_row_with_graph(conn, &params, pdu_graph.depth, &prev_events_json, &auth_events_json)
                .await?;

        // P2-1: Batch insert all prev_edges in a single round-trip using unnest().
        // `None` is the "no graph metadata" shape (`create_event` territory):
        // there is no parent list to record, so no edges are written for it.
        let prev_events: &[String] = pdu_graph.prev_events.as_deref().unwrap_or(&[]);
        if !prev_events.is_empty() {
            sqlx::query!(
                r#"
                INSERT INTO event_edges (event_id, prev_event_id, is_state)
                SELECT $1, unnest($2::text[]), false
                WHERE cardinality($2) > 0
                ON CONFLICT DO NOTHING
                "#,
                &params.event_id,
                prev_events,
            )
            .execute(&mut *conn)
            .await?;
        }

        if let Some(tx) = owned_tx {
            tx.commit().await?;
        }

        Ok(event)
    }

    /// Concrete-slice adapter over [`Self::create_event_with_pdu`].
    ///
    /// Callers that already hold the PDU's graph fields (the inbound federation
    /// transaction handler, backfill, room creation) hand over plain slices;
    /// this wraps them losslessly (`Some(..)`, never `None`, so the persisted
    /// `[]` / `0` shape is unchanged) and delegates, keeping one implementation
    /// of "event row + `event_edges` in one local transaction".
    ///
    /// ⚠️ 本方法**不**是 `create_event` 的后端：`create_event` 有自己的 INSERT
    /// （见文件顶部），两者对图列的处理**不同** —— `create_event` 写 SQL `NULL`，
    /// 本方法写 `[]` / `0`。该差异可被下游观测到：`synapse-web/.../federation/pdu.rs`
    /// 的 `event_id_array` 把 `NULL` 判为"图元数据缺失"，而 `[]` 会被判为 Complete
    /// 并据此签名。**不要把 `create_event` 改成委托到 `create_event_with_pdu`
    /// 并传空数组** —— 那等于给本地事件伪造 DAG 根。
    pub async fn create_event_with_graph(
        &self,
        params: CreateEventParams,
        prev_events: &[String],
        auth_events: &[String],
        depth: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        self.create_event_with_pdu(
            params,
            PduGraphFields {
                depth: Some(depth),
                prev_events: Some(prev_events.to_vec()),
                auth_events: Some(auth_events.to_vec()),
            },
            tx,
        )
        .await
    }

    /// Persist an inbound event whose `prev_events` point at parents this
    /// server does not hold — Synapse's "outlier" shape.
    ///
    /// The `events` row keeps the origin's graph columns verbatim
    /// (`depth` / `prev_events` / `auth_events`) so the PDU still projects as
    /// [`PduCompleteness::Complete`] and can be signed, but **no `event_edges`
    /// rows are written**: `event_edges.prev_event_id` has an FK to
    /// `events(event_id)` (`fk_event_edges_prev`), which a parent we never
    /// received cannot satisfy.
    ///
    /// ⚠️ 这**不是**对 [`Self::create_event_with_pdu`] 的放松：三条存储不变式
    /// （`…_rolls_back_event_when_edges_insert_fails`）断言"父事件缺失 ⇒ 写入失败并
    /// 回滚"，那条路径必须保持原样。outlier 是**另一种写入形状**，只由明确知道自己
    /// 拿不到父事件的调用方选择（联邦入站邀请，且本机不托管该房间）。
    ///
    /// ⚠️ outlier 会被 [`Self::get_forward_extremities_in_room`] 当成 forward
    /// extremity —— 该函数**只**从 `event_edges` 推导叶节点（取舍已在其文档注释中记录）。
    ///
    /// ⚠️ 本方法与 [`Self::create_event_with_pdu`] 共用**同一份** INSERT 字面量
    /// （见 [`Self::insert_event_row_with_graph`]）；差异只在于本方法**不执行**后续的
    /// `event_edges` 插入。共享一份字面量后，不再有"两处必须逐字节一致"的漂移风险，
    /// 二者也自然复用同一个 `.sqlx` 离线条目（缓存以 `sha256(SQL)` 为键）。
    pub async fn create_outlier_event(
        &self,
        params: CreateEventParams,
        prev_events: &[String],
        auth_events: &[String],
        depth: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        let prev_events_json = serde_json::to_value(prev_events).unwrap_or(serde_json::Value::Null);
        let auth_events_json = serde_json::to_value(auth_events).unwrap_or(serde_json::Value::Null);

        // 单条 INSERT 自带原子性，不需要 `create_event_with_pdu` 那层自有事务。
        let mut owned;
        let conn: &mut sqlx::PgConnection = match tx {
            Some(tx) => &mut *tx,
            None => {
                owned = self.pool.acquire().await?;
                &mut owned
            }
        };

        Self::insert_event_row_with_graph(conn, &params, Some(depth), &prev_events_json, &auth_events_json).await
    }

    /// Create a state event with MSC4242 `prev_state_events` (state DAG edges).
    ///
    /// This is the MSC4242 State DAG equivalent of `create_event_with_graph`:
    /// it stores `prev_state_events` in addition to `prev_events` and
    /// `auth_events`, forming the state DAG distinct from the room DAG.
    ///
    /// - `prev_events`: room DAG edges (all events)
    /// - `auth_events`: authorization events (sender-specified for v1-v11;
    ///   ignored / server-calculated for MSC4242 room versions)
    /// - `prev_state_events`: state DAG edges (state events only, MSC4242)
    ///
    /// For non-MSC4242 room versions, use `create_event_with_graph` instead
    /// (which leaves `prev_state_events` NULL).
    ///
    /// P2-1 Optimization (2026-09-23):
    /// - Batch inserts room edges and state edges in separate unnest() calls
    ///
    /// ⚠️ 同上：两组边的守卫必须是 `cardinality($2) > 0`，`$2 != '[]'` 会让语句在
    /// prepare 阶段报 `22P02`（详见 `create_event_with_graph` 的注释）。
    pub async fn create_state_event_with_dag(
        &self,
        params: CreateEventParams,
        prev_events: &[String],
        auth_events: &[String],
        prev_state_events: &[String],
        depth: i64,
        tx: Option<&mut sqlx::Transaction<'_, sqlx::Postgres>>,
    ) -> Result<RoomEvent, sqlx::Error> {
        let prev_events_json = serde_json::to_value(prev_events).unwrap_or(serde_json::Value::Null);
        let auth_events_json = serde_json::to_value(auth_events).unwrap_or(serde_json::Value::Null);
        let prev_state_events_json = serde_json::to_value(prev_state_events).unwrap_or(serde_json::Value::Null);

        // 连接来源收敛成一个（同 `create_event_with_graph`）：宏的绑定实参属于调用点。
        let mut owned_tx: Option<sqlx::Transaction<'_, sqlx::Postgres>> = None;
        let conn: &mut sqlx::PgConnection = match tx {
            Some(tx) => &mut *tx,
            None => {
                // 无调用方事务：事件行与**两组** DAG 边必须在**同一本地事务**里落库，
                // 否则 `event_edges` 插入失败会留下孤立 `events` 行（`/get_missing_events`
                // 永远走不到它）。这与 B8 给 `create_event_with_graph` 关掉的是同一个半写窗口，
                // 此前因为两处插入逻辑重复而漏掉了这一条路径（2026-09-23 实测；
                // 红证明 `create_state_event_with_dag_rolls_back_event_when_edges_insert_fails`）。
                let begun = self.pool.begin().await?;
                &mut *owned_tx.insert(begun)
            }
        };

        let event = sqlx::query_as!(
            RoomEvent,
            r#"
            INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, state_key,
                                origin_server_ts, is_redacted, redacts, depth,
                                prev_events, auth_events, prev_state_events)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, false, $9, $10, $11, $12, $13)
            RETURNING event_id, room_id, sender as user_id, event_type, content, state_key,
                      COALESCE(depth, 0) as "depth!", origin_server_ts as "processed_ts",
                      origin_server_ts, 0::BIGINT as "not_before!", 'pending' as "status?",
                      'self' as "origin!", stream_ordering, redacts
            "#,
            &params.event_id,
            &params.room_id,
            &params.user_id,
            &params.user_id,
            &params.event_type,
            &params.content,
            params.state_key.as_deref(),
            params.origin_server_ts,
            params.redacts.as_deref(),
            depth,
            &prev_events_json,
            &auth_events_json,
            &prev_state_events_json,
        )
        .fetch_one(&mut *conn)
        .await?;

        // P2-1: Batch room DAG edges
        if !prev_events.is_empty() {
            sqlx::query!(
                r#"
                INSERT INTO event_edges (event_id, prev_event_id, is_state)
                SELECT $1, unnest($2::text[]), false
                WHERE cardinality($2) > 0
                ON CONFLICT DO NOTHING
                "#,
                &params.event_id,
                prev_events,
            )
            .execute(&mut *conn)
            .await?;
        }
        // P2-1: Batch state DAG edges
        if !prev_state_events.is_empty() {
            sqlx::query!(
                r#"
                INSERT INTO event_edges (event_id, prev_event_id, is_state)
                SELECT $1, unnest($2::text[]), true
                WHERE cardinality($2) > 0
                ON CONFLICT DO NOTHING
                "#,
                &params.event_id,
                prev_state_events,
            )
            .execute(&mut *conn)
            .await?;
        }

        if let Some(tx) = owned_tx {
            tx.commit().await?;
        }

        Ok(event)
    }

    /// See [`upsert_power_levels_event`].
    pub async fn upsert_power_levels_event(
        &self,
        event_id: &str,
        room_id: &str,
        user_id: &str,
        content: serde_json::Value,
        origin_server_ts: i64,
        sender: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO events (event_id, room_id, user_id, event_type, content, state_key, origin_server_ts, sender, unsigned)
            VALUES ($1, $2, $3, 'm.room.power_levels', $4, '', $5, $6, '{}'::jsonb)
            ON CONFLICT (event_id) DO UPDATE SET content = $4
            "#,
            event_id,
            room_id,
            user_id,
            content,
            origin_server_ts,
            sender,
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`get_room_create_event`].
    pub async fn get_room_create_event(&self, room_id: &str) -> Result<Option<RoomEvent>, sqlx::Error> {
        sqlx::query_as!(
            RoomEvent,
            r#"
            SELECT event_id, room_id, COALESCE(user_id, sender) as "user_id!", event_type, content, state_key,
                   COALESCE(depth, 0) as "depth!", COALESCE(origin_server_ts, 0) as "origin_server_ts!",
                   COALESCE(origin_server_ts, 0) as "processed_ts!",
                   COALESCE(not_before, 0) as "not_before!", status, COALESCE(origin, 'self') as "origin!",
                   stream_ordering, redacts
            FROM events
            WHERE room_id = $1 AND event_type = 'm.room.create'
            ORDER BY origin_server_ts ASC
            LIMIT 1
            "#,
            room_id,
        )
        .fetch_optional(&*self.pool)
        .await
    }
}
