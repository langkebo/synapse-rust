use crate::e2ee::key_request::models::{KeyRequestInfo, KeyShareResponse};
use crate::e2ee::key_request::storage::KeyRequestStorage;
use crate::e2ee::megolm::MegolmService;
use crate::error::ApiError;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;

pub struct KeyRequestService {
    storage: KeyRequestStorage,
    megolm_service: Arc<RwLock<Option<MegolmService>>>,
    pending_requests: Arc<RwLock<HashMap<String, KeyRequestInfo>>>,
}

impl KeyRequestService {
    pub fn new(storage: KeyRequestStorage) -> Self {
        Self {
            storage,
            megolm_service: Arc::new(RwLock::new(None)),
            pending_requests: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn set_megolm_service(&self, service: MegolmService) {
        let mut megolm = self.megolm_service.write().await;
        *megolm = Some(service);
    }

    pub async fn create_request(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        session_id: &str,
        algorithm: &str,
    ) -> Result<String, ApiError> {
        let request_id = format!("{}", uuid::Uuid::new_v4());

        let action = match algorithm {
            "m.megolm.v1.aes-sha2" => "request",
            _ => "request",
        };

        let request_info = KeyRequestInfo {
            request_id: request_id.clone(),
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            room_id: room_id.to_string(),
            session_id: session_id.to_string(),
            algorithm: algorithm.to_string(),
            action: action.to_string(),
            created_ts: chrono::Utc::now().timestamp_millis(),
            fulfilled: false,
            fulfilled_by_device: None,
            fulfilled_ts: None,
        };

        self.storage.create_request(&request_info).await?;

        {
            let mut pending = self.pending_requests.write().await;
            pending.insert(request_id.clone(), request_info);
        }

        Ok(request_id)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_share_request(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        session_id: &str,
        _sender_key: &str,
        algorithm: &str,
        _requesting_device_id: &str,
    ) -> Result<String, ApiError> {
        self.create_request(user_id, device_id, room_id, session_id, algorithm).await
    }

    pub async fn fulfill_request(
        &self,
        request_id: &str,
        device_id: &str,
    ) -> Result<Option<KeyShareResponse>, ApiError> {
        let request = self.storage.get_request(request_id).await?;

        if let Some(request) = request {
            if request.fulfilled {
                return Ok(None);
            }

            let megolm = self.megolm_service.read().await;
            if let Some(ref megolm_service) = *megolm {
                if let Ok(sessions) = megolm_service.get_room_sessions(&request.room_id).await {
                    if sessions.iter().any(|s| s.session_id == request.session_id) {
                        self.storage.fulfill_request(request_id, device_id).await?;

                        {
                            let mut pending = self.pending_requests.write().await;
                            pending.remove(request_id);
                        }

                        return Ok(Some(KeyShareResponse {
                            room_id: request.room_id,
                            session_id: request.session_id,
                            session_key: "session_key_placeholder".to_string(),
                            sender_key: request.user_id.clone(),
                            algorithm: request.algorithm,
                            forwarding_curve25519_key: None,
                        }));
                    }
                }
            }
        }

        Ok(None)
    }

    pub async fn cancel_request(&self, request_id: &str) -> Result<(), ApiError> {
        self.storage.cancel_request(request_id).await?;

        {
            let mut pending = self.pending_requests.write().await;
            pending.remove(request_id);
        }

        Ok(())
    }

    pub async fn get_request(&self, request_id: &str) -> Result<Option<KeyRequestInfo>, ApiError> {
        self.storage.get_request(request_id).await
    }

    pub async fn get_pending_requests(&self, user_id: Option<&str>) -> Result<Vec<KeyRequestInfo>, ApiError> {
        match user_id {
            Some(uid) => self.storage.get_requests_for_user(uid).await,
            None => self.storage.get_all_pending_requests().await,
        }
    }

    pub async fn process_outgoing_key_requests(&self) -> Result<(), ApiError> {
        let pending = self.pending_requests.read().await;

        for (request_id, request) in pending.iter() {
            if !request.fulfilled {
                tracing::debug!(
                    "Processing key request: {} for room {} session {}",
                    request_id,
                    request.room_id,
                    request.session_id
                );
            }
        }

        Ok(())
    }

    pub async fn cleanup_old_requests(&self, max_age_hours: i64) -> Result<u64, ApiError> {
        let cutoff_ts = chrono::Utc::now().timestamp_millis() - (max_age_hours * 3600 * 1000);
        self.storage.delete_old_requests(cutoff_ts).await
    }
}
