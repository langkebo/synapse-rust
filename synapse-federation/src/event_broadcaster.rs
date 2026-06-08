use crate::client::{FederationClient, FederationTransaction};
use synapse_storage::membership::RoomMemberStorage;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationEvent {
    pub event_id: String,
    pub room_id: String,
    pub sender: String,
    pub event_type: String,
    pub content: serde_json::Value,
    pub origin: String,
    pub destination: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTransaction {
    pub destination: String,
    pub transaction: FederationTransaction,
    pub retry_count: u32,
    pub next_retry_at: i64,
    pub db_id: Option<i64>,
}

#[derive(Debug, Clone)]
enum OutgoingItem {
    Pdu(serde_json::Value),
    Edu(serde_json::Value),
}

#[derive(Debug, Clone)]
struct TransactionBatch {
    pdus: Vec<serde_json::Value>,
    edus: Vec<serde_json::Value>,
    origin: String,
}

#[derive(Clone)]
pub struct EventBroadcaster {
    server_name: String,
    federation_client: Option<Arc<FederationClient>>,
    membership_storage: Option<Arc<RoomMemberStorage>>,
    pending_queue: Arc<RwLock<Vec<PendingTransaction>>>,
    backoff_schedule: Vec<u64>,
    pool: Option<sqlx::PgPool>,
    batch_tx: Arc<tokio::sync::Mutex<Option<BatchSender>>>,
}

type DbPendingRow = (i64, String, String, Option<String>, serde_json::Value, i64, i32);
type BatchSender = mpsc::Sender<(String, OutgoingItem)>;

impl EventBroadcaster {
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

    pub fn with_client(mut self, client: Arc<FederationClient>) -> Self {
        self.federation_client = Some(client);
        self
    }

    pub fn with_pool(mut self, pool: sqlx::PgPool) -> Self {
        self.pool = Some(pool);
        self
    }

    pub fn with_membership_storage(mut self, storage: Arc<RoomMemberStorage>) -> Self {
        self.membership_storage = Some(storage);
        self
    }

    pub fn set_client(&mut self, client: Arc<FederationClient>) {
        self.federation_client = Some(client);
    }

    pub fn set_membership_storage(&mut self, storage: Arc<RoomMemberStorage>) {
        self.membership_storage = Some(storage);
    }

    pub fn set_pool(&mut self, pool: sqlx::PgPool) {
        self.pool = Some(pool);
    }

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
                            let destinations: Vec<String> = batches.keys().cloned().collect();
                            let c = match &client {
                                Some(c) => c,
                                None => continue,
                            };
                            for dest in &destinations {
                                send_batch(
                                    c,
                                    &retry_queue,
                                    &pool_opt,
                                    &backoff,
                                    &batches,
                                    dest,
                                ).await;
                                batches.remove(dest);
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

    pub async fn recover_pending_from_db(&self) -> Result<usize, FederationBroadcastError> {
        let pool = match &self.pool {
            Some(p) => p,
            None => return Ok(0),
        };

        let rows: Vec<DbPendingRow> = sqlx::query_as(
            r"
            SELECT id, destination, event_id, room_id, content, created_ts, retry_count
            FROM federation_queue
            WHERE status = 'pending'
            ORDER BY created_ts ASC
            LIMIT 500
            ",
        )
        .fetch_all(pool)
        .await
        .map_err(|e| FederationBroadcastError::SendFailed(e.to_string()))?;

        let now = chrono::Utc::now().timestamp_millis();
        let mut queue = self.pending_queue.write().await;
        let count = rows.len();

        for (db_id, destination, _event_id, _room_id, content, _created_ts, retry_count) in rows {
            let retry_count = retry_count as u32;
            let delay = self.get_backoff_delay(retry_count);

            let transaction: FederationTransaction = match serde_json::from_value(content) {
                Ok(txn) => txn,
                Err(e) => {
                    ::tracing::warn!("Failed to deserialize persisted transaction {}: {}", db_id, e);
                    let _ = sqlx::query("DELETE FROM federation_queue WHERE id = $1").bind(db_id).execute(pool).await;
                    continue;
                }
            };

            queue.push(PendingTransaction {
                destination,
                transaction,
                retry_count,
                next_retry_at: now + delay as i64,
                db_id: Some(db_id),
            });
        }

        if count > 0 {
            ::tracing::info!("Recovered {} pending federation transactions from database", count);
        }

        Ok(count)
    }

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

        let txn_id = format!("txn_{}_{}", chrono::Utc::now().timestamp_millis(), uuid::Uuid::new_v4());

        for destination in &destinations {
            if destination == &self.server_name {
                continue;
            }

            let transaction = FederationTransaction {
                transaction_id: txn_id.clone(),
                origin: origin.to_string(),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
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

        let txn_id = format!("edu_{}_{}", chrono::Utc::now().timestamp_millis(), uuid::Uuid::new_v4());

        let transaction = FederationTransaction {
            transaction_id: txn_id,
            origin: origin.to_string(),
            origin_server_ts: chrono::Utc::now().timestamp_millis(),
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

            let txn_id = format!("edu_{}_{}", chrono::Utc::now().timestamp_millis(), uuid::Uuid::new_v4());

            let transaction = FederationTransaction {
                transaction_id: txn_id,
                origin: origin.to_string(),
                origin_server_ts: chrono::Utc::now().timestamp_millis(),
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

    fn get_backoff_delay(&self, retry_count: u32) -> u64 {
        let idx = (retry_count as usize).min(self.backoff_schedule.len() - 1);
        self.backoff_schedule[idx]
    }

    async fn persist_transaction_to_db(&self, destination: &str, transaction: &FederationTransaction) -> Option<i64> {
        let pool = self.pool.as_ref()?;

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
        .bind(chrono::Utc::now().timestamp_millis())
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

    async fn update_db_status(&self, db_id: i64, status: &str) {
        if let Some(pool) = &self.pool {
            let result =
                match status {
                    "sent" => {
                        sqlx::query("UPDATE federation_queue SET status = 'sent', sent_at = $2 WHERE id = $1")
                            .bind(db_id)
                            .bind(chrono::Utc::now().timestamp_millis())
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
        let next_retry_at = chrono::Utc::now().timestamp_millis() + delay as i64;

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

    pub async fn retry_pending_transactions(&self) -> Result<usize, FederationBroadcastError> {
        let client = match &self.federation_client {
            Some(c) => c.clone(),
            None => return Ok(0),
        };

        let now = chrono::Utc::now().timestamp_millis();
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

    pub async fn get_pending_count(&self) -> usize {
        self.pending_queue.read().await.len()
    }

    pub async fn cleanup_old_transactions(&self, older_than_ts: i64) -> Result<u64, FederationBroadcastError> {
        let pool = match &self.pool {
            Some(p) => p,
            None => return Ok(0),
        };

        sqlx::query("DELETE FROM federation_queue WHERE status IN ('sent', 'failed') AND created_ts < $1")
            .bind(older_than_ts)
            .execute(pool)
            .await
            .map(|r| r.rows_affected())
            .map_err(|e| FederationBroadcastError::SendFailed(e.to_string()))
    }
}

async fn send_batch(
    client: &Arc<FederationClient>,
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
        transaction_id: format!("batch_{}_{}", chrono::Utc::now().timestamp_millis(), uuid::Uuid::new_v4()),
        origin: batch.origin.clone(),
        origin_server_ts: chrono::Utc::now().timestamp_millis(),
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

            let db_id = if let Some(pool) = pool_opt {
                let content = match serde_json::to_value(&txn) {
                    Ok(v) => v,
                    Err(_) => {
                        let mut queue = retry_queue.write().await;
                        queue.push(PendingTransaction {
                            destination: destination.to_string(),
                            transaction: txn,
                            retry_count: 0,
                            next_retry_at: chrono::Utc::now().timestamp_millis() + 5000,
                            db_id: None,
                        });
                        return;
                    }
                };

                let event_id = format!("txn:{}", txn.transaction_id);
                sqlx::query_as::<_, (i64,)>(
                    r"
                    INSERT INTO federation_queue (destination, event_id, event_type, room_id, content, created_ts, status)
                    VALUES ($1, $2, 'm.room.event', NULL, $3, $4, 'pending')
                    RETURNING id
                    ",
                )
                .bind(destination)
                .bind(&event_id)
                .bind(&content)
                .bind(chrono::Utc::now().timestamp_millis())
                .fetch_one(pool)
                .await
                .ok()
                .map(|r: (i64,)| r.0)
            } else {
                None
            };

            let delay = if txn.pdus.len() > 1 { 5000u64 } else { 1000u64 };
            let next_retry_at = chrono::Utc::now().timestamp_millis() + delay as i64;

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
pub enum FederationBroadcastError {
    #[error("Failed to send event: {0}")]
    SendFailed(String),
    #[error("Invalid event data: {0}")]
    InvalidEvent(String),
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
    pub event: FederationEvent,
}

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
        self.pending_queue
            .try_read()
            .map(|q| q.len())
            .unwrap_or(0)
    }
}

impl From<FederationBroadcastError> for synapse_common::traits::BroadcastError {
    fn from(e: FederationBroadcastError) -> Self {
        match e {
            FederationBroadcastError::SendFailed(msg) => {
                synapse_common::traits::BroadcastError::Transport(msg)
            }
            FederationBroadcastError::InvalidEvent(msg) => {
                synapse_common::traits::BroadcastError::EncodingFailed(msg)
            }
            FederationBroadcastError::NetworkError(msg) => {
                synapse_common::traits::BroadcastError::Transport(msg)
            }
        }
    }
}
