#![cfg(test)]

use sqlx::{Pool, Postgres};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::runtime::Runtime;

use synapse_rust::cache::{CacheConfig, CacheManager};
use synapse_rust::services::voice_service::{
    VoiceMessageUploadParams, VoiceService, VoiceStorage,
};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(1);

fn unique_id() -> u64 {
    TEST_COUNTER.fetch_add(1, Ordering::SeqCst)
}

async fn setup_test_database() -> Option<Pool<Postgres>> {
    let database_url = std::env::var("TEST_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .unwrap_or_else(|_| {
            "postgresql://synapse:secret@localhost:5432/synapse_test".to_string()
        });

    let pool = match sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            eprintln!(
                "Skipping voice service tests because test database is unavailable: {}",
                error
            );
            return None;
        }
    };

    sqlx::query("DROP TABLE IF EXISTS voice_usage_stats CASCADE")
        .execute(&pool)
        .await
        .ok();
    sqlx::query("DROP TABLE IF EXISTS voice_messages CASCADE")
        .execute(&pool)
        .await
        .ok();

    let cache = Arc::new(CacheManager::new(CacheConfig::default()));
    let storage = VoiceStorage::new(&Arc::new(pool.clone()), cache);
    storage
        .create_tables()
        .await
        .expect("Failed to create voice tables");

    Some(pool)
}

#[test]
fn test_save_voice_message_success() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let id = unique_id();
        let voice_path = format!("/tmp/test_voice_service_{}", id);
        let voice_service = VoiceService::new(&Arc::new(pool.clone()), cache, &voice_path);

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
        assert!(val["message_id"].as_str().unwrap().starts_with("vm_"));
        assert_eq!(val["size"], 4);

        let message_id = val["message_id"].as_str().unwrap();
        let file_path = std::path::PathBuf::from(&voice_path).join(format!("{}.ogg", message_id));
        assert!(file_path.exists());

        std::fs::remove_dir_all(&voice_path).ok();
    });
}

#[test]
fn test_get_voice_stats() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = match setup_test_database().await {
            Some(pool) => pool,
            None => return,
        };
        let cache = Arc::new(CacheManager::new(CacheConfig::default()));
        let id = unique_id();
        let voice_path = format!("/tmp/test_voice_stats_{}", id);
        let voice_service = VoiceService::new(&Arc::new(pool.clone()), cache, &voice_path);
        let user_id = format!("@alice_{}:localhost", id);

        let params = VoiceMessageUploadParams {
            user_id: user_id.clone(),
            room_id: None,
            session_id: None,
            content: vec![0; 1024],
            content_type: "audio/ogg".to_string(),
            duration_ms: 10000,
        };

        voice_service.save_voice_message(params).await.unwrap();

        let stats = voice_service
            .get_user_stats(&user_id, None, None)
            .await
            .unwrap();
        assert_eq!(stats["total_duration_ms"], 10000);
        assert_eq!(stats["total_file_size"], 1024);
        assert_eq!(stats["total_message_count"], 1);

        std::fs::remove_dir_all(&voice_path).ok();
    });
}
