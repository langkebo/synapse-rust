use async_trait::async_trait;
use sqlx::{Pool, Postgres};
use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

// ── Trait ───────────────────────────────────────────────────────────────

/// Focused trait for the device-list methods used by
/// [`AccountDeviceListService`](synapse_services::account_device_list_service::AccountDeviceListService).
#[async_trait]
pub trait DeviceListStoreApi: Send + Sync {
    /// See [`DeviceStorage::record_device_list_change`].
    ///
    /// **刻意不接收 `stream_id`**：调用方给不出一个本地可用的序号。联邦 EDU 带来的是
    /// **对端的** `stream_id`（毫秒时间戳，量级 1e12），而本表的所有读取都用本地序列
    /// （量级 1e3）做 `stream_id <= to` 的范围过滤 —— 把对端的值存进来，这一行就落在
    /// 每个读取之外（审计 NEW-07）。序号由存储层分配。
    async fn record_device_list_change(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        change_type: &str,
    ) -> Result<i64, sqlx::Error>;

    /// See [`create_device`].
    async fn create_device(
        &self,
        device_id: &str,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<Device, sqlx::Error>;

    /// See [`delete_device`].
    async fn delete_device(&self, device_id: &str) -> Result<(), sqlx::Error>;

    /// See [`get_user_devices`].
    async fn get_user_devices(&self, user_id: &str) -> Result<Vec<Device>, sqlx::Error>;

    /// See [`get_device`].
    async fn get_device(&self, device_id: &str) -> Result<Option<Device>, sqlx::Error>;

    /// See [`update_user_device_display_name`].
    async fn update_user_device_display_name(
        &self,
        user_id: &str,
        device_id: &str,
        display_name: &str,
    ) -> Result<u64, sqlx::Error>;

    /// See [`get_max_device_list_stream_id`].
    async fn get_max_device_list_stream_id(&self) -> Result<i64, sqlx::Error>;

    /// See [`get_max_device_list_stream_id_for_user`].
    async fn get_max_device_list_stream_id_for_user(&self, user_id: &str) -> Result<i64, sqlx::Error>;

    /// See [`get_device_list_changed_users`].
    async fn get_device_list_changed_users(
        &self,
        from: i64,
        to: i64,
        requester_id: &str,
    ) -> Result<Vec<String>, sqlx::Error>;

    /// See [`get_device_list_left_users`].
    async fn get_device_list_left_users(
        &self,
        from: i64,
        to: i64,
        requester_id: &str,
    ) -> Result<Vec<String>, sqlx::Error>;

    /// See [`get_users_devices_batch`].
    async fn get_users_devices_batch(
        &self,
        users: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<Device>>, sqlx::Error>;

    /// See [`get_device_list_changes`].
    async fn get_device_list_changes(
        &self,
        since: i64,
        to: i64,
        users: &[String],
    ) -> Result<Vec<(String, Option<String>, String, i64)>, sqlx::Error>;

    /// See [`get_devices_by_user_device_pairs`].
    async fn get_devices_by_user_device_pairs(
        &self,
        user_ids: &[&str],
        device_ids: &[&str],
    ) -> Result<Vec<(String, String, Option<String>, Option<i64>)>, sqlx::Error>;

    /// See [`filter_existing_users`].
    async fn filter_existing_users(&self, users: &[String]) -> Result<Vec<String>, sqlx::Error>;

    // ── incremental device-list polling ──────────────────────────────────

    /// See [`has_device_list_updates_since`].
    async fn has_device_list_updates_since(&self, since_stream_id: i64) -> Result<bool, sqlx::Error>;

    /// See [`get_device_lists_since_with_shared_rooms`].
    async fn get_device_lists_since_with_shared_rooms(
        &self,
        since_stream_id: i64,
        exclude_user_id: &str,
    ) -> Result<(Vec<String>, Vec<String>), sqlx::Error>;

    // ── lazy-loaded members ──────────────────────────────────────────────

    /// See [`get_lazy_loaded_members`].
    async fn get_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
    ) -> Result<Vec<LazyLoadedMemberRow>, sqlx::Error>;

    /// See [`upsert_lazy_loaded_members`].
    async fn upsert_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        members: &[LazyLoadedMemberRow],
    ) -> Result<u64, sqlx::Error>;

    // ── device management (widened set) ────────────────────────────────────

    /// See [`delete_user_devices_batch`].
    async fn delete_user_devices_batch(&self, user_id: &str, device_ids: &[String]) -> Result<u64, sqlx::Error>;

    /// See [`get_device_by_id`].
    async fn get_device_by_id(&self, device_id: &str) -> Result<Option<Device>, sqlx::Error>;

    /// See [`delete_device_returning_count`].
    async fn delete_device_returning_count(&self, user_id: &str, device_id: &str) -> Result<u64, sqlx::Error>;

    /// See [`delete_all_devices`].
    async fn delete_all_devices(&self, user_id: &str) -> Result<(), sqlx::Error>;

    /// See [`get_device_count`].
    async fn get_device_count(&self, user_id: &str) -> Result<i64, sqlx::Error>;
}

