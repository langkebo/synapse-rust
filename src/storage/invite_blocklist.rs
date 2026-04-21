// Invite Blocking Storage - MSC4380
// Allows room admins to control who can be invited to a room
// Following project field naming standards

use sqlx::PgPool;
use std::sync::Arc;

#[derive(Clone)]
pub struct InviteBlocklistStorage {
    pool: Arc<PgPool>,
}

impl InviteBlocklistStorage {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self { pool }
    }

    /// Set the invite blocklist for a room (users that cannot be invited)
    pub async fn set_invite_blocklist(
        &self,
        room_id: &str,
        user_ids: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        // Clear existing blocklist
        sqlx::query("DELETE FROM room_invite_blocklist WHERE room_id = $1")
            .bind(room_id)
            .execute(&*self.pool)
            .await?;

        // Insert new blocklist
        for user_id in &user_ids {
            sqlx::query(
                r#"
                INSERT INTO room_invite_blocklist (room_id, user_id, created_ts)
                VALUES ($1, $2, $3)
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(room_id)
            .bind(user_id)
            .bind(now)
            .execute(&*self.pool)
            .await?;
        }

        Ok(())
    }

    /// Get the invite blocklist for a room
    pub async fn get_invite_blocklist(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT user_id FROM room_invite_blocklist WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    /// Check if a user is blocked from being invited
    pub async fn is_user_blocked(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT user_id FROM room_invite_blocklist 
            WHERE room_id = $1 AND user_id = $2
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.is_some())
    }

    /// Set the invite allowlist for a room (only these users can be invited)
    pub async fn set_invite_allowlist(
        &self,
        room_id: &str,
        user_ids: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        // Clear existing allowlist
        sqlx::query("DELETE FROM room_invite_allowlist WHERE room_id = $1")
            .bind(room_id)
            .execute(&*self.pool)
            .await?;

        // Insert new allowlist
        for user_id in &user_ids {
            sqlx::query(
                r#"
                INSERT INTO room_invite_allowlist (room_id, user_id, created_ts)
                VALUES ($1, $2, $3)
                ON CONFLICT DO NOTHING
                "#,
            )
            .bind(room_id)
            .bind(user_id)
            .bind(now)
            .execute(&*self.pool)
            .await?;
        }

        Ok(())
    }

    /// Get the invite allowlist for a room
    pub async fn get_invite_allowlist(&self, room_id: &str) -> Result<Vec<String>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT user_id FROM room_invite_allowlist WHERE room_id = $1
            "#,
        )
        .bind(room_id)
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows.into_iter().map(|r| r.0).collect())
    }

    /// Check if a user is allowed to be invited (when allowlist is set)
    pub async fn is_user_allowed(&self, room_id: &str, user_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query_as::<_, (String,)>(
            r#"
            SELECT user_id FROM room_invite_allowlist 
            WHERE room_id = $1 AND user_id = $2
            "#,
        )
        .bind(room_id)
        .bind(user_id)
        .fetch_optional(&*self.pool)
        .await?;

        Ok(result.is_some())
    }

    /// Check if invite blocking is enabled for a room
    pub async fn has_any_invite_restriction(&self, room_id: &str) -> Result<bool, sqlx::Error> {
        let blocklist =
            sqlx::query("SELECT 1 FROM room_invite_blocklist WHERE room_id = $1 LIMIT 1")
                .bind(room_id)
                .fetch_optional(&*self.pool)
                .await?;

        if blocklist.is_some() {
            return Ok(true);
        }

        let allowlist =
            sqlx::query("SELECT 1 FROM room_invite_allowlist WHERE room_id = $1 LIMIT 1")
                .bind(room_id)
                .fetch_optional(&*self.pool)
                .await?;

        Ok(allowlist.is_some())
    }

    /// Get global invite blocklist (all rooms)
    pub async fn get_global_invite_blocklist(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, i64)>(
            r#"
            SELECT room_id, user_id, created_ts FROM room_invite_blocklist
            ORDER BY created_ts DESC
            "#,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(room_id, user_id, created_ts)| {
                serde_json::json!({
                    "room_id": room_id,
                    "user_id": user_id,
                    "created_ts": created_ts
                })
            })
            .collect())
    }

    /// Get global invite allowlist (all rooms)
    pub async fn get_global_invite_allowlist(&self) -> Result<Vec<serde_json::Value>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, String, i64)>(
            r#"
            SELECT room_id, user_id, created_ts FROM room_invite_allowlist
            ORDER BY created_ts DESC
            "#,
        )
        .fetch_all(&*self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(room_id, user_id, created_ts)| {
                serde_json::json!({
                    "room_id": room_id,
                    "user_id": user_id,
                    "created_ts": created_ts
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_user_id_format() {
        let valid_users = vec!["@user:localhost", "@alice:example.com"];

        for user in valid_users {
            assert!(user.starts_with('@'), "User ID should start with @");
            assert!(user.contains(':'), "User ID should contain : separator");
        }
    }

    #[test]
    fn test_room_id_format() {
        let valid_rooms = vec!["!room:localhost", "!abc123:matrix.org"];

        for room in valid_rooms {
            assert!(room.starts_with('!'), "Room ID should start with !");
            assert!(room.contains(':'), "Room ID should contain : separator");
        }
    }
}
