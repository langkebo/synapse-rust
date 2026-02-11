use crate::common::ApiError;
use crate::common::constants::*;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

pub type ValidationResult = Result<(), ValidationError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
    pub code: String,
}

impl ValidationError {
    pub fn new(field: &str, message: &str, code: &str) -> Self {
        Self {
            field: field.to_string(),
            message: message.to_string(),
            code: code.to_string(),
        }
    }
}

impl From<ValidationError> for ApiError {
    fn from(err: ValidationError) -> Self {
        ApiError::bad_request(format!("{}: {}", err.field, err.message))
    }
}

#[derive(Debug, Clone)]
pub struct Validator {
    username_regex: Regex,
    email_regex: Regex,
    matrix_id_regex: Regex,
    room_id_regex: Regex,
    device_id_regex: Regex,
    url_regex: Regex,
}

impl Validator {
    pub fn new() -> Result<Self, regex::Error> {
        Ok(Self {
            // Matrix localpart: [a-z0-9._=-]+
            username_regex: Regex::new(r"^[a-z0-9._=\-]{1,255}$")?,
            email_regex: Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$")?,
            matrix_id_regex: Regex::new(r"^@[a-z0-9._=\-]+:[a-zA-Z0-9.-]+$")?,
            room_id_regex: Regex::new(r"^![a-zA-Z0-9._=\-]+:[a-zA-Z0-9.-]+$")?,
            device_id_regex: Regex::new(r"^[a-zA-Z0-9._\-]{1,255}$")?,
            url_regex: Regex::new(r"^https?://[a-zA-Z0-9.-]+(:[0-9]+)?(/.*)?$")?,
        })
    }

    pub fn validate_username(&self, username: &str) -> ValidationResult {
        if username.is_empty() {
            return Err(ValidationError::new(
                "username",
                "Username cannot be empty",
                "EMPTY",
            ));
        }

        if username.len() > MAX_USERNAME_LENGTH {
            return Err(ValidationError::new(
                "username",
                &format!("Username must be at most {} characters", MAX_USERNAME_LENGTH),
                "TOO_LONG",
            ));
        }

        if !self.username_regex.is_match(username) {
            return Err(ValidationError::new(
                "username",
                "Username contains invalid characters",
                "INVALID_FORMAT",
            ));
        }

        Ok(())
    }

