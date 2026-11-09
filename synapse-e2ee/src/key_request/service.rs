use crate::key_request::models::{KeyRequestInfo, KeyRequestPagination};
use crate::key_request::storage::KeyRequestStorage;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The `KeyRequestStatusFilter` enum.
pub enum KeyRequestStatusFilter {
    /// The `Pending` variant.
    /// The `Fulfilled` variant.
    /// The `Cancelled` variant.
    /// The `All` variant.
    Pending,
    /// The `Fulfilled` variant.
    /// The `Cancelled` variant.
    /// The `All` variant.
    Fulfilled,
    /// The `Cancelled` variant.
    /// The `All` variant.
    Cancelled,
    /// The `All` variant.
    All,
}

/// Implementation of [`KeyRequestStatusFilter`] methods.
impl KeyRequestStatusFilter {
    /// See [`from_query`].
    pub fn from_query(status: Option<&str>) -> Result<Self, ApiError> {
        match status.unwrap_or("all") {
            "pending" => Ok(Self::Pending),
            "fulfilled" => Ok(Self::Fulfilled),
            "cancelled" | "canceled" | "cancellation" => Ok(Self::Cancelled),
            "all" => Ok(Self::All),
            other => Err(ApiError::bad_request(format!("Unsupported room key request status: {other}"))),
        }
    }
}

#[derive(Clone)]
/// The `KeyRequestService` type.
pub struct KeyRequestService {
    storage: KeyRequestStorage,
}

/// Implementation of [`KeyRequestService`] methods.
impl KeyRequestService {
    /// See [`new`].
    pub fn new(storage: KeyRequestStorage) -> Self {
        Self { storage }
    }

    // reason: `create_request` 是协议/存储字段的直传签名：参数逐个来自调用点，打包成结构体只会把同一份字段列表抄两遍
    #[allow(clippy::too_many_arguments)]
    /// See [`create_request`].
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
        let request_id = request_id.map_or_else(|| uuid::Uuid::new_v4().to_string(), ToOwned::to_owned);
        let action = request_type.unwrap_or("request");

        let request_info = KeyRequestInfo {
            request_id: request_id.clone(),
            user_id: user_id.to_string(),
            device_id: device_id.to_string(),
            room_id: room_id.to_string(),
            session_id: session_id.to_string(),
            algorithm: algorithm.to_string(),
            action: action.to_string(),
            created_ts: current_timestamp_millis(),
            is_fulfilled: false,
            fulfilled_by_device: None,
            fulfilled_ts: None,
        };