/// The `Device` struct.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Device {
    /// The `device_id` field.
    pub device_id: String,
    /// The `user_id` field.
    pub user_id: String,
    /// The `display_name` field.
    pub display_name: Option<String>,
    /// The `device_key` field.
    pub device_key: Option<serde_json::Value>,
    /// The `last_seen_ts` field.
    pub last_seen_ts: Option<i64>,
    /// The `last_seen_ip` field.
    pub last_seen_ip: Option<String>,
    /// The `created_ts` field.
    pub created_ts: i64,
    /// The `first_seen_ts` field.
    pub first_seen_ts: i64,
    /// The `user_agent` field.
    pub user_agent: Option<String>,
    /// The `appservice_id` field.
    pub appservice_id: Option<String>,
    /// The `ignored_user_list` field.
    pub ignored_user_list: Option<String>,
}

/// One persisted lazy-load entry: a member we have already sent to a
/// lazy-loading device, plus the position of the `/sync` response that carried
/// them.
///
/// The position is what makes the persisted suppression sound (upstream #20289
/// has the same field in memory): a client only comes back with a token at or
/// past the position of a response it actually *processed*, so an entry whose
/// position is newer than the request's `since` describes a response the client
/// never received and must not suppress that member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LazyLoadedMemberRow {
    /// `member_user_id`.
    pub member_user_id: String,
    /// `last_sent_position`: the `next_batch` position of the response that
    /// carried this membership. `None` means "not recorded" (a row written
    /// before this column existed) and is treated as "possibly not delivered",
    /// so it never suppresses. `Some(0)` is a *real* position: a quiet room can
    /// legitimately have `next_batch == 0`, which is why the column is nullable
    /// instead of defaulting to 0.
    pub last_sent_position: Option<i64>,
}

/// The `DeviceStorage` struct.
#[derive(Clone)]
pub struct DeviceStorage {
    /// The `pool` field.
    pub pool: Arc<Pool<Postgres>>,
}

impl DeviceStorage {
    /// See [`new`].
    pub fn new(pool: &Arc<Pool<Postgres>>) -> Self {
        Self { pool: pool.clone() }
    }

