//! Outbound federation backfill: fetch historical events from peer servers
//! when local history is insufficient.
//!
//! Reference: `element-hq/synapse` `synapse/handlers/federation.py::FederationHandler.backfill`
//! and `synapse/federation/federation_client.py::FederationClient.backfill`.
//!
//! Trigger points:
//!   - Admin endpoint `POST /_synapse/admin/v1/rooms/{room_id}/backfill`
//!     (manual / testing — always fires immediately)
//!   - `/messages` backward pagination when local results are insufficient
//!     (best-effort, async — rate-limited via a per-room cooldown)
//!
//! Candidate-server selection: servers with a currently-joined member are
//! guaranteed (by Matrix federation invariants) to hold the room's history.
//! We iterate them in the order returned by the database; the first server
//! that returns a non-empty PDU batch wins.  This is simpler than Synapse's
//! depth-absolute-distance ranking but sufficient for the common case where
//! the room has a small number of federated peers.
//!
//! Rate limiting: the `/messages` best-effort trigger uses a per-room
//! cooldown (default 60 s) to avoid hammering peer servers when a client
//! retries backward pagination rapidly.  The admin endpoint bypasses the
//! cooldown for manual/testing use.

use std::collections::HashMap;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;

use crate::common::error::{ApiError, ApiResult};
use synapse_federation::client_api::FederationClientApi;
use synapse_federation::signing::{check_pdu_size_limits, verify_event_content_hash, verify_pdu_signature_with_client};
use synapse_storage::CreateEventParams;

use super::service::RoomService;

/// Default number of events to request per `/backfill` call.
const DEFAULT_BACKFILL_LIMIT: u32 = 100;

/// Per-room cooldown for the `/messages` best-effort backfill trigger, in
/// milliseconds.  Prevents excessive federation requests when a client
/// retries backward pagination rapidly.
const BACKFILL_COOLDOWN_MS: i64 = 60_000;

/// Global per-room cooldown map for the `/messages` best-effort trigger.
/// Maps `room_id` → last backfill trigger timestamp (ms since epoch).
static BACKFILL_COOLDOWN: std::sync::LazyLock<tokio::sync::Mutex<HashMap<String, i64>>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(HashMap::new()));

/// Result of a single backfill attempt against one candidate server.
#[derive(Debug, Clone)]
pub struct BackfillOutcome {
    /// Server we successfully backfilled from, if any.
    pub source_server: Option<String>,
    /// Number of events persisted (after dedup against existing local events).
    pub persisted_events: usize,
    /// Number of candidate servers that were tried.
    pub candidates_tried: usize,
}

/// Checks the per-room backfill cooldown.  Returns `true` if a backfill is
/// allowed to proceed (and records the trigger timestamp), or `false` if the
/// room is still within its cooldown window.
///
/// This is used by the `/messages` best-effort trigger to avoid hammering
/// peer servers when a client retries backward pagination rapidly.  The
/// admin endpoint does **not** use this check — manual triggers always fire
/// immediately.
pub async fn check_backfill_cooldown(room_id: &str) -> bool {
    let now = current_timestamp_millis();
    let mut map = BACKFILL_COOLDOWN.lock().await;
    if let Some(&last_ts) = map.get(room_id) {
        if now - last_ts < BACKFILL_COOLDOWN_MS {
            return false;
        }
    }
    map.insert(room_id.to_string(), now);
    true
}