        self.storage.create_request(&request_info).await?;
        Ok(request_info)
    }

    // reason: `create_share_request` 是协议/存储字段的直传签名：参数逐个来自调用点，打包成结构体只会把同一份字段列表抄两遍
    #[allow(clippy::too_many_arguments)]
    /// See [`create_share_request`].
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
        let request =
            self.create_request(user_id, device_id, room_id, session_id, algorithm, Some("request"), None).await?;
        Ok(request.request_id)
    }

    /// See [`cancel_request`].
    pub async fn cancel_request(&self, request_id: &str) -> Result<(), ApiError> {
        self.storage.cancel_request(request_id).await?;
        Ok(())
    }

    /// 把同用户、同设备、同房间会话的待处理密钥请求标记为已兑现。
    ///
    /// D5：兑现由持有密钥的设备通过 to-device `m.forwarded_room_key` 完成 ——
    /// 服务端只记录兑现元数据（`is_fulfilled` / `fulfilled_by_device` /
    /// `fulfilled_ts`），**从不**接触会话密钥明文（S-12）。
    ///
    /// O-3：禁止同设备自我兑现 —— `fulfilled_by_device` 与请求发起设备相同的
    /// 请求会被跳过。
    pub async fn fulfill_pending_request(
        &self,
        user_id: &str,
        device_id: &str,
        room_id: &str,
        session_id: &str,
        fulfilled_by_device: &str,
    ) -> Result<u64, ApiError> {
        let requests = self.storage.get_requests_for_user(user_id).await?;
        let mut fulfilled = 0;
        for request in requests {
            if request.is_fulfilled
                || request.device_id != device_id
                || request.room_id != room_id
                || request.session_id != session_id
                || request.device_id == fulfilled_by_device
                || request.action == "cancellation"
                || request.action == "cancelled"
            {
                continue;
            }
            self.storage.fulfill_request(&request.request_id, fulfilled_by_device).await?;
            fulfilled += 1;
        }
        Ok(fulfilled)
    }

    /// See [`delete_request`].
    pub async fn delete_request(&self, request_id: &str) -> Result<(), ApiError> {
        self.storage.delete_request(request_id).await
    }

    /// See [`get_request`].
    pub async fn get_request(&self, request_id: &str) -> Result<Option<KeyRequestInfo>, ApiError> {
        self.storage.get_request(request_id).await
    }

    /// See [`get_requests`].
    pub async fn get_requests(&self, user_id: &str, status: Option<&str>) -> Result<Vec<KeyRequestInfo>, ApiError> {
        let status_filter = KeyRequestStatusFilter::from_query(status)?;
        let requests = self.storage.get_requests_for_user(user_id).await?;
        Ok(requests.into_iter().filter(|request| request_matches_status(request, status_filter)).collect())
    }

    /// See [`get_requests_paginated`].
    pub async fn get_requests_paginated(
        &self,
        pagination: KeyRequestPagination<'_>,
    ) -> Result<Vec<KeyRequestInfo>, ApiError> {
        self.storage.get_requests_paginated(pagination).await
    }

    /// See [`get_pending_requests`].
    pub async fn get_pending_requests(&self, user_id: Option<&str>) -> Result<Vec<KeyRequestInfo>, ApiError> {
        let requests = match user_id {
            Some(uid) => self.storage.get_requests_for_user(uid).await?,
            None => self.storage.get_all_pending_requests().await?,
        };

        Ok(requests
            .into_iter()
            .filter(|request| request_matches_status(request, KeyRequestStatusFilter::Pending))
            .collect())
    }

    /// See [`process_outgoing_key_requests`].
    pub async fn process_outgoing_key_requests(&self) -> Result<(), ApiError> {
        let requests = self.get_pending_requests(None).await?;

        for request in requests {
            tracing::info!(
                "Processing outgoing key request: {} for room {} session {} from device {}",
                request.request_id,
                request.room_id,
                request.session_id,
                request.device_id
            );

            if let Err(e) = self.storage.update_request_status(&request.request_id, "sent").await {
                tracing::warn!("Failed to update key request {} status: {}", request.request_id, e);
            }
        }

        Ok(())
    }

    /// See [`cleanup_old_requests`].
    pub async fn cleanup_old_requests(&self, max_age_hours: i64) -> Result<u64, ApiError> {
        let cutoff_ts = current_timestamp_millis() - (max_age_hours * 3600 * 1000);
        self.storage.delete_old_requests(cutoff_ts).await
    }
}