    /// Record one device-list change (`device_lists_stream` + `device_lists_changes`).
    ///
    /// **这是全仓唯一的实现**（D-37 / 铁律 2）：`synapse-e2ee` 的
    /// `DeviceKeyStoreApi::record_device_list_change` 直接委托到这里，
    /// 不再自带一份逐字相同的 SQL。返回新写入的 `stream_id`。
    pub async fn record_device_list_change(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        change_type: &str,
    ) -> Result<i64, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let stream_id = Self::record_device_list_change_tx(&mut tx, user_id, device_id, change_type).await?;
        tx.commit().await?;
        Ok(stream_id)
    }

    /// [`Self::record_device_list_change`] 的**事务内**实现。
    ///
    /// 游标行（`device_lists_stream`）与变更行（`device_lists_changes`）必须同生共死：
    /// 分两条语句各自 autocommit 时，进程在中间退出会留下"游标前进了但没有变更记录"
    /// （或反过来），`/sync` 的 device_lists 增量就永久错位 —— 而两者都只写不删。
    async fn record_device_list_change_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: &str,
        device_id: Option<&str>,
        change_type: &str,
    ) -> Result<i64, sqlx::Error> {
        let now = current_timestamp_millis();
        let stream_id: i64 = sqlx::query_scalar!(
            r"
            INSERT INTO device_lists_stream (user_id, device_id, created_ts)
            VALUES ($1, $2, $3)
            RETURNING stream_id
            ",
            user_id,
            device_id,
            now
        )
        .fetch_one(&mut **tx)
        .await?;

        sqlx::query!(
            r#"
            INSERT INTO device_lists_changes (user_id, device_id, change_type, stream_id, created_ts)
            VALUES ($1, $2, $3, $4, $5)
            "#,
            user_id,
            device_id,
            change_type,
            stream_id,
            now
        )
        .execute(&mut **tx)
        .await?;

        Ok(stream_id)
    }

    /// 批量记录 device-list 变更（`device_lists_stream` + `device_lists_changes`），**事务内**。
    ///
    /// 与 [`Self::record_device_list_change_tx`] 同一理由：游标行与变更行同生共死，
    /// 否则 `/sync` 的 device_lists 增量会永久错位。
    async fn record_device_list_changes_batch_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: &str,
        device_ids: &[String],
        change_type: &str,
    ) -> Result<(), sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(());
        }
        let now = current_timestamp_millis();
        sqlx::query!(
            r#"
            WITH inserted AS (
                INSERT INTO device_lists_stream (user_id, device_id, created_ts)
                SELECT $1, device_id, $2 FROM UNNEST($3::TEXT[]) AS device_id
                RETURNING stream_id, device_id
            )
            INSERT INTO device_lists_changes (user_id, device_id, change_type, stream_id, created_ts)
            SELECT $1, device_id, $4, stream_id, $2 FROM inserted
            "#,
            user_id,
            now,
            device_ids,
            change_type
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// See [`get_lazy_loaded_members`].
    ///
    /// Returns the stored `(member, position)` pairs **unfiltered**: comparing a
    /// position against the caller's `since` is a decision for the sync layer
    /// (`LazyLoadedMemberState`), which is the only place that knows whether the
    /// request was incremental at all.
    pub async fn get_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
    ) -> Result<Vec<LazyLoadedMemberRow>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT member_user_id, last_sent_position
            FROM lazy_loaded_members
            WHERE user_id = $1 AND device_id = $2 AND room_id = $3
            "#,
            user_id,
            device_id,
            room_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| LazyLoadedMemberRow {
                member_user_id: row.member_user_id,
                last_sent_position: row.last_sent_position,
            })
            .collect())
    }

    /// See [`upsert_lazy_loaded_members`].
    ///
    /// Each member carries **its own** position: members recorded in different
    /// responses must keep the position of the response that actually carried
    /// them, otherwise pruning by `since` would either drop a member the client
    /// did receive or keep one it did not.
    ///
    /// A member without a position ([`LazyLoadedMemberRow::last_sent_position`] is
    /// `None`) is **skipped**: a row that cannot say when it was sent can never
    /// suppress anything, so storing it would only mislead the next reader. This
    /// also keeps the parameter a plain `BIGINT[]` — the element type sqlx's
    /// offline metadata pins.
    pub async fn upsert_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        members: &[LazyLoadedMemberRow],
    ) -> Result<u64, sqlx::Error> {
        let members: Vec<&LazyLoadedMemberRow> =
            members.iter().filter(|member| member.last_sent_position.is_some()).collect();
        if members.is_empty() {
            return Ok(0);
        }

        let now = current_timestamp_millis();
        let member_user_ids: Vec<String> = members.iter().map(|m| m.member_user_id.clone()).collect();
        // The filter above guarantees `Some`, and the sync layer only ever
        // persists members it just sent, whose position is the response's.
        let positions: Vec<i64> = members.iter().filter_map(|m| m.last_sent_position).collect();
        let result = sqlx::query!(
            r#"
            INSERT INTO lazy_loaded_members (
                user_id,
                device_id,
                room_id,
                member_user_id,
                created_ts,
                updated_ts,
                last_sent_position
            )
            SELECT $1, $2, $3, member_user_id, $4, $4, last_sent_position
            FROM UNNEST($5::TEXT[], $6::BIGINT[]) AS t(member_user_id, last_sent_position)
            ON CONFLICT (user_id, device_id, room_id, member_user_id)
            DO UPDATE SET updated_ts = EXCLUDED.updated_ts, last_sent_position = EXCLUDED.last_sent_position
            "#,
            user_id,
            device_id,
            room_id,
            now,
            &member_user_ids[..],
            &positions[..]
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// 批量清掉某用户若干设备上的懒加载成员行，**事务内**（CODE-03）。
    async fn delete_lazy_loaded_members_for_devices_batch_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: &str,
        device_ids: &[String],
    ) -> Result<u64, sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(0);
        }
        sqlx::query!(
            r#"
            DELETE FROM lazy_loaded_members
            WHERE user_id = $1 AND device_id = ANY($2)
            "#,
            user_id,
            device_ids
        )
        .execute(&mut **tx)
        .await
        .map(|result| result.rows_affected())
    }

    /// [`Self::delete_lazy_loaded_members_for_user`] 的事务内版本（CODE-03：与删设备同一笔事务）。
    async fn delete_lazy_loaded_members_for_user_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: &str,
    ) -> Result<u64, sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM lazy_loaded_members
            WHERE user_id = $1
            "#,
            user_id
        )
        .execute(&mut **tx)
        .await
        .map(|result| result.rows_affected())
    }

    async fn delete_lazy_loaded_members_for_device_tx(
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        user_id: &str,
        device_id: &str,
    ) -> Result<u64, sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM lazy_loaded_members
            WHERE user_id = $1 AND device_id = $2
            "#,
            user_id,
            device_id
        )
        .execute(&mut **tx)
        .await
        .map(|result| result.rows_affected())
    }

    /// See [`create_device`].
    pub async fn create_device(
        &self,
        device_id: &str,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<Device, sqlx::Error> {
        let mut tx = self.pool.begin().await?;
        let device = self.create_device_tx(&mut tx, device_id, user_id, display_name).await?;
        tx.commit().await?;
        Ok(device)
    }

    /// Creates a new device in the database within a transaction.
    pub async fn create_device_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        device_id: &str,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<Device, sqlx::Error> {
        let now = current_timestamp_millis();
        let device = sqlx::query_as!(
            Device,
            r#"
            INSERT INTO devices (device_id, user_id, display_name, first_seen_ts, last_seen_ts, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, user_agent, appservice_id, ignored_user_list
            "#,
            device_id,
            user_id,
            display_name,
            now,
            now,
            now
        )
        .fetch_one(&mut **tx)
        .await?;

        Self::delete_lazy_loaded_members_for_device_tx(tx, user_id, device_id).await?;

        let stream_id: i64 = sqlx::query_scalar!(
            r#"
            INSERT INTO device_lists_stream (user_id, device_id, created_ts)
            VALUES ($1, $2, $3)
            RETURNING stream_id
            "#,
            user_id,
            device_id,
            now
        )
        .fetch_one(&mut **tx)
        .await?;

        sqlx::query!(
            r#"
            INSERT INTO device_lists_changes (user_id, device_id, change_type, stream_id, created_ts)
            VALUES ($1, $2, 'changed', $3, $4)
            "#,
            user_id,
            device_id,
            stream_id,
            now
        )
        .execute(&mut **tx)
        .await?;

        Ok(device)
    }

    /// See [`get_device`].
    pub async fn get_device(&self, device_id: &str) -> Result<Option<Device>, sqlx::Error> {
        sqlx::query_as!(
            Device,
            r#"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, user_agent, appservice_id, ignored_user_list
            FROM devices WHERE device_id = $1
            "#,
            device_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`get_user_devices`].
    pub async fn get_user_devices(&self, user_id: &str) -> Result<Vec<Device>, sqlx::Error> {
        sqlx::query_as!(
            Device,
            r#"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, user_agent, appservice_id, ignored_user_list
            FROM devices WHERE user_id = $1 ORDER BY last_seen_ts DESC, device_id DESC
            "#,
            user_id
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`update_device_display_name`].
    pub async fn update_device_display_name(&self, device_id: &str, display_name: &str) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE devices SET display_name = $1 WHERE device_id = $2
            "#,
            display_name,
            device_id
        )
        .execute(&*self.pool)
        .await?;

        if let Some(device) = self.get_device(device_id).await? {
            // D-37: was `let _ = …`. A dropped device-list change hides the rename from every
            // peer that tracks this device; `?` is safe because a retry re-runs the UPDATE and
            // this notification together, so the retry repairs the stream.
            self.record_device_list_change(&device.user_id, Some(device_id), "changed").await?;
        }
        Ok(())
    }

    /// See [`update_user_device_display_name`].
    pub async fn update_user_device_display_name(
        &self,
        user_id: &str,
        device_id: &str,
        display_name: &str,
    ) -> Result<u64, sqlx::Error> {
        let rows_affected = sqlx::query!(
            r#"
            UPDATE devices
            SET display_name = $1
            WHERE device_id = $2 AND user_id = $3
            "#,
            display_name,
            device_id,
            user_id
        )
        .execute(&*self.pool)
        .await
        .map(|result| result.rows_affected())?;

        if rows_affected > 0 {
            // D-37: same reasoning as `update_device_display_name` — retry re-runs both.
            self.record_device_list_change(user_id, Some(device_id), "changed").await?;
        }

        Ok(rows_affected)
    }

    /// See [`update_device_last_seen`].
    pub async fn update_device_last_seen(&self, device_id: &str) -> Result<(), sqlx::Error> {
        let now = current_timestamp_millis();
        sqlx::query!(
            r#"
            UPDATE devices SET last_seen_ts = $1 WHERE device_id = $2
            "#,
            now,
            device_id
        )
        .execute(&*self.pool)
        .await?;
        Ok(())
    }

    /// See [`delete_device`].
    ///
    /// ## 为什么是一笔事务（CODE-03）
    ///
    /// 这里原本是"删设备 → 清 `lazy_loaded_members` → 记 device_lists 变更"三步各自 autocommit，
    /// 后两步失败只 `warn!`。当时的理由是"行已经删了，重试也补不回来"，但那个理由只在**非事务**
    /// 的前提下成立：剩下的 `lazy_loaded_members` 行会让 `/sync` 把这些成员当成"客户端已知"
    /// 而抑制状态事件（用户看不到本该出现的成员变更），而 device_lists 少一条则让其它设备
    /// 永远不知道这个设备没了。
    ///
    /// 放进同一笔事务后，失败即整体回滚：设备还在、可以重试，也不会留下半截状态 —— 于是
    /// 那两条 `warn!` 变成了不该存在的分支，直接用 `?` 传播。
    ///
    /// 顺带把"先查后删"的 TOCTOU 收进事务：`existing` 只是用来拿 `user_id`，
    /// 真正决定是否记账的是事务内 `DELETE` 的 `rows_affected()` —— 若设备在这两步之间
    /// 已被并发删除，这一笔就什么都不做（由抢先的那笔负责记账，不重复也不遗漏）。
    pub async fn delete_device(&self, device_id: &str) -> Result<(), sqlx::Error> {
        let existing = self.get_device(device_id).await?;
        let mut tx = self.pool.begin().await?;

        let rows_affected = sqlx::query!(
            r#"
            DELETE FROM devices WHERE device_id = $1
            "#,
            device_id
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if rows_affected > 0 {
            if let Some(device) = existing {
                Self::delete_lazy_loaded_members_for_device_tx(&mut tx, &device.user_id, device_id).await?;
                Self::record_device_list_change_tx(&mut tx, &device.user_id, Some(device_id), "deleted").await?;
            }
        }

        tx.commit().await?;
        Ok(())
    }

    /// See [`delete_user_device`].
    pub async fn delete_user_device(&self, user_id: &str, device_id: &str) -> Result<u64, sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        let rows_affected = sqlx::query!(
            r#"
            DELETE FROM devices
            WHERE device_id = $1 AND user_id = $2
            "#,
            device_id,
            user_id
        )
        .execute(&mut *tx)
        .await
        .map(|result| result.rows_affected())?;

        if rows_affected > 0 {
            Self::delete_lazy_loaded_members_for_device_tx(&mut tx, user_id, device_id).await?;
            // 与 `delete_device` 共用同一个 `_tx` 实现，不再内联一份 SQL。
            Self::record_device_list_change_tx(&mut tx, user_id, Some(device_id), "deleted").await?;
        }

        tx.commit().await?;
        Ok(rows_affected)
    }

    /// See [`delete_user_devices`].
    pub async fn delete_user_devices(&self, user_id: &str) -> Result<(), sqlx::Error> {
        let mut tx = self.pool.begin().await?;

        // 取 id 列表与删设备放进同一笔事务：原来在事务外先 `SELECT` 再 `DELETE`，
        // 中间新建/删除的设备会漏记或错记 device_lists 变更。
        // （SQL 文本与事务外版本逐字相同，只是换了执行器 —— 这样 `.sqlx` 缓存无需重生成。）
        let device_ids: Vec<String> = sqlx::query_scalar!(
            r#"
            SELECT device_id FROM devices WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_all(&mut *tx)
        .await?;

        let rows_affected = sqlx::query!(
            r#"
            DELETE FROM devices WHERE user_id = $1
            "#,
            user_id
        )
        .execute(&mut *tx)
        .await
        .map(|result| result.rows_affected())?;

        if rows_affected > 0 {
            // CODE-03：清理与变更记录原来是"失败只 warn"，导致 stale `lazy_loaded_members`
            // 让 `/sync` 抑制成员状态事件。放进事务后失败即回滚，用 `?` 直接传播。
            Self::delete_lazy_loaded_members_for_user_tx(&mut tx, user_id).await?;
            Self::record_device_list_changes_batch_tx(&mut tx, user_id, &device_ids, "deleted").await?;
        }

        tx.commit().await?;
        Ok(())
    }

    /// See [`delete_devices_batch`].
    pub async fn delete_devices_batch(&self, device_ids: &[String]) -> Result<u64, sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(0);
        }

        let mut tx = self.pool.begin().await?;

        // 取 (user_id, device_id) 与删设备在同一笔事务里：原来在事务外先 `SELECT` 再 `DELETE`，
        // 并发删同一批设备时会重复记账或漏记。
        let rows = sqlx::query!(
            r"
            SELECT user_id, device_id FROM devices WHERE device_id = ANY($1)
            ",
            device_ids
        )
        .fetch_all(&mut *tx)
        .await?;

        let rows_affected = sqlx::query!(r#"DELETE FROM devices WHERE device_id = ANY($1)"#, device_ids)
            .execute(&mut *tx)
            .await
            .map(|result| result.rows_affected())?;

        if rows_affected > 0 {
            // Group deleted devices by user_id for batch side-effect writes
            let mut by_user: HashMap<&str, Vec<String>> = HashMap::new();
            for row in &rows {
                by_user.entry(row.user_id.as_str()).or_default().push(row.device_id.clone());
            }
            for (user_id, user_device_ids) in &by_user {
                // CODE-03：原来"失败只 warn"，stale `lazy_loaded_members` 会让 `/sync`
                // 抑制成员状态事件；现在同一笔事务，失败即回滚。
                Self::delete_lazy_loaded_members_for_devices_batch_tx(&mut tx, user_id, user_device_ids).await?;
                Self::record_device_list_changes_batch_tx(&mut tx, user_id, user_device_ids, "deleted").await?;
            }
        }

        tx.commit().await?;
        Ok(rows_affected)
    }

    /// See [`delete_user_devices_batch`].
    pub async fn delete_user_devices_batch(&self, user_id: &str, device_ids: &[String]) -> Result<u64, sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(0);
        }

        let mut tx = self.pool.begin().await?;
        let rows_affected =
            sqlx::query!(r#"DELETE FROM devices WHERE user_id = $1 AND device_id = ANY($2)"#, user_id, device_ids)
                .execute(&mut *tx)
                .await
                .map(|result| result.rows_affected())?;

        if rows_affected > 0 {
            // CODE-03：同上，放进事务后不再有"删了但没清干净"的半截状态。
            Self::delete_lazy_loaded_members_for_devices_batch_tx(&mut tx, user_id, device_ids).await?;
            Self::record_device_list_changes_batch_tx(&mut tx, user_id, device_ids, "deleted").await?;
        }

        tx.commit().await?;
        Ok(rows_affected)
    }

    /// See [`device_exists`].
    pub async fn device_exists(&self, device_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_scalar!(
            r#"
            SELECT 1 AS "exists!" FROM devices WHERE device_id = $1 LIMIT 1
            "#,
            device_id
        )
        .fetch_optional(&*self.pool)
        .await?;
        Ok(result.is_some())
    }

    /// See [`get_devices_batch`].
    pub async fn get_devices_batch(&self, device_ids: &[String]) -> Result<Vec<Device>, sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_as!(
            Device,
            r#"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, user_agent, appservice_id, ignored_user_list
            FROM devices WHERE device_id = ANY($1)
            ORDER BY last_seen_ts DESC, device_id DESC
            "#,
            device_ids
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`get_users_devices_batch`].
    pub async fn get_users_devices_batch(
        &self,
        user_ids: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<Device>>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let devices: Vec<Device> = sqlx::query_as!(
            Device,
            r"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, user_agent, appservice_id, ignored_user_list
            FROM devices WHERE user_id = ANY($1)
            ORDER BY user_id, last_seen_ts DESC
            ",
            user_ids
        )
        .fetch_all(&*self.pool)
        .await?;

        let mut result: std::collections::HashMap<String, Vec<Device>> =
            user_ids.iter().map(|id| (id.clone(), Vec::new())).collect();

        for device in devices {
            if let Some(user_devices) = result.get_mut(&device.user_id) {
                user_devices.push(device);
            }
        }

        Ok(result)
    }

    /// See [`get_device_keys_for_users`].
    pub async fn get_device_keys_for_users(
        &self,
        user_ids: &[String],
    ) -> Result<std::collections::HashMap<String, std::collections::HashMap<String, Device>>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(std::collections::HashMap::new());
        }

        let devices: Vec<Device> = sqlx::query_as!(
            Device,
            r"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, user_agent, appservice_id, ignored_user_list
            FROM devices
            WHERE user_id = ANY($1) AND device_key IS NOT NULL
            ",
            user_ids
        )
        .fetch_all(&*self.pool)
        .await?;

        let mut result: std::collections::HashMap<String, std::collections::HashMap<String, Device>> =
            user_ids.iter().map(|id| (id.clone(), std::collections::HashMap::new())).collect();

        for device in devices {
            if let Some(user_devices) = result.get_mut(&device.user_id) {
                user_devices.insert(device.device_id.clone(), device);
            }
        }

        Ok(result)
    }

    /// See [`get_device_count`].
    pub async fn get_device_count(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let count = sqlx::query_scalar!(
            r#"
            SELECT COUNT(*) AS "count!" FROM devices WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_one(&*self.pool)
        .await?;
        Ok(count)
    }

    /// See [`get_user_device`].
    pub async fn get_user_device(&self, user_id: &str, device_id: &str) -> Result<Option<Device>, sqlx::Error> {
        sqlx::query_as!(
            Device,
            r#"
            SELECT device_id, user_id, display_name, device_key, last_seen_ts, last_seen_ip, created_ts, first_seen_ts, user_agent, appservice_id, ignored_user_list
            FROM devices WHERE user_id = $1 AND device_id = $2
            "#,
            user_id,
            device_id
        )
        .fetch_optional(&*self.pool)
        .await
    }

    /// See [`filter_existing_users`].
    pub async fn filter_existing_users(&self, user_ids: &[String]) -> Result<Vec<String>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        sqlx::query_scalar!(
            r#"
            SELECT DISTINCT user_id FROM devices WHERE user_id = ANY($1)
            "#,
            user_ids
        )
        .fetch_all(&*self.pool)
        .await
    }

    /// See [`update_devices_last_seen_batch`].
    pub async fn update_devices_last_seen_batch(&self, device_ids: &[String]) -> Result<u64, sqlx::Error> {
        if device_ids.is_empty() {
            return Ok(0);
        }

        let now = current_timestamp_millis();
        let result = sqlx::query!(
            r#"
            UPDATE devices SET last_seen_ts = $1 WHERE device_id = ANY($2)
            "#,
            now,
            device_ids
        )
        .execute(&*self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    /// Get the maximum stream ID from the device_lists_stream table.
    /// Returns 0 if the table is empty.
    pub async fn get_max_device_list_stream_id(&self) -> Result<i64, sqlx::Error> {
        let max_id: i64 = sqlx::query_scalar!(
            r#"
            SELECT COALESCE(MAX(stream_id), 0) AS "max_id!" FROM device_lists_stream
            "#
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(max_id)
    }

    /// See [`get_max_device_list_stream_id_for_user`].
    pub async fn get_max_device_list_stream_id_for_user(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        let max_id: i64 = sqlx::query_scalar!(
            r#"
            SELECT COALESCE(MAX(stream_id), 0) AS "max_id!"
            FROM device_lists_stream
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_one(&*self.pool)
        .await?;

        Ok(max_id)
    }

    /// See [`has_device_list_updates_since`].
    pub async fn has_device_list_updates_since(&self, since_stream_id: i64) -> Result<bool, sqlx::Error> {
        Ok(self.get_max_device_list_stream_id().await? > since_stream_id)
    }

    /// See [`get_device_list_changed_users_since`].
    pub async fn get_device_list_changed_users_since(
        &self,
        since_stream_id: i64,
        exclude_user_id: &str,
    ) -> Result<(Vec<String>, i64), sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT user_id, MAX(stream_id) AS "max_id!"
            FROM device_lists_stream
            WHERE stream_id > $1
              AND user_id != $2
            GROUP BY user_id
            ORDER BY MAX(stream_id) ASC
            LIMIT 100
            "#,
            since_stream_id,
            exclude_user_id
        )
        .fetch_all(&*self.pool)
        .await?;

        let mut max_stream_id = since_stream_id;
        let changed = rows
            .into_iter()
            .map(|row| {
                let user_id = row.user_id;
                let stream_id = row.max_id;
                if stream_id > max_stream_id {
                    max_stream_id = stream_id;
                }
                user_id
            })
            .collect();

        Ok((changed, max_stream_id))
    }

    /// See [`get_device_list_left_users_since`].
    pub async fn get_device_list_left_users_since(
        &self,
        since_stream_id: i64,
        exclude_user_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT DISTINCT dl.user_id
            FROM device_lists_stream dl
            LEFT JOIN room_memberships rm ON rm.user_id = dl.user_id
            WHERE dl.stream_id > $1
              AND dl.user_id != $2
              AND rm.user_id IS NULL
            ORDER BY dl.user_id
            LIMIT 100
            "#,
            since_stream_id,
            exclude_user_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| row.user_id).collect())
    }

    /// Get device lists changes with shared room permission checks
    pub async fn get_device_lists_since_with_shared_rooms(
        &self,
        since_stream_id: i64,
        exclude_user_id: &str,
    ) -> Result<(Vec<String>, Vec<String>), sqlx::Error> {
        let changed_rows = sqlx::query!(
            r#"
            SELECT DISTINCT dls.user_id
            FROM device_lists_stream dls
            INNER JOIN room_memberships rm1 ON rm1.user_id = dls.user_id AND rm1.membership = 'join'
            INNER JOIN room_memberships rm2 ON rm2.room_id = rm1.room_id AND rm2.user_id = $2 AND rm2.membership = 'join'
            WHERE dls.stream_id > $1
              AND dls.user_id != $2
            ORDER BY dls.user_id
            LIMIT 100
            "#,
            since_stream_id,
            exclude_user_id
        )
        .fetch_all(&*self.pool)
        .await?;

        let changed: Vec<String> = changed_rows.into_iter().map(|row| row.user_id).collect();
        // We only persist current room membership, not a stream-aware history of
        // "shared room -> no shared room" transitions for the requesting user.
        // Returning `left` based only on current membership would leak isolated
        // users that never shared a room in the first place. Be conservative and
        // suppress `left` until we can derive it from a proper membership delta.
        let left = Vec::new();

        Ok((changed, left))
    }

    /// See [`get_device_list_changes`].
    pub async fn get_device_list_changes(
        &self,
        since: i64,
        to: i64,
        user_ids: &[String],
    ) -> Result<Vec<(String, Option<String>, String, i64)>, sqlx::Error> {
        if user_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rows = sqlx::query!(
            r"
            SELECT user_id, device_id, change_type, stream_id
            FROM device_lists_changes
            WHERE stream_id > $1
              AND stream_id <= $2
              AND user_id = ANY($3)
            ORDER BY stream_id ASC
            ",
            since,
            to,
            user_ids
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| (row.user_id, row.device_id, row.change_type, row.stream_id)).collect())
    }

    /// See [`get_devices_by_user_device_pairs`].
    pub async fn get_devices_by_user_device_pairs(
        &self,
        user_ids: &[&str],
        device_ids: &[&str],
    ) -> Result<Vec<(String, String, Option<String>, Option<i64>)>, sqlx::Error> {
        if user_ids.is_empty() || device_ids.is_empty() {
            return Ok(Vec::new());
        }

        // `query!` 的类型检查要求实参类型与推断出的参数类型精确一致（`&[String]`）；
        // `&[&str]` 只有动态 `.bind()` 才接受，因此这里转成 owned 切片再传入。
        let user_ids: Vec<String> = user_ids.iter().map(|id| (*id).to_owned()).collect();
        let device_ids: Vec<String> = device_ids.iter().map(|id| (*id).to_owned()).collect();

        let rows = sqlx::query!(
            r"
            SELECT user_id, device_id, display_name, last_seen_ts
            FROM devices
            WHERE (user_id, device_id) = ANY(SELECT * FROM UNNEST($1::text[], $2::text[]))
            ",
            &user_ids[..],
            &device_ids[..]
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| (row.user_id, row.device_id, row.display_name, row.last_seen_ts)).collect())
    }

    /// Get the distinct user IDs whose device lists changed in the given stream
    /// range, excluding the requesting user.
    pub async fn get_device_list_changed_users(
        &self,
        from: i64,
        to: i64,
        exclude_user_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT DISTINCT user_id
            FROM device_lists_stream
            WHERE stream_id > $1
              AND stream_id <= $2
              AND user_id != $3
            ORDER BY user_id
            LIMIT 100
            "#,
            from,
            to,
            exclude_user_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| row.user_id).collect())
    }

    /// Get the distinct user IDs who left (no room membership) in the given
    /// stream range, excluding the requesting user.
    pub async fn get_device_list_left_users(
        &self,
        from: i64,
        to: i64,
        exclude_user_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query!(
            r#"
            SELECT DISTINCT dl.user_id
            FROM device_lists_stream dl
            LEFT JOIN room_memberships rm ON rm.user_id = dl.user_id
            WHERE dl.stream_id > $1
              AND dl.stream_id <= $2
              AND dl.user_id != $3
              AND rm.user_id IS NULL
            ORDER BY dl.user_id
            LIMIT 100
            "#,
            from,
            to,
            exclude_user_id
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|row| row.user_id).collect())
    }
}

