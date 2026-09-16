//! In-memory [`RendezvousStoreApi`] covering both the legacy session/message
//! surface and the MSC4108 ETag surface.
//!
//! Semantics mirror the Postgres implementation where they are observable:
//!
//! * `get_session` / `get_msc4108_data` return `None` for a missing **or
//!   expired** session (`expires_at > now`).
//! * MSC4108 ETags are quoted millisecond timestamps (`"<millis>"`), and an
//!   internal monotonic clock advances on every read so two writes can never
//!   share an ETag (the real storage has the same millisecond granularity).
//! * A conditional `update_msc4108_data` distinguishes "not found" from "ETag
//!   precondition failed" exactly like `Msc4108UpdateOutcome`.
//! * `delete_session` is idempotent (`Ok(())` regardless of rows affected);
//!   `delete_msc4108_session` reports whether a row was removed.

use std::collections::HashMap;
use std::sync::Arc;

use crate::rendezvous::{
    CreateRendezvousSessionParams, Msc4108UpdateOutcome, RendezvousMessage, RendezvousSession, RendezvousStoreApi,
    StoredRendezvousMessage,
};
use synapse_common::current_timestamp_millis;
use tokio::sync::Mutex;

/// One MSC4108 session row stored in memory.
#[derive(Debug, Clone)]
struct Msc4108Row {
    data: String,
    /// ETag stored WITHOUT the surrounding quotes (the raw timestamp string),
    /// matching how the real storage compares `updated_ts::TEXT`.
    etag_raw: String,
    expires_at: i64,
}

/// In-memory [`RendezvousStoreApi`] for service-layer tests.
pub struct InMemoryRendezvousStore {
    sessions: Arc<Mutex<HashMap<String, RendezvousSession>>>,
    messages: Arc<Mutex<Vec<StoredRendezvousMessage>>>,
    msc4108: Arc<Mutex<HashMap<String, Msc4108Row>>>,
    next_id: Arc<Mutex<i64>>,
    clock: Arc<Mutex<i64>>,
}

impl Default for InMemoryRendezvousStore {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryRendezvousStore {
    /// See [`new`].
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            messages: Arc::new(Mutex::new(Vec::new())),
            msc4108: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(Mutex::new(1)),
            clock: Arc::new(Mutex::new(current_timestamp_millis())),
        }
    }

    /// Return the next mock timestamp (strictly increasing across calls).
    async fn now(&self) -> i64 {
        let mut c = self.clock.lock().await;
        *c += 1;
        *c
    }

    async fn alloc_id(&self) -> i64 {
        let mut id = self.next_id.lock().await;
        let value = *id;
        *id += 1;
        value
    }
}

#[async_trait::async_trait]
impl RendezvousStoreApi for InMemoryRendezvousStore {
    async fn create_session(&self, params: CreateRendezvousSessionParams) -> Result<RendezvousSession, sqlx::Error> {
        let now = self.now().await;
        let session = RendezvousSession {
            id: self.alloc_id().await,
            session_id: uuid::Uuid::new_v4().simple().to_string(),
            user_id: None,
            device_id: None,
            intent: Some(params.intent.as_str().to_string()),
            transport: Some(params.transport.as_str().to_string()),
            transport_data: params.transport_data,
            key: Some(format!("key-{}", uuid::Uuid::new_v4().simple())),
            created_ts: now,
            expires_at: now + params.expires_in_ms.unwrap_or(5 * 60 * 1000),
            status: Some("pending".to_string()),
        };
        self.sessions.lock().await.insert(session.session_id.clone(), session.clone());
        Ok(session)
    }

    async fn get_session(&self, session_id: &str) -> Result<Option<RendezvousSession>, sqlx::Error> {
        let now = self.now().await;
        Ok(self.sessions.lock().await.get(session_id).filter(|s| s.expires_at > now).cloned())
    }

    async fn update_session_status(&self, session_id: &str, status: &str) -> Result<(), sqlx::Error> {
        if let Some(session) = self.sessions.lock().await.get_mut(session_id) {
            session.status = Some(status.to_string());
        }
        Ok(())
    }

    async fn bind_user_to_session(&self, session_id: &str, user_id: &str, device_id: &str) -> Result<(), sqlx::Error> {
        if let Some(session) = self.sessions.lock().await.get_mut(session_id) {
            session.user_id = Some(user_id.to_string());
            session.device_id = Some(device_id.to_string());
        }
        Ok(())
    }

