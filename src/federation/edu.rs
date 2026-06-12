//! Unified EDU (Ephemeral Data Unit) dispatch for inbound federation transactions.
//!
//! Historically, inbound EDU processing was ad-hoc: only `m.presence` had a
//! dedicated handler in the transaction endpoint, while `m.typing` and
//! `m.device_list_update` were silently dropped. This module introduces a
//! typed `EduType` enum and an `EduDispatcher` that routes each EDU to the
//! correct handler uniformly.

use crate::web::routes::AppState;
use serde_json::Value;
use std::str::FromStr;

// ---------------------------------------------------------------------------
// EduType — discriminant for the three Matrix federation EDU types we handle
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EduType {
    Typing,
    Presence,
    DeviceListUpdate,
}

#[derive(Debug, Clone)]
pub struct UnknownEduType(String);

impl std::fmt::Display for UnknownEduType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown EDU type: {}", self.0)
    }
}

impl std::error::Error for UnknownEduType {}

impl FromStr for EduType {
    type Err = UnknownEduType;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "m.typing" => Ok(Self::Typing),
            "m.presence" => Ok(Self::Presence),
            "m.device_list_update" => Ok(Self::DeviceListUpdate),
            other => Err(UnknownEduType(other.to_string())),
        }
    }
}

// ---------------------------------------------------------------------------
// EduProcessResult — result of processing a batch of EDU updates
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct EduProcessResult {
    pub processed: usize,
    pub dropped: usize,
    pub errored: usize,
}

