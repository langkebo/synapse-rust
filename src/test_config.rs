//! Test configuration utilities
//!
//! Provides centralized configuration for test environments,
//! eliminating hardcoded connection strings and paths.

/// Returns the test database URL from environment or default
///
/// Reads from TEST_DATABASE_URL environment variable.
/// Default: postgres://synapse:synapse@localhost:5432/synapse_test
pub fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string())
}

/// Returns the test Redis URL from environment or default
///
/// Reads from TEST_REDIS_URL environment variable.
/// Default: redis://localhost:6379
pub fn test_redis_url() -> String {
    std::env::var("TEST_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_url_default() {
        std::env::remove_var("TEST_DATABASE_URL");
        assert_eq!(
            test_database_url(),
            "postgres://synapse:synapse@localhost:5432/synapse_test"
        );
    }

    #[test]
    fn test_database_url_from_env() {
        std::env::set_var(
            "TEST_DATABASE_URL",
            "postgres://custom:custom@localhost:5432/custom",
        );
        assert_eq!(
            test_database_url(),
            "postgres://custom:custom@localhost:5432/custom"
        );
        std::env::remove_var("TEST_DATABASE_URL");
    }
}
