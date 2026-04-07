#![cfg(test)]

use sqlx::PgPool;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::runtime::Runtime;

use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::voice_service::{VoiceMessageUploadParams, VoiceService, VoiceStorage};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database() -> Option<PgPool> {
    let pool = match synapse_rust::test_utils::prepare_empty_isolated_test_pool().await {
        Ok(pool) => pool.as_ref().clone(),
        Err(error) => {
            eprintln!(
                "Skipping voice service tests because test database is unavailable: {}",
                error
            );
            return None;
        }
    };

    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let pool = Arc::new(pool);
    let storage = VoiceStorage::new(&pool, cache);
    storage
        .create_tables()
        .await
        .expect("Failed to create voice tables");

    Some((*pool).clone())
}

#[test]
fn test_save_voice_message_success() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let id = unique_id();
        let voice_path = format!("/tmp/test_voice_service_{}", id);
        let voice_service = VoiceService::new(&pool, cache, &voice_path);

        let params = VoiceMessageUploadParams {
            user_id: format!("@alice_{}:localhost", id),
            room_id: Some(format!("!room_{}:localhost", id)),
            session_id: None,
            content: vec![0, 1, 2, 3],
            content_type: "audio/ogg".to_string(),
            duration_ms: 5000,
        };

        let result = voice_service.save_voice_message(params).await;
        assert!(result.is_ok());
        let val = result.unwrap();
        let event_id = val["event_id"]
            .as_str()
            .expect("voice upload should return event_id");
        assert!(event_id.starts_with('$'));
        assert_eq!(val["size"], 4);

        let file_prefix = event_id.trim_start_matches('$');
        let file_path = std::path::PathBuf::from(&voice_path).join(format!("{}.ogg", file_prefix));
        assert!(file_path.exists());

        std::fs::remove_dir_all(&voice_path).ok();
    });
}

#[test]
fn test_get_voice_stats() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let id = unique_id();
        let voice_path = format!("/tmp/test_voice_stats_{}", id);
        let voice_service = VoiceService::new(&pool, cache, &voice_path);
        let user_id = format!("@alice_{}:localhost", id);

        let params = VoiceMessageUploadParams {
            user_id: user_id.clone(),
            room_id: None,
            session_id: None,
            content: vec![0; 1024],
            content_type: "audio/ogg".to_string(),
            duration_ms: 10000,
        };

        let before_save_ts = chrono::Utc::now().timestamp();
        voice_service.save_voice_message(params).await.unwrap();
        let after_save_ts = chrono::Utc::now().timestamp();

        let stats = voice_service
            .get_user_stats(&user_id, None, None)
            .await
            .unwrap();
        assert_eq!(stats["total_duration_ms"], 10000);
        assert_eq!(stats["total_file_size"], 1024);
        assert_eq!(stats["total_message_count"], 1);

        let last_active_ts: i64 = sqlx::query_scalar(
            r#"
            SELECT last_active_ts
            FROM voice_usage_stats
            WHERE user_id = $1
            ORDER BY created_ts DESC
            LIMIT 1
            "#,
        )
        .bind(&user_id)
        .fetch_one(pool.as_ref())
        .await
        .expect("voice_usage_stats row should exist");
        assert!(
            (before_save_ts..=after_save_ts).contains(&last_active_ts),
            "expected last_active_ts {last_active_ts} to be within save window {before_save_ts}..={after_save_ts}"
        );

        std::fs::remove_dir_all(&voice_path).ok();
    });
}

#[test]
fn test_voice_usage_stats_schema_uses_last_active_ts() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => Arc::new(pool),
            None => return,
        };

        let columns: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT column_name
            FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = 'voice_usage_stats'
            ORDER BY ordinal_position
            "#,
        )
        .fetch_all(pool.as_ref())
        .await
        .expect("voice_usage_stats columns should be queryable");

        assert!(
            columns.iter().any(|column| column == "last_active_ts"),
            "voice_usage_stats should contain last_active_ts, got columns: {:?}",
            columns
        );
        assert!(
            columns.iter().all(|column| column != "last_activity_ts"),
            "voice_usage_stats should not contain legacy last_activity_ts, got columns: {:?}",
            columns
        );
    });
}
