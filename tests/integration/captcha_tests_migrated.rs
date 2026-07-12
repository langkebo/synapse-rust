#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use async_trait::async_trait;
use deadpool_redis::{Config as RedisPoolConfig, Runtime as RedisRuntime};
use redis::AsyncCommands;
use sqlx::Row;
use std::sync::Arc;
use synapse_common::error::ApiError;
use synapse_common::task_queue::RedisTaskQueue;
use synapse_rust::test_utils::TEST_ENV_LOCK;
use synapse_services::captcha_service::{CaptchaService, SendCaptchaRequest, VerifyCaptchaRequest};
use synapse_services::sms_provider::SmsProvider;
use synapse_storage::captcha::{CaptchaStorage, CreateCaptchaRequest, CreateSendLogRequest};
use tokio::sync::Mutex;

#[derive(Clone)]
struct RecordingSmsProvider {
    deliveries: Arc<Mutex<Vec<(String, String)>>>,
}

impl RecordingSmsProvider {
    fn new() -> Self {
        Self { deliveries: Arc::new(Mutex::new(Vec::new())) }
    }
}

#[async_trait]
impl SmsProvider for RecordingSmsProvider {
    async fn send(&self, to: &str, content: &str) -> Result<(), ApiError> {
        self.deliveries.lock().await.push((to.to_string(), content.to_string()));
        Ok(())
    }

    fn provider_name(&self) -> &'static str {
        "recording-sms"
    }
}

async fn ensure_default_template(
    pool: &Arc<sqlx::PgPool>,
    template_name: &str,
    captcha_type: &str,
    subject: Option<&str>,
    content: &str,
) {
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r"
        INSERT INTO captcha_template (
            template_name, captcha_type, subject, content, variables, is_default, is_enabled, created_ts, updated_ts
        )
        VALUES ($1, $2, $3, $4, '{}'::jsonb, TRUE, TRUE, $5, $5)
        ON CONFLICT (template_name) DO UPDATE
        SET captcha_type = EXCLUDED.captcha_type,
            subject = EXCLUDED.subject,
            content = EXCLUDED.content,
            is_default = TRUE,
            is_enabled = TRUE,
            updated_ts = EXCLUDED.updated_ts
        ",
    )
    .bind(template_name)
    .bind(captcha_type)
    .bind(subject)
    .bind(content)
    .bind(now)
    .execute(&**pool)
    .await
    .expect("default captcha template should be available");
}

#[tokio::test]
async fn test_captcha_storage_create() {
    // 串行执行测试，避免数据库连接池耗尽
    let _guard = TEST_ENV_LOCK.lock().await;
    let pool = crate::require_test_pool().await;

    let storage = CaptchaStorage::new(&pool);

    let captcha = storage
        .create_captcha(CreateCaptchaRequest {
            captcha_type: "email".to_string(),
            target: "test@example.com".to_string(),
            code: "123456".to_string(),
            expires_in_seconds: 600,
            ip_address: Some("127.0.0.1".to_string()),
            user_agent: Some("test".to_string()),
            max_attempts: 5,
            metadata: None,
        })
        .await;

    assert!(captcha.is_ok());
    let captcha = captcha.unwrap();
    assert_eq!(captcha.captcha_type, "email");
    assert_eq!(captcha.target, "test@example.com");
    assert_eq!(captcha.code, "123456");
    assert_eq!(captcha.status, "pending");
}

