use reqwest::StatusCode;
use serde_json::json;
use synapse_common::ApiError;
use synapse_storage::application_service::*;
use tracing::{error, info, warn};

use crate::application_service::scheduler::{
    SCHEDULER_STATE_BACKLOG_STATE, SCHEDULER_STATE_LAST_DISPATCHED_EVENTS, SCHEDULER_STATE_LAST_ELAPSED_MS,
    SCHEDULER_STATE_LAST_RESULT, SCHEDULER_STATE_LAST_TICK_TS, SCHEDULER_STATE_PENDING_EVENT_COUNT,
    SCHEDULER_STATE_PENDING_TRANSACTION_COUNT, SCHEDULER_STATE_TOTAL_BACKOFF_COUNT,
    SCHEDULER_STATE_TOTAL_CAPACITY_LIMITED_COUNT, SCHEDULER_STATE_TOTAL_FAILURE_COUNT,
    SCHEDULER_STATE_TOTAL_IN_FLIGHT_COUNT, SCHEDULER_STATE_TOTAL_SUCCESS_COUNT, SCHEDULER_STATE_TRANSACTION_STATE,
};
use crate::application_service::ApplicationServiceManager;

pub(super) const APPSERVICE_RETRY_BACKOFF_BASE_MS: i64 = 5_000;
pub(super) const APPSERVICE_RETRY_BACKOFF_MAX_MS: i64 = 5 * 60 * 1_000;
pub(super) const APPSERVICE_FATAL_FAILURE_THRESHOLD: i32 = 3;
pub(super) const APPSERVICE_RETRYABLE_FAILURE_THRESHOLD: i32 = 8;
pub(super) const APPSERVICE_STATE_DELIVERY_STATUS: &str = "delivery_status";
pub(super) const APPSERVICE_STATE_DELIVERY_LAST_ERROR: &str = "delivery_last_error";
pub(super) const APPSERVICE_STATE_DELIVERY_LAST_FAILURE_KIND: &str = "delivery_last_failure_kind";
pub(super) const APPSERVICE_STATE_DELIVERY_LAST_FAILURE_TS: &str = "delivery_last_failure_ts";
pub(super) const APPSERVICE_STATE_DELIVERY_DISABLED_REASON: &str = "delivery_disabled_reason";
pub(super) const APPSERVICE_SCHEDULER_STATE_KEYS: [&str; 13] = [
    SCHEDULER_STATE_LAST_TICK_TS,
    SCHEDULER_STATE_LAST_RESULT,
    SCHEDULER_STATE_PENDING_EVENT_COUNT,
    SCHEDULER_STATE_PENDING_TRANSACTION_COUNT,
    SCHEDULER_STATE_BACKLOG_STATE,
    SCHEDULER_STATE_TRANSACTION_STATE,
    SCHEDULER_STATE_LAST_DISPATCHED_EVENTS,
    SCHEDULER_STATE_LAST_ELAPSED_MS,
    SCHEDULER_STATE_TOTAL_SUCCESS_COUNT,
    SCHEDULER_STATE_TOTAL_FAILURE_COUNT,
    SCHEDULER_STATE_TOTAL_BACKOFF_COUNT,
    SCHEDULER_STATE_TOTAL_CAPACITY_LIMITED_COUNT,
    SCHEDULER_STATE_TOTAL_IN_FLIGHT_COUNT,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TransactionFailureKind {
    Retryable,
    Fatal,
}

impl TransactionFailureKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Retryable => "retryable",
            Self::Fatal => "fatal",
        }
    }
}

