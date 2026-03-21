use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordPolicy {
    pub min_length: u8,
    pub max_length: u8,
    pub require_uppercase: bool,
    pub require_lowercase: bool,
    pub require_digit: bool,
    pub require_special: bool,
    pub max_age_days: u32,
    pub history_count: u8,
    pub max_failed_attempts: u8,
    pub lockout_duration_minutes: u32,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PasswordValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub strength_score: u8,
}

impl PasswordPolicy {
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
            '!', '@', '#', '$', '%', '^', '&', '*', '(', ')', '_', '+', '-', '=', '[', ']', '{',
            '}', '|', ';', ':', ',', '.', '<', '>', '?',
        ];
        if self.require_special && !password.chars().any(|c| special_chars.contains(&c)) {
            errors.push("密码必须包含至少一个特殊字符 (!@#$%^&* 等)".to_string());
        } else if password.chars().any(|c| special_chars.contains(&c)) {
            score += 20;
        }

        PasswordValidationResult {
            is_valid: errors.is_empty(),
            errors,
            strength_score: score.min(100),
        }
    }

    pub fn is_password_expired(&self, password_changed_ts: Option<i64>) -> bool {
        if self.max_age_days == 0 {
            return false;
        }

        match password_changed_ts {
            Some(changed_at) => {
                let now = chrono::Utc::now().timestamp_millis();
                let max_age_ms = (self.max_age_days as i64) * 24 * 60 * 60 * 1000;
                now > changed_at + max_age_ms
            }
            None => true,
        }
    }

    pub fn calculate_password_expires_at(&self) -> i64 {
        if self.max_age_days == 0 {
            return 0;
        }
        let now = chrono::Utc::now().timestamp_millis();
        let max_age_ms = (self.max_age_days as i64) * 24 * 60 * 60 * 1000;
        now + max_age_ms
    }

    pub fn calculate_lockout_until(&self) -> i64 {
        let now = chrono::Utc::now().timestamp_millis();
        let lockout_ms = (self.lockout_duration_minutes as i64) * 60 * 1000;
        now + lockout_ms
    }
}

pub struct PasswordPolicyService {
    pool: PgPool,
    policy: PasswordPolicy,
}

impl PasswordPolicyService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            policy: PasswordPolicy::default(),
        }
    }

    pub fn policy(&self) -> &PasswordPolicy {
        &self.policy
    }

    pub async fn load_policy(&mut self) -> Result<(), sqlx::Error> {
        let rows = sqlx::query("SELECT name, value FROM password_policy")
            .fetch_all(&self.pool)
            .await?;

        for row in rows {
            let name: String = row.try_get("name")?;
            let value: String = row.try_get("value")?;

            match name.as_str() {
                "min_length" => {
                    if let Ok(v) = value.parse() {
                        self.policy.min_length = v;
                    }
                }
                "max_length" => {
                    if let Ok(v) = value.parse() {
                        self.policy.max_length = v;
                    }
                }
                "require_uppercase" => {
                    self.policy.require_uppercase = value == "true";
                }
                "require_lowercase" => {
                    self.policy.require_lowercase = value == "true";
                }
                "require_digit" => {
                    self.policy.require_digit = value == "true";
                }
                "require_special" => {
                    self.policy.require_special = value == "true";
                }
                "max_age_days" => {
                    if let Ok(v) = value.parse() {
                        self.policy.max_age_days = v;
                    }
                }
                "history_count" => {
                    if let Ok(v) = value.parse() {
                        self.policy.history_count = v;
                    }
                }
                "max_failed_attempts" => {
                    if let Ok(v) = value.parse() {
                        self.policy.max_failed_attempts = v;
                    }
                }
                "lockout_duration_minutes" => {
                    if let Ok(v) = value.parse() {
                        self.policy.lockout_duration_minutes = v;
                    }
                }
                "force_first_login_change" => {
                    self.policy.force_first_login_change = value == "true";
                }
                _ => {}
            }
        }

        Ok(())
    }

    pub async fn add_password_to_history(
        &self,
        user_id: &str,
        password_hash: &str,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query(
            "INSERT INTO password_history (user_id, password_hash, created_ts) VALUES ($1, $2, $3)",
        )
        .bind(user_id)
        .bind(password_hash)
        .bind(now)
        .execute(&self.pool)
        .await?;

        self.cleanup_password_history(user_id).await
    }

    async fn cleanup_password_history(&self, user_id: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM password_history 
            WHERE user_id = $1 
            AND id NOT IN (
                SELECT id FROM password_history 
                WHERE user_id = $1 
                ORDER BY created_ts DESC 
                LIMIT $2
            )
            "#,
        )
        .bind(user_id)
        .bind(self.policy.history_count as i64)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn is_password_in_history(
        &self,
        user_id: &str,
        password_hash: &str,
    ) -> Result<bool, sqlx::Error> {
        let row = sqlx::query(
            "SELECT COUNT(*) as count FROM password_history WHERE user_id = $1 AND password_hash = $2"
        )
        .bind(user_id)
        .bind(password_hash)
        .fetch_one(&self.pool)
        .await?;

        let count: i64 = row.try_get("count")?;
        Ok(count > 0)
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
        let policy = PasswordPolicy {
            max_age_days: 90,
            ..Default::default()
        };

        let now = chrono::Utc::now().timestamp_millis();
        let ninety_one_days_ago = now - (91 * 24 * 60 * 60 * 1000);

        assert!(policy.is_password_expired(Some(ninety_one_days_ago)));
        assert!(!policy.is_password_expired(Some(now)));
    }

    #[test]
    fn test_password_never_expires() {
        let policy = PasswordPolicy {
            max_age_days: 0,
            ..Default::default()
        };

        assert!(!policy.is_password_expired(None));
        assert!(!policy.is_password_expired(Some(0)));
    }
}