impl DeviceStorage {
    /// Adapter: trait `get_device_by_id(device_id)` → inherent `get_device(device_id)`
    pub async fn get_device_by_id(&self, device_id: &str) -> Result<Option<Device>, sqlx::Error> {
        self.get_device(device_id).await
    }

    /// Adapter: trait `delete_device_returning_count(user_id, device_id)` → inherent `delete_user_device`
    pub async fn delete_device_returning_count(&self, user_id: &str, device_id: &str) -> Result<u64, sqlx::Error> {
        self.delete_user_device(user_id, device_id).await
    }

    /// Adapter: trait `delete_all_devices(user_id)` → inherent `delete_user_devices`
    pub async fn delete_all_devices(&self, user_id: &str) -> Result<(), sqlx::Error> {
        self.delete_user_devices(user_id).await
    }
}

// ── Trait delegation ────────────────────────────────────────────────────

#[async_trait]
impl DeviceListStoreApi for DeviceStorage {
    async fn record_device_list_change(
        &self,
        user_id: &str,
        device_id: Option<&str>,
        change_type: &str,
    ) -> Result<i64, sqlx::Error> {
        // 显式走固有方法：`self.record_device_list_change(..)` 在 trait impl 内部
        // 会解析到 trait 自身（无限递归），即使固有方法通常优先。
        DeviceStorage::record_device_list_change(self, user_id, device_id, change_type).await
    }