impl ApplicationServiceManager {
    pub async fn send_transaction(&self, as_id: &str, events: Vec<serde_json::Value>) -> Result<(), ApiError> {
        let service = self
            .storage
            .get_by_id(as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get service", &e))?
            .ok_or_else(|| ApiError::not_found("Application service not found"))?;

        let transaction_id = format!("{}", uuid::Uuid::new_v4());

        let _transaction = self
            .storage
            .create_transaction(as_id, &transaction_id, &events)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create transaction", &e))?;

        self.deliver_transaction(&service, &transaction_id, &events).await
    }

    pub async fn process_pending_for_service(&self, as_id: &str, batch_limit: i64) -> Result<usize, ApiError> {
        let service = self
            .storage
            .get_by_id(as_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get application service", &e))?
            .ok_or_else(|| ApiError::not_found("Application service not found"))?;

        let pending_transactions =
            self.storage.get_pending_transactions(as_id).await.map_err(|e| {
                ApiError::internal_with_log("Failed to get pending application service transactions", &e)
            })?;
        if let Some(transaction) = pending_transactions.first() {
            let now = chrono::Utc::now().timestamp_millis();
            if !Self::is_transaction_ready_to_retry(transaction, now) {
                return Ok(0);
            }

            let events: Vec<serde_json::Value> = serde_json::from_value(transaction.events.clone()).map_err(|e| {
                ApiError::internal_with_log("Failed to decode pending application service transaction", &e)
            })?;
            let txn_id = transaction.txn_id.as_str();
            self.deliver_transaction(&service, txn_id, &events).await?;
            return Ok(0);
        }

        let pending_events = self
            .storage
            .get_pending_events(as_id, batch_limit)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get pending application service events", &e))?;
        if pending_events.is_empty() {
            return Ok(0);
        }

        let events = self.build_transaction_events(&pending_events).await?;
        let transaction_id = uuid::Uuid::new_v4().to_string();
        self.storage
            .create_transaction(as_id, &transaction_id, &events)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create application service transaction", &e))?;
        self.deliver_transaction(&service, &transaction_id, &events).await?;

        Ok(pending_events.len())
    }

    pub async fn process_pending_queues(&self, batch_limit: i64) -> Result<usize, ApiError> {
        let services = self.get_all_active().await?;
        let mut dispatched = 0_usize;

        for service in services {
            dispatched += self.process_pending_for_service(&service.as_id, batch_limit).await?;
        }

        Ok(dispatched)
    }

    async fn deliver_transaction(
        &self,
        service: &ApplicationService,
        transaction_id: &str,
        events: &[serde_json::Value],
    ) -> Result<(), ApiError> {
        let url = format!("{}/transactions/{}", service.url, transaction_id);

        let response = self
            .http_client
            .put(&url)
            .header("Authorization", format!("Bearer {}", service.hs_token))
            .json(&json!({
                "events": events
            }))
            .send()
            .await;

        match response {
            Ok(resp) if resp.status().is_success() => {
                if let Err(e) = self.storage.complete_transaction(&service.as_id, transaction_id).await {
                    error!(%e, as_id = %service.as_id, transaction_id, "Failed to complete transaction");
                }
                self.record_delivery_success(&service.as_id).await;

                for event in events {
                    if let Some(event_id) = event
                        .get("queue_event_id")
                        .and_then(|value| value.as_str())
                        .or_else(|| event.get("event_id").and_then(|value| value.as_str()))
                    {
                        if let Err(e) = self.storage.mark_event_processed(event_id).await {
                            warn!(%e, as_id = %service.as_id, transaction_id, event_id, "Failed to mark event processed");
                        }
                    }
                }

                info!(as_id = %service.as_id, transaction_id, "Transaction sent successfully");
                Ok(())
            }
            Ok(resp) => {
                let status = resp.status();
                let error_body = resp.text().await.unwrap_or_default();
                let failure_kind = Self::classify_http_failure(status);
                let failure_reason = format!("HTTP {status}: {error_body}");
                self.handle_transaction_failure(service, transaction_id, &failure_reason, failure_kind).await;

                Err(ApiError::internal_with_log("Application service returned error", &format!("HTTP {status}")))
            }
            Err(e) => {
                self.handle_transaction_failure(
                    service,
                    transaction_id,
                    &e.to_string(),
                    TransactionFailureKind::Retryable,
                )
                .await;

                Err(ApiError::internal_with_log("Failed to send transaction", &e))
            }
        }
    }

    async fn build_transaction_events(
        &self,
        pending_events: &[ApplicationServiceEvent],
    ) -> Result<Vec<serde_json::Value>, ApiError> {
        let source_event_ids: Vec<String> =
            pending_events.iter().map(|pe| Self::source_event_id(&pe.event_id)).collect();

        let source_events = self.event_reader.get_events_map(&source_event_ids).await.map_err(|e| {
            ApiError::internal_with_log("Failed to load source room events for application service", &e)
        })?;

        let mut events = Vec::with_capacity(pending_events.len());
        for (pending_event, source_event_id) in pending_events.iter().zip(source_event_ids.iter()) {
            if let Some(source_event) = source_events.get(source_event_id) {
                events.push(json!({
                    "event_id": source_event.event_id,
                    "queue_event_id": pending_event.event_id,
                    "room_id": source_event.room_id,
                    "type": source_event.event_type,
                    "sender": source_event.user_id,
                    "content": source_event.content,
                    "state_key": source_event.state_key,
                    "origin_server_ts": source_event.origin_server_ts,
                }));
                continue;
            }

            warn!(
                queue_event_id = %pending_event.event_id,
                source_event_id = %source_event_id,
                "Falling back to minimal application service event payload because source room event was not found"
            );

            events.push(json!({
                "event_id": source_event_id,
                "queue_event_id": pending_event.event_id,
                "room_id": pending_event.room_id,
                "type": pending_event.event_type,
                "sender": pending_event.sender,
                "content": pending_event.content,
                "state_key": pending_event.state_key,
                "origin_server_ts": pending_event.origin_server_ts,
            }));
        }

        Ok(events)
    }

    #[allow(dead_code)]
    async fn build_transaction_event(
        &self,
        pending_event: &ApplicationServiceEvent,
    ) -> Result<serde_json::Value, ApiError> {
        let source_event_id = Self::source_event_id(&pending_event.event_id);
        let source_event =
            self.event_reader.get_event(&source_event_id).await.map_err(|e| {
                ApiError::internal_with_log("Failed to load source room event for application service", &e)
            })?;

        if let Some(source_event) = source_event {
            return Ok(json!({
                "event_id": source_event.event_id,
                "queue_event_id": pending_event.event_id,
                "room_id": source_event.room_id,
                "type": source_event.event_type,
                "sender": source_event.user_id,
                "content": source_event.content,
                "state_key": source_event.state_key,
                "origin_server_ts": source_event.origin_server_ts,
            }));
        }

        warn!(
            queue_event_id = %pending_event.event_id,
            source_event_id = %source_event_id,
            "Falling back to minimal application service event payload because source room event was not found"
        );

        Ok(json!({
            "event_id": source_event_id,
            "queue_event_id": pending_event.event_id,
            "room_id": pending_event.room_id,
            "type": pending_event.event_type,
            "sender": pending_event.sender,
            "content": pending_event.content,
            "state_key": pending_event.state_key,
            "origin_server_ts": pending_event.origin_server_ts,
        }))
    }

    pub(super) fn source_event_id(queue_event_id: &str) -> String {
        queue_event_id
            .rsplit_once("::")
            .map_or_else(|| queue_event_id.to_owned(), |(source_event_id, _)| source_event_id.to_owned())
    }

    async fn handle_transaction_failure(
        &self,
        service: &ApplicationService,
        transaction_id: &str,
        failure_reason: &str,
        failure_kind: TransactionFailureKind,
    ) {
        let failed_transaction =
            match self.storage.fail_transaction(&service.as_id, transaction_id, failure_reason).await {
                Ok(transaction) => transaction,
                Err(e) => {
                    error!(%e, as_id = %service.as_id, transaction_id, "Failed to fail transaction");
                    return;
                }
            };

        self.record_delivery_failure(
            &service.as_id,
            failure_reason,
            failure_kind,
            failed_transaction.sent_ts.unwrap_or(0),
        )
        .await;

        if Self::should_disable_service(failure_kind, failed_transaction.retry_count) {
            self.disable_service_for_delivery_failure(service, &failed_transaction, failure_reason, failure_kind).await;
        }
    }

    async fn record_delivery_success(&self, as_id: &str) {
        self.set_delivery_state(as_id, APPSERVICE_STATE_DELIVERY_STATUS, "up").await;
        self.set_delivery_state(as_id, APPSERVICE_STATE_DELIVERY_LAST_ERROR, "").await;
        self.set_delivery_state(as_id, APPSERVICE_STATE_DELIVERY_DISABLED_REASON, "").await;
    }

    async fn record_delivery_failure(
        &self,
        as_id: &str,
        failure_reason: &str,
        failure_kind: TransactionFailureKind,
        failed_ts: i64,
    ) {
        self.set_delivery_state(as_id, APPSERVICE_STATE_DELIVERY_STATUS, "failing").await;
        self.set_delivery_state(as_id, APPSERVICE_STATE_DELIVERY_LAST_ERROR, failure_reason).await;
        self.set_delivery_state(as_id, APPSERVICE_STATE_DELIVERY_LAST_FAILURE_KIND, failure_kind.as_str()).await;
        self.set_delivery_state(as_id, APPSERVICE_STATE_DELIVERY_LAST_FAILURE_TS, &failed_ts.to_string()).await;
    }

    async fn disable_service_for_delivery_failure(
        &self,
        service: &ApplicationService,
        failed_transaction: &ApplicationServiceTransaction,
        failure_reason: &str,
        failure_kind: TransactionFailureKind,
    ) {
        let disable_reason = format!(
            "{} delivery failure threshold reached after {} attempts: {}",
            failure_kind.as_str(),
            failed_transaction.retry_count,
            failure_reason
        );

        match self.storage.update(&service.as_id, &UpdateApplicationServiceRequest::new().is_enabled(false)).await {
            Ok(_) => {
                self.set_delivery_state(&service.as_id, APPSERVICE_STATE_DELIVERY_STATUS, "disabled").await;
                self.set_delivery_state(&service.as_id, APPSERVICE_STATE_DELIVERY_DISABLED_REASON, &disable_reason)
                    .await;
                warn!(
                    as_id = %service.as_id,
                    transaction_id = %failed_transaction.txn_id,
                    retry_count = failed_transaction.retry_count,
                    failure_kind = failure_kind.as_str(),
                    failure_reason = %failure_reason,
                    "Disabled application service after repeated delivery failures"
                );
            }
            Err(e) => {
                error!(
                    %e,
                    as_id = %service.as_id,
                    transaction_id = %failed_transaction.txn_id,
                    "Failed to disable application service after repeated delivery failures"
                );
            }
        }
    }

    async fn set_delivery_state(&self, as_id: &str, state_key: &str, state_value: &str) {
        if let Err(e) = self.storage.set_state(as_id, state_key, state_value).await {
            warn!(%e, as_id, state_key, "Failed to update application service delivery state");
        }
    }

    pub(super) fn is_transaction_ready_to_retry(transaction: &ApplicationServiceTransaction, now_ts: i64) -> bool {
        now_ts.saturating_sub(transaction.sent_ts.unwrap_or(0)) >= Self::retry_backoff_ms(transaction.retry_count)
    }

    pub(super) fn retry_backoff_ms(retry_count: i32) -> i64 {
        if retry_count <= 0 {
            return 0;
        }

        let exponential = 1_i64.checked_shl((retry_count - 1).min(16) as u32).unwrap_or(i64::MAX);
        APPSERVICE_RETRY_BACKOFF_BASE_MS.saturating_mul(exponential).min(APPSERVICE_RETRY_BACKOFF_MAX_MS)
    }

    pub(super) fn classify_http_failure(status: StatusCode) -> TransactionFailureKind {
        if status.is_server_error()
            || matches!(status, StatusCode::TOO_MANY_REQUESTS | StatusCode::REQUEST_TIMEOUT | StatusCode::TOO_EARLY)
        {
            TransactionFailureKind::Retryable
        } else {
            TransactionFailureKind::Fatal
        }
    }

    pub(super) fn should_disable_service(failure_kind: TransactionFailureKind, retry_count: i32) -> bool {
        match failure_kind {
            TransactionFailureKind::Fatal => retry_count >= APPSERVICE_FATAL_FAILURE_THRESHOLD,
            TransactionFailureKind::Retryable => retry_count >= APPSERVICE_RETRYABLE_FAILURE_THRESHOLD,
        }
    }

    pub(super) fn scheduler_statistics_from_states(states: &[ApplicationServiceState]) -> serde_json::Value {
        let has_scheduler_state = APPSERVICE_SCHEDULER_STATE_KEYS
            .iter()
            .any(|state_key| Self::scheduler_state_value(states, state_key).is_some());

        serde_json::json!({
            "available": has_scheduler_state,
            "last_result": Self::scheduler_state_value(states, SCHEDULER_STATE_LAST_RESULT),
            "transaction_state": Self::scheduler_state_value(states, SCHEDULER_STATE_TRANSACTION_STATE),
            "backlog_state": Self::scheduler_state_value(states, SCHEDULER_STATE_BACKLOG_STATE),
            "pending_event_count": Self::scheduler_state_i64(states, SCHEDULER_STATE_PENDING_EVENT_COUNT),
            "pending_transaction_count": Self::scheduler_state_i64(states, SCHEDULER_STATE_PENDING_TRANSACTION_COUNT),
            "last_tick_ts": Self::scheduler_state_i64(states, SCHEDULER_STATE_LAST_TICK_TS),
            "last_dispatched_events": Self::scheduler_state_i64(states, SCHEDULER_STATE_LAST_DISPATCHED_EVENTS),
            "last_elapsed_ms": Self::scheduler_state_i64(states, SCHEDULER_STATE_LAST_ELAPSED_MS),
            "total_success_count": Self::scheduler_state_i64(states, SCHEDULER_STATE_TOTAL_SUCCESS_COUNT),
            "total_failure_count": Self::scheduler_state_i64(states, SCHEDULER_STATE_TOTAL_FAILURE_COUNT),
            "total_backoff_count": Self::scheduler_state_i64(states, SCHEDULER_STATE_TOTAL_BACKOFF_COUNT),
            "total_capacity_limited_count": Self::scheduler_state_i64(states, SCHEDULER_STATE_TOTAL_CAPACITY_LIMITED_COUNT),
            "total_in_flight_count": Self::scheduler_state_i64(states, SCHEDULER_STATE_TOTAL_IN_FLIGHT_COUNT),
        })
    }

    fn scheduler_state_value<'a>(states: &'a [ApplicationServiceState], state_key: &str) -> Option<&'a str> {
        states
            .iter()
            .find(|state| state.state_key == state_key)
            .map(|state| state.state_value.trim())
            .filter(|state_value| !state_value.is_empty())
    }