    pub fn validate_password(&self, password: &str) -> ValidationResult {
        if password.is_empty() {
            return Err(ValidationError::new(
                "password",
                "Password cannot be empty",
                "EMPTY",
            ));
        }

        if password.len() < MIN_PASSWORD_LENGTH {
            return Err(ValidationError::new(
                "password",
                &format!("Password must be at least {} characters", MIN_PASSWORD_LENGTH),
                "TOO_SHORT",
            ));
        }

        if password.len() > MAX_PASSWORD_LENGTH {
            return Err(ValidationError::new(
                "password",
                &format!("Password must be at most {} characters", MAX_PASSWORD_LENGTH),
                "TOO_LONG",
            ));
        }

        let has_upper = password.chars().any(|c| c.is_uppercase());
        let has_lower = password.chars().any(|c| c.is_lowercase());
        let has_digit = password.chars().any(|c| c.is_ascii_digit());
        let has_special = password
            .chars()
            .any(|c| "!@#$%^&*()_+-=[]{}|;:,.<>?".contains(c));

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
            return Err(ValidationError::new(
                "password",
                "Password must contain at least one digit",
                "NO_DIGIT",
            ));
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

    pub fn validate_email(&self, email: &str) -> ValidationResult {
        if email.is_empty() {
            return Err(ValidationError::new(
                "email",
                "Email cannot be empty",
                "EMPTY",
            ));
        }

        if !self.email_regex.is_match(email) {
            return Err(ValidationError::new(
                "email",
                "Invalid email format",
                "INVALID_FORMAT",
            ));
        }

        Ok(())
    }

    pub fn validate_matrix_id(&self, user_id: &str) -> ValidationResult {
        if user_id.is_empty() {
            return Err(ValidationError::new(
                "user_id",
                "User ID cannot be empty",
                "EMPTY",
            ));
        }

        if !self.matrix_id_regex.is_match(user_id) {
            return Err(ValidationError::new(
                "user_id",
                "Invalid Matrix ID format",
                "INVALID_FORMAT",
            ));
        }

        Ok(())
    }

    pub fn validate_room_id(&self, room_id: &str) -> ValidationResult {
        if room_id.is_empty() {
            return Err(ValidationError::new(
                "room_id",
                "Room ID cannot be empty",
                "EMPTY",
            ));
        }

        if !self.room_id_regex.is_match(room_id) {
            return Err(ValidationError::new(
                "room_id",
                "Invalid room ID format",
                "INVALID_FORMAT",
            ));
        }

        Ok(())
    }

    pub fn validate_device_id(&self, device_id: &str) -> ValidationResult {
        if device_id.is_empty() {
            return Err(ValidationError::new(
                "device_id",
                "Device ID cannot be empty",
                "EMPTY",
            ));
        }

        if device_id.len() > MAX_DEVICE_ID_LENGTH {
            return Err(ValidationError::new(
                "device_id",
                &format!("Device ID must be at most {} characters", MAX_DEVICE_ID_LENGTH),
                "TOO_LONG",
            ));
        }

        if !self.device_id_regex.is_match(device_id) {
            return Err(ValidationError::new(
                "device_id",
                "Device ID contains invalid characters",
                "INVALID_FORMAT",
            ));
        }

        Ok(())
    }

    pub fn validate_url(&self, url: &str) -> ValidationResult {
        if url.is_empty() {
            return Err(ValidationError::new("url", "URL cannot be empty", "EMPTY"));
        }

        if !self.url_regex.is_match(url) {
            return Err(ValidationError::new(
                "url",
                "Invalid URL format",
                "INVALID_FORMAT",
            ));
        }

        Ok(())
    }

    pub fn validate_string_length(
        &self,
        field: &str,
        value: &str,
        min: usize,
        max: usize,
    ) -> ValidationResult {
        if min > 0 && value.is_empty() {
            return Err(ValidationError::new(
                field,
                &format!("{} cannot be empty", field),
                "EMPTY",
            ));
        }

        if value.len() < min {
            return Err(ValidationError::new(
                field,
                &format!("{} must be at least {} characters", field, min),
                "TOO_SHORT",
            ));
        }

        if value.len() > max {
            return Err(ValidationError::new(
                field,
                &format!("{} must be at most {} characters", field, max),
                "TOO_LONG",
            ));
        }

        Ok(())
    }

    pub fn validate_display_name(&self, display_name: &str) -> ValidationResult {
        self.validate_string_length("display_name", display_name, 1, MAX_DISPLAY_NAME_LENGTH)
    }

    pub fn validate_reason(&self, reason: &str) -> ValidationResult {
        self.validate_string_length("reason", reason, 0, MAX_REASON_LENGTH)
    }

    pub fn validate_message(&self, message: &str) -> ValidationResult {
        self.validate_string_length("message", message, 1, MAX_MESSAGE_LENGTH)
    }

    pub fn validate_limit(&self, limit: i64, min: i64, max: i64) -> ValidationResult {
        if limit < min {
            return Err(ValidationError::new(
                "limit",
                format!("Limit must be at least {}", min).as_str(),
                "TOO_SMALL",
            ));
        }

        if limit > max {
            return Err(ValidationError::new(
                "limit",
                format!("Limit must be at most {}", max).as_str(),
                "TOO_LARGE",
            ));
        }

        Ok(())
    }

    pub fn validate_timestamp(&self, timestamp: i64) -> ValidationResult {
        let now = chrono::Utc::now().timestamp();
        let window = TIMESTAMP_WINDOW_SECONDS;
        let min_valid = now - window;
        let max_valid = now + window;

        if timestamp < min_valid {
            return Err(ValidationError::new(
                "timestamp",
                "Timestamp is too old",
                "TOO_OLD",
            ));
        }

        if timestamp > max_valid {
            return Err(ValidationError::new(
                "timestamp",
                "Timestamp is too far in the future",
                "TOO_FUTURE",
            ));
        }

        Ok(())
    }

    pub fn validate_ip_address(&self, ip: &str) -> ValidationResult {
        if ip.is_empty() {
            return Err(ValidationError::new(
                "ip_address",
                "IP address cannot be empty",
                "EMPTY",
            ));
        }

        if ip.parse::<std::net::IpAddr>().is_err() {
            return Err(ValidationError::new(
                "ip_address",
                "Invalid IP address format",
                "INVALID_FORMAT",
            ));
        }

        Ok(())
    }
}

impl Default for Validator {
    fn default() -> Self {
        Self::new().unwrap_or_else(|e| {
            panic!(
                "Critical: Failed to create default validator - regex compilation error: {}. \
                 This indicates a programming error in regex patterns.",
                e
            )
        })
    }
}

#[derive(Debug, Clone)]
pub struct ValidationContext {
    validator: Arc<Validator>,
    errors: Vec<ValidationError>,
}

impl ValidationContext {
    pub fn new(validator: Arc<Validator>) -> Self {
        Self {
            validator,
            errors: Vec::new(),
        }
    }

