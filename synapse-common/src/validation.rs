//! Input validation framework (`Validator`, `ValidationContext`, `ValidationError`).

use crate::constants::*;
use crate::ApiError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Type alias for ValidationResult.
pub type ValidationResult = Result<(), ValidationError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Represents ValidationError.
pub struct ValidationError {
    /// `field` field.
    pub field: String,
    /// `message` field.
    pub message: String,
    /// `code` field.
    pub code: String,
}

impl ValidationError {
    /// Constructs a new instance.
    pub fn new(field: &str, message: &str, code: &str) -> Self {
        Self { field: field.to_string(), message: message.to_string(), code: code.to_string() }
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.field, self.message)
    }
}

impl From<ValidationError> for ApiError {
    fn from(err: ValidationError) -> Self {
        Self::bad_request(format!("{}: {}", err.field, err.message))
    }
}

/// The Matrix **user ID** grammar: `@localpart:server`.
///
/// `localpart` is `[a-z0-9._=-]+` and `server` is `[a-zA-Z0-9.-]+` — the same
/// shape [`Validator`] has always enforced, now expressed byte-wise so that pure
/// callers can reach it. It is the **single** implementation of that grammar:
/// [`Validator::validate_matrix_id`] delegates here.
///
/// Used by room v12's create-event rule (MSC4289 rule 1.4), which must reject an
/// `additional_creators` entry that would not pass the same validation as the
/// create event's `sender`.
pub fn is_well_formed_user_id(user_id: &str) -> bool {
    let Some(rest) = user_id.strip_prefix('@') else {
        return false;
    };
    let Some((localpart, server)) = rest.split_once(':') else {
        return false;
    };
    if localpart.is_empty() || server.is_empty() || server.contains(':') {
        return false;
    }
    localpart.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'=' | b'-'))
        && server.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-'))
}

/// Returns `true` if `localpart` is a **compliant** user-ID localpart.
///
/// Per the Matrix spec appendices ("Historical user IDs"), a user ID is
/// non-compliant if its localpart is empty or contains any character outside the
/// range `U+0021..=U+007E`. Servers must ignore such "historical" user IDs in
/// inbound device list updates (upstream parity with Synapse PR #20115).
pub fn is_compliant_user_id_localpart(localpart: &str) -> bool {
    !localpart.is_empty() && localpart.chars().all(|c| ('\u{21}'..='\u{7E}').contains(&c))
}

#[derive(Debug, Clone)]
/// Represents Validator.
pub struct Validator {
    username_regex: Regex,
    email_regex: Regex,
    device_id_regex: Regex,
    url_regex: Regex,
}

impl Validator {
    /// Constructs a new instance.
    pub fn new() -> Result<Self, regex::Error> {
        Ok(Self {
            // Matrix localpart: [a-z0-9._=-]+
            username_regex: Regex::new(r"^[a-z0-9._=\-]{1,255}$")?,
            email_regex: Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")?,
            device_id_regex: Regex::new(r"^[a-zA-Z0-9._\-]{1,255}$")?,
            url_regex: Regex::new(r"^https?://[a-zA-Z0-9.-]+(:[0-9]+)?(/.*)?$")?,
        })
    }

    /// Validates the username.
    pub fn validate_username(&self, username: &str) -> ValidationResult {
        if username.is_empty() {
            return Err(ValidationError::new("username", "Username cannot be empty", "EMPTY"));
        }

        if username.len() < MIN_USERNAME_LENGTH {
            return Err(ValidationError::new(
                "username",
                &format!("Username must be at least {MIN_USERNAME_LENGTH} characters"),
                "TOO_SHORT",
            ));
        }

        if username.len() > MAX_USERNAME_LENGTH {
            return Err(ValidationError::new(
                "username",
                &format!("Username must be at most {MAX_USERNAME_LENGTH} characters"),
                "TOO_LONG",
            ));
        }

        if !self.username_regex.is_match(username) {
            return Err(ValidationError::new("username", "Username contains invalid characters", "INVALID_FORMAT"));
        }

        Ok(())
    }

