#[cfg(test)]
mod voice_service_tests {
    use sqlx::{Pool, Postgres};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    use synapse_rust::cache::{CacheConfig, CacheManager};
    use synapse_rust::services::voice_service::{
        VoiceMessageUploadParams, VoiceService, VoiceStorage,
    };

    async fn setup_test_database() -> Option<Pool<Postgres>> {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgresql://synapse:synapse@localhost:5432/synapse_test".to_string()
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

        sqlx::query("DROP TABLE IF EXISTS voice_messages CASCADE")
            .execute(&pool)
            .await
            .ok();
        sqlx::query("DROP TABLE IF EXISTS voice_usage_stats CASCADE")
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
            let voice_path = "/tmp/test_voice_service";
            let voice_service = VoiceService::new(&Arc::new(pool.clone()), cache, voice_path);

            let params = VoiceMessageUploadParams {
                user_id: "@alice:localhost".to_string(),
                room_id: Some("!room:localhost".to_string()),
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

            // Verify file exists
            let message_id = val["message_id"].as_str().unwrap();
            let file_path = PathBuf::from(voice_path).join(format!("{}.ogg", message_id));
            assert!(file_path.exists());

            // Cleanup
            std::fs::remove_dir_all(voice_path).ok();
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
            let voice_path = "/tmp/test_voice_stats";
            let voice_service = VoiceService::new(&Arc::new(pool.clone()), cache, voice_path);

            let params = VoiceMessageUploadParams {
                user_id: "@alice:localhost".to_string(),
                room_id: None,
                session_id: None,
                content: vec![0; 1024],
                content_type: "audio/ogg".to_string(),
                duration_ms: 10000,
            };

            voice_service.save_voice_message(params).await.unwrap();

            let stats = voice_service
                .get_user_stats("@alice:localhost", None, None)
                .await
                .unwrap();
            assert_eq!(stats["total_duration_ms"], 10000);
            assert_eq!(stats["total_file_size"], 1024);
            assert_eq!(stats["total_message_count"], 1);

            // Cleanup
            std::fs::remove_dir_all(voice_path).ok();
        });
    }
}
