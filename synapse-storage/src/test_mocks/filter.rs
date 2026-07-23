use super::*;
use synapse_common::current_timestamp_millis;

use std::sync::atomic::{AtomicI64, Ordering};

use crate::filter::{CreateFilterRequest, Filter, FilterStoreApi};

/// In-memory [`FilterStoreApi`] backed by a `HashMap` keyed on
/// `(user_id, filter_id)`. Auto-increments the synthetic `id` field to mirror
/// the Postgres serial column.
#[derive(Clone, Debug, Default)]
pub struct InMemoryFilterStore {
    #[allow(clippy::type_complexity)]
    filters: Arc<RwLock<HashMap<(String, String), Filter>>>,
    next_id: Arc<AtomicI64>,
}

impl InMemoryFilterStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl FilterStoreApi for InMemoryFilterStore {
    async fn create_filter(&self, request: CreateFilterRequest) -> Result<Filter, ApiError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let filter = Filter {
            id,
            user_id: request.user_id.clone(),
            filter_id: request.filter_id.clone(),
            content: request.content.clone(),
            created_ts: current_timestamp_millis(),
        };
        self.filters.write().await.insert((request.user_id, request.filter_id), filter.clone());
        Ok(filter)
    }

    async fn get_filter(&self, user_id: &str, filter_id: &str) -> Result<Option<Filter>, ApiError> {
        Ok(self.filters.read().await.get(&(user_id.to_string(), filter_id.to_string())).cloned())
    }

    async fn get_filters_by_user(&self, user_id: &str) -> Result<Vec<Filter>, ApiError> {
        let mut filters: Vec<_> =
            self.filters.read().await.values().filter(|f| f.user_id == user_id).cloned().collect();
        filters.sort_by(|a, b| a.filter_id.cmp(&b.filter_id));
        Ok(filters)
    }

    async fn delete_filter(&self, user_id: &str, filter_id: &str) -> Result<bool, ApiError> {
        Ok(self.filters.write().await.remove(&(user_id.to_string(), filter_id.to_string())).is_some())
    }

    async fn delete_filters_by_user(&self, user_id: &str) -> Result<u64, ApiError> {
        let mut filters = self.filters.write().await;
        let before = filters.len();
        filters.retain(|_, f| f.user_id != user_id);
        Ok((before - filters.len()) as u64)
    }
}