    fn scheduler_state_i64(states: &[ApplicationServiceState], state_key: &str) -> Option<i64> {
        Self::scheduler_state_value(states, state_key).and_then(|state_value| state_value.parse::<i64>().ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── source_event_id ───────────────────────────────────────────────

    #[test]
    fn source_event_id_strips_queue_suffix() {
        assert_eq!(ApplicationServiceManager::source_event_id("$event123::queue456"), "$event123");
    }

    #[test]
    fn source_event_id_no_suffix_returns_whole() {
        assert_eq!(ApplicationServiceManager::source_event_id("$event123"), "$event123");
    }

    #[test]
    fn source_event_id_multiple_separators() {
        assert_eq!(ApplicationServiceManager::source_event_id("$a::b::c"), "$a::b");
    }

    // ── retry_backoff_ms ──────────────────────────────────────────────

    #[test]
    fn retry_backoff_zero_or_negative_returns_zero() {
        assert_eq!(ApplicationServiceManager::retry_backoff_ms(0), 0);
        assert_eq!(ApplicationServiceManager::retry_backoff_ms(-1), 0);
    }

    #[test]
    fn retry_backoff_first_retry() {
        assert_eq!(ApplicationServiceManager::retry_backoff_ms(1), 5_000);
    }

    #[test]
    fn retry_backoff_exponential_growth() {
        assert_eq!(ApplicationServiceManager::retry_backoff_ms(2), 10_000);
        assert_eq!(ApplicationServiceManager::retry_backoff_ms(3), 20_000);
        assert_eq!(ApplicationServiceManager::retry_backoff_ms(4), 40_000);
    }

    #[test]
    fn retry_backoff_capped_at_max() {
        // With large retry count, should cap at APPSERVICE_RETRY_BACKOFF_MAX_MS (5 min)
        let backoff = ApplicationServiceManager::retry_backoff_ms(100);
        assert_eq!(backoff, APPSERVICE_RETRY_BACKOFF_MAX_MS);
    }

    // ── classify_http_failure ─────────────────────────────────────────

    #[test]
    fn server_error_is_retryable() {
        assert_eq!(
            ApplicationServiceManager::classify_http_failure(StatusCode::INTERNAL_SERVER_ERROR),
            TransactionFailureKind::Retryable
        );
    }

    #[test]
    fn too_many_requests_is_retryable() {
        assert_eq!(
            ApplicationServiceManager::classify_http_failure(StatusCode::TOO_MANY_REQUESTS),
            TransactionFailureKind::Retryable
        );
    }

    #[test]
    fn request_timeout_is_retryable() {
        assert_eq!(
            ApplicationServiceManager::classify_http_failure(StatusCode::REQUEST_TIMEOUT),
            TransactionFailureKind::Retryable
        );
    }

    #[test]
    fn too_early_is_retryable() {
        assert_eq!(
            ApplicationServiceManager::classify_http_failure(StatusCode::TOO_EARLY),
            TransactionFailureKind::Retryable
        );
    }

    #[test]
    fn client_error_is_fatal() {
        assert_eq!(
            ApplicationServiceManager::classify_http_failure(StatusCode::BAD_REQUEST),
            TransactionFailureKind::Fatal
        );
    }

    #[test]
    fn not_found_is_fatal() {
        assert_eq!(
            ApplicationServiceManager::classify_http_failure(StatusCode::NOT_FOUND),
            TransactionFailureKind::Fatal
        );
    }

    #[test]
    fn unauthorized_is_fatal() {
        assert_eq!(
            ApplicationServiceManager::classify_http_failure(StatusCode::UNAUTHORIZED),
            TransactionFailureKind::Fatal
        );
    }

    // ── should_disable_service ────────────────────────────────────────

    #[test]
    fn fatal_below_threshold_keeps_service() {
        assert!(!ApplicationServiceManager::should_disable_service(
            TransactionFailureKind::Fatal,
            APPSERVICE_FATAL_FAILURE_THRESHOLD - 1
        ));
    }

    #[test]
    fn fatal_at_threshold_disables_service() {
        assert!(ApplicationServiceManager::should_disable_service(
            TransactionFailureKind::Fatal,
            APPSERVICE_FATAL_FAILURE_THRESHOLD
        ));
    }

    #[test]
    fn retryable_below_threshold_keeps_service() {
        assert!(!ApplicationServiceManager::should_disable_service(
            TransactionFailureKind::Retryable,
            APPSERVICE_RETRYABLE_FAILURE_THRESHOLD - 1
        ));
    }

    #[test]
    fn retryable_at_threshold_disables_service() {
        assert!(ApplicationServiceManager::should_disable_service(
            TransactionFailureKind::Retryable,
            APPSERVICE_RETRYABLE_FAILURE_THRESHOLD
        ));
    }

    // ── is_transaction_ready_to_retry ─────────────────────────────────

    fn make_transaction(sent_ts: i64, retry_count: i32) -> ApplicationServiceTransaction {
        ApplicationServiceTransaction {
            id: 1,
            as_id: "test".into(),
            txn_id: "txn1".into(),
            transaction_id: None,
            events: json!([]),
            retry_count,
            sent_ts: Some(sent_ts),
            completed_ts: None,
            last_error: None,
        }
    }

    #[test]
    fn transaction_ready_when_backoff_elapsed() {
        let txn = make_transaction(1000, 1); // first retry → 5s backoff
        assert!(ApplicationServiceManager::is_transaction_ready_to_retry(&txn, 7000));
    }

    #[test]
    fn transaction_not_ready_when_backoff_not_elapsed() {
        let txn = make_transaction(1000, 1);
        assert!(!ApplicationServiceManager::is_transaction_ready_to_retry(&txn, 3000));
    }

    #[test]
    fn transaction_ready_at_exact_backoff_boundary() {
        let txn = make_transaction(1000, 1);
        assert!(ApplicationServiceManager::is_transaction_ready_to_retry(&txn, 6000));
    }

    #[test]
    fn transaction_ready_with_no_retries() {
        let txn = make_transaction(5000, 0);
        assert!(ApplicationServiceManager::is_transaction_ready_to_retry(&txn, 5000));
    }

    // ── scheduler_statistics_from_states ──────────────────────────────

    fn make_state(state_key: &str, state_value: &str) -> ApplicationServiceState {
        ApplicationServiceState {
            as_id: "test".into(),
            state_key: state_key.into(),
            state_value: state_value.into(),
            updated_ts: 0,
        }
    }

    #[test]
    fn scheduler_statistics_no_states_shows_unavailable() {
        let stats = ApplicationServiceManager::scheduler_statistics_from_states(&[]);
        assert_eq!(stats["available"], false);
    }

    #[test]
    fn scheduler_statistics_with_states_shows_available() {
        let states = [make_state(SCHEDULER_STATE_LAST_TICK_TS, "1000")];
        let stats = ApplicationServiceManager::scheduler_statistics_from_states(&states);
        assert_eq!(stats["available"], true);
        assert_eq!(stats["last_tick_ts"], 1000);
    }

    #[test]
    fn scheduler_statistics_empty_value_is_skipped() {
        let states = [make_state(SCHEDULER_STATE_LAST_RESULT, "  ")];
        let stats = ApplicationServiceManager::scheduler_statistics_from_states(&states);
        // Whitespace-only state value is trimmed and treated as missing
        assert_eq!(stats["available"], false);
    }
}