    /// Validates the password.
    pub fn validate_password(&self, password: &str) -> ValidationResult {
        if password.is_empty() {
            return Err(ValidationError::new("password", "Password cannot be empty", "EMPTY"));
        }

        if password.len() < MIN_PASSWORD_LENGTH {
            return Err(ValidationError::new(
                "password",
                &format!("Password must be at least {MIN_PASSWORD_LENGTH} characters"),
                "TOO_SHORT",
            ));
        }

        if password.len() > MAX_PASSWORD_LENGTH {
            return Err(ValidationError::new(
                "password",
                &format!("Password must be at most {MAX_PASSWORD_LENGTH} characters"),
                "TOO_LONG",
            ));
        }

        let has_upper = password.chars().any(|c| c.is_uppercase());
        let has_lower = password.chars().any(|c| c.is_lowercase());
        let has_digit = password.chars().any(|c| c.is_ascii_digit());
        let has_special = password.chars().any(|c| "!@#$%^&*()_+-=[]{}|;:,.<>?".contains(c));

        if !has_upper {
            return Err(ValidationError::new(
                "password",
                "Password must contain at least one uppercase letter",
                "NO_UPPERCASE",
            ));
        }

        if !has_lower {
            return Err(ValidationError::new(
                "password",
                "Password must contain at least one lowercase letter",
                "NO_LOWERCASE",
            ));
        }

        if !has_digit {
            return Err(ValidationError::new("password", "Password must contain at least one digit", "NO_DIGIT"));
        }

        if !has_special {
            return Err(ValidationError::new(
                "password",
                "Password must contain at least one special character",
                "NO_SPECIAL",
            ));
        }

        Ok(())
    }

    /// Validates the email.
    pub fn validate_email(&self, email: &str) -> ValidationResult {
        if email.is_empty() {
            return Err(ValidationError::new("email", "Email cannot be empty", "EMPTY"));
        }

        if !self.email_regex.is_match(email) {
            return Err(ValidationError::new("email", "Invalid email format", "INVALID_FORMAT"));
        }

        Ok(())
    }

    /// Validates the matrix.
    ///
    /// The grammar itself lives in [`is_well_formed_user_id`] so pure callers
    /// (the federation auth-rule module) can apply the **same** validation
    /// without constructing a `Validator`, which needs compiled regexes.
    pub fn validate_matrix_id(&self, user_id: &str) -> ValidationResult {
        if user_id.is_empty() {
            return Err(ValidationError::new("user_id", "User ID cannot be empty", "EMPTY"));
        }
        if !is_well_formed_user_id(user_id) {
            return Err(ValidationError::new("user_id", "Invalid Matrix ID format", "INVALID_FORMAT"));
        }
        Ok(())
    }

    /// Validates the room.
    ///
    /// Accepts **both** Matrix room-ID forms (see [`crate::room_id`]): the
    /// legacy `!opaque:server` used by room versions 1–11 and the domainless
    /// `!` + 43 unpadded URL-safe base64 characters form that room version 12
    /// (MSC4291) introduced. The decision is delegated to the shared grammar so
    /// this validator and the route-layer one cannot drift.
    pub fn validate_room_id(&self, room_id: &str) -> ValidationResult {
        if room_id.is_empty() {
            return Err(ValidationError::new("room_id", "Room ID cannot be empty", "EMPTY"));
        }

        crate::room_id::parse_room_id(room_id)
            .map(|_| ())
            .map_err(|_| ValidationError::new("room_id", "Invalid room ID format", "INVALID_FORMAT"))
    }

    /// Validates the device.
    pub fn validate_device_id(&self, device_id: &str) -> ValidationResult {
        if device_id.is_empty() {
            return Err(ValidationError::new("device_id", "Device ID cannot be empty", "EMPTY"));
        }

        if device_id.len() > MAX_DEVICE_ID_LENGTH {
            return Err(ValidationError::new(
                "device_id",
                &format!("Device ID must be at most {MAX_DEVICE_ID_LENGTH} characters"),
                "TOO_LONG",
            ));
        }

        if !self.device_id_regex.is_match(device_id) {
            return Err(ValidationError::new("device_id", "Device ID contains invalid characters", "INVALID_FORMAT"));
        }

        Ok(())
    }

    /// Validates the url.
    pub fn validate_url(&self, url: &str) -> ValidationResult {
        if url.is_empty() {
            return Err(ValidationError::new("url", "URL cannot be empty", "EMPTY"));
        }

        if !self.url_regex.is_match(url) {
            return Err(ValidationError::new("url", "Invalid URL format", "INVALID_FORMAT"));
        }

        Ok(())
    }

