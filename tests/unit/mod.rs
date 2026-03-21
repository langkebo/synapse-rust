// Basic unit tests module
// Note: Some original test files have compilation issues that need to be fixed separately

mod admin_api_tests;
mod admin_extra_api_tests;
mod app_service_api_tests;
mod background_update_api_tests;
mod captcha_api_tests;
mod core_api_tests;
mod e2ee_api_tests;
mod event_report_api_tests;
mod federation_api_tests;
mod federation_cache_api_tests;
mod friend_api_tests;
mod key_backup_api_tests;
mod media_api_tests;
mod media_quota_api_tests;
mod module_api_tests;
mod msc_tests;
mod push_api_tests;
mod rate_limit_api_tests;
mod reactions_api_tests;
mod refresh_token_api_tests;
mod registration_token_api_tests;
mod retention_api_tests;
mod room_summary_api_tests;
mod server_notification_api_tests;
mod sliding_sync_api_tests;
mod space_api_tests;
mod telemetry_api_tests;
mod thread_api_tests;
mod worker_api_tests;

mod directory_service_tests;
mod dm_service_tests;
mod typing_service_tests;

#[cfg(test)]
mod coverage_tests;
