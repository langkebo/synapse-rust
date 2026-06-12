use base64::Engine;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OAuthClient {
    pub client_id: String,
    pub client_secret: String,
    pub client_name: Option<String>,
    /// JSONB array stored as String; deserialized on read.
    pub redirect_uris: serde_json::Value,
    pub grant_types: serde_json::Value,
    pub response_types: serde_json::Value,
    pub scope: String,
    pub created_ts: i64,
    pub is_confidential: bool,
}

impl OAuthClient {
    pub fn redirect_uris_vec(&self) -> Vec<String> {
        self.redirect_uris
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default()
    }

    pub fn grant_types_vec(&self) -> Vec<String> {
        self.grant_types
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default()
    }

    pub fn response_types_vec(&self) -> Vec<String> {
        self.response_types
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default()
    }
}

#[derive(Clone)]
pub struct OAuthClientStorage {
    pool: std::sync::Arc<sqlx::PgPool>,
}

impl OAuthClientStorage {
    pub fn new(pool: &std::sync::Arc<sqlx::PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    pub async fn register_client(&self, client: &OAuthClient) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO oauth_clients
                (client_id, client_secret, client_name, redirect_uris, grant_types,
                 response_types, scope, created_ts, is_confidential)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            "#,
        )
        .bind(&client.client_id)
        .bind(&client.client_secret)
        .bind(&client.client_name)
        .bind(&client.redirect_uris)
        .bind(&client.grant_types)
        .bind(&client.response_types)
        .bind(&client.scope)
        .bind(client.created_ts)
        .bind(client.is_confidential)
        .execute(self.pool.as_ref())
        .await?;
        Ok(())
    }

    pub async fn get_client(&self, client_id: &str) -> Result<Option<OAuthClient>, sqlx::Error> {
        sqlx::query_as::<_, OAuthClient>(
            r#"
            SELECT client_id, client_secret, client_name, redirect_uris, grant_types,
                   response_types, scope, created_ts, is_confidential
            FROM oauth_clients WHERE client_id = $1
            "#,
        )
        .bind(client_id)
        .fetch_optional(self.pool.as_ref())
        .await
    }

    pub async fn validate_client(&self, client_id: &str, redirect_uri: &str) -> Result<bool, sqlx::Error> {
        let client = self.get_client(client_id).await?;
        match client {
            Some(c) => {
                let uris = c.redirect_uris_vec();
                Ok(uris.contains(&redirect_uri.to_string()))
            }
            None => Ok(false),
        }
    }

    /// Generate a new client_id and client_secret, persist the client, and return it.
    pub async fn create_dynamic_client(
        &self,
        client_name: Option<&str>,
        redirect_uris: Vec<String>,
        grant_types: Vec<String>,
        response_types: Vec<String>,
        scope: &str,
        is_confidential: bool,
    ) -> Result<OAuthClient, sqlx::Error> {
        let client_id = uuid::Uuid::new_v4().to_string();
        let client_secret = Self::generate_client_secret();
        let now_ts = Utc::now().timestamp_millis();

        let redirect_uris_json =
            serde_json::Value::Array(redirect_uris.into_iter().map(serde_json::Value::String).collect());
        let grant_types_json =
            serde_json::Value::Array(grant_types.into_iter().map(serde_json::Value::String).collect());
        let response_types_json =
            serde_json::Value::Array(response_types.into_iter().map(serde_json::Value::String).collect());

        let client = OAuthClient {
            client_id: client_id.clone(),
            client_secret: client_secret.clone(),
            client_name: client_name.map(String::from),
            redirect_uris: redirect_uris_json,
            grant_types: grant_types_json,
            response_types: response_types_json,
            scope: scope.to_string(),
            created_ts: now_ts,
            is_confidential,
        };

        self.register_client(&client).await?;
        Ok(client)
    }

    fn generate_client_secret() -> String {
        use rand::RngCore;
        let mut bytes = [0u8; 32];
        rand::rngs::OsRng.fill_bytes(&mut bytes);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_client() -> OAuthClient {
        OAuthClient {
            client_id: "test-client-id".to_string(),
            client_secret: "test-secret".to_string(),
            client_name: Some("Test App".to_string()),
            redirect_uris: serde_json::json!(["https://app.test/callback", "https://app.test/redirect"]),
            grant_types: serde_json::json!(["authorization_code", "refresh_token"]),
            response_types: serde_json::json!(["code"]),
            scope: "openid profile".to_string(),
            created_ts: 1700000000000,
            is_confidential: true,
        }
    }

    #[test]
    fn test_redirect_uris_vec() {
        let client = sample_client();
        let uris = client.redirect_uris_vec();
        assert_eq!(uris.len(), 2);
        assert!(uris.contains(&"https://app.test/callback".to_string()));
        assert!(uris.contains(&"https://app.test/redirect".to_string()));
    }

    #[test]
    fn test_grant_types_vec() {
        let client = sample_client();
        let types = client.grant_types_vec();
        assert_eq!(types.len(), 2);
        assert!(types.contains(&"authorization_code".to_string()));
        assert!(types.contains(&"refresh_token".to_string()));
    }

    #[test]
    fn test_response_types_vec() {
        let client = sample_client();
        let types = client.response_types_vec();
        assert_eq!(types.len(), 1);
        assert!(types.contains(&"code".to_string()));
    }

    #[test]
    fn test_redirect_uris_vec_empty() {
        let client = OAuthClient {
            client_id: "test".to_string(),
            client_secret: "secret".to_string(),
            client_name: None,
            redirect_uris: serde_json::json!([]),
            grant_types: serde_json::json!([]),
            response_types: serde_json::json!([]),
            scope: "openid".to_string(),
            created_ts: 0,
            is_confidential: false,
        };
        assert!(client.redirect_uris_vec().is_empty());
    }

    #[test]
    fn test_redirect_uris_vec_invalid_json() {
        let client = OAuthClient {
            client_id: "test".to_string(),
            client_secret: "secret".to_string(),
            client_name: None,
            redirect_uris: serde_json::json!("not an array"),
            grant_types: serde_json::json!("not an array"),
            response_types: serde_json::json!("not an array"),
            scope: "openid".to_string(),
            created_ts: 0,
            is_confidential: false,
        };
        assert!(client.redirect_uris_vec().is_empty());
        assert!(client.grant_types_vec().is_empty());
        assert!(client.response_types_vec().is_empty());
    }

    #[test]
    fn test_oauth_client_clone() {
        let client = sample_client();
        let cloned = client.clone();
        assert_eq!(cloned.client_id, client.client_id);
        assert_eq!(cloned.client_secret, client.client_secret);
        assert_eq!(cloned.scope, client.scope);
    }

    #[test]
    fn test_oauth_client_serialization() {
        let client = sample_client();
        let json = serde_json::to_string(&client).unwrap();
        assert!(json.contains("test-client-id"));
        assert!(json.contains("Test App"));

        let deserialized: OAuthClient = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.client_id, "test-client-id");
        assert_eq!(deserialized.scope, "openid profile");
    }

    #[test]
    fn test_generate_client_secret() {
        let secret1 = OAuthClientStorage::generate_client_secret();
        let secret2 = OAuthClientStorage::generate_client_secret();

        // Secrets should be different (extremely unlikely to collide)
        assert_ne!(secret1, secret2);
        // Should be valid base64url
        assert!(base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(&secret1).is_ok());
    }

    // DB-dependent tests marked with #[ignore]

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_register_client() {
        // Requires a running PostgreSQL with oauth_clients table
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_get_client() {
        // Requires a running PostgreSQL
    }

    #[tokio::test]
    #[ignore = "requires PostgreSQL database"]
    async fn test_delete_client() {
        // Requires a running PostgreSQL
    }
}
