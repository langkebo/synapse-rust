use crate::e2ee::key_request::models::{KeyRequestInfo, KeyShareResponse};
use crate::e2ee::key_request::storage::KeyRequestStorage;
use crate::e2ee::megolm::MegolmService;
use crate::error::ApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyRequestStatusFilter {
    Pending,
    Fulfilled,
    Cancelled,
    All,
}

impl KeyRequestStatusFilter {
    pub fn from_query(status: Option<&str>) -> Result<Self, ApiError> {
        match status.unwrap_or("all") {
            "pending" => Ok(Self::Pending),
            "fulfilled" => Ok(Self::Fulfilled),
            "cancelled" | "canceled" | "cancellation" => Ok(Self::Cancelled),
            "all" => Ok(Self::All),
            other => Err(ApiError::bad_request(format!(
                "Unsupported room key request status: {}",
                other
            ))),
        }
    }
}

#[derive(Clone)]
pub struct KeyRequestService {
    storage: KeyRequestStorage,
    megolm_service: MegolmService,
}

impl KeyRequestService {
    pub fn new(storage: KeyRequestStorage, megolm_service: MegolmService) -> Self {
        Self {
            storage,
            megolm_service,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_request(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        session_id: &str,
        algorithm: &str,
        request_type: Option<&str>,
        request_id: Option<&str>,
    ) -> Result<KeyRequestInfo, ApiError> {
        let request_id = request_id
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let action = request_type.unwrap_or("request");

        let request_info = KeyRequestInfo {
            request_id: request_id.clone(),
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            room_id: room_id.to_string(),
            session_id: session_id.to_string(),
            algorithm: algorithm.to_string(),
            action: action.to_string(),
            created_ts: chrono::Utc::now().timestamp_millis(),
            is_fulfilled: false,
            fulfilled_by_device: None,
            fulfilled_ts: None,
        };

        self.storage.create_request(&request_info).await?;
        Ok(request_info)
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
        let request = self
            .create_request(
                user_id,
                device_id,
                room_id,
                session_id,
                algorithm,
                Some("request"),
                None,
            )
            .await?;
        Ok(request.request_id)
    }

    /// Fulfill a key request by sharing the session key
    ///
    /// NOTE: This method is intentionally not implemented.
    /// To fully implement this feature, we need to:
    /// 1. Extract the actual Megolm session key from the session
    /// 2. Encrypt the session key for the requesting device
    /// 3. Handle key forwarding chains properly
    ///
    /// This method is not currently called by any route handler.
    /// It's a placeholder for future E2EE key sharing functionality.
    pub async fn fulfill_request(
        &self,
        request_id: &str,
        device_id: &str,
    ) -> Result<Option<KeyShareResponse>, ApiError> {
        let request = self.storage.get_request(request_id).await?;

        if let Some(request) = request {
            if request.is_fulfilled {
                return Ok(None);
            }

            if let Ok(sessions) = self
                .megolm_service
                .get_room_sessions(&request.room_id)
                .await
            {
                if sessions.iter().any(|s| s.session_id == request.session_id) {
                    let _ = (request_id, device_id);
                    let _ = request;
                    return Err(ApiError::unrecognized(
                        "E2EE room key request fulfillment is not supported".to_string(),
                    ));
                }
            }
        }

        Ok(None)
    }

    pub async fn cancel_request(&self, request_id: &str) -> Result<(), ApiError> {
        self.storage.cancel_request(request_id).await?;
        Ok(())
    }

    pub async fn delete_request(&self, request_id: &str) -> Result<(), ApiError> {
        self.storage.delete_request(request_id).await
    }

    pub async fn get_request(&self, request_id: &str) -> Result<Option<KeyRequestInfo>, ApiError> {
        self.storage.get_request(request_id).await
    }

    pub async fn get_requests(
        &self,
        user_id: &str,
        status: Option<&str>,
    ) -> Result<Vec<KeyRequestInfo>, ApiError> {
        let status_filter = KeyRequestStatusFilter::from_query(status)?;
        let requests = self.storage.get_requests_for_user(user_id).await?;
        Ok(requests
            .into_iter()
            .filter(|request| request_matches_status(request, status_filter))
            .collect())
    }

    pub async fn get_pending_requests(
        &self,
        user_id: Option<&str>,
    ) -> Result<Vec<KeyRequestInfo>, ApiError> {
        let requests = match user_id {
            Some(uid) => self.storage.get_requests_for_user(uid).await?,
            None => self.storage.get_all_pending_requests().await?,
        };

        Ok(requests
            .into_iter()
            .filter(|request| request_matches_status(request, KeyRequestStatusFilter::Pending))
            .collect())
    }

    pub async fn process_outgoing_key_requests(&self) -> Result<(), ApiError> {
        for request in self.get_pending_requests(None).await? {
            tracing::debug!(
                "Processing key request: {} for room {} session {}",
                request.request_id,
                request.room_id,
                request.session_id
            );
        }

        Ok(())
    }

    pub async fn cleanup_old_requests(&self, max_age_hours: i64) -> Result<u64, ApiError> {
        let cutoff_ts = chrono::Utc::now().timestamp_millis() - (max_age_hours * 3600 * 1000);
        self.storage.delete_old_requests(cutoff_ts).await
    }
}

fn request_matches_status(request: &KeyRequestInfo, status: KeyRequestStatusFilter) -> bool {
    match status {
        KeyRequestStatusFilter::Pending => !request.is_fulfilled,
        KeyRequestStatusFilter::Fulfilled => {
            request.is_fulfilled
                && request.action != "cancellation"
                && request.action != "cancelled"
        }
        KeyRequestStatusFilter::Cancelled => {
            request.action == "cancellation" || request.action == "cancelled"
        }
        KeyRequestStatusFilter::All => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{request_matches_status, KeyRequestInfo, KeyRequestStatusFilter};

    fn sample_request(action: &str, is_fulfilled: bool) -> KeyRequestInfo {
        KeyRequestInfo {
            request_id: "req-1".to_string(),
            user_id: "@alice:example.org".to_string(),
            device_id: "DEVICE".to_string(),
            room_id: "!room:example.org".to_string(),
            session_id: "session".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            action: action.to_string(),
            created_ts: 1,
            is_fulfilled,
            fulfilled_by_device: None,
            fulfilled_ts: None,
        }
    }

    #[test]
    fn matches_pending_status() {
        assert!(request_matches_status(
            &sample_request("request", false),
            KeyRequestStatusFilter::Pending,
        ));
        assert!(!request_matches_status(
            &sample_request("request", true),
            KeyRequestStatusFilter::Pending,
        ));
    }

    #[test]
    fn matches_cancelled_status() {
        assert!(request_matches_status(
            &sample_request("cancellation", true),
            KeyRequestStatusFilter::Cancelled,
        ));
        assert!(request_matches_status(
            &sample_request("cancelled", true),
            KeyRequestStatusFilter::Cancelled,
        ));
        assert!(!request_matches_status(
            &sample_request("request", true),
            KeyRequestStatusFilter::Cancelled,
        ));
    }

    #[test]
    fn matches_fulfilled_status() {
        assert!(request_matches_status(
            &sample_request("request", true),
            KeyRequestStatusFilter::Fulfilled,
        ));
        assert!(!request_matches_status(
            &sample_request("cancellation", true),
            KeyRequestStatusFilter::Fulfilled,
        ));
    }
}
