use serde::{Deserialize, Serialize};
use synapse_common::current_timestamp_millis;

/// The `PasswordPolicy` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordPolicy {
    /// The `min_length` field.
    pub min_length: u8,
    /// The `max_length` field.
    pub max_length: u8,
    /// The `require_uppercase` field.
    pub require_uppercase: bool,
    /// The `require_lowercase` field.
    pub require_lowercase: bool,
    /// The `require_digit` field.
    pub require_digit: bool,
    /// The `require_special` field.
    pub require_special: bool,
    /// The `max_age_days` field.
    pub max_age_days: u32,
    /// The `history_count` field.
    pub history_count: u8,
    /// The `max_failed_attempts` field.
    pub max_failed_attempts: u8,
    /// The `lockout_duration_minutes` field.
    pub lockout_duration_minutes: u32,
    /// The `force_first_login_change` field.
    pub force_first_login_change: bool,
}

impl Default for PasswordPolicy {
    fn default() -> Self {
        Self {
            min_length: 8,
            max_length: 128,
            require_uppercase: true,
            require_lowercase: true,
            require_digit: true,
            require_special: true,
            max_age_days: 90,
            history_count: 5,
            max_failed_attempts: 5,
            lockout_duration_minutes: 30,
            force_first_login_change: true,
        }
    }
}

/// The `PasswordValidationResult` struct.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordValidationResult {
    /// The `is_valid` field.
    pub is_valid: bool,
    /// The `errors` field.
    pub errors: Vec<String>,
    /// The `strength_score` field.
    pub strength_score: u8,
}

impl PasswordPolicy {
    /// See [`validate`].
    pub fn validate(&self, password: &str) -> PasswordValidationResult {
        let mut errors = Vec::new();
        let mut score: u8 = 0;

        if password.len() < self.min_length as usize {
            errors.push(format!("密码长度不能少于 {} 个字符", self.min_length));
        } else {
            score += 20;
        }

        if password.len() > self.max_length as usize {
            errors.push(format!("密码长度不能超过 {} 个字符", self.max_length));
        }

        if self.require_uppercase && !password.chars().any(|c| c.is_uppercase()) {
            errors.push("密码必须包含至少一个大写字母".to_string());
        } else if password.chars().any(|c| c.is_uppercase()) {
            score += 20;
        }

        if self.require_lowercase && !password.chars().any(|c| c.is_lowercase()) {
            errors.push("密码必须包含至少一个小写字母".to_string());
        } else if password.chars().any(|c| c.is_lowercase()) {
            score += 20;
        }

        if self.require_digit && !password.chars().any(|c| c.is_numeric()) {
            errors.push("密码必须包含至少一个数字".to_string());
        } else if password.chars().any(|c| c.is_numeric()) {
            score += 20;
        }

        let special_chars: &[char] = &[
            '!', '@', '#', '$', '%', '^', '&', '*', '(', ')', '_', '+', '-', '=', '[', ']', '{', '}', '|', ';', ':',
            ',', '.', '<', '>', '?',
        ];
        if self.require_special && !password.chars().any(|c| special_chars.contains(&c)) {
            errors.push("密码必须包含至少一个特殊字符 (!@#$%^&* 等)".to_string());
        } else if password.chars().any(|c| special_chars.contains(&c)) {
            score += 20;
        }

        PasswordValidationResult { is_valid: errors.is_empty(), errors, strength_score: score.min(100) }
    }

    /// See [`is_password_expired`].
    pub fn is_password_expired(&self, password_changed_ts: Option<i64>) -> bool {
        if self.max_age_days == 0 {
            return false;
        }

        match password_changed_ts {
            Some(changed_at) => {
                let now = current_timestamp_millis();
                let max_age_ms = (self.max_age_days as i64) * 24 * 60 * 60 * 1000;
                now > changed_at + max_age_ms
            }
            None => true,
        }
    }

    /// See [`calculate_password_expires_at`].
    pub fn calculate_password_expires_at(&self) -> i64 {
        if self.max_age_days == 0 {
            return 0;
        }
        let now = current_timestamp_millis();
        let max_age_ms = (self.max_age_days as i64) * 24 * 60 * 60 * 1000;
        now + max_age_ms
    }

    /// See [`calculate_lockout_until`].
    pub fn calculate_lockout_until(&self) -> i64 {
        let now = current_timestamp_millis();
        let lockout_ms = (self.lockout_duration_minutes as i64) * 60 * 1000;
        now + lockout_ms
    }
}

/// The `PasswordPolicyService` struct.
pub struct PasswordPolicyService {
    policy: PasswordPolicy,
}

impl PasswordPolicyService {
    /// See [`new`].
    pub fn new(_pool: sqlx::PgPool) -> Self {
        Self { policy: PasswordPolicy::default() }
    }

    /// See [`from_policy`].
    pub fn from_policy(policy: PasswordPolicy) -> Self {
        Self { policy }
    }

    /// See [`policy`].
    pub fn policy(&self) -> &PasswordPolicy {
        &self.policy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_password_validation_valid() {
        let policy = PasswordPolicy::default();
        let result = policy.validate("Password123!");
        assert!(result.is_valid);
        assert!(result.errors.is_empty());
        assert_eq!(result.strength_score, 100);
    }

    #[test]
    fn test_password_validation_too_short() {
        let policy = PasswordPolicy::default();
        let result = policy.validate("Pass1!");
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("少于")));
    }

    #[test]
    fn test_password_validation_missing_uppercase() {
        let policy = PasswordPolicy::default();
        let result = policy.validate("password123!");
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("大写字母")));
    }

    #[test]
    fn test_password_validation_missing_lowercase() {
        let policy = PasswordPolicy::default();
        let result = policy.validate("PASSWORD123!");
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("小写字母")));
    }

    #[test]
    fn test_password_validation_missing_digit() {
        let policy = PasswordPolicy::default();
        let result = policy.validate("Password!");
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("数字")));
    }

    #[test]
    fn test_password_validation_missing_special() {
        let policy = PasswordPolicy::default();
        let result = policy.validate("Password123");
        assert!(!result.is_valid);
        assert!(result.errors.iter().any(|e| e.contains("特殊字符")));
    }

    #[test]
    fn test_password_expiry() {
        let policy = PasswordPolicy { max_age_days: 90, ..Default::default() };

        let now = current_timestamp_millis();
        let ninety_one_days_ago = now - (91 * 24 * 60 * 60 * 1000);

        assert!(policy.is_password_expired(Some(ninety_one_days_ago)));
        assert!(!policy.is_password_expired(Some(now)));
    }

    #[test]
    fn test_password_never_expires() {
        let policy = PasswordPolicy { max_age_days: 0, ..Default::default() };

        assert!(!policy.is_password_expired(None));
        assert!(!policy.is_password_expired(Some(0)));
    }
}