    pub fn validate_username(&mut self, username: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_username(username) {
            self.errors.push(e);
        }
        self
    }

    pub fn validate_password(&mut self, password: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_password(password) {
            self.errors.push(e);
        }
        self
    }

    pub fn validate_email(&mut self, email: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_email(email) {
            self.errors.push(e);
        }
        self
    }

    pub fn validate_matrix_id(&mut self, user_id: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_matrix_id(user_id) {
            self.errors.push(e);
        }
        self
    }

    pub fn validate_room_id(&mut self, room_id: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_room_id(room_id) {
            self.errors.push(e);
        }
        self
    }

    pub fn validate_device_id(&mut self, device_id: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_device_id(device_id) {
            self.errors.push(e);
        }
        self
    }

    pub fn validate_url(&mut self, url: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_url(url) {
            self.errors.push(e);
        }
        self
    }

    pub fn validate_display_name(&mut self, display_name: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_display_name(display_name) {
            self.errors.push(e);
        }
        self
    }

    pub fn validate_reason(&mut self, reason: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_reason(reason) {
            self.errors.push(e);
        }
        self
    }

    pub fn validate_message(&mut self, message: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_message(message) {
            self.errors.push(e);
        }
        self
    }

    pub fn validate_limit(&mut self, limit: i64, min: i64, max: i64) -> &mut Self {
        if let Err(e) = self.validator.validate_limit(limit, min, max) {
            self.errors.push(e);
        }
        self
    }

    pub fn validate_timestamp(&mut self, timestamp: i64) -> &mut Self {
        if let Err(e) = self.validator.validate_timestamp(timestamp) {
            self.errors.push(e);
        }
        self
    }

    pub fn validate_ip_address(&mut self, ip: &str) -> &mut Self {
        if let Err(e) = self.validator.validate_ip_address(ip) {
            self.errors.push(e);
        }
        self
    }

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

    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn into_result(self) -> Result<(), ApiError> {
        if self.errors.is_empty() {
            Ok(())
        } else {
            Err(ApiError::bad_request(format!(
                "Validation failed: {}",
                self.errors
                    .iter()
                    .map(|e| format!("{}: {}", e.field, e.message))
                    .collect::<Vec<_>>()
                    .join(", ")
            )))
        }
    }

    pub fn into_error_map(self) -> HashMap<String, String> {
        self.errors
            .into_iter()
            .map(|e| (e.field, e.message))
            .collect()
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
    }

    #[test]
    fn test_validate_username_invalid() {
        let validator = Validator::new().unwrap();
        assert!(validator.validate_username("").is_err());
        assert!(validator
            .validate_username("a".repeat(256).as_str())
            .is_err());
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
    fn test_validate_matrix_id_valid() {
        let validator = Validator::new().unwrap();
        assert!(validator
            .validate_matrix_id("@testuser:example.com")
            .is_ok());
        assert!(validator
            .validate_matrix_id("@user_name:server.org")
            .is_ok());
    }

    #[test]
    fn test_validate_matrix_id_invalid() {
        let validator = Validator::new().unwrap();
        assert!(validator.validate_matrix_id("").is_err());
        assert!(validator
            .validate_matrix_id("testuser:example.com")
            .is_err());
        assert!(validator.validate_matrix_id("@testuser").is_err());
    }

    #[test]
    fn test_validate_room_id_valid() {
        let validator = Validator::new().unwrap();
        assert!(validator.validate_room_id("!abc123:example.com").is_ok());
        assert!(validator.validate_room_id("!room_id:server.org").is_ok());
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

        ctx.validate_username("testuser")
            .validate_password("TestPass123!")
            .validate_email("test@example.com");

        assert!(ctx.is_valid());
    }

    #[test]
    fn test_validation_context_with_errors() {
        let validator = Arc::new(Validator::new().unwrap());
        let mut ctx = ValidationContext::new(validator);

        ctx.validate_username("")
            .validate_password("short")
            .validate_email("invalid");

        assert!(!ctx.is_valid());
        assert_eq!(ctx.errors.len(), 3);
    }
}
