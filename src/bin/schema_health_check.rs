//! `schema_health_check` - CI/CD schema health check binary
//!
//! 这是 CI/CD 强制门禁的入口二进制。运行 `schema_health_check::run_schema_health_check`
//! 验证数据库 schema 是否与 Rust 代码期望的核心表/列/索引一致。
//!
//! ## 用法
//! ```bash
//! DATABASE_URL=postgres://user:pass@host:5432/db cargo run --bin schema_health_check
//! ```
//!
//! ## 退出码
//! - 0: schema 健康（所有必需表/列/索引存在）
//! - 1: schema 漂移（缺失必需表/列/索引）
//! - 2: 连接错误（无法连接到数据库）
//!
//! ## CI 集成
//! 在 GitHub Actions / 任何 CI 系统中：
//! ```yaml
//! - name: Schema Health Check
//!   run: |
//!     DATABASE_URL=postgres://... cargo run --bin schema_health_check
//! ```
//!
//! 详细报告模式（输出到文件）：
//! ```bash
//! DATABASE_URL=postgres://... SCHEMA_HEALTH_REPORT_OUT=./schema-report.md \
//!   cargo run --bin schema_health_check -- --detailed
//! ```

use std::process::ExitCode;

use sqlx::postgres::PgPoolOptions;

use synapse_rust::storage::schema_health_check::{
    detailed_report, quick_validate, run_schema_health_check, HealthCheckResult,
};

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let detailed = args.iter().any(|a| a == "--detailed" || a == "-d");
    let auto_repair = args.iter().any(|a| a == "--auto-repair" || a == "-r");

    let database_url = match std::env::var("DATABASE_URL") {
        Ok(url) => url,
        Err(_) => {
            eprintln!("ERROR: DATABASE_URL environment variable is not set");
            return ExitCode::from(2);
        }
    };

    eprintln!("[schema_health_check] Connecting to database...");
    let pool = match PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect(&database_url)
        .await
    {
        Ok(pool) => pool,
        Err(e) => {
            eprintln!("ERROR: Failed to connect to database: {e}");
            return ExitCode::from(2);
        }
    };

    let result = if detailed {
        match detailed_report(&pool).await {
            Ok(report) => {
                println!("{report}");
                if let Some(out_path) = std::env::var_os("SCHEMA_HEALTH_REPORT_OUT") {
                    if let Err(e) = std::fs::write(&out_path, &report) {
                        eprintln!("ERROR: Failed to write report to {out_path:?}: {e}");
                    } else {
                        eprintln!("[schema_health_check] Report written to: {out_path:?}");
                    }
                }
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("ERROR: Failed to generate detailed report: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        match run_schema_health_check(&pool, auto_repair).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("ERROR: Failed to run schema health check: {e}");
                return ExitCode::from(2);
            }
        }
    };

    print_summary(&result);

    if result.passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn print_summary(result: &HealthCheckResult) {
    println!("\n========================================");
    println!("  Schema Health Check Summary");
    println!("========================================");
    println!("Status:           {}", if result.passed { "PASSED" } else { "FAILED" });
    println!("Missing tables:   {}", result.missing_tables.len());
    println!("Missing columns:  {}", result.missing_columns.len());
    println!("Missing indexes:  {}", result.missing_indexes.len());
    println!("Repaired indexes: {}", result.repaired_indexes.len());
    println!("Warnings:         {}", result.warnings.len());
    println!("========================================\n");

    if !result.missing_tables.is_empty() {
        println!("❌ Missing tables:");
        for t in &result.missing_tables {
            println!("   - {t}");
        }
        println!();
    }

    if !result.missing_columns.is_empty() {
        println!("❌ Missing columns:");
        for c in &result.missing_columns {
            println!("   - {c}");
        }
        println!();
    }

    if !result.missing_indexes.is_empty() {
        println!("⚠️  Missing indexes:");
        for i in &result.missing_indexes {
            println!("   - {i}");
        }
        println!();
    }

    if !result.warnings.is_empty() {
        println!("⚠️  Warnings:");
        for w in &result.warnings {
            println!("   - {w}");
        }
        println!();
    }
}

/// 提示：若希望以更安静的方式集成到 CI，可使用：
/// ```bash
/// DATABASE_URL=... cargo run --bin schema_health_check --quiet
/// ```
/// 当前已通过退出码传达结果。
#[allow(dead_code)]
fn quick_check(database_url: &str) -> Result<bool, sqlx::Error> {
    let pool = futures::executor::block_on(PgPoolOptions::new().max_connections(1).connect(database_url))?;
    futures::executor::block_on(quick_validate(&pool))
}