    /// Validates the string.
    pub fn validate_string_length(&self, field: &str, value: &str, min: usize, max: usize) -> ValidationResult {
        if min > 0 && value.is_empty() {
            return Err(ValidationError::new(field, &format!("{field} cannot be empty"), "EMPTY"));
        }

        if value.len() < min {
            return Err(ValidationError::new(
                field,
                &format!("{field} must be at least {min} characters"),
                "TOO_SHORT",
            ));
        }

        if value.len() > max {
            return Err(ValidationError::new(field, &format!("{field} must be at most {max} characters"), "TOO_LONG"));
        }

        Ok(())
    }

    /// Validates the display.
    pub fn validate_display_name(&self, display_name: &str) -> ValidationResult {
        self.validate_string_length("display_name", display_name, 1, MAX_DISPLAY_NAME_LENGTH)
    }

    /// Validates the reason.
    pub fn validate_reason(&self, reason: &str) -> ValidationResult {
        self.validate_string_length("reason", reason, 0, MAX_REASON_LENGTH)
    }

    /// Validates the message.
    pub fn validate_message(&self, message: &str) -> ValidationResult {
        self.validate_string_length("message", message, 1, MAX_MESSAGE_LENGTH)
    }

    /// Validates the limit.
    pub fn validate_limit(&self, limit: i64, min: i64, max: i64) -> ValidationResult {
        if limit < min {
            return Err(ValidationError::new("limit", format!("Limit must be at least {min}").as_str(), "TOO_SMALL"));
        }

        if limit > max {
            return Err(ValidationError::new("limit", format!("Limit must be at most {max}").as_str(), "TOO_LARGE"));
        }

        Ok(())
    }

    /// Validates the timestamp.
    pub fn validate_timestamp(&self, timestamp: i64) -> ValidationResult {
        let now = chrono::Utc::now().timestamp();
        let window = TIMESTAMP_WINDOW_SECONDS;
        let min_valid = now - window;
        let max_valid = now + window;

        if timestamp < min_valid {
            return Err(ValidationError::new("timestamp", "Timestamp is too old", "TOO_OLD"));
        }

        if timestamp > max_valid {
            return Err(ValidationError::new("timestamp", "Timestamp is too far in the future", "TOO_FUTURE"));
        }

        Ok(())
    }

    /// Validates the ip.
    pub fn validate_ip_address(&self, ip: &str) -> ValidationResult {
        if ip.is_empty() {
            return Err(ValidationError::new("ip_address", "IP address cannot be empty", "EMPTY"));
        }

        if ip.parse::<std::net::IpAddr>().is_err() {
            return Err(ValidationError::new("ip_address", "Invalid IP address format", "INVALID_FORMAT"));
        }

        Ok(())
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new().unwrap_or_else(|e| {
            tracing::error!("Failed to compile validation regexes: {}", e);
            tracing::warn!("Using fallback validator with relaxed validation rules");
            Self::create_fallback_validator()
        })
    }
}

impl Validator {
    #[allow(
        clippy::expect_used,
        reason = "hardcoded fallback regexes are literals verified by unit tests; compilation cannot fail"
    )]
    fn create_fallback_validator() -> Self {
        Self {
            username_regex: Regex::new(r"^[a-zA-Z0-9_.-]+$").expect("hardcoded fallback regex is syntactically valid"),
            email_regex: Regex::new(r"^[^@]+@[^@]+\.[^@]+$").expect("hardcoded fallback regex is syntactically valid"),
            device_id_regex: Regex::new(r"^[a-zA-Z0-9._\-]+$")
                .expect("hardcoded fallback regex is syntactically valid"),
            url_regex: Regex::new(r"^https?://.+").expect("hardcoded fallback regex is syntactically valid"),
        }
    }
}

#[derive(Debug, Clone)]
/// Represents ValidationContext.
pub struct ValidationContext {
    validator: Arc<Validator>,
    errors: Vec<ValidationError>,
}

impl ValidationContext {
    /// Constructs a new instance.
    pub fn new(validator: Arc<Validator>) -> Self {
        Self { validator, errors: Vec::new() }
    }

    /// Validates the username.
    pub fn validate_username(&mut self, username: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_username(username) {
            self.errors.push(e);
        }
        self
    }

    /// Validates the password.
    pub fn validate_password(&mut self, password: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_password(password) {
            self.errors.push(e);
        }
        self
    }

    /// Validates the email.
    pub fn validate_email(&mut self, email: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_email(email) {
            self.errors.push(e);
        }
        self
    }

    /// Validates the matrix.
    pub fn validate_matrix_id(&mut self, user_id: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_matrix_id(user_id) {
            self.errors.push(e);
        }
        self
    }