    async fn complete_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        if let Some(session) = self.sessions.lock().await.get_mut(session_id) {
            session.status = Some("completed".to_string());
        }
        Ok(())
    }

    async fn delete_session(&self, session_id: &str) -> Result<(), sqlx::Error> {
        self.sessions.lock().await.remove(session_id);
        Ok(())
    }

    async fn cleanup_expired_sessions(&self) -> Result<u64, sqlx::Error> {
        let now = self.now().await;
        let mut sessions = self.sessions.lock().await;
        let before = sessions.len();
        sessions.retain(|_, s| s.expires_at > now);
        Ok((before - sessions.len()) as u64)
    }

    async fn store_message(
        &self,
        session_id: &str,
        direction: &str,
        message: &RendezvousMessage,
    ) -> Result<(), sqlx::Error> {
        let id = self.alloc_id().await;
        self.messages.lock().await.push(StoredRendezvousMessage {
            id,
            session_id: session_id.to_string(),
            direction: direction.to_string(),
            message_type: message.message_type.clone(),
            content: message.content.clone(),
            created_ts: current_timestamp_millis(),
        });
        Ok(())
    }

    async fn get_messages(
        &self,
        session_id: &str,
        after_id: Option<i64>,
    ) -> Result<Vec<StoredRendezvousMessage>, sqlx::Error> {
        let mut messages: Vec<StoredRendezvousMessage> = self
            .messages
            .lock()
            .await
            .iter()
            .filter(|m| m.session_id == session_id && after_id.is_none_or(|after| m.id > after))
            .cloned()
            .collect();
        messages.sort_by_key(|m| m.id);
        Ok(messages)
    }

    async fn create_msc4108_session(
        &self,
        initial_data: &str,
        ttl_ms: i64,
    ) -> Result<(String, String, i64, i64), sqlx::Error> {
        let now = self.now().await;
        let session_id = uuid::Uuid::new_v4().simple().to_string();
        let expires_at = now + ttl_ms;
        let etag_raw = now.to_string();

        self.msc4108.lock().await.insert(
            session_id.clone(),
            Msc4108Row { data: initial_data.to_string(), etag_raw: etag_raw.clone(), expires_at },
        );

        Ok((session_id, format!("\"{etag_raw}\""), now, expires_at))
    }

    async fn get_msc4108_data(&self, session_id: &str) -> Result<Option<(String, String, i64, i64)>, sqlx::Error> {
        let now = self.now().await;
        let guard = self.msc4108.lock().await;
        match guard.get(session_id) {
            Some(row) if row.expires_at > now => Ok(Some((
                row.data.clone(),
                format!("\"{}\"", row.etag_raw),
                row.etag_raw.parse::<i64>().unwrap_or(0),
                row.expires_at,
            ))),
            _ => Ok(None),
        }
    }

    async fn update_msc4108_data(
        &self,
        session_id: &str,
        data: &str,
        if_match: Option<&str>,
    ) -> Result<Msc4108UpdateOutcome, sqlx::Error> {
        let now = self.now().await;
        let mut guard = self.msc4108.lock().await;

        let (current_etag_raw, expires_at) = match guard.get(session_id) {
            Some(row) if row.expires_at > now => (row.etag_raw.clone(), row.expires_at),
            _ => return Ok(Msc4108UpdateOutcome::NotFound),
        };

        // Conditional update: verify the precondition first.
        if let Some(expected_etag) = if_match {
            let expected_raw = expected_etag.trim_matches('"').to_string();
            if expected_raw != current_etag_raw {
                return Ok(Msc4108UpdateOutcome::PreconditionFailed {
                    current_etag: format!("\"{current_etag_raw}\""),
                    updated_ts: current_etag_raw.parse::<i64>().unwrap_or(0),
                    expires_at,
                });
            }
        }

        match guard.get_mut(session_id) {
            Some(row) if row.expires_at > now => {
                let new_etag_raw = now.to_string();
                row.data = data.to_string();
                row.etag_raw = new_etag_raw.clone();
                Ok(Msc4108UpdateOutcome::Updated {
                    new_etag: format!("\"{new_etag_raw}\""),
                    updated_ts: now,
                    expires_at,
                })
            }
            _ => Ok(Msc4108UpdateOutcome::NotFound),
        }
    }

    async fn delete_msc4108_session(&self, session_id: &str) -> Result<bool, sqlx::Error> {
        let removed = self.msc4108.lock().await.remove(session_id).is_some();
        Ok(removed)
    }
}
