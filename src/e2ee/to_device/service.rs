use super::storage::ToDeviceStorage;
use crate::error::ApiError;
use serde_json::Value;

#[derive(Clone)]
pub struct ToDeviceService {
    storage: ToDeviceStorage,
}

impl ToDeviceService {
    pub fn new(storage: ToDeviceStorage) -> Self {
        Self { storage }
    }

    pub async fn send_messages(&self, _sender_id: &str, messages: &Value) -> Result<(), ApiError> {
        if let Some(msg_map) = messages.as_object() {
            for (user_id, devices) in msg_map {
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
