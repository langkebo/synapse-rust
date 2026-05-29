use crate::cache::CacheManager;
use crate::common::ApiError;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

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
        ]
    }

    pub fn get_password_change_flows() -> Vec<UiaFlow> {
        vec![
            UiaFlow { stages: vec!["m.login.password".to_string()] },
            UiaFlow { stages: vec!["m.login.email.identity".to_string()] },
        ]
    }

    pub fn get_delete_device_flows() -> Vec<UiaFlow> {
        vec![UiaFlow { stages: vec!["m.login.password".to_string()] }]
    }

    pub fn get_deactivate_account_flows() -> Vec<UiaFlow> {
        vec![UiaFlow { stages: vec!["m.login.password".to_string()] }]
    }

    pub fn get_cross_signing_flows() -> Vec<UiaFlow> {
        vec![UiaFlow { stages: vec!["m.login.password".to_string()] }]
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
        let _ = self.cache.set(&key, &session, self.session_timeout_secs as u64).await;
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

        let _ = self.cache.set(&key, &session, self.session_timeout_secs as u64).await;
        Some(session)
    }

    pub async fn remove_session(&self, session_id: &str) {
        let key = format!("uia:session:{session_id}");
        let _ = self.cache.delete(&key).await;
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

        json!({
            "errcode": errcode,
            "error": error,
            "flows": flows,
            "params": {},
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
        auth_service: &crate::auth::AuthService,
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
            let server_name = user_id.rsplit_once(':').map(|(_, s)| s).unwrap_or("localhost");
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

    pub fn verify_token_stage(&self, auth: &Value, _user_id: &str) -> Result<(), ApiError> {
        let _token = auth
            .get("token")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ApiError::bad_request("Token required for m.login.token".to_string()))?;

        let txn_id = auth.get("txn_id").and_then(|v| v.as_str()).unwrap_or("");

        if txn_id.is_empty() {
            return Err(ApiError::bad_request("Transaction ID required".to_string()));
        }

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
    pub async fn require_uia(
        &self,
        auth: Option<&Value>,
        user_id: &str,
        flows: Vec<UiaFlow>,
        auth_service: &crate::auth::AuthService,
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
        if let Err(uia_response) = self.validate_auth(auth_val, user_id, flows.clone()).await {
            return Err(uia_response);
        }

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
                if let Err(e) = self.verify_token_stage(auth_val, user_id) {
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