    async fn create_device(
        &self,
        device_id: &str,
        user_id: &str,
        display_name: Option<&str>,
    ) -> Result<Device, sqlx::Error> {
        self.create_device(device_id, user_id, display_name).await
    }

    async fn delete_device(&self, device_id: &str) -> Result<(), sqlx::Error> {
        self.delete_device(device_id).await
    }

    async fn get_user_devices(&self, user_id: &str) -> Result<Vec<Device>, sqlx::Error> {
        self.get_user_devices(user_id).await
    }

    async fn get_device(&self, device_id: &str) -> Result<Option<Device>, sqlx::Error> {
        self.get_device(device_id).await
    }

    async fn update_user_device_display_name(
        &self,
        user_id: &str,
        device_id: &str,
        display_name: &str,
    ) -> Result<u64, sqlx::Error> {
        self.update_user_device_display_name(user_id, device_id, display_name).await
    }

    async fn get_max_device_list_stream_id(&self) -> Result<i64, sqlx::Error> {
        self.get_max_device_list_stream_id().await
    }

    async fn get_max_device_list_stream_id_for_user(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        self.get_max_device_list_stream_id_for_user(user_id).await
    }

    async fn get_device_list_changed_users(
        &self,
        from: i64,
        to: i64,
        requester_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        self.get_device_list_changed_users(from, to, requester_id).await
    }

