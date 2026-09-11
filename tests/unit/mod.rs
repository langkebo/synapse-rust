// Basic unit tests module
// Note: Some original test files have compilation issues that need to be fixed separately

// 测试代码按 Rust 测试惯例允许 unwrap/expect/panic（与 tests/integration/mod.rs 一致）；
// 生产 lib 代码仍受 [lints.clippy] 的严格约束。
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../common/mod.rs"]
mod common;

mod admin_api_tests;
mod admin_extra_api_tests;
mod api_optimization_verification_tests;
mod api_optimized_features_tests;
mod app_service_api_tests;
mod background_update_api_tests;

mod canonical_json_vectors;

#[cfg(feature = "beacons")]
mod beacon_info_parse_tests;
mod boundary_tests;
mod captcha_api_tests;
mod client_push_service_tests;
mod core_api_tests;
mod e2ee_api_tests;
mod event_report_api_tests;
mod federation_api_tests;
mod federation_cache_api_tests;
mod friend_api_tests;
mod key_backup_api_tests;
mod key_rotation_route_tests;
mod key_rotation_service_tests;
mod ledger_export_tests;
mod media_api_tests;
mod media_quota_api_tests;
mod media_service_tests;
mod megolm_dual_write_metrics_tests;
mod migration_consistency_tests;
mod module_api_tests;
mod msc4108_rendezvous_route_tests;
mod msc_tests;
mod placeholder_scan_tests;
mod push_api_tests;
mod push_notification_route_tests;
mod rate_limit_api_tests;
mod reactions_api_tests;
mod refresh_token_api_tests;
mod registration_token_api_tests;
mod retention_api_tests;

mod room_summary_api_tests;

mod search_service_tests;
mod server_notification_api_tests;
mod sliding_sync_api_tests;
mod space_api_tests;
mod telemetry_api_tests;
mod thread_api_tests;

mod worker_api_tests;

mod directory_service_tests;
mod rendezvous_service_tests;
mod test_connection_budget_tests;
mod test_pagination_limit_clamp_tests;
mod typing_service_tests;
#[cfg(feature = "voice-extended")]
mod voice_service_tests;

#[cfg(feature = "voice-extended")]
mod voice_route_tests;

mod identity_service_tests;
mod sso_cas_tests;
mod sso_oidc_tests;

mod security_critical_tests;
mod security_signature_check_tests;

#[cfg(test)]
mod coverage_tests;

#[cfg(test)]
mod worker_coverage_tests;

mod benchmark_pr_gate_tests;
mod prelude_module_tests;
mod room_domain_refactor_tests;
mod services_remaining_domains_refactor_tests;
mod services_sync_domain_refactor_tests;
mod sliding_sync_perf_gate_tests;
mod storage_admin_domain_refactor_tests;
mod storage_remaining_domains_refactor_tests;

// P-096 route tests (12 files)
mod account_compat_route_tests;
mod assembly_route_tests;
mod auth_compat_route_tests;
mod burn_after_read_route_tests;
mod context_route_tests;
mod ephemeral_route_tests;
mod formatting_route_tests;
mod guest_route_tests;
mod pinned_route_tests;
mod qr_login_token_route_tests;
mod room_access_route_tests;

// P-097: insta snapshot tests for security-sensitive endpoint response shapes
mod security_endpoint_snapshots_tests;

// P-099 service tests (8 files)
mod admin_server_service_tests;
mod container_service_tests;
mod event_broadcaster_tests;
mod event_service_tests;
mod server_notification_service_tests;
mod sync_helpers_tests;
mod user_service_tests;
