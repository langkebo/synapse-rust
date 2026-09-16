//! 健康检查和根路由处理器

use crate::routes::context::AdminContext;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;
use synapse_common::rate_limit_config::RateLimitConfigManager;

/// Basic liveness + readiness probe used by Docker healthcheck.
///
/// Performs a lightweight database `SELECT 1` so that the container is
/// marked unhealthy when Postgres is unreachable, even if the HTTP server
/// itself is still accepting connections.
///
/// Also reports rate-limit configuration degradation (file vs. built-in
/// defaults) so the state is visible without scraping metrics. Degradation
/// does **not** fail the probe on its own: the server keeps serving a
/// last-good config, and flapping containers would be worse than a clear
/// signal.
pub async fn health_check(State(ctx): State<AdminContext>) -> impl IntoResponse {
    let db_ok = ctx.admin_server_service.is_database_healthy().await;
    let status = if db_ok { "ok" } else { "unhealthy" };
    let http_status = if db_ok { axum::http::StatusCode::OK } else { axum::http::StatusCode::SERVICE_UNAVAILABLE };

    let rate_limit = rate_limit_config_health(ctx.rate_limit_config_manager.as_deref());

    (
        http_status,
        Json(json!({
            "status": status,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "rate_limit_config": rate_limit
        })),
    )
}

/// Builds the `rate_limit_config` health fragment.
///
/// `source` is `"file"` when the operator's `RATE_LIMIT_CONFIG_PATH` is in
/// effect and `"defaults"` when the server fell back to built-in defaults
/// (missing/unparseable file). `degraded` is the single boolean an alert
/// should key on.
fn rate_limit_config_health(manager: Option<&RateLimitConfigManager>) -> serde_json::Value {
    match manager {
        Some(manager) => {
            let d = manager.degradation();
            json!({
                "status": if d.is_degraded() { "degraded" } else { "healthy" },
                "source": d.source.as_str(),
                "degraded": d.is_degraded(),
                "consecutive_reload_failures": d.consecutive_failures,
                "total_reload_failures": d.total_failures,
                "last_error": d.last_error,
                "path": manager.config_path().display().to_string()
            })
        }
        None => json!({
            "status": "absent",
            "source": "defaults",
            "degraded": true,
            "consecutive_reload_failures": 0,
            "total_reload_failures": 0,
            "last_error": serde_json::Value::Null
        }),
    }
}

/// See [`detailed_health_check`].
pub async fn detailed_health_check(State(ctx): State<AdminContext>) -> impl IntoResponse {
    let mut checks = serde_json::Map::new();
    let mut overall_status = "healthy";

    let db_start = std::time::Instant::now();
    let db_ok = ctx.admin_server_service.is_database_healthy().await;
    if db_ok {
        checks.insert(
            "database".to_string(),
            json!({
                "status": "healthy",
                "message": "Connection successful",
                "duration_ms": db_start.elapsed().as_millis()
            }),
        );
    } else {
        overall_status = "unhealthy";
        checks.insert(
            "database".to_string(),
            json!({
                "status": "unhealthy",
                "message": "Connection failed",
                "duration_ms": db_start.elapsed().as_millis()
            }),
        );
    }

    let schema_start = std::time::Instant::now();
    let required_tables = [
        "users",
        "rooms",
        "events",
        "devices",
        "access_tokens",
        "refresh_tokens",
        "federation_signing_keys",
        "room_memberships",
        "widgets",
        "secure_key_backups",
        "media_metadata",
    ];
    let missing_tables = ctx
        .admin_server_service
        .validate_required_tables(&required_tables)
        .await
        .unwrap_or_else(|_| required_tables.iter().map(|table| (*table).to_string()).collect());
    if missing_tables.is_empty() {
        checks.insert(
            "schema".to_string(),
            json!({
                "status": "healthy",
                "message": format!("All {} required tables exist", required_tables.len()),
                "duration_ms": schema_start.elapsed().as_millis()
            }),
        );
    } else {
        overall_status = "degraded";
        checks.insert(
            "schema".to_string(),
            json!({
                "status": "degraded",
                "message": format!("Missing tables: {}", missing_tables.join(", ")),
                "duration_ms": schema_start.elapsed().as_millis()
            }),
        );
    }

    // Redis connectivity probe — only checked when Redis is enabled in config.
    // A degraded Redis does NOT make the server unhealthy (in-memory fallback
    // exists), but it does affect rate-limit consistency in multi-worker setups.
    if ctx.cache.is_redis_enabled() {
        let redis_start = std::time::Instant::now();
        let redis_ok = ctx.cache.get::<String>("__health_probe__").await.is_ok();
        let redis_status =
            if redis_ok && redis_start.elapsed() < std::time::Duration::from_secs(5) { "healthy" } else { "degraded" };
        if redis_status == "degraded" && overall_status == "healthy" {
            overall_status = "degraded";
        }
        checks.insert(
            "redis".to_string(),
            json!({
                "status": redis_status,
                "message": if redis_status == "healthy" {
                    "Round-trip completed".to_string()
                } else {
                    "Round-trip failed or exceeded 5s threshold".to_string()
                },
                "duration_ms": redis_start.elapsed().as_millis()
            }),
        );
    } else {
        checks.insert(
            "redis".to_string(),
            json!({
                "status": "disabled",
                "message": "Redis not enabled — using in-memory cache"
            }),
        );
    }

    // Rate-limit configuration provenance. A server that fell back to built-in
    // defaults is still *serving*, but the operator's declared limits are not in
    // effect — that is a `degraded` signal, not an `unhealthy` one.
    {
        let rl = rate_limit_config_health(ctx.rate_limit_config_manager.as_deref());
        if rl.get("degraded").and_then(|v| v.as_bool()).unwrap_or(false) && overall_status == "healthy" {
            overall_status = "degraded";
        }
        checks.insert("rate_limit_config".to_string(), rl);
    }

    Json(json!({
        "status": overall_status,
        "version": env!("CARGO_PKG_VERSION"),
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "checks": checks
    }))
}