    async fn get_device_list_left_users(
        &self,
        from: i64,
        to: i64,
        requester_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        self.get_device_list_left_users(from, to, requester_id).await
    }

    async fn get_users_devices_batch(
        &self,
        users: &[String],
    ) -> Result<std::collections::HashMap<String, Vec<Device>>, sqlx::Error> {
        self.get_users_devices_batch(users).await
    }

    async fn get_device_list_changes(
        &self,
        since: i64,
        to: i64,
        users: &[String],
    ) -> Result<Vec<(String, Option<String>, String, i64)>, sqlx::Error> {
        self.get_device_list_changes(since, to, users).await
    }

    async fn get_devices_by_user_device_pairs(
        &self,
        user_ids: &[&str],
        device_ids: &[&str],
    ) -> Result<Vec<(String, String, Option<String>, Option<i64>)>, sqlx::Error> {
        self.get_devices_by_user_device_pairs(user_ids, device_ids).await
    }

    async fn filter_existing_users(&self, users: &[String]) -> Result<Vec<String>, sqlx::Error> {
        self.filter_existing_users(users).await
    }

    // ── incremental device-list polling ──────────────────────────────────

    async fn has_device_list_updates_since(&self, since_stream_id: i64) -> Result<bool, sqlx::Error> {
        self.has_device_list_updates_since(since_stream_id).await
    }

