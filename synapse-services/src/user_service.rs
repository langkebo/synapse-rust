use std::sync::Arc;

use synapse_common::ApiError;
use synapse_storage::user::{User, UserDirectorySearchResult, UserSearchResult, UserStore};
use tracing::instrument;

/// Convenience layer over `UserStore` that maps `sqlx::Error` → `ApiError`
/// and bundles common multi-step patterns duplicated across 15+ services.
///
/// The storage seam (`UserStore` trait) is unchanged; `UserService` is a pure
/// wrapper that eliminates boilerplate.
pub struct UserService {
    user_storage: Arc<dyn UserStore>,
}

impl UserService {
    pub fn new(user_storage: Arc<dyn UserStore>) -> Self {
        Self { user_storage }
    }

    /// Maps a `sqlx::Error` to `ApiError::internal_with_log`.
    fn db_error(e: sqlx::Error) -> ApiError {
        ApiError::internal_with_log("Database error", &e)
    }

    // ── user lookup (Patterns 1+2) ──────────────────────────────────────

    #[instrument(skip(self))]
    pub async fn get_user(&self, user_id: &str) -> Result<Option<User>, ApiError> {
        self.user_storage.get_user_by_id(user_id).await.map_err(Self::db_error)
    }

    #[instrument(skip(self))]
    pub async fn get_user_by_identifier(&self, identifier: &str) -> Result<Option<User>, ApiError> {
        self.user_storage.get_user_by_identifier(identifier).await.map_err(Self::db_error)
    }

    #[instrument(skip(self))]
    pub async fn get_user_by_username(&self, username: &str) -> Result<Option<User>, ApiError> {
        self.user_storage.get_user_by_username(username).await.map_err(Self::db_error)
    }

    #[instrument(skip(self))]
    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>, ApiError> {
        self.user_storage.get_user_by_email(email).await.map_err(Self::db_error)
    }

    #[instrument(skip(self))]
    pub async fn user_exists(&self, user_id: &str) -> Result<bool, ApiError> {
        self.user_storage.user_exists(user_id).await.map_err(Self::db_error)
    }

    /// Returns `Ok(user)` or `Err(ApiError::not_found)`.
    pub async fn get_user_or_not_found(&self, identifier: &str) -> Result<User, ApiError> {
        self.get_user_by_identifier(identifier).await?.ok_or_else(|| ApiError::not_found("User not found".to_string()))
    }

    /// Returns `Ok(())` if the user exists, otherwise `Err(ApiError::not_found)`.
    pub async fn ensure_user_exists(&self, user_id: &str) -> Result<(), ApiError> {
        if !self.user_exists(user_id).await? {
            return Err(ApiError::not_found("User not found".to_string()));
        }
        Ok(())
    }

    // ── profile (Patterns 3+4+5) ───────────────────────────────────────

    #[instrument(skip(self))]
    pub async fn get_profile(&self, user_id: &str) -> Result<serde_json::Value, ApiError> {
        let user = self.get_user(user_id).await?.ok_or_else(|| ApiError::not_found("User not found".to_string()))?;
        Ok(serde_json::json!({
            "user_id": user.user_id,
            "displayname": user.displayname,
            "avatar_url": user.avatar_url
        }))
    }

    #[instrument(skip(self))]
    pub async fn get_profiles_batch(&self, user_ids: &[String]) -> Result<Vec<serde_json::Value>, ApiError> {
        let profiles = self.user_storage.get_user_profiles_batch(user_ids).await.map_err(Self::db_error)?;
        Ok(profiles
            .into_iter()
            .map(
                |u| serde_json::json!({"user_id": u.user_id, "displayname": u.displayname, "avatar_url": u.avatar_url}),
            )
            .collect())
    }

    #[instrument(skip(self))]
    pub async fn update_displayname(&self, user_id: &str, displayname: Option<&str>) -> Result<(), ApiError> {
        self.user_storage.update_displayname(user_id, displayname).await.map_err(|e| {
            if e.to_string().contains("too long") {
                ApiError::bad_request("Displayname too long (max 255 characters)".to_string())
            } else {
                Self::db_error(e)
            }
        })
    }

    #[instrument(skip(self))]
    pub async fn update_avatar_url(&self, user_id: &str, avatar_url: Option<&str>) -> Result<(), ApiError> {
        self.user_storage.update_avatar_url(user_id, avatar_url).await.map_err(|e| {
            if e.to_string().contains("too long") {
                ApiError::bad_request("Avatar URL too long (max 255 characters)".to_string())
            } else {
                Self::db_error(e)
            }
        })
    }

    #[instrument(skip(self))]
    pub async fn update_profile(
        &self,
        user_id: &str,
        displayname: Option<&str>,
        avatar_url: Option<&str>,
    ) -> Result<(), ApiError> {
        if let Some(name) = displayname {
            self.update_displayname(user_id, Some(name)).await?;
        }
        if let Some(url) = avatar_url {
            self.update_avatar_url(user_id, Some(url)).await?;
        }
        Ok(())
    }

    // ── search / listing ───────────────────────────────────────────────

    #[instrument(skip(self))]
    pub async fn search_users(&self, query: &str, limit: i64) -> Result<Vec<UserSearchResult>, ApiError> {
        self.user_storage.search_users(query, limit).await.map_err(Self::db_error)
    }

    #[instrument(skip(self))]
    pub async fn search_directory_users(
        &self,
        query: &str,
        limit: i64,
        exact_only: bool,
    ) -> Result<Vec<UserDirectorySearchResult>, ApiError> {
        self.user_storage.search_directory_users(query, limit, exact_only).await.map_err(Self::db_error)
    }

    #[instrument(skip(self))]
    pub async fn get_users_paginated(
        &self,
        limit: i64,
        since_ts: Option<i64>,
        since_user_id: Option<&str>,
    ) -> Result<Vec<User>, ApiError> {
        self.user_storage.get_users_paginated(limit, since_ts, since_user_id).await.map_err(Self::db_error)
    }

    #[instrument(skip(self))]
    pub async fn get_user_count(&self) -> Result<i64, ApiError> {
        self.user_storage.get_user_count().await.map_err(Self::db_error)
    }

    // ── delegated access to raw store for non-convenience operations ───

    /// Access the underlying `UserStore` for operations not covered by convenience methods.
    pub fn store(&self) -> &Arc<dyn UserStore> {
        &self.user_storage
    }
}
