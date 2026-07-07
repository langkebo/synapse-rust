use super::*;

#[derive(Clone, Debug, Default)]
pub struct InMemoryAccountDataStore {
    #[allow(clippy::type_complexity)]
    data: Arc<tokio::sync::RwLock<HashMap<(String, String), serde_json::Value>>>,
}

impl InMemoryAccountDataStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl crate::account_data::AccountDataStoreApi for InMemoryAccountDataStore {
    async fn get_account_data_content(
        &self,
        user_id: &str,
        data_type: &str,
    ) -> Result<Option<serde_json::Value>, ApiError> {
        Ok(self.data.read().await.get(&(user_id.to_string(), data_type.to_string())).cloned())
    }

    async fn list_account_data(&self, user_id: &str) -> Result<Vec<crate::account_data::AccountDataRecord>, ApiError> {
        let mut records: Vec<_> = self
            .data
            .read()
            .await
            .iter()
            .filter(|((uid, _), _)| uid == user_id)
            .map(|((_, data_type), content)| crate::account_data::AccountDataRecord {
                data_type: data_type.clone(),
                content: content.clone(),
            })
            .collect();
        records.sort_by(|a, b| a.data_type.cmp(&b.data_type));
        Ok(records)
    }

    async fn delete_account_data(&self, user_id: &str, data_type: &str) -> Result<bool, ApiError> {
        Ok(self.data.write().await.remove(&(user_id.to_string(), data_type.to_string())).is_some())
    }

    async fn upsert_account_data(
        &self,
        user_id: &str,
        data_type: &str,
        content: serde_json::Value,
    ) -> Result<(), ApiError> {
        self.data.write().await.insert((user_id.to_string(), data_type.to_string()), content);
        Ok(())
    }
}
