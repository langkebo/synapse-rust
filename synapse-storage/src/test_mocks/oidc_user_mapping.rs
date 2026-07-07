use super::*;

/// In-memory OIDC user mapping store mirroring [`crate::oidc_user_mapping::OidcUserMappingStorage`].
#[derive(Clone, Default)]
#[allow(clippy::type_complexity)]
pub struct InMemoryOidcUserMappingStore {
    mappings: Arc<RwLock<HashMap<(String, String), (String, i64, i64, i64)>>>,
}

impl InMemoryOidcUserMappingStore {
    pub fn new() -> Self {
        Self { mappings: Arc::new(RwLock::new(HashMap::new())) }
    }
}

#[async_trait::async_trait]
impl crate::oidc_user_mapping::OidcUserMappingStoreApi for InMemoryOidcUserMappingStore {
    async fn get_bound_user_id(&self, issuer: &str, subject: &str) -> Result<Option<String>, sqlx::Error> {
        Ok(self.mappings.read().await.get(&(issuer.to_string(), subject.to_string())).map(|v| v.0.clone()))
    }

    async fn update_last_authenticated(&self, issuer: &str, subject: &str, now_ts: i64) -> Result<(), sqlx::Error> {
        let mut map = self.mappings.write().await;
        if let Some(entry) = map.get_mut(&(issuer.to_string(), subject.to_string())) {
            entry.2 = now_ts;
            entry.3 += 1;
        }
        Ok(())
    }

    async fn insert_mapping(&self, issuer: &str, subject: &str, user_id: &str, now_ts: i64) -> Result<(), sqlx::Error> {
        self.mappings
            .write()
            .await
            .insert((issuer.to_string(), subject.to_string()), (user_id.to_string(), now_ts, now_ts, 1));
        Ok(())
    }
}
