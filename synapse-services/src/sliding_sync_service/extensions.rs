use std::collections::BTreeSet;

use super::SlidingSyncService;
use serde_json::{json, Value};
use synapse_common::error::ApiError;

/// SS-12: Extracted from 6 identical inline checks across all sliding-sync
/// extensions. Returns `true` when the named extension is requested.
///
/// An extension entry is considered enabled when it is either:
/// - a bare `true` boolean, or
/// - an object whose `enabled` field is `true` (defaulting to `true` when the
///   object is present but `enabled` is omitted, matching MSC3886 sliding-sync
///   semantics).
///
/// Returns `false` when the entry is absent or explicitly disabled.
fn is_extension_enabled(request_extensions: &serde_json::Value, name: &str) -> bool {
    request_extensions
        .get(name)
        .and_then(|v| {
            if v.as_bool() == Some(true) {
                Some(true)
            } else {
                v.as_object().map(|obj| obj.get("enabled").and_then(|e| e.as_bool()).unwrap_or(true))
            }
        })
        .unwrap_or(false)
}

/// W7+ 缓存治理：extensions 去重缓存的 TTL（秒）。
///
/// 用于 presence / receipts / account_data 三条「上次下发的载荷」基线。
/// 语义是**去重基线**而非权威状态——残留只会导致下次 sync 多回显或少
/// 回显一次，不影响正确性，因此取值只需大于客户端最长轮询间隔即可。
///
/// 约束：必须 **≥ 连接空闲 GC 周期**（`CONNECTION_TTL_MS` = 30 分钟），否则
/// 客户端还在线（长轮询一直续期）时基线先过期，导致每次 sync 都判定
/// changed → 全量回显 → `is_idle` 失效 → 忙循环复发（S7 修复的那个开关）。
/// 30 分钟与 GC 周期对齐：连接真过期时两者同时失效，`invalidate_connection_
/// cache` 也会主动清一遍，这里是兜底。
const EXTENSION_DEDUP_CACHE_TTL_SECS: u64 = 30 * 60;

/// W7+ 缓存治理：e2ee 状态推进缓存的 TTL（秒）。
///
/// 用于 device list stream id 与 shared_users 快照。与去重基线不同，这两条
/// 是**游标**——残留会导致 since 回退（重发 device list）或前进过头（漏发）。
/// 前进过头是真实丢数据，因此 TTL 要显著长于去重基线：1 小时，覆盖长连接
/// 会话 + 一次跨实例路由故障恢复窗口。
const E2EE_STATE_CACHE_TTL_SECS: u64 = 60 * 60;

/// MSC3575 account_data 扩展载荷格式：事件数组 `[{type, content}]`。
///
/// storage 层（`get_global_account_data` / `get_room_account_data`）返回的是
/// `Map<type, content>` 紧凑结构，直接下发时 SDK 侧
/// `data.global.length > 0` 对 Object 恒为 false，事件永远不会被分发 ——
/// 表现为 `setDefaultKeyId` 等 sync 回送的流程（SSSS bootstrap / 安全密钥
/// 设置）永久挂起直至前端 15s 超时。此处统一转换为规范事件数组。
fn account_data_map_to_events(map: serde_json::Value) -> Vec<serde_json::Value> {
    match map {
        serde_json::Value::Object(entries) => entries
            .into_iter()
            .map(|(data_type, content)| serde_json::json!({ "type": data_type, "content": content }))
            .collect(),
        // 已是数组（如测试桩/未来 storage 改造）则原样透传
        serde_json::Value::Array(events) => events,
        other => vec![other],
    }
}

