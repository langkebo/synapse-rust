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
    /// P1-18: JWT issuer claim — prevents token confusion when jwt_secret is reused across services.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iss: Option<String>,
    /// P1-18: JWT audience claim — ensures token is only accepted by the intended server.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aud: Option<String>,
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
    iss: Option<String>,
    aud: Option<String>,
}

impl ClaimsBuilder {
    pub fn new() -> Self {
        Self {
            sub: None,
            user_id: None,
            jti: None,
            is_admin: false,
            exp: None,
            iat: None,
            device_id: None,
            iss: None,
            aud: None,
        }
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

    /// P1-18: Set the JWT issuer claim (typically the server_name).
    pub fn iss(mut self, iss: impl Into<String>) -> Self {
        self.iss = Some(iss.into());
        self
    }

    /// P1-18: Set the JWT audience claim (typically the server_name).
    pub fn aud(mut self, aud: impl Into<String>) -> Self {
        self.aud = Some(aud.into());
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
            iss: self.iss,
            aud: self.aud,
        }
    }
}

impl Default for ClaimsBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claims_builder_minimal() {
        let claims = ClaimsBuilder::new().sub("@alice:ex.com").exp(9999999999).build();
        assert_eq!(claims.sub, "@alice:ex.com");
        assert_eq!(claims.user_id, "@alice:ex.com"); // defaults to sub
        assert_eq!(claims.is_admin, false);
        assert_eq!(claims.exp, 9999999999);
        assert!(claims.iat > 0);
        assert!(claims.device_id.is_none());
    }

    #[test]
    fn claims_builder_full() {
        let claims = ClaimsBuilder::new()
            .sub("@alice:ex.com")
            .user_id("@alice:ex.com")
            .jti("jti-123")
            .is_admin(true)
            .exp(9999999999)
            .iat(1000000)
            .device_id(Some("DEVICE1".into()))
            .iss("ex.com")
            .aud("ex.com")
            .build();
        assert_eq!(claims.sub, "@alice:ex.com");
        assert_eq!(claims.user_id, "@alice:ex.com");
        assert_eq!(claims.jti, "jti-123");
        assert!(claims.is_admin);
        assert_eq!(claims.exp, 9999999999);
        assert_eq!(claims.iat, 1000000);
        assert_eq!(claims.device_id, Some("DEVICE1".into()));
        assert_eq!(claims.iss, Some("ex.com".into()));
        assert_eq!(claims.aud, Some("ex.com".into()));
    }

    #[test]
    fn claims_builder_user_id_falls_back_to_sub() {
        let claims = ClaimsBuilder::new().sub("@bob:ex.com").exp(9999999999).build();
        assert_eq!(claims.user_id, "@bob:ex.com");
    }

    #[test]
    fn claims_builder_jti_auto_generated() {
        let claims = ClaimsBuilder::new().sub("@alice:ex.com").exp(9999999999).build();
        // jti should be a valid UUID
        assert!(uuid::Uuid::parse_str(&claims.jti).is_ok());
    }

    #[test]
    fn claims_builder_iat_defaults_to_now() {
        let now = chrono::Utc::now().timestamp();
        let claims = ClaimsBuilder::new().sub("@alice:ex.com").exp(9999999999).build();
        assert!((claims.iat - now).abs() < 5);
    }

    #[test]
    fn claims_builder_is_admin_defaults_false() {
        let claims = ClaimsBuilder::new().sub("@alice:ex.com").exp(9999999999).build();
        assert!(!claims.is_admin);
    }

    #[test]
    fn claims_serialization_roundtrip() {
        let claims =
            ClaimsBuilder::new().sub("@alice:ex.com").jti("jti-1").is_admin(false).exp(9999999999).iat(1000000).build();
        let json = serde_json::to_string(&claims).unwrap();
        let deserialized: Claims = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.sub, claims.sub);
        assert_eq!(deserialized.jti, claims.jti);
        assert_eq!(deserialized.exp, claims.exp);
    }

    #[test]
    fn claims_serialization_skips_none_optional_fields() {
        let claims = ClaimsBuilder::new().sub("@alice:ex.com").exp(9999999999).build();
        let json = serde_json::to_string(&claims).unwrap();
        // iss and aud have skip_serializing_if, device_id does not
        assert!(!json.contains("\"iss\""));
        assert!(!json.contains("\"aud\""));
    }

    #[test]
    fn claims_default_builder() {
        let builder = ClaimsBuilder::default();
        assert!(builder.sub.is_none());
        assert!(builder.user_id.is_none());
        assert!(!builder.is_admin);
    }
}
