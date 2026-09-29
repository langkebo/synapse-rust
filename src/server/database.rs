use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;

use crate::common::config::Config;
use synapse_services::database_initializer::DatabaseInitService;
use synapse_storage::schema_health_check::run_schema_health_check;

/// PG 会话参数**值**的统一形态：`30s`（不带引号）。
///
/// PG 接受 `SET ... = <int>ms` 或 `'<int>{ms|s|min}'`；本项目所有超时都按秒配置，
/// 显式带单位避免歧义（`'0'` 会被解析为毫秒）。这里返回的是**裸值**，因为它不再被拼进
/// SQL 文本，而是作为 `set_config(name, $1, false)` 的绑定参数发送（D14-1）——
/// 带引号的形态（`'30s'`）只在文本拼接时代才需要。
fn pg_timeout_value(seconds: u64) -> String {
    format!("{seconds}s")
}

/// Apply the three configured session GUCs to a freshly established connection.
///
/// **D14-1**：这三条原先由 `format!("SET <guc> = {}", value)` 拼出 SQL 文本，现在改为
/// `set_config(name, $1, false)` —— `is_local = false` 即 **session 级**，与
/// `SET <guc> = '30s'` 语义等价，但**值**改走绑定参数：配置值不再进入 SQL 文本
/// （标识符/表名无法参数化，**值**可以，R7 与 §8.6 的分档据此而来）。
///
/// ⚠️ R4③：`set_config` 是没有关系来源的函数调用 ⇒ sqlx 按**可空**推断，而返回的就是刚写入
/// 的值（恒非 NULL）⇒ 断言 `AS "applied!"`。
///
/// 抽成独立函数是为了让 R8④ 的"真 baseline 往返"能直接调用它并读回 GUC ——
/// 否则这段只在 `after_connect` 里执行，任何测试都覆盖不到（见 `mod tests`）。
async fn apply_session_timeouts(
    conn: &mut sqlx::PgConnection,
    statement_timeout: &str,
    lock_timeout: &str,
    idle_in_tx_timeout: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query_scalar!(r#"SELECT set_config('statement_timeout', $1, false) AS "applied!""#, statement_timeout)
        .fetch_one(&mut *conn)
        .await?;
    sqlx::query_scalar!(r#"SELECT set_config('lock_timeout', $1, false) AS "applied!""#, lock_timeout)
        .fetch_one(&mut *conn)
        .await?;
    sqlx::query_scalar!(
        r#"SELECT set_config('idle_in_transaction_session_timeout', $1, false) AS "applied!""#,
        idle_in_tx_timeout
    )
    .fetch_one(&mut *conn)
    .await?;
    Ok(())
}

/// See [`build_database_pool`].
pub async fn build_database_pool(config: &Config) -> Result<PgPool, Box<dyn std::error::Error>> {
    let db_cfg = &config.database;
    let statement_timeout = pg_timeout_value(db_cfg.statement_timeout_secs);
    let lock_timeout = pg_timeout_value(db_cfg.lock_timeout_secs);
    let idle_in_tx_timeout = pg_timeout_value(db_cfg.idle_in_transaction_timeout_secs);

    let min_idle = db_cfg.min_idle.unwrap_or(db_cfg.min_idle_floor);
    let max_lifetime = Duration::from_secs(db_cfg.max_lifetime_secs);
    let idle_timeout = Duration::from_secs(db_cfg.idle_timeout_secs);

    let pool_options = PgPoolOptions::new()
        .max_connections(db_cfg.max_size)
        .min_connections(min_idle)
        .acquire_timeout(Duration::from_secs(db_cfg.connection_timeout))
        .max_lifetime(max_lifetime)
        .idle_timeout(idle_timeout)
        .after_connect(move |conn, _meta| {
            let statement_timeout = statement_timeout.clone();
            let lock_timeout = lock_timeout.clone();
            let idle_in_tx_timeout = idle_in_tx_timeout.clone();
            Box::pin(async move {
                apply_session_timeouts(conn, &statement_timeout, &lock_timeout, &idle_in_tx_timeout).await
            })
        })
        .test_before_acquire(false);

    ::tracing::info!(
        "[启动阶段 1/4] 连接数据库 (pool: max={}, min_idle={}, timeout={}s, max_lifetime={}s, idle_timeout={}s)",
        db_cfg.max_size,
        min_idle,
        db_cfg.connection_timeout,
        db_cfg.max_lifetime_secs,
        db_cfg.idle_timeout_secs
    );
    ::tracing::info!(
        "[启动阶段 1/4] PG session 超时: statement_timeout={}s, lock_timeout={}s, idle_in_transaction={}s",
        db_cfg.statement_timeout_secs,
        db_cfg.lock_timeout_secs,
        db_cfg.idle_in_transaction_timeout_secs
    );

    let database_url = config.database_url();
    let pool = pool_options.connect(&database_url).await?;

    // 记录 PG 服务端版本, 便于排查兼容性问题 (如 PG14 以下不支持某些 SQL 语法)
    //
    // D-107（C63-0 先修）：原先这里是 `.ok().flatten()` —— `SELECT version()` 失败时**静默**降级成
    // "unknown"，日志里看不出"取版本失败"和"库没报版本"的区别（D-33/D-94 同型吞错）。
    // 这里**不**把错误往上抛：pool 已经建好，因为一条元数据查询失败就让整个启动失败是过度反应；
    // 但必须**记下来** —— 改成显式 match + `warn!`，行为（继续启动、日志显示 unknown）不变。
    let pg_version: Option<String> =
        match sqlx::query_scalar!(r#"SELECT version() AS "version!""#).fetch_optional(&pool).await {
            Ok(version) => version,
            Err(error) => {
                ::tracing::warn!(%error, "查询 PG 服务端版本失败，启动继续（日志里记为 unknown）");
                None
            }
        };
    ::tracing::info!("[启动阶段 1/4] 数据库连接建立: {}", pg_version.as_deref().unwrap_or("unknown"));
    let pool = Arc::new(pool);

    // 先执行运行时数据库初始化，确保所有表存在
    let runtime_db_init_enabled = std::env::var("SYNAPSE_ENABLE_RUNTIME_DB_INIT")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);
    let skip_db_init = std::env::var("SYNAPSE_SKIP_DB_INIT")
        .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
        .unwrap_or(false);
    let migrations_dir = if std::path::Path::new("/app/migrations").exists() {
        "/app/migrations"
    } else if std::path::Path::new("./migrations").exists() {
        "./migrations"
    } else {
        "(none)"
    };
    if !runtime_db_init_enabled || skip_db_init {
        ::tracing::info!(
            "[启动阶段 1/4] 运行时数据库初始化已禁用 (SYNAPSE_ENABLE_RUNTIME_DB_INIT={}, SYNAPSE_SKIP_DB_INIT={}); 迁移主链: docker/db_migrate.sh + db-migration-gate.yml, 迁移目录: {}",
            runtime_db_init_enabled, skip_db_init, migrations_dir
        );
    } else {
        ::tracing::info!("[启动阶段 1/4] 开始运行时数据库初始化 (迁移目录: {})", migrations_dir);
        let db_init_service = DatabaseInitService::new(pool.clone());
        db_init_service.initialize().await?;
        ::tracing::info!("[启动阶段 1/4] 运行时数据库初始化完成");
    }

    // 运行数据库 Schema 健康检查（在运行时初始化之后）
    let skip_schema_check_value = std::env::var(SKIP_SCHEMA_CHECK_ENV).ok();
    let skip_schema_check = schema_check_skip_requested(skip_schema_check_value.as_deref());

    if skip_schema_check {
        ::tracing::error!(
            "[启动阶段 1/4] 🚨 跳过数据库 schema 健康检查 —— 半迁移的 schema 不会在启动时被发现，\
             运行期可能产生脏数据。仅在应急恢复时使用，并在恢复后立即取消该变量。"
        );
    } else {
        if let Some(unrecognised) = skip_schema_check_value.as_deref().filter(|v| !v.trim().is_empty()) {
            // A stale `=true` (the value this bypass used to accept) must not fail
            // silently in the "safe" direction without saying so — otherwise an
            // operator believes the check is still being skipped while it is not.
            ::tracing::warn!(
                "[启动阶段 1/4] {}={} 不是可识别的跳过值，schema 健康检查照常执行。\
                 如需应急跳过，请显式设置 {}={}",
                SKIP_SCHEMA_CHECK_ENV,
                unrecognised,
                SKIP_SCHEMA_CHECK_ENV,
                SKIP_SCHEMA_CHECK_SENTINEL
            );
        }
        ::tracing::info!("[启动阶段 1/4] 开始数据库 schema 健康检查...");
        match run_schema_health_check(&pool, false).await {
            Ok(result) => {
                if result.passed {
                    ::tracing::info!("[启动阶段 1/4] ✅ 数据库 schema 校验通过");
                } else {
                    ::tracing::error!("[启动阶段 1/4] ❌ 数据库 schema 校验失败");
                    if !result.missing_tables.is_empty() {
                        ::tracing::error!("  Missing tables: {:?}", result.missing_tables);
                    }
                    if !result.missing_columns.is_empty() {
                        ::tracing::error!("  Missing columns: {:?}", result.missing_columns);
                    }
                    if !result.repaired_indexes.is_empty() {
                        ::tracing::info!("  Repaired indexes: {:?}", result.repaired_indexes);
                    }
                    // 如果有严重问题（缺少表或列），给出可执行的修复指引后退出
                    if !result.missing_tables.is_empty() || !result.missing_columns.is_empty() {
                        ::tracing::error!(
                            "💡 To fix: run pending migrations against your database, e.g.\n   \
                             DATABASE_URL=\"postgresql://USER:PASS@HOST:PORT/DBNAME\" \\\n   \
                             bash docker/db_migrate.sh migrate\n   \
                             If you understand the risk and want to start anyway, set \
                             {} (NOT recommended for production).",
                            skip_schema_check_hint()
                        );
                        return Err(format!(
                            "Database schema validation failed: missing critical tables or columns. \
                             Run `docker/db_migrate.sh migrate` against the configured database \
                             (or set {} to bypass this check).",
                            skip_schema_check_hint()
                        )
                        .into());
                    }
                }
                if !result.warnings.is_empty() {
                    ::tracing::warn!("Schema warnings (non-critical): {:?}", result.warnings);
                }
            }
            Err(e) => {
                ::tracing::error!("[启动阶段 1/4] 数据库 schema 健康检查执行失败: {}", e);
                // Schema health check itself failed — this is NOT safe to ignore.
                // We might be running against a half-migrated schema.
                return Err(format!(
                    "Database schema health check failed to execute: {e}. \
                     This may indicate a connectivity issue or a corrupt migration state. \
                     Fix the database connection or set {} \
                     to bypass this check (NOT recommended for production).",
                    skip_schema_check_hint()
                )
                .into());
            }
        }
    }

    // Drop the Arc wrapper and return the inner PgPool.
    // The db_init_service has already been dropped, so there should be
    // exactly one reference to the Arc.
    match Arc::try_unwrap(pool) {
        Ok(p) => Ok(p),
        Err(arc) => {
            // Fallback: if for some reason there are still outstanding
            // references, clone the underlying pool (PgPool::clone is cheap).
            Ok((*arc).clone())
        }
    }
}

/// Environment variable that bypasses the startup schema health check.
const SKIP_SCHEMA_CHECK_ENV: &str = "SYNAPSE_SKIP_SCHEMA_CHECK";

/// The only value accepted for [`SKIP_SCHEMA_CHECK_ENV`].
///
/// This used to accept a plain `true`, so the bypass could be switched on by habit,
/// by a copy-pasted troubleshooting snippet, or by a stale `.env` line — and a
/// boolean that reads like an ordinary feature flag never looks like a decision.
/// Requiring a sentence makes it deliberate; the value is checked
/// case-insensitively and compared after trimming.
const SKIP_SCHEMA_CHECK_SENTINEL: &str = "I_UNDERSTAND_SCHEMA_CHECKS_ARE_SKIPPED";

/// Whether the caller explicitly asked to skip the schema health check.
///
/// Fail-closed: anything other than the sentinel — including the old `true`, an
/// empty string, or an unrelated value — means "do not skip", so the schema check
/// runs exactly as if the variable were unset.
fn schema_check_skip_requested(value: Option<&str>) -> bool {
    value.is_some_and(|v| v.trim().eq_ignore_ascii_case(SKIP_SCHEMA_CHECK_SENTINEL))
}

/// Operator-facing instruction for enabling the bypass.
///
/// Derived from [`SKIP_SCHEMA_CHECK_ENV`] and [`SKIP_SCHEMA_CHECK_SENTINEL`] rather
/// than written as a literal. The three startup error messages used to spell out
/// `SYNAPSE_SKIP_SCHEMA_CHECK=true` — which is exactly the value
/// [`schema_check_skip_requested`] now *rejects*, so they advised operators to set a
/// variable that leaves the schema check running. Building the hint from the two
/// constants keeps the message and the check from drifting again;
/// `schema_check_skip_hint_names_an_accepted_value` pins that.
fn skip_schema_check_hint() -> String {
    format!("{SKIP_SCHEMA_CHECK_ENV}={SKIP_SCHEMA_CHECK_SENTINEL}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_check_skip_requires_the_explicit_sentinel() {
        assert!(schema_check_skip_requested(Some(SKIP_SCHEMA_CHECK_SENTINEL)));
        // Whitespace and case are tolerated: the point is deliberateness, not obfuscation.
        assert!(schema_check_skip_requested(Some("  i_understand_schema_checks_are_skipped  ")));
    }

    #[test]
    fn schema_check_skip_is_fail_closed_for_everything_else() {
        // `true` is the value this bypass used to accept; it must now fall through to
        // the real schema check rather than silently disabling it.
        for value in [None, Some(""), Some("   "), Some("true"), Some("TRUE"), Some("1"), Some("yes"), Some("skip")] {
            assert!(!schema_check_skip_requested(value), "{value:?} must NOT skip the schema health check");
        }
    }

    #[test]
    fn schema_check_skip_hint_names_an_accepted_value() {
        let hint = skip_schema_check_hint();
        let (name, value) = hint.split_once('=').expect("hint must be NAME=VALUE");
        assert_eq!(name, SKIP_SCHEMA_CHECK_ENV);
        // If the hint ever goes back to naming `true`, this fails: the startup error
        // would tell an operator to set a value that leaves the schema check running.
        assert!(schema_check_skip_requested(Some(value)), "hint must name a value the bypass accepts: {hint}");
    }

    /// D14-1 的行为证据（R8④：本批改的是"每条新连接建立时执行什么"，只看编译通过不够）。
    ///
    /// `set_config(<guc>, $1, false)` 必须真的写进**会话级** GUC，且值形态与旧文本拼接
    /// （`SET <guc> = '30s'`）等价 —— 这是"值改走绑定参数"唯一的行为风险点。用例直接调用
    /// `after_connect` 用的同一个 [`apply_session_timeouts`]，再从连接上读回 GUC。
    #[tokio::test]
    async fn session_timeouts_are_applied_as_session_level_gucs() {
        let isolated = synapse_common::test_isolation::IsolatedTestPool::new(include_str!(
            "../../migrations/00000000_unified_schema_v12.sql"
        ))
        .await
        .expect("isolated test pool");
        let pool = isolated.pool();
        let mut conn = pool.acquire().await.expect("acquire a pooled connection");

        // 三个值各不相同：若哪一条写错了 GUC 名（例如 idle 与 lock 互换），断言会指出是哪一个。
        apply_session_timeouts(&mut conn, &pg_timeout_value(30), &pg_timeout_value(7), &pg_timeout_value(11))
            .await
            .expect("apply the session timeouts");

        // R9：测试区探针一律动态 SQL（宏的条目不会被 `cargo sqlx prepare` 收集，
        // 离线 `--all-targets` 会 E0282）。一次往返读回三个 GUC，避免同一断言产生三处站点。
        let (statement, lock, idle): (String, String, String) = sqlx::query_as::<_, (String, String, String)>(
            "SELECT current_setting('statement_timeout'), current_setting('lock_timeout'), \
             current_setting('idle_in_transaction_session_timeout')",
        )
        .fetch_one(&mut *conn)
        .await
        .expect("read back the session GUCs");

        assert_eq!(statement, "30s", "statement_timeout 必须按会话级生效");
        assert_eq!(lock, "7s", "lock_timeout 必须按会话级生效");
        assert_eq!(idle, "11s", "idle_in_transaction_session_timeout 必须按会话级生效");
    }
}