impl SlidingSyncService {
    pub(super) async fn build_extensions_response(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        since_pos: Option<&str>,
        rooms_response: &serde_json::Value,
        request_extensions: Option<&serde_json::Value>,
    ) -> Result<Option<serde_json::Value>, sqlx::Error> {
        let Some(request_extensions) = request_extensions else {
            return Ok(None);
        };

        let mut response_extensions = request_extensions.as_object().cloned().unwrap_or_default();

        let account_data_enabled = is_extension_enabled(request_extensions, "account_data");

        if account_data_enabled {
            let room_ids: Vec<String> =
                rooms_response.as_object().map(|obj| obj.keys().cloned().collect()).unwrap_or_default();

            let global = account_data_map_to_events(self.storage.get_global_account_data(user_id).await?);
            let room_map = self.storage.get_room_account_data(user_id, &room_ids).await?;
            let rooms = match room_map {
                serde_json::Value::Object(entries) => {
                    let mut rooms_out = serde_json::Map::new();
                    for (room_id, inner) in entries {
                        rooms_out.insert(room_id, serde_json::Value::Array(account_data_map_to_events(inner)));
                    }
                    serde_json::Value::Object(rooms_out)
                }
                other => other,
            };

            // account_data 去重：全量回显（SSSS 密钥 + m.direct 等）每次都非空，
            // 会让 has_new_extension_data 恒 true → is_idle 失效 → 忙循环（与
            // presence 自激励同源）。只有内容真正变化时才 insert，与 presence 对称。
            let cache_key = Self::account_data_cache_key(user_id, device_id, conn_id);
            // 事件数组按 type 排序，保证序列化结果稳定（HashMap 迭代序不稳定）。
            let mut canonical_global = global.clone();
            canonical_global.sort_by(|a, b| {
                let ta = a.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let tb = b.get("type").and_then(|v| v.as_str()).unwrap_or("");
                ta.cmp(tb)
            });
            let canonical = serde_json::to_string(&serde_json::json!({
                "global": canonical_global,
                "rooms": rooms,
            }))
            .unwrap_or_default();
            let changed = since_pos.is_none()
                || self
                    .cache
                    .get_raw_shared(&cache_key)
                    .await
                    .map(|prev| prev != canonical)
                    .unwrap_or(true);

            if changed {
                response_extensions.insert(
                    "account_data".to_string(),
                    serde_json::json!({
                        "global": global,
                        "rooms": rooms
                    }),
                );
                self.cache.set_raw(&cache_key, &canonical, EXTENSION_DEDUP_CACHE_TTL_SECS).await;
            }
        }

        let receipts_enabled = is_extension_enabled(request_extensions, "receipts");

        if receipts_enabled {
            let room_ids: Vec<String> =
                rooms_response.as_object().map(|obj| obj.keys().cloned().collect()).unwrap_or_default();
            let receipts = self.storage.get_receipts_for_rooms(&room_ids).await?;
            let receipts_payload = serde_json::json!({ "rooms": receipts });

            // Receipts dedup (same pattern as presence): cache the serialized
            // payload per connection; only insert into extensions_response when
            // the payload actually changed. Without this, receipts is always
            // non-empty (returns full read-receipt state for subscribed rooms),
            // has_new_extension_data returns true, is_idle never fires, and the
            // client busy-loops at ~30+ req/s.
            let cache_key = Self::receipts_cache_key(user_id, device_id, conn_id);
            let canonical = receipts_payload.to_string();
            let changed = self.cache.get_raw_shared(&cache_key).await.is_none_or(|prev| prev != canonical);
            if changed {
                response_extensions.insert("receipts".to_string(), receipts_payload);
                self.cache.set_raw(&cache_key, &canonical, EXTENSION_DEDUP_CACHE_TTL_SECS).await;
            }
        }

        let typing_enabled = is_extension_enabled(request_extensions, "typing");

        if typing_enabled {
            let room_ids: Vec<String> =
                rooms_response.as_object().map(|obj| obj.keys().cloned().collect()).unwrap_or_default();
            let mut typing_rooms = serde_json::Map::new();
            match self.typing_service.get_typing_users_batch(&room_ids).await {
                Ok(batch) => {
                    for (room_id, user_ids) in batch {
                        typing_rooms.insert(room_id, serde_json::json!({ "user_ids": user_ids }));
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, room_count = room_ids.len(), "Failed to get typing users batch");
                }
            }
            response_extensions.insert(
                "typing".to_string(),
                serde_json::json!({
                    "rooms": typing_rooms
                }),
            );
        }

        let to_device_request = request_extensions.get("to_device");
        let to_device_enabled = is_extension_enabled(request_extensions, "to_device");

        if to_device_enabled {
            let to_device = self.build_to_device_extension(user_id, device_id, to_device_request).await?;
            response_extensions.insert("to_device".to_string(), to_device);
        }

        let e2ee_enabled = is_extension_enabled(request_extensions, "e2ee");

        if e2ee_enabled {
            let e2ee = self.build_e2ee_extension(user_id, device_id, conn_id, since_pos).await?;
            response_extensions.insert("e2ee".to_string(), e2ee);
        }

        let presence_enabled = is_extension_enabled(request_extensions, "presence");

        if presence_enabled {
            let room_ids: Vec<String> =
                rooms_response.as_object().map(|obj| obj.keys().cloned().collect()).unwrap_or_default();

            let mut all_members = std::collections::HashSet::new();
            all_members.insert(user_id.to_string());

            if let Ok(batch) = self.member_storage.get_members_batch(&room_ids, "join").await {
                for members in batch.values() {
                    for member in members {
                        all_members.insert(member.user_id.clone());
                    }
                }
            }

            let member_list: Vec<String> = all_members.into_iter().collect();
            // SS-07: 需要 last_active_ts 计算真实的 last_active_ago，
            // 因此走 get_presence_snapshots 而非丢弃时间戳的 get_presences。
            let snapshots = self.presence_storage.get_presence_snapshots(&member_list).await?;
            let now_ts = synapse_common::current_timestamp_millis();
            let (presence_events, canonical_events) = Self::build_presence_events(&snapshots, now_ts);

            // De-duplicate the presence extension across incremental syncs.
            //
            // The presence extension previously echoed the full presence set on
            // EVERY response. Since the client long-polls in a tight loop, that
            // produced a 1:1 sync↔presence self-excitation (every sync carried a
            // "fresh" presence event). We now only include the presence
            // extension when the payload actually changed since the last sync
            // for this connection. When nothing changed, the response carries no
            // presence data, so the client's sliding-sync loop has nothing to
            // react to and backs off instead of busy-looping.
            //
            // S7: 去重状态走 `get_raw_shared`（L1 未命中回源 Redis 并回填）。
            // 此前用只读 L1 的同步 `get_raw`：跨实例路由/进程重启/本地驱逐后
            // 误判 changed=true → presence 回声 → extensions 非空 → is_idle
            // 失效 → 忙循环复发。这正是 S7 要关掉的复发开关。
            let cache_key = Self::presence_cache_key(user_id, device_id, conn_id);
            let payload = serde_json::json!({ "events": presence_events });
            // SS-07: 去重比较用时间无关的规范化载荷（last_active_ts），
            // 否则 last_active_ago 每毫秒都变，去重永远不命中、回声复发。
            let payload_str =
                serde_json::to_string(&serde_json::json!({ "events": canonical_events })).unwrap_or_default();

            let changed = since_pos.is_none()
                || self.cache.get_raw_shared(&cache_key).await.map(|prev| prev != payload_str).unwrap_or(true);

            if changed {
                response_extensions.insert("presence".to_string(), payload);
                self.cache.set_raw(&cache_key, &payload_str, EXTENSION_DEDUP_CACHE_TTL_SECS).await;
            }
        }

        if response_extensions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(serde_json::Value::Object(response_extensions)))
        }
    }

    /// SS-07: 由 presence 快照构建 extensions 事件。
    ///
    /// 返回 `(wire_events, canonical_events)`：
    /// - `wire_events` 是下发给客户端的 m.presence 事件，`last_active_ago`
    ///   由 `now_ts - last_active_ts` 实时计算（offline 或无时间戳时为 null）；
    /// - `canonical_events` 携带原始 `last_active_ts`，用于增量 sync 的去重
    ///   比较——若用 wire 事件比较，`last_active_ago` 随时间漂移会导致
    ///   每次 sync 都判定 changed，presence 回声复发（S7 的复发开关）。
    ///
    /// 两个列表都按 sender 排序：HashMap 迭代序不稳定，不排序会让序列化
    /// 结果每次不同，去重同样永不命中。
    pub(crate) fn build_presence_events(
        snapshots: &std::collections::HashMap<String, synapse_storage::presence::PresenceSnapshot>,
        now_ts: i64,
    ) -> (Vec<Value>, Vec<Value>) {
        let mut wire_events = Vec::with_capacity(snapshots.len().min(32));
        let mut canonical_events = Vec::with_capacity(snapshots.len().min(32));
        for (uid, snap) in snapshots {
            let last_active_ago =
                if snap.presence == "offline" { None } else { snap.last_active_ts.map(|ts| (now_ts - ts).max(0)) };
            wire_events.push(serde_json::json!({
                "sender": uid,
                "type": "m.presence",
                "content": {
                    "user_id": uid,
                    "presence": snap.presence,
                    "status_msg": snap.status_msg,
                    "last_active_ago": last_active_ago,
                }
            }));
            canonical_events.push(serde_json::json!({
                "sender": uid,
                "presence": snap.presence,
                "status_msg": snap.status_msg,
                // 注意：canonical（去重比较）载荷必须排除 last_active_ts。
                // sync 本身是「活跃操作」，每次 sync 都会把本用户的 presence
                // last_active_ts 刷新为当前时间 → 若 canonical 含 last_active_ts，
                // 每次比较都不同 → changed 恒 true → presence 每次插入 →
                // has_new_extension_data 恒 true → is_idle 失效 → sync↔presence
                // 自激励忙循环（实测 ~40 req/s）。只比较真正的状态
                // （presence + status_msg），last_active_ts 变化不算「新 presence 事件」。
            }));
        }
        let sort_by_sender = |events: &mut Vec<Value>| {
            events.sort_by(|a, b| {
                let sa = a.get("sender").and_then(|v| v.as_str()).unwrap_or("");
                let sb = b.get("sender").and_then(|v| v.as_str()).unwrap_or("");
                sa.cmp(sb)
            });
        };
        sort_by_sender(&mut wire_events);
        sort_by_sender(&mut canonical_events);
        (wire_events, canonical_events)
    }

    async fn build_e2ee_extension(
        &self,
        user_id: &str,
        device_id: &str,
        conn_id: Option<&str>,
        since_pos: Option<&str>,
    ) -> Result<Value, sqlx::Error> {
        let key_counts = self
            .device_key_storage
            .get_one_time_keys_count_by_algorithm(user_id, device_id)
            .await
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

        let stream_cache_key = Self::e2ee_device_list_stream_cache_key(user_id, device_id, conn_id);
        let shared_users_cache_key = Self::e2ee_shared_users_cache_key(user_id, device_id, conn_id);
        let since_stream_id = if since_pos.is_some() {
            // S7: L1 未命中回源 Redis，避免跨实例后 since 回退 0 全量重发 device list
            self.cache.get_raw_shared(&stream_cache_key).await.and_then(|raw| raw.parse::<i64>().ok()).unwrap_or(0)
        } else {
            0
        };
        let current_stream_id = self.get_current_device_list_stream_id().await?;
        let changed = self.get_changed_device_lists_since(user_id, since_stream_id).await?;
        let previous_shared_users =
            if since_pos.is_some() { self.load_cached_shared_users(&shared_users_cache_key).await } else { Vec::new() };
        let current_shared_users = self.get_current_shared_users(user_id).await?;
        let left = Self::compute_left_shared_users(&previous_shared_users, &current_shared_users);

        self.cache.set_raw(&stream_cache_key, &current_stream_id.to_string(), E2EE_STATE_CACHE_TTL_SECS).await;
        self.cache
            .set_raw(
                &shared_users_cache_key,
                &serde_json::to_string(&current_shared_users).unwrap_or_else(|_| "[]".to_string()),
                E2EE_STATE_CACHE_TTL_SECS,
            )
            .await;

        let mut otk_counts = serde_json::Map::new();
        for (algo, count) in key_counts {
            otk_counts.insert(algo, json!(count));
        }

        let unused_fallback_types =
            self.device_key_storage.get_unused_fallback_key_types(user_id, device_id).await.unwrap_or_else(|_| vec![]);

        Ok(json!({
            "device_lists": {
                "changed": changed,
                "left": left,
            },
            "device_one_time_keys_count": otk_counts,
            "device_unused_fallback_key_types": unused_fallback_types,
        }))
    }

    async fn build_to_device_extension(
        &self,
        user_id: &str,
        device_id: &str,
        request_to_device: Option<&Value>,
    ) -> Result<Value, sqlx::Error> {
        let since_stream_id = request_to_device
            .and_then(|value| value.as_object())
            .and_then(|obj| obj.get("since"))
            .and_then(|value| value.as_str())
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0);
        let limit = request_to_device
            .and_then(|value| value.as_object())
            .and_then(|obj| obj.get("limit"))
            .and_then(|value| value.as_i64())
            .filter(|value| *value > 0)
            .unwrap_or(100);

        let (events, next_batch) = self
            .get_to_device_extension_payload(user_id, device_id, since_stream_id, limit)
            .await
            .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;

        Ok(json!({
            "events": events,
            "next_batch": next_batch,
        }))
    }

    pub(crate) fn e2ee_device_list_stream_cache_key(user_id: &str, device_id: &str, conn_id: Option<&str>) -> String {
        match conn_id {
            Some(conn_id) => format!("sliding_sync:e2ee:{user_id}:{device_id}:{conn_id}"),
            None => format!("sliding_sync:e2ee:{user_id}:{device_id}:"),
        }
    }

    pub(crate) fn e2ee_shared_users_cache_key(user_id: &str, device_id: &str, conn_id: Option<&str>) -> String {
        match conn_id {
            Some(conn_id) => format!("sliding_sync:e2ee:shared_users:{user_id}:{device_id}:{conn_id}"),
            None => format!("sliding_sync:e2ee:shared_users:{user_id}:{device_id}:"),
        }
    }

    /// Cache key under which the last-sent presence extension payload for a
    /// connection is stored, used to de-duplicate presence echoes across
    /// incremental syncs (see `build_extensions_response`).
    pub(crate) fn presence_cache_key(user_id: &str, device_id: &str, conn_id: Option<&str>) -> String {
        match conn_id {
            Some(conn_id) => format!("sliding_sync:presence:{user_id}:{device_id}:{conn_id}"),
            None => format!("sliding_sync:presence:{user_id}:{device_id}:"),
        }
    }

    /// Cache key for account_data 去重（避免每次 sync 全量回显 SSSS 密钥 /
    /// m.direct 导致 is_idle 失效、忙循环）。
    pub(crate) fn account_data_cache_key(user_id: &str, device_id: &str, conn_id: Option<&str>) -> String {
        match conn_id {
            Some(conn_id) => format!("sliding_sync:account_data:{user_id}:{device_id}:{conn_id}"),
            None => format!("sliding_sync:account_data:{user_id}:{device_id}:"),
        }
    }

    /// Cache key for receipts 去重（避免每次 sync 回显全量已读状态
    /// 导致 is_idle 失效、忙循环）。
    pub(crate) fn receipts_cache_key(user_id: &str, device_id: &str, conn_id: Option<&str>) -> String {
        match conn_id {
            Some(conn_id) => format!("sliding_sync:receipts:{user_id}:{device_id}:{conn_id}"),
            None => format!("sliding_sync:receipts:{user_id}:{device_id}:"),
        }
    }

    async fn get_current_device_list_stream_id(&self) -> Result<i64, sqlx::Error> {
        self.device_storage.get_max_device_list_stream_id().await
    }

    async fn get_changed_device_lists_since(
        &self,
        user_id: &str,
        since_stream_id: i64,
    ) -> Result<Vec<String>, sqlx::Error> {
        let (changed, _) =
            self.device_storage.get_device_lists_since_with_shared_rooms(since_stream_id, user_id).await?;
        Ok(changed)
    }

    async fn get_current_shared_users(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let mut users = self.member_storage.get_shared_room_users(user_id).await?;
        users.sort();
        users.dedup();
        Ok(users)
    }

    /// S7: 改为 async 并走 `get_raw_shared`——L1 未命中时回源 Redis。
    /// 此前同步 `get_raw` 在跨实例/重启后返回 None，previous 视为空集，
    /// `left` 永远算不出来（漏报离开共享房间的用户），属正确性问题。
    async fn load_cached_shared_users(&self, cache_key: &str) -> Vec<String> {
        self.cache
            .get_raw_shared(cache_key)
            .await
            .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
            .map(|mut users| {
                users.sort();
                users.dedup();
                users
            })
            .unwrap_or_default()
    }

    pub(crate) fn compute_left_shared_users(previous: &[String], current: &[String]) -> Vec<String> {
        let previous: BTreeSet<&str> = previous.iter().map(String::as_str).collect();
        let current: BTreeSet<&str> = current.iter().map(String::as_str).collect();

        previous.difference(&current).map(|user_id| (*user_id).to_string()).collect()
    }

    async fn get_to_device_extension_payload(
        &self,
        user_id: &str,
        device_id: &str,
        since_stream_id: i64,
        limit: i64,
    ) -> Result<(Vec<Value>, Option<String>), ApiError> {
        let (events, last_stream_id) =
            self.to_device_storage.get_messages_since(user_id, device_id, since_stream_id, limit).await?;

        let next_batch = if events.is_empty() {
            Some(self.to_device_storage.get_current_stream_id(user_id, device_id).await?.to_string())
        } else {
            Some(last_stream_id.to_string())
        };

        Ok((events, next_batch))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_data_map_converted_to_msc3575_event_array() {
        let map = serde_json::json!({
            "m.secret_storage.default_key": { "key": "abc" },
            "m.push_rules": { "global": [] }
        });
        let events = account_data_map_to_events(map);
        assert_eq!(events.len(), 2);
        let default_key = events.iter().find(|e| e["type"] == "m.secret_storage.default_key").expect("default_key event");
        assert_eq!(default_key["content"]["key"], serde_json::json!("abc"));
    }

    #[test]
    fn account_data_array_passthrough() {
        let events_in = serde_json::json!([{ "type": "t", "content": {} }]);
        let events = account_data_map_to_events(events_in.clone());
        assert_eq!(serde_json::Value::Array(events), events_in);
    }

    #[test]
    fn account_data_empty_map_becomes_empty_array() {
        assert!(account_data_map_to_events(serde_json::json!({})).is_empty());
    }

    #[test]
    fn compute_left_shared_users_empty_both() {
        let left = SlidingSyncService::compute_left_shared_users(&[], &[]);
        assert!(left.is_empty());
    }

    #[test]
    fn compute_left_shared_users_no_change() {
        let prev = vec!["alice".into(), "bob".into()];
        let curr = vec!["alice".into(), "bob".into()];
        let left = SlidingSyncService::compute_left_shared_users(&prev, &curr);
        assert!(left.is_empty());
    }

    #[test]
    fn compute_left_shared_users_user_left() {
        let prev = vec!["alice".into(), "bob".into(), "carol".into()];
        let curr = vec!["alice".into(), "carol".into()];
        let left = SlidingSyncService::compute_left_shared_users(&prev, &curr);
        assert_eq!(left, vec!["bob"]);
    }

    #[test]
    fn compute_left_shared_users_new_user_joined() {
        let prev = vec!["alice".into()];
        let curr = vec!["alice".into(), "bob".into()];
        let left = SlidingSyncService::compute_left_shared_users(&prev, &curr);
        assert!(left.is_empty());
    }

    #[test]
    fn compute_left_shared_users_multiple_left() {
        let prev = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        let curr = vec!["a".into(), "d".into()];
        let left = SlidingSyncService::compute_left_shared_users(&prev, &curr);
        let mut left = left;
        left.sort();
        assert_eq!(left, vec!["b", "c"]);
    }

    #[test]
    fn e2ee_device_list_stream_cache_key_with_conn_id() {
        let key = SlidingSyncService::e2ee_device_list_stream_cache_key("alice", "DEV1", Some("conn123"));
        assert_eq!(key, "sliding_sync:e2ee:alice:DEV1:conn123");
    }

    #[test]
    fn e2ee_device_list_stream_cache_key_without_conn_id() {
        let key = SlidingSyncService::e2ee_device_list_stream_cache_key("alice", "DEV1", None);
        assert_eq!(key, "sliding_sync:e2ee:alice:DEV1:");
    }

    #[test]
    fn e2ee_shared_users_cache_key_with_conn_id() {
        let key = SlidingSyncService::e2ee_shared_users_cache_key("bob", "DEV2", Some("conn456"));
        assert_eq!(key, "sliding_sync:e2ee:shared_users:bob:DEV2:conn456");
    }

    #[test]
    fn e2ee_shared_users_cache_key_without_conn_id() {
        let key = SlidingSyncService::e2ee_shared_users_cache_key("bob", "DEV2", None);
        assert_eq!(key, "sliding_sync:e2ee:shared_users:bob:DEV2:");
    }

    fn presence_snapshot(
        user: &str,
        presence: &str,
        status_msg: Option<&str>,
        last_active_ts: Option<i64>,
    ) -> synapse_storage::presence::PresenceSnapshot {
        synapse_storage::presence::PresenceSnapshot {
            user_id: user.to_string(),
            presence: presence.to_string(),
            status_msg: status_msg.map(|s| s.to_string()),
            last_active_ts,
        }
    }

    #[test]
    fn ss07_presence_events_compute_real_last_active_ago() {
        let mut snapshots = std::collections::HashMap::new();
        snapshots.insert("@a:x".to_string(), presence_snapshot("@a:x", "online", Some("hi"), Some(1_000_000)));
        let (wire, _) = SlidingSyncService::build_presence_events(&snapshots, 1_060_000);
        assert_eq!(wire.len(), 1);
        let content = &wire[0]["content"];
        assert_eq!(content["presence"], serde_json::json!("online"));
        assert_eq!(content["status_msg"], serde_json::json!("hi"));
        assert_eq!(content["last_active_ago"], serde_json::json!(60_000));
    }

    #[test]
    fn ss07_presence_wire_event_has_content_user_id() {
        // 契约对齐：Matrix m.presence 事件必须在 content 携带 user_id，
        // SDK 的 presence 处理（processPresenceEvents / ExtensionPresence）
        // 依赖 content.user_id 提取归属用户，而非 sender 字段。
        let mut snapshots = std::collections::HashMap::new();
        snapshots.insert("@a:x".to_string(), presence_snapshot("@a:x", "online", None, Some(1_000_000)));
        let (wire, _) = SlidingSyncService::build_presence_events(&snapshots, 2_000_000);
        assert_eq!(wire[0]["content"]["user_id"], serde_json::json!("@a:x"));
    }

    #[test]
    fn ss07_offline_presence_has_null_last_active_ago() {
        let mut snapshots = std::collections::HashMap::new();
        snapshots.insert("@a:x".to_string(), presence_snapshot("@a:x", "offline", None, Some(1_000_000)));
        let (wire, _) = SlidingSyncService::build_presence_events(&snapshots, 2_000_000);
        assert_eq!(wire[0]["content"]["last_active_ago"], serde_json::Value::Null);
    }

    #[test]
    fn ss07_missing_last_active_ts_has_null_last_active_ago() {
        let mut snapshots = std::collections::HashMap::new();
        snapshots.insert("@a:x".to_string(), presence_snapshot("@a:x", "online", None, None));
        let (wire, _) = SlidingSyncService::build_presence_events(&snapshots, 2_000_000);
        assert_eq!(wire[0]["content"]["last_active_ago"], serde_json::Value::Null);
    }

    #[test]
    fn ss07_future_last_active_ts_clamps_to_zero() {
        let mut snapshots = std::collections::HashMap::new();
        snapshots.insert("@a:x".to_string(), presence_snapshot("@a:x", "online", None, Some(5_000_000)));
        let (wire, _) = SlidingSyncService::build_presence_events(&snapshots, 2_000_000);
        assert_eq!(wire[0]["content"]["last_active_ago"], serde_json::json!(0));
    }

    #[test]
    fn ss07_canonical_payload_is_time_independent() {
        // 去重比较的载荷不得随 now 变化，否则增量 sync 每次都判定 changed，
        // presence 回声复发（S7 回归）。
        let mut snapshots = std::collections::HashMap::new();
        snapshots.insert("@a:x".to_string(), presence_snapshot("@a:x", "online", Some("hi"), Some(1_000_000)));
        let (_, canonical_t1) = SlidingSyncService::build_presence_events(&snapshots, 1_060_000);
        let (_, canonical_t2) = SlidingSyncService::build_presence_events(&snapshots, 9_999_000);
        assert_eq!(serde_json::to_string(&canonical_t1).unwrap(), serde_json::to_string(&canonical_t2).unwrap());
    }

    #[test]
    fn ss07_wire_and_canonical_events_sorted_by_sender() {
        let mut snapshots = std::collections::HashMap::new();
        for u in ["@c:x", "@a:x", "@b:x"] {
            snapshots.insert(u.to_string(), presence_snapshot(u, "online", None, Some(1_000)));
        }
        let (wire, canonical) = SlidingSyncService::build_presence_events(&snapshots, 2_000);
        let senders: Vec<&str> = wire.iter().map(|e| e["sender"].as_str().unwrap()).collect();
        assert_eq!(senders, vec!["@a:x", "@b:x", "@c:x"]);
        let canonical_senders: Vec<&str> = canonical.iter().map(|e| e["sender"].as_str().unwrap()).collect();
        assert_eq!(canonical_senders, vec!["@a:x", "@b:x", "@c:x"]);
    }
}
