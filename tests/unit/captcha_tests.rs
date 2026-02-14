#[cfg(test)]
mod tests {
    use crate::storage::captcha::{CaptchaStorage, CreateCaptchaRequest, CreateSendLogRequest};
    use crate::services::captcha_service::{CaptchaService, SendCaptchaRequest, VerifyCaptchaRequest};
    use sqlx::PgPool;
    use std::sync::Arc;

    async fn setup_test_db() -> Option<Arc<PgPool>> {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:5432/synapse_test".to_string());
        
        let pool = PgPool::connect(&db_url).await.ok()?;
        Some(Arc::new(pool))
    }

    #[tokio::test]
    async fn test_captcha_storage_create() {
        let pool = match setup_test_db().await {
            Some(p) => p,
            None => return,
        };

        let storage = CaptchaStorage::new(&pool);

        let captcha = storage.create_captcha(CreateCaptchaRequest {
            captcha_type: "email".to_string(),
            target: "test@example.com".to_string(),
            code: "123456".to_string(),
            expires_in_seconds: 600,
            ip_address: Some("127.0.0.1".to_string()),
            user_agent: Some("test".to_string()),
            max_attempts: 5,
            metadata: None,
        }).await;

        assert!(captcha.is_ok());
        let captcha = captcha.unwrap();
        assert_eq!(captcha.captcha_type, "email");
        assert_eq!(captcha.target, "test@example.com");
        assert_eq!(captcha.code, "123456");
        assert_eq!(captcha.status, "pending");
    }

    #[tokio::test]
    async fn test_captcha_storage_get() {
        let pool = match setup_test_db().await {
            Some(p) => p,
            None => return,
        };

        let storage = CaptchaStorage::new(&pool);

        let created = storage.create_captcha(CreateCaptchaRequest {
            captcha_type: "email".to_string(),
            target: "test2@example.com".to_string(),
            code: "654321".to_string(),
            expires_in_seconds: 600,
            ip_address: None,
            user_agent: None,
            max_attempts: 5,
            metadata: None,
        }).await.unwrap();

        let fetched = storage.get_captcha(&created.captcha_id).await;
        assert!(fetched.is_ok());
        let fetched = fetched.unwrap();
        assert!(fetched.is_some());
        let fetched = fetched.unwrap();
        assert_eq!(fetched.captcha_id, created.captcha_id);
        assert_eq!(fetched.code, "654321");
    }

    #[tokio::test]
    async fn test_captcha_storage_verify() {
        let pool = match setup_test_db().await {
            Some(p) => p,
            None => return,
        };

        let storage = CaptchaStorage::new(&pool);

        let created = storage.create_captcha(CreateCaptchaRequest {
            captcha_type: "email".to_string(),
            target: "test3@example.com".to_string(),
            code: "111111".to_string(),
            expires_in_seconds: 600,
            ip_address: None,
            user_agent: None,
            max_attempts: 5,
            metadata: None,
        }).await.unwrap();

        let wrong_result = storage.verify_captcha(&created.captcha_id, "000000").await;
        assert!(wrong_result.is_ok());
        assert!(!wrong_result.unwrap());

        let correct_result = storage.verify_captcha(&created.captcha_id, "111111").await;
        assert!(correct_result.is_ok());
        assert!(correct_result.unwrap());

        let verified = storage.get_captcha(&created.captcha_id).await.unwrap().unwrap();
        assert_eq!(verified.status, "verified");
    }

    #[tokio::test]
    async fn test_captcha_storage_rate_limit() {
        let pool = match setup_test_db().await {
            Some(p) => p,
            None => return,
        };

        let storage = CaptchaStorage::new(&pool);

        let result = storage.check_rate_limit("rate_limit_test@example.com", "email", 100).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_captcha_storage_send_log() {
        let pool = match setup_test_db().await {
            Some(p) => p,
            None => return,
        };

        let storage = CaptchaStorage::new(&pool);

        let log = storage.create_send_log(CreateSendLogRequest {
            captcha_id: None,
            captcha_type: "email".to_string(),
            target: "log_test@example.com".to_string(),
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: Some("test-agent".to_string()),
            success: true,
            error_message: None,
            provider: Some("smtp".to_string()),
            provider_response: None,
        }).await;

        assert!(log.is_ok());
        let log = log.unwrap();
        assert_eq!(log.captcha_type, "email");
        assert_eq!(log.target, "log_test@example.com");
        assert!(log.success);
    }

    #[tokio::test]
    async fn test_captcha_storage_template() {
        let pool = match setup_test_db().await {
            Some(p) => p,
            None => return,
        };

        let storage = CaptchaStorage::new(&pool);

        let template = storage.get_default_template("email").await;
        assert!(template.is_ok());
        let template = template.unwrap();
        assert!(template.is_some());
        let template = template.unwrap();
        assert!(template.content.contains("{{code}}"));
    }

    #[tokio::test]
    async fn test_captcha_storage_config() {
        let pool = match setup_test_db().await {
            Some(p) => p,
            None => return,
        };

        let storage = CaptchaStorage::new(&pool);

        let config = storage.get_config("email.code_length").await;
        assert!(config.is_ok());
        let config = config.unwrap();
        assert!(config.is_some());
        assert_eq!(config.unwrap(), "6");

        let int_config = storage.get_config_as_int("email.code_expiry_minutes", 10).await;
        assert!(int_config.is_ok());
        assert_eq!(int_config.unwrap(), 10);
    }

    #[tokio::test]
    async fn test_captcha_service_generate_code() {
        let pool = match setup_test_db().await {
            Some(p) => p,
            None => return,
        };

        let storage = Arc::new(CaptchaStorage::new(&pool));
        let service = CaptchaService::new(storage);

        let request = SendCaptchaRequest {
            captcha_type: "image".to_string(),
            target: "image_test@example.com".to_string(),
            template_name: None,
        };

        let result = service.send_captcha(request, Some("127.0.0.1"), None).await;
        assert!(result.is_ok());
        let response = result.unwrap();
        assert!(!response.captcha_id.is_empty());
        assert!(response.expires_in > 0);
        assert_eq!(response.captcha_type, "image");
    }

    #[tokio::test]
    async fn test_captcha_service_verify() {
        let pool = match setup_test_db().await {
            Some(p) => p,
            None => return,
        };

        let storage = Arc::new(CaptchaStorage::new(&pool));
        let service = CaptchaService::new(storage.clone());

        let send_request = SendCaptchaRequest {
            captcha_type: "email".to_string(),
            target: "verify_test@example.com".to_string(),
            template_name: None,
        };

        let sent = service.send_captcha(send_request, None, None).await.unwrap();

        let captcha = storage.get_captcha(&sent.captcha_id).await.unwrap().unwrap();

        let verify_request = VerifyCaptchaRequest {
            captcha_id: sent.captcha_id.clone(),
            code: captcha.code,
        };

        let result = service.verify_captcha(verify_request).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_captcha_service_invalid_type() {
        let pool = match setup_test_db().await {
            Some(p) => p,
            None => return,
        };

        let storage = Arc::new(CaptchaStorage::new(&pool));
        let service = CaptchaService::new(storage);

        let request = SendCaptchaRequest {
            captcha_type: "invalid".to_string(),
            target: "invalid@example.com".to_string(),
            template_name: None,
        };

        let result = service.send_captcha(request, None, None).await;
        assert!(result.is_err());
    }
}
