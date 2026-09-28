use crate::client::FederationTransaction;
use crate::client_api::FederationClientApi;
use futures::FutureExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_storage::membership::MemberStoreApi;
use tokio::sync::{mpsc, RwLock};
use tracing::{info_span, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `FederationEvent` type.
pub struct FederationEvent {
    /// The `event_id` field.
    /// The `room_id` field.
    /// The `sender` field.
    /// The `event_type` field.
    /// The `content` field.
    /// The `origin` field.
    /// The `destination` field.
    pub event_id: String,
    /// The `room_id` field.
    /// The `sender` field.
    /// The `event_type` field.
    /// The `content` field.
    /// The `origin` field.
    /// The `destination` field.
    pub room_id: String,
    /// The `sender` field.
    /// The `event_type` field.
    /// The `content` field.
    /// The `origin` field.
    /// The `destination` field.
    pub sender: String,
    /// The `event_type` field.
    /// The `content` field.
    /// The `origin` field.
    /// The `destination` field.
    pub event_type: String,
    /// The `content` field.
    /// The `origin` field.
    /// The `destination` field.
    pub content: serde_json::Value,
    /// The `origin` field.
    /// The `destination` field.
    pub origin: String,
    /// The `destination` field.
    pub destination: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `PendingTransaction` type.
pub struct PendingTransaction {
    /// The `destination` field.
    /// The `transaction` field.
    /// The `retry_count` field.
    /// The `next_retry_at` field.
    /// The `db_id` field.
    pub destination: String,
    /// The `transaction` field.
    /// The `retry_count` field.
    /// The `next_retry_at` field.
    /// The `db_id` field.
    pub transaction: FederationTransaction,
    /// The `retry_count` field.
    /// The `next_retry_at` field.
    /// The `db_id` field.
    pub retry_count: u32,
    /// The `next_retry_at` field.
    /// The `db_id` field.
    pub next_retry_at: i64,
    /// The `db_id` field.
    pub db_id: Option<i64>,
}

#[derive(Debug, Clone)]
enum OutgoingItem {
    /// The `Pdu` variant.
    /// The `Edu` variant.
    Pdu(serde_json::Value),
    /// The `Edu` variant.
    Edu(serde_json::Value),
}

#[derive(Debug, Clone)]
struct TransactionBatch {
    pdus: Vec<serde_json::Value>,
    edus: Vec<serde_json::Value>,
    origin: String,
}

#[derive(Clone)]
/// The `EventBroadcaster` type.
pub struct EventBroadcaster {
    server_name: String,
    federation_client: Option<Arc<dyn FederationClientApi>>,
    membership_storage: Option<Arc<dyn MemberStoreApi>>,
    pending_queue: Arc<RwLock<Vec<PendingTransaction>>>,
    backoff_schedule: Vec<u64>,
    pool: Option<sqlx::PgPool>,
    batch_tx: Arc<tokio::sync::Mutex<Option<BatchSender>>>,
}

type BatchSender = mpsc::Sender<(String, OutgoingItem)>;

/// Implementation of [`EventBroadcaster`] methods.
impl EventBroadcaster {
    /// See [`new`.
    pub fn new(server_name: String) -> Self {
        Self {
            server_name,
            federation_client: None,
            membership_storage: None,
            pending_queue: Arc::new(RwLock::new(Vec::new())),
            backoff_schedule: vec![1000, 5000, 15000, 30000, 60_000, 300000, 900000],
            pool: None,
            batch_tx: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// See [`with_client`.
    pub fn with_client(mut self, client: Arc<dyn FederationClientApi>) -> Self {
        self.federation_client = Some(client);
        self
    }

    /// See [`with_pool`.
    pub fn with_pool(mut self, pool: sqlx::PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    /// See [`with_membership_storage`.
    pub fn with_membership_storage(mut self, storage: Arc<dyn MemberStoreApi>) -> Self {
        self.membership_storage = Some(storage);
        self
    }

    /// See [`set_client`.
    pub fn set_client(&mut self, client: Arc<dyn FederationClientApi>) {
        self.federation_client = Some(client);
    }

    /// See [`set_membership_storage`.
    pub fn set_membership_storage(&mut self, storage: Arc<dyn MemberStoreApi>) {
        self.membership_storage = Some(storage);
    }

    /// See [`set_pool`.
    pub fn set_pool(&mut self, pool: sqlx::PgPool) {
        self.pool = Some(pool);
    }

    /// See [`start_batch_sender`.
    pub async fn start_batch_sender(&self, origin: String, batch_max_size: usize, flush_interval_ms: u64) {
        let (tx, mut rx) = mpsc::channel::<(String, OutgoingItem)>(10000);
        *self.batch_tx.lock().await = Some(tx);

        let client = self.federation_client.clone();
        let retry_queue = self.pending_queue.clone();
        let pool_opt = self.pool.clone();
        let backoff = self.backoff_schedule.clone();
        let server_name_clone = self.server_name.clone();

        tokio::spawn(async move {
            let mut batches: HashMap<String, TransactionBatch> = HashMap::new();
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(flush_interval_ms));

            loop {
                tokio::select! {
                    msg = rx.recv() => {
                        let Some((destination, item)) = msg else {
                            break;
                        };

                        if destination == server_name_clone {
                            continue;
                        }

                        let batch = batches
                            .entry(destination.clone())
                            .or_insert_with(|| TransactionBatch {
                                pdus: Vec::new(),
                                edus: Vec::new(),
                                origin: origin.clone(),
                            });

                        match item {
                            OutgoingItem::Pdu(pdu) => batch.pdus.push(pdu),
                            OutgoingItem::Edu(edu) => batch.edus.push(edu),
                        }

                        let total = batch.pdus.len() + batch.edus.len();
                        if total >= batch_max_size {
                            if let Some(client) = &client {
                                send_batch(
                                    client,
                                    &retry_queue,
                                    &pool_opt,
                                    &backoff,
                                    &batches,
                                    &destination,
                                ).await;
                                batches.remove(&destination);
                            }
                        }
                    }

                    _ = interval.tick() => {
                        if !batches.is_empty() {
                            let c = match &client {
                                Some(c) => c,
                                None => continue,
                            };

                            // P3: Send batches to all destinations concurrently
                            // via tokio::spawn. Each send_batch is an independent
                            // outbound HTTP request to a different server.
                            // We drain from the shared batches map so the next tick
                            // starts with a clean slate; spawned tasks hold their
                            // own owned copy of the batch data.
                            let destinations: Vec<(String, TransactionBatch)> =
                                std::mem::take(&mut batches).into_iter().collect();

                            for (dest, batch) in destinations {
                                let c = Arc::clone(c);
                                let retry_q = retry_queue.clone();
                                let pool = pool_opt.clone();
                                let backoff_list = backoff.clone();
                                let span = info_span!(
                                    "EventBroadcaster.send_batch_fire_and_forget",
                                    destination = %dest,
                                );
                                tokio::spawn(async move {
                                    // W-06: catch_unwind ensures panics in the
                                    // per-destination send_batch task are logged
                                    // instead of silently swallowed when the
                                    // JoinHandle is dropped. The federation
                                    // send_batch body holds &c / &retry_q / &pool
                                    // / &backoff_list — all safe across unwind
                                    // boundaries, hence AssertUnwindSafe.
                                    let dest_for_panic = dest.clone();
                                    let result = AssertUnwindSafe(async move {
                                        let _enter = span.enter();
                                        let mut local_batches: HashMap<String, TransactionBatch> = HashMap::new();
                                        local_batches.insert(dest.clone(), batch);
                                        send_batch(
                                            &c,
                                            &retry_q,
                                            &pool,
                                            &backoff_list,
                                            &local_batches,
                                            &dest,
                                        ).await;
                                    })
                                    .catch_unwind()
                                    .await;

                                    if let Err(panic_payload) = result {
                                        warn!(
                                            panic = ?panic_payload,
                                            destination = %dest_for_panic,
                                            "W-06: EventBroadcaster.send_batch_fire_and_forget task panicked — panic was caught and logged"
                                        );
                                    }
                                });
                            }
                        }
                    }
                }
            }
        });
    }

    async fn push_pdu(&self, destination: &str, pdu: serde_json::Value) {
        let guard = self.batch_tx.lock().await;
        if let Some(tx) = guard.as_ref() {
            if let Err(e) = tx.try_send((destination.to_string(), OutgoingItem::Pdu(pdu))) {
                ::tracing::warn!("Failed to queue PDU for federation broadcast to {}: {}", destination, e);
            }
        }
    }

    async fn push_edu(&self, destination: &str, edu: serde_json::Value) {
        let guard = self.batch_tx.lock().await;
        if let Some(tx) = guard.as_ref() {
            if let Err(e) = tx.try_send((destination.to_string(), OutgoingItem::Edu(edu))) {
                ::tracing::warn!("Failed to queue EDU for federation broadcast to {}: {}", destination, e);
            }
        }
    }

    /// See [`broadcast_event`.
    pub async fn broadcast_event(
        &self,
        room_id: &str,
        event: &serde_json::Value,
        origin: &str,
    ) -> Result<(), FederationBroadcastError> {
        let event_id = event.get("event_id").and_then(|v| v.as_str()).unwrap_or("unknown");

        let destinations = self.get_eligible_destinations(room_id).await;

        if destinations.is_empty() {
            return Ok(());
        }

        let has_batch = self.batch_tx.lock().await.is_some();
        if has_batch {
            for destination in &destinations {
                if destination == &self.server_name {
                    continue;
                }
                self.push_pdu(destination, event.clone()).await;
            }
            ::tracing::debug!("Pushed event {} to batch channel ({} destinations)", event_id, destinations.len());
            return Ok(());
        }

        let client = match &self.federation_client {
            Some(c) => c,
            None => return Ok(()),
        };

        let txn_id = format!("txn_{}_{}", current_timestamp_millis(), uuid::Uuid::new_v4());

        for destination in &destinations {
            if destination == &self.server_name {
                continue;
            }

            let transaction = FederationTransaction {
                transaction_id: txn_id.clone(),
                origin: origin.to_string(),
                origin_server_ts: current_timestamp_millis(),
                destination: destination.clone(),
                pdus: vec![event.clone()],
                edus: vec![],
            };

            match client.send_transaction(destination, &transaction).await {
                Ok(_) => {
                    ::tracing::info!("Successfully sent event {} to {}", event_id, destination);
                }
                Err(e) => {
                    ::tracing::warn!("Failed to send event {} to {}: {}", event_id, destination, e);
                    self.enqueue_for_retry(destination.clone(), transaction, 0).await;
                }
            }
        }

        Ok(())
    }

    /// See [`broadcast_edu`.
    pub async fn broadcast_edu(
        &self,
        destination: &str,
        edu: &serde_json::Value,
        origin: &str,
    ) -> Result<(), FederationBroadcastError> {
        if destination == self.server_name.as_str() {
            return Ok(());
        }

        let has_batch = self.batch_tx.lock().await.is_some();
        if has_batch {
            self.push_edu(destination, edu.clone()).await;
            return Ok(());
        }

        let client = match &self.federation_client {
            Some(c) => c,
            None => return Ok(()),
        };

        let txn_id = format!("edu_{}_{}", current_timestamp_millis(), uuid::Uuid::new_v4());

        let transaction = FederationTransaction {
            transaction_id: txn_id,
            origin: origin.to_string(),
            origin_server_ts: current_timestamp_millis(),
            destination: destination.to_string(),
            pdus: vec![],
            edus: vec![edu.clone()],
        };

        client
            .send_transaction(destination, &transaction)
            .await
            .map_err(|e| FederationBroadcastError::SendFailed(e.to_string()))?;

        Ok(())
    }

    /// See [`broadcast_edu_to_room`.
    pub async fn broadcast_edu_to_room(
        &self,
        room_id: &str,
        edu: &serde_json::Value,
        origin: &str,
    ) -> Result<(), FederationBroadcastError> {
        let destinations = self.get_eligible_destinations(room_id).await;

        if destinations.is_empty() {
            return Ok(());
        }

        let has_batch = self.batch_tx.lock().await.is_some();
        if has_batch {
            for destination in &destinations {
                if destination.as_str() == self.server_name.as_str() {
                    continue;
                }
                self.push_edu(destination, edu.clone()).await;
            }
            return Ok(());
        }

        let client = match &self.federation_client {
            Some(c) => c,
            None => return Ok(()),
        };

        for destination in &destinations {
            if destination.as_str() == self.server_name.as_str() {
                continue;
            }

            let txn_id = format!("edu_{}_{}", current_timestamp_millis(), uuid::Uuid::new_v4());

            let transaction = FederationTransaction {
                transaction_id: txn_id,
                origin: origin.to_string(),
                origin_server_ts: current_timestamp_millis(),
                destination: destination.clone(),
                pdus: vec![],
                edus: vec![edu.clone()],
            };

            if let Err(e) = client.send_transaction(destination, &transaction).await {
                ::tracing::warn!("Failed to send EDU to {} for room {}: {}", destination, room_id, e);
                self.enqueue_for_retry(destination.clone(), transaction, 0).await;
            }
        }

        Ok(())
    }

    async fn get_eligible_destinations(&self, room_id: &str) -> Vec<String> {
        if let Some(membership_storage) = &self.membership_storage {
            if let Ok(members) = membership_storage.get_joined_members(room_id).await {
                let mut servers: std::collections::HashSet<String> = std::collections::HashSet::new();
                for member in &members {
                    if let Some(pos) = member.user_id.find(':') {
                        let server = &member.user_id[pos + 1..];
                        if server != self.server_name {
                            servers.insert(server.to_string());
                        }
                    }
                }
                return servers.into_iter().collect();
            }
        }
        Vec::new()
    }

    pub(crate) fn get_backoff_delay(&self, retry_count: u32) -> u64 {
        let idx = (retry_count as usize).min(self.backoff_schedule.len() - 1);
        self.backoff_schedule[idx]
    }

    async fn persist_transaction_to_db(&self, destination: &str, transaction: &FederationTransaction) -> Option<i64> {
        let pool = self.pool.as_ref()?;
        persist_transaction_row(pool, destination, transaction).await
    }

    async fn update_db_status(&self, db_id: i64, status: &str) {
        if let Some(pool) = &self.pool {
            let result =
                match status {
                    "sent" => {
                        sqlx::query("UPDATE federation_queue SET status = 'sent', sent_at = $2 WHERE id = $1")
                            .bind(db_id)
                            .bind(current_timestamp_millis())
                            .execute(pool)
                            .await
                    }
                    "retry" => sqlx::query(
                        "UPDATE federation_queue SET retry_count = retry_count + 1, status = 'pending' WHERE id = $1",
                    )
                    .bind(db_id)
                    .execute(pool)
                    .await,
                    _ => {
                        sqlx::query("UPDATE federation_queue SET status = $2 WHERE id = $1")
                            .bind(db_id)
                            .bind(status)
                            .execute(pool)
                            .await
                    }
                };

            if let Err(e) = result {
                ::tracing::warn!("Failed to update federation_queue status for {}: {}", db_id, e);
            }
        }
    }

    async fn enqueue_for_retry(&self, destination: String, transaction: FederationTransaction, retry_count: u32) {
        let delay = self.get_backoff_delay(retry_count);
        let next_retry_at = current_timestamp_millis() + delay as i64;

        let db_id = self.persist_transaction_to_db(&destination, &transaction).await;

        let pending = PendingTransaction { destination, transaction, retry_count, next_retry_at, db_id };

        let mut queue = self.pending_queue.write().await;
        queue.push(pending.clone());
        ::tracing::info!(
            "Enqueued transaction for retry to {} (attempt {}), next retry in {}ms, persisted={}",
            pending.destination,
            retry_count + 1,
            delay,
            db_id.is_some()
        );
    }

    /// See [`retry_pending_transactions`.
    pub async fn retry_pending_transactions(&self) -> Result<usize, FederationBroadcastError> {
        let client = match &self.federation_client {
            Some(c) => c.clone(),
            None => return Ok(0),
        };

        let now = current_timestamp_millis();
        let mut queue = self.pending_queue.write().await;
        let mut retried = 0;
        let max_retries = 7u32;

        let mut still_pending = Vec::new();
        for pending in queue.drain(..) {
            if pending.next_retry_at > now {
                still_pending.push(pending);
                continue;
            }

            if pending.retry_count >= max_retries {
                ::tracing::warn!(
                    "Dropping transaction to {} after {} retries (db_id={:?})",
                    pending.destination,
                    pending.retry_count,
                    pending.db_id
                );
                if let Some(db_id) = pending.db_id {
                    self.update_db_status(db_id, "failed").await;
                }
                continue;
            }

            match client.send_transaction(&pending.destination, &pending.transaction).await {
                Ok(_) => {
                    ::tracing::info!(
                        "Retry succeeded for transaction to {} (attempt {})",
                        pending.destination,
                        pending.retry_count + 1
                    );
                    if let Some(db_id) = pending.db_id {
                        self.update_db_status(db_id, "sent").await;
                    }
                    retried += 1;
                }
                Err(e) => {
                    ::tracing::warn!(
                        "Retry failed for transaction to {} (attempt {}): {}",
                        pending.destination,
                        pending.retry_count + 1,
                        e
                    );
                    let delay = self.get_backoff_delay(pending.retry_count + 1);
                    let new_retry_count = pending.retry_count + 1;
                    if let Some(db_id) = pending.db_id {
                        self.update_db_status(db_id, "retry").await;
                    }
                    still_pending.push(PendingTransaction {
                        retry_count: new_retry_count,
                        next_retry_at: now + delay as i64,
                        ..pending
                    });
                }
            }
        }

        *queue = still_pending;
        Ok(retried)
    }
}

/// 把一条出站事务落库到 `federation_queue`（`status = 'pending'`），返回新行 id。
///
/// 失败（无法序列化 / 写库报错）一律记 `error!` 并返回 `None` —— 调用方据此退化为
/// "仅在内存队列里重试"。抽成自由函数是因为 `send_batch` 没有 `&self`（D-82）。
async fn persist_transaction_row(
    pool: &sqlx::PgPool,
    destination: &str,
    transaction: &FederationTransaction,
) -> Option<i64> {
    let content = match serde_json::to_value(transaction) {
        Ok(v) => v,
        Err(e) => {
            ::tracing::error!("Failed to serialize transaction for persistence: {}", e);
            return None;
        }
    };

    let event_id = format!("txn:{}", transaction.transaction_id);
    let event_type = if transaction.edus.is_empty() { "m.room.event" } else { "m.edu" };

    let room_id = if !transaction.pdus.is_empty() {
        transaction.pdus.first().and_then(|p| p.get("room_id").and_then(|v| v.as_str()).map(String::from))
    } else {
        None
    };

    match sqlx::query_as::<_, (i64,)>(
        r"
        INSERT INTO federation_queue (destination, event_id, event_type, room_id, content, created_ts, status)
        VALUES ($1, $2, $3, $4, $5, $6, 'pending')
        RETURNING id
        ",
    )
    .bind(destination)
    .bind(&event_id)
    .bind(event_type)
    .bind(&room_id)
    .bind(&content)
    .bind(current_timestamp_millis())
    .fetch_one(pool)
    .await
    {
        Ok(row) => Some(row.0),
        Err(e) => {
            ::tracing::error!("Failed to persist transaction to federation_queue: {}", e);
            None
        }
    }
}

async fn send_batch(
    client: &Arc<dyn FederationClientApi>,
    retry_queue: &Arc<RwLock<Vec<PendingTransaction>>>,
    pool_opt: &Option<sqlx::PgPool>,
    _backoff: &[u64],
    batches: &HashMap<String, TransactionBatch>,
    destination: &str,
) {
    let batch = match batches.get(destination) {
        Some(b) => b,
        None => return,
    };

    if batch.pdus.is_empty() && batch.edus.is_empty() {
        return;
    }

    let txn = FederationTransaction {
        transaction_id: format!("batch_{}_{}", current_timestamp_millis(), uuid::Uuid::new_v4()),
        origin: batch.origin.clone(),
        origin_server_ts: current_timestamp_millis(),
        destination: destination.to_string(),
        pdus: batch.pdus.clone(),
        edus: batch.edus.clone(),
    };

    match client.send_transaction(destination, &txn).await {
        Ok(_) => {
            ::tracing::debug!("Batch sent to {} ({} PDUs, {} EDUs)", destination, txn.pdus.len(), txn.edus.len());
        }
        Err(e) => {
            ::tracing::warn!(
                "Batch send to {} failed: {} ({} PDUs, {} EDUs)",
                destination,
                e,
                txn.pdus.len(),
                txn.edus.len()
            );

            // D-82：这里原本是 `persist_transaction_to_db` 的**第二份实现**（同一个 INSERT
            // 复制一份，`event_type` 硬编码 `'m.room.event'`、`room_id` 恒 `NULL`，且用
            // `.ok()` 把写库错误**静默吞掉**）。现在两处共用 `persist_transaction_row`：
            // 元数据正确（EDU 批次记 `m.edu`、`room_id` 取首条 PDU），错误统一 `error!` 记录。
            let db_id =
                if let Some(pool) = pool_opt { persist_transaction_row(pool, destination, &txn).await } else { None };

            let delay = if txn.pdus.len() > 1 { 5000u64 } else { 1000u64 };
            let next_retry_at = current_timestamp_millis() + delay as i64;

            let mut queue = retry_queue.write().await;
            queue.push(PendingTransaction {
                destination: destination.to_string(),
                transaction: txn,
                retry_count: 0,
                next_retry_at,
                db_id,
            });
        }
    }
}

#[derive(Debug, thiserror::Error)]
/// The `FederationBroadcastError` enum.
pub enum FederationBroadcastError {
    /// The `SendFailed` variant.
    #[error("Failed to send event: {0}")]
    SendFailed(String),
    /// The `InvalidEvent` variant.
    #[error("Invalid event data: {0}")]
    InvalidEvent(String),
    /// The `NetworkError` variant.
    #[error("Network error: {0}")]
    NetworkError(String),
}

// ---------------------------------------------------------------------------
// EventBroadcaster trait implementation
// ---------------------------------------------------------------------------

/// Message type for the federation [`EventBroadcaster`] trait implementation.
///
/// Wraps a [`FederationEvent`] so it can be published through the generic
/// `EventBroadcaster` interface.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FederationBroadcastMessage {
    /// The `event` field.
    pub event: FederationEvent,
}

/// Implementation of [`synapse_common`] methods.
impl synapse_common::traits::EventBroadcaster for EventBroadcaster {
    type Message = FederationBroadcastMessage;

    async fn broadcast_publish(&self, message: Self::Message) -> Result<(), synapse_common::traits::BroadcastError> {
        let event_value = serde_json::to_value(&message.event)
            .map_err(|e| synapse_common::traits::BroadcastError::EncodingFailed(e.to_string()))?;

        self.broadcast_event(&message.event.room_id, &event_value, &message.event.origin)
            .await
            .map_err(|e| synapse_common::traits::BroadcastError::Transport(e.to_string()))
    }

    fn broadcast_subscriber_count(&self) -> usize {
        // Federation broadcaster doesn't track subscribers in the traditional sense;
        // return the pending queue length as a proxy for active work.
        self.pending_queue.try_read().map(|q| q.len()).unwrap_or(0)
    }
}

/// Implementation of [`From`] methods.
impl From<FederationBroadcastError> for synapse_common::traits::BroadcastError {
    fn from(e: FederationBroadcastError) -> Self {
        match e {
            FederationBroadcastError::SendFailed(msg) => synapse_common::traits::BroadcastError::Transport(msg),
            FederationBroadcastError::InvalidEvent(msg) => synapse_common::traits::BroadcastError::EncodingFailed(msg),
            FederationBroadcastError::NetworkError(msg) => synapse_common::traits::BroadcastError::Transport(msg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_backoff_delay_first_retry() {
        let broadcaster = EventBroadcaster::new("test.local".into());
        assert_eq!(broadcaster.get_backoff_delay(0), 1000);
        assert_eq!(broadcaster.get_backoff_delay(1), 5000);
        assert_eq!(broadcaster.get_backoff_delay(2), 15000);
    }

    #[test]
    fn get_backoff_delay_max_retries_clamped() {
        let broadcaster = EventBroadcaster::new("test.local".into());
        // Schedule: [1000, 5000, 15000, 30000, 60000, 300000, 900000] — 7 elements
        assert_eq!(broadcaster.get_backoff_delay(6), 900_000); // last element
        assert_eq!(broadcaster.get_backoff_delay(7), 900_000); // clamped
        assert_eq!(broadcaster.get_backoff_delay(100), 900_000); // clamped
    }

    #[test]
    fn get_backoff_delay_middle_elements() {
        let broadcaster = EventBroadcaster::new("test.local".into());
        assert_eq!(broadcaster.get_backoff_delay(3), 30_000);
        assert_eq!(broadcaster.get_backoff_delay(4), 60_000);
        assert_eq!(broadcaster.get_backoff_delay(5), 300_000);
    }
}

/// `federation_queue` 写入/状态流转在**真 baseline** 上的往返。
///
/// C45 的 4 处转换全在 `persist_transaction_row` 与 `update_db_status` 里，而本文件此前
/// **没有任何 DB 测试**（`mod tests` 只覆盖退避表这类纯函数）⇒ 按 R8/R9 先补真基线覆盖：
/// `federation_queue.content` 是 `JSONB NOT NULL`、`status`/`retry_count`/`sent_at` 只有
/// `DEFAULT` 没有 `NOT NULL`，这些可空性错配只会在真 catalog 前的第一次往返里暴露。
#[cfg(test)]
mod db_tests {
    use super::*;
    use sqlx::Row;

    fn transaction(id: &str) -> FederationTransaction {
        FederationTransaction {
            transaction_id: id.to_string(),
            origin: "origin.example.com".to_string(),
            origin_server_ts: 1_700_000_000_000,
            destination: "dest.example.com".to_string(),
            pdus: vec![serde_json::json!({"room_id": "!r:origin.example.com", "type": "m.room.message"})],
            edus: Vec::new(),
        }
    }

    /// 读回一行 `federation_queue`（**测试区**夹具，R9/D-13：宏不进 `cargo sqlx prepare`）。
    #[allow(clippy::type_complexity)]
    async fn queue_row(
        pool: &sqlx::PgPool,
        id: i64,
    ) -> (String, String, String, Option<String>, Option<String>, Option<i64>, Option<i32>) {
        let row = sqlx::query(
            "SELECT destination, event_id, event_type, room_id, status, sent_at, retry_count \
             FROM federation_queue WHERE id = $1",
        )
        .bind(id)
        .fetch_one(pool)
        .await
        .expect("federation_queue row");
        (
            row.get("destination"),
            row.get("event_id"),
            row.get("event_type"),
            row.get("room_id"),
            row.get("status"),
            row.get("sent_at"),
            row.get("retry_count"),
        )
    }

    #[tokio::test]
    async fn persist_and_status_transitions_round_trip_on_the_migration_template() {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = isolated.pool();
        let broadcaster = EventBroadcaster::new("origin.example.com".to_string()).with_pool((*pool).clone());

        // PDU 批次：`event_type = 'm.room.event'`，`room_id` 取首条 PDU。
        let id = broadcaster
            .persist_transaction_to_db("dest.example.com", &transaction("pdu-1"))
            .await
            .expect("persisted row id");
        let (destination, event_id, event_type, room_id, status, sent_at, retry_count) = queue_row(&pool, id).await;
        assert_eq!(destination, "dest.example.com");
        assert_eq!(event_id, "txn:pdu-1");
        assert_eq!(event_type, "m.room.event");
        assert_eq!(room_id.as_deref(), Some("!r:origin.example.com"));
        assert_eq!(status.as_deref(), Some("pending"));
        assert_eq!(sent_at, None);
        assert_eq!(retry_count, Some(0));

        // EDU 批次：没有 PDU ⇒ `event_type = 'm.edu'`、`room_id` 为 NULL。
        let mut edu_txn = transaction("edu-1");
        edu_txn.pdus.clear();
        edu_txn.edus.push(serde_json::json!({"edu_type": "m.typing"}));
        let edu_id =
            broadcaster.persist_transaction_to_db("dest.example.com", &edu_txn).await.expect("persisted edu row id");
        let (_, _, edu_event_type, edu_room_id, _, _, _) = queue_row(&pool, edu_id).await;
        assert_eq!(edu_event_type, "m.edu");
        assert_eq!(edu_room_id, None);

        // `update_db_status` 的三个分支：sent（写入 sent_at）、retry（计数 +1 并回到 pending）、
        // 以及兜底分支（状态名直接落库）。
        broadcaster.update_db_status(id, "sent").await;
        let (_, _, _, _, status, sent_at, retry_count) = queue_row(&pool, id).await;
        assert_eq!(status.as_deref(), Some("sent"));
        assert!(sent_at.is_some(), "sent 分支必须写 sent_at");
        assert_eq!(retry_count, Some(0));

        broadcaster.update_db_status(id, "retry").await;
        let (_, _, _, _, status, _, retry_count) = queue_row(&pool, id).await;
        assert_eq!(status.as_deref(), Some("pending"));
        assert_eq!(retry_count, Some(1), "retry 分支必须把 retry_count 加一");

        broadcaster.update_db_status(id, "failed").await;
        let (_, _, _, _, status, _, retry_count) = queue_row(&pool, id).await;
        assert_eq!(status.as_deref(), Some("failed"));
        assert_eq!(retry_count, Some(1));
    }

    #[tokio::test]
    async fn send_batch_persists_the_transaction_when_the_send_fails() {
        let isolated = crate::test_isolation::isolated_test_pool().await.expect("isolated pool");
        let pool = isolated.pool();

        let mock = Arc::new(crate::test_mocks::MockFederationClient::new("origin.example.com"));
        mock.fail_send_transactions(true);
        let client: Arc<dyn FederationClientApi> = mock.clone();

        let mut batches = HashMap::new();
        batches.insert(
            "dest.example.com".to_string(),
            TransactionBatch {
                pdus: vec![serde_json::json!({"room_id": "!r:origin.example.com", "type": "m.room.message"})],
                edus: Vec::new(),
                origin: "origin.example.com".to_string(),
            },
        );
        let retry_queue: Arc<RwLock<Vec<PendingTransaction>>> = Arc::new(RwLock::new(Vec::new()));

        send_batch(&client, &retry_queue, &Some((*pool).clone()), &[1000], &batches, "dest.example.com").await;

        // 内存队列拿到一条待重试事务，并且它**确实落库**了（D-82 后走共享实现）。
        let queue = retry_queue.read().await;
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].destination, "dest.example.com");
        let db_id = queue[0].db_id.expect("failed send must still persist the transaction");
        drop(queue);

        let (_, event_id, event_type, room_id, status, _, _) = queue_row(&pool, db_id).await;
        assert!(event_id.starts_with("txn:batch_"), "unexpected event_id {event_id}");
        assert_eq!(event_type, "m.room.event");
        assert_eq!(room_id.as_deref(), Some("!r:origin.example.com"), "共享实现必须带上 PDU 的 room_id");
        assert_eq!(status.as_deref(), Some("pending"));

        // 对照组：发送成功时不落库（只在 `sent_transactions` 里留痕）。
        let ok_mock = Arc::new(crate::test_mocks::MockFederationClient::new("origin.example.com"));
        let ok_client: Arc<dyn FederationClientApi> = ok_mock.clone();
        let ok_queue: Arc<RwLock<Vec<PendingTransaction>>> = Arc::new(RwLock::new(Vec::new()));
        send_batch(&ok_client, &ok_queue, &Some((*pool).clone()), &[1000], &batches, "dest.example.com").await;
        assert!(ok_queue.read().await.is_empty());
        assert_eq!(ok_mock.sent_transactions().await.len(), 1);
    }
}