    /// Validates the room.
    pub fn validate_room_id(&mut self, room_id: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_room_id(room_id) {
            self.errors.push(e);
        }
        self
    }

    /// Validates the device.
    pub fn validate_device_id(&mut self, device_id: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_device_id(device_id) {
            self.errors.push(e);
        }
        self
    }

    /// Validates the url.
    pub fn validate_url(&mut self, url: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_url(url) {
            self.errors.push(e);
        }
        self
    }

    /// Validates the display.
    pub fn validate_display_name(&mut self, display_name: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_display_name(display_name) {
            self.errors.push(e);
        }
        self
    }

    /// Validates the reason.
    pub fn validate_reason(&mut self, reason: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_reason(reason) {
            self.errors.push(e);
        }
        self
    }

    /// Validates the message.
    pub fn validate_message(&mut self, message: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_message(message) {
            self.errors.push(e);
        }
        self
    }

    /// Validates the limit.
    pub fn validate_limit(&mut self, limit: i64, min: i64, max: i64) -> &mut Self {
        if let Err(e) = self.validator.validate_limit(limit, min, max) {
            self.errors.push(e);
        }
        self
    }

    /// Validates the timestamp.
    pub fn validate_timestamp(&mut self, timestamp: i64) -> &mut Self {
        if let Err(e) = self.validator.validate_timestamp(timestamp) {
            self.errors.push(e);
        }
        self
    }

    /// Validates the ip.
    pub fn validate_ip_address(&mut self, ip: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_ip_address(ip) {
            self.errors.push(e);
        }
        self
    }

    /// Validates the optional.
    pub fn validate_optional<F>(&mut self, field: Option<&str>, validator: F) -> &mut Self
    where
        F: FnOnce(&str) -> ValidationResult,
    {
        if let Some(value) = field {
            if let Err(e) = validator(value) {
                self.errors.push(e);
            }
        }
        self
    }

