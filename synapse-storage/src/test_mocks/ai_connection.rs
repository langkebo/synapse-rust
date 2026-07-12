#[cfg(feature = "openclaw-routes")]
use super::*;

#[cfg(feature = "openclaw-routes")]
#[derive(Clone, Default)]
pub struct InMemoryAiConnectionStore {
    connections: Arc<RwLock<HashMap<String, crate::ai_connection::AiConnection>>>,
}

#[cfg(feature = "openclaw-routes")]
impl InMemoryAiConnectionStore {
    pub fn new() -> Self {
        Self { connections: Arc::new(RwLock::new(HashMap::new())) }
    }
}

#[cfg(feature = "openclaw-routes")]
#[async_trait::async_trait]
impl crate::ai_connection::AiConnectionStoreApi for InMemoryAiConnectionStore {
    async fn create_connection(&self, conn: &crate::ai_connection::AiConnection) -> Result<(), sqlx::Error> {
        self.connections.write().await.insert(conn.id.clone(), conn.clone());
        Ok(())
    }

    async fn get_connection(&self, id: &str) -> Result<Option<crate::ai_connection::AiConnection>, sqlx::Error> {
        Ok(self.connections.read().await.get(id).cloned())
    }

    async fn get_user_connections(
        &self,
        user_id: &str,
    ) -> Result<Vec<crate::ai_connection::AiConnection>, sqlx::Error> {
        let mut results: Vec<_> =
            self.connections.read().await.values().filter(|c| c.user_id == user_id).cloned().collect();
        results.sort_by_key(|c| std::cmp::Reverse(c.created_ts));
        Ok(results)
    }

    async fn get_user_provider_connection(
        &self,
        user_id: &str,
        provider: &str,
    ) -> Result<Option<crate::ai_connection::AiConnection>, sqlx::Error> {
        let conns = self.connections.read().await;
        let mut matches: Vec<_> =
            conns.values().filter(|c| c.user_id == user_id && c.provider == provider && c.is_active).collect();
        matches.sort_by_key(|c| std::cmp::Reverse(c.created_ts));
        Ok(matches.first().cloned().cloned())
    }

    async fn update_connection_status(&self, id: &str, is_active: bool) -> Result<(), sqlx::Error> {
        if let Some(conn) = self.connections.write().await.get_mut(id) {
            conn.is_active = is_active;
            conn.updated_ts = Some(chrono::Utc::now().timestamp_millis());
        }
        Ok(())
    }

    async fn delete_connection(&self, id: &str) -> Result<(), sqlx::Error> {
        self.connections.write().await.remove(id);
        Ok(())
    }
}
