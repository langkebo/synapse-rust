use crate::common::{generate_event_id, ApiError, ApiResult};
use crate::storage::{CreateEventParams, EventStorage};
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct ReadReceipt {
    pub user_id: String,
    pub room_id: String,
    pub event_id: String,
    pub thread_id: Option<String>,
    pub timestamp: i64,
    pub receipt_type: ReadReceiptType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadReceiptType {
    Read,
    ReadPrivate,
}

#[derive(Debug, Clone)]
pub struct ReceiptEvent {
    pub room_id: String,
    pub event_id: String,
    pub user_id: String,
    pub receipt_type: ReadReceiptType,
    pub timestamp: i64,
}

pub struct ReadReceiptService {
    event_storage: EventStorage,
    receipts: Arc<RwLock<HashMap<String, Vec<ReadReceipt>>>>,
    server_name: String,
}

impl ReadReceiptService {
    pub fn new(event_storage: EventStorage, server_name: String) -> Self {
        Self {
            event_storage,
            receipts: Arc::new(RwLock::new(HashMap::new())),
            server_name,
        }
    }

    pub async fn set_read_receipt(
        &self,
        user_id: &str,
        room_id: &str,
        event_id: &str,
        receipt_type: ReadReceiptType,
    ) -> ApiResult<()> {
        let now = chrono::Utc::now().timestamp_millis();
        
        let receipt = ReadReceipt {
            user_id: user_id.to_string(),
            room_id: room_id.to_string(),
            event_id: event_id.to_string(),
            thread_id: None,
            timestamp: now,
            receipt_type,
        };

        let content = json!({
            "user_id": user_id,
            "event_id": event_id,
            "ts": now,
            "type": match receipt_type {
                ReadReceiptType::Read => "m.read",
                ReadReceiptType::ReadPrivate => "m.read.private",
            }
        });

        self.event_storage
            .create_event(
                CreateEventParams {
                    event_id: generate_event_id(&self.server_name),
                    room_id: room_id.to_string(),
                    user_id: user_id.to_string(),
                    event_type: "m.receipt".to_string(),
                    content,
                    state_key: Some(user_id.to_string()),
                    origin_server_ts: now,
                },
                None,
            )
            .await
            .map_err(|e| {
                let error_msg = e.to_string();
                if error_msg.contains("foreign key") {
                    if error_msg.contains("room_id") {
                        ApiError::not_found("Room not found")
                    } else if error_msg.contains("sender") || error_msg.contains("user_id") {
                        ApiError::not_found("User not found")
                    } else {
                        ApiError::database(error_msg)
                    }
                } else {
                    ApiError::database(error_msg)
                }
            })?;

        let mut receipts = self.receipts.write().await;
        let room_receipts = receipts.entry(room_id.to_string()).or_insert_with(Vec::new);
        
        room_receipts.retain(|r| r.user_id != user_id);
        room_receipts.push(receipt);

        Ok(())
    }

    pub async fn get_read_receipts(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> ApiResult<Vec<ReadReceipt>> {
        let receipts = self.receipts.read().await;
        Ok(receipts
            .get(room_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.event_id == event_id || event_id.is_empty())
            .collect())
    }

    pub async fn get_user_read_receipt(
        &self,
        room_id: &str,
        user_id: &str,
    ) -> ApiResult<Option<ReadReceipt>> {
        let receipts = self.receipts.read().await;
        Ok(receipts
            .get(room_id)
            .and_then(|room_receipts| {
                room_receipts.iter().find(|r| r.user_id == user_id).cloned()
            }))
    }

    pub async fn get_unread_count(
        &self,
        room_id: &str,
        user_id: &str,
        since_event_id: Option<&str>,
    ) -> ApiResult<i64> {
        let receipts = self.receipts.read().await;
        let room_receipts = receipts.get(room_id).cloned().unwrap_or_default();
        
        let user_receipt = room_receipts
            .iter()
            .find(|r| r.user_id == user_id);
        
        if let Some(receipt) = user_receipt {
            if since_event_id.is_some() {
                let receipts_since = room_receipts
                    .iter()
                    .filter(|r| r.timestamp > receipt.timestamp && r.user_id != user_id)
                    .count();
                Ok(receipts_since as i64)
            } else {
                Ok(0)
            }
        } else {
            Ok(0)
        }
    }

    pub async fn get_public_read_receipts(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> ApiResult<Vec<ReadReceipt>> {
        let receipts = self.receipts.read().await;
        Ok(receipts
            .get(room_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|r| {
                r.event_id == event_id || event_id.is_empty()
            })
            .filter(|r| r.receipt_type == ReadReceiptType::Read)
            .collect())
    }

    pub async fn get_private_read_receipts(
        &self,
        room_id: &str,
        event_id: &str,
    ) -> ApiResult<Vec<ReadReceipt>> {
        let receipts = self.receipts.read().await;
        Ok(receipts
            .get(room_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|r| {
                r.event_id == event_id || event_id.is_empty()
            })
            .filter(|r| r.receipt_type == ReadReceiptType::ReadPrivate)
            .collect())
    }

    pub async fn cleanup_old_receipts(&self, max_age_ms: i64) {
        let now = chrono::Utc::now().timestamp_millis();
        let mut receipts = self.receipts.write().await;
        
        let empty_rooms: Vec<String> = receipts
            .iter_mut()
            .filter_map(|(room_id, room_receipts)| {
                room_receipts.retain(|r| now - r.timestamp < max_age_ms);
                if room_receipts.is_empty() {
                    Some(room_id.clone())
                } else {
                    None
                }
            })
            .collect();
        
        for room_id in empty_rooms {
            receipts.remove(&room_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_receipt_creation() {
        let receipt = ReadReceipt {
            user_id: "@alice:example.com".to_string(),
            room_id: "!room:example.com".to_string(),
            event_id: "$event:example.com".to_string(),
            thread_id: None,
            timestamp: 1234567890,
            receipt_type: ReadReceiptType::Read,
        };
        
        assert_eq!(receipt.user_id, "@alice:example.com");
        assert_eq!(receipt.receipt_type, ReadReceiptType::Read);
    }

    #[test]
    fn test_receipt_type_equality() {
        assert_eq!(ReadReceiptType::Read, ReadReceiptType::Read);
        assert_ne!(ReadReceiptType::Read, ReadReceiptType::ReadPrivate);
    }
}