    async fn get_device_lists_since_with_shared_rooms(
        &self,
        since_stream_id: i64,
        exclude_user_id: &str,
    ) -> Result<(Vec<String>, Vec<String>), sqlx::Error> {
        self.get_device_lists_since_with_shared_rooms(since_stream_id, exclude_user_id).await
    }

    // ── lazy-loaded members ──────────────────────────────────────────────

    async fn get_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
    ) -> Result<Vec<LazyLoadedMemberRow>, sqlx::Error> {
        self.get_lazy_loaded_members(user_id, device_id, room_id).await
    }

    async fn upsert_lazy_loaded_members(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        members: &[LazyLoadedMemberRow],
    ) -> Result<u64, sqlx::Error> {
        self.upsert_lazy_loaded_members(user_id, device_id, room_id, members).await
    }

    async fn delete_user_devices_batch(&self, user_id: &str, device_ids: &[String]) -> Result<u64, sqlx::Error> {
        self.delete_user_devices_batch(user_id, device_ids).await
    }

    async fn get_device_by_id(&self, device_id: &str) -> Result<Option<Device>, sqlx::Error> {
        self.get_device_by_id(device_id).await
    }

    async fn delete_device_returning_count(&self, user_id: &str, device_id: &str) -> Result<u64, sqlx::Error> {
        self.delete_device_returning_count(user_id, device_id).await
    }

    async fn delete_all_devices(&self, user_id: &str) -> Result<(), sqlx::Error> {
        self.delete_all_devices(user_id).await
    }

    async fn get_device_count(&self, user_id: &str) -> Result<i64, sqlx::Error> {
        self.get_device_count(user_id).await
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod db_tests;
