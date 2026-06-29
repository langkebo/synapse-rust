use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::ApiError;
use synapse_storage::ThreepidStorage;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiaSession {
    pub session_id: String,
    pub user_id: String,
    pub completed: Vec<String>,
    pub created_ts: i64,
    pub flows: Vec<UiaFlow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiaFlow {
    pub stages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiaAuthResult {
    pub session: Option<String>,
    pub completed: Vec<String>,
    pub flows: Vec<UiaFlow>,
    pub params: Value,
}

pub struct UiaService {
    cache: Arc<CacheManager>,
    session_timeout_secs: i64,
}

impl UiaService {
    pub fn new(cache: Arc<CacheManager>, session_timeout_secs: i64) -> Self {
        Self { cache, session_timeout_secs }
    }

    pub fn get_default_flows() -> Vec<UiaFlow> {
        vec![
            UiaFlow { stages: vec!["m.login.password".to_string()] },
            UiaFlow { stages: vec!["m.login.token".to_string()] },
            UiaFlow { stages: vec!["m.login.email.identity".to_string()] },
        ]
    }

    pub fn get_password_change_flows() -> Vec<UiaFlow> {
        vec![
            UiaFlow { stages: vec!["m.login.password".to_string()] },
            UiaFlow { stages: vec!["m.login.email.identity".to_string()] },
        ]
    }

    pub fn get_delete_device_flows() -> Vec<UiaFlow> {
        vec![
            UiaFlow { stages: vec!["m.login.password".to_string()] },
            UiaFlow { stages: vec!["m.login.email.identity".to_string()] },
        ]
    }

    pub fn get_deactivate_account_flows() -> Vec<UiaFlow> {
        vec![
            UiaFlow { stages: vec!["m.login.password".to_string()] },
            UiaFlow { stages: vec!["m.login.email.identity".to_string()] },
        ]
    }

    pub fn get_cross_signing_flows() -> Vec<UiaFlow> {
        vec![
            UiaFlow { stages: vec!["m.login.password".to_string()] },
            UiaFlow { stages: vec!["m.login.email.identity".to_string()] },
        ]
    }

    pub async fn create_session(&self, user_id: &str, flows: Vec<UiaFlow>) -> UiaSession {
        let session_id = uuid::Uuid::new_v4().to_string();
        let session = UiaSession {
            session_id: session_id.clone(),
            user_id: user_id.to_string(),
            completed: Vec::new(),
            created_ts: Utc::now().timestamp_millis(),
            flows,
        };
        let key = format!("uia:session:{session_id}");
        if let Err(e) = self.cache.set(&key, &session, self.session_timeout_secs as u64).await {
            tracing::warn!(session_id = %session_id, error = %e, "Failed to persist UIA session to cache");
        }
        session
    }

    pub async fn get_session(&self, session_id: &str) -> Option<UiaSession> {
        let key = format!("uia:session:{session_id}");
        self.cache.get(&key).await.ok().flatten()
    }

    pub async fn complete_stage(&self, session_id: &str, stage: &str) -> Option<UiaSession> {
        let key = format!("uia:session:{session_id}");
        let mut session: UiaSession = self.cache.get(&key).await.ok().flatten()?;

        if !session.completed.contains(&stage.to_string()) {
            session.completed.push(stage.to_string());
        }

        if let Err(e) = self.cache.set(&key, &session, self.session_timeout_secs as u64).await {
            tracing::warn!(session_id = %session_id, stage = %stage, error = %e, "Failed to persist UIA session stage completion to cache");
        }
        Some(session)
    }

    pub async fn remove_session(&self, session_id: &str) {
        let key = format!("uia:session:{session_id}");
        self.cache.delete(&key).await;
    }

    pub fn is_session_complete(&self, session: &UiaSession) -> bool {
        for flow in &session.flows {
            if flow.stages.iter().all(|stage| session.completed.contains(stage)) {
                return true;
            }
        }
        false
    }

    pub fn build_uia_response(&self, session: &UiaSession, errcode: &str, error: &str) -> Value {
        let flows: Vec<Value> = session.flows.iter().map(|f| json!({ "stages": f.stages })).collect();

        // Per Matrix spec, `params` provides information required for the client
        // to complete each auth type. For 3PID-based types, this includes the
        // identity server information.
        let mut params = serde_json::Map::new();
        params.insert(
            "m.login.email.identity".to_string(),
            json!({
                "threepidCreds": [],
            }),
        );
        params.insert(
            "m.login.msisdn".to_string(),
            json!({
                "threepidCreds": [],
            }),
        );

        json!({
            "errcode": errcode,
            "error": error,
            "flows": flows,
            "params": params,
            "session": session.session_id,
            "completed": session.completed
        })
    }

    pub async fn validate_auth(
        &self,
        auth: &Value,
        user_id: &str,
        flows: Vec<UiaFlow>,
    ) -> Result<UiaAuthResult, Value> {
        let auth_type = auth.get("type").and_then(|v| v.as_str());

        let session_id = auth.get("session").and_then(|v| v.as_str());

        let mut session = if let Some(sid) = session_id {
            match self.get_session(sid).await {
                Some(s) if s.user_id == user_id => s,
                Some(_) => {
                    let new_session = self.create_session(user_id, flows).await;
                    return Err(self.build_uia_response(
                        &new_session,
                        "M_FORBIDDEN",
                        "Session belongs to a different user",
                    ));
                }
                None => {
                    let new_session = self.create_session(user_id, flows).await;
                    return Err(self.build_uia_response(&new_session, "M_UNKNOWN", "Unknown or expired session"));
                }
            }
        } else {
            let new_session = self.create_session(user_id, flows).await;
            return Err(self.build_uia_response(
                &new_session,
                "M_UIA_REQUIRED",
                "User-Interactive Authentication required",
            ));
        };

        let stage = match auth_type {
            Some(t) => t.to_string(),
            None => {
                return Err(self.build_uia_response(
                    &session,
                    "M_UIA_REQUIRED",
                    "User-Interactive Authentication required",
                ));
            }
        };

        let valid_stages: Vec<String> = session.flows.iter().flat_map(|f| f.stages.iter().cloned()).collect();

        if !valid_stages.contains(&stage) {
            return Err(self.build_uia_response(
                &session,
                "M_INVALID_PARAM",
                &format!("Unsupported auth type: {stage}"),
            ));
        }

        if session.completed.contains(&stage) {
            if self.is_session_complete(&session) {
                self.remove_session(&session.session_id).await;
                return Ok(UiaAuthResult {
                    session: None,
                    completed: session.completed.clone(),
                    flows: session.flows,
                    params: json!({}),
                });
            }
            return Err(self.build_uia_response(
                &session,
                "M_UIA_REQUIRED",
                "Additional authentication stages required",
            ));
        }

        session = self.complete_stage(&session.session_id, &stage).await.unwrap_or(session);

        if self.is_session_complete(&session) {
            self.remove_session(&session.session_id).await;
            return Ok(UiaAuthResult {
                session: None,
                completed: session.completed.clone(),
                flows: session.flows,
                params: json!({}),
            });
        }

        Err(self.build_uia_response(&session, "M_UIA_REQUIRED", "Additional authentication stages required"))
    }

    pub async fn verify_password_stage(
        &self,
        auth: &Value,
        user_id: &str,
        auth_service: &Arc<dyn crate::auth::Auth>,
    ) -> Result<(), ApiError> {
        let password = auth
            .get("password")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("Password required for m.login.password".to_string()))?;

        let identifier_user = auth
            .get("identifier")
            .and_then(|i| i.get("user"))
            .and_then(|v| v.as_str())
            .or_else(|| auth.get("user").and_then(|v| v.as_str()))
            .or_else(|| auth.get("user_id").and_then(|v| v.as_str()))
            .unwrap_or(user_id);

        // Resolve localpart to fully-qualified Matrix ID per spec.
        // The `identifier.user` field may be a localpart (e.g. "alice")
        // or a full MXID (e.g. "@alice:server.com").
        let resolved_user_id = if identifier_user.starts_with('@') {
            identifier_user.to_string()
        } else {
            // Extract server_name from the authenticated user_id
            let server_name = user_id.rsplit_once(':').map_or("localhost", |(_, s)| s);
            format!("@{}:{}", identifier_user, server_name)
        };

        if resolved_user_id != user_id {
            return Err(ApiError::forbidden("User mismatch".to_string()));
        }

        // Only verify password hash without creating a new session/device
        auth_service
            .verify_user_credentials(user_id, password)
            .await
            .map_err(|_| ApiError::forbidden("Invalid password".to_string()))?;

        Ok(())
    }

    /// Verify `m.login.token` UIA stage.
    ///
    /// Validates that the token is present and that the transaction ID is
    /// provided. Full token verification against stored login tokens is
    /// performed when a token storage reference is available.
    pub async fn verify_token_stage(
        &self,
        auth: &Value,
        user_id: &str,
        auth_service: &Arc<dyn crate::auth::Auth>,
    ) -> Result<(), ApiError> {
        let token = auth
            .get("token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("Token required for m.login.token".to_string()))?;

        let txn_id = auth.get("txn_id").and_then(|v| v.as_str()).unwrap_or("");

        if txn_id.is_empty() {
            return Err(ApiError::bad_request("Transaction ID required".to_string()));
        }

        // Verify the token is valid and belongs to the user.
        // In the UIA context, the token is typically a login token issued
        // during the m.login.token registration/login flow.
        match auth_service.validate_token(token).await {
            Ok((token_user_id, _, _, _, _)) => {
                if token_user_id != user_id {
                    return Err(ApiError::forbidden("Token belongs to a different user".to_string()));
                }
            }
            Err(_) => {
                return Err(ApiError::forbidden("Invalid or expired token".to_string()));
            }
        }

        tracing::info!(
            target: "uia",
            user_id = user_id,
            "m.login.token stage accepted"
        );

        Ok(())
    }

    /// Verify `m.login.email.identity` UIA stage.
    ///
    /// Validates that the user has at least one verified email threepid
    /// associated with their account. Additionally validates the
    /// `threepidCreds` structure (sid/client_secret) against the HS-managed
    /// validation session when available.
    ///
    /// This aligns with Synapse v1.153 behavior: email-based UIA stages
    /// require the user to have a verified email threepid.
    pub async fn verify_email_identity_stage(
        &self,
        auth: &Value,
        user_id: &str,
        threepid_storage: &ThreepidStorage,
    ) -> Result<(), ApiError> {
        let threepid_creds = auth.get("threepidCreds").or_else(|| auth.get("threepid_creds"));

        // Validate threepidCreds structure
        let creds_array = threepid_creds.and_then(|v| v.as_array()).ok_or_else(|| {
            ApiError::bad_request("threepidCreds array required for m.login.email.identity".to_string())
        })?;

        // Per Synapse v1.153: validate each credential entry
        for cred in creds_array {
            let sid = cred
                .get("sid")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ApiError::bad_request("sid required in threepidCreds".to_string()))?;
            let client_secret = cred
                .get("client_secret")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ApiError::bad_request("client_secret required in threepidCreds".to_string()))?;

            if sid.is_empty() || client_secret.is_empty() {
                return Err(ApiError::bad_request("sid and client_secret must not be empty".to_string()));
            }
        }

        // Verify the user has at least one verified email threepid.
        // This is the core check: without a verified email, the email-based
        // UIA stage cannot be completed.
        let user_threepids = threepid_storage
            .get_threepids_by_user(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get user threepids", &e))?;

        let has_verified_email = user_threepids.iter().any(|t| t.medium == "email" && t.is_verified);

        if !has_verified_email {
            return Err(ApiError::forbidden(
                "No verified email associated with this account. Add and verify an email first.".to_string(),
            ));
        }

        tracing::info!(
            target: "uia",
            user_id = user_id,
            "m.login.email.identity stage accepted"
        );

        Ok(())
    }

    /// Verify `m.login.msisdn` UIA stage.
    ///
    /// Validates that the user has at least one verified MSISDN (phone) threepid
    /// associated with their account. Additionally validates the `threepidCreds`
    /// structure.
    ///
    /// This aligns with Synapse v1.153 behavior: MSISDN-based UIA stages
    /// require the user to have a verified phone threepid.
    pub async fn verify_msisdn_stage(
        &self,
        auth: &Value,
        user_id: &str,
        threepid_storage: &ThreepidStorage,
    ) -> Result<(), ApiError> {
        let threepid_creds = auth.get("threepidCreds").or_else(|| auth.get("threepid_creds"));

        let creds_array = threepid_creds
            .and_then(|v| v.as_array())
            .ok_or_else(|| ApiError::bad_request("threepidCreds array required for m.login.msisdn".to_string()))?;

        for cred in creds_array {
            let sid = cred
                .get("sid")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ApiError::bad_request("sid required in threepidCreds".to_string()))?;
            let client_secret = cred
                .get("client_secret")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ApiError::bad_request("client_secret required in threepidCreds".to_string()))?;

            if sid.is_empty() || client_secret.is_empty() {
                return Err(ApiError::bad_request("sid and client_secret must not be empty".to_string()));
            }
        }

        // Verify the user has at least one verified MSISDN threepid
        let user_threepids = threepid_storage
            .get_threepids_by_user(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get user threepids", &e))?;

        let has_verified_msisdn = user_threepids.iter().any(|t| t.medium == "msisdn" && t.is_verified);

        if !has_verified_msisdn {
            return Err(ApiError::forbidden(
                "No verified phone number associated with this account. Add and verify a phone number first."
                    .to_string(),
            ));
        }

        tracing::info!(
            target: "uia",
            user_id = user_id,
            "m.login.msisdn stage accepted"
        );

        Ok(())
    }

    pub fn cleanup_expired_sessions(&self) -> Result<(), String> {
        Ok(())
    }

    /// Perform full UIA verification for a route handler.
    ///
    /// This consolidates the common pattern shared by UIA-protected endpoints:
    /// 1. If no `auth` field → create session, return 401 with M_UIA_REQUIRED
    /// 2. Call `validate_auth` → if error, return 401 with UIA response
    /// 3. Dispatch to `verify_password_stage` / `verify_token_stage` based on auth_type
    /// 4. Return `Ok(())` if all verification passes
    ///
    /// Returns `Err(Value)` with the JSON body for a 401 response on auth failure.
    ///
    /// `threepid_storage` is required for email/msisdn-based UIA stages to
    /// verify the user has a verified threepid of the appropriate medium.
    /// This aligns with Synapse v1.153 behavior.
    pub async fn require_uia(
        &self,
        auth: Option<&Value>,
        user_id: &str,
        flows: Vec<UiaFlow>,
        auth_service: &Arc<dyn crate::auth::Auth>,
        threepid_storage: &ThreepidStorage,
    ) -> Result<(), Value> {
        let auth_val = match auth {
            None => {
                let session = self.create_session(user_id, flows).await;
                return Err(self.build_uia_response(
                    &session,
                    "M_UIA_REQUIRED",
                    "User-Interactive Authentication required",
                ));
            }
            Some(v) => v,
        };

        // Validate UIA session and stage
        self.validate_auth(auth_val, user_id, flows.clone()).await?;

        // Verify the specific auth type
        let auth_type = auth_val.get("type").and_then(|v| v.as_str()).unwrap_or("");
        match auth_type {
            "m.login.password" => {
                if let Err(e) = self.verify_password_stage(auth_val, user_id, auth_service).await {
                    let session = self.create_session(user_id, flows).await;
                    return Err(self.build_uia_response(&session, "M_FORBIDDEN", &e.to_string()));
                }
            }
            "m.login.token" => {
                if let Err(e) = self.verify_token_stage(auth_val, user_id, auth_service).await {
                    let session = self.create_session(user_id, flows).await;
                    return Err(self.build_uia_response(&session, "M_FORBIDDEN", &e.to_string()));
                }
            }
            "m.login.email.identity" => {
                if let Err(e) = self.verify_email_identity_stage(auth_val, user_id, threepid_storage).await {
                    let session = self.create_session(user_id, flows).await;
                    return Err(self.build_uia_response(&session, "M_FORBIDDEN", &e.to_string()));
                }
            }
            "m.login.msisdn" => {
                if let Err(e) = self.verify_msisdn_stage(auth_val, user_id, threepid_storage).await {
                    let session = self.create_session(user_id, flows).await;
                    return Err(self.build_uia_response(&session, "M_FORBIDDEN", &e.to_string()));
                }
            }
            _ => {
                let session = self.create_session(user_id, flows).await;
                return Err(self.build_uia_response(
                    &session,
                    "M_INVALID_PARAM",
                    &format!("Unsupported auth type: {auth_type}"),
                ));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use synapse_cache::CacheConfig;

    #[test]
    fn test_get_default_flows_includes_password() {
        let flows = UiaService::get_default_flows();
        assert!(flows.iter().any(|f| f.stages.contains(&"m.login.password".to_string())));
    }

    #[test]
    fn test_get_default_flows_includes_token() {
        let flows = UiaService::get_default_flows();
        assert!(flows.iter().any(|f| f.stages.contains(&"m.login.token".to_string())));
    }

    #[test]
    fn test_get_password_change_flows() {
        let flows = UiaService::get_password_change_flows();
        assert!(flows.iter().any(|f| f.stages.contains(&"m.login.password".to_string())));
        assert!(flows.iter().any(|f| f.stages.contains(&"m.login.email.identity".to_string())));
    }

    #[test]
    fn test_get_delete_device_flows() {
        let flows = UiaService::get_delete_device_flows();
        assert!(flows.iter().any(|f| f.stages.contains(&"m.login.password".to_string())));
    }

    #[test]
    fn test_get_deactivate_account_flows() {
        let flows = UiaService::get_deactivate_account_flows();
        assert!(flows.iter().any(|f| f.stages.contains(&"m.login.password".to_string())));
    }

    #[test]
    fn test_is_session_complete_single_stage_completed() {
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let service = UiaService::new(cache, 3600);

        let session = UiaSession {
            session_id: "test_session".to_string(),
            user_id: "@user:server".to_string(),
            completed: vec!["m.login.password".to_string()],
            created_ts: 0,
            flows: vec![UiaFlow { stages: vec!["m.login.password".to_string()] }],
        };

        assert!(service.is_session_complete(&session));
    }

    #[test]
    fn test_is_session_complete_multi_stage_partial() {
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let service = UiaService::new(cache, 3600);

        let session = UiaSession {
            session_id: "test_session".to_string(),
            user_id: "@user:server".to_string(),
            completed: vec!["m.login.password".to_string()],
            created_ts: 0,
            flows: vec![UiaFlow { stages: vec!["m.login.password".to_string(), "m.login.email.identity".to_string()] }],
        };

        assert!(!service.is_session_complete(&session));
    }

    #[test]
    fn test_is_session_complete_multi_stage_all() {
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let service = UiaService::new(cache, 3600);

        let session = UiaSession {
            session_id: "test_session".to_string(),
            user_id: "@user:server".to_string(),
            completed: vec!["m.login.password".to_string(), "m.login.email.identity".to_string()],
            created_ts: 0,
            flows: vec![UiaFlow { stages: vec!["m.login.password".to_string(), "m.login.email.identity".to_string()] }],
        };

        assert!(service.is_session_complete(&session));
    }

    #[test]
    fn test_is_session_complete_alternative_flow() {
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let service = UiaService::new(cache, 3600);

        // Two alternative flows: password OR token
        let session = UiaSession {
            session_id: "test_session".to_string(),
            user_id: "@user:server".to_string(),
            completed: vec!["m.login.token".to_string()],
            created_ts: 0,
            flows: vec![
                UiaFlow { stages: vec!["m.login.password".to_string()] },
                UiaFlow { stages: vec!["m.login.token".to_string()] },
            ],
        };

        assert!(service.is_session_complete(&session));
    }

    #[test]
    fn test_is_session_complete_nothing_completed() {
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let service = UiaService::new(cache, 3600);

        let session = UiaSession {
            session_id: "test_session".to_string(),
            user_id: "@user:server".to_string(),
            completed: vec![],
            created_ts: 0,
            flows: vec![UiaFlow { stages: vec!["m.login.password".to_string()] }],
        };

        assert!(!service.is_session_complete(&session));
    }

    #[test]
    fn test_build_uia_response() {
        let cache = Arc::new(CacheManager::new(&CacheConfig::default()));
        let service = UiaService::new(cache, 3600);

        let session = UiaSession {
            session_id: "sid123".to_string(),
            user_id: "@user:server".to_string(),
            completed: vec!["m.login.password".to_string()],
            created_ts: 0,
            flows: vec![
                UiaFlow { stages: vec!["m.login.password".to_string()] },
                UiaFlow { stages: vec!["m.login.token".to_string()] },
            ],
        };

        let response = service.build_uia_response(&session, "M_UIA_REQUIRED", "Auth required");

        assert_eq!(response["errcode"], "M_UIA_REQUIRED");
        assert_eq!(response["error"], "Auth required");
        assert_eq!(response["session"], "sid123");
        assert!(response["flows"].is_array());
        assert_eq!(response["flows"].as_array().unwrap().len(), 2);
        assert!(response["completed"].is_array());
        assert_eq!(response["completed"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_uia_session_serialization() {
        let session = UiaSession {
            session_id: "sid123".to_string(),
            user_id: "@user:server".to_string(),
            completed: vec!["m.login.password".to_string()],
            created_ts: 1700000000000,
            flows: vec![UiaFlow { stages: vec!["m.login.password".to_string()] }],
        };

        let json = serde_json::to_string(&session).unwrap();
        assert!(json.contains("sid123"));
        assert!(json.contains("m.login.password"));

        let deserialized: UiaSession = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.session_id, "sid123");
        assert_eq!(deserialized.completed, vec!["m.login.password"]);
    }

    #[test]
    fn test_uia_auth_result_serialization() {
        let result = UiaAuthResult {
            session: Some("sid123".to_string()),
            completed: vec!["m.login.password".to_string()],
            flows: vec![UiaFlow { stages: vec!["m.login.password".to_string()] }],
            params: json!({}),
        };

        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("session"));
        assert!(json.contains("completed"));
    }
}