/// P1a 修复：批量存在性检查（替代循环内逐 PDU `get_event` 的 N+1 查询）。
///
/// 收集 PDU 中的全部有效 `event_id`，一次 `find_missing_event_ids`
/// （底层 `WHERE event_id = ANY($1)`）得出本地缺失集合。持久化循环用该
/// 集合做 O(1) 去重，并在持久化成功后从集合移除对应 ID，以保持
/// "批内重复 PDU 静默跳过"的原语义。
async fn compute_missing_event_ids(
    event_reader: &Arc<dyn synapse_storage::event::EventReader>,
    pdus: &[serde_json::Value],
) -> ApiResult<std::collections::HashSet<String>> {
    let event_ids: Vec<String> =
        pdus.iter().filter_map(|pdu| pdu.get("event_id").and_then(|v| v.as_str()).map(String::from)).collect();
    if event_ids.is_empty() {
        return Ok(std::collections::HashSet::new());
    }
    let missing = event_reader
        .find_missing_event_ids(&event_ids)
        .await
        .map_err(|e| ApiError::internal_with_log("Failed to batch-check existing events for backfill", &e))?;
    Ok(missing.into_iter().collect())
}

impl RoomService {
    /// Fetch historical events for `room_id` from federated peers and persist
    /// them locally (including DAG metadata via `create_event_with_graph`).
    ///
    /// The method is **best-effort**: errors from individual candidate servers
    /// are logged and the next candidate is tried.  Only errors that prevent
    /// any progress (e.g. no candidates, no seed events) are surfaced to the
    /// caller.
    pub async fn backfill_room_history(
        &self,
        federation_client: &Arc<dyn FederationClientApi>,
        room_id: &str,
        limit: Option<u32>,
    ) -> ApiResult<BackfillOutcome> {
        let limit = limit.unwrap_or(DEFAULT_BACKFILL_LIMIT);

        // 1. Collect candidate servers (joined members' home servers).
        let mut candidates = self
            .member_storage
            .get_joined_servers_in_room(room_id, &self.server_name)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to load joined servers for backfill", &e))?;

        if candidates.is_empty() {
            ::tracing::debug!(
                room_id = %room_id,
                "Backfill skipped: no federated candidates in room"
            );
            return Ok(BackfillOutcome { source_server: None, persisted_events: 0, candidates_tried: 0 });
        }

        // 2. Seed event IDs — the most recent events we already have.  The
        //    remote server walks backwards from these.
        let seed_event_ids = self
            .event_reader
            .get_latest_event_ids_in_room(room_id, 20)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to load seed event IDs for backfill", &e))?;

        if seed_event_ids.is_empty() {
            ::tracing::debug!(
                room_id = %room_id,
                "Backfill skipped: no local seed events"
            );
            return Ok(BackfillOutcome {
                source_server: None,
                persisted_events: 0,
                candidates_tried: candidates.len(),
            });
        }

        // 3. Iterate candidates.  The first server that returns a non-empty
        //    PDU batch wins; remaining candidates are not tried.
        //
        //    Synapse additionally ranks candidates by absolute depth
        //    distance, but for rooms with a small federated footprint the
        //    simpler "first that answers" strategy is adequate and avoids
        //    an extra round of `get_event` probes.
        let mut tried = 0;
        for candidate in candidates.drain(..) {
            tried += 1;
            ::tracing::debug!(
                room_id = %room_id,
                candidate = %candidate,
                seed_count = seed_event_ids.len(),
                limit = limit,
                "Requesting backfill from candidate"
            );

            let response = match federation_client.backfill(&candidate, room_id, &seed_event_ids, limit).await {
                Ok(response) => response,
                Err(error) => {
                    ::tracing::info!(
                        room_id = %room_id,
                        candidate = %candidate,
                        error = %error,
                        "Backfill candidate failed; trying next"
                    );
                    continue;
                }
            };

            if response.pdus.is_empty() {
                ::tracing::debug!(
                    room_id = %room_id,
                    candidate = %candidate,
                    "Backfill candidate returned no PDUs"
                );
                continue;
            }

            // 4. Persist each PDU.  Skip events we already have locally —
            //    `create_event_with_graph` will fail on the unique event_id
            //    constraint, so we check first to avoid noisy error logs.
            //
            //    P1a 修复：一次性批量查询缺失集合（此前循环内逐 PDU 一次
            //    `get_event`，100 个 PDU = 100 次串行 DB 往返）。
            let mut missing_ids = compute_missing_event_ids(&self.event_reader, &response.pdus).await?;
            let mut persisted = 0usize;
            for pdu in &response.pdus {
                let Some(event_id) = pdu.get("event_id").and_then(|v| v.as_str()) else {
                    ::tracing::warn!(
                        room_id = %room_id,
                        candidate = %candidate,
                        "Backfill PDU missing event_id; skipping"
                    );
                    continue;
                };

                // Skip if already present locally (O(1) 内存判断；持久化成功后
                // 从集合移除，保持批内重复 PDU 静默跳过的原语义).
                if !missing_ids.contains(event_id) {
                    continue;
                }

                // N4: Verify PDU integrity before persisting.  Backfilled
                // events come from a remote server and must be validated the
                // same way as inbound transaction PDUs — a compromised peer
                // could otherwise inject forged events into room history.
                if let Err(e) = check_pdu_size_limits(pdu) {
                    ::tracing::warn!(
                        target: "security_audit",
                        event = "backfill_pdu_size_exceeded",
                        room_id = %room_id,
                        candidate = %candidate,
                        event_id = %event_id,
                        error = %e,
                        "Backfill PDU exceeded size limits — skipping"
                    );
                    continue;
                }

                if let Err(e) = verify_event_content_hash(pdu) {
                    ::tracing::warn!(
                        target: "security_audit",
                        event = "backfill_pdu_hash_mismatch",
                        room_id = %room_id,
                        candidate = %candidate,
                        event_id = %event_id,
                        error = %e,
                        "Backfill PDU content hash verification failed — skipping"
                    );
                    continue;
                }

                if let Err(e) = verify_pdu_signature_with_client(federation_client.as_ref(), pdu).await {
                    ::tracing::warn!(
                        target: "security_audit",
                        event = "backfill_pdu_signature_invalid",
                        room_id = %room_id,
                        candidate = %candidate,
                        event_id = %event_id,
                        error = %e,
                        "Backfill PDU sender signature verification failed — skipping"
                    );
                    continue;
                }

                let pdu_room_id = pdu.get("room_id").and_then(|v| v.as_str()).unwrap_or(room_id);
                let pdu_user_id = pdu.get("sender").and_then(|v| v.as_str()).unwrap_or("");
                let pdu_event_type = pdu.get("type").and_then(|v| v.as_str()).unwrap_or("m.room.message");
                let pdu_content = pdu.get("content").cloned().unwrap_or(serde_json::json!({}));
                let pdu_state_key = pdu.get("state_key").and_then(|v| v.as_str()).map(String::from);
                let pdu_ost = pdu.get("origin_server_ts").and_then(|v| v.as_i64()).unwrap_or(0);
                let pdu_prev: Vec<String> = pdu
                    .get("prev_events")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let pdu_auth: Vec<String> = pdu
                    .get("auth_events")
                    .and_then(|v| v.as_array())
                    .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();
                let pdu_depth = pdu.get("depth").and_then(|v| v.as_i64()).unwrap_or(0);
                let pdu_redacts = pdu.get("redacts").and_then(|v| v.as_str()).map(String::from);

                let params = CreateEventParams {
                    event_id: event_id.to_string(),
                    room_id: pdu_room_id.to_string(),
                    user_id: pdu_user_id.to_string(),
                    event_type: pdu_event_type.to_string(),
                    content: pdu_content,
                    state_key: pdu_state_key,
                    origin_server_ts: pdu_ost,
                    redacts: pdu_redacts,
                };

                if let Err(error) =
                    self.messaging.create_event_with_graph(params, &pdu_prev, &pdu_auth, pdu_depth, None).await
                {
                    ::tracing::warn!(
                        room_id = %room_id,
                        candidate = %candidate,
                        event_id = %event_id,
                        error = %error,
                        "Failed to persist backfilled event"
                    );
                    continue;
                }
                // 持久化成功：从缺失集合移除，批内重复 PDU 将静默跳过。
                missing_ids.remove(event_id);
                persisted += 1;
            }

            ::tracing::info!(
                room_id = %room_id,
                source_server = %candidate,
                received_pdus = response.pdus.len(),
                persisted_events = persisted,
                "Backfill completed from candidate"
            );

            return Ok(BackfillOutcome {
                source_server: Some(candidate),
                persisted_events: persisted,
                candidates_tried: tried,
            });
        }

        ::tracing::info!(
            room_id = %room_id,
            candidates_tried = tried,
            "Backfill exhausted all candidates without receiving PDUs"
        );
        Ok(BackfillOutcome { source_server: None, persisted_events: 0, candidates_tried: tried })
    }
}