fn request_matches_status(request: &KeyRequestInfo, status: KeyRequestStatusFilter) -> bool {
    match status {
        KeyRequestStatusFilter::Pending => !request.is_fulfilled,
        KeyRequestStatusFilter::Fulfilled => {
            request.is_fulfilled && request.action != "cancellation" && request.action != "cancelled"
        }
        KeyRequestStatusFilter::Cancelled => request.action == "cancellation" || request.action == "cancelled",
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
        assert!(request_matches_status(&sample_request("request", false), KeyRequestStatusFilter::Pending,));
        assert!(!request_matches_status(&sample_request("request", true), KeyRequestStatusFilter::Pending,));
    }

    #[test]
    fn matches_cancelled_status() {
        assert!(request_matches_status(&sample_request("cancellation", true), KeyRequestStatusFilter::Cancelled,));
        assert!(request_matches_status(&sample_request("cancelled", true), KeyRequestStatusFilter::Cancelled,));
        assert!(!request_matches_status(&sample_request("request", true), KeyRequestStatusFilter::Cancelled,));
    }

    #[test]
    fn matches_fulfilled_status() {
        assert!(request_matches_status(&sample_request("request", true), KeyRequestStatusFilter::Fulfilled,));
        assert!(!request_matches_status(&sample_request("cancellation", true), KeyRequestStatusFilter::Fulfilled,));
    }

    // ── KeyRequestStatusFilter::from_query ──────────────────────

    #[test]
    fn status_filter_from_query_pending() {
        assert_eq!(KeyRequestStatusFilter::from_query(Some("pending")).unwrap(), KeyRequestStatusFilter::Pending);
    }

    #[test]
    fn status_filter_from_query_fulfilled() {
        assert_eq!(KeyRequestStatusFilter::from_query(Some("fulfilled")).unwrap(), KeyRequestStatusFilter::Fulfilled);
    }

    #[test]
    fn status_filter_from_query_cancelled_variants() {
        assert_eq!(KeyRequestStatusFilter::from_query(Some("cancelled")).unwrap(), KeyRequestStatusFilter::Cancelled);
        assert_eq!(KeyRequestStatusFilter::from_query(Some("canceled")).unwrap(), KeyRequestStatusFilter::Cancelled);
        assert_eq!(
            KeyRequestStatusFilter::from_query(Some("cancellation")).unwrap(),
            KeyRequestStatusFilter::Cancelled
        );
    }

    #[test]
    fn status_filter_from_query_all() {
        assert_eq!(KeyRequestStatusFilter::from_query(Some("all")).unwrap(), KeyRequestStatusFilter::All);
    }

    #[test]
    fn status_filter_from_query_none_defaults_to_all() {
        assert_eq!(KeyRequestStatusFilter::from_query(None).unwrap(), KeyRequestStatusFilter::All);
    }

    #[test]
    fn status_filter_from_query_empty_string_rejects() {
        // Some("") does NOT default to "all" — the empty string hits the catch-all error arm.
        let result = KeyRequestStatusFilter::from_query(Some(""));
        assert!(result.is_err(), "empty string is not a valid status");
    }

    #[test]
    fn status_filter_from_query_unknown_rejects() {
        let result = KeyRequestStatusFilter::from_query(Some("invalid_status"));
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("Unsupported room key request status"));
    }

    #[test]
    fn status_filter_from_query_case_sensitive() {
        // Status matching is case-sensitive: "Pending" ≠ "pending"
        let result = KeyRequestStatusFilter::from_query(Some("Pending"));
        assert!(result.is_err(), "case-sensitive matching should reject 'Pending'");
    }

    // ── KeyRequestInfo construction ─────────────────────────────

    #[test]
    fn key_request_info_all_fields() {
        let info = KeyRequestInfo {
            request_id: "req-test".to_string(),
            user_id: "@alice:example.org".to_string(),
            device_id: "DEVICE1".to_string(),
            room_id: "!room:test.org".to_string(),
            session_id: "session1".to_string(),
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            action: "request".to_string(),
            created_ts: 1_700_000_000_000,
            is_fulfilled: false,
            fulfilled_by_device: None,
            fulfilled_ts: None,
        };
        assert_eq!(info.request_id, "req-test");
        assert!(!info.is_fulfilled);
        assert_eq!(info.fulfilled_by_device, None);
    }
}

// ---------------------------------------------------------------------------
// D5：密钥请求兑现语义
//
// 兑现由持有密钥的设备用 to-device `m.forwarded_room_key` 触发，服务端只写
// 元数据。这里锁住「哪些请求会被兑现」的判定：同用户 + 同请求设备 + 同房间
// 会话 + 未完成 + 非撤销 + 禁止同设备自我兑现（O-3）。
// ---------------------------------------------------------------------------
#[cfg(test)]
mod db_tests {
    use super::*;
    use synapse_common::test_isolation::IsolatedTestPool;

