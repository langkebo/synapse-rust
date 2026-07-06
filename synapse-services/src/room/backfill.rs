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

use crate::common::error::{ApiError, ApiResult};
use synapse_federation::client_api::FederationClientApi;
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
    let now = chrono::Utc::now().timestamp_millis();
    let mut map = BACKFILL_COOLDOWN.lock().await;
    if let Some(&last_ts) = map.get(room_id) {
        if now - last_ts < BACKFILL_COOLDOWN_MS {
            return false;
        }
    }
    map.insert(room_id.to_string(), now);
    true
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
            .event_storage
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

                // Skip if already present locally.
                let already_present = self.event_storage.get_event(event_id).await.ok().flatten().is_some();
                if already_present {
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