#[cfg(test)]
mod rate_limit_health_tests {
    //! The `/health` and `/_health` responses must expose rate-limit
    //! configuration provenance.
    //!
    //! Regression context: a missing or unparseable `RATE_LIMIT_CONFIG_PATH`
    //! made the server fall back to `RateLimitConfigFile::default()` with only a
    //! single log line. Operators had no machine-readable way to notice that the
    //! limits they declared were being ignored — the same class of silent
    //! degradation as a stale single-file bind mount
    //! (docs/audit/P4_performance_baseline_2026-09-11.md §5.6/§5.7).

    use super::rate_limit_config_health;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use synapse_common::rate_limit_config::{RateLimitConfigFile, RateLimitConfigManager};

    /// Unique temp path (avoids a dev-dependency on `tempfile` from `src/`).
    fn temp_yaml_path() -> PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("synapse_rl_health_{}_{}.yaml", std::process::id(), n))
    }

    fn write_valid_config(path: &std::path::Path) {
        std::fs::write(path, serde_yaml::to_string(&RateLimitConfigFile::default()).expect("serialize"))
            .expect("write temp config");
    }

    #[tokio::test]
    async fn file_backed_manager_reports_healthy_and_file_source() {
        let path = temp_yaml_path();
        write_valid_config(&path);
        let manager = RateLimitConfigManager::from_file(&path).await.expect("load");

        let health = rate_limit_config_health(Some(&manager));
        assert_eq!(health["status"], "healthy");
        assert_eq!(health["source"], "file");
        assert_eq!(health["degraded"], false);
        assert_eq!(health["consecutive_reload_failures"], 0);
        assert!(health["last_error"].is_null());
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn defaults_backed_manager_reports_degraded() {
        let manager = RateLimitConfigManager::new(RateLimitConfigFile::default(), PathBuf::from("/nope.yaml"));

        let health = rate_limit_config_health(Some(&manager));
        assert_eq!(
            health["degraded"], true,
            "回退到内置默认值必须报告 degraded —— 否则运维写入的限流规则被忽略却毫无信号"
        );
        assert_eq!(health["source"], "defaults");
        assert_eq!(health["status"], "degraded");
    }

    #[tokio::test]
    async fn failing_reload_surfaces_as_degraded_with_error() {
        let path = temp_yaml_path();
        write_valid_config(&path);
        let manager = RateLimitConfigManager::from_file(&path).await.expect("load");
        // Healthy before the failure.
        assert_eq!(rate_limit_config_health(Some(&manager))["degraded"], false);

        std::fs::remove_file(&path).expect("remove");
        assert!(manager.reload().await.is_err());

        let health = rate_limit_config_health(Some(&manager));
        assert_eq!(health["degraded"], true, "热加载失败必须报告 degraded");
        assert_eq!(health["consecutive_reload_failures"], 1);
        assert!(health["last_error"].is_string(), "必须暴露最后一次错误信息");
    }

    #[test]
    fn absent_manager_is_reported_as_degraded_not_missing_field() {
        let health = rate_limit_config_health(None);
        assert_eq!(health["degraded"], true);
        assert_eq!(health["status"], "absent");
        assert_eq!(health["source"], "defaults");
    }

    #[tokio::test]
    async fn health_fragment_includes_the_watched_path() {
        let path = temp_yaml_path();
        write_valid_config(&path);
        let manager = RateLimitConfigManager::from_file(&path).await.expect("load");

        let health = rate_limit_config_health(Some(&manager));
        let reported = health["path"].as_str().expect("path must be a string");
        assert_eq!(reported, path.display().to_string(), "健康检查必须报告实际监听的路径");
        let _ = std::fs::remove_file(&path);
    }
}