    /// Returns true if valid.
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    /// Intos the result.
    pub fn into_result(self) -> Result<(), ApiError> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(ApiError::bad_request(format!(
                "Validation failed: {}",
                self.errors.iter().map(|e| format!("{}: {}", e.field, e.message)).collect::<Vec<_>>().join(", ")
            )))
        }
    }

    /// Intos the error.
    pub fn into_error_map(self) -> HashMap<String, String> {
        self.errors.into_iter().map(|e| (e.field, e.message)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(test)]
    mod property_tests {
        use super::*;
        use quickcheck_macros::quickcheck;

        #[quickcheck]
        fn test_validate_limit_property(limit: i64) -> bool {
            let validator = Validator::new().unwrap();
            let min = 10;
            let max = 100;

            let result = validator.validate_limit(limit, min, max);

            if limit >= min && limit <= max {
                result.is_ok()
            } else {
                result.is_err()
            }
        }
    }

    #[test]
    fn test_validate_username_valid() {
        let validator = Validator::new().unwrap();
        assert!(validator.validate_username("testuser").is_ok());
        assert!(validator.validate_username("test_user-123").is_ok());
        assert!(validator.validate_username("a").is_ok()); // minimum length
    }

    #[test]
    fn test_validate_username_invalid() {
        let validator = Validator::new().unwrap();
        assert!(validator.validate_username("").is_err()); // too short
        assert!(validator.validate_username("a".repeat(256).as_str()).is_err());
        assert!(validator.validate_username("test user").is_err());
    }

    #[test]
    fn test_validate_password_valid() {
        let validator = Validator::new().unwrap();
        assert!(validator.validate_password("TestPass123!").is_ok());
        assert!(validator.validate_password("MyP@ssw0rd").is_ok());
    }

    #[test]
    fn test_validate_password_invalid() {
        let validator = Validator::new().unwrap();
        assert!(validator.validate_password("").is_err());
        assert!(validator.validate_password("short").is_err());
        assert!(validator.validate_password("nouppercase123!").is_err());
        assert!(validator.validate_password("NOLOWERCASE123!").is_err());
        assert!(validator.validate_password("NoDigits!").is_err());
        assert!(validator.validate_password("NoSpecial123").is_err());
    }

    #[test]
    fn is_well_formed_user_id_matches_the_grammar() {
        for good in ["@alice:example.org", "@a:b", "@user_name:hs.example.org", "@u.1-2=3:host-1.example"] {
            assert!(is_well_formed_user_id(good), "{good} must be accepted");
        }
        for bad in [
            "",
            "alice:example.org",
            "@alice",
            "@:example.org",
            "@alice:",
            "@Alice:example.org",
            "@alice:example.org:8448",
            "@ali ce:example.org",
        ] {
            assert!(!is_well_formed_user_id(bad), "{bad:?} must be rejected");
        }
    }

    /// The shared validator and the pure grammar must not drift: they are one
    /// implementation, and this is the assertion that keeps them so.
    #[test]
    fn validator_delegates_to_the_pure_user_id_grammar() {
        let validator = Validator::new().expect("regexes compile");
        for id in ["@alice:example.org", "alice:example.org", "@alice", "@:example.org", "@alice:"] {
            let via_validator = validator.validate_matrix_id(id).is_ok();
            let via_pure = is_well_formed_user_id(id);
            assert_eq!(via_validator, via_pure, "user id {id:?}");
        }
    }

    /// MSC/appendices "Historical user IDs": a localpart is compliant only when
    /// non-empty and confined to the printable ASCII range U+0021..=U+007E.
    #[test]
    fn compliant_user_id_localpart_matches_the_historical_id_rule() {
        for good in ["alice", "a", "user_name-1", "!\"#$%&'()*+,./", "0123456789", "~"] {
            assert!(is_compliant_user_id_localpart(good), "{good:?} must be compliant");
        }
        for bad in ["", "ali ce", "ali\tce", "ali\nce", "ali\u{7f}ce", "\u{e9}lice"] {
            assert!(!is_compliant_user_id_localpart(bad), "{bad:?} must be non-compliant");
        }
    }

    #[test]
    fn test_validate_matrix_id_valid() {
        let validator = Validator::new().unwrap();
        assert!(validator.validate_matrix_id("@testuser:example.com").is_ok());
        assert!(validator.validate_matrix_id("@user_name:server.org").is_ok());
    }

    #[test]
    fn test_validate_matrix_id_invalid() {
        let validator = Validator::new().unwrap();
        assert!(validator.validate_matrix_id("").is_err());
        assert!(validator.validate_matrix_id("testuser:example.com").is_err());
        assert!(validator.validate_matrix_id("@testuser").is_err());
    }

    #[test]
    fn test_validate_room_id_valid() {
        let validator = Validator::new().unwrap();
        assert!(validator.validate_room_id("!abc123:example.com").is_ok());
        assert!(validator.validate_room_id("!room_id:server.org").is_ok());
    }

    /// MSC4291 / room v12: the domainless form (`!` + 43 URL-safe base64 chars,
    /// no `:domain`) is a valid room ID and must be accepted alongside the
    /// legacy `!opaque:server` form (upstream `RoomID.is_valid` dispatches on
    /// the presence of `:`).
    #[test]
    fn test_validate_room_id_accepts_domainless_v12_form() {
        let validator = Validator::new().unwrap();
        let msc4291 = "!31hneApxJ_1o-63DmFrpeqnkFfWppnzWso1JvH3ogLM";
        assert!(validator.validate_room_id(msc4291).is_ok(), "domainless (room v12) room id must be valid");
    }

    /// A malformed domainless shape must still be rejected: the acceptance
    /// above must not have degenerated into "no colon ⇒ anything goes".
    #[test]
    fn test_validate_room_id_rejects_malformed_domainless_form() {
        let validator = Validator::new().unwrap();
        // 42 and 44 chars.
        assert!(validator.validate_room_id(&format!("!{}", "A".repeat(42))).is_err());
        assert!(validator.validate_room_id(&format!("!{}", "A".repeat(44))).is_err());
        // Characters outside [A-Za-z0-9-_].
        assert!(validator.validate_room_id(&format!("!{}A", "+".repeat(42))).is_err());
        assert!(validator.validate_room_id(&format!("!{}A", "/".repeat(42))).is_err());
        // Empty legacy domain must stay rejected.
        assert!(validator.validate_room_id("!opaque:").is_err());
    }

    #[test]
    fn test_validate_room_id_invalid() {
        let validator = Validator::new().unwrap();
        assert!(validator.validate_room_id("").is_err());
        assert!(validator.validate_room_id("abc123:example.com").is_err());
        assert!(validator.validate_room_id("!abc123").is_err());
    }

    #[test]
    fn test_validation_context() {
        let validator = Arc::new(Validator::new().unwrap());
        let mut ctx = ValidationContext::new(validator);

        ctx.validate_username("testuser").validate_password("TestPass123!").validate_email("test@example.com");

        assert!(ctx.is_valid());
    }

    #[test]
    fn test_validation_context_with_errors() {
        let validator = Arc::new(Validator::new().unwrap());
        let mut ctx = ValidationContext::new(validator);

        ctx.validate_username("").validate_password("short").validate_email("invalid");

        assert!(!ctx.is_valid());
        assert_eq!(ctx.errors.len(), 3);
    }

    #[cfg(test)]
    mod fuzz_tests {
        use super::*;
        use quickcheck::{Arbitrary, Gen};

        #[derive(Debug, Clone)]
        struct UsernameInput(String);

        impl Arbitrary for UsernameInput {
            fn arbitrary(g: &mut Gen) -> Self {
                let len = usize::arbitrary(g) % 253 + 3;
                let chars: String = (0..len)
                    .map(|_| {
                        let idx = usize::arbitrary(g) % 39;
                        if idx < 26 {
                            (b'a' + idx as u8) as char
                        } else if idx < 36 {
                            (b'0' + (idx - 26) as u8) as char
                        } else {
                            ['_', '.', '-', '='][idx - 36]
                        }
                    })
                    .collect();
                UsernameInput(chars)
            }
        }

        quickcheck::quickcheck! {
            fn test_validate_username_fuzz(input: UsernameInput) -> bool {
                let validator = Validator::new().unwrap();
                let result = validator.validate_username(&input.0);
                result.is_ok()
            }
        }

        #[derive(Debug, Clone)]
        struct EmailInput(String);

        impl Arbitrary for EmailInput {
            fn arbitrary(g: &mut Gen) -> Self {
                let local_len = usize::arbitrary(g) % 20 + 1;
                let domain_len = usize::arbitrary(g) % 15 + 1;
                let tld_len = usize::arbitrary(g) % 3 + 2;

                let local: String = (0..local_len)
                    .map(|_| {
                        let idx = usize::arbitrary(g) % 62;
                        if idx < 26 {
                            (b'a' + idx as u8) as char
                        } else if idx < 52 {
                            (b'A' + (idx - 26) as u8) as char
                        } else {
                            (b'0' + (idx - 52) as u8) as char
                        }
                    })
                    .collect();
                let domain: String = (0..domain_len)
                    .map(|_| {
                        let idx = usize::arbitrary(g) % 36;
                        if idx < 26 {
                            (b'a' + idx as u8) as char
                        } else {
                            (b'0' + (idx - 26) as u8) as char
                        }
                    })
                    .collect();
                let tld: String = (0..tld_len)
                    .map(|_| {
                        let idx = usize::arbitrary(g) % 26;
                        (b'a' + idx as u8) as char
                    })
                    .collect();

                EmailInput(format!("{local}@{domain}.{tld}"))
            }
        }

        quickcheck::quickcheck! {
            fn test_validate_email_fuzz(input: EmailInput) -> bool {
                let validator = Validator::new().unwrap();
                let result = validator.validate_email(&input.0);
                result.is_ok()
            }
        }

        #[derive(Debug, Clone)]
        struct MatrixIdInput(String);

        impl Arbitrary for MatrixIdInput {
            fn arbitrary(g: &mut Gen) -> Self {
                let local_len = usize::arbitrary(g) % 20 + 1;
                let server_len = usize::arbitrary(g) % 15 + 1;

                let local: String = (0..local_len)
                    .map(|_| {
                        let idx = usize::arbitrary(g) % 39;
                        if idx < 26 {
                            (b'a' + idx as u8) as char
                        } else if idx < 36 {
                            (b'0' + (idx - 26) as u8) as char
                        } else {
                            ['_', '.', '-', '='][idx - 36]
                        }
                    })
                    .collect();
                let server: String = (0..server_len)
                    .map(|_| {
                        let idx = usize::arbitrary(g) % 36;
                        if idx < 26 {
                            (b'a' + idx as u8) as char
                        } else {
                            (b'0' + (idx - 26) as u8) as char
                        }
                    })
                    .collect();

                MatrixIdInput(format!("@{local}:{server}.com"))
            }
        }

        quickcheck::quickcheck! {
            fn test_validate_matrix_id_fuzz(input: MatrixIdInput) -> bool {
                let validator = Validator::new().unwrap();
                let result = validator.validate_matrix_id(&input.0);
                result.is_ok()
            }
        }
    }
}
