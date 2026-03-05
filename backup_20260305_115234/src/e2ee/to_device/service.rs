use super::storage::ToDeviceStorage;
use crate::error::ApiError;
use crate::storage::UserStorage;
use serde_json::Value;

#[derive(Clone)]
pub struct ToDeviceService {
    storage: ToDeviceStorage,
    user_storage: Option<UserStorage>, // Made optional to avoid breaking tests if any
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

    pub async fn send_messages(&self, _sender_id: &str, messages: &Value) -> Result<(), ApiError> {
        if let Some(msg_map) = messages.as_object() {
            for (user_id, devices) in msg_map {
                // Check if user exists if user_storage is available
                if let Some(user_storage) = &self.user_storage {
                    if !user_storage
                        .user_exists(user_id)
                        .await
                        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?
                    {
                        ::tracing::warn!(
                            "Skipping to-device message for non-existent user: {}",
                            user_id
                        );
                        continue;
                    }
                }

                if let Some(device_map) = devices.as_object() {
                    for (device_id, content) in device_map {
                        // In a real implementation, we might want to extract the message type
                        // from the content or pass it separately.
                        let msg_type = content
                            .get("type")
                            .and_then(|t| t.as_str())
                            .unwrap_or("m.room.message");
                        self.storage
                            .add_message(user_id, device_id, msg_type, content.clone())
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
        self.storage.get_messages(user_id, device_id).await
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
