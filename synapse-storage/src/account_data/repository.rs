use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;

use super::AccountDataRecord;

#[async_trait]
pub trait AccountDataRepository: Send + Sync + std::fmt::Debug {
    /// Returns a reference to the database connection pool.
    fn pool(&self) -> &Arc<sqlx::PgPool>;

    async fn get_account_data_content(&self, user_id: &str, data_type: &str) -> Result<Option<Value>, sqlx::Error>;

    async fn list_account_data(&self, user_id: &str) -> Result<Vec<AccountDataRecord>, sqlx::Error>;

    async fn delete_account_data(&self, user_id: &str, data_type: &str) -> Result<bool, sqlx::Error>;

    async fn upsert_account_data(&self, user_id: &str, data_type: &str, content: Value) -> Result<(), sqlx::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_account_data_repository_is_trait_object_safe() {
        fn _accept_trait_object(_: &dyn AccountDataRepository) {}
    }

    #[test]
    fn test_boxed_account_data_repository_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<Box<dyn AccountDataRepository>>();
        assert_sync::<Box<dyn AccountDataRepository>>();
    }

    #[test]
    fn test_arced_account_data_repository_is_send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}
        assert_send::<std::sync::Arc<dyn AccountDataRepository>>();
        assert_sync::<std::sync::Arc<dyn AccountDataRepository>>();
    }
}