#[cfg(test)]
mod tests {
    //! P1a 修复的 TDD 测试：backfill 去重从"逐 PDU 一次 `get_event`"（N+1）
    //! 改为一次 `find_missing_event_ids` 批量查询。

    use super::*;
    use synapse_storage::event::{EventReader, RoomEvent};
    use synapse_storage::test_mocks::InMemoryEventStore;

    fn seeded_event(event_id: &str) -> RoomEvent {
        RoomEvent {
            event_id: event_id.to_string(),
            room_id: "!room:test.server".to_string(),
            user_id: "@alice:test.server".to_string(),
            event_type: "m.room.message".to_string(),
            content: serde_json::json!({}),
            state_key: None,
            depth: 1,
            origin_server_ts: 1_700_000_000_000,
            processed_ts: 1_700_000_000_000,
            not_before: 0,
            status: None,
            reference_image: None,
            origin: "test.server".to_string(),
            stream_ordering: None,
            redacts: None,
        }
    }

    #[tokio::test]
    async fn missing_event_ids_computed_in_one_batch() {
        let store = InMemoryEventStore::new();
        store.seed_events(vec![seeded_event("$existing1"), seeded_event("$existing2")]).await;
        let reader: Arc<dyn EventReader> = Arc::new(store);

        let pdus = vec![
            serde_json::json!({"event_id": "$existing1"}), // 已存在 → 不在缺失集
            serde_json::json!({"event_id": "$new1"}),      // 缺失
            serde_json::json!({"event_id": "$new2"}),      // 缺失
            serde_json::json!({"no_event_id": true}),      // 无 event_id → 不进入缺失集（循环内单独跳过）
            serde_json::json!({"event_id": "$new1"}),      // 批内重复 → 仍在缺失集（持久化后由循环移除）
        ];

        let missing = compute_missing_event_ids(&reader, &pdus).await.expect("batch query must succeed");

        assert!(missing.contains("$new1"));
        assert!(missing.contains("$new2"));
        assert!(!missing.contains("$existing1"));
        assert!(!missing.contains("$existing2"));
        assert_eq!(missing.len(), 2, "缺失集应恰好包含两个新事件（批内重复只计一次）");
    }

    #[tokio::test]
    async fn missing_event_ids_empty_when_all_present() {
        let store = InMemoryEventStore::new();
        store.seed_events(vec![seeded_event("$a"), seeded_event("$b")]).await;
        let reader: Arc<dyn EventReader> = Arc::new(store);

        let pdus = vec![serde_json::json!({"event_id": "$a"}), serde_json::json!({"event_id": "$b"})];
        let missing = compute_missing_event_ids(&reader, &pdus).await.unwrap();
        assert!(missing.is_empty());
    }

    #[tokio::test]
    async fn missing_event_ids_empty_input_short_circuits() {
        let reader: Arc<dyn EventReader> = Arc::new(InMemoryEventStore::new());
        let missing = compute_missing_event_ids(&reader, &[]).await.unwrap();
        assert!(missing.is_empty());
    }
}
