use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct MediaMetadata {
    pub media_id: String,
    pub server_name: String,
    pub content_type: String,
    pub file_name: Option<String>,
    pub size: i64,
    pub uploader_user_id: Option<String>,
    pub created_ts: i64,
    pub last_accessed_at: Option<i64>,
    pub quarantine_status: Option<String>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct Thumbnail {
    pub id: i64,
    pub media_id: String,
    pub width: i32,
    pub height: i32,
    pub method: String,
    pub content_type: String,
    pub size: i64,
    pub created_ts: i64,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct MediaQuota {
    pub id: i64,
    pub user_id: String,
    pub max_bytes: i64,
    pub used_bytes: i64,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_media_metadata() {
        let media = MediaMetadata {
            media_id: "media_abc123".to_string(),
            server_name: "example.com".to_string(),
            content_type: "image/png".to_string(),
            file_name: Some("avatar.png".to_string()),
            size: 102400,
            uploader_user_id: Some("@alice:example.com".to_string()),
            created_ts: 1234567890000,
            last_accessed_at: Some(1234567900000),
            quarantine_status: None,
        };

        assert_eq!(media.media_id, "media_abc123");
        assert_eq!(media.content_type, "image/png");
    }

    #[test]
    fn test_thumbnail() {
        let thumbnail = Thumbnail {
            id: 1,
            media_id: "media_abc123".to_string(),
            width: 100,
            height: 100,
            method: "crop".to_string(),
            content_type: "image/jpeg".to_string(),
            size: 10240,
            created_ts: 1234567890000,
        };

        assert_eq!(thumbnail.width, 100);
        assert_eq!(thumbnail.method, "crop");
    }

    #[test]
    fn test_media_quota() {
        let quota = MediaQuota {
            id: 1,
            user_id: "@alice:example.com".to_string(),
            max_bytes: 1073741824,
            used_bytes: 52428800,
            created_ts: 1234567890000,
            updated_ts: Some(1234567900000),
        };

        assert_eq!(quota.max_bytes, 1073741824);
        assert_eq!(quota.used_bytes, 52428800);
    }
}
