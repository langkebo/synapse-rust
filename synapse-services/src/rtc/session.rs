use super::metrics::RtcMetrics;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::error::ApiError;
use synapse_storage::matrixrtc::{
    CreateMembershipParams, CreateSessionParams, MatrixRTCStorage, RTCEncryptionKey, RTCMembership, RTCSession,
    SessionWithMemberships,
};

#[derive(Clone)]
pub struct RtcSessionService {
    storage: MatrixRTCStorage,
    cache: Arc<CacheManager>,
}

impl RtcSessionService {
    pub fn new(storage: MatrixRTCStorage, cache: Arc<CacheManager>) -> Self {
        Self { storage, cache }
    }

    pub async fn create_session(
        &self,
        room_id: String,
        session_id: String,
        application: String,
        call_id: Option<String>,
        creator: String,
        config: serde_json::Value,
    ) -> Result<RTCSession, ApiError> {
        let params =
            CreateSessionParams { room_id, session_id, application: application.clone(), call_id, creator, config };

        let session = self
            .storage
            .create_session(params)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create RTC session", &e))?;

        RtcMetrics::increment_session_created(&application);

        self.invalidate_room_cache(&session.room_id).await;

        Ok(session)
    }

    pub async fn get_session(&self, room_id: &str, session_id: &str) -> Result<Option<RTCSession>, ApiError> {
        let cache_key = format!("matrixrtc:session:{}:{}", room_id, session_id);

        if let Ok(Some(session)) = self.cache.get::<RTCSession>(&cache_key).await {
            return Ok(Some(session));
        }

        let session = self
            .storage
            .get_session(room_id, session_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get RTC session", &e))?;

        if let Some(ref s) = session {
            if let Err(e) = self.cache.set(&cache_key, s, 60).await {
                ::tracing::warn!(
                    room_id = %room_id,
                    session_id = %session_id,
                    cache_key = %cache_key,
                    error = %e,
                    "Failed to cache RTC session"
                );
            }
        }

        Ok(session)
    }

    pub async fn get_active_sessions_for_room(&self, room_id: &str) -> Result<Vec<RTCSession>, ApiError> {
        let cache_key = format!("matrixrtc:sessions:{}", room_id);

        if let Ok(Some(sessions)) = self.cache.get::<Vec<RTCSession>>(&cache_key).await {
            return Ok(sessions);
        }

        let sessions = self
            .storage
            .get_active_sessions_for_room(room_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get active sessions", &e))?;

        if let Err(e) = self.cache.set(&cache_key, &sessions, 30).await {
            ::tracing::warn!(room_id = %room_id, cache_key = %cache_key, error = %e, "Failed to cache active RTC sessions");
        }

        Ok(sessions)
    }

    pub async fn end_session(&self, room_id: &str, session_id: &str, user_id: &str) -> Result<(), ApiError> {
        let session = self.get_session(room_id, session_id).await?;

        match session {
            Some(s) if s.creator == user_id => {
                self.storage
                    .end_session(room_id, session_id)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to end session", &e))?;

                self.invalidate_room_cache(room_id).await;
                Ok(())
            }
            Some(_) => Err(ApiError::forbidden("Only session creator can end the session")),
            None => Err(ApiError::not_found("Session not found")),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_membership(
        &self,
        room_id: String,
        session_id: String,
        user_id: String,
        device_id: String,
        membership_id: String,
        application: String,
        call_id: Option<String>,
        foci_active: Option<String>,
        foci_preferred: Option<serde_json::Value>,
        application_data: Option<serde_json::Value>,
    ) -> Result<RTCMembership, ApiError> {
        let session = self.get_session(&room_id, &session_id).await?;
        if session.is_none() {
            return Err(ApiError::not_found("Session not found"));
        }

        let params = CreateMembershipParams {
            room_id,
            session_id,
            user_id,
            device_id,
            membership_id,
            application,
            call_id,
            foci_active,
            foci_preferred,
            application_data,
        };

        let membership = self
            .storage
            .create_membership(params)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create membership", &e))?;

        RtcMetrics::increment_membership_created();

        self.invalidate_session_cache(&membership.room_id, &membership.session_id).await;

        Ok(membership)
    }

    pub async fn get_memberships_for_session(
        &self,
        room_id: &str,
        session_id: &str,
    ) -> Result<Vec<RTCMembership>, ApiError> {
        let cache_key = format!("matrixrtc:memberships:{}:{}", room_id, session_id);

        if let Ok(Some(memberships)) = self.cache.get::<Vec<RTCMembership>>(&cache_key).await {
            return Ok(memberships);
        }

        let memberships = self
            .storage
            .get_memberships_for_session(room_id, session_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get memberships", &e))?;

        if let Err(e) = self.cache.set(&cache_key, &memberships, 30).await {
            ::tracing::warn!(
                room_id = %room_id,
                session_id = %session_id,
                cache_key = %cache_key,
                error = %e,
                "Failed to cache RTC memberships"
            );
        }

        Ok(memberships)
    }

    pub async fn end_membership(
        &self,
        room_id: &str,
        session_id: &str,
        user_id: &str,
        device_id: &str,
    ) -> Result<(), ApiError> {
        self.storage
            .end_membership(room_id, session_id, user_id, device_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to end membership", &e))?;

        self.invalidate_session_cache(room_id, session_id).await;

        Ok(())
    }

    pub async fn get_session_with_memberships(
        &self,
        room_id: &str,
        session_id: &str,
    ) -> Result<Option<SessionWithMemberships>, ApiError> {
        let result = self
            .storage
            .get_session_with_memberships(room_id, session_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get session with memberships", &e))?;

        Ok(result)
    }

    pub async fn store_encryption_key(
        &self,
        room_id: &str,
        session_id: &str,
        key_index: i32,
        key: &str,
        sender_user_id: &str,
        sender_device_id: &str,
    ) -> Result<RTCEncryptionKey, ApiError> {
        let encryption_key = self
            .storage
            .store_encryption_key(room_id, session_id, key_index, key, sender_user_id, sender_device_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to store encryption key", &e))?;

        self.invalidate_key_cache(room_id, session_id).await;

        Ok(encryption_key)
    }

    pub async fn get_encryption_keys(
        &self,
        room_id: &str,
        session_id: &str,
    ) -> Result<Vec<RTCEncryptionKey>, ApiError> {
        let cache_key = format!("matrixrtc:keys:{}:{}", room_id, session_id);

        if let Ok(Some(keys)) = self.cache.get::<Vec<RTCEncryptionKey>>(&cache_key).await {
            return Ok(keys);
        }

        let keys = self
            .storage
            .get_encryption_keys(room_id, session_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get encryption keys", &e))?;

        if let Err(e) = self.cache.set(&cache_key, &keys, 60).await {
            ::tracing::warn!(
                room_id = %room_id,
                session_id = %session_id,
                cache_key = %cache_key,
                error = %e,
                "Failed to cache RTC encryption keys"
            );
        }

        Ok(keys)
    }

    pub async fn cleanup_expired_memberships(&self) -> Result<u64, ApiError> {
        let count = self
            .storage
            .cleanup_expired_memberships()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to cleanup expired memberships", &e))?;

        Ok(count)
    }

    async fn invalidate_room_cache(&self, room_id: &str) {
        let cache_key = format!("matrixrtc:sessions:{room_id}");
        self.cache.delete(&cache_key).await;
    }

    async fn invalidate_session_cache(&self, room_id: &str, session_id: &str) {
        let session_cache_key = format!("matrixrtc:session:{room_id}:{session_id}");
        self.cache.delete(&session_cache_key).await;

        let membership_cache_key = format!("matrixrtc:memberships:{room_id}:{session_id}");
        self.cache.delete(&membership_cache_key).await;
    }

    async fn invalidate_key_cache(&self, room_id: &str, session_id: &str) {
        let cache_key = format!("matrixrtc:keys:{room_id}:{session_id}");
        self.cache.delete(&cache_key).await;
    }
}

pub fn to_matrix_event(session: &RTCSession, memberships: &[RTCMembership]) -> serde_json::Value {
    serde_json::json!({
        "type": "org.matrix.msc3401.call",
        "room_id": session.room_id,
        "content": {
            "session_id": session.session_id,
            "application": session.application,
            "call_id": session.call_id,
            "creator": session.creator,
            "config": session.config,
            "memberships": memberships.iter().map(|m| {
                serde_json::json!({
                    "membership_id": m.membership_id,
                    "user_id": m.user_id,
                    "device_id": m.device_id,
                    "application": m.application,
                    "foci_active": m.foci_active,
                    "foci_preferred": m.foci_preferred,
                    "application_data": m.application_data,
                })
            }).collect::<Vec<_>>()
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_session() -> RTCSession {
        RTCSession {
            id: 1,
            room_id: "!room:ex.com".into(),
            session_id: "sess1".into(),
            application: "m.call".into(),
            call_id: Some("call1".into()),
            creator: "@alice:ex.com".into(),
            created_ts: 1000,
            updated_ts: 2000,
            is_active: true,
            config: json!({"sdp_semantics": "unified-plan"}),
        }
    }

    fn make_membership(user_id: &str, device_id: &str, membership_id: &str) -> RTCMembership {
        RTCMembership {
            id: 1,
            room_id: "!room:ex.com".into(),
            session_id: "sess1".into(),
            user_id: user_id.into(),
            device_id: device_id.into(),
            membership_id: membership_id.into(),
            application: "m.call".into(),
            call_id: Some("call1".into()),
            created_ts: 1000,
            updated_ts: 2000,
            expires_at: Some(9999),
            foci_active: Some("true".into()),
            foci_preferred: Some(json!(["focus1"])),
            application_data: Some(json!({"streams": []})),
            is_active: true,
        }
    }

    #[test]
    fn to_matrix_event_basic_structure() {
        let session = make_session();
        let result = to_matrix_event(&session, &[]);

        assert_eq!(result["type"], "org.matrix.msc3401.call");
        assert_eq!(result["room_id"], "!room:ex.com");
        assert_eq!(result["content"]["session_id"], "sess1");
        assert_eq!(result["content"]["application"], "m.call");
        assert_eq!(result["content"]["call_id"], "call1");
        assert_eq!(result["content"]["creator"], "@alice:ex.com");
        assert_eq!(result["content"]["config"]["sdp_semantics"], "unified-plan");
    }

    #[test]
    fn to_matrix_event_with_single_membership() {
        let session = make_session();
        let membership = make_membership("@bob:ex.com", "DEVICE1", "mem1");
        let result = to_matrix_event(&session, &[membership]);

        let memberships = result["content"]["memberships"].as_array().unwrap();
        assert_eq!(memberships.len(), 1);
        assert_eq!(memberships[0]["user_id"], "@bob:ex.com");
        assert_eq!(memberships[0]["device_id"], "DEVICE1");
        assert_eq!(memberships[0]["membership_id"], "mem1");
        assert_eq!(memberships[0]["foci_active"], "true");
        assert_eq!(memberships[0]["foci_preferred"], json!(["focus1"]));
    }

    #[test]
    fn to_matrix_event_with_multiple_memberships() {
        let session = make_session();
        let m1 = make_membership("@bob:ex.com", "D1", "mem1");
        let m2 = make_membership("@charlie:ex.com", "D2", "mem2");
        let result = to_matrix_event(&session, &[m1, m2]);

        let memberships = result["content"]["memberships"].as_array().unwrap();
        assert_eq!(memberships.len(), 2);
    }

    #[test]
    fn to_matrix_event_empty_memberships() {
        let session = make_session();
        let result = to_matrix_event(&session, &[]);
        let memberships = result["content"]["memberships"].as_array().unwrap();
        assert!(memberships.is_empty());
    }

    #[test]
    fn to_matrix_event_none_foci_and_app_data() {
        let session = make_session();
        let mut m = make_membership("@alice:ex.com", "D1", "mem1");
        m.foci_active = None;
        m.foci_preferred = None;
        m.application_data = None;
        let result = to_matrix_event(&session, &[m]);
        let mem = &result["content"]["memberships"].as_array().unwrap()[0];
        assert!(mem["foci_active"].is_null());
        assert!(mem["foci_preferred"].is_null());
        assert!(mem["application_data"].is_null());
    }
}
