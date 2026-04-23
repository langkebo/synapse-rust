use super::storage::{ToDeviceMessage, ToDeviceStorage};
use crate::error::ApiError;
use crate::storage::UserStorage;
use serde_json::Value;

const TRANSACTION_MAX_AGE_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Clone)]
pub struct ToDeviceService {
    storage: ToDeviceStorage,
    user_storage: Option<UserStorage>,
}

impl ToDeviceService {
    pub fn new(storage: ToDeviceStorage) -> Self {
        Self {
            storage,
            user_storage: None,
        }
    }

    pub fn with_user_storage(mut self, user_storage: UserStorage) -> Self {
        self.user_storage = Some(user_storage);
        self
    }

    pub async fn send_messages(
        &self,
        sender_user_id: &str,
        sender_device_id: &str,
        event_type: &str,
        message_id: Option<&str>,
        messages: &Value,
    ) -> Result<(), ApiError> {
        if let Some(mid) = message_id {
            if self
                .storage
                .is_duplicate_transaction(sender_user_id, sender_device_id, mid)
                .await?
            {
                tracing::debug!(
                    "Duplicate to-device transaction {} from {}:{}",
                    mid,
                    sender_user_id,
                    sender_device_id
                );
                return Ok(());
            }

            self.storage
                .record_transaction(sender_user_id, sender_device_id, mid)
                .await?;

            let _ = self
                .storage
                .cleanup_old_transactions(TRANSACTION_MAX_AGE_MS)
                .await;
        }

        if let Some(msg_map) = messages.as_object() {
            for (user_id, devices) in msg_map {
                if let Some(user_storage) = &self.user_storage {
                    if !user_storage
                        .user_exists(user_id)
                        .await
                        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
                    {
                        tracing::warn!(
                            "Skipping to-device message for non-existent user: {}",
                            user_id
                        );
                        continue;
                    }
                }

                if let Some(device_map) = devices.as_object() {
                    for (device_id, content) in device_map {
                        self.storage
                            .add_message(ToDeviceMessage {
                                sender_user_id,
                                sender_device_id,
                                recipient_user_id: user_id,
                                recipient_device_id: device_id,
                                event_type,
                                message_id,
                                content: content.clone(),
                            })
                            .await?;
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn get_messages_for_sync(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Vec<Value>, ApiError> {
        self.storage
            .get_and_delete_messages(user_id, device_id)
            .await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn test_to_device_message_structure() {
        let messages = json!({
            "@alice:example.com": {
                "DEVICE1": {
                    "type": "m.room_key",
                    "content": {
                        "algorithm": "m.megolm.v1.aes-sha2"
                    }
                }
            }
        });
        assert!(messages["@alice:example.com"]["DEVICE1"]["type"].is_string());
    }

    #[test]
    fn test_to_device_multiple_devices() {
        let messages = json!({
            "@bob:example.com": {
                "DEVICE1": {"type": "m.test"},
                "DEVICE2": {"type": "m.test"}
            }
        });
        let devices = messages["@bob:example.com"].as_object().unwrap();
        assert_eq!(devices.len(), 2);
    }

    #[test]
    fn test_to_device_empty_messages() {
        let messages = json!({});
        assert!(messages.as_object().unwrap().is_empty());
    }
}