impl EduProcessResult {
    pub fn is_empty(&self) -> bool {
        self.processed == 0 && self.dropped == 0 && self.errored == 0
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Check that a Matrix user_id belongs to the given origin server.
fn user_matches_origin(user_id: &str, origin: &str) -> bool {
    user_id.rsplit_once(':').is_some_and(|(_, server_name)| server_name == origin)
}

fn increment_counter(state: &AppState, name: &str) {
    if let Some(counter) = state.services.metrics.get_counter(name) {
        counter.inc();
    } else {
        state.services.metrics.register_counter(name.to_string()).inc();
    }
}

fn increment_counter_by(state: &AppState, name: &str, delta: u64) {
    if let Some(counter) = state.services.metrics.get_counter(name) {
        counter.inc_by(delta);
    } else {
        state.services.metrics.register_counter(name.to_string()).inc_by(delta);
    }
}

async fn set_presence_backoff(state: &AppState, origin: &str) {
    let until =
        chrono::Utc::now().timestamp_millis() + state.services.config.federation.inbound_presence_backoff_ms as i64;
    let mut guard = state.federation_presence_backoff_until.write().await;
    guard.insert(origin.to_string(), until);
}

// ---------------------------------------------------------------------------
// Per-type processing functions
// ---------------------------------------------------------------------------

async fn handle_presence_edu(state: &AppState, origin: &str, edu: &Value, remaining: usize) -> EduProcessResult {
    let Some(push) = edu.get("content").and_then(|c| c.get("push")).and_then(|v| v.as_array()) else {
        increment_counter(state, "federation_inbound_presence_dropped_total");
        return EduProcessResult::default();
    };

    let mut result = EduProcessResult::default();

    for update in push.iter().take(remaining) {
        let Some(user_id) = update.get("user_id").and_then(|v| v.as_str()) else {
            result.dropped += 1;
            continue;
        };

        if !user_matches_origin(user_id, origin) {
            result.dropped += 1;
            continue;
        }

        let presence_str = update.get("presence").and_then(|v| v.as_str()).unwrap_or("online");
        let presence =
            crate::common::PresenceState::from_str_opt(presence_str).unwrap_or(crate::common::PresenceState::Online);
        let status_msg = update.get("status_msg").and_then(|v| v.as_str());

        let exists = match state.services.account.user_storage.user_exists(user_id).await {
            Ok(exists) => exists,
            Err(error) => {
                ::tracing::warn!("Failed to validate presence user {} from {}: {}", user_id, origin, error);
                result.errored += 1;
                set_presence_backoff(state, origin).await;
                break;
            }
        };

        if !exists {
            result.dropped += 1;
            continue;
        }

        if let Err(error) =
            state.services.account.presence_storage.set_presence(user_id, presence.as_str(), status_msg).await
        {
            ::tracing::warn!("Failed to persist presence update for {} from {}: {}", user_id, origin, error);
            result.errored += 1;
            set_presence_backoff(state, origin).await;
            break;
        }

        result.processed += 1;
    }

    if result.processed > 0 {
        increment_counter_by(state, "federation_inbound_presence_processed_total", result.processed as u64);
    }
    if result.dropped > 0 {
        increment_counter_by(state, "federation_inbound_presence_dropped_total", result.dropped as u64);
    }
    if result.errored > 0 {
        increment_counter_by(state, "federation_inbound_presence_error_total", result.errored as u64);
    }

    result
}

async fn handle_typing_edu(state: &AppState, origin: &str, edu: &Value, _remaining: usize) -> EduProcessResult {
    let room_id = match edu.get("room_id").and_then(|v| v.as_str()) {
        Some(r) => r,
        None => {
            ::tracing::debug!("Dropping m.typing EDU from {} without room_id", origin);
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    let user_ids = edu
        .get("content")
        .and_then(|c| c.get("user_ids"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .filter(|uid| user_matches_origin(uid, origin))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if user_ids.is_empty() {
        ::tracing::debug!("No valid user_ids in m.typing EDU from {} for room {}", origin, room_id);
        return EduProcessResult { dropped: 1, ..Default::default() };
    }

    let mut result = EduProcessResult::default();
    for user_id in &user_ids {
        match state.services.account.presence_storage.set_typing(room_id, user_id, true).await {
            Ok(()) => result.processed += 1,
            Err(e) => {
                ::tracing::warn!("Failed to persist typing EDU for {} in {} from {}: {}", user_id, room_id, origin, e);
                result.errored += 1;
            }
        }
    }

    if result.processed > 0 {
        increment_counter_by(state, "federation_inbound_typing_processed_total", result.processed as u64);
    }

    result
}

async fn handle_device_list_update_edu(
    state: &AppState,
    origin: &str,
    edu: &Value,
    _remaining: usize,
) -> EduProcessResult {
    let content = match edu.get("content") {
        Some(c) => c,
        None => {
            ::tracing::debug!("Dropping m.device_list_update EDU from {} without content", origin);
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    let user_id = match content.get("user_id").and_then(|v| v.as_str()) {
        Some(uid) => uid,
        None => {
            ::tracing::debug!("Dropping m.device_list_update EDU from {} without user_id", origin);
            return EduProcessResult { dropped: 1, ..Default::default() };
        }
    };

    if !user_matches_origin(user_id, origin) {
        ::tracing::debug!("Dropping m.device_list_update EDU: user_id {} does not match origin {}", user_id, origin);
        return EduProcessResult { dropped: 1, ..Default::default() };
    }

    let device_id = content.get("device_id").and_then(|v| v.as_str());

    // Record the change in the device_lists_changes stream so that local
    // clients can pick it up via /keys/changes or /sync.
    let stream_id =
        content.get("stream_id").and_then(|v| v.as_i64()).unwrap_or_else(|| chrono::Utc::now().timestamp_millis());

    let change_type =
        if content.get("deleted").and_then(|v| v.as_bool()).unwrap_or(false) { "deleted" } else { "updated" };

    let pool = &*state.services.device_storage.pool;
    let result = sqlx::query!(
        r#"
        INSERT INTO device_lists_changes (user_id, device_id, change_type, stream_id, created_ts)
        VALUES ($1, $2, $3, $4, $4)
        ON CONFLICT DO NOTHING
        "#,
        user_id,
        device_id,
        change_type,
        stream_id
    )
    .execute(pool)
    .await;

    match result {
        Ok(_) => {
            ::tracing::debug!(
                "Processed m.device_list_update EDU for user {} device {:?} from {}",
                user_id,
                device_id,
                origin
            );
            increment_counter(state, "federation_inbound_device_list_update_processed_total");
            EduProcessResult { processed: 1, ..Default::default() }
        }
        Err(e) => {
            ::tracing::warn!("Failed to persist m.device_list_update EDU for {} from {}: {}", user_id, origin, e);
            increment_counter(state, "federation_inbound_device_list_update_error_total");
            EduProcessResult { errored: 1, ..Default::default() }
        }
    }
}

// ---------------------------------------------------------------------------
// EduDispatcher — routes inbound EDUs to the correct handler
// ---------------------------------------------------------------------------

pub struct EduDispatcher;

impl EduDispatcher {
    /// Dispatch a single inbound EDU to the matching handler.
    ///
    /// Returns `None` if no handler matches the EDU type (i.e. the EDU type
    /// is unknown or unsupported). Returns `Some(result)` otherwise.
    pub async fn dispatch(state: &AppState, origin: &str, edu: &Value, remaining: usize) -> Option<EduProcessResult> {
        let edu_type_str = edu.get("edu_type").and_then(|v| v.as_str()).unwrap_or("");
        let edu_type = EduType::from_str(edu_type_str).ok()?;

        let result = match edu_type {
            EduType::Presence => handle_presence_edu(state, origin, edu, remaining).await,
            EduType::Typing => handle_typing_edu(state, origin, edu, remaining).await,
            EduType::DeviceListUpdate => handle_device_list_update_edu(state, origin, edu, remaining).await,
        };

        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- EduType ---

    #[test]
    fn test_edu_type_from_str_valid() {
        assert_eq!("m.typing".parse::<EduType>().unwrap(), EduType::Typing);
        assert_eq!("m.presence".parse::<EduType>().unwrap(), EduType::Presence);
        assert_eq!("m.device_list_update".parse::<EduType>().unwrap(), EduType::DeviceListUpdate);
    }

    #[test]
    fn test_edu_type_from_str_invalid() {
        assert!("m.unknown".parse::<EduType>().is_err());
        assert!("".parse::<EduType>().is_err());
        assert!("random".parse::<EduType>().is_err());
    }

    #[test]
    fn test_edu_type_from_str_error_message() {
        let err = "m.typo".parse::<EduType>().unwrap_err();
        assert_eq!(err.to_string(), "unknown EDU type: m.typo");
        assert_eq!(err.0, "m.typo");
    }

    #[test]
    fn test_edu_type_copy() {
        let edu = EduType::Presence;
        let copied = edu;
        assert_eq!(edu, copied);
    }

    #[test]
    fn test_edu_type_equality() {
        assert_eq!(EduType::Typing, EduType::Typing);
        assert_ne!(EduType::Typing, EduType::Presence);
        assert_ne!(EduType::Presence, EduType::DeviceListUpdate);
    }

    #[test]
    fn test_edu_type_clone() {
        assert_eq!(EduType::Typing.clone(), EduType::Typing);
    }

    // --- UnknownEduType ---

    #[test]
    fn test_unknown_edu_type_display() {
        let err = UnknownEduType("m.custom_edu".to_string());
        assert_eq!(err.to_string(), "unknown EDU type: m.custom_edu");
    }

    #[test]
    fn test_unknown_edu_type_clone() {
        let err = UnknownEduType("test".to_string());
        assert_eq!(err.clone().0, "test");
    }

    // --- EduProcessResult ---

    #[test]
    fn test_edu_process_result_default() {
        let result = EduProcessResult::default();
        assert_eq!(result.processed, 0);
        assert_eq!(result.dropped, 0);
        assert_eq!(result.errored, 0);
    }

    #[test]
    fn test_edu_process_result_is_empty() {
        assert!(EduProcessResult::default().is_empty());
        assert!(!EduProcessResult { processed: 1, dropped: 0, errored: 0 }.is_empty());
        assert!(!EduProcessResult { processed: 0, dropped: 1, errored: 0 }.is_empty());
        assert!(!EduProcessResult { processed: 0, dropped: 0, errored: 1 }.is_empty());
    }

    #[test]
    fn test_edu_process_result_clone() {
        let result = EduProcessResult { processed: 5, dropped: 2, errored: 1 };
        let cloned = result.clone();
        assert_eq!(cloned.processed, 5);
        assert_eq!(cloned.dropped, 2);
        assert_eq!(cloned.errored, 1);
    }

    // --- user_matches_origin ---

    #[test]
    fn test_user_matches_origin_valid() {
        assert!(user_matches_origin("@alice:example.com", "example.com"));
        assert!(user_matches_origin("@bob:matrix.org", "matrix.org"));
        assert!(user_matches_origin("@user:sub.domain.com", "sub.domain.com"));
    }

    #[test]
    fn test_user_matches_origin_invalid() {
        assert!(!user_matches_origin("@alice:example.com", "other.com"));
        assert!(!user_matches_origin("@bob:example.com", "example.org"));
    }

    #[test]
    fn test_user_matches_origin_no_colon() {
        // user_id without colon should not match
        assert!(!user_matches_origin("plainuser", "example.com"));
    }

    #[test]
    fn test_user_matches_origin_empty_origin() {
        assert!(!user_matches_origin("@alice:example.com", ""));
    }
}