    /// 与 `key_request/storage.rs` 的副本逐字节一致（共享模板名是内容指纹）。
    const BASELINE_SQL: &str = include_str!("../../../migrations/00000000_unified_schema_v12.sql");

    const USER: &str = "@alice:example.com";
    const REQUESTER_DEVICE: &str = "DEV1";
    const ROOM_ID: &str = "!kr_room:example.com";

    async fn service() -> (IsolatedTestPool, KeyRequestService) {
        let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated baseline test pool");
        let pool = (*isolated.pool()).clone();
        let service = KeyRequestService::new(KeyRequestStorage::new(&pool));
        (isolated, service)
    }

    async fn seed(
        service: &KeyRequestService,
        request_id: &str,
        device_id: &str,
        room_id: &str,
        session_id: &str,
        action: &str,
    ) {
        service
            .create_request(
                USER,
                device_id,
                room_id,
                session_id,
                "m.megolm.v1.aes-sha2",
                Some(action),
                Some(request_id),
            )
            .await
            .expect("create_request");
    }

    #[tokio::test]
    async fn fulfill_pending_request_marks_matching_request_once() {
        let (_isolated, service) = service().await;
        seed(&service, "req-1", REQUESTER_DEVICE, ROOM_ID, "sess1", "request").await;

        let count = service
            .fulfill_pending_request(USER, REQUESTER_DEVICE, ROOM_ID, "sess1", "DEV2")
            .await
            .expect("fulfill_pending_request");
        assert_eq!(count, 1, "匹配的待处理请求必须被兑现");

        let request = service.get_request("req-1").await.expect("get_request").expect("row must exist");
        assert!(request.is_fulfilled);
        assert_eq!(request.fulfilled_by_device.as_deref(), Some("DEV2"));
        assert!(request.fulfilled_ts.is_some(), "兑现时间必须写入");

        // 幂等：重复兑现同一请求不会再次计数。
        let again = service
            .fulfill_pending_request(USER, REQUESTER_DEVICE, ROOM_ID, "sess1", "DEV2")
            .await
            .expect("fulfill_pending_request again");
        assert_eq!(again, 0);
    }

    #[tokio::test]
    async fn fulfill_pending_request_skips_non_matching_requests() {
        let (_isolated, service) = service().await;
        seed(&service, "other-session", REQUESTER_DEVICE, ROOM_ID, "sess-other", "request").await;
        seed(&service, "other-room", REQUESTER_DEVICE, "!other:example.com", "sess1", "request").await;
        seed(&service, "other-device", "DEV9", ROOM_ID, "sess1", "request").await;
        seed(&service, "cancelled", REQUESTER_DEVICE, ROOM_ID, "sess1", "cancellation").await;

        let count = service
            .fulfill_pending_request(USER, REQUESTER_DEVICE, ROOM_ID, "sess1", "DEV2")
            .await
            .expect("fulfill_pending_request");
        assert_eq!(count, 0, "只有同设备 + 同房间 + 同会话的未撤销请求才允许兑现");

        for request_id in ["other-session", "other-room", "other-device", "cancelled"] {
            let request = service.get_request(request_id).await.expect("get_request").expect("row must exist");
            assert_eq!(request.fulfilled_by_device, None, "{request_id} 不应被兑现");
        }
    }

    #[tokio::test]
    async fn fulfill_pending_request_rejects_self_fulfillment() {
        let (_isolated, service) = service().await;
        seed(&service, "req-self", REQUESTER_DEVICE, ROOM_ID, "sess1", "request").await;

        // O-3：请求发起设备不能自己把请求标记为已兑现。
        let count = service
            .fulfill_pending_request(USER, REQUESTER_DEVICE, ROOM_ID, "sess1", REQUESTER_DEVICE)
            .await
            .expect("fulfill_pending_request");
        assert_eq!(count, 0);

        let request = service.get_request("req-self").await.expect("get_request").expect("row must exist");
        assert!(!request.is_fulfilled, "同设备自我兑现必须被拒绝");
    }
}
