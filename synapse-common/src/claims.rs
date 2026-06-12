use serde::{Deserialize, Serialize};

/// JWT Claims structure used for authentication tokens.
/// Moved to common to resolve circular dependency between cache and auth modules.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,
    pub user_id: String,
    pub jti: String,
    #[serde(rename = "admin")]
    pub is_admin: bool,
    pub exp: i64,
    pub iat: i64,
    pub device_id: Option<String>,
}

/// Builder for constructing [`Claims`] with consistent defaults.
pub struct ClaimsBuilder {
    sub: Option<String>,
    user_id: Option<String>,
    jti: Option<String>,
    is_admin: bool,
    exp: Option<i64>,
    iat: Option<i64>,
    device_id: Option<String>,
}

impl ClaimsBuilder {
    pub fn new() -> Self {
        Self { sub: None, user_id: None, jti: None, is_admin: false, exp: None, iat: None, device_id: None }
    }

    #[allow(clippy::should_implement_trait)]
    pub fn sub(mut self, sub: impl Into<String>) -> Self {
        self.sub = Some(sub.into());
        self
    }

    pub fn user_id(mut self, user_id: impl Into<String>) -> Self {
        self.user_id = Some(user_id.into());
        self
    }

    pub fn jti(mut self, jti: impl Into<String>) -> Self {
        self.jti = Some(jti.into());
        self
    }

    pub fn is_admin(mut self, is_admin: bool) -> Self {
        self.is_admin = is_admin;
        self
    }

    pub fn exp(mut self, exp: i64) -> Self {
        self.exp = Some(exp);
        self
    }

    pub fn iat(mut self, iat: i64) -> Self {
        self.iat = Some(iat);
        self
    }

    pub fn device_id(mut self, device_id: Option<String>) -> Self {
        self.device_id = device_id;
        self
    }

    #[allow(clippy::expect_used)]
    pub fn build(self) -> Claims {
        let now = chrono::Utc::now().timestamp();
        let sub = self.sub.expect("ClaimsBuilder: sub is required");
        Claims {
            sub: sub.clone(),
            user_id: self.user_id.unwrap_or(sub),
            jti: self.jti.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            is_admin: self.is_admin,
            exp: self.exp.expect("ClaimsBuilder: exp is required"),
            iat: self.iat.unwrap_or(now),
            device_id: self.device_id,
        }
    }
}

impl Default for ClaimsBuilder {
    fn default() -> Self {
        Self::new()
    }
}