#[tokio::test]
async fn test_captcha_storage_get() {
    let pool = crate::require_test_pool().await;

    let storage = CaptchaStorage::new(&pool);

    let created = storage
        .create_captcha(CreateCaptchaRequest {
            captcha_type: "email".to_string(),
            target: "test2@example.com".to_string(),
            code: "654321".to_string(),
            expires_in_seconds: 600,
            ip_address: None,
            user_agent: None,
            max_attempts: 5,
            metadata: None,
        })
        .await
        .unwrap();

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
    let pool = crate::require_test_pool().await;

    let storage = CaptchaStorage::new(&pool);

    let created = storage
        .create_captcha(CreateCaptchaRequest {
            captcha_type: "email".to_string(),
            target: "test3@example.com".to_string(),
            code: "111111".to_string(),
            expires_in_seconds: 600,
            ip_address: None,
            user_agent: None,
            max_attempts: 5,
            metadata: None,
        })
        .await
        .unwrap();

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
    let pool = crate::require_test_pool().await;

    let storage = CaptchaStorage::new(&pool);

    let result = storage.check_rate_limit("rate_limit_test@example.com", "email", 100).await;
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[tokio::test]
async fn test_captcha_storage_send_log() {
    let pool = crate::require_test_pool().await;

    let storage = CaptchaStorage::new(&pool);

    let log = storage
        .create_send_log(CreateSendLogRequest {
            captcha_id: None,
            captcha_type: "email".to_string(),
            target: "log_test@example.com".to_string(),
            ip_address: Some("192.168.1.1".to_string()),
            user_agent: Some("test-agent".to_string()),
            is_success: true,
            error_message: None,
            provider: Some("smtp".to_string()),
            provider_response: None,
        })
        .await;

    assert!(log.is_ok());
    let log = log.unwrap();
    assert_eq!(log.captcha_type, "email");
    assert_eq!(log.target, "log_test@example.com");
    assert!(log.is_success.unwrap_or(false));
}

#[tokio::test]
async fn test_captcha_storage_template() {
    let pool = crate::require_test_pool().await;

    // 初始化默认模板
    ensure_default_template(
        &pool,
        "default_email",
        "email",
        Some("Your verification code"),
        "Your verification code is {{code}} and expires in {{expiry_minutes}} minutes.",
    )
    .await;

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
    let pool = crate::require_test_pool().await;

    // 初始化测试需要的配置
    let now = chrono::Utc::now().timestamp_millis();
    sqlx::query(
        r"
        INSERT INTO captcha_config (
            config_key, config_value, created_ts, updated_ts
        )
        VALUES ($1, $2, $3, $3)
        ON CONFLICT (config_key) DO UPDATE
        SET config_value = EXCLUDED.config_value,
            updated_ts = EXCLUDED.updated_ts
        ",
    )
    .bind("email.code_length")
    .bind("6")
    .bind(now)
    .execute(&*pool)
    .await
    .expect("test captcha config should be initialized");

    sqlx::query(
        r"
        INSERT INTO captcha_config (
            config_key, config_value, created_ts, updated_ts
        )
        VALUES ($1, $2, $3, $3)
        ON CONFLICT (config_key) DO UPDATE
        SET config_value = EXCLUDED.config_value,
            updated_ts = EXCLUDED.updated_ts
        ",
    )
    .bind("email.code_expiry_minutes")
    .bind("10")
    .bind(now)
    .execute(&*pool)
    .await
    .expect("test captcha config should be initialized");

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
    let pool = crate::require_test_pool().await;

    ensure_default_template(
        &pool,
        "default_image",
        "image",
        None,
        "Your verification code is {{code}} and expires in {{expiry_minutes}} minutes.",
    )
    .await;

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
    let pool = crate::require_test_pool().await;

    ensure_default_template(
        &pool,
        "default_image",
        "image",
        None,
        "Your verification code is {{code}} and expires in {{expiry_minutes}} minutes.",
    )
    .await;

    let storage = Arc::new(CaptchaStorage::new(&pool));
    let service = CaptchaService::new(storage.clone());

    let send_request = SendCaptchaRequest {
        captcha_type: "image".to_string(),
        target: "verify_test@example.com".to_string(),
        template_name: None,
    };

    let sent = service.send_captcha(send_request, None, None).await.unwrap();

    let captcha = storage.get_captcha(&sent.captcha_id).await.unwrap().unwrap();

    let verify_request = VerifyCaptchaRequest { captcha_id: sent.captcha_id.clone(), code: captcha.code };

    let result = service.verify_captcha(verify_request).await;
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[tokio::test]
async fn test_captcha_service_invalid_type() {
    let pool = crate::require_test_pool().await;

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

#[tokio::test]
async fn test_captcha_service_email_requires_configured_provider() {
    let pool = crate::require_test_pool().await;

    // 初始化默认邮件模板
    ensure_default_template(
        &pool,
        "default_email",
        "email",
        Some("Your verification code"),
        "Your verification code is {{code}} and expires in {{expiry_minutes}} minutes.",
    )
    .await;

    let storage = Arc::new(CaptchaStorage::new(&pool));
    let service = CaptchaService::new(storage.clone());
    let request = SendCaptchaRequest {
        captcha_type: "email".to_string(),
        target: "provider_missing@example.com".to_string(),
        template_name: None,
    };

    let result = service.send_captcha(request, Some("127.0.0.1"), None).await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    // 打印错误详情，便于调试断言失败问题
    println!("captcha service error: kind={:?}, message={}", err.kind, err);
    assert!(err.is_not_implemented(), "error should be not_implemented, got {:?}", err.kind);
    assert!(err.to_string().contains("Captcha email delivery"));

    let send_log = sqlx::query(
        r"
        SELECT is_success, error_message
        FROM captcha_send_log
        WHERE target = $1 AND captcha_type = 'email'
        ORDER BY sent_ts DESC
        LIMIT 1
        ",
    )
    .bind("provider_missing@example.com")
    .fetch_one(&*pool)
    .await
    .unwrap();

    assert!(!send_log.get::<Option<bool>, _>("is_success").unwrap_or(true));
    assert!(send_log.get::<Option<String>, _>("error_message").unwrap_or_default().contains("Captcha email delivery"));
}

#[tokio::test]
async fn test_captcha_service_sms_provider_successfully_delivers_and_logs_provider() {
    let pool = crate::require_test_pool().await;

    let storage = Arc::new(CaptchaStorage::new(&pool));
    ensure_default_template(
        &pool,
        "default_sms_runtime_test",
        "sms",
        None,
        "您的验证码是 {{code}}，有效期 {{expiry_minutes}} 分钟。",
    )
    .await;

    let provider = Arc::new(RecordingSmsProvider::new());
    let service = CaptchaService::with_sms_provider(storage.clone(), None, false, Some(provider.clone()));
    let request = SendCaptchaRequest {
        captcha_type: "sms".to_string(),
        target: "+8613800138000".to_string(),
        template_name: None,
    };

    let response = service
        .send_captcha(request, Some("127.0.0.1"), Some("captcha-tests"))
        .await
        .expect("configured sms provider should deliver captcha");

    let captcha = storage
        .get_captcha(&response.captcha_id)
        .await
        .expect("captcha lookup should succeed")
        .expect("captcha should be persisted");

    let deliveries = provider.deliveries.lock().await;
    assert_eq!(deliveries.len(), 1, "sms provider should be invoked exactly once");
    assert_eq!(deliveries[0].0, "+8613800138000");
    assert!(deliveries[0].1.contains(&captcha.code), "rendered sms body should include the generated captcha code");

    let send_log = sqlx::query(
        r"
        SELECT is_success, error_message, provider
        FROM captcha_send_log
        WHERE captcha_id = $1
        ORDER BY sent_ts DESC
        LIMIT 1
        ",
    )
    .bind(&response.captcha_id)
    .fetch_one(&*pool)
    .await
    .unwrap();

    assert!(send_log.get::<Option<bool>, _>("is_success").unwrap_or(false));
    assert!(send_log.get::<Option<String>, _>("error_message").unwrap_or_default().is_empty());
    assert_eq!(send_log.get::<Option<String>, _>("provider").as_deref(), Some("recording-sms"));
}

#[tokio::test]
async fn test_captcha_service_email_enqueues_background_job_when_delivery_is_configured() {
    let pool = crate::require_test_pool().await;
    ensure_default_template(
        &pool,
        "default_email_runtime_test",
        "email",
        Some("Your verification code"),
        "Your verification code is {{code}} and expires in {{expiry_minutes}} minutes.",
    )
    .await;

    let redis_pool = RedisPoolConfig::from_url(synapse_rust::test_config::test_redis_url())
        .create_pool(Some(RedisRuntime::Tokio1))
        .expect("test redis pool should be created");
    let mut redis_conn = redis_pool.get().await.expect("test redis connection should be available");
    let queue_len_before: u64 = redis_conn.xlen("mq:tasks:default").await.expect("queue length should be readable");

    let storage = Arc::new(CaptchaStorage::new(&pool));
    let queue = Arc::new(RedisTaskQueue::from_pool(redis_pool.clone()));
    let service = CaptchaService::with_delivery(storage.clone(), Some(queue), true);
    let target = format!("worker_success_{}@example.com", chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default());
    let request = SendCaptchaRequest { captcha_type: "email".to_string(), target: target.clone(), template_name: None };

    let response = service
        .send_captcha(request, Some("127.0.0.1"), Some("captcha-tests"))
        .await
        .expect("configured email delivery should enqueue a background job");

    let queue_len_after: u64 =
        redis_conn.xlen("mq:tasks:default").await.expect("queue length should still be readable");
    assert!(queue_len_after > queue_len_before, "email captcha should enqueue at least one background job");

    let send_log = sqlx::query(
        r"
        SELECT is_success, error_message, provider
        FROM captcha_send_log
        WHERE captcha_id = $1
        ORDER BY sent_ts DESC
        LIMIT 1
        ",
    )
    .bind(&response.captcha_id)
    .fetch_one(&*pool)
    .await
    .unwrap();

    assert!(send_log.get::<Option<bool>, _>("is_success").unwrap_or(false));
    assert!(send_log.get::<Option<String>, _>("error_message").unwrap_or_default().is_empty());
    assert_eq!(send_log.get::<Option<String>, _>("provider").as_deref(), Some("smtp"));
}
